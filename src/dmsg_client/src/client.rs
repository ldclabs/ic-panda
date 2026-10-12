//! One device's client: the workspace, the login and, while unlocked, the
//! keys. The daemon owns a single instance; mobile apps will too.
//!
//! Unlocking needs a login: the user home releases the device's unlock
//! secret only to a Principal bound to the account. Fast unlock opens a
//! second copy of the local data key with a key the operating system keeps,
//! for seven days after the last login unlock. Locking drops the login
//! together with every key; with "background online" (`background_online`,
//! plan D-7) only the device signing key stays, so presigned and already
//! sealed versions keep moving while nothing can be read.

use crate::account::Security;
use crate::cloud::{Cloud, Signer};
use crate::config::{environment_name, Config};
use crate::content::VAULT;
use crate::crypto::Key;
use crate::ic::Ic;
use crate::now_ms;
use crate::root::RootKeys;
use crate::secure::SecureStorage;
use crate::store::LOCAL;
use crate::sync::LastSync;
use crate::workspace::{AccountRef, Keys, Workspace};
use candid::Principal;
use dmsg_types::{ensure, AccountId, Environment, Error, Hash, Result, DAY};
use ic_agent::Identity;
use serde::Serialize;
use serde_json::Value as Json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zeroize::Zeroizing;

/// Fast unlock is refused this long after the last login unlock.
pub const FAST_UNLOCK_MS: u64 = 7 * DAY;

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
    /// This device's ID.
    pub device_id: String,
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
}

/// A device's client.
pub struct Client {
    pub(crate) config: Config,
    dir: PathBuf,
    secure: Option<Arc<dyn SecureStorage>>,
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
}

impl Client {
    /// Open the client of `config` with its workspace under `dir`, creating
    /// a new device on first use. `secure` is the operating system's
    /// credential store; without it fast unlock is unavailable.
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
        std::fs::create_dir_all(dir).map_err(|e| Error::Unavailable(format!("storage: {e}")))?;
        let workspace =
            Workspace::open(&dir.join("workspace.sqlite"), &config.environment, now_ms())?;
        let cloud = Cloud::new(
            &config.relay_origin(),
            config.environment == Environment::Local,
        )?;
        Ok(Self {
            config,
            dir: dir.to_path_buf(),
            secure,
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
        })
    }

    /// The deployment parameters.
    pub fn config(&self) -> &Config {
        &self.config
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
            device_id: hex::encode(*meta.device_id),
            account: meta.account.as_ref().map(|a| a.id.to_string()),
            home: meta.account.as_ref().map(|a| a.home.to_text()),
            principal: login.map(|l| l.principal.to_text()),
            login_expires_at: login.map(|l| l.expires_at),
            state,
            root_generation: self.root.as_ref().map(|r| r.generation),
            pending_sync: self.workspace.store.count_in(LOCAL)?,
            conflicts: self.workspace.store.conflicts(VAULT)?.len() as u64,
            background_online: self.config.background_online,
            resident: self.keys.is_none() && self.resident.is_some(),
            fast_unlock: meta.fast_unlock.is_some(),
            last_sync: self.last_sync()?,
        })
    }

    /// Accept a login: check it against every user home with the read-only
    /// `my_account`, then unlock. A failed check discards the login.
    ///
    /// # Errors
    /// Returns `Error::AuthRequired` when the login is not bound to this
    /// workspace's account; query failures discard the login.
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
        let checked = match self.locate().await {
            Ok(found) => match &self.workspace.meta.account {
                Some(account) => ensure(
                    found == Some((account.home, account.id)),
                    Error::AuthRequired,
                ),
                None => Ok(()),
            },
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
            if let Err(error) = self.resume().await {
                if error != Error::Expired {
                    return Err(error);
                }
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
    }

    /// Drop everything, the resident key included, as the daemon exits.
    pub fn shutdown(&mut self) {
        self.lock();
        self.resident = None;
    }

    /// Erase this device's workspace and its fast-unlock key and start over
    /// as a new device. Versions the relay has not accepted are lost.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when files cannot be removed.
    pub fn reset(&mut self) -> Result<Status> {
        if let (Some(secure), Ok(name)) = (&self.secure, self.fast_name()) {
            secure.delete(&name)?;
        }
        self.shutdown();
        let environment = self.config.environment.clone();
        // Close the database before removing its files.
        self.workspace = Workspace::load(crate::store::Store::memory()?, &environment, now_ms())?;
        for name in [
            "workspace.sqlite",
            "workspace.sqlite-wal",
            "workspace.sqlite-shm",
        ] {
            match std::fs::remove_file(self.dir.join(name)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(Error::Unavailable(format!("storage: {e}")))
                }
                _ => {}
            }
        }
        self.workspace =
            Workspace::open(&self.dir.join("workspace.sqlite"), &environment, now_ms())?;
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
        if self.anonymous.is_none() {
            self.anonymous = Some(Ic::anonymous(&self.config).await?);
        }
        self.anonymous.clone().ok_or(Error::AuthRequired)
    }

    /// Fresh certified security whose evidence the relay holds.
    pub(crate) async fn cloud_state(&mut self) -> Result<Security> {
        let account = self.account()?.clone();
        if let (Some(state), Some(at)) = (&self.security, self.published) {
            if state.certified_at == at
                && state.snapshot.account_id == account.id
                && state.expires_at() > now_ms() + 10_000
            {
                return Ok(state.clone());
            }
        }
        let mut attempts = 0;
        loop {
            let state = self.refresh_security(account.home, &account.id).await?;
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
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
