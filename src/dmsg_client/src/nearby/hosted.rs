//! Host-level nearby share coordinator (`NearbyHost`) shared by `dmsg_cli`
//! daemon and `dmsg_mobile` UniFFI, plus `BridgeTransport` for Apple AWDL
//! `NWConnection` byte bridges and Happy Eyeballs parallel racing.

use super::offer::{NearbyOffer, RelayOffer, advert, pk_hint, sanitize_alias};
use super::relay::{WsFrameTransport, cancel_room, create_room};
use super::session::{
    Progress, ReceiveDecision, ReceiveOutcome, SendFileItem, SendOutcome, receive_after_handshake,
    receiver_handshake, send_files_after_handshake, send_secret_package_after_handshake,
    sender_handshake,
};
use super::wire::{
    Channel, EphemeralKey, FileSummary, FrameTransport, HandshakeSecrets, OfferKind, RECEIVER,
    SENDER, StreamTransport,
};
use super::{MAX_SESSION_BYTES, OFFER_TTL_MS, RELAY_MAX_BYTES, RELAY_MAX_FILES};
use crate::hold::{Hold, Session};
use crate::secrets::shares::{NearbyTarget, ShareInput};
use crate::{error_code, now_ms};
use dmsg_types::{Error, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteArray;
use serde_json::{Value as Json, json};
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio::task::AbortHandle;

/// Delay before launching blind relay in parallel with local direct probes.
pub const RELAY_STAGGER: Duration = Duration::from_millis(1800);
/// Maximum concurrent unaccepted incoming handshakes held on a receiver.
pub const MAX_PENDING_PEERS: usize = 4;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn random_16() -> [u8; 16] {
    let mut out = [0u8; 16];
    getrandom::fill(&mut out).expect("operating system random source");
    out
}

/// Bind a local TCP listener on an ephemeral port for nearby receiving.
pub async fn listen() -> Result<TcpListener> {
    match TcpListener::bind((Ipv6Addr::UNSPECIFIED, 0)).await {
        Ok(listener) => Ok(listener),
        Err(_) => TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))
            .await
            .map_err(|e| Error::Unavailable(format!("nearby listen: {e}"))),
    }
}

/// Best-effort private local `ip:port` candidate addresses for `port`.
pub fn local_addresses(port: u16, include_loopback: bool) -> Vec<String> {
    let mut addrs = Vec::new();
    if let Ok(socket) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        && socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).is_ok()
        && let Ok(local) = socket.local_addr()
        && super::offer::is_allowed_local_ip(local.ip(), include_loopback)
    {
        addrs.push(SocketAddr::new(local.ip(), port).to_string());
    }
    if include_loopback {
        let lo = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), port).to_string();
        if !addrs.contains(&lo) {
            addrs.push(lo);
        }
    }
    addrs
}

/// Bidirectional in-memory frame transport used by Apple AWDL (`Network.framework`)
/// byte bridges on iOS (UniFFI) and macOS (Swift daemon helper).
pub struct BridgeTransport {
    tx: mpsc::Sender<Vec<u8>>,
    rx: mpsc::Receiver<Vec<u8>>,
}

/// External bridge handle held by Swift/ObjC to feed inbound frames and pull outbound frames.
pub struct BridgeHandle {
    /// Push an inbound frame received from `NWConnection` into Rust.
    pub inbound: mpsc::Sender<Vec<u8>>,
    /// Pull outbound frames produced by Rust to send over `NWConnection`.
    pub outbound: mpsc::Receiver<Vec<u8>>,
}

impl BridgeTransport {
    /// Create a matched `(BridgeTransport, BridgeHandle)` pair with bounded queues.
    pub fn pair(capacity: usize) -> (Self, BridgeHandle) {
        let (out_tx, out_rx) = mpsc::channel(capacity.max(4));
        let (in_tx, in_rx) = mpsc::channel(capacity.max(4));
        (
            Self {
                tx: out_tx,
                rx: in_rx,
            },
            BridgeHandle {
                inbound: in_tx,
                outbound: out_rx,
            },
        )
    }
}

impl FrameTransport for BridgeTransport {
    async fn send_frame(&mut self, bytes: &[u8]) -> Result<()> {
        self.tx
            .send(bytes.to_vec())
            .await
            .map_err(|_| Error::Unavailable("nearby bridge closed".into()))
    }

    async fn recv_frame(&mut self, max: usize) -> Result<Vec<u8>> {
        let bytes = self
            .rx
            .recv()
            .await
            .ok_or_else(|| Error::Unavailable("nearby bridge closed".into()))?;
        ensure(
            !bytes.is_empty() && bytes.len() <= max,
            Error::QuotaExceeded,
        )?;
        Ok(bytes)
    }
}

/// Type-erased frame transport so TCP, WebSocket relay and AWDL bridge share one runner.
pub enum AnyTransport {
    /// Local TCP socket transport.
    Tcp(StreamTransport<TcpStream>),
    /// Ephemeral blind relay WebSocket transport.
    Relay(Box<WsFrameTransport>),
    /// Native byte-bridge transport (Apple AWDL `NWConnection`).
    Bridge(BridgeTransport),
}

impl AnyTransport {
    /// Short transport kind label for UI telemetry.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Tcp(_) => "lan",
            Self::Relay(_) => "relay",
            Self::Bridge(_) => "awdl",
        }
    }
}

