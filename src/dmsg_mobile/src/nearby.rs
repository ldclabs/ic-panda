//! Native mobile nearby sharing (`dmsg-nearby-share/1`) over UniFFI, including
//! M1c byte-bridge endpoints for iOS `Network.framework` AWDL `NWConnection`.

use crate::error::Result;
use crate::secrets::{MAX_PACKAGE_BYTES, SecretSnapshotInput};
use crate::{MobileEngine, ObjectSnapshot, runtime};
use dmsg_client::hold::Hold;
use dmsg_client::nearby::{
    BridgeHandle, BridgeTransport, NearbyHost, NearbyOffer, SendFileItem, SendPayload, SendTarget,
    sanitize_alias,
};
use dmsg_types::{Error, ensure};
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as AsyncMutex;

/// One local library file selected for nearby sharing.
#[derive(uniffi::Record)]
pub struct NearbyFileItem {
    pub file_id: String,
    pub version_id: Option<String>,
}

struct BridgeSlot {
    inbound: tokio::sync::mpsc::Sender<Vec<u8>>,
    outbound: Arc<AsyncMutex<tokio::sync::mpsc::Receiver<Vec<u8>>>>,
    pending_outgoing: Option<BridgeTransport>,
}

/// Mobile-owned wrapper around `NearbyHost` and active native AWDL byte bridges.
#[derive(Default)]
pub(crate) struct MobileNearby {
    pub(crate) core: Arc<NearbyHost>,
    next_bridge: AtomicU64,
    bridges: Mutex<HashMap<u64, BridgeSlot>>,
}

impl MobileNearby {
    /// Immediately cancel active send/receive sessions and drop all AWDL bridges.
    pub(crate) fn cancel_sync(&self) {
        self.core.clear();
        self.bridges
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    fn allocate_bridge(&self) -> (u64, BridgeTransport) {
        let (transport, handle) = BridgeTransport::pair(16);
        let id = self.next_bridge.fetch_add(1, Ordering::Relaxed) + 1;
        let BridgeHandle { inbound, outbound } = handle;
        let slot = BridgeSlot {
            inbound,
            outbound: Arc::new(AsyncMutex::new(outbound)),
            pending_outgoing: None,
        };
        let mut map = self.bridges.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(id, slot);
        (id, transport)
    }

    fn take_outgoing_bridge(&self, id: u64) -> Result<BridgeTransport> {
        let mut map = self.bridges.lock().unwrap_or_else(|e| e.into_inner());
        map.get_mut(&id)
            .and_then(|s| s.pending_outgoing.take())
            .ok_or_else(|| Error::NotFound.into())
    }
}

/// Validate a `dmsg-nearby/1:...` QR or pasted offer and return a JSON summary.
#[uniffi::export]
pub fn nearby_parse_offer(text: String, expected_env: Option<String>) -> Result<String> {
    let offer = NearbyOffer::parse(&text, expected_env.as_deref())?;
    Ok(json!({
        "offer": text.trim(),
        "expires_at": offer.exp,
        "addrs": offer.addrs,
        "advert": offer.adv,
        "has_relay": offer.relay.is_some(),
        "pk_hint": dmsg_client::nearby::pk_hint(&offer.pk),
    })
    .to_string())
}

/// Sanitize a device display alias for mDNS / AWDL TXT advertisements.
#[uniffi::export]
pub fn nearby_sanitize_alias(raw: String) -> String {
    sanitize_alias(&raw)
}

#[uniffi::export]
impl MobileEngine {
    /// Snapshot of current nearby receive/send status, pending peers, SAS and progress.
    pub fn nearby_status(&self) -> ObjectSnapshot {
        ObjectSnapshot {
            generation: self.host.generation(),
            json: self.nearby.core.view().to_string(),
        }
    }

    /// Start a foreground nearby receive session. Defaults to QR-only (`discoverable = false`).
    pub async fn nearby_receive_start(
        &self,
        alias: Option<String>,
        discoverable: bool,
    ) -> Result<ObjectSnapshot> {
        let nearby = self.nearby.clone();
        let generation = self.host.generation();
        let view = self
            .held(move |h| {
                Box::pin(async move {
                    let noop: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
                    nearby
                        .core
                        .receive_start(h.clone(), alias, discoverable, MAX_PACKAGE_BYTES, noop)
                        .await
                })
            })
            .await?;
        Ok(ObjectSnapshot {
            generation,
            json: view.to_string(),
        })
    }

