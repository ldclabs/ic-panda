//! Offline fixtures for the separate Android QA build. This module and its FFI
//! constructor are absent unless the explicit ui-fixtures feature is enabled.
use super::*;
use dmsg_client::{testing, vault::Changes};

#[uniffi::export]
impl MobileEngine {
    /// Local projection fixtures; native read/snooze/observe semantics remain real.
    pub async fn seed_ui_reminders(&self) -> Result<ObjectSnapshot> {
        self.object(|c| {
            Box::pin(async {
                let account = testing::account().id;
                ensure(
                    c.session_state().account == Some(account.to_string()),
                    Error::Forbidden,
                )?;
                let now = dmsg_client::now_ms();
                c.with_notification_store(&account, |store| {
                    use dmsg_client::notifications::{Item, Target, upsert};
                    let mut first = Item::new(
                        account.to_string(),
                        Target::Approval("dd".repeat(32)),
                        9_007_199_254_740_993,
                        now,
                    );
                    first.actionable = true;
                    first.deadline = Some(now + 3_600_000);
                    upsert(store, first.clone())?;
                    let mut second = Item::new(
                        account.to_string(),
                        Target::Channel("ab".repeat(32)),
                        1,
                        now,
                    );
                    second.unread = Some(1);
                    upsert(store, second.clone())?;
                    Ok(serde_json::json!([first, second]))
                })
            })
        })
        .await
    }

    /// Synthetic public profile cache, while actual online publication remains unavailable.
    pub async fn seed_ui_profile(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async {
            ensure(c.session_state().account == Some(testing::account().id.to_string()), Error::Forbidden)?;
            let profile = serde_json::json!({
                "name":"Mina's workspace", "description":"A public profile used only in offline QA.",
                "avatar_url":null, "links":[{"name":"Project","url":"https://dmsg.net/","rel":"related"}],
                "updated_at":1
            });
            c.cache_save(dmsg_client::cache::Kind::ProfileEditor, "", &profile, dmsg_client::now_ms())?;
            c.cache_save(dmsg_client::cache::Kind::Profile, "", &profile, dmsg_client::now_ms())?;
            Ok(profile)
        })).await
    }

    pub async fn seed_ui_channel_members(&self, role: String) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move {
                ensure(
                    c.session_state().account == Some(testing::account().id.to_string()),
                    Error::Forbidden,
                )?;
                let role = match role.as_str() {
                    "owner" => dmsg_client::channel_log::Role::Owner,
                    "admin" => dmsg_client::channel_log::Role::Admin,
                    "member" => dmsg_client::channel_log::Role::Member,
                    _ => return Err(dmsg_types::invalid("role")),
                };
                let channel = "ab".repeat(32);
                testing::channel_members(c, &channel, role);
                c.channel_view(&channel)
            })
        })
        .await
    }

    pub async fn seed_ui_received_file(&self) -> Result<ObjectSnapshot> {
        self.object(|c| {
            Box::pin(async {
                ensure(
                    c.session_state().account == Some(testing::account().id.to_string()),
                    Error::Forbidden,
                )?;
                let channel = "ab".repeat(32);
                let id = testing::saved_shared_file(c, &channel, 1);
                Ok(serde_json::json!({"file":id,"channel":channel,"sequence":1}))
            })
        })
        .await
    }

    /// Isolated QA only; uses real library imports and version records.
    pub async fn seed_ui_file_history(&self) -> Result<ObjectSnapshot> {
        self.object(|c| {
            Box::pin(async {
                ensure(
                    c.session_state().account == Some(testing::account().id.to_string()),
                    Error::Forbidden,
                )?;
                let id = testing::file_history(c);
                c.files_show(&id)
            })
        })
        .await
    }

    #[uniffi::constructor]
    pub async fn open_ui_fixture(
        directory: String,
        staging: String,
        platform: Box<dyn Platform>,
    ) -> Result<Arc<Self>> {
        diagnostics::init();
        let directory = PathBuf::from(directory);
        let staging = PathBuf::from(staging);
        ensure_valid(
            directory.is_absolute() && staging.is_absolute() && directory != staging,
            "fixture paths",
        )?;
        // This is a freshly created database, never an existing account workspace.
        ensure(!directory.exists(), Error::VersionConflict)?;
        std::fs::create_dir_all(&directory)
            .map_err(|_| Error::Unavailable("fixture directory".into()))?;
        let client = runtime()
            .spawn_blocking(move || {
                let mut c = testing::unlocked(&directory, false, None);
                c.use_read_only_previews();
                c.require_prepared_approval();
                for (kind, title, notes, favorite) in [
                    (
                        "login",
                        "Studio account",
                        "Offline QA fixture · no real credentials",
                        true,
                    ),
                    (
                        "note",
                        "Project notebook",
                        "A private note with a long title and 中文测试内容。",
                        false,
                    ),
                ] {
                    c.vault_create(
                        kind,
                        &Changes {
                            title: Some(title.into()),
                            notes: Some(notes.into()),
                            favorite: Some(favorite),
                            ..Default::default()
                        },
                    )?;
                }
                let channel = "ab".repeat(32);
                testing::joined_channel(&mut c, &channel);
                c.channel_draft(&channel, "Continue the conversation")?;
                c.save_letter_draft(
                    &"cd".repeat(32),
                    &testing::account().id.to_string(),
                    "A sealed letter draft",
                    None,
                )?;
                c.files_mkdir("root", "Project materials")?;
                Ok::<_, Error>(c)
            })
            .await
            .map_err(|_| Error::Unavailable("fixture worker".into()))??;
        Ok(Self::fixture_engine(client, staging, platform))
    }

    /// QA-only restart of the encrypted offline fixture. No production credentials.
    #[uniffi::constructor]
    pub async fn reopen_ui_fixture(
        directory: String,
        staging: String,
        platform: Box<dyn Platform>,
    ) -> Result<Arc<Self>> {
        let path = PathBuf::from(directory);
        ensure(path.is_absolute() && path.is_dir(), Error::NotFound)?;
        let client = runtime()
            .spawn_blocking(move || testing::reopen_fixture(&path))
            .await
            .map_err(|_| Error::Unavailable("fixture restart".into()))??;
        Ok(Self::fixture_engine(
            client,
            PathBuf::from(staging),
            platform,
        ))
    }

    /// QA-only unlock for the fixed, loopback-only synthetic account.
    pub async fn unlock_ui_fixture(&self) -> Result<SessionSnapshot> {
        ensure(
            self.config.environment == dmsg_types::Environment::Local
                && self.config.ic_host == "http://127.0.0.1:9",
            Error::Forbidden,
        )?;
        let generation = self.host.inner.generation();
        self.host
            .inner
            .with_at(generation, |c| {
                ensure(
                    c.session_state().account == Some(testing::account().id.to_string()),
                    Error::Forbidden,
                )?;
                testing::unlock_fixture(c);
                Ok(())
            })
            .await?;
        self.scoped(generation)?.session_state().await
    }
}

impl MobileEngine {
    fn fixture_engine(client: Client, staging: PathBuf, platform: Box<dyn Platform>) -> Arc<Self> {
        let config = client.config().clone();
        Arc::new(Self {
            host: ScopedHost::new(Host::start(client, runtime().handle()), 0),
            platform: Arc::from(platform),
            config,
            staging,
            login: Arc::default(),
            approval_turn: Arc::default(),
            read_turns: Arc::default(),
            sync_tracker: Arc::default(),
            hints_redial: Arc::default(),
            sync_signal: Arc::default(),
            plan_checked: Arc::default(),
            transfers: Arc::default(),
        })
    }
}
