//! Named native notification operations. Shared policy stays in dmsg_client;
//! Swift owns permission, actual visibility, presentation and navigation.
use super::*;
use dmsg_client::{notifications as notice, now_ms};
use serde_json::json;

#[derive(Clone, Copy, uniffi::Enum)]
pub enum ReminderScope {
    NeedsAction,
    Recent,
}

#[derive(Clone, uniffi::Record)]
pub struct VisibleReminder {
    pub id: String,
    pub version: String,
}

#[derive(Clone, uniffi::Record)]
pub struct NotificationPresentationContext {
    pub allowed: bool,
    pub foreground: bool,
    pub quiet_until: Option<u64>,
    pub visible: Vec<VisibleReminder>,
}

#[derive(Clone, Copy, uniffi::Enum)]
pub enum NotificationPresentationResult {
    Submitted,
    Shown,
    Failed,
    Suppressed,
}

fn account(value: &str) -> dmsg_types::Result<dmsg_types::AccountId> {
    value.parse().map_err(|_| dmsg_types::invalid("account"))
}

fn version(value: &str) -> dmsg_types::Result<u64> {
    let result = value
        .parse::<u64>()
        .map_err(|_| dmsg_types::invalid("notification version"))?;
    ensure(
        result.to_string() == value,
        Error::InvalidInput("notification version".into()),
    )?;
    Ok(result)
}

#[uniffi::export]
impl MobileEngine {
    pub async fn notification_preferences(
        &self,
        selected: Option<String>,
    ) -> Result<ObjectSnapshot> {
        self.object(move |c| {
            Box::pin(async move {
                let runtime = notice::Runtime::load(&c.notification_runtime_path(), now_ms())?;
                let selected = selected
                    .or(c.session_state().account)
                    .map(|a| account(&a))
                    .transpose()?;
                let settings = selected
                    .as_ref()
                    .map(|a| c.with_notification_store(a, notice::settings))
                    .transpose()?;
                let supported = selected
                    .as_ref()
                    .map(|a| c.notification_remote_supported(a))
                    .transpose()?
                    .unwrap_or(false);
                let conflict = selected
                    .as_ref()
                    .map(|a| c.notification_settings_conflict(a))
                    .transpose()?
                    .unwrap_or(false);
                Ok(json!({
                    "global": runtime.settings, "account": settings,
                    "account_id": selected.map(|a| a.to_string()), "remote_supported": supported,
                    "global_pending_sync": runtime.settings.revision > runtime.remote_revision,
                    "global_conflict": runtime.remote_blocked == Some(runtime.settings.revision),
                    "account_conflict": conflict
                }))
            })
        })
        .await
    }

    pub async fn notification_set_global(
        &self,
        expected: u64,
        settings_json: String,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(settings_json.len() <= 65536, "notification settings")?;
        let settings: notice::GlobalSettings = serde_json::from_str(&settings_json)
            .map_err(|_| dmsg_types::invalid("notification settings"))?;
        self.object(move |c| {
            Box::pin(async move {
                let path = c.notification_runtime_path();
                let mut runtime = notice::Runtime::load(&path, now_ms())?;
                runtime.configure(expected, settings)?;
                runtime.save(&path)?;
                Ok(runtime.settings)
            })
        })
        .await
    }

    pub async fn notification_set_account(
        &self,
        selected: String,
        expected: u64,
        settings_json: String,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(settings_json.len() <= 65536, "notification settings")?;
        let settings: notice::Settings = serde_json::from_str(&settings_json)
            .map_err(|_| dmsg_types::invalid("notification settings"))?;
        let selected = account(&selected)?;
        self.object(move |c| {
            Box::pin(async move {
                c.with_notification_store(&selected, |s| notice::configure(s, expected, settings))
            })
        })
        .await
    }

    pub async fn notification_import_legacy(&self, selected: String, enabled: bool) -> Result<()> {
        let selected = account(&selected)?;
        self.host
            .run(move |c| {
                Box::pin(async move {
                    let imported = c
                        .with_notification_store(&selected, |s| {
                            s.meta::<bool>("notification:ios-imported")
                        })?
                        .unwrap_or(false);
                    if imported {
                        return Ok(());
                    }
                    c.with_notification_store(&selected, |s| {
                        let mut settings = notice::settings(s)?;
                        if settings.revision == 0 && enabled {
                            settings.enabled = true;
                            notice::configure(s, 0, settings)?;
                        }
                        s.set_meta("notification:ios-imported", &true)
                    })?;
                    let path = c.notification_runtime_path();
                    let mut runtime = notice::Runtime::load(&path, now_ms())?;
                    if runtime.settings.revision == 0 && enabled {
                        let mut settings = runtime.settings.clone();
                        settings.enabled = true;
                        runtime.configure(0, settings)?;
                        runtime.save(&path)?;
                    }
                    Ok(())
                })
            })
            .await
            .map_err(Into::into)
    }