impl FrameTransport for AnyTransport {
    async fn send_frame(&mut self, bytes: &[u8]) -> Result<()> {
        match self {
            Self::Tcp(t) => t.send_frame(bytes).await,
            Self::Relay(t) => t.send_frame(bytes).await,
            Self::Bridge(t) => t.send_frame(bytes).await,
        }
    }

    async fn recv_frame(&mut self, max: usize) -> Result<Vec<u8>> {
        match self {
            Self::Tcp(t) => t.recv_frame(max).await,
            Self::Relay(t) => t.recv_frame(max).await,
            Self::Bridge(t) => t.recv_frame(max).await,
        }
    }
}

/// Discovered nearby peer returned by platform mDNS / AWDL browsing.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredPeer {
    /// Ephemeral 16-hex mDNS instance name.
    pub advert: String,
    /// Sanitized display alias from TXT `n=`.
    pub alias: String,
    /// Short public-key hint from TXT `p=`.
    pub pk_hint: String,
    /// Resolved host or IP address.
    pub host: String,
    /// Resolved TCP port.
    pub port: u16,
    /// Transport medium (`lan` or `awdl`).
    pub transport: String,
}

/// Summary of an unaccepted incoming peer waiting on the receiver.
#[derive(Serialize, Clone, Debug)]
pub struct PendingPeerView {
    /// Opaque local peer identifier for `receive_accept` / `receive_reject`.
    pub peer_id: String,
    /// 6-digit visual verification code (`000000`..=`999999`).
    pub sas: String,
    /// Whether the peer proved possession of the offer's QR secret.
    pub qr_bound: bool,
    /// Transport medium (`lan`, `relay`, `awdl`).
    pub transport: String,
    /// Payload kind (`files` or `secret_package`).
    pub kind: OfferKind,
    /// Offered files (empty for `secret_package`).
    pub files: Vec<FileSummary>,
    /// Package size in bytes for `secret_package`.
    pub package_size: Option<u64>,
    /// Untrusted sender alias.
    pub alias: Option<String>,
}

struct PendingPeer {
    view: PendingPeerView,
    decision_tx: Option<oneshot::Sender<ReceiveDecision>>,
}

struct ReceiveState {
    id: u64,
    alias: String,
    discoverable: bool,
    advert: String,
    pk_hint: String,
    port: u16,
    addrs: Vec<String>,
    offer_text: String,
    expires_at: u64,
    relay_rid: Option<[u8; 16]>,
    relay_ready: bool,
    r_key: Arc<EphemeralKey>,
    qrs: [u8; 16],
    /// Pending unaccepted handshakes (slot is NOT locked until user accepts one).
    pending: BTreeMap<String, PendingPeer>,
    /// Locked active peer ID once accepted.
    active_peer: Option<String>,
    transport: Option<String>,
    state: &'static str,
    progress: Progress,
    outcome: Option<ReceiveOutcome>,
    error: Option<Json>,
    started_at: u64,
    ended_at: Option<u64>,
    tasks: Vec<AbortHandle>,
}

struct SendState {
    id: u64,
    kind: OfferKind,
    transport: Option<String>,
    sas: Option<String>,
    transcript: Option<String>,
    qr_bound: bool,
    peer_label: String,
    /// Waiting for user to confirm 6-digit SAS in browse mode.
    sas_tx: Option<oneshot::Sender<Result<()>>>,
    /// Pending `SecretShare` approval ID for vault items.
    share_job_id: Option<String>,
    approval_id: Option<String>,
    approval_tx: Option<oneshot::Sender<Result<String>>>,
    state: &'static str,
    progress: Progress,
    outcome: Option<SendOutcome>,
    error: Option<Json>,
    started_at: u64,
    ended_at: Option<u64>,
    task: Option<AbortHandle>,
}

/// Host controller managing nearby receive and send sessions for one client/daemon.
#[derive(Default)]
pub struct NearbyHost {
    seq: AtomicU64,
    receive: Arc<Mutex<Option<ReceiveState>>>,
    send: Arc<Mutex<Option<SendState>>>,
}

impl NearbyHost {
    /// Create a new empty nearby controller.
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of current receive and send sessions for UI/RPC polling.
    pub fn view(&self) -> Json {
        let receive = lock(&self.receive).as_ref().map(|r| {
            let pending: Vec<&PendingPeerView> = r.pending.values().map(|p| &p.view).collect();
            json!({
                "id": r.id,
                "state": r.state,
                "alias": r.alias,
                "discoverable": r.discoverable,
                "advert": r.advert,
                "pk_hint": r.pk_hint,
                "port": r.port,
                "addrs": r.addrs,
                "offer": r.offer_text,
                "expires_at": r.expires_at,
                "relay_ready": r.relay_ready,
                "pending_peers": pending,
                "active_peer": r.active_peer,
                "transport": r.transport,
                "progress": r.progress,
                "outcome": r.outcome,
                "error": r.error,
                "started_at": r.started_at,
                "ended_at": r.ended_at,
            })
        });
        let send = lock(&self.send).as_ref().map(|s| {
            json!({
                "id": s.id,
                "state": s.state,
                "kind": s.kind,
                "transport": s.transport,
                "sas": s.sas,
                "transcript": s.transcript,
                "qr_bound": s.qr_bound,
                "peer_label": s.peer_label,
                "awaiting_sas_confirm": s.sas_tx.is_some(),
                "share_job_id": s.share_job_id,
                "approval_id": s.approval_id,
                "progress": s.progress,
                "outcome": s.outcome,
                "error": s.error,
                "started_at": s.started_at,
                "ended_at": s.ended_at,
            })
        });
        json!({
            "receive": receive,
            "send": send,
        })
    }

