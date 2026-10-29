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
    assert_eq!(counts(&[stored.clone()], 4_000).unread, 0);
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
