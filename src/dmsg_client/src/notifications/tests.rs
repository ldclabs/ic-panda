use super::*;
use policy::{Context, Decision, Reason, evaluate};

fn example() -> (Item, Settings, Runtime, Context) {
    let mut item = Item::new(
        "account-a".into(),
        Target::Channel("channel-a".into()),
        9,
        1_000,
    );
    item.unread = Some(2);
    let settings = Settings {
        enabled: true,
        ..Settings::default()
    };
    let mut runtime = Runtime::default();
    runtime.settings.enabled = true;
    let context = Context {
        now: 4_000,
        allowed: true,
        ..Context::default()
    };
    (item, settings, runtime, context)
}

#[test]
fn notifications_shared_policy_vectors() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/vectors/client/notifications-v1.json"
    ))
    .unwrap();
    assert_eq!(vectors["version"], VERSION);
    for lifetime in vectors["lifetimes"].as_array().unwrap() {
        assert_eq!(
            policy::expires_at(
                serde_json::from_value(lifetime["category"].clone()).unwrap(),
                lifetime["now"].as_u64().unwrap(),
                lifetime["deadline"].as_u64()
            ),
            lifetime["expires"].as_u64().unwrap()
        );
    }
    for identity in vectors["identities"].as_array().unwrap() {
        let target: Target = serde_json::from_value(identity["target"].clone()).unwrap();
        assert_eq!(
            item_id(identity["account"].as_str().unwrap(), &target),
            identity["id"].as_str().unwrap()
        );
    }
    for vector in vectors["cases"].as_array().unwrap() {
        let (mut item, mut settings, mut runtime, mut ctx) = example();
        if let Some(value) = vector.get("item") {
            let mut base = serde_json::to_value(&item).unwrap();
            for (key, value) in value.as_object().unwrap() {
                base[key] = value.clone();
            }
            item = serde_json::from_value(base).unwrap();
        }
        if let Some(value) = vector.get("context") {
            let mut base = serde_json::to_value(&ctx).unwrap();
            for (key, value) in value.as_object().unwrap() {
                base[key] = value.clone();
            }
            ctx = serde_json::from_value(base).unwrap();
        }
        if let Some(enabled) = vector["enabled"].as_bool() {
            settings.enabled = enabled;
        }
        if let Some(sound) = vector["sound"].as_bool() {
            settings.categories.get_mut(&item.category()).unwrap().sound = sound;
        }
        if let Some(value) = vector.get("runtime") {
            let mut base = serde_json::to_value(&runtime).unwrap();
            for (key, value) in value.as_object().unwrap() {
                base[key] = value.clone();
            }
            runtime = serde_json::from_value(base).unwrap();
        }
        let expected: Decision = serde_json::from_value(vector["expected"].clone()).unwrap();
        assert_eq!(
            evaluate(&item, &settings, &runtime, &ctx),
            expected,
            "{}",
            vector["name"]
        );
    }
}

#[test]
fn notifications_replay_restart_and_business_completion_are_independent() {
    let temp = crate::testing::TempDir::new();
    let path = temp.0.join("workspace.sqlite");
    let (mut item, _, _, _) = example();
    {
        let store = Store::open(&path).unwrap();
        assert!(upsert(&store, item.clone()).unwrap());
        observe(&store, &item.id, item.version).unwrap();
        item.notified = item.version;
        item.observed = item.version;
        store.set_notification_item(&item).unwrap();
    }
    let store = Store::open(&path).unwrap();
    let mut replay = item.clone();
    replay.notified = 0;
    replay.observed = 0;
    assert!(!upsert(&store, replay.clone()).unwrap());
    let restored = store.notification_item(&item.id).unwrap().unwrap();
    assert!(restored.open);
    assert_eq!(counts(&[restored], 10_000).unread, 2);
    replay.open = false;
    replay.unread = Some(0);
    upsert(&store, replay.clone()).unwrap();
    replay.open = true;
    upsert(&store, replay).unwrap();
    assert!(!store.notification_item(&item.id).unwrap().unwrap().open);
}