    /// Accept one pending peer after checking its 6-digit verification code.
    pub fn nearby_receive_accept(
        &self,
        peer_id: String,
        folder: String,
        selected: Vec<u32>,
        share_identity: bool,
    ) -> Result<ObjectSnapshot> {
        let view = self
            .nearby
            .core
            .receive_accept(&peer_id, folder, selected, share_identity)?;
        Ok(ObjectSnapshot {
            generation: self.host.generation(),
            json: view.to_string(),
        })
    }

    /// Reject one pending peer while keeping the receiver open for others.
    pub fn nearby_receive_reject(&self, peer_id: String) -> Result<ObjectSnapshot> {
        let view = self.nearby.core.receive_reject(&peer_id)?;
        Ok(ObjectSnapshot {
            generation: self.host.generation(),
            json: view.to_string(),
        })
    }

    /// Stop the active nearby receive session and cancel any open relay room.
    pub async fn nearby_receive_cancel(&self) -> Result<ObjectSnapshot> {
        {
            let mut map = self
                .nearby
                .bridges
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            map.clear();
        }
        let host = self.host.clone();
        let nearby = self.nearby.clone();
        let generation = self.host.generation();
        let view = runtime()
            .spawn(async move { nearby.core.receive_cancel(Some(&host)).await })
            .await
            .map_err(|_| Error::Unavailable("nearby cancel".into()))?;
        Ok(ObjectSnapshot {
            generation,
            json: view.to_string(),
        })
    }

    /// Start sending one or more local library files via QR offer, mDNS peer or AWDL bridge.
    #[allow(clippy::too_many_arguments)]
    pub async fn nearby_send_files(
        &self,
        offer: Option<String>,
        host: Option<String>,
        port: Option<u16>,
        peer_alias: Option<String>,
        resolved_host: Option<String>,
        resolved_port: Option<u16>,
        awdl_bridge_id: Option<u64>,
        files: Vec<NearbyFileItem>,
        alias: Option<String>,
        share_identity: bool,
    ) -> Result<ObjectSnapshot> {
        ensure(!files.is_empty(), Error::InvalidInput("files".into()))?;
        let awdl = awdl_bridge_id
            .map(|id| self.nearby.take_outgoing_bridge(id))
            .transpose()?;
        let items: Vec<SendFileItem> = files
            .into_iter()
            .map(|f| SendFileItem {
                file_id: f.file_id,
                version_id: f.version_id,
            })
            .collect();
        self.nearby_send_inner(
            offer,
            host,
            port,
            peer_alias,
            resolved_host,
            resolved_port,
            awdl,
            SendPayload::Files(items),
            alias,
            share_identity,
        )
        .await
    }

    /// Start sending a Vault snapshot (`dmsg-secret-package/1`) directly after receiver acceptance.
    #[allow(clippy::too_many_arguments)]
    pub async fn nearby_send_vault(
        &self,
        offer: Option<String>,
        host: Option<String>,
        port: Option<u16>,
        peer_alias: Option<String>,
        resolved_host: Option<String>,
        resolved_port: Option<u16>,
        awdl_bridge_id: Option<u64>,
        input: SecretSnapshotInput,
        alias: Option<String>,
        share_identity: bool,
    ) -> Result<ObjectSnapshot> {
        let awdl = awdl_bridge_id
            .map(|id| self.nearby.take_outgoing_bridge(id))
            .transpose()?;
        self.nearby_send_inner(
            offer,
            host,
            port,
            peer_alias,
            resolved_host,
            resolved_port,
            awdl,
            SendPayload::VaultItem {
                input: input.into(),
                max_plain_bytes: MAX_PACKAGE_BYTES,
            },
            alias,
            share_identity,
        )
        .await
    }

    /// Confirm the 6-digit verification code on the sender side in browse mode.
    pub fn nearby_send_confirm_sas(&self) -> Result<ObjectSnapshot> {
        let view = self.nearby.core.send_confirm_sas()?;
        Ok(ObjectSnapshot {
            generation: self.host.generation(),
            json: view.to_string(),
        })
    }

    /// Cancel any active nearby send session.
    pub fn nearby_send_cancel(&self) -> ObjectSnapshot {
        self.nearby.core.send_cancel();
        self.nearby_status()
    }

