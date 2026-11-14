//! Native mobile API. The generated API exposes named operations, not daemon RPC.
mod agents;
mod apple;
mod approvals;
mod background;
pub use apple::{AppAssertion, PushRegistration};
mod connect;
mod content;
pub use content::{ChannelMemberChange, CredentialSelection};
mod diagnostics;
mod display;
mod error;
mod feedback;
mod file_library;
pub use feedback::FeedbackDraftInput;
mod hints;
mod messaging;
mod models;
pub use messaging::LetterDraftInput;
mod notifications;
pub use notifications::{
    NotificationPresentationContext, NotificationPresentationResult, ReminderScope, VisibleReminder,
};
mod operation;
mod passkey;
mod platform;
mod recovery;
mod safety;
pub use safety::ReportInput;
mod wallet;
pub use wallet::{WalletChange, WalletTransfer};
mod secrets;
mod security;
mod session;
mod sync;
pub use sync::ContentSyncStatus;
mod succession;
mod transfer;
mod vault;
pub use vault::VaultBrowseRequest;

#[cfg(test)]
mod binding_tests;
#[cfg(test)]
mod sync_tests;

use dmsg_client::hold::Hold;
use dmsg_client::{
    Client,
    config::Config,
    host::Host,
    login::{InteractiveLogin, LoginSession},
};
use dmsg_types::{Error, ensure, ensure_valid};
pub use error::MobileError;
use error::Result;
pub use models::*;
pub use operation::{OperationToken, TransferProgress};
pub use platform::{Agreement, Platform, SigningKey};
pub use secrets::{SecretAttachmentSelection, SecretPackageSelection, SecretSnapshotInput};
use serde_json::json;
use session::ScopedHost;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};
use tokio::runtime::Runtime;

static SNAPSHOT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

uniffi::setup_scaffolding!();

#[uniffi::export]
pub fn vault_kinds() -> Vec<String> {
    dmsg_client::vault::TYPES
        .iter()
        .map(|v| (*v).into())
        .collect()
}

#[uniffi::export]
pub fn vault_template(kind: String) -> Vec<VaultField> {
    dmsg_client::vault::template(&kind)
        .iter()
        .map(|(id, kind)| VaultField {
            id: (*id).into(),
            kind: (*kind).into(),
            label: String::new(),
            section: String::new(),
            value: String::new(),
            concealed: matches!(*kind, "concealed" | "otp"),
        })
        .collect()
}

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .max_blocking_threads(2)
            .thread_stack_size(8 * 1024 * 1024)
            .thread_name("dmsg-mobile")
            .enable_all()
            .build()
            .expect("mobile runtime")
    })
}

#[derive(uniffi::Object)]
pub struct MobileEngine {
    host: ScopedHost,
    platform: Arc<dyn Platform>,
    login: Arc<Mutex<LoginState>>,
    config: Config,
    staging: PathBuf,
    approval_turn: Arc<tokio::sync::Mutex<()>>,
    read_turns: Arc<display::ReadTurns>,
    sync_tracker: Arc<sync::Tracker>,
    hints_redial: Arc<tokio::sync::Notify>,
    sync_signal: Arc<sync::SyncSignal>,
    /// When the plan's expiry reminders were last refreshed.
    plan_checked: Arc<std::sync::atomic::AtomicU64>,
    /// The device transfer, shared by every view of the engine.
    transfers: Arc<dmsg_client::transfer::Transfers>,
}

struct PendingLogin {
    session: InteractiveLogin,
    binding_generation: Option<u64>,
    resetting: bool,
    callback: Option<Vec<u8>>,
    delegated: Option<dmsg_client::login::Delegated>,
}
#[derive(Default)]
struct LoginState {
    revision: u64,
    pending: Option<PendingLogin>,
    completing: Option<tokio::task::AbortHandle>,
}

impl MobileEngine {
    /// Apply a verified logout-all result. Capacity recovery only clears the
    /// remote target; ordinary logout also ends the selected local workspace.
    async fn finish_passkey_logout(
        &self,
        reset_generation: Option<u64>,
    ) -> Result<SessionSnapshot> {
        if let Some(generation) = reset_generation {
            ensure(self.host.inner.generation() == generation, Error::Locked)?;
            return self.session_state().await;
        }
        self.logout().await
    }

    async fn held<T: Send + 'static>(
        &self,
        work: impl for<'a> FnOnce(&'a ScopedHost) -> dmsg_client::host::Work<'a, T> + Send + 'static,
    ) -> Result<T> {
        let host = self.host.clone();
        let task = runtime().spawn(async move {
            host.inner
                .while_current(host.generation(), work(&host))
                .await
        });
        struct Abort(tokio::task::AbortHandle);
        impl Drop for Abort {
            fn drop(&mut self) {
                self.0.abort();
            }
        }
        let _abort = Abort(task.abort_handle());
        task.await.map_err(|_| Error::Locked)?.map_err(Into::into)
    }

    async fn held_object<T: serde::Serialize + Send + 'static>(
        &self,
        work: impl for<'a> FnOnce(&'a ScopedHost) -> dmsg_client::host::Work<'a, T> + Send + 'static,
    ) -> Result<ObjectSnapshot> {
        let generation = self.host.generation();
        let json = self
            .held(move |h| {
                Box::pin(async move {
                    serde_json::to_string(&work(h).await?).map_err(|_| Error::IntegrityFailed)
                })
            })
            .await?;
        Ok(ObjectSnapshot { generation, json })
    }

    async fn object<T: serde::Serialize + Send + 'static>(
        &self,
        work: impl for<'a> FnOnce(&'a mut Client) -> dmsg_client::host::Work<'a, T> + Send + 'static,
    ) -> Result<ObjectSnapshot> {
        let generation = self.host.generation();
        let json = self
            .host
            .run_at(generation, move |c| {
                Box::pin(async move {
                    serde_json::to_string(&work(c).await?).map_err(|_| Error::IntegrityFailed)
                })
            })
            .await?;
        Ok(ObjectSnapshot { generation, json })
    }

    /// Fence old UI handles before renewal, including a failed attempt. The
    /// transition's Hold keeps signing and network work outside the client.
    async fn transition_with_login(&self) -> Result<session::Transition> {
        let transition = self.host.begin_transition()?;
        self.platform.cancel_presence();
        runtime()
            .spawn(async move {
                transition.ensure_login().await?;
                Ok::<_, Error>(transition)
            })
            .await
            .map_err(|_| Error::Unavailable("login transition worker".into()))?
            .map_err(Into::into)
    }
}

impl MobileEngine {
    async fn open_config(
        directory: String,
        staging: String,
        platform: Box<dyn Platform>,
        mut config: Config,
    ) -> Result<Arc<Self>> {
        diagnostics::init();
        let platform: Arc<dyn Platform> = Arc::from(platform);
        let adapter = Arc::new(platform::Adapter(platform.clone()));

        config.files_max_bytes = dmsg_client::files::MAX_BACKUP;
        let directory = PathBuf::from(directory);
        let staging = PathBuf::from(staging);
        ensure_valid(
            directory.is_absolute() && staging.is_absolute() && directory != staging,
            "sandbox paths",
        )?;
        let opened_config = config.clone();
        let client = runtime()
            .spawn_blocking(move || {
                let mut client = Client::open(opened_config, &directory, Some(adapter.clone()))?;
                client.set_presence_key(adapter.clone());
                client.set_approval_keys(adapter.clone());
                client.set_hardware_anchor(adapter.clone())?;
                client.set_cosign_provider(adapter);
                client.require_fast_presence();
                client.use_read_only_previews();
                client.set_thumbnail_budget(dmsg_client::library::import::ThumbnailBudget::MOBILE);
                client.require_prepared_approval();
                Ok::<_, Error>(client)
            })
            .await
            .map_err(|_| Error::Unavailable("mobile worker".into()))??;
        Ok(Arc::new(Self {
            host: ScopedHost::new(Host::start(client, runtime().handle()), 0),
            platform,
            login: Arc::new(Mutex::new(LoginState::default())),
            config,
            staging,
            approval_turn: Arc::new(tokio::sync::Mutex::new(())),
            read_turns: Arc::new(display::ReadTurns::default()),
            sync_tracker: Arc::default(),
            hints_redial: Arc::default(),
            sync_signal: Arc::default(),
            plan_checked: Arc::default(),
            transfers: Arc::default(),
        }))
    }
}