#[test]
fn notifications_domain_transaction_rollback_and_reinitialization() {
    let store = Store::memory().unwrap();
    let (item, _, _, _) = example();
    let result: Result<()> = store.atomic(|store| {
        upsert(store, item.clone())?;
        store.set_meta("domain:cursor", &10_u64)?;
        Err(Error::IntegrityFailed)
    });
    assert!(result.is_err());
    assert!(store.notification_items().unwrap().is_empty());
    assert!(store.meta::<u64>("domain:cursor").unwrap().is_none());
}

#[test]
fn notifications_muted_arrivals_do_not_replay_when_mute_expires() {
    let store = Store::memory().unwrap();
    let (mut item, settings, runtime, mut ctx) = example();
    item.muted = true;
    item.muted_until = Some(5_000);
    upsert(&store, item.clone()).unwrap();
    let stored = store.notification_item(&item.id).unwrap().unwrap();
    assert_eq!(counts(std::slice::from_ref(&stored), 4_000).unread, 0);
    ctx.now = 6_000;
    assert_eq!(
        evaluate(&stored, &settings, &runtime, &ctx),
        Decision::Keep {
            reason: Reason::Covered
        }
    );
    item.version = 10;
    item.updated_at = 6_000;
    upsert(&store, item.clone()).unwrap();
    assert_eq!(
        store.notification_item(&item.id).unwrap().unwrap().notified,
        9
    );
}

#[test]
fn notifications_view_and_snooze_are_version_bound_and_do_not_resolve_actions() {
    let store = Store::memory().unwrap();
    let mut item = Item::new("a".into(), Target::Approval("r".into()), 1, 10);
    item.actionable = true;
    item.deadline = Some(100_000);
    upsert(&store, item.clone()).unwrap();
    assert_eq!(observe(&store, &item.id, 2), Err(Error::VersionConflict));
    assert!(snooze(&store, &item.id, 1, 100_000, 10).is_err());
    snooze(&store, &item.id, 1, 50_000, 10).unwrap();
    observe(&store, &item.id, 1).unwrap();
    let stored = store.notification_item(&item.id).unwrap().unwrap();
    assert!(stored.open && stored.actionable);
    assert_eq!(stored.snoozed_until, Some(50_000));
    assert_eq!(counts(std::slice::from_ref(&stored), 99_999).actionable, 1);
    assert_eq!(counts(&[stored], 100_000).actionable, 0);
}

#[test]
fn notifications_settings_conflicts_keep_the_previous_committed_draft() {
    let store = Store::memory().unwrap();
    let draft = Settings {
        enabled: true,
        ..Settings::default()
    };
    let saved = configure(&store, 0, draft).unwrap();
    assert_eq!(saved.revision, 1);
    assert_eq!(saved.remote_revision, 0);
    assert!(matches!(
        configure(&store, 0, Settings::default()),
        Err(Error::VersionConflict)
    ));
    assert_eq!(settings(&store).unwrap(), saved);
}

#[test]
fn notifications_runtime_persists_cross_account_budgets_and_fails_quiet() {
    let temp = crate::testing::TempDir::new();
    let path = temp.0.join("notification-runtime.json");
    let mut runtime = Runtime::load(&path, 1_000).unwrap();
    runtime.settings.enabled = true;
    runtime.reserve(Category::Message, true, 1_000);
    runtime.save(&path).unwrap();
    let restored = Runtime::load(&path, 2_000).unwrap();
    assert_eq!(restored.installation, runtime.installation);
    assert_eq!(restored.messages, vec![1_000]);
    assert_eq!(restored.sound_at, Some(1_000));
    std::fs::write(&path, b"{").unwrap();
    let rebuilt = Runtime::load(&path, 3_000).unwrap();
    assert!(!rebuilt.settings.enabled);
    assert_eq!(rebuilt.quiet_until, 63_000);
    assert!(path.with_extension("corrupt").exists());
}