    pub async fn notification_status(&self) -> Result<ObjectSnapshot> {
        self.object(|c| {
            Box::pin(async {
                let now = now_ms();
                let runtime = notice::Runtime::load(&c.notification_runtime_path(), now)?;
                let mut total = notice::Counts::default();
                let mut accounts = Vec::new();
                for local in c.accounts()?.into_iter().filter(|a| a.bound) {
                    c.with_notification_store(&account(&local.account)?, |store| {
                        let settings = notice::settings(store)?;
                        let counts = notice::counts(&store.notification_items()?, now);
                        if settings.enabled {
                            total.actionable = total.actionable.saturating_add(counts.actionable);
                            total.unread = total.unread.saturating_add(counts.unread);
                            total.unverified |= counts.unverified;
                        }
                        accounts.push(json!({"account":local.account,"enabled":settings.enabled,"counts":counts}));
                        Ok(())
                    })?;
                }
                let badge = match runtime.settings.badge {
                    notice::Badge::All => total.actionable.saturating_add(total.unread),
                    notice::Badge::Actions => total.actionable,
                    notice::Badge::Off => 0,
                };
                Ok(json!({"counts":total,"accounts":accounts,"badge":badge,"enabled":runtime.settings.enabled}))
            })
        }).await
    }

    pub async fn notification_items(
        &self,
        selected: String,
        scope: ReminderScope,
        offset: u32,
    ) -> Result<ObjectSnapshot> {
        let selected = account(&selected)?;
        self.object(move |c| {
            Box::pin(async move {
                let now = now_ms();
                c.with_notification_store(&selected, |s| {
                    notice::list(
                        s,
                        matches!(scope, ReminderScope::NeedsAction),
                        offset as usize,
                        100,
                        now,
                    )
                })
            })
        })
        .await
    }

    pub async fn notification_reference(
        &self,
        selected: String,
        target_json: String,
    ) -> Result<ObjectSnapshot> {
        let selected = account(&selected)?;
        let target: notice::Target = serde_json::from_str(&target_json)
            .map_err(|_| dmsg_types::invalid("notification target"))?;
        self.object(move |c| {
            Box::pin(async move {
                c.with_notification_store(&selected, |s| {
                    s.notification_item(&notice::item_id(&selected.to_string(), &target))
                })
            })
        })
        .await
    }

    pub async fn notification_observed(
        &self,
        selected: String,
        id: String,
        expected_version: String,
    ) -> Result<()> {
        let selected = account(&selected)?;
        let expected_version = version(&expected_version)?;
        self.host
            .run(move |c| {
                Box::pin(async move {
                    ensure(
                        c.session_state().unlocked_account() == Some(selected.to_string().as_str()),
                        Error::Locked,
                    )?;
                    c.with_notification_store(&selected, |s| {
                        notice::observe(s, &id, expected_version)
                    })
                })
            })
            .await
            .map_err(Into::into)
    }

    pub async fn notification_snooze(
        &self,
        selected: String,
        id: String,
        expected_version: String,
        until: u64,
    ) -> Result<()> {
        let selected = account(&selected)?;
        let expected_version = version(&expected_version)?;
        self.host
            .run(move |c| {
                Box::pin(async move {
                    c.with_notification_store(&selected, |s| {
                        notice::snooze(s, &id, expected_version, until, now_ms())
                    })
                })
            })
            .await
            .map_err(Into::into)
    }

    pub async fn notification_clear_recent(&self, selected: String) -> Result<()> {
        let selected = account(&selected)?;
        self.host
            .run(move |c| {
                Box::pin(async move {
                    c.with_notification_store(&selected, |s| {
                        s.set_meta("notification:cleared_at", &now_ms())
                    })
                })
            })
            .await
            .map_err(Into::into)
    }

    pub async fn notification_register_apple(
        &self,
        installation: String,
        revision: u64,
        language: String,
    ) -> Result<ObjectSnapshot> {
        self.held_object(move |h| {
            Box::pin(async move {
                notice::refresh_capabilities(h).await?;
                let context = notice::remote::register(
                    h,
                    &language,
                    Some(notice::remote::AppleRegistration {
                        installation,
                        revision,
                    }),
                )
                .await?;
                if let Some(context) = &context {
                    notice::remote::sync_settings(h, context).await?;
                    notice::preferences::sync(h, context).await?;
                }
                Ok(context)
            })
        })
        .await
    }

