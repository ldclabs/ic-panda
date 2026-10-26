//! Named content operations shared with the native editor and task screens.
use super::*;
use dmsg_client::{channel, channel_log, library, vault};
use serde_json::json;

#[derive(Clone, serde::Serialize, uniffi::Record)]
pub struct CredentialSelection {
    pub id: String,
    pub revision: String,
}

#[derive(uniffi::Record)]
pub struct ChannelMemberChange {
    pub channel: String,
    pub action: String,
    pub account: Option<String>,
    pub role: Option<String>,
    pub can_rotate: bool,
    pub request_id: String,
}

impl MobileEngine {
    fn staged_file(&self, name: &str) -> Result<(PathBuf, String)> {
        let handle = PathBuf::from(name);
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
            .ok_or_else(|| dmsg_types::invalid("account"))?
            .parse::<dmsg_types::AccountId>()
            .map_err(|_| dmsg_types::invalid("account"))?
            .to_string();
        let root = self
            .staging
            .canonicalize()
            .map_err(|_| dmsg_types::invalid("staging"))?;
        let path = root
            .join(name)
            .canonicalize()
            .map_err(|_| dmsg_types::invalid("import"))?;
        ensure_valid(
            path.parent().and_then(|p| p.parent()) == Some(root.join(&owner).as_path()),
            "import path",
        )?;
        Ok((path, owner))
    }
}

#[uniffi::export]
impl MobileEngine {
    /// Export only reviewed login fields after independent system confirmation.
    pub async fn export_autofill(&self, items: Vec<CredentialSelection>) -> Result<ObjectSnapshot> {
        ensure_valid(!items.is_empty() && items.len() <= 50, "selection")?;
        let platform = self.platform.clone();
        self.held_object(move |h| Box::pin(async move {
            let session = h.with(|c| vault::reading::Session::start(c)).await?;
            let digest = hex::encode(*dmsg_protocol::digest("dmsg/autofill-export/1", &items));
            runtime().spawn_blocking(move || platform.confirm(hex::encode(dmsg_client::crypto::random_id()), digest, "autofill_export".into()))
                .await.map_err(|_| Error::Locked)?.map_err(platform::platform_error)?;
            let mut exported = Vec::with_capacity(items.len());
            for selected in items {
                exported.push(h.with(|c| {
                    session.check(c)?;
                    let entry = c.vault_show(&selected.id)?;
                    ensure(entry.revision == selected.revision, Error::VersionConflict)?;
                    ensure(!entry.deleted && !entry.item.reserved() && entry.item.kind == "login", Error::Forbidden)?;
                    let field = |id| entry.item.field(id).map_or("", |f| f.value.as_str());
                    ensure_valid(!field("password").is_empty(), "password")?;
                    Ok(json!({"title":entry.item.title,"username":field("username"),"password":field("password"),"url":field("url")}))
                }).await?);
            }
            h.with(|c| session.check(c)).await?;
            Ok(exported)
        })).await
    }

