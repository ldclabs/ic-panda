//! One device's client: the workspace, the login and, while unlocked, the
//! keys. The daemon owns a single instance; mobile apps will too.
//!
//! Each account has its own workspace, so accounts sharing a device never
//! see each other's data: `accounts/<account>/` holds the workspace (and the
//! agent connectors' state) of an account, bound or waiting for approval,
//! and `logins/<principal>/` that of a login without an account, which
//! becomes `accounts/<account>/` once the login has one. A login selects the
//! workspace of the account it is bound to; `current` names the account
//! selected last, which a restart and fast unlock use.
//!
//! Unlocking needs a login: the user home releases the device's unlock
//! secret only to a Principal bound to the account. Fast unlock opens a
//! second copy of the local data key with a key the operating system keeps,
//! for thirty days after the last login unlock; a login unlock turns it on
//! unless the person turned it off. Where hardware keeps that key and asks
//! for the person (a [`PresenceKey`], such as the Secure Enclave behind Touch
//! ID), the login is also kept, sealed under the local data key, and a fast
//! unlock restores it until it expires. Locking drops the login together
//! with every key; with "background online" (`background_online`, plan D-7)
//! only the device signing key stays, so presigned and already sealed
//! versions keep moving while nothing can be read.

use crate::account::{Security, device_bundle, verify_security};
use crate::cloud::{Cloud, Evidence, EvidenceEntry, Prepared, Signer};
use crate::codec::{MAX_RECORD, canonical, decode};
use crate::config::{Config, environment_name};
use crate::content::VAULT;
use crate::crypto::Key;
use crate::hold::Hold;
use crate::ic::Ic;
use crate::library::blobs::Blobs;
use crate::library::catalog::Catalog;
use crate::login::{Delegated, KeptLogin};
use crate::now_ms;
use crate::root::RootKeys;
use crate::secure::{Presence, PresenceKey, SecureStorage};
use crate::storage;
use crate::store::LOCAL;
use crate::sync::LastSync;
use crate::workspace::{AccountRef, Keys, Workspace};
use candid::Principal;
use dmsg_types::{
    AccountId, CertifiedBatch, DAY, Environment, Error, Hash, Result, ensure, user::Device,
};
use futures_util::future::try_join_all;
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, ByteBuf};
use serde_json::Value as Json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use zeroize::Zeroizing;

/// Fast unlock is refused this long after the last login unlock, as long as
/// the login lasts.
pub const FAST_UNLOCK_MS: u64 = 30 * DAY;
/// The login kept sealed under the LDK.
const KEPT_LOGIN: &str = "login";
/// The agent connectors' state file in the workspace directory.
const AGENT_RUNTIME: &str = "agents.sqlite";
/// The workspace file in its directory.
const WORKSPACE: &str = "workspace.sqlite";
/// The file library's chunk directory in the workspace directory.
const BLOBS: &str = "blobs";
/// The account selected last.
const CURRENT: &str = "current";

/// The workspace a client uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    /// None yet: an empty workspace in memory, until a login selects one.
    Idle,
    /// An account's workspace, bound or waiting for approval.
    Account(AccountId),
    /// The workspace of a login that has no account.
    Login(Principal),
}

/// An authenticated login: an agent with the delegated session identity.
pub(crate) struct Login {
    pub(crate) ic: Ic,
    pub(crate) principal: Principal,
    pub(crate) expires_at: u64,
    /// What restores it after a fast unlock.
    kept: KeptLogin,
}

/// What the credential store keeps for fast unlock.
#[derive(Serialize, Deserialize)]
enum FastKey {
    /// The key itself.
    Plain(ByteArray<32>),
    /// A [`PresenceKey`]'s handle and peer, which recover the key's secret
    /// with the person present.
    Present { handle: ByteBuf, peer: ByteBuf },
}

/// The fast-unlock key of a workspace from a [`PresenceKey`]'s secret.
fn presence_key(secret: &Key, name: &str) -> Zeroizing<Key> {
    crate::crypto::derive(secret, &("dmsg/fast-unlock/1", name))
}

/// What a client can do right now; nothing here is secret.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Status {
    /// Lowercase environment name.
    pub environment: String,
    /// This device's ID in the selected workspace; none before a login
    /// selects one.
    pub device_id: Option<String>,
    /// The account, once the device belongs to one.
    pub account: Option<String>,
    /// The account's user home.
    pub home: Option<String>,
    /// The login Principal, while logged in.
    pub principal: Option<String>,
    /// When the login's delegation expires, in Unix milliseconds.
    pub login_expires_at: Option<u64>,
    /// `logged_out`, `locked`, `no_account`, `pairing_required`,
    /// `root_required` or `unlocked`.
    pub state: &'static str,
    /// The content-root generation in use.
    pub root_generation: Option<u64>,
    /// Local versions the relay has not accepted yet.
    pub pending_sync: u64,
    /// Conflict copies waiting for a decision.
    pub conflicts: u64,
    /// The configured background-online policy.
    pub background_online: bool,
    /// The device key stays while locked.
    pub resident: bool,
    /// Fast unlock is enabled.
    pub fast_unlock: bool,
    /// How fast unlock checks the person, when hardware keeps its key.
    pub fast_presence: Option<Presence>,
    /// The latest sync attempt.
    pub last_sync: Option<LastSync>,
    /// Approval requests waiting for a decision; counted while locked too.
    pub approvals_waiting: u64,
    /// Web requests fetched but not yet opened, as while locked.
    pub sealed_requests: u64,
    /// Unread channel messages, muted and hidden channels aside; while
    /// unlocked.
    pub channels_unread: u64,
}

/// The session fields of [`Status`], read without touching storage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionState {
    /// As [`Status::state`].
    pub state: &'static str,
    /// As [`Status::account`].
    pub account: Option<String>,
    /// As [`Status::resident`].
    pub resident: bool,
}

impl SessionState {
    /// The account while unlocked.
    pub fn unlocked_account(&self) -> Option<&str> {
        self.account.as_deref().filter(|_| self.state == "unlocked")
    }
}

/// An account workspace on this device.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct LocalAccount {
    /// Account ID.
    pub account: String,
    /// This device's ID in the account.
    pub device_id: String,
    /// The device belongs to the account; otherwise it waits for approval
    /// or recovery.
    pub bound: bool,
    /// Fast unlock is enabled.
    pub fast_unlock: bool,
    /// The latest unlock through a login, in Unix milliseconds.
    pub login_unlocked_at: Option<u64>,
    /// The selected workspace.
    pub current: bool,
}

