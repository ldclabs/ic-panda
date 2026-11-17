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

pub use crate::passkey::test_session as passkey_session;

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
        auth: None,
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

/// Synthetic complete state of the offline account; no certificate verification.
pub fn account_state(client: &Client) -> crate::account::AccountState {
    use dmsg_types::user::{
        AccountInfo, ControllerRole, Device, DeviceInput, SensitivePolicy, VaultWriteState,
    };
    use std::collections::BTreeMap;
    let account = account();
    let device_id = client.workspace.meta.device_id;
    let devices = BTreeMap::from([(
        device_id,
        Device {
            input: DeviceInput {
                device_id,
                signing_pub: client.workspace.meta.signing_pub,
                hpke_pub: client.workspace.meta.hpke_pub,
                role: ControllerRole::Administrator,
                capabilities: vec![
                    Capability::RootManage,
                    Capability::VaultUnlock,
                    Capability::FormalApprove,
                ],
            },
            added_at: 1,
            added_by: None,
            revoked_at: None,
            next_sequence: 0,
        },
    )]);
    crate::account::AccountState {
        security: crate::testing::security(1, devices.clone()),
        info: AccountInfo {
            created_at_ms: 1,
            issuer: account.issuer,
            account_id: account.id,
            home_user: account.home,
            home_cose: account.home,
            auth_bindings: vec![],
            account_version: 1,
            security_epoch: 1,
            devices,
            recovery_delay_ms: 0,
            recovery_enabled: None,
            pending_recovery: None,
            recovered_device: None,
            current_root: None,
            root_slot: None,
            vault_write_state: VaultWriteState::Ready,
            sensitive_policy: SensitivePolicy::default(),
            approval_keys: BTreeMap::new(),
            hardware_policy: vec![],
        },
    }
}

/// Restore the fixed offline fixture's local keys after a lock, without network
/// or platform authentication. Available only with the explicit testing feature.
pub fn unlock_fixture(client: &mut Client) {
    assert_eq!(client.account().unwrap().id, account().id);
    client.keys = Some(client.workspace.unlock(&SECRET).unwrap());
    client.root = Some(RootKeys::single(1, [5; 32]));
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

/// Install a valid test delegation without performing account discovery.
pub async fn login(client: &mut Client, seed: u8) -> crate::login::Delegated {
    let delegated = crate::login::kept_for_tests(seed, crate::now_ms() + dmsg_types::DAY)
        .delegated()
        .unwrap();
    client.login = Some(crate::client::Login {
        ic: crate::ic::Ic::connect(&client.config, delegated.identity.clone())
            .await
            .unwrap(),
        principal: delegated.principal,
        expires_at: delegated.expires_at,
        kept: delegated.kept.clone(),
    });
    delegated
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

/// Two real local versions with different byte formats, for isolated native UI QA.
pub fn file_history(client: &mut Client) -> String {
    let first = client
        .files_write(
            "root",
            "Version review.txt",
            b"Original reviewed text version",
        )
        .unwrap();
    let mut image = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(4, 4)
        .write_to(&mut image, image::ImageFormat::Png)
        .unwrap();
    client
        .files_write("root", "Version review.txt", image.get_ref())
        .unwrap();
    first.file.id
}

/// Synthetic message metadata pointing to a genuinely imported local image.
/// This exercises saved-copy UI without claiming remote delivery/signature proof.
pub fn saved_shared_file(client: &mut Client, channel: &str, seq: u64) -> String {
    let id = file_history(client);
    client.files_rename(&id, "Received image.png").unwrap();
    let file = client.files_show(&id).unwrap().file;
    let mut version = client.version(&id, &file.current).unwrap();
    version.origin = crate::library::catalog::Origin::SharedIn {
        channel: channel.into(),
        seq,
    };
    client.save_version(&id, &file.current, &version).unwrap();
    shared_image(client, channel, seq);
    let key = format!("{}{channel}:{seq:020}", crate::channel::INBOX);
    let mut message: crate::channel::Message = client
        .workspace
        .private(client.keys().unwrap(), &key)
        .unwrap()
        .unwrap();
    message.message_id = crate::upload::random_hex();
    message.digest = "01".repeat(32);
    message.text = "A file already saved on this device".into();
    message.created_at = crate::now_ms();
    let reference = message.file.as_mut().unwrap();
    reference.name = file.name;
    reference.mime = file.mime;
    reference.size = file.size;
    reference.file_id = id.clone();
    reference.version = file.current;
    client
        .workspace
        .put_private(client.keys().unwrap(), &key, &message)
        .unwrap();
    id
}

/// A synthetic pending mailbox entry, for measuring the serialized pairing
/// check before it attempts to interpret the request envelope.
pub fn pending_connect_entry(client: &Client) {
    let id = Hash::new([0xa1; 32]);
    let now = crate::now_ms();
    let entry: crate::connect::Entry = serde_json::from_value(serde_json::json!({
        "seq":1, "request_id":id, "pairing_id":Hash::new([0xb2;32]),
        "kind":dmsg_types::connect::RequestKind::Read, "created_at":now,
        "expires_at":now+300_000, "envelope":[1,2,3], "state":"pending",
        "version":1, "pinned":null, "acked":null,
    }))
    .unwrap();
    client
        .workspace
        .store
        .put_inbox(&hex::encode(*id), 1, &entry, false)
        .unwrap();
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
            recovery_enabled: None,
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

/// Apply a fixture in place of a verified certificate, including its local
/// observation. Tests must not use this as evidence of certificate validation.
pub fn observe_security(client: &mut Client, security: &Security) -> dmsg_types::Result<()> {
    client.observe(&security.snapshot.account_id, security)
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

/// Synthetic membership and certified-name cache for offline UI permission checks.
/// This is not a remote control-log or certificate-verification fixture.
pub fn channel_members(client: &mut Client, channel: &str, role: crate::channel_log::Role) {
    use crate::channel_log::{Member, Role};
    let key = format!("channel:{channel}");
    let mut local: serde_json::Value = client
        .workspace
        .private(client.keys().unwrap(), &key)
        .unwrap()
        .unwrap();
    let mut ledger: crate::channel_log::Ledger =
        serde_json::from_value(local["ledger"].clone()).unwrap();
    let me = account().id.to_string();
    let other = "040g2082040g2082040g";
    let member = "040g2083040g2083040g";
    ledger.members.clear();
    ledger.owner = if role == Role::Owner {
        me.clone()
    } else {
        other.into()
    };
    for (id, role) in [
        (me.as_str(), role),
        (
            other,
            if role == Role::Owner {
                Role::Admin
            } else {
                Role::Owner
            },
        ),
        (member, Role::Member),
    ] {
        ledger.members.insert(
            id.into(),
            Member {
                account: id.into(),
                role,
                can_rotate: matches!(role, Role::Owner | Role::Admin),
                joined_epoch: 1,
                history: vec![],
            },
        );
    }
    local["ledger"] = serde_json::to_value(ledger).unwrap();
    client
        .workspace
        .put_private(client.keys().unwrap(), &key, &local)
        .unwrap();
    let mut names = std::collections::BTreeMap::new();
    for id in [other, member] {
        names.insert(
            id.parse().unwrap(),
            crate::display::Name {
                name: Some("Mina".into()),
                avatar_url: None,
                updated_at: Some(1),
                observed_at_ms: crate::now_ms(),
            },
        );
    }
    client
        .keep_public_names(names.into_iter().collect())
        .unwrap();
}