    /// Open a native AWDL byte bridge.
    ///
    /// When `incoming == true`, attaches directly to the waiting receive session.
    /// When `incoming == false`, returns a `bridge_id` to pass into `nearby_send_files` / `nearby_send_vault`.
    pub fn nearby_bridge_open(&self, incoming: bool) -> Result<u64> {
        let (id, transport) = self.nearby.allocate_bridge();
        if incoming {
            let noop: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
            let host = self.host.clone();
            let nearby = self.nearby.clone();
            let _guard = runtime().enter();
            if let Err(err) =
                nearby
                    .core
                    .receive_attach_bridge(host, transport, MAX_PACKAGE_BYTES, noop)
            {
                let mut map = self
                    .nearby
                    .bridges
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                map.remove(&id);
                return Err(err.into());
            }
        } else {
            let mut map = self
                .nearby
                .bridges
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if let Some(slot) = map.get_mut(&id) {
                slot.pending_outgoing = Some(transport);
            }
        }
        Ok(id)
    }

    /// Push one complete length-stripped AWDL frame received by `NWConnection` into Rust.
    pub async fn nearby_bridge_push(&self, bridge_id: u64, frame: Vec<u8>) -> Result<()> {
        let tx = {
            let map = self
                .nearby
                .bridges
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            map.get(&bridge_id)
                .map(|s| s.inbound.clone())
                .ok_or(Error::NotFound)?
        };
        tx.send(frame)
            .await
            .map_err(|_| Error::Unavailable("nearby bridge closed".into()).into())
    }

    /// Pull the next outbound frame for `NWConnection` to send (returns `None` when closed).
    pub async fn nearby_bridge_pull(&self, bridge_id: u64) -> Result<Option<Vec<u8>>> {
        let rx = {
            let map = self
                .nearby
                .bridges
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            map.get(&bridge_id)
                .map(|s| s.outbound.clone())
                .ok_or(Error::NotFound)?
        };
        let mut guard = rx.lock().await;
        Ok(guard.recv().await)
    }