    /// Start a new foreground receive session on a random TCP port, optionally
    /// asynchronously attaching an ephemeral blind relay room when online.
    ///
    /// Default `discoverable = false` (QR-only mode, no mDNS broadcast).
    pub async fn receive_start<H: Hold + Clone + Send + 'static>(
        &self,
        h: H,
        alias: Option<String>,
        discoverable: bool,
        max_package_bytes: u64,
        on_change: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Json> {
        self.receive_cancel(Some(&h)).await;

        let (env, include_loopback, online) = h
            .with(|c| {
                c.keys()?;
                let env = c.workspace.meta.environment.clone();
                Ok((env.clone(), env == "local", !c.offline_mode()))
            })
            .await?;

        let listener = listen().await?;
        let port = listener
            .local_addr()
            .map_err(|e| Error::Unavailable(format!("nearby addr: {e}")))?
            .port();
        let addrs = local_addresses(port, include_loopback);
        let r_key = Arc::new(EphemeralKey::generate());
        let qrs = random_16();
        let advert_sid = random_16();
        let adv_label = advert(&advert_sid);
        let hint = pk_hint(&r_key.public_bytes());
        let alias = sanitize_alias(alias.as_deref().unwrap_or("dMsg Device"));
        let started_at = now_ms();
        let expires_at = started_at + OFFER_TTL_MS;

        let offer = NearbyOffer {
            v: super::FORMAT,
            pk: ByteArray::from(r_key.public_bytes()),
            qrs: ByteArray::from(qrs),
            exp: expires_at,
            adv: discoverable.then(|| adv_label.clone()),
            addrs: addrs.clone(),
            relay: None,
        };
        let offer_text = offer.encode_for(Some(&env))?;
        let id = self.seq.fetch_add(1, Ordering::Relaxed) + 1;

        {
            let mut slot = lock(&self.receive);
            *slot = Some(ReceiveState {
                id,
                alias,
                discoverable,
                advert: adv_label.clone(),
                pk_hint: hint,
                port,
                addrs: addrs.clone(),
                offer_text,
                expires_at,
                relay_rid: None,
                relay_ready: false,
                r_key: r_key.clone(),
                qrs,
                pending: BTreeMap::new(),
                active_peer: None,
                transport: None,
                state: "waiting",
                progress: Progress::default(),
                outcome: None,
                error: None,
                started_at,
                ended_at: None,
                tasks: Vec::new(),
            });
        }

        // Spawn TCP accept loop.
        let recv_arc = self.receive.clone();
        let h_tcp = h.clone();
        let cb_tcp = on_change.clone();
        let tcp_task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                if !is_receive_accepting(&recv_arc, id) {
                    drop(stream);
                    continue;
                }
                spawn_incoming_peer(
                    recv_arc.clone(),
                    id,
                    h_tcp.clone(),
                    AnyTransport::Tcp(StreamTransport::new(stream)),
                    max_package_bytes,
                    cb_tcp.clone(),
                );
            }
        });
        push_receive_task(&self.receive, id, tcp_task.abort_handle());

        // Spawn async blind relay room creation when logged in & online (never blocks local listen).
        if online {
            let recv_arc = self.receive.clone();
            let h_relay = h.clone();
            let cb_relay = on_change.clone();
            let r_key_relay = r_key.clone();
            let relay_task = tokio::spawn(async move {
                let Ok(room) = create_room(&h_relay).await else {
                    return;
                };
                let rid = room.rid;
                let send_tok = *room.send_token;
                let recv_tok = *room.recv_token;
                let origin = room.origin.clone();
                let env_name = room.env.clone();

                let Ok(ws) = WsFrameTransport::connect(&origin, &rid, &recv_tok).await else {
                    let _ = cancel_room(&h_relay, rid).await;
                    return;
                };

                let attached = {
                    let mut guard = lock(&recv_arc);
                    if let Some(st) = guard
                        .as_mut()
                        .filter(|s| s.id == id && s.state == "waiting")
                    {
                        st.relay_rid = Some(rid);
                        st.relay_ready = true;
                        let updated = NearbyOffer {
                            v: super::FORMAT,
                            pk: ByteArray::from(r_key_relay.public_bytes()),
                            qrs: ByteArray::from(st.qrs),
                            exp: st.expires_at,
                            adv: st.discoverable.then(|| st.advert.clone()),
                            addrs: st.addrs.clone(),
                            relay: Some(RelayOffer {
                                env: env_name.clone(),
                                rid: ByteArray::from(rid),
                                tok: ByteArray::from(send_tok),
                            }),
                        };
                        if let Ok(encoded) = updated.encode_for(Some(&env_name)) {
                            st.offer_text = encoded;
                        }
                        true
                    } else {
                        false
                    }
                };
                if !attached {
                    let _ = cancel_room(&h_relay, rid).await;
                    return;
                }
                cb_relay();
                spawn_incoming_peer(
                    recv_arc,
                    id,
                    h_relay,
                    AnyTransport::Relay(Box::new(ws)),
                    max_package_bytes,
                    cb_relay,
                );
            });
            push_receive_task(&self.receive, id, relay_task.abort_handle());
        }

        let _ = env;
        on_change();
        Ok(self.view())
    }

    /// Inject an incoming native AWDL `BridgeTransport` into the active receive session.
    pub fn receive_attach_bridge<H: Hold + Clone + Send + 'static>(
        &self,
        h: H,
        transport: BridgeTransport,
        max_package_bytes: u64,
        on_change: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<()> {
        let id = {
            let guard = lock(&self.receive);
            let Some(st) = guard.as_ref().filter(|s| s.state == "waiting") else {
                return Err(Error::Unavailable("nearby receive not waiting".into()));
            };
            st.id
        };
        spawn_incoming_peer(
            self.receive.clone(),
            id,
            h,
            AnyTransport::Bridge(transport),
            max_package_bytes,
            on_change,
        );
        Ok(())
    }

    /// Accept one pending incoming peer after visually checking its 6-digit SAS.
    ///
    /// Locking the active transfer slot happens here (never on initial TCP connect),
    /// rejecting all competing pending connections immediately.
    pub fn receive_accept(
        &self,
        peer_id: &str,
        folder: String,
        selected: Vec<u32>,
        share_identity: bool,
    ) -> Result<Json> {
        let mut guard = lock(&self.receive);
        let st = guard
            .as_mut()
            .filter(|s| s.state == "waiting" && s.active_peer.is_none())
            .ok_or(Error::NotFound)?;
        let mut chosen = st.pending.remove(peer_id).ok_or(Error::NotFound)?;
        let tx = chosen.decision_tx.take().ok_or(Error::Forbidden)?;

        // Reject every other competing pending peer.
        for (_, mut other) in std::mem::take(&mut st.pending) {
            if let Some(other_tx) = other.decision_tx.take() {
                let _ = other_tx.send(ReceiveDecision::Reject {
                    code: "BUSY".into(),
                });
            }
        }

        st.active_peer = Some(peer_id.to_string());
        st.transport = Some(chosen.view.transport.clone());
        st.state = "running";
        let _ = tx.send(ReceiveDecision::Accept {
            folder,
            selected,
            share_identity,
        });
        drop(guard);
        Ok(self.view())
    }

    /// Reject one pending incoming peer while keeping the receiver open for others.
    pub fn receive_reject(&self, peer_id: &str) -> Result<Json> {
        let mut guard = lock(&self.receive);
        let st = guard.as_mut().ok_or(Error::NotFound)?;
        let mut peer = st.pending.remove(peer_id).ok_or(Error::NotFound)?;
        if let Some(tx) = peer.decision_tx.take() {
            let _ = tx.send(ReceiveDecision::Reject {
                code: "REJECTED".into(),
            });
        }
        drop(guard);
        Ok(self.view())
    }

    /// Stop the current receive session, aborting listeners/connections and
    /// best-effort cancelling any open relay room.
    pub async fn receive_cancel<H: Hold>(&self, h: Option<&H>) -> Json {
        let rid = {
            let mut guard = lock(&self.receive);
            if let Some(st) = guard.as_mut() {
                for task in st.tasks.drain(..) {
                    task.abort();
                }
                for (_, mut peer) in std::mem::take(&mut st.pending) {
                    if let Some(tx) = peer.decision_tx.take() {
                        let _ = tx.send(ReceiveDecision::Reject {
                            code: "CANCELLED".into(),
                        });
                    }
                }
                if matches!(st.state, "waiting" | "running") {
                    st.state = "cancelled";
                    st.ended_at = Some(now_ms());
                }
                st.relay_rid.take()
            } else {
                None
            }
        };
        if let (Some(h), Some(rid)) = (h, rid) {
            let _ = cancel_room(h, rid).await;
        }
        self.view()
    }

    /// Start sending Files or a Vault snapshot using either a `dmsg-nearby/1:`
    /// offer (parallel-racing local addrs + optional mDNS/AWDL + staggered blind relay)
    /// or a direct discovered peer (`host:port`).
    #[allow(clippy::too_many_arguments)]
    pub async fn send_start<H: Hold + Clone + Send + 'static>(
        &self,
        h: H,
        target: SendTarget,
        payload: SendPayload,
        alias: Option<String>,
        share_identity: bool,
        approval_source: String,
        on_change: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Json> {
        self.send_cancel();

        let (env, relay_origin, online) = h
            .with(|c| {
                c.keys()?;
                Ok((
                    c.workspace.meta.environment.clone(),
                    c.config.relay.clone(),
                    !c.offline_mode(),
                ))
            })
            .await?;

        let (kind, total_bytes, file_count) = match &payload {
            SendPayload::Files(items) => {
                ensure(
                    !items.is_empty() && items.len() <= super::MAX_SESSION_FILES,
                    Error::QuotaExceeded,
                )?;
                let total = h
                    .with(|c| {
                        let mut sum = 0u64;
                        for item in items {
                            let (_, _, m) =
                                c.library_manifest(&item.file_id, item.version_id.as_deref())?;
                            sum = sum.checked_add(m.size).ok_or(Error::QuotaExceeded)?;
                        }
                        Ok(sum)
                    })
                    .await?;
                ensure(total <= MAX_SESSION_BYTES, Error::QuotaExceeded)?;
                (OfferKind::Files, total, items.len())
            }
            SendPayload::VaultItem(input) => {
                // Validate selection and compute exact package size upfront without persisting yet.
                let plain_bytes = h
                    .with(|c| {
                        let prepared = c.secret_nearby_preparation(
                            input.clone(),
                            NearbyTarget {
                                transcript: "00".repeat(32),
                                sas: "000000".into(),
                                peer_label: "preview".into(),
                            },
                        )?;
                        let _ = &prepared;
                        // Extract plain bytes via temporary check without saving job.
                        let mut check = input.clone();
                        check.expires_in_ms = None;
                        check.max_download_sessions = None;
                        let entry = c.vault_show(&check.item_id)?;
                        ensure(
                            !entry.deleted
                                && !entry.item.reserved()
                                && entry.revision == check.revision,
                            Error::VersionConflict,
                        )?;
                        let mut size = 0u64;
                        for a in &check.attachments {
                            let (_, _, m) = c.library_manifest(&a.file_id, Some(&a.version_id))?;
                            size = size.checked_add(m.size).ok_or(Error::QuotaExceeded)?;
                        }
                        Ok(size)
                    })
                    .await?;
                let _ = plain_bytes;
                (OfferKind::SecretPackage, 0, 1)
            }
        };

        let (peer_label, qr_bound) = match &target {
            SendTarget::Offer { offer, .. } => {
                offer.validate(Some(&env))?;
                ("QR device".to_string(), true)
            }
            SendTarget::Direct {
                alias: peer_alias, ..
            } => (
                peer_alias
                    .clone()
                    .unwrap_or_else(|| "Nearby device".to_string()),
                false,
            ),
        };

        let id = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        let started_at = now_ms();
        {
            let mut slot = lock(&self.send);
            *slot = Some(SendState {
                id,
                kind,
                transport: None,
                sas: None,
                transcript: None,
                qr_bound,
                peer_label: peer_label.clone(),
                sas_tx: None,
                share_job_id: None,
                approval_id: None,
                approval_tx: None,
                state: "connecting",
                progress: Progress::default(),
                outcome: None,
                error: None,
                started_at,
                ended_at: None,
                task: None,
            });
        }

        let send_arc = self.send.clone();
        let cb = on_change.clone();
        let task = tokio::spawn(async move {
            let result = run_sender(
                send_arc.clone(),
                id,
                h,
                target,
                payload,
                alias,
                share_identity,
                approval_source,
                relay_origin,
                online,
                total_bytes,
                file_count,
                cb.clone(),
            )
            .await;

            let mut guard = lock(&send_arc);
            if let Some(st) = guard.as_mut().filter(|s| s.id == id) {
                match result {
                    Ok(outcome) => {
                        st.state = "done";
                        st.outcome = Some(outcome);
                    }
                    Err(err) => {
                        if st.state != "cancelled" {
                            st.state = "failed";
                            st.error = Some(json!({
                                "code": error_code(&err),
                                "message": err.to_string(),
                            }));
                        }
                    }
                }
                st.sas_tx = None;
                st.approval_tx = None;
                st.ended_at = Some(now_ms());
                st.task = None;
            }
            drop(guard);
            cb();
        });

        if let Some(st) = lock(&self.send).as_mut().filter(|s| s.id == id) {
            st.task = Some(task.abort_handle());
        }
        on_change();
        Ok(self.view())
    }

    /// Confirm the 6-digit SAS on the sender side in browse mode, unlocking `OfferMeta`.
    pub fn send_confirm_sas(&self) -> Result<Json> {
        let mut guard = lock(&self.send);
        let st = guard.as_mut().ok_or(Error::NotFound)?;
        let tx = st.sas_tx.take().ok_or(Error::Forbidden)?;
        st.state = "awaiting_accept";
        let _ = tx.send(Ok(()));
        drop(guard);
        Ok(self.view())
    }

    /// Notify the sender task that the pending `SecretShare` approval finished.
    pub fn send_approval_ready(&self, share_job_id: String) -> Result<Json> {
        let mut guard = lock(&self.send);
        let st = guard.as_mut().ok_or(Error::NotFound)?;
        let tx = st.approval_tx.take().ok_or(Error::Forbidden)?;
        let _ = tx.send(Ok(share_job_id));
        drop(guard);
        Ok(self.view())
    }

    /// Cancel the active send session.
    pub fn send_cancel(&self) -> Json {
        let mut guard = lock(&self.send);
        if let Some(st) = guard.as_mut() {
            if let Some(task) = st.task.take() {
                task.abort();
            }
            if let Some(tx) = st.sas_tx.take() {
                let _ = tx.send(Err(Error::Unavailable("nearby: cancelled".into())));
            }
            if let Some(tx) = st.approval_tx.take() {
                let _ = tx.send(Err(Error::Unavailable("nearby: cancelled".into())));
            }
            if matches!(
                st.state,
                "connecting" | "awaiting_sas" | "awaiting_accept" | "awaiting_approval" | "running"
            ) {
                st.state = "cancelled";
                st.ended_at = Some(now_ms());
            }
        }
        drop(guard);
        self.view()
    }

    /// Abort both receive and send sessions immediately (called on lock, account
    /// switch or mobile backgrounding).
    pub async fn reset<H: Hold>(&self, h: Option<&H>) {
        let _ = self.receive_cancel(h).await;
        let _ = self.send_cancel();
    }
}