#[test]
fn notifications_sequences_are_lossless_and_ids_are_account_bound() {
    let (mut item, _, _, _) = example();
    item.version = u64::MAX;
    let mut json = serde_json::to_value(&item).unwrap();
    assert_eq!(json["version"], "18446744073709551615");
    assert_eq!(
        serde_json::from_value::<Item>(json.clone())
            .unwrap()
            .version,
        u64::MAX
    );
    json["version"] = serde_json::json!("01");
    assert!(serde_json::from_value::<Item>(json).is_err());
    assert_ne!(item_id("a", &item.target), item_id("b", &item.target));
}

#[test]
fn notifications_provider_retry_and_ttl_are_bounded() {
    assert_eq!(policy::retry_at(1, 1_000, 10_000), Some(6_000));
    assert_eq!(policy::retry_at(1, 1_000, 6_000), None);
    assert_eq!(policy::retry_at(12, 1_000, u64::MAX), None);
    assert_eq!(policy::retry_at(11, 1_000, u64::MAX), Some(901_000));
    assert_eq!(
        policy::expires_at(Category::Action, 1_000, Some(5_000)),
        5_000
    );
    assert_eq!(
        policy::expires_at(Category::Security, 1_000, None),
        86_401_000
    );
}

#[test]
fn notifications_unverified_updates_keep_last_known_counts_without_claiming_a_new_count() {
    let store = Store::memory().unwrap();
    let (mut item, settings, mut runtime, mut ctx) = example();
    configure(&store, 0, settings).unwrap();
    upsert(&store, item.clone()).unwrap();
    item.version += 1;
    item.unread = None;
    upsert(&store, item.clone()).unwrap();
    let stored = store.notification_item(&item.id).unwrap().unwrap();
    assert_eq!(stored.unread, Some(2));
    assert!(stored.unverified);
    let count = counts(std::slice::from_ref(&stored), ctx.now);
    assert_eq!(count.actionable, 0);
    assert_eq!(count.unread, 2);
    assert!(count.unverified);
    ctx.now = 5000;
    let batch = next(&store, &mut runtime, &ctx, |_| Ok(())).unwrap();
    assert_eq!(batch.plans.len(), 1);
    assert_eq!(batch.plans[0].count, None);
    item.unread = Some(3);
    upsert(&store, item).unwrap();
    assert!(!store.notification_items().unwrap()[0].unverified);
}

#[test]
fn notifications_delivery_reserves_once_and_reconnects_silently_without_reading_business() {
    let store = Store::memory().unwrap();
    let (item, mut settings, mut runtime, mut ctx) = example();
    settings
        .categories
        .get_mut(&Category::Message)
        .unwrap()
        .sound = true;
    configure(&store, 0, settings).unwrap();
    upsert(&store, item.clone()).unwrap();
    let batch = next(&store, &mut runtime, &ctx, |_| Ok(())).unwrap();
    assert_eq!(batch.plans.len(), 1);
    let plan = &batch.plans[0];
    assert!(plan.sound);
    assert_eq!(runtime.messages, vec![4_000]);
    assert!(
        next(&store, &mut runtime, &ctx, |_| Ok(()))
            .unwrap()
            .plans
            .is_empty()
    );
    ctx.now = 9_000;
    let retry = next(&store, &mut runtime, &ctx, |_| {
        panic!("retry must not reserve")
    })
    .unwrap();
    assert_eq!(retry.plans[0].delivery_id, plan.delivery_id);
    assert!(!retry.plans[0].sound);
    assert_eq!(retry.plans[0].attempts, 2);
    result(
        &store,
        &plan.system_id,
        &plan.delivery_id,
        DeliveryResult::Submitted,
        ctx.now,
    )
    .unwrap();
    result(
        &store,
        &plan.system_id,
        &plan.delivery_id,
        DeliveryResult::Shown,
        ctx.now,
    )
    .unwrap();
    result(
        &store,
        &plan.system_id,
        &plan.delivery_id,
        DeliveryResult::Failed,
        ctx.now,
    )
    .unwrap();
    assert!(
        next(&store, &mut runtime, &ctx, |_| Ok(()))
            .unwrap()
            .plans
            .is_empty()
    );
    assert_eq!(
        counts(&store.notification_items().unwrap(), ctx.now).unread,
        2
    );
    assert_eq!(
        store.notification_item(&item.id).unwrap().unwrap().observed,
        0
    );
}