    /// Close and release a native AWDL byte bridge.
    pub fn nearby_bridge_close(&self, bridge_id: u64) {
        let mut map = self
            .nearby
            .bridges
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        map.remove(&bridge_id);
    }
}

impl MobileEngine {
    #[allow(clippy::too_many_arguments)]
    async fn nearby_send_inner(
        &self,
        offer: Option<String>,
        host: Option<String>,
        port: Option<u16>,
        peer_alias: Option<String>,
        resolved_host: Option<String>,
        resolved_port: Option<u16>,
        awdl: Option<BridgeTransport>,
        payload: SendPayload,
        alias: Option<String>,
        share_identity: bool,
    ) -> Result<ObjectSnapshot> {
        let nearby = self.nearby.clone();
        let generation = self.host.generation();
        let view = self
            .held(move |h| {
                Box::pin(async move {
                    let env = h
                        .with(|c| {
                            Ok(
                                dmsg_client::config::environment_name(&c.config().environment)
                                    .to_string(),
                            )
                        })
                        .await?;
                    let target = if let Some(text) =
                        offer.as_deref().map(str::trim).filter(|s| !s.is_empty())
                    {
                        let parsed = NearbyOffer::parse(text, Some(&env))?;
                        let resolved = match (resolved_host, resolved_port) {
                            (Some(rh), Some(rp)) if !rh.is_empty() && rp > 0 => Some((rh, rp)),
                            _ => None,
                        };
                        SendTarget::Offer {
                            offer: parsed,
                            resolved,
                            awdl,
                        }
                    } else {
                        let direct_host = host.unwrap_or_default();
                        let direct_port = port.unwrap_or(0);
                        ensure(
                            awdl.is_some() || (!direct_host.is_empty() && direct_port > 0),
                            Error::InvalidInput("missing offer or peer endpoint".into()),
                        )?;
                        SendTarget::Direct {
                            host: direct_host,
                            port: direct_port,
                            alias: peer_alias,
                            awdl,
                        }
                    };
                    let noop: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
                    nearby
                        .core
                        .send_start(h.clone(), target, payload, alias, share_identity, noop)
                        .await
                })
            })
            .await?;
        Ok(ObjectSnapshot {
            generation,
            json: view.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::ScopedHost;
    use dmsg_client::host::Host;
    use dmsg_client::library::catalog::ROOT;
    use std::time::Duration;

    #[tokio::test]
    async fn mobile_nearby_awdl_bridge_transfers_file_end_to_end() {
        let dir = dmsg_client::testing::TempDir::new();
        let mut sender_c = dmsg_client::testing::unlocked(&dir.0.join("s"), false, None);
        let receiver_c = dmsg_client::testing::unlocked(&dir.0.join("r"), false, None);
        let path = dir.0.join("mobile-awdl.txt");
        std::fs::write(&path, b"hello ios awdl bridge").unwrap();
        let imported = sender_c
            .files_import(&path, ROOT, true, &mut |_, _| {})
            .unwrap();

        let s_host = Host::start(sender_c, &tokio::runtime::Handle::current());
        let r_host = Host::start(receiver_c, &tokio::runtime::Handle::current());
        let s_scoped = ScopedHost::new(s_host.clone(), s_host.generation());
        let r_scoped = ScopedHost::new(r_host.clone(), r_host.generation());

        let s_nearby = Arc::new(MobileNearby::default());
        let r_nearby = Arc::new(MobileNearby::default());
        let noop: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});

        let started = r_nearby
            .core
            .receive_start(
                r_scoped.clone(),
                Some("Bob iPhone".into()),
                false,
                MAX_PACKAGE_BYTES,
                noop.clone(),
            )
            .await
            .unwrap();
        let offer_text = started["receive"]["offer"].as_str().unwrap().to_string();
        let offer = NearbyOffer::parse(&offer_text, Some("local")).unwrap();

        let (r_bid, r_transport) = r_nearby.allocate_bridge();
        let (s_bid, s_transport) = s_nearby.allocate_bridge();
        r_nearby
            .core
            .receive_attach_bridge(
                r_scoped.clone(),
                r_transport,
                MAX_PACKAGE_BYTES,
                noop.clone(),
            )
            .unwrap();

        // Pump frames between the two bridge IDs just like iOS NWConnection Swift tasks do.
        let r_pump = r_nearby.clone();
        let s_pump = s_nearby.clone();
        let r_out = r_pump
            .bridges
            .lock()
            .unwrap()
            .get(&r_bid)
            .unwrap()
            .outbound
            .clone();
        let s_in = s_pump
            .bridges
            .lock()
            .unwrap()
            .get(&s_bid)
            .unwrap()
            .inbound
            .clone();
        let s_out = s_pump
            .bridges
            .lock()
            .unwrap()
            .get(&s_bid)
            .unwrap()
            .outbound
            .clone();
        let r_in = r_pump
            .bridges
            .lock()
            .unwrap()
            .get(&r_bid)
            .unwrap()
            .inbound
            .clone();
        let pump1 = tokio::spawn(async move {
            let mut rx = r_out.lock().await;
            while let Some(frame) = rx.recv().await {
                if s_in.send(frame).await.is_err() {
                    break;
                }
            }
        });
        let pump2 = tokio::spawn(async move {
            let mut rx = s_out.lock().await;
            while let Some(frame) = rx.recv().await {
                if r_in.send(frame).await.is_err() {
                    break;
                }
            }
        });

        s_nearby
            .core
            .send_start(
                s_scoped.clone(),
                SendTarget::Offer {
                    offer,
                    resolved: None,
                    awdl: Some(s_transport),
                },
                SendPayload::Files(vec![SendFileItem {
                    file_id: imported.file.id,
                    version_id: None,
                }]),
                Some("Alice iPhone".into()),
                false,
                noop,
            )
            .await
            .unwrap();

        let mut peer_id = String::new();
        for _ in 0..100 {
            let v = r_nearby.core.view();
            if let Some(arr) = v["receive"]["pending_peers"].as_array()
                && !arr.is_empty()
            {
                peer_id = arr[0]["peer_id"].as_str().unwrap().to_string();
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(!peer_id.is_empty());
        r_nearby
            .core
            .receive_accept(&peer_id, ROOT.into(), vec![0], false)
            .unwrap();

        for _ in 0..100 {
            let rv = r_nearby.core.view();
            let sv = s_nearby.core.view();
            if rv["receive"]["state"] == "done" && sv["send"]["state"] == "done" {
                assert_eq!(
                    rv["receive"]["outcome"]["files"][0]["name"],
                    "mobile-awdl.txt"
                );
                pump1.abort();
                pump2.abort();
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("mobile nearby awdl bridge timed out");
    }
}