/// Destination for [`NearbyHost::send_start`].
pub enum SendTarget {
    /// Parsed `dmsg-nearby/1:` QR/pasted offer plus optional pre-resolved mDNS or AWDL transport.
    Offer {
        /// Validated offer.
        offer: NearbyOffer,
        /// Optional mDNS-resolved `host:port` candidate.
        resolved: Option<(String, u16)>,
        /// Optional pre-connected AWDL bridge candidate.
        awdl: Option<BridgeTransport>,
    },
    /// Directly discovered nearby device (browse mode, requires SAS confirmation).
    Direct {
        /// Host or IP address.
        host: String,
        /// TCP port.
        port: u16,
        /// Display alias.
        alias: Option<String>,
        /// Optional pre-connected AWDL bridge instead of TCP.
        awdl: Option<BridgeTransport>,
    },
}

/// Payload for [`NearbyHost::send_start`].
pub enum SendPayload {
    /// One or more library files.
    Files(Vec<SendFileItem>),
    /// One Vault entry snapshot selection (`dmsg-secret-package/1`).
    VaultItem(ShareInput),
}

fn is_receive_accepting(recv: &Arc<Mutex<Option<ReceiveState>>>, id: u64) -> bool {
    lock(recv).as_ref().is_some_and(|st| {
        st.id == id && st.state == "waiting" && st.pending.len() < MAX_PENDING_PEERS
    })
}