#[test]
fn notifications_cannot_return_a_plan_after_installation_storage_fails() {
    let store = Store::memory().unwrap();
    let (item, settings, mut runtime, ctx) = example();
    configure(&store, 0, settings).unwrap();
    upsert(&store, item.clone()).unwrap();
    assert!(
        next(&store, &mut runtime, &ctx, |_| Err(Error::Unavailable(
            "disk".into()
        )))
        .is_err()
    );
    assert!(store.notification_deliveries().unwrap().is_empty());
    assert_eq!(
        store.notification_item(&item.id).unwrap().unwrap().notified,
        0
    );
}

#[test]
fn notifications_same_source_new_requests_keep_distinct_todos_and_share_the_interval() {
    let store = Store::memory().unwrap();
    let (_, settings, mut runtime, mut ctx) = example();
    configure(&store, 0, settings).unwrap();
    let mut first = Item::new(
        "account-a".into(),
        Target::Approval("first".into()),
        1,
        1_000,
    );
    first.group = "verified-source".into();
    first.actionable = true;
    upsert(&store, first.clone()).unwrap();
    let plan = next(&store, &mut runtime, &ctx, |_| Ok(()))
        .unwrap()
        .plans
        .remove(0);
    result(
        &store,
        &plan.system_id,
        &plan.delivery_id,
        DeliveryResult::Submitted,
        ctx.now,
    )
    .unwrap();
    let mut second = Item::new(
        "account-a".into(),
        Target::Approval("second".into()),
        1,
        5_000,
    );
    second.group = first.group;
    second.actionable = true;
    upsert(&store, second).unwrap();
    ctx.now = 8_000;
    let waiting = next(&store, &mut runtime, &ctx, |_| Ok(())).unwrap();
    assert!(waiting.plans.is_empty());
    assert_eq!(waiting.next_due, Some(64_000));
    assert_eq!(
        counts(&store.notification_items().unwrap(), ctx.now).actionable,
        2
    );
    ctx.now = 64_000;
    let next_plan = next(&store, &mut runtime, &ctx, |_| Ok(())).unwrap();
    assert_eq!(next_plan.plans.len(), 1);
    assert_ne!(next_plan.plans[0].delivery_id, plan.delivery_id);
}

#[test]
fn notifications_source_interval_survives_a_cross_source_summary() {
    let store = Store::memory().unwrap();
    let (_, settings, mut runtime, mut ctx) = example();
    configure(&store, 0, settings).unwrap();
    for source in ["first-source", "second-source"] {
        let mut item = Item::new(
            "account-a".into(),
            Target::Approval(source.into()),
            1,
            1_000,
        );
        item.group = source.into();
        item.actionable = true;
        upsert(&store, item).unwrap();
    }
    let plan = next(&store, &mut runtime, &ctx, |_| Ok(()))
        .unwrap()
        .plans
        .remove(0);
    assert_eq!(plan.covered.len(), 2);
    result(
        &store,
        &plan.system_id,
        &plan.delivery_id,
        DeliveryResult::Submitted,
        ctx.now,
    )
    .unwrap();
    let mut item = Item::new(
        "account-a".into(),
        Target::Approval("new-request".into()),
        1,
        5_000,
    );
    item.group = "first-source".into();
    item.actionable = true;
    upsert(&store, item).unwrap();
    ctx.now = 8_000;
    let batch = next(&store, &mut runtime, &ctx, |_| Ok(())).unwrap();
    assert!(
        batch.plans.is_empty(),
        "a summary must still reserve each source's interval"
    );
    assert_eq!(batch.next_due, Some(64_000));
    ctx.now = 64_000;
    assert_eq!(
        next(&store, &mut runtime, &ctx, |_| Ok(()))
            .unwrap()
            .plans
            .len(),
        1
    );
}