#[uniffi::export]
impl MobileEngine {
    #[uniffi::constructor]
    pub async fn open(
        directory: String,
        staging: String,
        platform: Box<dyn Platform>,
    ) -> Result<Arc<Self>> {
        Self::open_config(directory, staging, platform, Config::production()?).await
    }

    #[uniffi::constructor]
    pub async fn open_staging(
        directory: String,
        staging: String,
        platform: Box<dyn Platform>,
        deployment: StagingDeployment,
    ) -> Result<Arc<Self>> {
        let mut config = Config::staging(
            &deployment.relay,
            &deployment.handle,
            &deployment.user_homes,
            deployment.wallet_hub.as_deref(),
            &deployment.connect_url,
        )?;
        config.auth = deployment
            .auth
            .map(|text| text.parse().map_err(|_| dmsg_types::invalid("auth")))
            .transpose()?;
        config.validate()?;
        Self::open_config(directory, staging, platform, config).await
    }

    pub async fn session_state(&self) -> Result<SessionSnapshot> {
        self.host.ensure_available()?;
        let generation = self.host.inner.generation();
        let (status, sequence) = self
            .host
            .inner
            .run_at(generation, |c| {
                Box::pin(async {
                    Ok((
                        c.status()?,
                        SNAPSHOT_SEQUENCE.fetch_add(1, Ordering::Relaxed),
                    ))
                })
            })
            .await?;
        self.host.ensure_available()?;
        ensure(generation == self.host.inner.generation(), Error::Locked)?;
        Ok(SessionSnapshot::from_status(status, generation, sequence))
    }

    /// A new immutable view of the same Client and runtime, not a second engine.
    pub fn scoped(&self, generation: u64) -> Result<Arc<Self>> {
        ensure(generation == self.host.inner.generation(), Error::Locked)?;
        Ok(Arc::new(Self {
            host: self.host.scoped(generation)?,
            platform: self.platform.clone(),
            login: self.login.clone(),
            config: self.config.clone(),
            staging: self.staging.clone(),
            approval_turn: self.approval_turn.clone(),
            read_turns: self.read_turns.clone(),
            sync_tracker: self.sync_tracker.clone(),
            hints_redial: self.hints_redial.clone(),
            sync_signal: self.sync_signal.clone(),
            plan_checked: self.plan_checked.clone(),
            transfers: self.transfers.clone(),
        }))
    }

    /// Explicit lock flushes already authored drafts behind a transition fence.
    /// Existing network work is cancelled before the short local transaction.
    pub async fn lock_with_drafts(
        &self,
        account: String,
        drafts: Vec<MessageDraft>,
        letters: Vec<LetterDraftInput>,
        feedback: Option<FeedbackDraftInput>,
    ) -> Result<SessionSnapshot> {
        ensure_valid(drafts.len() <= 100, "drafts")?;
        let transition = self.host.begin_transition()?;
        struct LockOnDrop {
            host: Host,
            armed: bool,
        }
        impl Drop for LockOnDrop {
            fn drop(&mut self) {
                if self.armed {
                    self.host.lock();
                }
            }
        }
        let mut cleanup = LockOnDrop {
            host: self.host.inner.clone(),
            armed: true,
        };
        self.platform.cancel_presence();
        transition
            .run(move |c| {
                Box::pin(async move {
                    let saved = (|| {
                        ensure(
                            c.session_state().account.as_deref() == Some(account.as_str()),
                            Error::Locked,
                        )?;
                        for draft in drafts {
                            c.channel_draft(&draft.channel, &draft.text)?;
                        }
                        let now = dmsg_client::now_ms();
                        for draft in letters {
                            c.save_letter_draft_at(
                                &draft.id,
                                &draft.recipient,
                                &draft.text,
                                draft.invitation,
                                now,
                            )?;
                        }
                        // Last, so a report the core refuses keeps no letter from saving.
                        if let Some(mut draft) = feedback {
                            if let Some(current) = c.feedback()? {
                                ensure(
                                    current.id == draft.id && current.state == "draft",
                                    Error::VersionConflict,
                                )?;
                                draft.revision = current.revision;
                            }
                            draft.save(c)?;
                        }
                        Ok(())
                    })();
                    c.lock();
                    saved
                })
            })
            .await?;
        cleanup.armed = false;
        transition.finish()?;
        self.session_state().await
    }

    pub async fn lock(&self) -> Result<SessionSnapshot> {
        self.lock_now();
        self.session_state().await
    }

    pub fn lock_now(&self) {
        self.cancel_login();
        self.host.inner.lock();
        self.platform.cancel_presence();
    }

    pub fn suspend(&self) {
        self.host.inner.pause();
        self.platform.cancel_presence();
    }

    pub fn control_generation(&self) -> u64 {
        self.host.inner.generation()
    }

    pub async fn resume(&self, expected_generation: u64) -> Result<SessionSnapshot> {
        self.host.inner.resume_if(expected_generation)?;
        self.session_state().await
    }

    pub async fn close(&self) -> Result<()> {
        self.host.inner.close();
        self.platform.cancel_presence();
        self.cancel_login();
        self.host.inner.closed().await?;
        Ok(())
    }

    pub async fn logout(&self) -> Result<SessionSnapshot> {
        let transition = self.host.begin_transition()?;
        self.platform.cancel_presence();
        self.cancel_login();
        runtime()
            .spawn(async move {
                dmsg_client::passkey::logout(&transition).await?;
                transition.finish()
            })
            .await
            .map_err(|_| Error::Unavailable("logout worker".into()))??;
        self.session_state().await
    }

