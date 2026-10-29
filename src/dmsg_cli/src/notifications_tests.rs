use super::*;
use dmsg_client::{notifications as notice, testing};

fn fixture(dir: &testing::TempDir) -> (Daemon, String) {
    let c = testing::unlocked(&dir.path("workspace"), false, None);
    let account = testing::account().id;
    let now = now_ms();
    let mut item = notice::Item::new(
        account.to_string(),
        notice::Target::Approval("request".into()),
        1,
        now.saturating_sub(4_000),
    );
    item.actionable = true;
    c.with_notification_store(&account, |store| {
        notice::configure(
            store,
            0,
            notice::Settings {
                enabled: true,
                ..Default::default()
            },
        )?;
        notice::upsert(store, item.clone())?;
        Ok(())
    })
    .unwrap();
    let path = c.notification_runtime_path();
    let mut runtime = notice::Runtime::load(&path, now).unwrap();
    runtime.settings.enabled = true;
    runtime.save(&path).unwrap();
    let home = Home {
        dir: dir.path("home"),
    };
    let daemon = Daemon::new(
        "owner".into(),
        0,
        home.settings().unwrap(),
        c,
        dmsg_agent::Runtime::new(&dmsg_agent::Options::default()).unwrap(),
        crate::mcp_events::McpEvents::new(&[]).unwrap(),
    );
    (daemon, item.id)
}

#[tokio::test]
async fn notifications_headless_queries_do_not_consume_the_native_presentation() {
    let dir = testing::TempDir::new();
    let (d, id) = fixture(&dir);
    let status = d
        .call_notifications("notifications/status", &json!({}))
        .await
        .unwrap();
    assert_eq!(status["counts"]["actionable"], 1);
    let next = d
        .call_notifications("notifications/next", &json!({}))
        .await
        .unwrap();
    assert_eq!(next["plans"], json!([]));
    assert_eq!(
        d.client()
            .lock()
            .await
            .with_notification_store(&testing::account().id, |store| Ok(store
                .notification_item(&id)?
                .unwrap()
                .notified))
            .unwrap(),
        0
    );

    // Only an initialized native owner connection consumes plans. HTTP/CLI
    // queries cannot take the desktop's plans or extend the idle lock.
    let before = d.activity.load(Ordering::Relaxed);
    let request = RpcRequest {
        jsonrpc: "2.0".into(),
        id: json!(1),
        method: "notifications/next".into(),
        params: json!({}),
    };
    let denied = d.dispatch(request, &Session::Owner, false).await;
    assert_eq!(denied["error"]["data"]["code"], "FORBIDDEN");
    assert_eq!(before, d.activity.load(Ordering::Relaxed));
    let session = d.status().await.unwrap()["session"].clone();
    let context = notice::policy::Context {
        allowed: true,
        ..Default::default()
    };
    d.call_notifications(
        "notifications/context",
        &json!({"session":session,"context":context}),
    )
    .await
    .unwrap();
    let next = d
        .call_notifications("notifications/next", &json!({}))
        .await
        .unwrap();
    assert_eq!(next["plans"].as_array().unwrap().len(), 1);
    let item = d
        .client()
        .lock()
        .await
        .with_notification_store(&testing::account().id, |store| {
            Ok(store.notification_item(&id)?.unwrap())
        })
        .unwrap();
    assert!(item.open && item.actionable);
    assert_eq!(item.observed, 0);
}

#[tokio::test]
async fn notifications_late_visibility_and_foreign_list_cursors_are_rejected() {
    let dir = testing::TempDir::new();
    let (d, id) = fixture(&dir);
    let session = d.status().await.unwrap()["session"].clone();
    d.client().lock().await.lock();
    assert_eq!(
        d.call_notifications(
            "notifications/observed",
            &json!({"id":id,"version":"1","session":session})
        )
        .await
        .unwrap_err(),
        Error::Locked
    );
    let refused = d
        .call_notifications(
            "notifications/list",
            &json!({"scope":"open","cursor":format!("{}:recent:1",testing::account().id)}),
        )
        .await;
    assert!(matches!(refused, Err(Error::InvalidInput(_))));
    let status = d
        .call_notifications("notifications/status", &json!({}))
        .await
        .unwrap();
    assert_eq!(status["counts"]["actionable"], 1);
    d.call_notifications("notifications/clear_recent", &json!({}))
        .await
        .unwrap();
    let items = d
        .call_notifications("notifications/list", &json!({"scope":"open"}))
        .await
        .unwrap();
    assert_eq!(items["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn notifications_settings_revision_rejects_an_overwritten_ui_draft() {
    let dir = testing::TempDir::new();
    let (d, _) = fixture(&dir);
    let read = d
        .call_notifications("notifications/settings", &json!({}))
        .await
        .unwrap();
    let mut draft = read["account"].clone();
    draft["enabled"] = false.into();
    let args = json!({"scope":"account","expected_revision":1,"settings":draft});
    d.call_notifications("notifications/settings_set", &args)
        .await
        .unwrap();
    assert_eq!(
        d.call_notifications("notifications/settings_set", &args)
            .await
            .unwrap_err(),
        Error::VersionConflict
    );
    let read = d
        .call_notifications("notifications/settings", &json!({}))
        .await
        .unwrap();
    assert_eq!(read["account"]["enabled"], false);
    assert_eq!(read["account"]["revision"], 2);
    assert_eq!(read["account"]["remote_revision"], 0);
}