/// Work that started and has not settled. All of it is journaled and resumes
/// after a restart; an update waits for it rather than leaving it to
/// reconciliation.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct InFlight {
    /// Approved requests whose outcome is not known yet.
    pub approvals: u64,
    /// An account update whose outcome is not known yet; read while unlocked.
    pub account_update: bool,
    /// A root rotation that has not committed.
    pub root_rotation: bool,
}

impl InFlight {
    /// Whether anything is in flight.
    pub fn any(&self) -> bool {
        self.approvals > 0 || self.account_update || self.root_rotation
    }
}

/// A device's client.
pub struct Client {
    pub(crate) config: Config,
    /// The environment's directory.
    dir: PathBuf,
    secure: Option<Arc<dyn SecureStorage>>,
    /// Hardware that keeps the fast-unlock key and asks for the person.
    presence: Option<Arc<dyn PresenceKey>>,
    slot: Slot,
    pub(crate) workspace: Workspace,
    pub(crate) cloud: Cloud,
    pub(crate) login: Option<Login>,
    pub(crate) anonymous: Option<Ic>,
    pub(crate) keys: Option<Keys>,
    resident: Option<Zeroizing<Key>>,
    pub(crate) root: Option<RootKeys>,
    pub(crate) security: Option<Security>,
    /// When the security the relay holds was certified.
    pub(crate) published: Option<u64>,
    pub(crate) located: Option<(Principal, AccountId)>,
    /// The deployment's user homes, read once from the handle registry.
    pub(crate) homes: Option<Vec<Principal>>,
    /// The user home of each other account read so far.
    peers: BTreeMap<AccountId, Principal>,
    /// The payer loaded from a PEM file, until the device locks.
    pub(crate) payer: Option<(Principal, Ic)>,
    /// This device's team-wallet key and agent, until the device locks.
    pub(crate) wallet_ic: Option<(Principal, Ic)>,
    /// The team wallets' chain endpoints, once read.
    pub(crate) chains: Option<crate::wallet::rpc::Chains>,
    /// The platform keystore of cosigner keys.
    pub(crate) cosign: Option<Arc<dyn crate::wallet::cosign::CosignProvider>>,
    /// A second login waiting for its binding to this account.
    pub(crate) binding: Option<crate::security::PendingBinding>,
    /// The file library's chunk directory of the selected workspace.
    pub(crate) blobs: Option<Blobs>,
    /// The library's catalog, read on first use while unlocked.
    pub(crate) library: Option<Catalog>,
    /// When the library's maintenance last ran.
    pub(crate) library_maintained_at: Option<u64>,
    /// When the library's unreferenced chunks were last collected.
    pub(crate) library_collected_at: Option<u64>,
    /// Set from outside the client to stop the imports under way; checked
    /// only while one is given.
    pub(crate) import_cancel: Option<Arc<AtomicBool>>,
}

/// The network a client's relay requests wait on, taken under the client by
/// [`Client::warmup`] and run without it; [`Client::warmed`] keeps what it
/// brings back. It holds public state and agents, never a key.
pub struct Warmup {
    account: AccountId,
    home: Principal,
    device: Hash,
    config: Config,
    cloud: Cloud,
    /// The login's agent or the anonymous one; `None` before there is one.
    ic: Option<Ic>,
    /// What the security needs: nothing, the home's answer, or fresh state
    /// handed to the relay, starting from the state at hand, if any.
    security: Option<Warm>,
    state: Option<Security>,
    /// Whether the user homes are still to be read.
    homes: bool,
    /// Whether a public agent is still to be made.
    reader: bool,
}

/// What a [`Client::warmup`] brings the certified security to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Warm {
    /// Fresh evidence the relay holds, for signed relay requests.
    Relay,
    /// The user home's answer now, for display; the relay is not involved.
    Home,
}

/// What a [`Warmup`] brought back.
pub struct Warmed {
    account: AccountId,
    /// What the user home answered.
    fetched: Option<Security>,
    /// The state handed to the relay, and whether the relay took it.
    handed: Option<(Security, Result<()>)>,
    homes: Option<Vec<Principal>>,
    /// The anonymous agent made on the way.
    anonymous: Option<Ic>,
}

impl Warmup {
    /// Ask the user home, and for [`Warm::Relay`] hand the state to the
    /// relay, which asks for a newer one when it holds newer; read the user
    /// homes. A failed read of the homes leaves them to whoever needs them
    /// next.
    ///
    /// # Errors
    /// Errors of the user home's queries; the relay's refusal is kept in
    /// [`Warmed`] for [`Client::warmed`].
    pub async fn run(self) -> Result<Warmed> {
        let mut warmed = Warmed {
            account: self.account,
            fetched: None,
            handed: None,
            homes: None,
            anonymous: None,
        };
        let mut ic = self.ic;
        if self.homes || self.reader {
            let ic = agent(&mut ic, &self.config, &mut warmed.anonymous).await?;
            if self.homes {
                let read =
                    crate::account::user_homes(&ic, self.config.handle, &self.config.user_homes)
                        .await;
                warmed.homes = read.ok().map(|config| config.user_homes);
            }
        }
        match self.security {
            None => return Ok(warmed),
            Some(Warm::Home) => {
                let ic = agent(&mut ic, &self.config, &mut warmed.anonymous).await?;
                let state = crate::account::fetch_security(&ic, self.home, &self.account).await?;
                warmed.fetched = Some(state);
                return Ok(warmed);
            }
            Some(Warm::Relay) => {}
        }
        let mut cached = self.state;
        let mut attempts = 0;
        loop {
            let state = match cached.take() {
                Some(state) => state,
                None => {
                    let ic = agent(&mut ic, &self.config, &mut warmed.anonymous).await?;
                    let state =
                        crate::account::fetch_security(&ic, self.home, &self.account).await?;
                    warmed.fetched = Some(state.clone());
                    state
                }
            };
            // The relay refuses a revoked device, which goes before asking.
            if state
                .device(&self.device)
                .is_some_and(|d| d.revoked_at.is_some())
            {
                warmed.handed = Some((state, Err(Error::DeviceNotApproved)));
                return Ok(warmed);
            }
            match self.cloud.publish_security(&state.evidence).await {
                // Replicas may answer with an older, still valid certificate.
                Err(Error::PolicyStale) if attempts < 2 => attempts += 1,
                handed => {
                    warmed.handed = Some((state, handed));
                    return Ok(warmed);
                }
            }
        }
    }
}

/// The agent of a [`Warmup`], an anonymous one made the first time it is
/// needed when there is none; `made` keeps that one for the client.
async fn agent(ic: &mut Option<Ic>, config: &Config, made: &mut Option<Ic>) -> Result<Ic> {
    if let Some(ic) = ic {
        return Ok(ic.clone());
    }
    let anonymous = Ic::anonymous(config).await?;
    *made = Some(anonymous.clone());
    *ic = Some(anonymous.clone());
    Ok(anonymous)
}