    pub async fn accounts(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.accounts() })).await
    }

    pub async fn switch_account(&self, account: String) -> Result<SessionSnapshot> {
        let account = account
            .parse()
            .map_err(|_| dmsg_types::invalid("account"))?;
        let transition = self.host.begin_transition()?;
        self.platform.cancel_presence();
        self.cancel_login();
        transition
            .run(move |c| Box::pin(async move { c.switch_account(&account) }))
            .await?;
        transition.finish()?;
        self.session_state().await
    }

    /// The workspaces set aside after they came from other hardware.
    pub async fn moved_copies(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.moved_copies() })).await
    }

    /// Set the selected workspace, which came from other hardware, aside
    /// and start a new device of its account in its place.
    pub async fn set_aside_moved(&self) -> Result<SessionSnapshot> {
        let transition = self.host.begin_transition()?;
        self.platform.cancel_presence();
        self.cancel_login();
        transition
            .run(|c| Box::pin(async { c.set_aside_moved() }))
            .await?;
        transition.finish()?;
        self.session_state().await
    }

    /// Delete a copy set aside; the person confirmed it.
    pub async fn discard_moved(&self, id: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.discard_moved(&id) }))
            .await
    }

    pub async fn begin_login(&self) -> Result<LoginStart> {
        self.cancel_login();
        let generation = self.host.generation();
        let revision = self.login.lock().map_err(|_| Error::Locked)?.revision;
        let config = self.config.clone();
        let session = runtime()
            .spawn(async move {
                let targets = dmsg_client::account::login_targets(&config).await?;
                LoginSession::ios(
                    &targets,
                    "https://dmsg.net",
                    dmsg_client::login::DEFAULT_TTL_MS,
                    dmsg_client::now_ms(),
                )
            })
            .await
            .map_err(|_| Error::Unavailable("login worker".into()))??;
        let started = LoginStart {
            url: session.url(&self.config),
            deadline_ms: session.deadline,
        };
        let mut state = self.login.lock().map_err(|_| Error::Locked)?;
        ensure(state.revision == revision, Error::Expired)?;
        ensure(self.host.inner.generation() == generation, Error::Locked)?;
        state.pending = Some(PendingLogin {
            session: session.into(),
            binding_generation: None,
            resetting: false,
            callback: None,
            delegated: None,
        });
        Ok(started)
    }

    pub fn cancel_login(&self) {
        if let Ok(mut login) = self.login.lock() {
            login.revision = login.revision.wrapping_add(1);
            login.pending = None;
            if let Some(task) = login.completing.take() {
                task.abort();
            }
        }
    }

    pub async fn begin_binding(&self) -> Result<LoginStart> {
        self.held(|h| Box::pin(async { h.ensure_login().await }))
            .await?;
        let generation = self.host.generation();
        self.host
            .run_at(generation, |c| {
                Box::pin(async { ensure(c.session_state().state == "unlocked", Error::Locked) })
            })
            .await?;
        let start = self.begin_login().await?;
        let mut login = self.login.lock().map_err(|_| Error::Locked)?;
        ensure(
            login
                .pending
                .as_ref()
                .is_some_and(|p| p.session.url(&self.config) == start.url),
            Error::Expired,
        )?;
        if self.host.inner.generation() != generation {
            login.pending = None;
            return Err(Error::Locked.into());
        }
        login
            .pending
            .as_mut()
            .ok_or(Error::Expired)?
            .binding_generation = Some(generation);
        Ok(start)
    }

    pub async fn complete_binding(&self, callback: String) -> Result<ObjectSnapshot> {
        ensure_valid(callback.len() <= 96 * 1024, "callback size")?;
        let (target, fragment) = callback
            .split_once('#')
            .ok_or_else(|| dmsg_types::invalid("callback"))?;
        ensure_valid(
            target == dmsg_protocol::login::IOS_CALLBACK,
            "callback target",
        )?;
        let bytes = dmsg_client::codec::unb64(fragment)?;
        let (revision, generation, task) = {
            let mut login = self.login.lock().map_err(|_| Error::Locked)?;
            let pending = login.pending.as_mut().ok_or(Error::Expired)?;
            ensure(!pending.resetting, Error::Forbidden)?;
            let generation = pending.binding_generation.ok_or(Error::Forbidden)?;
            ensure(self.host.inner.generation() == generation, Error::Locked)?;
            if let Some(accepted) = &pending.callback {
                ensure(*accepted == bytes, Error::IntegrityFailed)?;
            }
            // Validate before claiming. Once authentication succeeded, a retry
            // resumes binding the cached identity, not the expired challenge.
            let proof = if pending.delegated.is_none() {
                Some(pending.session.accept(&bytes, dmsg_client::now_ms())?)
            } else {
                None
            };
            pending.callback = Some(bytes);
            let mut pending = login.pending.take().expect("validated binding callback");
            let host = self.host.clone();
            let task = runtime().spawn(async move {
                let result = async {
                    if let Some(proof) = proof {
                        pending.delegated = pending.session.complete(proof).await?;
                    }
                    let delegated = pending.delegated.clone().ok_or(Error::Forbidden)?;
                    host.run_at(generation, move |c| {
                        Box::pin(async move {
                            let view = c.bind_delegated_login("gui", delegated).await?;
                            serde_json::to_string(&view).map_err(|_| Error::IntegrityFailed)
                        })
                    })
                    .await
                }
                .await;
                (pending, result)
            });
            login.completing = Some(task.abort_handle());
            (login.revision, generation, task)
        };
        let (pending, result) = task.await.map_err(|_| Error::Locked)?;
        {
            let mut login = self.login.lock().map_err(|_| Error::Locked)?;
            ensure(
                login.revision == revision && self.host.inner.generation() == generation,
                Error::Locked,
            )?;
            login.completing = None;
            if matches!(
                &result,
                Err(Error::ExecutionUnknown | Error::Unavailable(_))
            ) {
                login.pending = Some(pending);
            }
        }
        let json = result?;
        Ok(ObjectSnapshot { generation, json })
    }

    pub async fn complete_login(&self, callback: String) -> Result<SessionSnapshot> {
        ensure_valid(callback.len() <= 96 * 1024, "callback size")?;
        let (target, fragment) = callback
            .split_once('#')
            .ok_or_else(|| dmsg_types::invalid("callback"))?;
        ensure_valid(
            target == dmsg_protocol::login::IOS_CALLBACK,
            "callback target",
        )?;
        let mut session = self
            .login
            .lock()
            .map_err(|_| Error::Locked)?
            .pending
            .take()
            .ok_or(Error::Expired)?;
        let reset_generation = if session.resetting {
            Some(session.binding_generation.ok_or(Error::Forbidden)?)
        } else {
            ensure(session.binding_generation.is_none(), Error::Forbidden)?;
            None
        };
        if let Some(generation) = reset_generation {
            ensure(self.host.inner.generation() == generation, Error::Locked)?;
        }
        let bytes = dmsg_client::codec::unb64(fragment)?;
        let proof = session.session.accept(&bytes, dmsg_client::now_ms())?;
        // Backgrounding during the login pauses the host without ending it;
        // logout, account switches and the app's lock cancel the pending login.
        let delegated = runtime()
            .spawn(async move { session.session.complete(proof).await })
            .await
            .map_err(|_| Error::Unavailable("login worker".into()))??;
        let Some(delegated) = delegated else {
            return self.finish_passkey_logout(reset_generation).await;
        };
        ensure(reset_generation.is_none(), Error::Forbidden)?;
        // A newly authenticated identity starts a new content session. Even a
        // handle captured on the locked screen may not act for this new account.
        let controller = self.host.scoped(self.host.inner.generation())?;
        let transition = controller.begin_transition()?;
        self.platform.cancel_presence();
        let generation = transition.generation();
        let prepared = transition
            .run(move |c| {
                Box::pin(async move {
                    c.lock();
                    c.prepare_login(delegated).await
                })
            })
            .await?;
        self.finish_login_unlock(generation, prepared).await?;
        transition.finish()?;
        self.session_state().await
    }

    pub async fn unlock_fast(&self) -> Result<SessionSnapshot> {
        let transition = self.host.begin_transition()?;
        self.platform.cancel_presence();
        let prepared = transition
            .run(|c| Box::pin(async { c.prepare_fast_unlock() }))
            .await?;
        let (prepared, key) = runtime()
            .spawn_blocking(move || {
                let key = prepared.open();
                (prepared, key)
            })
            .await
            .map_err(|_| Error::Unavailable("presence worker".into()))?;
        let failure = key.as_ref().err().cloned().map(security::hardware_failure);
        let completed = transition
            .run(move |c| Box::pin(async move { c.complete_fast_unlock(prepared, key).await }))
            .await;
        if matches!(completed, Err(Error::Locked)) {
            return Err(Error::Locked.into());
        }
        if let Some(error) = failure {
            return Err(error);
        }
        completed?;
        transition.finish()?;
        self.session_state().await
    }

    pub async fn set_fast_unlock(&self, enabled: bool) -> Result<SessionSnapshot> {
        let generation = self.host.generation();
        if enabled {
            let prepared = self
                .host
                .run_at(generation, |c| {
                    Box::pin(async { c.prepare_enable_fast_unlock() })
                })
                .await?;
            if let Some(prepared) = prepared {
                let (prepared, key) = runtime()
                    .spawn_blocking(move || {
                        let key = prepared.open();
                        (prepared, key)
                    })
                    .await
                    .map_err(|_| Error::Unavailable("hardware: worker".into()))?;
                let failure = key.as_ref().err().cloned().map(security::hardware_failure);
                let completed = self
                    .host
                    .run_at(generation, move |c| {
                        Box::pin(async move { c.complete_enable_fast_unlock(prepared, key) })
                    })
                    .await;
                if matches!(completed, Err(Error::Locked)) {
                    return Err(Error::Locked.into());
                }
                if let Some(error) = failure {
                    return Err(error);
                }
                completed?;
            }
        } else {
            self.host
                .run_at(generation, |c| Box::pin(async { c.disable_fast_unlock() }))
                .await?;
        }
        self.session_state().await
    }

    pub async fn initialize_account(&self) -> Result<SessionSnapshot> {
        let transition = self.transition_with_login().await?;
        let generation = transition.generation();
        let prepared = transition
            .run(|c| Box::pin(async { c.prepare_initialize_account().await }))
            .await?;
        self.finish_login_unlock(generation, prepared).await?;
        transition
            .run(|c| Box::pin(async { c.initialize_account().await }))
            .await?;
        transition.finish()?;
        self.session_state().await
    }

    pub async fn pair_request(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| {
            Box::pin(async {
                dmsg_client::account::pair_request(
                    h,
                    dmsg_types::user::ControllerRole::Administrator,
                )
                .await
                .map(|request| request.token)
            })
        })
        .await
    }

    /// Send this provisional device's signed request to a scanned desktop
    /// invitation. The desktop still requires explicit approval.
    pub async fn join_device_invitation(&self, invitation: String) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(async move { dmsg_client::pairing::join(h, &invitation).await })
        })
        .await
    }

    pub async fn refresh_account(&self) -> Result<SessionSnapshot> {
        let transition = self.transition_with_login().await?;
        let generation = transition.generation();
        let prepared = transition
            .run(|c| Box::pin(async { c.prepare_unlock().await }))
            .await?;
        self.finish_login_unlock(generation, prepared).await?;
        transition.finish()?;
        self.session_state().await
    }

    pub async fn vault_page(
        &self,
        query: String,
        kind: Option<String>,
        favorites_only: bool,
        offset: u32,
        limit: u32,
    ) -> Result<VaultPage> {
        ensure_valid(query.len() <= 1000, "query")?;
        let generation = self.host.generation();
        let (items, has_more) = self
            .host
            .run_at(generation, move |c| {
                Box::pin(async move {
                    dmsg_client::vault::reading::page(
                        &c.cooperative_hold(),
                        &dmsg_client::vault::Filter {
                            trash: false,
                            query: Some(query),
                        },
                        kind.as_deref(),
                        favorites_only,
                        offset as usize,
                        limit as usize,
                    )
                    .await
                })
            })
            .await?;
        Ok(VaultPage {
            generation,
            items: items.into_iter().map(VaultItem::summary).collect(),
            has_more,
        })
    }

    pub async fn vault_history(&self, id: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.vault_history(&id) }))
            .await
    }

    pub async fn revert_item(
        &self,
        id: String,
        revision: String,
        expected_revision: String,
    ) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move {
                ensure(
                    c.vault_show(&id)?.revision == expected_revision,
                    Error::VersionConflict,
                )?;
                c.vault_revert(&id, &revision).map(|e| e.masked(false))
            })
        })
        .await
    }

    pub async fn vault_item(&self, id: String, revision: Option<String>) -> Result<VaultDetail> {
        let generation = self.host.generation();
        let item = self
            .host
            .run_at(generation, move |c| {
                Box::pin(async move {
                    let is_current = revision.is_none();
                    let mut item = match revision {
                        Some(revision) => c.vault_version(&id, &revision),
                        None => c.vault_show(&id),
                    }
                    .map(VaultItem::from)?;
                    if is_current {
                        item.conflict |=
                            dmsg_client::vault::reading::conflict_ids(c)?.contains(&id);
                    }
                    Ok(item)
                })
            })
            .await?;
        Ok(VaultDetail { generation, item })
    }

    pub async fn reveal_field(
        &self,
        id: String,
        revision: String,
        field: String,
    ) -> Result<SecretValue> {
        let generation = self.host.generation();
        self.host
            .run_at(generation, move |c| {
                Box::pin(async move {
                    let entry = c.vault_version(&id, &revision)?;
                    ensure(
                        !entry.item.reserved() && !c.vault_show(&id)?.item.reserved(),
                        Error::Forbidden,
                    )?;
                    let field = entry.item.field(&field).ok_or(Error::NotFound)?;
                    let (value, remaining_ms) = if field.kind == "otp" {
                        let code = dmsg_client::vault::otp::parse(&field.value)?
                            .code(dmsg_client::now_ms());
                        (code.code, Some(code.remaining_ms))
                    } else {
                        (field.value.clone(), None)
                    };
                    Ok(SecretValue {
                        generation,
                        value,
                        remaining_ms,
                    })
                })
            })
            .await
            .map_err(Into::into)
    }

    pub async fn save_item(
        &self,
        id: Option<String>,
        revision: Option<String>,
        edit: VaultEdit,
    ) -> Result<VaultDetail> {
        let generation = self.host.generation();
        let item = self
            .host
            .run_at(generation, move |c| {
                Box::pin(async move {
                    let changes = dmsg_client::vault::Changes {
                        title: Some(edit.title),
                        notes: Some(edit.notes),
                        favorite: Some(edit.favorite),
                        tags: Some(edit.tags),
                        files: edit.files,
                        fields: Some(
                            edit.fields
                                .into_iter()
                                .map(|f| dmsg_client::vault::FieldChange {
                                    id: Some(f.id),
                                    kind: Some(f.kind),
                                    label: Some(f.label),
                                    section: Some(f.section),
                                    value: f.value,
                                })
                                .collect(),
                        ),
                        ..Default::default()
                    };
                    let entry = if let Some(id) = id {
                        let base = revision
                            .as_deref()
                            .ok_or_else(|| dmsg_types::invalid("revision"))?;
                        c.vault_edit(&id, Some(base), &changes)?
                    } else {
                        c.vault_create(&edit.kind, &changes)?
                    };
                    Ok(VaultItem::from(entry))
                })
            })
            .await?;
        Ok(VaultDetail { generation, item })
    }

    pub async fn delete_item(&self, id: String) -> Result<()> {
        self.host
            .run(move |c| Box::pin(async move { c.vault_delete(&id).map(|_| ()) }))
            .await?;
        Ok(())
    }

    pub async fn vault_trash(&self, offset: u32, limit: u32) -> Result<VaultPage> {
        let generation = self.host.generation();
        let (items, has_more) = self
            .host
            .run_at(generation, move |c| {
                Box::pin(async move {
                    dmsg_client::vault::reading::page(
                        &c.cooperative_hold(),
                        &dmsg_client::vault::Filter {
                            trash: true,
                            query: None,
                        },
                        None,
                        false,
                        offset as usize,
                        limit as usize,
                    )
                    .await
                })
            })
            .await?;
        Ok(VaultPage {
            generation,
            items: items.into_iter().map(VaultItem::summary).collect(),
            has_more,
        })
    }

    pub async fn restore_item(&self, id: String) -> Result<()> {
        self.host
            .run(move |c| Box::pin(async move { c.vault_restore(&id).map(|_| ()) }))
            .await?;
        Ok(())
    }

    pub async fn conflicts(&self) -> Result<ObjectSnapshot> {
        self.object(|c| {
            Box::pin(async {
                Ok(c.vault_conflicts()?
                    .into_iter()
                    .map(|v| v.masked(false))
                    .collect::<Vec<_>>())
            })
        })
        .await
    }

    pub async fn resolve_conflict(
        &self,
        key: String,
        expected_revision: String,
        keep_current: bool,
    ) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move {
                let (id, _) = key
                    .split_once(':')
                    .ok_or_else(|| dmsg_types::invalid("conflict"))?;
                ensure(
                    c.vault_show(id)?.revision == expected_revision,
                    Error::VersionConflict,
                )?;
                c.vault_resolve(
                    &key,
                    &if keep_current {
                        dmsg_client::vault::Keep::Current
                    } else {
                        dmsg_client::vault::Keep::Theirs
                    },
                )
                .map(|entry| entry.masked(false))
            })
        })
        .await
    }

    pub async fn local_change(&self) -> Result<u64> {
        self.held(|h| Box::pin(async { h.with(|c| c.local_change()).await }))
            .await
    }

    pub async fn reconcile(&self, operation: Arc<OperationToken>) -> Result<ObjectSnapshot> {
        let _turn = self.approval_turn.lock().await;
        let turn = self.read_turns.lane("connect-read");
        operation
            .run(async {
                let shared = self
                    .held(move |h| {
                        Box::pin(async move {
                            // Without a login its jobs stay retryable while
                            // relay-only reconciliation still proceeds.
                            if h.with(|c| c.reconciliation_needs_login()).await? {
                                let _ = h.ensure_login().await;
                            }
                            let attestations =
                                dmsg_client::approval::reconcile_attestations(h).await?;
                            let wallets = dmsg_client::approval::reconcile_wallets(h).await?;
                            let _turn = turn.lock().await;
                            Ok(attestations
                                + wallets
                                + dmsg_client::connect::reconcile_reads(h).await?)
                        })
                    })
                    .await?;
                // Account/root mutations and payment journals retain their exclusive
                // runner. Ordinary background reads never enter that runner.
                self.object(move |c| {
                    Box::pin(async move { Ok(shared + c.approvals_reconcile_others().await?) })
                })
                .await
            })
            .await
    }

    pub async fn sync_connect(&self, operation: Arc<OperationToken>) -> Result<ObjectSnapshot> {
        let turn = self.read_turns.lane("connect-read");
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let _turn = turn.lock().await;
                    let fetched = dmsg_client::connect::fetch_mailbox(h).await?;
                    dmsg_client::connect::entries(h, fetched).await
                })
            }))
            .await
    }

    pub async fn run_backups(&self, operation: Arc<OperationToken>) -> Result<ObjectSnapshot> {
        let progress = operation.clone();
        let turn = self.read_turns.lane("files-backup");
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let _turn = turn.lock().await;
                    dmsg_client::library::backup::backup_run(h, &mut |job| {
                        progress.report(job.sent, job.chunks)
                    })
                    .await
                })
            }))
            .await
    }

    pub async fn backup_status(&self) -> Result<ObjectSnapshot> {
        self.object(|c| {
            Box::pin(async {
                let mut values = Vec::new();
                for job in c.files_backup_status()? {
                    let name = c
                        .files_show(&job.file)
                        .ok()
                        .map(|details| details.file.name);
                    let mut value = json!(job);
                    value["name"] = json!(name);
                    values.push(value);
                }
                Ok(values)
            })
        })
        .await
    }

    pub async fn channels(&self) -> Result<ChannelPage> {
        let generation = self.host.generation();
        let items = self
            .host
            .run_at(generation, |c| {
                Box::pin(async {
                    let account = c.session_state().account.ok_or(Error::AuthRequired)?;
                    let mut items = c
                        .channel_list()?
                        .into_iter()
                        .filter(|v| !v.hidden)
                        .map(|v| {
                            let pending = c.message_intents(&v.id)?;
                            let can_publish = v.ledger.as_ref().is_some_and(|l| {
                                dmsg_client::channel_log::may_publish(l, &account)
                            });
                            let can_invite = v
                                .ledger
                                .as_ref()
                                .is_some_and(|l| messaging::can_invite_to_channel(l, &account));
                            Ok(ChannelItem {
                                last_at: v
                                    .last
                                    .as_ref()
                                    .map_or(0, |m| m.stored_at)
                                    .max(pending.iter().map(|i| i.created_at).max().unwrap_or(0))
                                    .max(v.active_at),
                                news: v.news,
                                muted: v.muted,
                                status: v
                                    .ledger
                                    .as_ref()
                                    .map(|l| match l.status {
                                        dmsg_client::channel_log::Status::Active => "active",
                                        dmsg_client::channel_log::Status::Archived => "archived",
                                        _ => "rotation_required",
                                    })
                                    .unwrap_or("unavailable")
                                    .into(),
                                can_publish,
                                can_invite,
                                pending_sends: pending.len() as u64,
                                read_through: v.read,
                                id: v.id,
                                name: v.name,
                                unread: v.unread,
                                draft: v.draft,
                                preview: v.last.map(|m| m.text).unwrap_or_default(),
                            })
                        })
                        .collect::<dmsg_types::Result<Vec<_>>>()?;
                    items.sort_by(|a, b| {
                        b.last_at
                            .cmp(&a.last_at)
                            .then(a.name.cmp(&b.name))
                            .then(a.id.cmp(&b.id))
                    });
                    Ok(items)
                })
            })
            .await?;
        Ok(ChannelPage { generation, items })
    }

    pub async fn create_channel(
        &self,
        name: String,
        operation: Arc<OperationToken>,
    ) -> Result<String> {
        operation
            .run(async {
                self.host
                    .run(move |c| {
                        Box::pin(async move {
                            c.channel_create(
                                &name,
                                dmsg_client::channel_log::ChannelType::Collaboration,
                            )
                            .await
                        })
                    })
                    .await
                    .map_err(Into::into)
            })
            .await
    }

    pub async fn join_channel(
        &self,
        invitation: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(invitation.len() <= 16 * 1024, "invitation")?;
        let bytes = if let Some(fragment) = invitation.trim().strip_prefix("dmsg://channel/join#") {
            dmsg_client::codec::unb64(fragment)?
        } else {
            invitation.into_bytes()
        };
        let invitation: dmsg_client::channel::Invitation =
            serde_json::from_slice(&bytes).map_err(|_| dmsg_types::invalid("invitation"))?;
        operation
            .run(self.object(move |c| Box::pin(async move { c.channel_join(&invitation).await })))
            .await
    }

    pub async fn invite_channel(
        &self,
        channel: String,
        account: String,
        operation: Arc<OperationToken>,
    ) -> Result<String> {
        let account = account
            .parse()
            .map_err(|_| dmsg_types::invalid("account"))?;
        operation
            .run(async {
                self.host
                    .run(move |c| {
                        Box::pin(async move {
                            let invitation = dmsg_client::channel::invite(
                                &c.cooperative_hold(),
                                &channel,
                                &account,
                                dmsg_client::channel_log::Role::Member,
                            )
                            .await?;
                            let bytes = serde_json::to_vec(&invitation)
                                .map_err(|_| Error::IntegrityFailed)?;
                            Ok(format!(
                                "dmsg://channel/join#{}",
                                dmsg_client::codec::b64(&bytes)
                            ))
                        })
                    })
                    .await
                    .map_err(Into::into)
            })
            .await
    }

    pub async fn messages(
        &self,
        channel: String,
        before: Option<u64>,
        limit: u32,
    ) -> Result<MessagePage> {
        ensure_valid(limit > 0 && limit <= 100, "page")?;
        let generation = self.host.generation();
        let page = self
            .host
            .run_at(generation, move |c| {
                Box::pin(async move {
                    c.channel_message_page(&channel, before.unwrap_or(u64::MAX), limit as usize)
                })
            })
            .await?;
        Ok(MessagePage {
            generation,
            items: page.items.into_iter().map(MessageItem::from).collect(),
            has_more: page.has_more,
            next_before: page.next_before,
        })
    }

    pub async fn earlier_messages(
        &self,
        channel: String,
        before: u64,
        limit: u32,
        operation: Arc<OperationToken>,
    ) -> Result<MessagePage> {
        ensure_valid(limit > 0 && limit <= 100, "page")?;
        let generation = self.host.generation();
        let turn = self.read_turns.lane(&format!("channel:{channel}"));
        let page = operation
            .run(self.held(move |h| {
                Box::pin(async move {
                    let _turn = turn.lock().await;
                    dmsg_client::channel::earlier_messages(h, &channel, before, limit as usize)
                        .await
                })
            }))
            .await?;
        Ok(MessagePage {
            generation,
            items: page.items.into_iter().map(MessageItem::from).collect(),
            has_more: page.has_more,
            next_before: page.next_before,
        })
    }

    pub async fn mark_read(&self, channel: String, through: u64) -> Result<()> {
        self.host
            .run(move |c| Box::pin(async move { c.channel_mark(&channel, through).map(|_| ()) }))
            .await?;
        Ok(())
    }

    pub async fn save_message_drafts(&self, drafts: Vec<MessageDraft>) -> Result<()> {
        ensure_valid(drafts.len() <= 100, "drafts")?;
        self.host
            .run(move |c| {
                Box::pin(async move {
                    c.save_message_drafts(
                        &drafts
                            .into_iter()
                            .map(|d| (d.channel, d.text))
                            .collect::<Vec<_>>(),
                    )
                })
            })
            .await?;
        Ok(())
    }

    pub async fn save_draft(&self, channel: String, text: String) -> Result<()> {
        self.host
            .run(move |c| Box::pin(async move { c.channel_draft(&channel, &text) }))
            .await?;
        Ok(())
    }

    pub async fn queue_message(&self, channel: String, text: String) -> Result<String> {
        self.host
            .run(move |c| Box::pin(async move { c.queue_message(&channel, &text).map(|i| i.id) }))
            .await
            .map_err(Into::into)
    }

    pub async fn queue_attachment(
        &self,
        channel: String,
        file: String,
        text: String,
    ) -> Result<String> {
        self.host
            .run(move |c| {
                Box::pin(async move { c.queue_attachment(&channel, &file, &text).map(|i| i.id) })
            })
            .await
            .map_err(Into::into)
    }

    pub async fn outbox(&self, channel: String) -> Result<OutboxPage> {
        let generation = self.host.generation();
        let items = self
            .host
            .run_at(generation, move |c| {
                Box::pin(async move {
                    Ok(c.message_intents(&channel)?
                        .into_iter()
                        .map(OutgoingMessage::from)
                        .collect())
                })
            })
            .await?;
        Ok(OutboxPage { generation, items })
    }

    pub async fn send_message(
        &self,
        channel: String,
        message_id: String,
        operation: Arc<OperationToken>,
    ) -> Result<SentMessage> {
        let generation = self.host.generation();
        let message = operation
            .run(self.held(move |h| {
                Box::pin(async move {
                    let value = dmsg_client::outbox::send(h, &channel, &message_id).await?;
                    let message: dmsg_client::channel::Message =
                        serde_json::from_value(value["message"].clone())
                            .map_err(|_| Error::IntegrityFailed)?;
                    Ok(MessageItem::from(message))
                })
            }))
            .await?;
        Ok(SentMessage {
            generation,
            message,
        })
    }

    pub async fn refresh_channel(
        &self,
        channel: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        let turn = self.read_turns.lane(&format!("channel:{channel}"));
        let control = self.read_turns.lane(&format!("channel-control:{channel}"));
        let requested = dmsg_client::now_ms();
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let _turn = turn.lock().await;
                    if h.with(|c| {
                        Ok(c.channel_observed(&channel)?
                            .is_some_and(|at| at >= requested))
                    })
                    .await?
                    {
                        return h
                            .with(|c| {
                                Ok(dmsg_client::channel::ChannelSync {
                                    through: c.channel_view(&channel)?.latest_cursor,
                                    received: 0,
                                })
                            })
                            .await;
                    }
                    let result = dmsg_client::channel::sync_recent(h, &channel).await;
                    drop(_turn);
                    hints::maintain_channel(h, &control, &channel).await;
                    result
                })
            }))
            .await
    }

    pub async fn refresh_channel_attachments(
        &self,
        channel: String,
        operation: Arc<OperationToken>,
    ) -> Result<()> {
        operation
            .run(self.held(move |h| {
                Box::pin(
                    async move { dmsg_client::channel::sync_file_deletions(h, &channel).await },
                )
            }))
            .await
    }

    pub async fn save_attachment(
        &self,
        channel: String,
        sequence: u64,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        let progress = operation.clone();
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    dmsg_client::channel::save_regular_shared(
                        h,
                        &channel,
                        sequence,
                        "root",
                        &mut |done, total| progress.report(done, total),
                    )
                    .await
                })
            }))
            .await
    }

    pub async fn cached_inbox_status(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { h.with(|c| c.inbox_observation()).await }))
            .await
    }

    pub async fn inbox(&self, refresh: bool) -> Result<ObjectSnapshot> {
        let requested = dmsg_client::now_ms();
        let turn = self.read_turns.lane("inbox");
        let sync_turn = self.read_turns.lane("sync");
        let tracker = self.sync_tracker.clone();
        let generation = self.host.generation();
        self.held_object(move |h| {
            Box::pin(async move {
                if refresh {
                    let _turn = turn.lock().await;
                    if h.with(|c| {
                        Ok(c.inbox_observation()?["observed_at_ms"]
                            .as_u64()
                            .is_some_and(|at| at >= requested))
                    })
                    .await?
                    {
                        return h.with(|c| c.inbox_cached()).await;
                    }
                    match dmsg_client::inbox::list(h).await {
                        Err(Error::RecoveryIncomplete) => {
                            // The receiving key belongs to encrypted account content.
                            // A newly authorized device may open the inbox before its
                            // first content sync. Fetch it without uploading local work,
                            // then retry the same uncommitted inbox page.
                            {
                                let _sync = sync_turn.lock().await;
                                // A pass that finished while we waited already checked
                                // the missing key. Coalesce it; otherwise receive only
                                // new content, with the ordinary cooldown and status.
                                sync::content_pass(
                                    h,
                                    &tracker,
                                    generation,
                                    requested,
                                    sync::Trigger::Inbox,
                                )
                                .await?;
                            }
                            dmsg_client::inbox::list(h).await
                        }
                        result => result,
                    }
                } else {
                    h.with(|c| c.inbox_cached()).await
                }
            })
        })
        .await
    }

    pub async fn mark_letter(
        &self,
        order: String,
        read: bool,
        archived: bool,
    ) -> Result<ObjectSnapshot> {
        let turn = self.read_turns.lane(&format!("inbox-mark:{order}"));
        self.held_object(move |h| {
            Box::pin(async move {
                let _turn = turn.lock().await;
                dmsg_client::inbox::mark_contact(h, &order, read, archived, false).await
            })
        })
        .await
    }

    pub async fn maintain_files(&self) -> Result<ObjectSnapshot> {
        self.object(|c| {
            Box::pin(async {
                let now = dmsg_client::now_ms();
                c.files_maintain(now)
            })
        })
        .await
    }

    pub async fn files(&self, folder: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.files_list(&folder, false) }))
            .await
    }

    /// What only this device holds: file versions without a cloud backup,
    /// changes waiting to sync and when a sync last succeeded.
    pub async fn local_only(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.local_only() })).await
    }

    pub async fn file_details(&self, id: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.files_show(&id) }))
            .await
    }

    pub async fn open_file(
        &self,
        id: String,
        version: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(
                    async move { dmsg_client::library::open::open(h, &id, Some(&version)).await },
                )
            }))
            .await
    }

    pub async fn close_file(&self, nonce: String) -> Result<()> {
        self.host
            .run(move |c| {
                Box::pin(async move {
                    c.files_open_cleanup(Some(&nonce), false, &mut |_, _| {})
                        .map(|_| ())
                })
            })
            .await?;
        Ok(())
    }

    pub async fn import_file(
        &self,
        name: String,
        folder: String,
        shared_id: Option<String>,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        let path = self.staging.join(&name);
        let handle = PathBuf::from(&name);
        let parts: Vec<_> = handle.components().collect();
        ensure_valid(
            parts.len() == 3
                && parts
                    .iter()
                    .all(|p| matches!(p, std::path::Component::Normal(_)))
                && parts[1]
                    .as_os_str()
                    .to_str()
                    .is_some_and(|s| s.starts_with("dmsg-import-")),
            "import handle",
        )?;
        let owner = parts[0]
            .as_os_str()
            .to_str()
            .ok_or_else(|| dmsg_types::invalid("import account"))?
            .parse::<dmsg_types::AccountId>()
            .map_err(|_| dmsg_types::invalid("import account"))?
            .to_string();
        let directory = self
            .staging
            .canonicalize()
            .map_err(|_| dmsg_types::invalid("staging"))?;
        let canonical = path
            .canonicalize()
            .map_err(|_| dmsg_types::invalid("import"))?;
        ensure_valid(
            canonical.parent().and_then(|p| p.parent()) == Some(directory.join(&owner).as_path()),
            "import path",
        )?;
        let progress = operation.clone();
        let receipt = if let Some(shared) = shared_id {
            ensure_valid(
                shared.len() == 36 && shared.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'),
                "shared import ID",
            )?;
            hex::encode(*dmsg_client::codec::sha256(
                format!("shared:{shared}").as_bytes(),
            ))
        } else {
            hex::encode(*dmsg_client::codec::sha256(name.as_bytes()))
        };
        operation
            .run(self.object(move |c| {
                Box::pin(async move {
                    ensure(c.session_state().state == "unlocked", Error::Locked)?;
                    ensure(
                        c.session_state().account.as_deref() == Some(owner.as_str()),
                        Error::Forbidden,
                    )?;
                    dmsg_client::library::import::file_once(
                        &c.cooperative_hold(),
                        &path,
                        &folder,
                        true,
                        Some(&receipt),
                        &mut |done, total| progress.report(done, total),
                    )
                    .await
                })
            }))
            .await
    }

    pub async fn backup_file(
        &self,
        id: String,
        expected_version: String,
    ) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(async move {
                dmsg_client::library::backup::backup_version(h, &id, &expected_version).await
            })
        })
        .await
    }

    pub async fn create_folder(&self, parent: String, name: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.files_mkdir(&parent, &name) }))
            .await
    }

    pub async fn trash_file(&self, id: String) -> Result<()> {
        self.host
            .run(move |c| Box::pin(async move { c.files_trash(&[id]) }))
            .await?;
        Ok(())
    }

    pub async fn file_trash(&self) -> Result<ObjectSnapshot> {
        self.object(|c| {
            Box::pin(async {
                let mut value = json!(c.files_list("root", true)?);
                value["trash_days"] = json!(c.config().files_trash_days);
                Ok(value)
            })
        })
        .await
    }

    pub async fn restore_file(&self, id: String) -> Result<()> {
        self.host
            .run(move |c| Box::pin(async move { c.files_restore(&[id]) }))
            .await?;
        Ok(())
    }

    pub async fn delete_backup(
        &self,
        upload: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(
                    async move { dmsg_client::library::cloud::cloud_delete(h, &[upload]).await },
                )
            }))
            .await
    }

    pub async fn cloud_backup_status(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { h.with(|c| c.files_cloud_observation()).await }))
            .await
    }

    pub async fn cloud_backups(
        &self,
        refresh: bool,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        let turn = self.read_turns.lane("cloud-inventory");
        let requested = dmsg_client::now_ms();
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let session = h.with(|c| c.display_session()).await?;
                    let scoped = dmsg_client::display::Scoped { inner: h, session };
                    let h = &scoped;
                    if refresh {
                        let _turn = turn.lock().await;
                        let last = h.with(|c| c.files_cloud_observation()).await?;
                        if last["observed_at_ms"]
                            .as_u64()
                            .is_none_or(|at| at < requested)
                        {
                            dmsg_client::library::cloud::cloud_refresh(h).await?;
                        }
                    }
                    h.with(|c| c.files_cloud_list()).await
                })
            }))
            .await
    }

    pub async fn restore_backup(
        &self,
        upload: String,
        folder: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    h.with(|c| c.files_download_start(&upload, Some(&folder), false))
                        .await
                })
            }))
            .await
    }

    pub async fn download_status(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { h.with(|c| c.files_download_status()).await }))
            .await
    }

    pub async fn retry_file_transfers(&self) -> Result<()> {
        self.held(|h| {
            Box::pin(async {
                h.with(|c| {
                    c.files_backup_retry(None, None)?;
                    c.files_download_retry(None)
                })
                .await
            })
        })
        .await
    }

    pub async fn cancel_download(&self, upload: String) -> Result<()> {
        self.held(move |h| {
            Box::pin(async move { h.with(|c| c.files_download_cancel(&upload)).await })
        })
        .await
    }

    pub async fn run_downloads(&self, operation: Arc<OperationToken>) -> Result<ObjectSnapshot> {
        let turn = self.read_turns.lane("files-download");
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let _turn = turn.lock().await;
                    dmsg_client::library::fetch::run(h, &mut |_| {}).await
                })
            }))
            .await
    }

    pub async fn approve_pairing_request(
        &self,
        token: String,
        device: String,
    ) -> Result<ObjectSnapshot> {
        self.held(|h| Box::pin(async { dmsg_client::hold::Hold::ensure_login(h).await }))
            .await?;
        self.object(move |c| {
            Box::pin(async move {
                dmsg_client::approval::add_device_request(
                    &c.cooperative_hold(),
                    "gui",
                    &token,
                    &device,
                )
                .await
            })
        })
        .await
    }

    pub async fn inspect_pairing_request(&self, token: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.pairing_preview(&token) }))
            .await
    }

    pub async fn security(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { dmsg_client::security::account_security(h).await }))
            .await
    }

    pub async fn profile(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { dmsg_client::principal::show(h).await }))
            .await
    }

    /// Pure local avatar read; an unobserved image is absent, never fetched here.
    pub async fn cached_profile_avatar(&self, url: String) -> Result<Option<Vec<u8>>> {
        self.held(move |h| {
            Box::pin(async move {
                h.with(|c| Ok(c.profile_avatar(&url)?.map(|a| a.bytes)))
                    .await
            })
        })
        .await
    }

    /// Refresh the current profile image separately from its document. Failed
    /// image reads keep a valid cached image and cannot delay the text snapshot.
    pub async fn profile_avatar(&self, url: String) -> Result<Option<Vec<u8>>> {
        let turn = self.read_turns.lane("profile-avatar");
        self.held(move |h| {
            Box::pin(async move {
                let session = h.with(|c| c.display_session()).await?;
                let scoped = dmsg_client::display::Scoped { inner: h, session };
                let h = &scoped;
                let _turn = turn.lock().await;
                dmsg_client::snapshot::refresh_avatar(h).await;
                h.with(|c| Ok(c.profile_avatar(&url)?.map(|a| a.bytes)))
                    .await
            })
        })
        .await
    }

    pub async fn publish_profile(
        &self,
        name: String,
        description: String,
        expected_updated_at: i64,
        avatar_url: Option<String>,
        links_json: Option<String>,
    ) -> Result<ObjectSnapshot> {
        self.held(|h| Box::pin(async { h.ensure_login().await }))
            .await?;
        let text = |value: String| Some(value.trim().to_string()).filter(|v| !v.is_empty());
        let (name, description) = (text(name), text(description));
        self.object(move |c| {
            Box::pin(async move {
                let mut document = dmsg_client::principal::editable(&c.cooperative_hold()).await?;
                ensure(
                    document.updated_at == expected_updated_at,
                    Error::VersionConflict,
                )?;
                document.name = name;
                document.description = description;
                if let Some(avatar) = avatar_url {
                    document.avatar_url = (!avatar.is_empty()).then_some(avatar);
                }
                if let Some(links) = links_json {
                    document.links = serde_json::from_str(&links)
                        .map_err(|_| dmsg_types::invalid("profile links"))?;
                }
                let info = c.publish_document(&document).await?;
                Ok(serde_json::json!({
                    "version": info.state.version,
                    "updated_at": info.state.updated_at,
                    "published_version": info.published_version,
                    "published": info.state.version == info.published_version,
                }))
            })
        })
        .await
    }

    pub async fn profile_publication(&self) -> Result<ObjectSnapshot> {
        self.held(|h| Box::pin(async { h.ensure_login().await }))
            .await?;
        self.object(|c| {
            Box::pin(async {
                let pending = c.document_pending()?;
                let info = c.principal().await?;
                Ok(serde_json::json!({"pending": pending,
                "published": info.as_ref().is_none_or(|i| i.state.version == i.published_version),
                "updated_at": info.map(|i| i.state.updated_at)}))
            })
        })
        .await
    }

    pub async fn retry_profile_publication(&self) -> Result<ObjectSnapshot> {
        self.held(|h| Box::pin(async { h.ensure_login().await }))
            .await?;
        self.object(|c| {
            Box::pin(async {
                let info = c.retry_document().await?;
                Ok(serde_json::json!({"updated_at": info.state.updated_at,
                "published": info.state.version == info.published_version}))
            })
        })
        .await
    }

    pub async fn create_binding_request(&self, account: String) -> Result<ObjectSnapshot> {
        self.held(|h| Box::pin(async { h.ensure_login().await }))
            .await?;
        let account = account
            .parse()
            .map_err(|_| dmsg_types::invalid("account"))?;
        self.object(move |c| Box::pin(async move { c.bind_request(&account).await }))
            .await
    }

    pub async fn accept_binding(&self, account: String) -> Result<SessionSnapshot> {
        let account = account
            .parse()
            .map_err(|_| dmsg_types::invalid("account"))?;
        let transition = self.transition_with_login().await?;
        let generation = transition.generation();
        let prepared = transition
            .run(move |c| Box::pin(async move { c.prepare_bind_accept(&account).await }))
            .await?;
        self.finish_login_unlock(generation, prepared).await?;
        transition.finish()?;
        self.session_state().await
    }

    pub async fn review_binding(&self, token: String) -> Result<ObjectSnapshot> {
        self.held(|h| Box::pin(async { h.ensure_login().await }))
            .await?;
        self.object(move |c| {
            Box::pin(async move {
                let binding = c.inspect_binding(&token)?;
                dmsg_client::security::account_request(
                    &c.cooperative_hold(),
                    "gui",
                    dmsg_client::security::AccountChange::BindLogin {
                        principal: binding.principal.to_text(),
                        nonce: binding.nonce,
                    },
                )
                .await
            })
        })
        .await
    }

    pub async fn remove_login(&self, principal: String) -> Result<ObjectSnapshot> {
        self.held(|h| Box::pin(async { h.ensure_login().await }))
            .await?;
        self.object(move |c| {
            Box::pin(async move {
                dmsg_client::security::account_request(
                    &c.cooperative_hold(),
                    "gui",
                    dmsg_client::security::AccountChange::RemoveLogin { principal },
                )
                .await
            })
        })
        .await
    }

    pub async fn devices(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| {
            Box::pin(async {
                let state = h
                    .warm(dmsg_client::client::Warm::Home)
                    .await?
                    .ok_or(Error::PolicyStale)?;
                h.with(|c| c.account_view_of(&state)).await
            })
        })
        .await
    }

    pub async fn revoke_device(&self, device: String) -> Result<ObjectSnapshot> {
        self.held(|h| Box::pin(async { dmsg_client::hold::Hold::ensure_login(h).await }))
            .await?;
        let bytes = hex::decode(device)
            .ok()
            .and_then(|v| <[u8; 32]>::try_from(v).ok())
            .ok_or_else(|| dmsg_types::invalid("device"))?;
        self.object(move |c| {
            Box::pin(async move {
                dmsg_client::approval::revoke_request(
                    &c.cooperative_hold(),
                    "gui",
                    dmsg_types::Hash::new(bytes),
                )
                .await
            })
        })
        .await
    }

    pub async fn change_recovery_delay(&self, days: u32) -> Result<ObjectSnapshot> {
        self.held(|h| Box::pin(async { h.ensure_login().await }))
            .await?;
        ensure_valid((1..=7).contains(&days), "recovery delay")?;
        self.object(move |c| {
            Box::pin(async move {
                dmsg_client::security::account_request(
                    &c.cooperative_hold(),
                    "gui",
                    dmsg_client::security::AccountChange::RecoveryDelay {
                        delay_ms: u64::from(days) * dmsg_types::DAY,
                    },
                )
                .await
            })
        })
        .await
    }

    /// Ask the person to change recovery; root rotation is part of approval.
    pub async fn change_recovery_policy(&self, enabled: bool, days: u32) -> Result<ObjectSnapshot> {
        self.held(|h| Box::pin(async { h.ensure_login().await }))
            .await?;
        ensure_valid((1..=7).contains(&days), "recovery delay")?;
        self.object(move |c| {
            Box::pin(async move {
                dmsg_client::security::account_request(
                    &c.cooperative_hold(),
                    "gui",
                    dmsg_client::security::AccountChange::RecoveryPolicy {
                        enabled,
                        delay_ms: u64::from(days) * dmsg_types::DAY,
                    },
                )
                .await
            })
        })
        .await
    }

    pub async fn dispute_recovery(&self, operation: String) -> Result<ObjectSnapshot> {
        self.held(|h| Box::pin(async { h.ensure_login().await }))
            .await?;
        let bytes = hex::decode(operation)
            .ok()
            .and_then(|v| <[u8; 32]>::try_from(v).ok())
            .ok_or_else(|| dmsg_types::invalid("operation"))?;
        self.object(move |c| {
            Box::pin(async move {
                dmsg_client::security::account_request(
                    &c.cooperative_hold(),
                    "gui",
                    dmsg_client::security::AccountChange::DisputeRecovery {
                        op_id: dmsg_types::Hash::new(bytes),
                    },
                )
                .await
            })
        })
        .await
    }

    pub async fn recovery_status(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { dmsg_client::recovery::status(h).await }))
            .await
    }

    pub async fn request_recovery(&self) -> Result<ObjectSnapshot> {
        self.held(|h| Box::pin(async { h.ensure_login().await }))
            .await?;
        self.object(|c| Box::pin(async { c.recovery_request().await }))
            .await
    }

    pub async fn complete_recovery(&self) -> Result<SessionSnapshot> {
        let transition = self.transition_with_login().await?;
        let generation = transition.generation();
        let prepared = transition
            .run(|c| Box::pin(async { c.prepare_recovery_complete().await }))
            .await?;
        self.finish_login_unlock(generation, prepared).await?;
        transition
            .run(|c| Box::pin(async { c.recovery_complete().await }))
            .await?;
        transition.finish()?;
        self.session_state().await
    }
}
