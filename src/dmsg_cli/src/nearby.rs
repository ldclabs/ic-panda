//! Nearby cross-device sharing of Files and Vault snapshots (`dmsg-nearby-share/1`).
//!
//! Coordinates `dmsg_client::nearby::NearbyHost` with cross-platform `_dmsg-share._tcp`
//! mDNS (`crate::mdns`), macOS Apple AWDL P2P (`crate::nearby_awdl`), QR image
//! generation/scanning and daemon lifecycle (locking, switching accounts or stopping
//! immediately tears down listeners, advertisements and active transfers).

use crate::daemon::Daemon;
use crate::mdns::{self, NearbyAdvert};
use crate::nearby_awdl::{self, AwdlListener};
use crate::params::{optional, parse, source, text};
use base64::Engine;
use dmsg_client::hold::Hold;
use dmsg_client::nearby::{
    DiscoveredPeer, NearbyHost, NearbyOffer, SendFileItem, SendPayload, SendTarget,
};
use dmsg_client::secrets::shares::ShareInput;
use dmsg_client::{Client, qr};
use dmsg_types::{Error, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;

/// JSON-RPC methods served by the nearby sharing module.
pub(crate) const METHODS: [&str; 10] = [
    "nearby/status",
    "nearby/browse",
    "nearby/receive_start",
    "nearby/receive_accept",
    "nearby/receive_reject",
    "nearby/receive_cancel",
    "nearby/send_start",
    "nearby/send_confirm_sas",
    "nearby/send_cancel",
    "nearby/scan",
];

/// Events announced after a successful RPC call.
pub(crate) fn events_of(method: &str) -> &'static [&'static str] {
    match method {
        "nearby/status" | "nearby/browse" | "nearby/scan" => &[],
        _ => &["nearby/changed"],
    }
}

#[derive(Default)]
struct ReceiveHandles {
    advert: Option<NearbyAdvert>,
    awdl: Option<AwdlListener>,
    mdns_error: Option<String>,
}

/// Daemon-owned coordinator for nearby receive and send sessions.
#[derive(Default)]
pub(crate) struct Nearby {
    host: OnceLock<Weak<Daemon>>,
    core: Arc<NearbyHost>,
    handles: Mutex<ReceiveHandles>,
}

/// Owned [`Hold`] bound to one daemon session generation.
///
/// Never holds the client mutex across an `.await`; locking or switching
/// accounts invalidates all subsequent steps with `Error::Locked`.
#[derive(Clone)]
struct DaemonHold {
    daemon: Arc<Daemon>,
    generation: u64,
}

impl Hold for DaemonHold {
    async fn with<T: Send>(&self, step: impl FnOnce(&mut Client) -> Result<T> + Send) -> Result<T> {
        let mut client = self.daemon.client().lock().await;
        ensure(
            client.session_generation() == self.generation,
            Error::Locked,
        )?;
        let result = step(&mut client);
        let changed = client.session_generation() != self.generation;
        let state = client.session_state().state;
        drop(client);
        if changed {
            self.daemon.emit("session/changed", json!({"state": state}));
        }
        result
    }
}

#[derive(Deserialize)]
struct FileParam {
    file_id: String,
    #[serde(default)]
    version_id: Option<String>,
}

fn default_alias() -> String {
    let raw = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "dMsg Desktop".to_string());
    dmsg_client::nearby::sanitize_alias(&raw)
}

impl Nearby {
    /// Bind the daemon handle once during startup.
    pub(crate) fn bind(&self, host: Weak<Daemon>) {
        let _ = self.host.set(host);
    }

    fn daemon(&self) -> Result<Arc<Daemon>> {
        self.host
            .get()
            .and_then(Weak::upgrade)
            .ok_or_else(|| Error::Unavailable("the daemon is stopping".into()))
    }

    /// Snapshot of receive/send state enriched with a QR data URL and optional mDNS notice.
    pub(crate) fn view(&self) -> Value {
        let mut view = self.core.view();
        with_qr(&mut view);
        if let Some(recv) = view.get_mut("receive").and_then(Value::as_object_mut) {
            let mdns_error = self
                .handles
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .mdns_error
                .clone();
            if let Some(err) = mdns_error {
                recv.insert("mdns_error".into(), Value::String(err));
            }
        }
        view
    }