/// Other accounts' certified security, read without holding the client:
/// [`Client::peers_of`] takes the public agent, the user homes and the home
/// each account was met at, [`Peers::read`] asks the homes, and
/// [`Client::peers_met`] keeps what it learned. It holds no key.
pub struct Peers {
    accounts: Vec<AccountId>,
    config: Config,
    /// The login's agent or the anonymous one; `None` before there is one.
    ic: Option<Ic>,
    /// The anonymous agent made on the way.
    anonymous: Option<Ic>,
    homes: Option<Vec<Principal>>,
    /// The user home each account was met at.
    met: BTreeMap<AccountId, Principal>,
}

impl Peers {
    /// The accounts' certified security from their user homes, with the
    /// public queries: their device maps side by side, from the home each
    /// was met at or else the first that knows it, then one certified batch
    /// per home.
    ///
    /// # Errors
    /// Returns `Error::NotFound` when no user home knows an account.
    pub(crate) async fn read(&mut self) -> Result<Vec<(AccountId, Security)>> {
        let ic = agent(&mut self.ic, &self.config, &mut self.anonymous).await?;
        let homes = match &self.homes {
            Some(homes) => homes.clone(),
            None => {
                let config =
                    crate::account::user_homes(&ic, self.config.handle, &self.config.user_homes)
                        .await?;
                self.homes = Some(config.user_homes.clone());
                config.user_homes
            }
        };
        let lookups = self.accounts.iter().map(|account| {
            let candidates = self
                .met
                .get(account)
                .map_or_else(|| homes.clone(), |home| vec![*home]);
            let ic = &ic;
            async move {
                for home in candidates {
                    if let Ok((_, devices)) = device_bundle(ic, home, account).await? {
                        return Ok((home, devices));
                    }
                }
                Err(Error::NotFound)
            }
        });
        let mut by_home: BTreeMap<Principal, Vec<Peer>> = BTreeMap::new();
        for (account, (home, devices)) in self.accounts.iter().zip(try_join_all(lookups).await?) {
            by_home.entry(home).or_default().push((*account, devices));
        }
        let groups: Vec<(Principal, Vec<Peer>)> = by_home
            .into_iter()
            .flat_map(|(home, peers)| {
                peers
                    .chunks(dmsg_types::MAX_BATCH)
                    .map(|chunk| (home, chunk.to_vec()))
                    .collect::<Vec<_>>()
            })
            .collect();
        let batches = try_join_all(groups.iter().map(|(home, peers)| {
            let ids: Vec<AccountId> = peers.iter().map(|(account, _)| *account).collect();
            let ic = &ic;
            async move {
                let batch: Result<CertifiedBatch> =
                    ic.query(*home, "security_snapshot_batch", (ids,)).await?;
                batch
            }
        }))
        .await?;
        let (root_key, now) = (ic.root_key(), now_ms());
        let mut found = Vec::with_capacity(self.accounts.len());
        for ((home, peers), batch) in groups.into_iter().zip(batches) {
            for (account, devices) in peers {
                let entry = batch
                    .entries
                    .iter()
                    .find(|e| e.key.as_slice() == account.as_slice())
                    .ok_or(Error::IntegrityFailed)?;
                let single = CertifiedBatch {
                    schema: batch.schema,
                    canister: batch.canister,
                    certificate: batch.certificate.clone(),
                    entries: vec![entry.clone()],
                };
                let security = verify_security(home, &account, &single, devices, &root_key, now)?;
                self.met.insert(account, home);
                found.push((account, security));
            }
        }
        Ok(found)
    }
}

/// Whether `state` is the account's and has more than ten seconds left.
fn fresh(state: &Security, account: &AccountId) -> bool {
    state.snapshot.account_id == *account && state.expires_at() > now_ms() + 10_000
}

impl Client {
    /// Open the client of `config` with its workspaces under `dir`, on the
    /// account selected last. `secure` is the operating system's credential
    /// store; without it fast unlock is unavailable.
    ///
    /// # Errors
    /// An invalid configuration returns `Error::InvalidInput`; storage
    /// failures return `Error::Unavailable`.
    pub fn open(
        config: Config,
        dir: &Path,
        secure: Option<Arc<dyn SecureStorage>>,
    ) -> Result<Self> {
        config.validate()?;
        std::fs::create_dir_all(dir).map_err(storage)?;
        let cloud = Cloud::new(
            &config.relay_origin(),
            config.environment == Environment::Local,
        )?;
        let workspace = idle(&config.environment)?;
        let mut client = Self {
            config,
            dir: dir.to_path_buf(),
            secure,
            presence: None,
            slot: Slot::Idle,
            workspace,
            cloud,
            login: None,
            anonymous: None,
            keys: None,
            resident: None,
            root: None,
            security: None,
            published: None,
            located: None,
            homes: None,
            peers: BTreeMap::new(),
            payer: None,
            wallet_ic: None,
            chains: None,
            cosign: None,
            binding: None,
            blobs: None,
            library: None,
            library_maintained_at: None,
            library_collected_at: None,
            import_cancel: None,
        };
        let current = std::fs::read_to_string(dir.join(CURRENT))
            .ok()
            .and_then(|text| text.trim().parse::<AccountId>().ok());
        if let Some(account) = current
            && client
                .slot_dir(&Slot::Account(account))
                .join(WORKSPACE)
                .exists()
        {
            client.select(Slot::Account(account))?;
        }
        Ok(client)
    }

    /// Keep the fast-unlock key in `key`, which asks for the person; fast
    /// unlock turned on from now on uses it.
    pub fn set_presence_key(&mut self, key: Arc<dyn PresenceKey>) {
        self.presence = Some(key);
    }

    /// The directory of a workspace.
    fn slot_dir(&self, slot: &Slot) -> PathBuf {
        match slot {
            Slot::Idle => self.dir.clone(),
            Slot::Account(account) => self.dir.join("accounts").join(account.to_string()),
            Slot::Login(principal) => self.dir.join("logins").join(principal.to_text()),
        }
    }

