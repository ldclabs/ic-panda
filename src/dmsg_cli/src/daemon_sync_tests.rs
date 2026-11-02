use super::tests::stalled;
use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Default)]
struct RelayState {
    exports: AtomicU64,
    deltas: AtomicU64,
    uploads: AtomicU64,
    busy: AtomicBool,
    hold_export: AtomicBool,
    hold_upload: AtomicBool,
    entered: Notify,
    release: Notify,
}

struct Fixture {
    daemon: Arc<Daemon>,
    state: Arc<RelayState>,
    server: tokio::task::JoinHandle<()>,
    _dir: dmsg_client::testing::TempDir,
}

impl Fixture {
    async fn new() -> Self {
        let dir = dmsg_client::testing::TempDir::new();
        let (daemon, listener, _ic) = stalled(&dir).await;
        let state = Arc::new(RelayState::default());
        let server_state = state.clone();
        let server = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let state = server_state.clone();
                tokio::spawn(async move {
                    let mut bytes = Vec::new();
                    loop {
                        let mut buf = [0; 8192];
                        let n = socket.read(&mut buf).await.unwrap();
                        if n == 0 {
                            return;
                        }
                        bytes.extend_from_slice(&buf[..n]);
                        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                            let header = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                            let length = header
                                .lines()
                                .find_map(|l| l.strip_prefix("content-length:"))
                                .map(|n| n.trim().parse::<usize>().unwrap())
                                .unwrap_or(0);
                            if bytes.len() >= end + 4 + length {
                                break;
                            }
                        }
                    }
                    let request = String::from_utf8_lossy(&bytes);
                    let path = request
                        .lines()
                        .next()
                        .unwrap()
                        .split_whitespace()
                        .nth(1)
                        .unwrap();
                    let account = dmsg_client::testing::account().id.to_string();
                    let data = if path.ends_with("/exports") {
                        state.exports.fetch_add(1, Ordering::SeqCst);
                        if state.hold_export.swap(false, Ordering::SeqCst) {
                            state.entered.notify_one();
                            state.release.notified().await;
                        }
                        if state.busy.load(Ordering::SeqCst) {
                            json!({"ok":false,"error":{"code":"SYNC_BUSY","retry_after_ms":90_000}})
                        } else {
                            json!({"ok":true,"data":{"account":account,"scope":"cloud_snapshot","through":0,
                                    "counts":[],"content_sync":1,"export_id":"aa".repeat(32),"expires_at":now_ms()+60_000}})
                        }
                    } else if path.ends_with("/close") {
                        json!({"ok":true,"data":{"closed":true}})
                    } else if path.contains("/exports/") {
                        json!({"ok":true,"data":{"entries":[],"next_cursor":0,"inventory_end":true}})
                    } else if path.contains("/content/changes?") {
                        state.deltas.fetch_add(1, Ordering::SeqCst);
                        json!({"ok":true,"data":{"content_sync":1,"account":account,"entries":[],"through":0,"next_cursor":0,"end":true}})
                    } else if path.ends_with("/uploads") {
                        state.uploads.fetch_add(1, Ordering::SeqCst);
                        if state.hold_upload.swap(false, Ordering::SeqCst) {
                            state.entered.notify_one();
                            state.release.notified().await;
                        }
                        json!({"ok":false,"error":{"code":"QUOTA_EXCEEDED"}})
                    } else if path.starts_with("/v1/inboxes/") {
                        json!({"ok":false,"error":{"code":"RECOVERY_INCOMPLETE"}})
                    } else {
                        assert!(
                            path.contains("/operations/") || path.contains("/uploads/"),
                            "{path}"
                        );
                        json!({"ok":false,"error":{"code":"NOT_FOUND"}})
                    };
                    let body = data.to_string();
                    let _ = socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",body.len()).as_bytes()).await;
                });
            }
        });
        Self {
            daemon,
            state,
            server,
            _dir: dir,
        }
    }

    async fn draft(&self) {
        self.daemon
            .client
            .lock()
            .await
            .vault_create(
                "note",
                &dmsg_client::vault::Changes {
                    title: Some("new local work".into()),
                    ..Default::default()
                },
            )
            .unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

#[tokio::test]
async fn manual_sync_resumes_uploads_after_a_queued_automatic_pass_hits_quota() {
    let fixture = Fixture::new().await;
    fixture.draft().await;
    fixture.state.hold_upload.store(true, Ordering::SeqCst);
    let automatic = tokio::spawn({
        let daemon = fixture.daemon.clone();
        async move { daemon.sync_pass(SyncTrigger::Automatic, None).await }
    });
    tokio::time::timeout(Duration::from_secs(3), fixture.state.entered.notified())
        .await
        .unwrap();
    let owner = fixture.daemon.call_daemon("sync/run", &Value::Null);
    let release = async {
        tokio::task::yield_now().await;
        fixture.state.release.notify_one();
    };
    let (owner, _) = tokio::join!(owner, release);
    assert_eq!(owner.unwrap()["upload_error"], "QUOTA_EXCEEDED");
    automatic.await.unwrap().unwrap();
    assert_eq!(fixture.state.uploads.load(Ordering::SeqCst), 2);
    assert_eq!(fixture.state.exports.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn inbox_key_recovery_uses_incremental_receiving_and_honors_server_cooldown() {
    let fixture = Fixture::new().await;
    fixture
        .daemon
        .call_daemon("sync/run", &Value::Null)
        .await
        .unwrap();
    fixture.draft().await;
    for _ in 0..3 {
        assert!(
            fixture
                .daemon
                .call_more("inbox/list", &Value::Null)
                .await
                .is_err()
        );
    }
    assert_eq!(fixture.state.exports.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.state.deltas.load(Ordering::SeqCst), 3);
    assert_eq!(fixture.state.uploads.load(Ordering::SeqCst), 0);
    let fixture = Fixture::new().await;
    fixture.state.busy.store(true, Ordering::SeqCst);
    assert!(
        fixture
            .daemon
            .call_daemon("sync/run", &Value::Null)
            .await
            .is_err()
    );
    assert!(
        fixture
            .daemon
            .call_more("inbox/list", &Value::Null)
            .await
            .is_err()
    );
    assert_eq!(fixture.state.exports.load(Ordering::SeqCst), 1);
    assert_eq!(
        fixture.daemon.status().await.unwrap()["last_sync"]["error"],
        "SYNC_BUSY"
    );
}

#[tokio::test]
async fn progress_and_initialization_are_local_and_a_cancelled_pass_clears_running() {
    let fixture = Fixture::new().await;
    fixture.state.hold_export.store(true, Ordering::SeqCst);
    let sync = tokio::spawn({
        let daemon = fixture.daemon.clone();
        async move { daemon.call_daemon("sync/run", &Value::Null).await }
    });
    tokio::time::timeout(Duration::from_secs(3), fixture.state.entered.notified())
        .await
        .unwrap();
    let status = fixture.daemon.status().await.unwrap();
    assert_eq!(status["sync_initialized"], false);
    assert_eq!(status["sync_progress"]["stage"], "export_start");
    assert_eq!(status["syncing"], true);
    fixture
        .daemon
        .call_daemon("app/activity", &Value::Null)
        .await
        .unwrap();
    fixture
        .daemon
        .call_vault("vault/list", &Value::Null)
        .await
        .unwrap();
    assert!(!sync.is_finished());
    sync.abort();
    let _ = sync.await;
    assert_eq!(fixture.daemon.status().await.unwrap()["syncing"], false);
}

#[tokio::test]
async fn unlock_sync_starts_while_unrelated_mailbox_work_is_stalled() {
    let dir = dmsg_client::testing::TempDir::new();
    let (daemon, relay, _ic) = stalled(&dir).await;
    let maintenance = tokio::spawn({
        let daemon = daemon.clone();
        async move { daemon.connect_pass().await }
    });
    let (_mailbox, _) = tokio::time::timeout(Duration::from_secs(3), relay.accept())
        .await
        .unwrap()
        .unwrap();
    let scheduler = tokio::spawn(content_sync::run(daemon.clone()));
    daemon.request_sync();
    let (mut export, _) = tokio::time::timeout(Duration::from_secs(2), relay.accept())
        .await
        .unwrap()
        .unwrap();
    let mut bytes = [0; 8192];
    let n = export.read(&mut bytes).await.unwrap();
    assert!(
        String::from_utf8_lossy(&bytes[..n])
            .lines()
            .next()
            .unwrap()
            .contains("/exports ")
    );
    assert!(!maintenance.is_finished());
    scheduler.abort();
    maintenance.abort();
}

#[tokio::test]
async fn inbox_recovery_cannot_start_in_a_replacement_session() {
    let fixture = Fixture::new().await;
    let generation = fixture.daemon.client.lock().await.session_generation();
    fixture
        .daemon
        .call_daemon("session/lock", &Value::Null)
        .await
        .unwrap();
    assert!(matches!(
        fixture
            .daemon
            .sync_pass(SyncTrigger::Inbox, Some(generation))
            .await,
        Err(Error::Locked)
    ));
    assert_eq!(fixture.state.exports.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn queued_automatic_sync_rechecks_the_previous_pass_cooldown() {
    let fixture = Fixture::new().await;
    fixture.state.busy.store(true, Ordering::SeqCst);
    fixture.state.hold_export.store(true, Ordering::SeqCst);
    let owner = tokio::spawn({
        let daemon = fixture.daemon.clone();
        async move { daemon.call_daemon("sync/run", &Value::Null).await }
    });
    tokio::time::timeout(Duration::from_secs(3), fixture.state.entered.notified())
        .await
        .unwrap();
    let automatic = fixture.daemon.sync_pass(SyncTrigger::Automatic, None);
    let release = async {
        tokio::task::yield_now().await;
        fixture.state.release.notify_one();
    };
    let (automatic, _) = tokio::join!(automatic, release);
    assert!(automatic.unwrap().is_none());
    assert!(owner.await.unwrap().is_err());
    assert_eq!(fixture.state.exports.load(Ordering::SeqCst), 1);
}
