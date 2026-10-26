//! Native mobile API. The generated API exposes named operations, not daemon RPC.
mod approvals;
mod diagnostics;
mod error;
mod models;
mod operation;
mod platform;
mod recovery;
mod secrets;
mod security;
mod session;

use dmsg_client::hold::Hold;
use dmsg_client::{Client, config::Config, host::Host, login::LoginSession};
use dmsg_types::{Error, ensure, ensure_valid};
pub use error::MobileError;
use error::Result;
pub use models::*;
pub use operation::OperationToken;
pub use platform::{Agreement, Platform, SigningKey};
pub use secrets::{SecretAttachmentSelection, SecretPackageSelection, SecretSnapshotInput};
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
}

struct PendingLogin {
    session: LoginSession,
    binding_generation: Option<u64>,
}
#[derive(Default)]
struct LoginState {
    revision: u64,
    pending: Option<PendingLogin>,
}

impl MobileEngine {
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
}

#[uniffi::export]
impl MobileEngine {
    #[uniffi::constructor]
    pub async fn open(
        directory: String,
        staging: String,
        platform: Box<dyn Platform>,
    ) -> Result<Arc<Self>> {
        diagnostics::init();
        let platform: Arc<dyn Platform> = Arc::from(platform);
        let adapter = Arc::new(platform::Adapter(platform.clone()));
        let mut config = Config::production()?;
        config.files_max_bytes = 100 * 1024 * 1024;
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
                client.set_approval_keys(adapter);
                client.require_fast_presence();
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
        }))
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
        }))
    }

    pub async fn lock(&self) -> Result<SessionSnapshot> {
        self.lock_now();
        self.session_state().await
    }

    pub fn lock_now(&self) {
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
        transition.run(|c| Box::pin(async { c.logout() })).await?;
        transition.finish()?;
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

    pub async fn begin_login(&self) -> Result<LoginStart> {
        self.cancel_login();
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
        state.pending = Some(PendingLogin {
            session,
            binding_generation: None,
        });
        Ok(started)
    }

    pub fn cancel_login(&self) {
        if let Ok(mut login) = self.login.lock() {
            login.revision = login.revision.wrapping_add(1);
            login.pending = None;
        }
    }

    pub async fn begin_binding(&self) -> Result<LoginStart> {
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
        let pending = self
            .login
            .lock()
            .map_err(|_| Error::Locked)?
            .pending
            .take()
            .ok_or(Error::Expired)?;
        let generation = pending.binding_generation.ok_or(Error::Forbidden)?;
        let delegated = pending
            .session
            .accept(&dmsg_client::codec::unb64(fragment)?, dmsg_client::now_ms())?;
        let json = self
            .host
            .run_at(generation, move |c| {
                Box::pin(async move {
                    let view = c
                        .bind_login("gui", delegated.identity, delegated.expires_at)
                        .await?;
                    serde_json::to_string(&view).map_err(|_| Error::IntegrityFailed)
                })
            })
            .await?;
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
        let session = self
            .login
            .lock()
            .map_err(|_| Error::Locked)?
            .pending
            .take()
            .ok_or(Error::Expired)?;
        ensure(session.binding_generation.is_none(), Error::Forbidden)?;
        let bytes = dmsg_client::codec::unb64(fragment)?;
        let delegated = session.session.accept(&bytes, dmsg_client::now_ms())?;
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
        let transition = self.host.begin_transition()?;
        self.platform.cancel_presence();
        transition
            .run(|c| Box::pin(async { c.initialize_account().await }))
            .await?;
        transition.finish()?;
        self.session_state().await
    }

    pub async fn pair_request(&self) -> Result<ObjectSnapshot> {
        self.object(|c| {
            Box::pin(async {
                c.pair_request(dmsg_types::user::ControllerRole::Administrator)
                    .await
            })
        })
        .await
    }

    pub async fn refresh_account(&self) -> Result<SessionSnapshot> {
        let transition = self.host.begin_transition()?;
        self.platform.cancel_presence();
        let generation = transition.generation();
        let prepared = transition
            .run(|c| Box::pin(async { c.prepare_unlock().await }))
            .await?;
        self.finish_login_unlock(generation, prepared).await?;
        transition.finish()?;
        self.session_state().await
    }

    pub async fn vault_page(&self, query: String, offset: u32, limit: u32) -> Result<VaultPage> {
        ensure_valid(query.len() <= 1000 && limit > 0 && limit <= 100, "page")?;
        let generation = self.host.generation();
        let items = self
            .host
            .run_at(generation, move |c| {
                Box::pin(async move {
                    Ok(c.vault_list(&dmsg_client::vault::Filter {
                        trash: false,
                        query: Some(query),
                    })?
                    .into_iter()
                    .skip(offset as usize)
                    .take(limit as usize + 1)
                    .map(VaultItem::summary)
                    .collect::<Vec<_>>())
                })
            })
            .await?;
        let has_more = items.len() > limit as usize;
        Ok(VaultPage {
            generation,
            items: items.into_iter().take(limit as usize).collect(),
            has_more,
        })
    }

    pub async fn vault_item(&self, id: String, revision: Option<String>) -> Result<VaultDetail> {
        let generation = self.host.generation();
        let item = self
            .host
            .run_at(generation, move |c| {
                Box::pin(async move {
                    match revision {
                        Some(revision) => c.vault_version(&id, &revision),
                        None => c.vault_show(&id),
                    }
                    .map(VaultItem::from)
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
                        files: edit.files,
                        fields: Some(
                            edit.fields
                                .into_iter()
                                .map(|f| dmsg_client::vault::FieldChange {
                                    id: Some(f.id),
                                    kind: Some(f.kind),
                                    label: Some(f.label),
                                    value: f.value,
                                    ..Default::default()
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
        ensure_valid(limit > 0 && limit <= 100, "page")?;
        let generation = self.host.generation();
        let mut items: Vec<VaultItem> = self
            .host
            .run_at(generation, move |c| {
                Box::pin(async move {
                    Ok(c.vault_list(&dmsg_client::vault::Filter {
                        trash: true,
                        query: None,
                    })?
                    .into_iter()
                    .skip(offset as usize)
                    .take(limit as usize + 1)
                    .map(VaultItem::summary)
                    .collect())
                })
            })
            .await?;
        let has_more = items.len() > limit as usize;
        items.truncate(limit as usize);
        Ok(VaultPage {
            generation,
            items,
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
        keep_current: bool,
    ) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move {
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

    pub async fn sync(&self, operation: Arc<OperationToken>) -> Result<ObjectSnapshot> {
        operation
            .run(self.object(|c| {
                Box::pin(async { dmsg_client::sync::sync(&c.cooperative_hold()).await })
            }))
            .await
    }

    pub async fn reconcile(&self, operation: Arc<OperationToken>) -> Result<ObjectSnapshot> {
        operation
            .run(self.object(|c| Box::pin(async { c.approvals_reconcile().await })))
            .await
    }

    pub async fn sync_connect(&self, operation: Arc<OperationToken>) -> Result<ObjectSnapshot> {
        operation
            .run(self.object(|c| {
                Box::pin(async {
                    let fetched =
                        dmsg_client::connect::fetch_mailbox(&c.cooperative_hold()).await?;
                    c.connect_entries(fetched).await
                })
            }))
            .await
    }

    pub async fn run_backups(&self, operation: Arc<OperationToken>) -> Result<ObjectSnapshot> {
        operation
            .run(self.object(|c| {
                Box::pin(async {
                    dmsg_client::library::backup::backup_run(
                        &c.cooperative_hold(),
                        Some(15_000),
                        &mut |_, _, _| {},
                    )
                    .await
                })
            }))
            .await
    }

    pub async fn backup_status(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.files_backup_status() }))
            .await
    }

    pub async fn channels(&self) -> Result<ChannelPage> {
        let generation = self.host.generation();
        let items = self
            .host
            .run_at(generation, |c| {
                Box::pin(async {
                    Ok(c.channel_list()?
                        .into_iter()
                        .filter(|v| !v.hidden)
                        .map(|v| ChannelItem {
                            id: v.id,
                            name: v.name,
                            unread: v.unread,
                            draft: v.draft,
                            preview: v.last.map(|m| m.text).unwrap_or_default(),
                        })
                        .collect())
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
        let mut items: Vec<MessageItem> = self
            .host
            .run_at(generation, move |c| {
                Box::pin(async move {
                    Ok(c.channel_messages_before(
                        &channel,
                        before.unwrap_or(u64::MAX),
                        limit as usize + 1,
                    )?
                    .into_iter()
                    .map(|m| MessageItem {
                        id: m.message_id,
                        sequence: m.seq,
                        account: m.account,
                        text: m.text,
                        created_at_ms: m.created_at,
                        attachment_name: m.file.as_ref().map(|f| f.name.clone()),
                        attachment_mime: m.file.as_ref().map(|f| f.mime.clone()),
                        attachment_size: m.file.as_ref().map(|f| f.size),
                        unreadable: m.unreadable,
                    })
                    .collect())
                })
            })
            .await?;
        let has_more = items.len() > limit as usize;
        if has_more {
            items.remove(0);
        }
        Ok(MessagePage {
            generation,
            items,
            has_more,
        })
    }

    pub async fn mark_read(&self, channel: String, through: u64) -> Result<()> {
        self.host
            .run(move |c| Box::pin(async move { c.channel_mark(&channel, through).map(|_| ()) }))
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
                        .map(|i| OutgoingMessage {
                            id: i.id,
                            text: i.text,
                            attachment_name: i.attachment.map(|f| f.name),
                            attempted: i.attempted,
                            error: i.error,
                        })
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
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.object(move |c| {
                Box::pin(async move {
                    dmsg_client::outbox::send(&c.cooperative_hold(), &channel, &message_id).await
                })
            }))
            .await
    }

    pub async fn refresh_channel(
        &self,
        channel: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.object(move |c| {
                Box::pin(async move {
                    dmsg_client::channel::sync(&c.cooperative_hold(), &channel, false).await
                })
            }))
            .await
    }

    pub async fn save_attachment(
        &self,
        channel: String,
        sequence: u64,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.object(move |c| {
                Box::pin(async move {
                    dmsg_client::channel::save_regular_shared(
                        &c.cooperative_hold(),
                        &channel,
                        sequence,
                        "root",
                        &mut |_, _| {},
                    )
                    .await
                })
            }))
            .await
    }

    pub async fn inbox(&self, refresh: bool) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move {
                if refresh {
                    dmsg_client::inbox::list(&c.cooperative_hold()).await
                } else {
                    c.inbox_cached()
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
        self.object(move |c| {
            Box::pin(async move {
                dmsg_client::inbox::mark(&c.cooperative_hold(), &order, read, archived).await
            })
        })
        .await
    }

    pub async fn files(&self, folder: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.files_list(&folder, false) }))
            .await
    }

    pub async fn file_details(&self, id: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.files_show(&id) }))
            .await
    }

    pub async fn open_file(&self, id: String, version: String) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move {
                dmsg_client::library::open::open(&c.cooperative_hold(), &id, Some(&version)).await
            })
        })
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
        operation
            .run(self.object(move |c| {
                Box::pin(async move {
                    ensure(c.session_state().state == "unlocked", Error::Locked)?;
                    ensure(
                        c.session_state().account.as_deref() == Some(owner.as_str()),
                        Error::Forbidden,
                    )?;
                    dmsg_client::library::import::file(
                        &c.cooperative_hold(),
                        &path,
                        &folder,
                        true,
                        &mut |_, _| {},
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
        self.object(move |c| {
            Box::pin(async move {
                ensure(
                    c.files_show(&id)?.file.current == expected_version,
                    Error::VersionConflict,
                )?;
                dmsg_client::library::backup::backup(&c.cooperative_hold(), &[id], false).await
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
        self.object(|c| Box::pin(async { c.files_list("root", true) }))
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
            .run(self.object(move |c| {
                Box::pin(async move {
                    dmsg_client::library::cloud::cloud_delete(&c.cooperative_hold(), &[upload])
                        .await
                })
            }))
            .await
    }

    pub async fn cloud_backups(
        &self,
        refresh: bool,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.object(move |c| {
                Box::pin(async move {
                    if refresh {
                        dmsg_client::library::cloud::cloud_refresh(&c.cooperative_hold()).await?;
                    }
                    c.files_cloud_list()
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
            .run(self.object(move |c| {
                Box::pin(async move {
                    let entry = c
                        .files_cloud_list()?
                        .into_iter()
                        .find(|e| e.upload_id == upload)
                        .ok_or(Error::NotFound)?;
                    ensure(
                        entry.size.is_some_and(|s| s <= c.config().files_max_bytes),
                        Error::QuotaExceeded,
                    )?;
                    dmsg_client::library::cloud::cloud_fetch(
                        &c.cooperative_hold(),
                        &upload,
                        Some(&folder),
                        &mut |_, _| {},
                    )
                    .await
                })
            }))
            .await
    }

    pub async fn approve_pairing_request(
        &self,
        token: String,
        device: String,
    ) -> Result<ObjectSnapshot> {
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
        self.object(|c| {
            Box::pin(async { dmsg_client::security::account_security(&c.cooperative_hold()).await })
        })
        .await
    }

    pub async fn profile(&self) -> Result<ObjectSnapshot> {
        self.object(|c| {
            Box::pin(async { dmsg_client::principal::show(&c.cooperative_hold()).await })
        })
        .await
    }

    /// Fetch a public profile image through the core's anonymous, bounded
    /// downloader. A stale account handle cannot return image bytes to Swift.
    pub async fn profile_avatar(&self, url: String) -> Result<Option<Vec<u8>>> {
        self.host
            .run(move |c| {
                Box::pin(async move {
                    let image = dmsg_client::principal::avatar(&c.cooperative_hold(), &url).await?;
                    Ok(image.map(|(_, bytes)| bytes))
                })
            })
            .await
            .map_err(Into::into)
    }

    pub async fn publish_profile(
        &self,
        name: String,
        description: String,
        expected_updated_at: i64,
    ) -> Result<ObjectSnapshot> {
        let text = |value: String| Some(value.trim().to_string()).filter(|v| !v.is_empty());
        let (name, description) = (text(name), text(description));
        self.object(move |c| {
            Box::pin(async move {
                let mut document = dmsg_client::principal::show(&c.cooperative_hold()).await?;
                ensure(
                    document.updated_at == expected_updated_at,
                    Error::VersionConflict,
                )?;
                document.name = name;
                document.description = description;
                let info = c.publish_document(&document).await?;
                Ok(serde_json::json!({
                    "version": info.state.version,
                    "published_version": info.published_version,
                    "published": info.state.version == info.published_version,
                }))
            })
        })
        .await
    }

    pub async fn create_binding_request(&self, account: String) -> Result<ObjectSnapshot> {
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
        let transition = self.host.begin_transition()?;
        self.platform.cancel_presence();
        let generation = transition.generation();
        let prepared = transition
            .run(move |c| Box::pin(async move { c.prepare_bind_accept(&account).await }))
            .await?;
        self.finish_login_unlock(generation, prepared).await?;
        transition.finish()?;
        self.session_state().await
    }

    pub async fn review_binding(&self, token: String) -> Result<ObjectSnapshot> {
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
        self.object(|c| {
            Box::pin(async {
                let state = c
                    .cooperative_hold()
                    .warm(dmsg_client::client::Warm::Home)
                    .await?
                    .ok_or(Error::PolicyStale)?;
                c.account_view_of(&state)
            })
        })
        .await
    }

    pub async fn revoke_device(&self, device: String) -> Result<ObjectSnapshot> {
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

    pub async fn dispute_recovery(&self, operation: String) -> Result<ObjectSnapshot> {
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
        self.object(|c| Box::pin(async { c.recovery_status().await }))
            .await
    }

    pub async fn request_recovery(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.recovery_request().await }))
            .await
    }

    pub async fn complete_recovery(&self) -> Result<SessionSnapshot> {
        let transition = self.host.begin_transition()?;
        self.platform.cancel_presence();
        transition
            .run(|c| Box::pin(async { c.recovery_complete().await }))
            .await?;
        transition.finish()?;
        self.session_state().await
    }
}
