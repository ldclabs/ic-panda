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