    /// Wake a waiting Vault nearby sender right after its `SecretShare` approval settles.
    pub(crate) fn notify_approval_ready(&self, job_id: String) {
        let _ = self.core.send_approval_ready(job_id);
    }

    /// Tear down all active listeners, mDNS/AWDL advertisements, relay rooms and send tasks.
    pub(crate) async fn cancel_all(&self) {
        {
            let mut h = self.handles.lock().unwrap_or_else(|e| e.into_inner());
            h.advert = None;
            h.awdl = None;
            h.mdns_error = None;
        }
        self.core.send_cancel();
        let daemon = self.daemon().ok();
        if let Some(d) = &daemon {
            let h = d.hold();
            let _ = self.core.receive_cancel(Some(&h)).await;
            d.emit("nearby/changed", self.view());
        } else {
            let _ = self.core.receive_cancel::<DaemonHold>(None).await;
        }
    }

    fn change_hook(&self, daemon: Arc<Daemon>) -> Arc<dyn Fn() + Send + Sync> {
        let core = self.core.clone();
        let approving = AtomicBool::new(false);
        Arc::new(move || {
            let mut view = core.view();
            let recv_state = view["receive"]["state"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let send_state = view["send"]["state"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            if matches!(recv_state.as_str(), "running" | "waiting")
                || matches!(send_state.as_str(), "running")
            {
                daemon.note_activity();
            }
            with_qr(&mut view);
            if matches!(recv_state.as_str(), "done" | "failed" | "cancelled") {
                let mut h = daemon
                    .nearby
                    .handles
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                h.advert = None;
                h.awdl = None;
            }
            if recv_state == "done" {
                daemon.emit("files/changed", json!({"nearby": true}));
            }
            // Once more when the wait ends: the send settles or cancels the approval.
            let awaiting = send_state == "awaiting_approval";
            let was_awaiting = approving.swap(awaiting, Ordering::Relaxed);
            if awaiting || was_awaiting {
                daemon.emit("approval/changed", Value::Null);
            }
            daemon.emit("nearby/changed", view);
        })
    }
}

/// Attach the offer's QR image while the receiver waits for a sender; a
/// transfer reports progress per chunk and shows no code.
fn with_qr(view: &mut Value) {
    if let Some(recv) = view.get_mut("receive").and_then(Value::as_object_mut)
        && recv.get("state").and_then(Value::as_str) == Some("waiting")
        && let Some(offer) = recv.get("offer").and_then(Value::as_str)
        && !offer.is_empty()
        && let Ok(qr_png) = qr::png_data(offer)
    {
        recv.insert("qr".into(), Value::String(qr_png));
    }
}

impl Daemon {
    /// Dispatch one of [`METHODS`].
    pub(crate) async fn call_nearby(&self, method: &str, params: &Value) -> Result<Value> {
        match method {
            "nearby/status" => Ok(self.nearby.view()),
            "nearby/browse" => self.nearby_browse(params).await,
            "nearby/receive_start" => self.nearby_receive_start(params).await,
            "nearby/receive_accept" => self.nearby_receive_accept(params),
            "nearby/receive_reject" => self.nearby_receive_reject(params),
            "nearby/receive_cancel" => self.nearby_receive_cancel().await,
            "nearby/send_start" => self.nearby_send_start(params).await,
            "nearby/send_confirm_sas" => self
                .nearby
                .core
                .send_confirm_sas()
                .map(|_| self.nearby.view()),
            "nearby/send_cancel" => {
                self.nearby.core.send_cancel();
                Ok(self.nearby.view())
            }
            "nearby/scan" => self.nearby_scan(params).await,
            _ => Err(Error::NotFound),
        }
    }

    async fn owned_hold(&self) -> Result<DaemonHold> {
        let daemon = self.nearby.daemon()?;
        let generation = self
            .hold()
            .with(|c| {
                ensure(c.session_state().state == "unlocked", Error::Locked)?;
                Ok(c.session_generation())
            })
            .await?;
        Ok(DaemonHold { daemon, generation })
    }

    async fn nearby_browse(&self, params: &Value) -> Result<Value> {
        let timeout_ms = params["timeout_ms"]
            .as_u64()
            .unwrap_or(2_500)
            .clamp(400, 10_000);
        let timeout = Duration::from_millis(timeout_ms);
        let own_advert = self.nearby.core.view()["receive"]["advert"]
            .as_str()
            .map(str::to_owned);

        let mdns_task = tokio::task::spawn_blocking(move || mdns::browse(timeout));
        let awdl_task = tokio::task::spawn_blocking(move || nearby_awdl::browse(timeout));
        let (mdns_res, awdl_res) = tokio::join!(mdns_task, awdl_task);

        let mut peers: Vec<DiscoveredPeer> = Vec::new();
        let mut seen = BTreeSet::new();
        let mut errors: Vec<String> = Vec::new();

        match awdl_res {
            Ok(Ok(list)) => {
                for peer in list {
                    if own_advert.as_deref() == Some(peer.advert.as_str()) {
                        continue;
                    }
                    let key = (peer.advert.clone(), peer.transport.clone());
                    if seen.insert(key) {
                        peers.push(peer);
                    }
                }
            }
            Ok(Err(e)) => errors.push(e.to_string()),
            Err(_) => {}
        }

        match mdns_res {
            Ok(Ok(list)) => {
                for peer in list {
                    if own_advert.as_deref() == Some(peer.advert.as_str()) {
                        continue;
                    }
                    let key = (peer.advert.clone(), peer.transport.clone());
                    if seen.insert(key) {
                        peers.push(peer);
                    }
                }
            }
            Ok(Err(e)) => errors.push(e.to_string()),
            Err(_) => {}
        }

        Ok(json!({
            "peers": peers,
            "errors": errors,
        }))
    }

    async fn nearby_receive_start(&self, params: &Value) -> Result<Value> {
        let h = self.owned_hold().await?;
        let daemon = h.daemon.clone();
        let alias = params["alias"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(default_alias);
        let discoverable = params["discoverable"].as_bool().unwrap_or(false);
        let on_change = self.nearby.change_hook(daemon.clone());

        {
            let mut handles = self
                .nearby
                .handles
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            handles.advert = None;
            handles.awdl = None;
            handles.mdns_error = None;
        }

        let started = self
            .nearby
            .core
            .receive_start(
                h.clone(),
                Some(alias.clone()),
                discoverable,
                dmsg_client::files::MAX_FILE,
                on_change.clone(),
            )
            .await?;

        if discoverable {
            let adv = started["receive"]["advert"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let pk_hint = started["receive"]["pk_hint"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let port = started["receive"]["port"].as_u64().unwrap_or(0) as u16;

            if !adv.is_empty() && port > 0 {
                let adv_m = adv.clone();
                let alias_m = alias.clone();
                let hint_m = pk_hint.clone();
                let mdns_res = tokio::task::spawn_blocking(move || {
                    mdns::advertise(&adv_m, port, &alias_m, &hint_m)
                })
                .await
                .map_err(|e| Error::Unavailable(format!("mdns: {e}")))?;

                let core_awdl = self.nearby.core.clone();
                let h_awdl = h.clone();
                let cb_awdl = on_change.clone();
                let awdl_res = nearby_awdl::listen(
                    &adv,
                    &alias,
                    &pk_hint,
                    Arc::new(move |bridge| {
                        let _ = core_awdl.receive_attach_bridge(
                            h_awdl.clone(),
                            bridge,
                            dmsg_client::files::MAX_FILE,
                            cb_awdl.clone(),
                        );
                    }),
                );

                let mut handles = self
                    .nearby
                    .handles
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                match mdns_res {
                    Ok(advert) => handles.advert = Some(advert),
                    Err(e) => handles.mdns_error = Some(e.to_string()),
                }
                if let Ok(listener) = awdl_res {
                    handles.awdl = Some(listener);
                }
            }
        }

        crate::logs::write("nearby/receive_start", &[("discoverable", &discoverable)]);
        Ok(self.nearby.view())
    }

    fn nearby_receive_accept(&self, params: &Value) -> Result<Value> {
        let peer_id = text(params, "peer_id")?;
        let folder = params["folder"].as_str().unwrap_or("root").to_string();
        let selected: Vec<u32> = optional(params, "selected", parse)?.unwrap_or_default();
        let share_identity = params["share_identity"].as_bool().unwrap_or(false);
        self.nearby
            .core
            .receive_accept(&peer_id, folder, selected, share_identity)?;
        Ok(self.nearby.view())
    }

    fn nearby_receive_reject(&self, params: &Value) -> Result<Value> {
        let peer_id = text(params, "peer_id")?;
        self.nearby.core.receive_reject(&peer_id)?;
        Ok(self.nearby.view())
    }

    async fn nearby_receive_cancel(&self) -> Result<Value> {
        {
            let mut handles = self
                .nearby
                .handles
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            handles.advert = None;
            handles.awdl = None;
            handles.mdns_error = None;
        }
        let h = self.hold();
        let _ = self.nearby.core.receive_cancel(Some(&h)).await;
        Ok(self.nearby.view())
    }

    async fn nearby_send_start(&self, params: &Value) -> Result<Value> {
        let h = self.owned_hold().await?;
        let daemon = h.daemon.clone();
        let env = h
            .with(|c| {
                Ok(dmsg_client::config::environment_name(&c.config().environment).to_string())
            })
            .await?;
        let approval_source = source(params).unwrap_or("gui").to_string();
        let alias = params["alias"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .or_else(|| Some(default_alias()));
        let share_identity = params["share_identity"].as_bool().unwrap_or(false);

        let payload = if let Some(vault_val) = params.get("vault_item").filter(|v| !v.is_null()) {
            let input: ShareInput = serde_json::from_value(vault_val.clone())
                .map_err(|_| Error::InvalidInput("vault_item".into()))?;
            SendPayload::VaultItem(input)
        } else if let Some(files_val) = params.get("files").filter(|v| !v.is_null()) {
            let files: Vec<FileParam> = serde_json::from_value(files_val.clone())
                .map_err(|_| Error::InvalidInput("files".into()))?;
            ensure(!files.is_empty(), Error::InvalidInput("files".into()))?;
            SendPayload::Files(
                files
                    .into_iter()
                    .map(|f| SendFileItem {
                        file_id: f.file_id,
                        version_id: f.version_id,
                    })
                    .collect(),
            )
        } else {
            return Err(Error::InvalidInput("missing files or vault_item".into()));
        };

        let target = if let Some(offer_text) = params["offer"]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let offer = NearbyOffer::parse(offer_text, Some(&env))?;
            let awdl = offer
                .adv
                .as_deref()
                .and_then(|adv| nearby_awdl::connect(adv).ok());
            SendTarget::Offer {
                offer,
                resolved: None,
                awdl,
            }
        } else {
            let transport = params["transport"].as_str().unwrap_or("lan");
            let peer_alias = params["peer_alias"].as_str().map(str::to_owned);
            let advert = params["advert"].as_str().map(str::to_owned);
            let awdl = if transport == "awdl" {
                advert.as_deref().and_then(|a| nearby_awdl::connect(a).ok())
            } else {
                None
            };
            let host = params["host"].as_str().unwrap_or_default().to_string();
            let port = params["port"].as_u64().unwrap_or(0) as u16;
            ensure(
                awdl.is_some() || (!host.is_empty() && port > 0),
                Error::InvalidInput("missing offer or peer host/port".into()),
            )?;
            SendTarget::Direct {
                host,
                port,
                alias: peer_alias,
                awdl,
            }
        };

        let on_change = self.nearby.change_hook(daemon);
        self.nearby
            .core
            .send_start(
                h,
                target,
                payload,
                alias,
                share_identity,
                approval_source,
                on_change,
            )
            .await?;
        Ok(self.nearby.view())
    }

    async fn nearby_scan(&self, params: &Value) -> Result<Value> {
        let encoded = text(params, "image")?;
        ensure(encoded.len() <= 12 * 1024 * 1024, Error::QuotaExceeded)?;
        let image = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| Error::InvalidInput("image".into()))?;
        ensure(image.len() <= 8 * 1024 * 1024, Error::QuotaExceeded)?;
        let h = self.hold();
        let (session, env) = h
            .with(|c| {
                Ok((
                    c.session_generation(),
                    dmsg_client::config::environment_name(&c.config().environment).to_string(),
                ))
            })
            .await?;
        let offer_text = tokio::task::spawn_blocking(move || qr::scan(&image))
            .await
            .map_err(|e| Error::Unavailable(e.to_string()))??;
        h.with(|c| ensure(c.session_generation() == session, Error::Locked))
            .await?;
        let offer = NearbyOffer::parse(&offer_text, Some(&env))?;
        Ok(json!({
            "offer": offer_text,
            "expires_at": offer.exp,
            "addrs": offer.addrs,
            "has_relay": offer.relay.is_some(),
            "advert": offer.adv,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::home::Home;
    use dmsg_client::testing::{TempDir, unlocked};

    fn test_daemon(dir: &TempDir) -> Arc<Daemon> {
        let home = Home {
            dir: dir.0.join("home"),
        };
        let daemon = Arc::new(Daemon::new(
            "owner-token".into(),
            0,
            home.settings().unwrap(),
            unlocked(&dir.0.join("workspace"), false, None),
            dmsg_agent::Runtime::new(&dmsg_agent::Options::default()).unwrap(),
            crate::mcp_events::McpEvents::new(&[]).unwrap(),
        ));
        daemon.nearby.bind(Arc::downgrade(&daemon));
        daemon
    }

    #[tokio::test]
    async fn daemon_nearby_receive_send_files_and_lock_teardown() {
        let dir = TempDir::new();
        let sender_d = test_daemon(&dir);
        let dir2 = TempDir::new();
        let receiver_d = test_daemon(&dir2);

        let sample = dir.0.join("hello-nearby.txt");
        std::fs::write(&sample, b"daemon nearby share payload").unwrap();
        let file_id = {
            let mut c = sender_d.client().lock().await;
            c.files_import(&sample, "root", true, &mut |_, _| {})
                .unwrap()
                .file
                .id
        };

        let started = receiver_d
            .call_nearby("nearby/receive_start", &json!({"alias": "Bob Mac"}))
            .await
            .unwrap();
        assert_eq!(started["receive"]["state"], "waiting");
        assert!(
            started["receive"]["qr"]
                .as_str()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        let offer = started["receive"]["offer"].as_str().unwrap().to_string();

        sender_d
            .call_nearby(
                "nearby/send_start",
                &json!({
                    "offer": offer,
                    "files": [{"file_id": file_id}],
                    "alias": "Alice Mac",
                }),
            )
            .await
            .unwrap();

        let mut peer_id = String::new();
        for _ in 0..100 {
            let st = receiver_d.nearby.view();
            if let Some(arr) = st["receive"]["pending_peers"].as_array()
                && !arr.is_empty()
            {
                peer_id = arr[0]["peer_id"].as_str().unwrap().to_string();
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert!(!peer_id.is_empty(), "pending peer never arrived");

        receiver_d
            .call_nearby(
                "nearby/receive_accept",
                &json!({"peer_id": peer_id, "folder": "root", "selected": [0]}),
            )
            .await
            .unwrap();

        for _ in 0..100 {
            let rv = receiver_d.nearby.view();
            let sv = sender_d.nearby.view();
            if rv["receive"]["state"] == "done" && sv["send"]["state"] == "done" {
                assert_eq!(
                    rv["receive"]["outcome"]["files"][0]["name"],
                    "hello-nearby.txt"
                );
                // Cancel resets cleanly after completion.
                receiver_d.nearby.cancel_all().await;
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!(
            "nearby daemon transfer timed out: recv={} send={}",
            receiver_d.nearby.view(),
            sender_d.nearby.view()
        );
    }
}