    /// Use the workspace of `slot`, creating a new device in it on first
    /// use; the keys of the previous one go, the login stays. An account
    /// without a workspace here takes over the one its login used before
    /// it had the account.
    pub(crate) fn select(&mut self, slot: Slot) -> Result<()> {
        if slot == self.slot {
            return Ok(());
        }
        self.library_lock();
        self.keys = None;
        self.resident = None;
        self.root = None;
        self.security = None;
        self.published = None;
        self.located = None;
        self.payer = None;
        self.wallet_ic = None;
        self.binding = None;
        // Close the database before its directory moves or another opens.
        self.workspace = idle(&self.config.environment)?;
        self.slot = Slot::Idle;
        let dir = self.slot_dir(&slot);
        if let (Slot::Account(account), Some(login)) = (&slot, &self.login) {
            let from = self.slot_dir(&Slot::Login(login.principal));
            if !dir.join(WORKSPACE).exists() && from.join(WORKSPACE).exists() {
                self.move_workspace(&from, account)?;
            }
        }
        std::fs::create_dir_all(&dir).map_err(storage)?;
        self.workspace = Workspace::open(&dir.join(WORKSPACE), &self.config.environment, now_ms())?;
        if let Slot::Account(account) = &slot {
            std::fs::write(self.dir.join(CURRENT), account.to_string()).map_err(storage)?;
        }
        self.blobs = (slot != Slot::Idle).then(|| Blobs::new(dir.join(BLOBS)));
        self.slot = slot;
        Ok(())
    }

    /// Move a login's workspace to the account it now belongs to.
    fn move_workspace(&self, from: &Path, account: &AccountId) -> Result<()> {
        let to = self.slot_dir(&Slot::Account(*account));
        std::fs::create_dir_all(self.dir.join("accounts")).map_err(storage)?;
        // A workspace left from before, without its database, gives way.
        if to.exists() {
            std::fs::remove_dir_all(&to).map_err(storage)?;
        }
        std::fs::rename(from, to).map_err(storage)
    }

    /// Move the selected login workspace, now bound to `account`, to the
    /// account's directory, keeping the keys and the login.
    pub(crate) fn adopt(&mut self, account: &AccountId) -> Result<()> {
        let Slot::Login(principal) = self.slot.clone() else {
            return Ok(());
        };
        let keys = self.keys.take();
        self.workspace = idle(&self.config.environment)?;
        self.slot = Slot::Idle;
        let target = self.slot_dir(&Slot::Account(*account));
        ensure(!target.join(WORKSPACE).exists(), Error::IntegrityFailed)?;
        self.move_workspace(&self.slot_dir(&Slot::Login(principal)), account)?;
        self.workspace =
            Workspace::open(&target.join(WORKSPACE), &self.config.environment, now_ms())?;
        std::fs::write(self.dir.join(CURRENT), account.to_string()).map_err(storage)?;
        self.blobs = Some(Blobs::new(target.join(BLOBS)));
        self.slot = Slot::Account(*account);
        self.keys = keys;
        Ok(())
    }