#[test]
fn notifications_retry_respects_a_disabled_agent_opt_in() {
    let store = Store::memory().unwrap();
    let (mut item, mut settings, mut runtime, mut ctx) = example();
    item.target = Target::AgentMail("agent".into());
    item.id = item_id(&item.account, &item.target);
    settings.agent_mail.insert("agent".into(), true);
    let mut settings = configure(&store, 0, settings).unwrap();
    upsert(&store, item).unwrap();
    let plan = next(&store, &mut runtime, &ctx, |_| Ok(()))
        .unwrap()
        .plans
        .remove(0);
    result(
        &store,
        &plan.system_id,
        &plan.delivery_id,
        DeliveryResult::Failed,
        ctx.now,
    )
    .unwrap();
    settings.agent_mail.insert("agent".into(), false);
    configure(&store, settings.revision, settings).unwrap();
    ctx.now += 5_000;
    let batch = next(&store, &mut runtime, &ctx, |_| Ok(())).unwrap();
    assert!(batch.plans.is_empty());
    assert!(batch.remove.contains(&plan.system_id));
}

#[test]
fn notifications_retry_does_not_repeat_a_visible_security_phase() {
    let store = Store::memory().unwrap();
    let (_, settings, mut runtime, mut ctx) = example();
    configure(&store, 0, settings).unwrap();
    let mut item = Item::new(
        "account-a".into(),
        Target::Recovery("incident".into()),
        1,
        1_000,
    );
    item.actionable = true;
    item.deadline = Some(ctx.now + 3_600_000);
    upsert(&store, item).unwrap();
    ctx.foreground = true;
    let plan = next(&store, &mut runtime, &ctx, |_| Ok(()))
        .unwrap()
        .plans
        .remove(0);
    assert_eq!(plan.covered[0].stage, 3);
    ctx.visible = plan.covered.clone();
    ctx.now += 5_000;
    let batch = next(&store, &mut runtime, &ctx, |_| Ok(())).unwrap();
    assert!(batch.plans.is_empty());
}

#[test]
fn notifications_disabling_an_account_queues_revoke_while_locked() {
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    let mut draft = configure(
        &client.workspace.store,
        0,
        Settings {
            enabled: true,
            ..Default::default()
        },
    )
    .unwrap();
    client.lock();
    draft.enabled = false;
    configure(&client.workspace.store, draft.revision, draft).unwrap();
    assert_eq!(
        client
            .workspace
            .store
            .meta::<bool>("notification:revoke_pending")
            .unwrap(),
        Some(true)
    );
}

#[test]
fn notifications_one_sound_budget_covers_different_categories_in_the_same_pass() {
    let store = Store::memory().unwrap();
    let (message, mut settings, mut runtime, ctx) = example();
    settings
        .categories
        .get_mut(&Category::Message)
        .unwrap()
        .sound = true;
    configure(&store, 0, settings).unwrap();
    upsert(&store, message).unwrap();
    let mut security = Item::new(
        "account-a".into(),
        Target::Recovery("digest".into()),
        1,
        1_000,
    );
    security.actionable = true;
    upsert(&store, security).unwrap();
    let plans = next(&store, &mut runtime, &ctx, |_| Ok(())).unwrap().plans;
    assert_eq!(plans.len(), 2);
    assert_eq!(plans.iter().filter(|plan| plan.sound).count(), 1);
}