fn push_receive_task(recv: &Arc<Mutex<Option<ReceiveState>>>, id: u64, handle: AbortHandle) {
    if let Some(st) = lock(recv).as_mut().filter(|s| s.id == id) {
        st.tasks.push(handle);
    } else {
        handle.abort();
    }
}

fn spawn_incoming_peer<H: Hold + Clone + Send + 'static>(
    recv: Arc<Mutex<Option<ReceiveState>>>,
    id: u64,
    h: H,
    mut transport: AnyTransport,
    max_package_bytes: u64,
    on_change: Arc<dyn Fn() + Send + Sync>,
) {
    let recv_for_task = recv.clone();
    let task = tokio::spawn(async move {
        let Some((r_key, qrs, require_qr)) = lock(&recv_for_task)
            .as_ref()
            .filter(|s| s.id == id && s.state == "waiting")
            .map(|s| (s.r_key.clone(), s.qrs, !s.discoverable))
        else {
            return;
        };

        let transport_name = transport.kind_name().to_string();
        let Ok(secrets) = receiver_handshake(&mut transport, &r_key, &qrs, require_qr).await else {
            return;
        };
        let peer_id = hex::encode(&secrets.sid[..8]);
        let sas = secrets.sas.clone();
        let qr_bound = secrets.qr_bound;
        let channel = Channel::new(transport, &secrets, RECEIVER);

        let recv_for_decide = recv_for_task.clone();
        let cb_decide = on_change.clone();
        let peer_for_decide = peer_id.clone();

        let recv_for_prog = recv_for_task.clone();
        let cb_prog = on_change.clone();
        let peer_for_prog = peer_id.clone();

        let outcome = receive_after_handshake(
            &h,
            channel,
            secrets,
            max_package_bytes,
            move |kind, files, package_size, alias| async move {
                let (tx, rx) = oneshot::channel();
                let inserted = {
                    let mut guard = lock(&recv_for_decide);
                    if let Some(st) = guard.as_mut().filter(|s| {
                        s.id == id && s.state == "waiting" && s.pending.len() < MAX_PENDING_PEERS
                    }) {
                        st.pending.insert(
                            peer_for_decide.clone(),
                            PendingPeer {
                                view: PendingPeerView {
                                    peer_id: peer_for_decide.clone(),
                                    sas,
                                    qr_bound,
                                    transport: transport_name,
                                    kind,
                                    files,
                                    package_size,
                                    alias,
                                },
                                decision_tx: Some(tx),
                            },
                        );
                        true
                    } else {
                        false
                    }
                };
                if !inserted {
                    return Ok(ReceiveDecision::Reject {
                        code: "BUSY".into(),
                    });
                }
                cb_decide();
                rx.await
                    .map_err(|_| Error::Unavailable("nearby: cancelled".into()))
            },
            move |progress| {
                if let Some(st) = lock(&recv_for_prog)
                    .as_mut()
                    .filter(|s| s.id == id && s.active_peer.as_deref() == Some(&peer_for_prog))
                {
                    st.progress = progress;
                }
                cb_prog();
            },
        )
        .await;

        let relay_to_cancel = {
            let mut guard = lock(&recv_for_task);
            if let Some(st) = guard.as_mut().filter(|s| s.id == id) {
                st.pending.remove(&peer_id);
                if st.active_peer.as_deref() == Some(&peer_id) {
                    match outcome {
                        Ok(done) => {
                            st.state = "done";
                            st.outcome = Some(done);
                        }
                        Err(err) => {
                            if st.state != "cancelled" {
                                st.state = "failed";
                                st.error = Some(json!({
                                    "code": error_code(&err),
                                    "message": err.to_string(),
                                }));
                            }
                        }
                    }
                    st.ended_at = Some(now_ms());
                    for task in st.tasks.drain(..) {
                        task.abort();
                    }
                    st.relay_rid.take()
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some(rid) = relay_to_cancel {
            let _ = cancel_room(&h, rid).await;
        }
        on_change();
    });
    push_receive_task(&recv, id, task.abort_handle());
}

/// Happy Eyeballs parallel racer for connecting and handshaking with a nearby receiver.
async fn race_sender_connect(
    target: SendTarget,
    relay_origin: &str,
    online: bool,
    total_bytes: u64,
    file_count: usize,
) -> Result<(AnyTransport, HandshakeSecrets)> {
    match target {
        SendTarget::Direct {
            host, port, awdl, ..
        } => {
            let mut transport = if let Some(bridge) = awdl {
                AnyTransport::Bridge(bridge)
            } else {
                let stream = tokio::time::timeout(
                    Duration::from_secs(10),
                    TcpStream::connect((host.as_str(), port)),
                )
                .await
                .map_err(|_| Error::Unavailable("nearby: peer did not answer".into()))?
                .map_err(|e| Error::Unavailable(format!("nearby connect: {e}")))?;
                AnyTransport::Tcp(StreamTransport::new(stream))
            };
            let secrets = sender_handshake(&mut transport, None, None).await?;
            Ok((transport, secrets))
        }
        SendTarget::Offer {
            offer,
            resolved,
            awdl,
        } => {
            let expected_pk = *offer.pk;
            let qrs = *offer.qrs;
            let mut set: tokio::task::JoinSet<Result<(AnyTransport, HandshakeSecrets)>> =
                tokio::task::JoinSet::new();

            if let Some(bridge) = awdl {
                set.spawn(async move {
                    let mut t = AnyTransport::Bridge(bridge);
                    let secrets = sender_handshake(&mut t, Some(&expected_pk), Some(&qrs)).await?;
                    Ok((t, secrets))
                });
            }

            if let Some((host, port)) = resolved {
                set.spawn(async move {
                    let stream = tokio::time::timeout(
                        Duration::from_secs(8),
                        TcpStream::connect((host.as_str(), port)),
                    )
                    .await
                    .map_err(|_| Error::Unavailable("nearby: mDNS connect timed out".into()))?
                    .map_err(|e| Error::Unavailable(format!("nearby connect: {e}")))?;
                    let mut t = AnyTransport::Tcp(StreamTransport::new(stream));
                    let secrets = sender_handshake(&mut t, Some(&expected_pk), Some(&qrs)).await?;
                    Ok((t, secrets))
                });
            }

            for addr in offer.socket_addrs() {
                set.spawn(async move {
                    let stream =
                        tokio::time::timeout(Duration::from_secs(8), TcpStream::connect(addr))
                            .await
                            .map_err(|_| {
                                Error::Unavailable("nearby: LAN connect timed out".into())
                            })?
                            .map_err(|e| Error::Unavailable(format!("nearby connect: {e}")))?;
                    let mut t = AnyTransport::Tcp(StreamTransport::new(stream));
                    let secrets = sender_handshake(&mut t, Some(&expected_pk), Some(&qrs)).await?;
                    Ok((t, secrets))
                });
            }

            let relay_eligible = offer.relay.is_some()
                && online
                && total_bytes <= RELAY_MAX_BYTES
                && file_count <= RELAY_MAX_FILES;
            let relay_over_limit = offer.relay.is_some()
                && online
                && (total_bytes > RELAY_MAX_BYTES || file_count > RELAY_MAX_FILES);

            if let Some(relay_info) = offer.relay.filter(|_| relay_eligible) {
                let origin = relay_origin.to_string();
                let stagger = if set.is_empty() {
                    Duration::ZERO
                } else {
                    RELAY_STAGGER
                };
                set.spawn(async move {
                    if !stagger.is_zero() {
                        tokio::time::sleep(stagger).await;
                    }
                    let mut t = AnyTransport::Relay(Box::new(
                        WsFrameTransport::connect(&origin, &relay_info.rid, &relay_info.tok)
                            .await?,
                    ));
                    let secrets = sender_handshake(&mut t, Some(&expected_pk), Some(&qrs)).await?;
                    Ok((t, secrets))
                });
            }

            let mut last_err = None;
            while let Some(joined) = set.join_next().await {
                match joined {
                    Ok(Ok(winner)) => {
                        set.abort_all();
                        return Ok(winner);
                    }
                    Ok(Err(err)) => last_err = Some(err),
                    Err(_) => {}
                }
            }

            if relay_over_limit {
                return Err(Error::QuotaExceeded);
            }
            Err(last_err
                .unwrap_or_else(|| Error::Unavailable("nearby: could not reach receiver".into())))
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_sender<H: Hold + Clone + Send + 'static>(
    send_arc: Arc<Mutex<Option<SendState>>>,
    id: u64,
    h: H,
    target: SendTarget,
    payload: SendPayload,
    alias: Option<String>,
    share_identity: bool,
    approval_source: String,
    relay_origin: String,
    online: bool,
    total_bytes: u64,
    file_count: usize,
    on_change: Arc<dyn Fn() + Send + Sync>,
) -> Result<SendOutcome> {
    let bound = Session::new(&h).await?;
    let (transport, secrets) =
        race_sender_connect(target, &relay_origin, online, total_bytes, file_count).await?;
    let transport_name = transport.kind_name().to_string();
    let max_bytes = if transport_name == "relay" {
        RELAY_MAX_BYTES
    } else {
        MAX_SESSION_BYTES
    };
    let channel = Channel::new(transport, &secrets, SENDER);

    {
        let mut guard = lock(&send_arc);
        if let Some(st) = guard.as_mut().filter(|s| s.id == id) {
            st.transport = Some(transport_name);
            st.sas = Some(secrets.sas.clone());
            st.transcript = Some(hex::encode(*secrets.transcript));
            st.qr_bound = secrets.qr_bound;
            st.state = if secrets.qr_bound {
                "awaiting_accept"
            } else {
                "awaiting_sas"
            };
        }
    }
    on_change();

    let send_for_sas = send_arc.clone();
    let cb_sas = on_change.clone();
    let confirm_sas = move |_s: &HandshakeSecrets| async move {
        let (tx, rx) = oneshot::channel();
        {
            let mut guard = lock(&send_for_sas);
            if let Some(st) = guard.as_mut().filter(|s| s.id == id) {
                st.sas_tx = Some(tx);
                st.state = "awaiting_sas";
            }
        }
        cb_sas();
        rx.await
            .map_err(|_| Error::Unavailable("nearby: cancelled".into()))??;
        Ok(())
    };

    let send_for_prog = send_arc.clone();
    let cb_prog = on_change.clone();
    let on_progress = move |progress: Progress| {
        if let Some(st) = lock(&send_for_prog).as_mut().filter(|s| s.id == id) {
            st.state = "running";
            st.progress = progress;
        }
        cb_prog();
    };

    match payload {
        SendPayload::Files(items) => {
            send_files_after_handshake(
                &bound,
                channel,
                secrets,
                &items,
                alias,
                share_identity,
                max_bytes,
                confirm_sas,
                on_progress,
            )
            .await
        }
        SendPayload::VaultItem(input) => {
            // Prepare exact package metadata bound to this verified transcript!
            let peer_label = lock(&send_arc)
                .as_ref()
                .filter(|s| s.id == id)
                .map(|s| s.peer_label.clone())
                .unwrap_or_else(|| "Nearby device".into());
            let target_info = NearbyTarget {
                transcript: hex::encode(*secrets.transcript),
                sas: secrets.sas.clone(),
                peer_label,
            };
            let (job_id, plain_bytes, approval_view) = bound
                .with(|c| {
                    let prepared = c.secret_nearby_preparation(input, target_info)?;
                    let job_view = c.secret_share_save_preparation(prepared)?;
                    let job_id = job_view["id"]
                        .as_str()
                        .ok_or(Error::IntegrityFailed)?
                        .to_string();
                    let plain_bytes = job_view["plan"]["plain_bytes"]
                        .as_u64()
                        .ok_or(Error::IntegrityFailed)?;
                    let approval = c.secret_share_request(&approval_source, &job_id)?;
                    Ok((job_id, plain_bytes, approval))
                })
                .await?;
            let approval_id = approval_view["id"]
                .as_str()
                .ok_or(Error::IntegrityFailed)?
                .to_string();

            let send_for_approval = send_arc.clone();
            let cb_approval = on_change.clone();
            let h_for_approval = h.clone();
            let job_id_for_wait = job_id.clone();

            send_secret_package_after_handshake(
                &bound,
                channel,
                secrets,
                plain_bytes,
                alias,
                share_identity,
                confirm_sas,
                move |_s: &HandshakeSecrets| async move {
                    let bound_for_approval = Session::new(&h_for_approval).await?;
                    let (tx, mut rx) = oneshot::channel();
                    {
                        let mut guard = lock(&send_for_approval);
                        if let Some(st) = guard.as_mut().filter(|s| s.id == id) {
                            st.share_job_id = Some(job_id_for_wait.clone());
                            st.approval_id = Some(approval_id.clone());
                            st.approval_tx = Some(tx);
                            st.state = "awaiting_approval";
                        }
                    }
                    cb_approval();

                    // Poll approval status in case host approved externally without calling `send_approval_ready`.
                    let mut tick = tokio::time::interval(Duration::from_millis(400));
                    let deadline = tokio::time::Instant::now() + super::CONFIRM_TIMEOUT;
                    loop {
                        tokio::select! {
                            res = &mut rx => {
                                return res.map_err(|_| Error::Unavailable("nearby: cancelled".into()))?;
                            }
                            _ = tick.tick() => {
                                if tokio::time::Instant::now() >= deadline {
                                    return Err(Error::Expired);
                                }
                                let view = bound_for_approval
                                    .with(|c| c.secret_share_get(&job_id_for_wait, false))
                                    .await?;
                                match view["state"].as_str() {
                                    Some("ready_for_nearby") => return Ok(job_id_for_wait),
                                    Some("cancelled" | "failed") => {
                                        return Err(Error::Unavailable("nearby: share approval rejected".into()));
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                },
                on_progress,
            )
            .await
        }
    }
}