    /// The account workspaces on this device.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn accounts(&self) -> Result<Vec<LocalAccount>> {
        let mut list = Vec::new();
        let entries = match std::fs::read_dir(self.dir.join("accounts")) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(list),
            Err(e) => return Err(storage(e)),
        };
        for entry in entries {
            let entry = entry.map_err(storage)?;
            let Ok(account) = entry.file_name().to_string_lossy().parse::<AccountId>() else {
                continue;
            };
            let current = self.slot == Slot::Account(account);
            let meta = if current {
                Some(self.workspace.meta.clone())
            } else if entry.path().join(WORKSPACE).exists() {
                crate::store::Store::open(&entry.path().join(WORKSPACE))?
                    .meta::<crate::workspace::Meta>(crate::workspace::META)?
            } else {
                None
            };
            let Some(meta) = meta else {
                continue;
            };
            list.push(LocalAccount {
                account: account.to_string(),
                device_id: hex::encode(*meta.device_id),
                bound: meta.account.is_some_and(|a| a.id == account),
                fast_unlock: meta.fast_unlock.is_some(),
                login_unlocked_at: meta.login_unlocked_at,
                current,
            });
        }
        list.sort_by_key(|a| std::cmp::Reverse(a.login_unlocked_at));
        Ok(list)
    }

    /// Select another account's workspace on this device, locked; it then
    /// unlocks with its own login or fast unlock.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an account without a workspace here.
    pub fn switch_account(&mut self, account: &AccountId) -> Result<Status> {
        let slot = Slot::Account(*account);
        ensure(
            self.slot_dir(&slot).join(WORKSPACE).exists(),
            Error::NotFound,
        )?;
        if self.slot != slot {
            self.lock();
            self.select(slot)?;
        }
        self.status()
    }

    /// The deployment parameters.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Where the agent connectors of the selected workspace keep their
    /// state; the daemon's agent host owns the file, a reset removes it.
    ///
    /// # Errors
    /// Returns `Error::AuthRequired` before a login selects a workspace.
    pub fn agent_runtime(&self) -> Result<PathBuf> {
        ensure(self.slot != Slot::Idle, Error::AuthRequired)?;
        Ok(self.slot_dir(&self.slot).join(AGENT_RUNTIME))
    }

    /// The current status.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn status(&self) -> Result<Status> {
        let meta = &self.workspace.meta;
        let login = self.login.as_ref().filter(|l| l.expires_at > now_ms());
        let session = self.session_state();
        Ok(Status {
            environment: environment_name(&self.config.environment).into(),
            device_id: (self.slot != Slot::Idle).then(|| hex::encode(*meta.device_id)),
            account: session.account,
            home: meta.account.as_ref().map(|a| a.home.to_text()),
            principal: login.map(|l| l.principal.to_text()),
            login_expires_at: login.map(|l| l.expires_at),
            state: session.state,
            root_generation: self.root.as_ref().map(|r| r.generation),
            pending_sync: self.workspace.store.count_in(LOCAL)?,
            conflicts: self.workspace.store.count_conflicts(VAULT)?,
            background_online: self.config.background_online,
            resident: session.resident,
            fast_unlock: meta.fast_unlock.is_some(),
            fast_presence: self
                .presence
                .as_ref()
                .filter(|_| meta.fast_unlock.is_some() && meta.fast_presence)
                .and_then(|key| key.presence()),
            last_sync: self.last_sync()?,
            approvals_waiting: self.approvals_waiting()?,
            sealed_requests: self.sealed_requests()?,
            channels_unread: match session.state {
                "unlocked" => self.channels_unread().unwrap_or(0),
                _ => 0,
            },
        })
    }

    /// The lock state, the account and residency, for checks that run
    /// often; [`Self::status`] also reads the counts from storage.
    pub fn session_state(&self) -> SessionState {
        let account = self.workspace.meta.account.as_ref();
        let login = self.login.as_ref().filter(|l| l.expires_at > now_ms());
        let state = match (account, login, &self.keys) {
            (None, None, _) => "logged_out",
            (None, Some(_), _) if self.located.is_some() => "pairing_required",
            (None, Some(_), _) => "no_account",
            (Some(_), _, None) => "locked",
            (Some(_), _, Some(_)) if self.root.is_none() => "root_required",
            (Some(_), _, Some(_)) => "unlocked",
        };
        SessionState {
            state,
            account: account.map(|a| a.id.to_string()),
            resident: self.keys.is_none() && self.resident.is_some(),
        }
    }

    /// What has started and not settled.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn in_flight(&self) -> Result<InFlight> {
        Ok(InFlight {
            approvals: self.approvals_unsettled()?,
            account_update: self.keys.is_some() && self.pending()?.is_some(),
            root_rotation: self
                .workspace
                .store
                .sealed(crate::root::ROOT_JOB)?
                .is_some(),
        })
    }

    /// Accept a login: find its account across every user home with the
    /// read-only `my_account`, select that account's workspace, or the
    /// login's own while it has none, then unlock. A failed check discards
    /// the login.
    ///
    /// # Errors
    /// Query failures discard the login; a workspace bound to another
    /// account or home returns `Error::IntegrityFailed`.
    pub async fn login(&mut self, login: Delegated) -> Result<Status> {
        ensure(login.expires_at > now_ms(), Error::Expired)?;
        let ic = Ic::connect(&self.config, login.identity).await?;
        let principal = ic.principal()?;
        self.lock();
        self.login = Some(Login {
            ic,
            principal,
            expires_at: login.expires_at,
            kept: login.kept,
        });
        self.enter().await
    }

    /// Select the workspace of the login's account, or the login's own
    /// while it has none, and unlock it; any failure discards the login.
    pub(crate) async fn enter(&mut self) -> Result<Status> {
        let principal = self.session()?.principal;
        let checked = match self.locate().await {
            Ok(found) => {
                let slot = match found {
                    Some((_, account)) => Slot::Account(account),
                    None => Slot::Login(principal),
                };
                self.select(slot)
                    .and_then(|_| match &self.workspace.meta.account {
                        Some(account) => ensure(
                            found == Some((account.home, account.id)),
                            Error::IntegrityFailed,
                        ),
                        None => Ok(()),
                    })
            }
            Err(error) => Err(error),
        };
        if let Err(error) = checked {
            self.lock();
            return Err(error);
        }
        self.unlock().await
    }

    /// Unlock with the current login: a bound workspace through its unlock
    /// secret, a provisional one with its provisional key. A provisional
    /// device the account has approved is bound on the way. A bound device
    /// then settles its journal, opens the root and syncs once (R13).
    ///
    /// # Errors
    /// Returns `Error::AuthRequired` without a valid login and
    /// `Error::DeviceNotApproved` for a revoked device, whose workspace is
    /// then erased.
    pub async fn unlock(&mut self) -> Result<Status> {
        self.session()?;
        if self.workspace.provisional() {
            self.keys = Some(self.workspace.unlock_provisional()?);
            self.located = self.locate().await?;
            if let Some((home, account)) = self.located {
                let state = self.refresh(home, &account).await?;
                if state.active(&self.workspace.meta.device_id).is_ok() {
                    self.join(home, account).await?;
                }
            }
        } else {
            let account = self.account()?.clone();
            let security = self.refresh_security(account.home, &account.id).await?;
            self.check_revoked(&security)?;
            let secret = self.unlock_secret(account.home, &account.id).await?;
            self.keys = Some(self.workspace.unlock(&secret)?);
            self.workspace.meta.login_unlocked_at = Some(now_ms());
            self.workspace.save_meta()?;
            self.after_login_unlock()?;
        }
        if !self.workspace.provisional() {
            if let Err(error) = self.resume().await
                && error != Error::Expired
            {
                return Err(error);
            }
            self.open_root().await?;
        }
        self.status()
    }

    /// Unlock without a login, with the fast-unlock key the operating
    /// system keeps; hardware that keeps it asks for the person first, and
    /// the login kept at the last login unlock comes back while it lasts.
    /// Online, the device then checks it is still approved.
    ///
    /// # Errors
    /// Returns `Error::NotFound` when fast unlock is off, `Error::Forbidden`
    /// when the person declines, `Error::AuthRequired` thirty days after the
    /// last login unlock or when the key is gone, and
    /// `Error::DeviceNotApproved` for a revoked device, whose workspace is
    /// then erased.
    pub async fn unlock_fast(&mut self) -> Result<Status> {
        let account = self.account()?.clone();
        ensure(self.workspace.meta.fast_unlock.is_some(), Error::NotFound)?;
        let unlocked = self.workspace.meta.login_unlocked_at.unwrap_or(0);
        ensure(now_ms() < unlocked + FAST_UNLOCK_MS, Error::AuthRequired)?;
        let key = self.fast_key()?;
        let keys = self.workspace.unlock_fast(&key)?;
        self.lock();
        self.keys = Some(keys);
        match self.refresh_security(account.home, &account.id).await {
            Ok(security) => self.check_revoked(&security)?,
            // Offline: the check runs with the next sync.
            Err(Error::Unavailable(_)) => {}
            Err(error) => {
                self.lock();
                return Err(error);
            }
        }
        self.open_root().await?;
        if self.workspace.meta.fast_presence {
            // Without its login the account still opens; the next login
            // unlock keeps a new one.
            let _ = self.restore_login().await;
        }
        self.status()
    }

    /// The fast-unlock key from the credential store, through the hardware
    /// that keeps it with the person present. A key that is gone, or that
    /// the hardware no longer opens, turns fast unlock off and asks for a
    /// login.
    fn fast_key(&mut self) -> Result<Zeroizing<Key>> {
        let secure = self.secure.clone().ok_or(Error::NotFound)?;
        let name = self.fast_name()?;
        let stored = secure
            .get(&name)?
            .and_then(|bytes| decode::<FastKey>(&bytes, MAX_RECORD).ok());
        let key = match (stored, &self.presence) {
            (Some(FastKey::Plain(key)), _) => Ok(Zeroizing::new(*key)),
            (Some(FastKey::Present { handle, peer }), Some(hardware)) => hardware
                .open(&handle, &peer)
                .map(|secret| presence_key(&secret, &name)),
            _ => Err(Error::IntegrityFailed),
        };
        if matches!(key, Err(Error::IntegrityFailed)) {
            self.workspace.disable_fast()?;
            return Err(Error::AuthRequired);
        }
        key
    }

    /// After an unlock through a login: turn fast unlock on unless the
    /// person turned it off, and keep the login for the next fast unlock.
    pub(crate) fn after_login_unlock(&mut self) -> Result<()> {
        let meta = &self.workspace.meta;
        if meta.fast_unlock.is_none() && !meta.fast_declined && self.secure.is_some() {
            // A keystore that refuses leaves fast unlock off; the unlock
            // stands.
            if self.enable_fast_unlock().is_ok() {
                return Ok(());
            }
        }
        self.keep_login()
    }

    /// Seal the current login under the LDK while fast unlock asks for the
    /// person; without that, no login stays on disk.
    fn keep_login(&self) -> Result<()> {
        if !self.workspace.meta.fast_presence {
            return self.workspace.delete_private(KEPT_LOGIN);
        }
        match &self.login {
            Some(login) => self
                .workspace
                .put_private(self.keys()?, KEPT_LOGIN, &login.kept),
            None => Ok(()),
        }
    }

    /// Restore the kept login while it lasts; an expired one is deleted.
    async fn restore_login(&mut self) -> Result<()> {
        let Some(kept) = self
            .workspace
            .private::<KeptLogin>(self.keys()?, KEPT_LOGIN)?
        else {
            return Ok(());
        };
        if kept.expires_at <= now_ms() {
            return self.workspace.delete_private(KEPT_LOGIN);
        }
        let login = kept.delegated()?;
        let ic = Ic::connect(&self.config, login.identity).await?;
        self.login = Some(Login {
            ic,
            principal: login.principal,
            expires_at: login.expires_at,
            kept: login.kept,
        });
        Ok(())
    }

    /// Load the root, fetching a newer committed one, then sync once.
    async fn open_root(&mut self) -> Result<()> {
        match self.load_root().await {
            // Approved, but no root is wrapped to this device yet.
            Err(Error::DeviceNotApproved) => {}
            // Offline after a fast unlock: the stored root still reads.
            Err(Error::Unavailable(_)) if self.root.is_some() && self.login.is_none() => {}
            result => result?,
        }
        if self.root.is_some() {
            // The outcome is recorded in the status; it never fails the unlock.
            let _ = self.sync().await;
        }
        Ok(())
    }

    fn fast_name(&self) -> Result<String> {
        Ok(format!("{}:fast-unlock", self.workspace.name()?))
    }

    /// Enable fast unlock on this device: a key in the operating system's
    /// credential store seals a copy of the local data key. With a
    /// [`PresenceKey`] the store keeps only what the hardware needs to
    /// recover the key with the person present, and the current login is
    /// kept too.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and `Error::NotFound` without a
    /// credential store.
    pub fn enable_fast_unlock(&mut self) -> Result<Status> {
        let secure = self.secure.clone().ok_or(Error::NotFound)?;
        self.keys()?;
        let name = self.fast_name()?;
        let hardware = self.presence.clone().filter(|key| key.presence().is_some());
        let (stored, key) = match hardware {
            Some(hardware) => {
                let (handle, peer, secret) = hardware.create()?;
                let stored = FastKey::Present {
                    handle: handle.into(),
                    peer: peer.into(),
                };
                (stored, presence_key(&secret, &name))
            }
            None => {
                let key = Zeroizing::new(crate::crypto::random_key());
                (FastKey::Plain(ByteArray::new(*key)), key)
            }
        };
        let presence = matches!(stored, FastKey::Present { .. });
        secure.set(&name, &Zeroizing::new(canonical(&stored)))?;
        let keys = self.keys.take().ok_or(Error::Locked)?;
        let enabled = self.workspace.enable_fast(&keys, &key, now_ms(), presence);
        self.keys = Some(keys);
        enabled?;
        self.keep_login()?;
        self.status()
    }

    /// Disable fast unlock, delete its key and the kept login; a later login
    /// unlock leaves it off.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn disable_fast_unlock(&mut self) -> Result<Status> {
        if let (Some(secure), Ok(name)) = (&self.secure, self.fast_name()) {
            secure.delete(&name)?;
        }
        self.workspace.disable_fast()?;
        self.workspace.delete_private(KEPT_LOGIN)?;
        self.workspace.meta.fast_declined = true;
        self.workspace.save_meta()?;
        self.status()
    }

    /// Lock: drop the login and every key. With background online the
    /// device signing key stays, so presigned and sealed versions still go
    /// out; nothing can be read or decided.
    pub fn lock(&mut self) {
        self.library_lock();
        let signing = self.keys.as_ref().map(|k| k.signing.clone());
        if self.config.background_online {
            self.resident = signing.or(self.resident.take());
        } else {
            self.resident = None;
        }
        self.login = None;
        self.keys = None;
        self.root = None;
        self.security = None;
        self.published = None;
        self.located = None;
        self.payer = None;
        self.wallet_ic = None;
        self.binding = None;
    }

    /// Drop everything, the resident key included, as the daemon exits.
    pub fn shutdown(&mut self) {
        self.lock();
        self.resident = None;
    }

    /// Erase the selected workspace and its fast-unlock key and start over
    /// as a new device in its place. Versions the relay has not accepted
    /// are lost; other accounts' workspaces stay.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when files cannot be removed.
    pub fn reset(&mut self) -> Result<Status> {
        if let (Some(secure), Ok(name)) = (&self.secure, self.fast_name()) {
            secure.delete(&name)?;
            // Resident Agent ID copies go with the workspace.
            for agent in self.agents()?.list {
                self.forget_resident(secure.as_ref(), &agent.agent_id)?;
            }
        }
        self.shutdown();
        let slot = std::mem::replace(&mut self.slot, Slot::Idle);
        // Close the database before removing its files.
        self.workspace = idle(&self.config.environment)?;
        if slot == Slot::Idle {
            return self.status();
        }
        let dir = self.slot_dir(&slot);
        for name in [
            WORKSPACE,
            "workspace.sqlite-wal",
            "workspace.sqlite-shm",
            AGENT_RUNTIME,
            "agents.sqlite-wal",
            "agents.sqlite-shm",
        ] {
            match std::fs::remove_file(dir.join(name)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(storage(e)),
                _ => {}
            }
        }
        // The file library's chunks, opened copies and mount uploads go with
        // the workspace.
        for name in [
            BLOBS,
            crate::library::open::OPEN_DIR,
            crate::library::mount::MOUNT_DIR,
        ] {
            match std::fs::remove_dir_all(dir.join(name)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(storage(e)),
                _ => {}
            }
        }
        self.select(slot)?;
        self.status()
    }

    /// Erase the workspace of a device its account revoked.
    pub(crate) fn check_revoked(&mut self, security: &Security) -> Result<()> {
        let revoked = security
            .device(&self.workspace.meta.device_id)
            .is_some_and(|d| d.revoked_at.is_some());
        if revoked && !self.workspace.provisional() {
            self.reset()?;
            return Err(Error::DeviceNotApproved);
        }
        Ok(())
    }

    pub(crate) fn session(&self) -> Result<&Login> {
        self.login
            .as_ref()
            .filter(|l| l.expires_at > now_ms())
            .ok_or(Error::AuthRequired)
    }

    pub(crate) fn secure(&self) -> Option<Arc<dyn SecureStorage>> {
        self.secure.clone()
    }

    pub(crate) fn keys(&self) -> Result<&Keys> {
        self.keys.as_ref().ok_or(Error::Locked)
    }

    /// The device signing key: unlocked, or resident while locked.
    pub(crate) fn signing(&self) -> Result<&Key> {
        match (&self.keys, &self.resident) {
            (Some(keys), _) => Ok(&keys.signing),
            (None, Some(resident)) => Ok(resident),
            (None, None) => Err(Error::Locked),
        }
    }

    pub(crate) fn account(&self) -> Result<&AccountRef> {
        self.workspace
            .meta
            .account
            .as_ref()
            .ok_or(Error::RecoveryIncomplete)
    }

    /// An agent for public queries: the login's, else an anonymous one.
    pub(crate) async fn reader(&mut self) -> Result<Ic> {
        if let Ok(login) = self.session() {
            return Ok(login.ic.clone());
        }
        self.anonymous().await
    }

    /// An anonymous agent, for canisters outside the login's delegation
    /// targets, such as commerce.
    pub(crate) async fn anonymous(&mut self) -> Result<Ic> {
        if self.anonymous.is_none() {
            self.anonymous = Some(Ic::anonymous(&self.config).await?);
        }
        self.anonymous.clone().ok_or(Error::AuthRequired)
    }

    /// Fresh certified security whose evidence the relay holds.
    ///
    /// # Errors
    /// Returns `Error::DeviceNotApproved` for a revoked device, whose
    /// workspace is then erased.
    pub(crate) async fn cloud_state(&mut self) -> Result<Security> {
        let (_, state) = self.hold().signed(Client::fresh).await?;
        Ok(state)
    }

    /// [`Self::fresh_state`], or `Error::PolicyStale` until a
    /// [`Self::warmup`] made it fresh.
    pub(crate) fn fresh(&mut self) -> Result<Security> {
        self.fresh_state().cloned().ok_or(Error::PolicyStale)
    }

    /// The certified security the relay holds, while it is fresh enough to
    /// sign requests with.
    pub(crate) fn fresh_state(&self) -> Option<&Security> {
        let account = self.account().ok()?;
        let state = self.security.as_ref()?;
        (self.published == Some(state.certified_at) && fresh(state, &account.id)).then_some(state)
    }

    /// What this client's requests wait on the network for, to run without
    /// holding the client: the certified security `warm` asks for, and the
    /// user homes. `None` when nothing is due.
    ///
    /// # Errors
    /// Returns `Error::RecoveryIncomplete` before the device has an account.
    pub fn warmup(&self, warm: Warm) -> Result<Option<Warmup>> {
        self.warmup_for(Some(warm), true)
    }

    /// [`Self::warmup`] of what `warm` asks for, if anything, and with
    /// `trust` of what checks other accounts' evidence: the user homes and
    /// a public agent.
    pub(crate) fn warmup_for(&self, warm: Option<Warm>, trust: bool) -> Result<Option<Warmup>> {
        let account = self.account()?.clone();
        let security = match warm {
            Some(Warm::Relay) if self.fresh_state().is_some() => None,
            warm => warm,
        };
        let homes = trust && self.homes.is_none();
        let reader = trust && self.session().is_err() && self.anonymous.is_none();
        if security.is_none() && !homes && !reader {
            return Ok(None);
        }
        Ok(Some(Warmup {
            home: account.home,
            device: self.workspace.meta.device_id,
            config: self.config.clone(),
            cloud: self.cloud.clone(),
            ic: match self.session() {
                Ok(login) => Some(login.ic.clone()),
                Err(_) => self.anonymous.clone(),
            },
            // State this device just read, such as on unlock, is handed over
            // as it is; the relay asks for newer when it has seen newer.
            state: match security {
                Some(Warm::Relay) => self.security.clone().filter(|s| fresh(s, &account.id)),
                _ => None,
            },
            account: account.id,
            security,
            homes,
            reader,
        }))
    }

    /// Keep what a [`Warmup`] brought back, unless the account changed while
    /// it ran; returns the security the relay now holds, or the home's
    /// answer when the relay was not involved.
    ///
    /// # Errors
    /// Returns `Error::DeviceNotApproved` for a revoked device, whose
    /// workspace is then erased, and the relay's refusal of the evidence.
    pub fn warmed(&mut self, warmed: Warmed) -> Result<Option<Security>> {
        ensure(
            self.account()?.id == warmed.account,
            Error::Unavailable("the account changed".into()),
        )?;
        if let Some(ic) = warmed.anonymous {
            self.anonymous.get_or_insert(ic);
        }
        if let Some(homes) = warmed.homes {
            self.homes = Some(homes);
        }
        if let Some(state) = &warmed.fetched {
            self.observe(&warmed.account, state)?;
        }
        let Some((state, handed)) = warmed.handed else {
            return Ok(warmed.fetched);
        };
        // The relay refuses a revoked device; the workspace is erased here.
        self.check_revoked(&state)?;
        handed?;
        self.published = Some(state.certified_at);
        Ok(Some(state))
    }

    /// A device-authenticated GET signed now, for the caller to send without
    /// the client; `Error::PolicyStale` until a [`Self::warmup`] made the
    /// relay's evidence fresh.
    pub(crate) fn signed_get(&self, path: &str) -> Result<Prepared> {
        let state = self.fresh_state().ok_or(Error::PolicyStale)?;
        self.cloud.prepare_get(&self.signer(state)?, path, now_ms())
    }

    /// A device command signed now, as [`Self::signed_get`].
    pub(crate) fn signed_post<P: Serialize>(
        &self,
        path: &str,
        action: &str,
        payload: &P,
        request_id: &Hash,
    ) -> Result<Prepared> {
        self.signed_post_sealed(path, action, payload, request_id, None)
    }

    /// [`Self::signed_post`] with a sealed reply beside the command.
    pub(crate) fn signed_post_sealed<P: Serialize>(
        &self,
        path: &str,
        action: &str,
        payload: &P,
        request_id: &Hash,
        sealed: Option<&[u8]>,
    ) -> Result<Prepared> {
        let state = self.fresh_state().ok_or(Error::PolicyStale)?;
        self.cloud.prepare_post(
            &self.signer(state)?,
            path,
            action,
            payload,
            request_id,
            sealed,
            now_ms(),
        )
    }

    /// The upload of a chunk signed now, as [`Self::signed_get`].
    pub(crate) fn signed_put(&self, path: &str, bytes: &[u8]) -> Result<Prepared> {
        let state = self.fresh_state().ok_or(Error::PolicyStale)?;
        self.cloud
            .prepare_put(&self.signer(state)?, path, bytes, now_ms())
    }

    pub(crate) fn signer<'a>(&'a self, state: &'a Security) -> Result<Signer<'a>> {
        Ok(Signer {
            issuer: &state.snapshot.issuer,
            device_id: self.workspace.meta.device_id,
            security_epoch: state.snapshot.security_epoch,
            seed: self.signing()?,
            expires_at: state.expires_at(),
        })
    }

    /// The relay client, for the public reads that sign nothing, such as
    /// [`crate::profile::avatar`]: a caller can let go of the client while
    /// they wait on the network.
    pub fn relay(&self) -> Cloud {
        self.cloud.clone()
    }

    // Every relay request below signs with fresh evidence, so a long sequence
    // of them never outlives the 60-second certificate.

    /// A device-authenticated GET; `None` for an absent resource.
    pub(crate) async fn relay_find(&mut self, path: &str) -> Result<Option<Json>> {
        self.hold().find(path).await
    }

    /// A device-authenticated GET.
    pub(crate) async fn relay_get(&mut self, path: &str) -> Result<Json> {
        self.hold().get(path).await
    }

    /// A signed device command.
    pub(crate) async fn relay_post<P: Serialize + Sync>(
        &mut self,
        path: &str,
        action: &str,
        payload: &P,
        request_id: &Hash,
    ) -> Result<Json> {
        self.hold().post(path, action, payload, request_id).await
    }

    /// A signed device command with a sealed reply beside it.
    pub(crate) async fn relay_post_sealed<P: Serialize + Sync>(
        &mut self,
        path: &str,
        action: &str,
        payload: &P,
        request_id: &Hash,
        sealed: Option<&[u8]>,
    ) -> Result<Json> {
        self.hold()
            .send(|c| c.signed_post_sealed(path, action, payload, request_id, sealed))
            .await
    }

    pub(crate) fn base(&self) -> Result<String> {
        Ok(format!("/v1/accounts/{}", self.account()?.id))
    }

    /// The deployment's user homes, checked against the configured pins.
    pub(crate) async fn homes(&mut self) -> Result<Vec<Principal>> {
        if let Some(homes) = &self.homes {
            return Ok(homes.clone());
        }
        let ic = self.reader().await?;
        let config =
            crate::account::user_homes(&ic, self.config.handle, &self.config.user_homes).await?;
        self.homes = Some(config.user_homes.clone());
        Ok(config.user_homes)
    }

    /// What other accounts' relay evidence is checked against.
    pub(crate) async fn trust(&mut self) -> Result<crate::evidence::Trust> {
        self.hold().trust().await
    }

    /// [`Self::trust`] once the user homes and a public agent are here;
    /// `None` before.
    pub(crate) fn trust_now(&self) -> Result<Option<crate::evidence::Trust>> {
        let account = self.account()?;
        let namespace = account
            .issuer
            .strip_suffix(&account.id.to_string())
            .ok_or(Error::IntegrityFailed)?
            .to_string();
        let reader = match self.session() {
            Ok(login) => Some(&login.ic),
            Err(_) => self.anonymous.as_ref(),
        };
        let (Some(homes), Some(reader)) = (&self.homes, reader) else {
            return Ok(None);
        };
        Ok(Some(crate::evidence::Trust::new(
            homes.clone(),
            namespace,
            reader.root_key(),
        )))
    }

    /// Another account's certified security from its user home, with the
    /// public queries; the relay is handed its evidence.
    ///
    /// # Errors
    /// Returns `Error::NotFound` when no user home knows the account.
    pub(crate) async fn peer_security(&mut self, account: &AccountId) -> Result<Security> {
        let mut peers = self.peers_security(std::slice::from_ref(account)).await?;
        peers.pop().map(|(_, s)| s).ok_or(Error::NotFound)
    }

    /// [`Self::peer_security`] of several accounts at once.
    pub(crate) async fn peers_security(
        &mut self,
        accounts: &[AccountId],
    ) -> Result<Vec<(AccountId, Security)>> {
        let found = self.read_peers(accounts).await?;
        // One request per certificate, of at most 16 accounts' evidence.
        let mut grouped: BTreeMap<(&str, &str), Vec<EvidenceEntry>> = BTreeMap::new();
        for (_, security) in &found {
            let evidence = &security.evidence;
            grouped
                .entry((&evidence.canister, &evidence.certificate))
                .or_default()
                .extend(evidence.entries.iter().cloned());
        }
        let requests: Vec<Evidence> = grouped
            .into_iter()
            .flat_map(|((canister, certificate), entries)| {
                entries
                    .chunks(16)
                    .map(|chunk| Evidence {
                        schema: 1,
                        canister: canister.into(),
                        certificate: certificate.into(),
                        entries: chunk.to_vec(),
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        try_join_all(requests.iter().map(|e| self.cloud.publish_security(e))).await?;
        Ok(found)
    }

    /// [`Peers::read`] of `accounts`, waiting on the network with the client.
    ///
    /// # Errors
    /// Returns `Error::NotFound` when no user home knows an account.
    pub(crate) async fn read_peers(
        &mut self,
        accounts: &[AccountId],
    ) -> Result<Vec<(AccountId, Security)>> {
        let mut peers = self.peers_of(accounts);
        let found = peers.read().await;
        self.peers_met(peers);
        found
    }

    /// What reads `accounts` without the client, for a caller that lets go
    /// of it while the user homes answer; [`Self::peers_met`] keeps what it
    /// learned.
    pub fn peers_of(&self, accounts: &[AccountId]) -> Peers {
        Peers {
            accounts: accounts.to_vec(),
            config: self.config.clone(),
            ic: match self.session() {
                Ok(login) => Some(login.ic.clone()),
                Err(_) => self.anonymous.clone(),
            },
            anonymous: None,
            homes: self.homes.clone(),
            met: accounts
                .iter()
                .filter_map(|account| Some((*account, *self.peers.get(account)?)))
                .collect(),
        }
    }

    /// Keep what `peers` learned: the user homes, the anonymous agent made
    /// on the way and the home each account was met at.
    pub fn peers_met(&mut self, peers: Peers) {
        if let Some(ic) = peers.anonymous {
            self.anonymous.get_or_insert(ic);
        }
        if let Some(homes) = peers.homes {
            self.homes = Some(homes);
        }
        self.peers.extend(peers.met);
    }
}

/// Another account and the device map its home returned.
type Peer = (AccountId, BTreeMap<Hash, Device>);

/// The empty workspace of no selection.
fn idle(environment: &Environment) -> Result<Workspace> {
    Workspace::load(crate::store::Store::memory()?, environment, now_ms())
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