    pub async fn notification_refresh(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| {
            Box::pin(async {
                let session = h.with(|c| Ok(c.session_state())).await?;
                if session.unlocked_account().is_some() {
                    let _ = notice::refresh_capabilities(h).await;
                    h.with(|c| c.reconcile_notifications(now_ms())).await?;
                    // Ordinary incoming letters use the same verifier as the inbox page.
                    let _ = dmsg_client::inbox::sync(h, 3).await;
                    let _ = dmsg_client::wallet::refresh_notifications(h).await;
                    let _ = dmsg_client::commerce::plan_status(h, false).await;
                }
                let contexts = h.with(|c| c.notification_receivers()).await?;
                let mut changed = false;
                for context in contexts {
                    if session.unlocked_account() == Some(context.account.as_str()) {
                        let _ = notice::remote::sync_settings(h, &context).await;
                        let _ = notice::preferences::sync(h, &context).await;
                        let _ = notice::remote::sync_snoozes(h, &context).await;
                        let _ = notice::remote::sync_recovery(h, &context).await;
                    }
                    let _ = notice::remote::send_observed(h, &context).await;
                    for _ in 0..3 {
                        match notice::remote::refresh(h, &context).await {
                            Ok(more) => {
                                changed = true;
                                if !more {
                                    break;
                                }
                            }
                            Err(error) => {
                                if matches!(error, Error::AuthRequired | Error::DeviceNotApproved) {
                                    let _ = h
                                        .with(|c| c.notification_receiver_health(&context, true))
                                        .await;
                                }
                                break;
                            }
                        }
                    }
                }
                Ok(json!({"changed":changed}))
            })
        })
        .await
    }

    pub async fn notification_next(
        &self,
        context: NotificationPresentationContext,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(context.visible.len() <= 32, "visible notifications")?;
        let visible = context
            .visible
            .into_iter()
            .map(|v| {
                Ok(notice::Covered {
                    id: v.id,
                    version: version(&v.version)?,
                    stage: 0,
                })
            })
            .collect::<dmsg_types::Result<Vec<_>>>()?;
        self.object(move |c| {
            Box::pin(async move {
                let now = now_ms();
                let path = c.notification_runtime_path();
                let mut runtime = notice::Runtime::load(&path, now)?;
                let context = notice::policy::Context {
                    now,
                    allowed: context.allowed,
                    foreground: context.foreground,
                    quiet_until: context.quiet_until,
                    visible,
                    remote_owner: true,
                    ..Default::default()
                };
                let mut all = notice::Batch::default();
                for local in c.accounts()?.into_iter().filter(|a| a.bound) {
                    let batch = c.with_notification_store(&account(&local.account)?, |s| {
                        notice::next(s, &mut runtime, &context, |r| r.save(&path))
                    })?;
                    all.plans.extend(batch.plans);
                    all.remove.extend(batch.remove);
                    if let Some(at) = batch.next_due {
                        all.next_due = Some(all.next_due.map_or(at, |old| old.min(at)));
                    }
                }
                Ok(all)
            })
        })
        .await
    }

    pub async fn notification_result(
        &self,
        selected: String,
        system_id: String,
        delivery_id: String,
        result: NotificationPresentationResult,
    ) -> Result<()> {
        let selected = account(&selected)?;
        let result = match result {
            NotificationPresentationResult::Submitted => notice::DeliveryResult::Submitted,
            NotificationPresentationResult::Shown => notice::DeliveryResult::Shown,
            NotificationPresentationResult::Failed => notice::DeliveryResult::Failed,
            NotificationPresentationResult::Suppressed => notice::DeliveryResult::Suppressed,
        };
        self.host
            .run(move |c| {
                Box::pin(async move {
                    c.with_notification_store(&selected, |s| {
                        notice::result(s, &system_id, &delivery_id, result, now_ms())
                    })
                })
            })
            .await
            .map_err(Into::into)
    }

    pub async fn notification_resolve(&self, route: String) -> Result<ObjectSnapshot> {
        ensure_valid(route.len() <= 160, "notification route")?;
        self.held_object(move |h| {
            Box::pin(async move {
                if let Some(local) = route.strip_prefix("local:") {
                    ensure(dmsg_client::channel_log::is_hash(local), Error::InvalidInput("notification route".into()))?;
                    return h.with(|c| {
                        for account_ref in c.accounts()?.into_iter().filter(|a| a.bound) {
                            let plan = c.with_notification_store(&account(&account_ref.account)?, |s| {
                                Ok(s.notification_deliveries()?.into_iter().find(|p| p.system_id == local))
                            })?;
                            if let Some(plan) = plan {
                                return Ok(json!({"account":plan.account,"target":plan.target,"ended":plan.result == notice::DeliveryResult::Suppressed}));
                            }
                        }
                        Err(Error::NotFound)
                    }).await;
                }
                let contexts = h.with(|c| c.notification_receivers()).await?;
                for context in contexts {
                    if !route.starts_with(&context.installation) { continue; }
                    match notice::remote::resolve(h, &context, &route).await {
                        Ok(value) => return Ok(value),
                        Err(Error::Forbidden | Error::AuthRequired | Error::NotFound | Error::DeviceNotApproved) => {},
                        Err(error) => return Err(error),
                    }
                }
                Err(Error::NotFound)
            })
        }).await
    }

    pub async fn channel_mute_until(
        &self,
        channel: String,
        until: Option<u64>,
    ) -> Result<ObjectSnapshot> {
        self.object(move |c| Box::pin(async move { c.channel_notification_mute(&channel, until) }))
            .await
    }
}
