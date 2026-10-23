//! Offline clients for unit tests: a bound workspace and an opened root,
//! with every endpoint unreachable; and the test account's security under
//! one epoch, with devices that sign commands, behind no certificate.

use crate::account::Security;
use crate::client::Client;
use crate::cloud::{Evidence, Signer, sign_command};
use crate::config::{Config, LOGIN_URL};
use crate::root::RootKeys;
use crate::secure::SecureStorage;
use crate::workspace::AccountRef;
use candid::Principal;
use dmsg_types::user::{
    Capability, ControllerRole, Device, DeviceInput, SecuritySnapshot, VaultWriteState,
};
use dmsg_types::{AccountId, Environment, Hash};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

/// The test account's unlock secret.
pub const SECRET: [u8; 32] = [9; 32];

/// A directory removed when dropped.
pub struct TempDir(pub PathBuf);

impl TempDir {
    /// A fresh directory under the system's temporary directory.
    pub fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "dmsg-client-{}",
            hex::encode(crate::crypto::random_id())
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    /// A path inside it.
    pub fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Default for TempDir {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A credential store in memory.
#[derive(Default)]
pub struct MemoryStorage(std::sync::Mutex<std::collections::BTreeMap<String, Vec<u8>>>);

impl SecureStorage for MemoryStorage {
    fn get(&self, name: &str) -> dmsg_types::Result<Option<zeroize::Zeroizing<Vec<u8>>>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(name)
            .cloned()
            .map(zeroize::Zeroizing::new))
    }

    fn set(&self, name: &str, secret: &[u8]) -> dmsg_types::Result<()> {
        self.0.lock().unwrap().insert(name.into(), secret.to_vec());
        Ok(())
    }

    fn delete(&self, name: &str) -> dmsg_types::Result<()> {
        self.0.lock().unwrap().remove(name);
        Ok(())
    }
}

/// The test account.
pub fn account() -> AccountRef {
    AccountRef {
        id: AccountId::from_str("040g2081040g2081040g").unwrap(),
        home: Principal::from_text("aaaaa-aa").unwrap(),
        issuer: "https://dmsg.test/u/040g2081040g2081040g".into(),
    }
}

/// A local configuration whose endpoints are unreachable.
pub fn config(background_online: bool) -> Config {
    Config {
        environment: Environment::Local,
        ic_host: "http://127.0.0.1:9".into(),
        relay: "http://127.0.0.1:9".into(),
        handle: Principal::from_text("aaaaa-aa").unwrap(),
        user_homes: vec![],
        root_key: Some(vec![]),
        login_url: LOGIN_URL.into(),
        connect_url: crate::config::CONNECT_URL.into(),
        background_online,
        files_max_bytes: crate::config::DEFAULT_FILES_MAX_BYTES,
        files_trash_days: crate::config::DEFAULT_FILES_TRASH_DAYS,
        wallet: Default::default(),
        legacy: None,
    }
}

/// An unlocked client of the test account under `dir`, holding root
/// generation 1 (`[5; 32]`).
pub fn unlocked(
    dir: &Path,
    background_online: bool,
    secure: Option<Arc<dyn SecureStorage>>,
) -> Client {
    let mut client = Client::open(config(background_online), dir, secure).unwrap();
    client
        .select(crate::client::Slot::Account(account().id))
        .unwrap();
    let keys = client.workspace.unlock_provisional().unwrap();
    client.workspace.bind(&keys, account(), &SECRET).unwrap();
    client.workspace.meta.login_unlocked_at = Some(crate::now_ms());
    client.workspace.save_meta().unwrap();
    client.keys = Some(client.workspace.unlock(&SECRET).unwrap());
    let root = RootKeys::single(1, [5; 32]);
    client.save_root(&root).unwrap();
    client.root = Some(root);
    client
}

/// Point `client`'s relay at `origin`, a loopback server the test runs.
pub fn relay_to(client: &mut Client, origin: &str) {
    client.config.relay = origin.into();
    client.cloud = crate::cloud::Cloud::new(origin, true).unwrap();
}

/// Keep a dedicated Mail callback without creating an external relay endpoint.
pub fn mail_push_callback(client: &mut Client, agent: &str) -> crate::mail_push::Callback {
    use crate::mcp_events::{Binding, Endpoint, Secret};
    let endpoint = Endpoint {
        endpoint_id: "callback".into(),
        url: "https://callback.example/mcp-events/callback".into(),
        secrets: vec![Secret {
            secret: "test-secret".into(),
            created_at: crate::now_ms(),
            retire_at: None,
        }],
        agent: Some(agent.into()),
        mcp_session: Some("mail-push".into()),
        context: BTreeMap::from([("label".into(), "dMsg Mail push".into())]),
        binding: Binding::default(),
        state: "active".into(),
        relay_seq: 0,
        created_at: crate::now_ms(),
        ended_at: None,
    };
    let endpoints = vec![endpoint];
    client
        .workspace
        .put_private(client.keys().unwrap(), "mcp-endpoints", &endpoints)
        .unwrap();
    client.mail_push_endpoints_changed(&endpoints).unwrap();
    client.mail_push_endpoint(agent).unwrap().unwrap()
}

