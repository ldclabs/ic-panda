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
//! for seven days after the last login unlock. Locking drops the login
//! together with every key; with "background online" (`background_online`,
//! plan D-7) only the device signing key stays, so presigned and already
//! sealed versions keep moving while nothing can be read.

use crate::account::{device_bundle, verify_security, Security};
use crate::cloud::{Cloud, Evidence, EvidenceEntry, Signer};
use crate::config::{environment_name, Config};
use crate::content::VAULT;
use crate::crypto::Key;
use crate::ic::Ic;
use crate::library::blobs::Blobs;
use crate::library::catalog::Catalog;
use crate::now_ms;
use crate::root::RootKeys;
use crate::secure::SecureStorage;
use crate::store::LOCAL;
use crate::sync::LastSync;
use crate::workspace::{AccountRef, Keys, Workspace};
use candid::Principal;
use dmsg_types::{
    ensure, user::Device, AccountId, CertifiedBatch, Environment, Error, Hash, Result, DAY,
};
use futures_util::future::try_join_all;
use ic_agent::Identity;
use serde::Serialize;
use serde_json::Value as Json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zeroize::Zeroizing;

/// Fast unlock is refused this long after the last login unlock.
pub const FAST_UNLOCK_MS: u64 = 7 * DAY;
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
    /// The latest sync attempt.
    pub last_sync: Option<LastSync>,
    /// Approval requests waiting for a decision; counted while locked too.
    pub approvals_waiting: u64,
    /// Web requests fetched but not yet opened, as while locked.
    pub sealed_requests: u64,
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
    slot: Slot,
    pub(crate) workspace: Workspace,
    pub(crate) cloud: Cloud,
    pub(crate) login: Option<Login>,
    anonymous: Option<Ic>,
    pub(crate) keys: Option<Keys>,
    resident: Option<Zeroizing<Key>>,
    pub(crate) root: Option<RootKeys>,
    pub(crate) security: Option<Security>,
    published: Option<u64>,
    pub(crate) located: Option<(Principal, AccountId)>,
    /// The deployment's user homes, read once from the handle registry.
    homes: Option<Vec<Principal>>,
    /// The user home of each other account read so far.
    peers: BTreeMap<AccountId, Principal>,
    /// The payer loaded from a PEM file, until the device locks.
    pub(crate) payer: Option<(Principal, Ic)>,
    /// This device's team-wallet key and agent, until the device locks.
    pub(crate) wallet_ic: Option<(Principal, Ic)>,
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
            cosign: None,
            binding: None,
            blobs: None,
            library: None,
            library_maintained_at: None,
            library_collected_at: None,
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
        let state = match (&meta.account, login, &self.keys) {
            (None, None, _) => "logged_out",
            (None, Some(_), _) if self.located.is_some() => "pairing_required",
            (None, Some(_), _) => "no_account",
            (Some(_), _, None) => "locked",
            (Some(_), _, Some(_)) if self.root.is_none() => "root_required",
            (Some(_), _, Some(_)) => "unlocked",
        };
        Ok(Status {
            environment: environment_name(&self.config.environment).into(),
            device_id: (self.slot != Slot::Idle).then(|| hex::encode(*meta.device_id)),
            account: meta.account.as_ref().map(|a| a.id.to_string()),
            home: meta.account.as_ref().map(|a| a.home.to_text()),
            principal: login.map(|l| l.principal.to_text()),
            login_expires_at: login.map(|l| l.expires_at),
            state,
            root_generation: self.root.as_ref().map(|r| r.generation),
            pending_sync: self.workspace.store.count_in(LOCAL)?,
            conflicts: self.workspace.store.count_conflicts(VAULT)?,
            background_online: self.config.background_online,
            resident: self.keys.is_none() && self.resident.is_some(),
            fast_unlock: meta.fast_unlock.is_some(),
            last_sync: self.last_sync()?,
            approvals_waiting: self.approvals_waiting()?,
            sealed_requests: self.sealed_requests()?,
        })
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
    pub async fn login(&mut self, identity: Arc<dyn Identity>, expires_at: u64) -> Result<Status> {
        ensure(expires_at > now_ms(), Error::Expired)?;
        let ic = Ic::connect(&self.config, identity).await?;
        let principal = ic.principal()?;
        self.lock();
        self.login = Some(Login {
            ic,
            principal,
            expires_at,
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
    /// system keeps. Online, the device then checks it is still approved.
    ///
    /// # Errors
    /// Returns `Error::NotFound` when fast unlock is off,
    /// `Error::AuthRequired` seven days after the last login unlock or when
    /// the key is gone, and `Error::DeviceNotApproved` for a revoked device,
    /// whose workspace is then erased.
    pub async fn unlock_fast(&mut self) -> Result<Status> {
        let account = self.account()?.clone();
        ensure(self.workspace.meta.fast_unlock.is_some(), Error::NotFound)?;
        let unlocked = self.workspace.meta.login_unlocked_at.unwrap_or(0);
        ensure(now_ms() < unlocked + FAST_UNLOCK_MS, Error::AuthRequired)?;
        let secure = self.secure.clone().ok_or(Error::NotFound)?;
        let key = match secure.get(&self.fast_name()?)? {
            Some(key) => key,
            None => {
                self.workspace.disable_fast()?;
                return Err(Error::AuthRequired);
            }
        };
        let key: Zeroizing<Key> = Zeroizing::new(
            key.as_slice()
                .try_into()
                .map_err(|_| Error::IntegrityFailed)?,
        );
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
        self.status()
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

    /// Enable fast unlock on this device: a random key goes to the
    /// operating system's credential store and seals a copy of the local
    /// data key.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked and `Error::NotFound` without a
    /// credential store.
    pub fn enable_fast_unlock(&mut self) -> Result<Status> {
        let secure = self.secure.clone().ok_or(Error::NotFound)?;
        self.keys()?;
        let key = Zeroizing::new(crate::crypto::random_key());
        secure.set(&self.fast_name()?, key.as_slice())?;
        let keys = self.keys.take().ok_or(Error::Locked)?;
        let enabled = self.workspace.enable_fast(&keys, &key, now_ms());
        self.keys = Some(keys);
        enabled?;
        self.status()
    }

    /// Disable fast unlock and delete its key.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    pub fn disable_fast_unlock(&mut self) -> Result<Status> {
        if let (Some(secure), Ok(name)) = (&self.secure, self.fast_name()) {
            secure.delete(&name)?;
        }
        self.workspace.disable_fast()?;
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
        let account = self.account()?.clone();
        let fresh = |state: &Security| {
            state.snapshot.account_id == account.id && state.expires_at() > now_ms() + 10_000
        };
        if let (Some(state), Some(at)) = (&self.security, self.published)
            && state.certified_at == at
            && fresh(state)
        {
            return Ok(state.clone());
        }
        // State this device just read, such as on unlock, is handed over as
        // it is; the relay asks for newer when it has seen newer.
        let mut cached = self.security.clone().filter(|s| fresh(s));
        let mut attempts = 0;
        loop {
            let state = match cached.take() {
                Some(state) => state,
                None => self.refresh_security(account.home, &account.id).await?,
            };
            // The relay refuses a revoked device; erase it here first.
            self.check_revoked(&state)?;
            match self.cloud.publish_security(&state.evidence).await {
                Ok(()) => {
                    self.published = Some(state.certified_at);
                    return Ok(state);
                }
                // Replicas may answer with an older, still valid certificate.
                Err(Error::PolicyStale) if attempts < 2 => attempts += 1,
                Err(error) => return Err(error),
            }
        }
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

    // Every relay request below signs with fresh evidence, so a long sequence
    // of them never outlives the 60-second certificate.

    /// A device-authenticated GET; `None` for an absent resource.
    pub(crate) async fn relay_find(&mut self, path: &str) -> Result<Option<Json>> {
        let state = self.cloud_state().await?;
        self.cloud.find(&self.signer(&state)?, path, now_ms()).await
    }

    /// A device-authenticated GET.
    pub(crate) async fn relay_get(&mut self, path: &str) -> Result<Json> {
        self.relay_find(path).await?.ok_or(Error::NotFound)
    }

    /// A signed device command.
    pub(crate) async fn relay_post<P: Serialize>(
        &mut self,
        path: &str,
        action: &str,
        payload: &P,
        request_id: &Hash,
    ) -> Result<Json> {
        let state = self.cloud_state().await?;
        self.cloud
            .post(
                &self.signer(&state)?,
                path,
                action,
                payload,
                request_id,
                now_ms(),
            )
            .await
    }

    /// A signed device command with a sealed reply beside it.
    pub(crate) async fn relay_post_sealed<P: Serialize>(
        &mut self,
        path: &str,
        action: &str,
        payload: &P,
        request_id: &Hash,
        sealed: Option<&[u8]>,
    ) -> Result<Json> {
        let state = self.cloud_state().await?;
        self.cloud
            .post_sealed(
                &self.signer(&state)?,
                path,
                action,
                payload,
                request_id,
                sealed,
                now_ms(),
            )
            .await
    }

    /// Upload one chunk.
    pub(crate) async fn relay_put(&mut self, path: &str, bytes: &[u8]) -> Result<()> {
        let state = self.cloud_state().await?;
        self.cloud
            .put_chunk(&self.signer(&state)?, path, bytes, now_ms())
            .await
    }

    /// Download one chunk and check its digest.
    pub(crate) async fn relay_chunk(&mut self, path: &str, digest: &str) -> Result<Vec<u8>> {
        let state = self.cloud_state().await?;
        self.cloud
            .get_chunk(&self.signer(&state)?, path, digest, now_ms())
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
        let account = self.account()?.clone();
        let namespace = account
            .issuer
            .strip_suffix(&account.id.to_string())
            .ok_or(Error::IntegrityFailed)?
            .to_string();
        let homes = self.homes().await?;
        let root_key = self.reader().await?.root_key();
        Ok(crate::evidence::Trust::new(homes, namespace, root_key))
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

    /// Other accounts' certified security from their user homes, with the
    /// public queries: their device maps side by side, from the home each
    /// was met at or else the first that knows it, then one certified batch
    /// per home.
    ///
    /// # Errors
    /// Returns `Error::NotFound` when no user home knows an account.
    pub(crate) async fn read_peers(
        &mut self,
        accounts: &[AccountId],
    ) -> Result<Vec<(AccountId, Security)>> {
        let ic = self.reader().await?;
        let homes = self.homes().await?;
        let lookups = accounts.iter().map(|account| {
            let candidates = self
                .peers
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
        for (account, (home, devices)) in accounts.iter().zip(try_join_all(lookups).await?) {
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
        let mut found = Vec::with_capacity(accounts.len());
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
                self.peers.insert(account, home);
                found.push((account, security));
            }
        }
        Ok(found)
    }
}

/// Another account and the device map its home returned.
type Peer = (AccountId, BTreeMap<Hash, Device>);

/// The empty workspace of no selection.
fn idle(environment: &Environment) -> Result<Workspace> {
    Workspace::load(crate::store::Store::memory()?, environment, now_ms())
}

fn storage(error: std::io::Error) -> Error {
    Error::Unavailable(format!("storage: {error}"))
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
