//! One device's client: the workspace, the login and, while unlocked, the
//! keys. The daemon owns a single instance; mobile apps will too.
//!
//! Unlocking needs a login: the user home releases the device's unlock
//! secret only to a Principal bound to the account. Locking drops the login
//! together with every key.

use crate::account::AccountState;
use crate::cloud::{Cloud, Signer};
use crate::config::{environment_name, Config};
use crate::ic::Ic;
use crate::now_ms;
use crate::root::RootKeys;
use crate::store::LOCAL;
use crate::workspace::{AccountRef, Keys, Workspace};
use candid::Principal;
use dmsg_types::{ensure, AccountId, Environment, Error, Hash, Result};
use ic_agent::Identity;
use serde::Serialize;
use serde_json::Value as Json;
use std::path::Path;
use std::sync::Arc;

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
    pub pending_sync: usize,
}

/// A device's client.
pub struct Client {
    pub(crate) config: Config,
    pub(crate) workspace: Workspace,
    pub(crate) cloud: Cloud,
    pub(crate) login: Option<Login>,
    pub(crate) keys: Option<Keys>,
    pub(crate) root: Option<RootKeys>,
    pub(crate) state: Option<AccountState>,
    published: Option<u64>,
    pub(crate) located: Option<(Principal, AccountId)>,
}

impl Client {
    /// Open the client of `config` with its workspace under `dir`, creating
    /// a new device on first use.
    ///
    /// # Errors
    /// An invalid configuration returns `Error::InvalidInput`; storage
    /// failures return `Error::Unavailable`.
    pub fn open(config: Config, dir: &Path) -> Result<Self> {
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
            workspace,
            cloud,
            login: None,
            keys: None,
            root: None,
            state: None,
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
            pending_sync: self.workspace.store.objects_in(LOCAL)?.len(),
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
    /// device the account has approved is bound on the way.
    ///
    /// # Errors
    /// Returns `Error::AuthRequired` without a valid login and
    /// `Error::DeviceNotApproved` for a revoked device.
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
            match self.load_root().await {
                // Approved, but no root is wrapped to this device yet.
                Err(Error::DeviceNotApproved) => {}
                result => result?,
            }
        }
        self.status()
    }

    /// Drop the login and every key.
    pub fn lock(&mut self) {
        self.login = None;
        self.keys = None;
        self.root = None;
        self.state = None;
        self.published = None;
        self.located = None;
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

    pub(crate) fn account(&self) -> Result<&AccountRef> {
        self.workspace
            .meta
            .account
            .as_ref()
            .ok_or(Error::RecoveryIncomplete)
    }

    /// Fresh account state whose evidence the relay holds.
    pub(crate) async fn cloud_state(&mut self) -> Result<AccountState> {
        if let (Some(state), Some(at)) = (&self.state, self.published) {
            if state.certified_at == at && state.expires_at() > now_ms() + 10_000 {
                return Ok(state.clone());
            }
        }
        let account = self.account()?.clone();
        let mut attempts = 0;
        loop {
            let state = self.refresh(account.home, &account.id).await?;
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

    fn signer<'a>(&'a self, state: &'a AccountState) -> Result<Signer<'a>> {
        Ok(Signer {
            issuer: &state.info.issuer,
            device_id: self.workspace.meta.device_id,
            security_epoch: state.info.security_epoch,
            seed: &self.keys()?.signing,
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