#[test]
fn notifications_network_failure_needs_unfinished_work_and_fifteen_minutes() {
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    let failed = crate::sync::LastSync {
        at: 1_000,
        error: Some("NETWORK_UNAVAILABLE".into()),
        stage: None,
        ok_at: None,
    };
    client
        .workspace
        .store
        .set_meta("last-sync", &failed)
        .unwrap();
    assert_eq!(client.notification_health(1_000).unwrap(), None);
    client
        .vault_create(
            "note",
            &crate::vault::Changes {
                title: Some("unsynced".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(client.notification_health(1_000).unwrap(), Some(901_000));
    assert_eq!(client.notification_health(900_999).unwrap(), Some(901_000));
    assert_eq!(
        counts(
            &client.workspace.store.notification_items().unwrap(),
            900_999
        )
        .actionable,
        0
    );
    client.notification_health(901_000).unwrap();
    client.notification_health(902_000).unwrap();
    assert_eq!(
        counts(
            &client.workspace.store.notification_items().unwrap(),
            902_000
        )
        .actionable,
        1
    );
    client
        .workspace
        .store
        .set_meta(
            "last-sync",
            &crate::sync::LastSync {
                at: 903_000,
                error: None,
                stage: None,
                ok_at: Some(903_000),
            },
        )
        .unwrap();
    client.notification_health(903_000).unwrap();
    assert_eq!(
        counts(
            &client.workspace.store.notification_items().unwrap(),
            903_000
        )
        .actionable,
        0
    );
}

#[test]
fn notifications_public_recovery_refresh_preserves_verified_deadline_without_guessing_one() {
    let store = Store::memory().unwrap();
    let mut item = Item::new("a".into(), Target::Recovery("digest".into()), 1, 1_000);
    item.actionable = true;
    upsert(&store, item.clone()).unwrap();
    assert_eq!(
        store.notification_item(&item.id).unwrap().unwrap().deadline,
        None
    );
    item.deadline = Some(100_000);
    upsert(&store, item.clone()).unwrap();
    item.deadline = None;
    item.version = 2;
    upsert(&store, item.clone()).unwrap();
    assert_eq!(
        store.notification_item(&item.id).unwrap().unwrap().deadline,
        Some(100_000)
    );
}

#[test]
fn notifications_lock_keeps_opt_in_but_logout_disables_only_that_account() {
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    let account = crate::testing::account().id;
    configure(
        &client.workspace.store,
        0,
        Settings {
            enabled: true,
            ..Default::default()
        },
    )
    .unwrap();
    client.lock();
    assert!(settings(&client.workspace.store).unwrap().enabled);
    client.logout().unwrap();
    client
        .with_notification_store(&account, |store| {
            assert!(!settings(store)?.enabled);
            assert_eq!(
                store.meta::<bool>("notification:revoke_pending")?,
                Some(true)
            );
            Ok(())
        })
        .unwrap();
}

#[test]
fn notifications_verified_contract_claims_only_its_legacy_phase() {
    let dir = crate::testing::TempDir::new();
    let client = crate::testing::unlocked(&dir.0, false, None);
    let contract = "a1".repeat(32);
    let unknown = "b1".repeat(32);
    let path = client.notification_runtime_path();
    let mut runtime = Runtime::load(&path, 1000).unwrap();
    runtime.reminders_imported = true;
    runtime.legacy_plans.insert(contract.clone(), 2);
    runtime.legacy_plans.insert(unknown.clone(), 3);
    runtime.save(&path).unwrap();
    client.project_plan_notification(&serde_json::json!({
        "current":{"contract":contract,"ends_at_ms":100_000,"terminated_at_ms":null},"next":null
    }), 1_000).unwrap();
    let item = client
        .workspace
        .store
        .notification_items()
        .unwrap()
        .into_iter()
        .find(|item| matches!(item.target, Target::Plan(_)))
        .unwrap();
    assert_eq!(item.stage, 2);
    let runtime = Runtime::load(&path, 2_000).unwrap();
    assert!(!runtime.legacy_plans.contains_key(&contract));
    assert_eq!(runtime.legacy_plans.get(&unknown), Some(&3));
}

#[test]
fn notifications_recent_limit_and_clear_never_hide_unfinished_work() {
    let store = Store::memory().unwrap();
    for index in 1..=1001 {
        let mut item = Item::new(
            "a".into(),
            Target::Inbox(Some(index.to_string())),
            1,
            1000 + index,
        );
        item.open = false;
        upsert(&store, item).unwrap();
    }
    let mut pending = Item::new("a".into(), Target::Recovery("pending".into()), 1, 1);
    pending.actionable = true;
    upsert(&store, pending.clone()).unwrap();
    let page = list(&store, false, 1000, 100, 3000).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, pending.id);
    assert_eq!(page.next, None);
    store
        .set_meta("notification:cleared_at", &3000_u64)
        .unwrap();
    let page = list(&store, false, 0, 100, RETENTION_MS + 3001).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, pending.id);
    assert!(page.items[0].open);
}

#[test]
fn notifications_unlocking_ends_placeholders_so_a_later_locked_hint_alerts_again() {
    let dir = crate::testing::TempDir::new();
    let client = crate::testing::unlocked(&dir.0, false, None);
    let account = crate::testing::account().id.to_string();
    let store = &client.workspace.store;
    let (_, settings, runtime, ctx) = example();
    for category in [Category::Action, Category::Message] {
        let mut hint = Item::new(account.clone(), Target::Summary(category), 1_000, 1_000);
        hint.unread = None;
        hint.quiet = true;
        upsert(store, hint.clone()).unwrap();
        let mut shown = store.notification_item(&hint.id).unwrap().unwrap();
        shown.stage = 1;
        shown.notified = shown.version;
        shown.attempted_at = Some(1_000);
        store.set_notification_item(&shown).unwrap();
    }
    assert!(counts(&store.notification_items().unwrap(), 2_000).unverified);
    client.reconcile_notifications(5_000).unwrap();
    let items = store.notification_items().unwrap();
    assert!(items.iter().all(|item| !item.open));
    assert!(!counts(&items, 5_000).unverified);
    let mut again = Item::new(account, Target::Summary(Category::Action), 9_000, 9_000);
    again.unread = None;
    again.quiet = true;
    upsert(store, again.clone()).unwrap();
    let item = store.notification_item(&again.id).unwrap().unwrap();
    let ctx = Context { now: 70_000, ..ctx };
    assert_eq!(
        evaluate(&item, &settings, &runtime, &ctx),
        Decision::Present {
            stage: 1,
            sound: false,
            in_app: false,
            update: false
        }
    );
}

#[test]
fn notifications_badge_counts_the_selected_account_before_any_opt_in() {
    let dir = crate::testing::TempDir::new();
    let client = crate::testing::unlocked(&dir.0, false, None);
    let mut pending = Item::new(
        crate::testing::account().id.to_string(),
        Target::Approval("request".into()),
        1,
        1_000,
    );
    pending.actionable = true;
    upsert(&client.workspace.store, pending).unwrap();
    let status = client
        .notification_status(&Runtime::default(), 2_000)
        .unwrap();
    assert_eq!(status["accounts"][0]["enabled"], false);
    assert_eq!(status["counts"]["actionable"], 1);
    assert_eq!(status["badge"], 1);
}

#[test]
fn notifications_pruning_a_tombstone_drops_its_item_markers() {
    let store = Store::memory().unwrap();
    let mut ended = Item::new("a".into(), Target::Inbox(Some("ended".into())), 1, 1_000);
    ended.open = false;
    upsert(&store, ended.clone()).unwrap();
    let mut unread = Item::new("a".into(), Target::Inbox(Some("unread".into())), 1, 1_000);
    unread.unread = Some(1);
    upsert(&store, unread.clone()).unwrap();
    let kinds = ["seen", "remote-observed", "observed-ack", "snooze"];
    for id in [&ended.id, &unread.id] {
        for kind in kinds {
            store
                .set_meta(&format!("notification:{kind}:{id}"), &1_u64)
                .unwrap();
        }
    }
    store.prune_notifications(2_000).unwrap();
    assert!(store.notification_item(&ended.id).unwrap().is_none());
    assert!(store.notification_item(&unread.id).unwrap().is_some());
    for kind in kinds {
        let marker = |id: &str| {
            store
                .meta::<u64>(&format!("notification:{kind}:{id}"))
                .unwrap()
        };
        assert_eq!(marker(&ended.id), None);
        assert_eq!(marker(&unread.id), Some(1));
    }
}

#[tokio::test]
async fn notifications_failed_capability_probe_keeps_the_last_answer() {
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    for key in ["notification:publish_v1", "notification:remote_v1"] {
        client.workspace.store.set_meta(key, &true).unwrap();
    }
    // The test relay refuses connections: a transport error, not a missing endpoint.
    assert!(refresh_capabilities(&client.hold()).await.is_err());
    for key in ["notification:publish_v1", "notification:remote_v1"] {
        assert_eq!(
            client.workspace.store.meta::<bool>(key).unwrap(),
            Some(true)
        );
    }
}
