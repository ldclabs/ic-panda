use super::*;
use dmsg_client::testing;

#[tokio::test]
async fn recovery_watch_notifies_and_status_warns_without_a_login_or_resident_key() {
    let dir = testing::TempDir::new();
    let mut client = testing::unlocked(&dir.0, false, None);
    let mut security = testing::security(1, Default::default());
    security.certified_at = now_ms();
    security.snapshot.pending_recovery_digest = Some(dmsg_types::Hash::new([7; 32]));
    testing::observe_security(&mut client, &security).unwrap();
    client.lock();
    let home = Home {
        dir: dir.path("daemon"),
    };
    let daemon = Arc::new(Daemon::new(
        "owner".into(),
        0,
        home.settings().unwrap(),
        client,
        dmsg_agent::Runtime::new(&dmsg_agent::Options::default()).unwrap(),
        crate::mcp_events::McpEvents::new(&[]).unwrap(),
    ));
    let activity = daemon.activity.load(Ordering::Relaxed);
    let mut events = daemon.events.subscribe();
    let task = tokio::spawn(crate::recovery_watch::run(daemon.clone()));
    let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event["type"], "recovery/changed");
    assert!(event["data"].is_null());
    let status = daemon.status().await.unwrap();
    assert_eq!(status["state"], "locked");
    assert_eq!(status["resident"], false);
    assert!(status["principal"].is_null());
    assert_eq!(status["recovery"]["pending"], true);
    assert_eq!(status["recovery"]["stale"], false);
    assert_eq!(status["recovery"]["digest"], "07".repeat(32));
    assert_eq!(daemon.activity.load(Ordering::Relaxed), activity);
    task.abort();
}