    pub async fn vault_import_preview(
        &self,
        name: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        let (path, owner) = self.staged_file(&name)?;
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let session = h
                        .with(|c| {
                            ensure(
                                c.session_state().account.as_deref() == Some(owner.as_str()),
                                Error::Forbidden,
                            )?;
                            vault::reading::Session::start(c)
                        })
                        .await?;
                    let imported = runtime()
                        .spawn_blocking(move || vault::import::read(&path))
                        .await
                        .map_err(|_| Error::Locked)??;
                    let items = vault::reading::list(h, &vault::Filter::default()).await?;
                    h.with(|c| session.check(c)).await?;
                    Ok(imported.preview(&items))
                })
            }))
            .await
    }

    pub async fn vault_import_selected(
        &self,
        name: String,
        digest: String,
        picks: Vec<u32>,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(!picks.is_empty() && picks.len() <= 1000, "selection")?;
        let (path, owner) = self.staged_file(&name)?;
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let session = h
                        .with(|c| {
                            ensure(
                                c.session_state().account.as_deref() == Some(owner.as_str()),
                                Error::Forbidden,
                            )?;
                            vault::reading::Session::start(c)
                        })
                        .await?;
                    let items = runtime()
                        .spawn_blocking(move || {
                            vault::import::read(&path)?.select(
                                &digest,
                                &picks.into_iter().map(|i| i as usize).collect::<Vec<_>>(),
                            )
                        })
                        .await
                        .map_err(|_| Error::Locked)??;
                    h.with(|c| {
                        session.check(c)?;
                        c.vault_add_all_once(&name, items)
                    })
                    .await
                })
            }))
            .await
    }

    pub async fn search_files(
        &self,
        query: String,
        folder: Option<String>,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(query.len() <= 1000, "query")?;
        self.object(move |c| {
            Box::pin(async move {
                c.files_search(
                    &query,
                    &library::SearchFilter {
                        folder,
                        ..Default::default()
                    },
                )
            })
        })
        .await
    }

    pub async fn rename_file(&self, id: String, name: String) -> Result<()> {
        self.host
            .run(move |c| Box::pin(async move { c.files_rename(&id, &name) }))
            .await?;
        Ok(())
    }

    pub async fn move_file(&self, id: String, folder: String) -> Result<()> {
        self.host
            .run(move |c| Box::pin(async move { c.files_move(&[id], &folder) }))
            .await?;
        Ok(())
    }

    pub async fn search_messages(&self, channel: String, query: String) -> Result<MessagePage> {
        let generation = self.host.generation();
        let items = self
            .host
            .run(move |c| Box::pin(async move { c.channel_search(&channel, &query, 100) }))
            .await?;
        Ok(MessagePage {
            generation,
            items: items.into_iter().map(MessageItem::from).collect(),
            has_more: false,
        })
    }

    pub async fn queue_message_action(
        &self,
        channel: String,
        action: String,
        sequence: u64,
        digest: String,
        text: String,
    ) -> Result<String> {
        self.host
            .run(move |c| {
                Box::pin(async move {
                    let target = c
                        .channel_messages_before(&channel, sequence.saturating_add(1), 1)?
                        .pop()
                        .ok_or(Error::NotFound)?;
                    ensure(
                        target.seq == sequence
                            && target.digest == digest
                            && !target.retracted
                            && !target.unreadable,
                        Error::VersionConflict,
                    )?;
                    let reference = channel::MessageRef {
                        seq: sequence,
                        digest,
                    };
                    let mut draft = channel::Draft {
                        text,
                        ..Default::default()
                    };
                    match action.as_str() {
                        "reply" => draft.reply_to = Some(reference),
                        "edit" | "retract" => {
                            ensure(
                                c.session_state().account.as_deref()
                                    == Some(target.account.as_str()),
                                Error::Forbidden,
                            )?;
                            if action == "edit" {
                                draft.edit_of = Some(reference);
                            } else {
                                ensure_valid(draft.text.is_empty(), "withdrawal text")?;
                                draft.retract = Some(reference);
                            }
                        }
                        _ => return Err(dmsg_types::invalid("message action")),
                    }
                    c.queue_draft(&channel, &draft).map(|i| i.id)
                })
            })
            .await
            .map_err(Into::into)
    }

    pub async fn pending_channel_operations(&self, channel: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.channel_pending(&channel) }))
            .await
    }

    pub async fn resume_channel_operation(
        &self,
        channel: String,
        key: String,
    ) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.channel_resume(&channel, &key).await }))
            .await
    }

    pub async fn channel_details(&self, channel: String) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.channel_view(&channel) }))
            .await
    }

    pub async fn channel_settings(
        &self,
        channel: String,
        name: String,
        muted: bool,
        hidden: bool,
    ) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(
                async move { c.channel_settings(&channel, Some(&name), Some(muted), Some(hidden)) },
            )
        })
        .await
    }

    pub async fn channel_member_action(
        &self,
        change: ChannelMemberChange,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        let ChannelMemberChange {
            channel,
            action,
            account,
            role,
            can_rotate,
            request_id,
        } = change;
        ensure_valid(
            request_id.len() == 64 && request_id.bytes().all(|b| b.is_ascii_hexdigit()),
            "request ID",
        )?;
        operation
            .run(self.object(move |c| {
                Box::pin(async move {
                    if action == "leave" {
                        c.channel_leave(&channel, &request_id).await?;
                        return Ok(json!({"left":true}));
                    }
                    let member = || {
                        account
                            .as_ref()
                            .and_then(|a| a.parse::<dmsg_types::AccountId>().ok())
                            .map(|a| a.to_string())
                            .ok_or_else(|| dmsg_types::invalid("account"))
                    };
                    let action = match action.as_str() {
                        "remove" => channel_log::Action::Remove { account: member()? },
                        "role" => channel_log::Action::Role {
                            account: member()?,
                            role: match role.as_deref() {
                                Some("admin") => channel_log::Role::Admin,
                                Some("member") => channel_log::Role::Member,
                                Some("publisher") => channel_log::Role::Publisher,
                                _ => return Err(dmsg_types::invalid("role")),
                            },
                            can_rotate,
                        },
                        "archive" => channel_log::Action::Archive {},
                        "rekey" => channel_log::Action::Rekey {},
                        _ => return Err(dmsg_types::invalid("channel action")),
                    };
                    c.channel_control(&channel, action, &request_id).await?;
                    serde_json::to_value(c.channel_view(&channel)?)
                        .map_err(|_| Error::IntegrityFailed)
                })
            }))
            .await
    }

    pub async fn queue_free_letter(
        &self,
        id: String,
        recipient: String,
        text: String,
        invitation: Option<String>,
    ) -> Result<()> {
        self.host
            .run(move |c| {
                Box::pin(async move { c.queue_free_letter(&id, &recipient, &text, invitation) })
            })
            .await?;
        Ok(())
    }

    pub async fn send_free_letter(
        &self,
        id: String,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move { dmsg_client::outbox::send_free_letter(h, &id).await })
            }))
            .await
    }

    pub async fn pending_letters(&self) -> Result<ObjectSnapshot> {
        self.object(|c| Box::pin(async { c.pending_letters() }))
            .await
    }

    pub async fn upload_profile_avatar(
        &self,
        bytes: Vec<u8>,
        operation: Arc<OperationToken>,
    ) -> Result<String> {
        ensure_valid(bytes.len() <= dmsg_client::principal::MAX_AVATAR, "avatar")?;
        operation
            .run(self.held(move |h| {
                Box::pin(async move { dmsg_client::principal::upload_avatar(h, &bytes).await })
            }))
            .await
    }

    pub async fn profile_editor(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { dmsg_client::principal::editable(h).await }))
            .await
    }

    pub async fn account_plan(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { dmsg_client::commerce::plan_status(h, false).await }))
            .await
    }
}

#[uniffi::export]
impl MobileEngine {
    pub async fn inbox_policy(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { dmsg_client::inbox::policy(h).await }))
            .await
    }

    pub async fn configure_inbox(
        &self,
        mode: String,
        allow: Vec<String>,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(
            matches!(mode.as_str(), "closed" | "contacts" | "free"),
            "free inbox policy",
        )?;
        let input = dmsg_client::inbox::PolicyInput {
            mode,
            allow,
            block: None,
            invitations: None,
            capacity: None,
            price: None,
            payee: None,
        };
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move { dmsg_client::inbox::configure(h, &input).await })
            }))
            .await
    }
}