/// Point `client`'s queries at `origin`, a loopback server the test runs.
pub fn ic_to(client: &mut Client, origin: &str) {
    client.config.ic_host = origin.into();
}

/// Give `client` fresh certified security the relay is taken to hold, and
/// the user homes, as a warmup brings them: its relay requests are signed
/// at once.
pub fn warm(client: &mut Client) {
    let mut state = security(1, BTreeMap::new());
    state.certified_at = crate::now_ms();
    client.published = Some(state.certified_at);
    client.security = Some(state);
    client.homes = Some(vec![account().home]);
}

/// Keep an active, empty channel owned by the test account, without network setup.
pub fn joined_channel(client: &mut Client, channel: &str) -> crate::channel_log::Ledger {
    use crate::channel_log::{ChannelType, Genesis, Status, start};
    use crate::codec::canonical;
    use serde_json::json;
    let head = "a1".repeat(32);
    let mut ledger = start(
        &Genesis {
            channel_id: channel.into(),
            nonce: "b1".repeat(32),
            kind: ChannelType::Collaboration,
        },
        &account().id.to_string(),
        &head,
    )
    .unwrap();
    ledger.epoch = 1;
    ledger.status = Status::Active;
    client
        .write(
            crate::channel::CHANNEL,
            channel.into(),
            None,
            false,
            &canonical(&json!({
                "format": "dmsg-channel-client/2", "id": channel, "name": "test channel",
                "genesis": head, "keys": {}, "muted": false, "hidden": false,
            })),
            None,
        )
        .unwrap();
    client
        .workspace
        .put_private(
            client.keys().unwrap(),
            &format!("channel:{channel}"),
            &json!({
                "ledger": ledger, "heads": {head: 0},
            }),
        )
        .unwrap();
    ledger
}

/// Keep message `seq` of `channel`, a PNG the test account shared, in
/// `client`'s workspace as a read of the channel keeps it; its upload is
/// nowhere.
pub fn shared_image(client: &Client, channel: &str, seq: u64) {
    let message = crate::channel::Message {
        seq,
        account: account().id.to_string(),
        file: Some(crate::channel::FileRef {
            upload_id: "ab".repeat(32),
            file_id: "cd".repeat(32),
            version: "ef".repeat(32),
            epoch: 1,
            manifest_digest: "01".repeat(32),
            name: "image.png".into(),
            size: 1024,
            mime: "image/png".into(),
            key: crate::codec::b64(&[7; 32]),
        }),
        ..Default::default()
    };
    client
        .workspace
        .put_private(
            client.keys().unwrap(),
            &format!("{}{channel}:{seq:020}", crate::channel::INBOX),
            &message,
        )
        .unwrap();
}

/// Device `n` of the test account: its seed is `[n; 32]`, and it signs
/// content unless revoked at `revoked_at`.
pub fn device(n: u8, revoked_at: Option<u64>) -> (Hash, Device) {
    let id = Hash::new([n.wrapping_add(100); 32]);
    let device = Device {
        input: DeviceInput {
            device_id: id,
            signing_pub: Hash::new(crate::crypto::signing_public(&[n; 32])),
            hpke_pub: Hash::new([n; 32]),
            role: ControllerRole::Administrator,
            capabilities: vec![Capability::ContentSign, Capability::VaultUnlock],
        },
        added_at: 1,
        added_by: None,
        revoked_at,
        next_sequence: 0,
    };
    (id, device)
}

/// The test account's security under `epoch` with `devices`, as a checked
/// certificate would give it.
pub fn security(epoch: u64, devices: BTreeMap<Hash, Device>) -> Security {
    let account = account();
    Security {
        snapshot: SecuritySnapshot {
            issuer: account.issuer,
            home_cose: account.home,
            schema: 4,
            account_id: account.id,
            home_user: account.home,
            account_version: 1,
            security_epoch: epoch,
            devices_root: Hash::new([0; 32]),
            recovery_delay_ms: 0,
            pending_recovery_digest: None,
            content_root_generation: 1,
            content_root_digest: None,
            vault_write_state: VaultWriteState::Ready,
            principal_updated_at: None,
        },
        devices,
        evidence: Evidence {
            schema: 1,
            canister: account.home.to_text(),
            certificate: String::new(),
            entries: vec![],
        },
        certified_at: 1,
    }
}

/// A command `action` of the test account signed by device `n` under
/// `epoch`, its payload `{"by": n}`.
pub fn signed(n: u8, epoch: u64, action: &str) -> String {
    command(n, epoch, action, &BTreeMap::from([("by", n.to_string())]))
}

/// A command `action` with `payload` of the test account, signed by device
/// `n` under `epoch`; its deadline is 10.
pub fn command<P: serde::Serialize>(n: u8, epoch: u64, action: &str, payload: &P) -> String {
    let issuer = account().issuer;
    let signer = Signer {
        issuer: &issuer,
        device_id: device(n, None).0,
        security_epoch: epoch,
        seed: &[n; 32],
        expires_at: u64::MAX,
    };
    sign_command(&signer, action, payload, "01", 10)
}
