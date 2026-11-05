use super::*;
use dmsg_client::testing;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Fixture {
    engine: MobileEngine,
    uploads: Arc<AtomicU64>,
    exports: Arc<AtomicU64>,
    deltas: Arc<AtomicU64>,
    delay_page: Arc<AtomicU64>,
    fail_delta: Arc<Mutex<Option<String>>>,
    server: tokio::task::JoinHandle<()>,
    _dir: testing::TempDir,
}

impl Fixture {
    async fn new(first_error: Option<&str>) -> Self {
        let dir = testing::TempDir::new();
        let mut client = testing::unlocked(&dir.0, false, None);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        testing::relay_to(
            &mut client,
            &format!("http://{}", listener.local_addr().unwrap()),
        );
        testing::warm(&mut client);
        let uploads = Arc::new(AtomicU64::new(0));
        let requests = uploads.clone();
        let exports = Arc::new(AtomicU64::new(0));
        let export_count = exports.clone();
        let deltas = Arc::new(AtomicU64::new(0));
        let delta_count = deltas.clone();
        let delay_page = Arc::new(AtomicU64::new(0));
        let page_delay = delay_page.clone();
        let fail_delta = Arc::new(Mutex::new(None::<String>));
        let delta_error = fail_delta.clone();
        let mut first_error = first_error.map(str::to_owned);
        let server = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut part = [0; 8192];
                    let n = socket.read(&mut part).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&part[..n]);
                    if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                        let length = headers
                            .lines()
                            .find_map(|s| s.strip_prefix("content-length:"))
                            .map(|n| n.trim().parse::<usize>().unwrap())
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                let request = String::from_utf8_lossy(&bytes);
                let mut fields = request.lines().next().unwrap().split_whitespace();
                let (method, path) = (fields.next().unwrap(), fields.next().unwrap());
                let body = if path.starts_with("/v1/inboxes/") {
                    json!({"ok":false,"error":{"code":"RECOVERY_INCOMPLETE"}})
                } else if path.ends_with("/exports") {
                    export_count.fetch_add(1, Ordering::SeqCst);
                    match first_error.take() {
                        Some(code) => {
                            json!({"ok":false,"error":{"code":code,"retry_after_ms":90_000}})
                        }
                        None => json!({"ok":true,"data":{
                            "account":testing::account().id.to_string(),"scope":"cloud_snapshot",
                            "through":0,"counts":[],"content_sync":1,"export_id":"aa".repeat(32),
                            "expires_at":dmsg_client::now_ms()+60_000
                        }}),
                    }
                } else if path.ends_with("/close") {
                    json!({"ok":true,"data":{"closed":true}})
                } else if path.contains("/exports/") {
                    tokio::time::sleep(std::time::Duration::from_millis(
                        page_delay.load(Ordering::SeqCst),
                    ))
                    .await;
                    json!({"ok":true,"data":{"entries":[],"next_cursor":0,"inventory_end":true}})
                } else if path.contains("/content/changes?") {
                    delta_count.fetch_add(1, Ordering::SeqCst);
                    if let Some(code) = delta_error.lock().unwrap().take() {
                        json!({"ok":false,"error":{"code":code,"retry_after_ms":90_000}})
                    } else {
                        json!({"ok":true,"data":{"content_sync":1,"account":testing::account().id.to_string(),
                        "entries":[],"through":0,"next_cursor":0,"end":true}})
                    }
                } else if method == "POST" && path.ends_with("/uploads") {
                    requests.fetch_add(1, Ordering::SeqCst);
                    json!({"ok":false,"error":{"code":"QUOTA_EXCEEDED"}})
                } else {
                    assert!(
                        path.contains("/uploads/") || path.contains("/operations/"),
                        "{path}"
                    );
                    json!({"ok":false,"error":{"code":"NOT_FOUND"}})
                };
                let body = body.to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
        });
        Self {
            engine: MobileEngine {
                config: client.config().clone(),
                host: ScopedHost::new(Host::start(client, runtime().handle()), 0),
                platform: Arc::new(crate::platform::tests::PasscodeFallback::default()),
                login: Arc::default(),
                staging: dir.path("staging"),
                approval_turn: Arc::default(),
                read_turns: Arc::default(),
                sync_tracker: Arc::default(),
                hints_redial: Arc::default(),
                sync_signal: Arc::default(),
                plan_checked: Arc::default(),
                transfers: Arc::default(),
            },
            uploads,
            exports,
            deltas,
            delay_page,
            fail_delta,
            server,
            _dir: dir,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.engine.host.inner.close();
        self.server.abort();
    }
}

#[tokio::test]
async fn background_completion_never_starts_a_content_download_for_pending_edits() {
    let fixture = Fixture::new(None).await;
    let engine = &fixture.engine;
    engine
        .host
        .with(|c| {
            c.vault_create(
                "note",
                &dmsg_client::vault::Changes {
                    title: Some("pending edit".into()),
                    ..Default::default()
                },
            )
        })
        .await
        .unwrap();
    assert_eq!(
        engine.flush_content(OperationToken::new()).await.unwrap(),
        0
    );
    assert_eq!(fixture.exports.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.deltas.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.uploads.load(Ordering::SeqCst), 0);
    assert_eq!(engine.session_state().await.unwrap().pending_sync, 1);
    assert!(!engine.content_sync_status().await.unwrap().initialized);
}

#[tokio::test]
async fn a_network_retry_keeps_the_hint_connection_until_explicitly_redialed() {
    let fixture = Fixture::new(Some("UNAVAILABLE")).await;
    let engine = &fixture.engine;
    assert!(engine.sync(true, OperationToken::new()).await.is_err());
    let redial = engine.hints_redial.notified();
    tokio::pin!(redial);
    redial.as_mut().enable();
    assert!(engine.retry_sync_early(true).await.unwrap());
    assert!(
        engine
            .content_sync_status()
            .await
            .unwrap()
            .retry_at_ms
            .unwrap()
            <= dmsg_client::now_ms()
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), &mut redial)
            .await
            .is_err()
    );
    engine.reconnect_hints();
    tokio::time::timeout(std::time::Duration::from_secs(1), redial)
        .await
        .unwrap();
}

#[tokio::test]
async fn manual_sync_retries_a_failure_requiring_user_action() {
    let fixture = Fixture::new(Some("INTEGRITY_FAILED")).await;
    let engine = &fixture.engine;
    assert!(matches!(engine.sync(true, OperationToken::new()).await,
        Err(MobileError::Failure { code }) if code == "INTEGRITY_FAILED"));
    let paused: Value =
        serde_json::from_str(&engine.sync(true, OperationToken::new()).await.unwrap().json)
            .unwrap();
    assert_eq!(paused["paused"], true);
    let retry: Value = serde_json::from_str(
        &engine
            .sync(false, OperationToken::new())
            .await
            .unwrap()
            .json,
    )
    .unwrap();
    assert_eq!(
        retry["full_snapshot"], true,
        "a manual retry must reach the relay"
    );
}

#[tokio::test]
async fn mobile_reports_quota_and_manual_sync_resumes_uploads() {
    let fixture = Fixture::new(None).await;
    let engine = &fixture.engine;
    engine
        .host
        .with(|c| {
            c.vault_create(
                "note",
                &dmsg_client::vault::Changes {
                    title: Some("keep this draft".into()),
                    ..Default::default()
                },
            )
        })
        .await
        .unwrap();
    engine.sync(true, OperationToken::new()).await.unwrap();
    let status = engine.session_state().await.unwrap();
    assert_eq!(status.pending_sync, 1);
    assert_eq!(status.sync_error.as_deref(), Some("QUOTA_EXCEEDED"));
    assert_eq!(fixture.uploads.load(Ordering::SeqCst), 1);
    // Start a later scheduled pass, outside the millisecond coalescing window.
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    let automatic: Value =
        serde_json::from_str(&engine.sync(true, OperationToken::new()).await.unwrap().json)
            .unwrap();
    assert_eq!(
        automatic["delta_pages"], 1,
        "paused uploads must still receive changes"
    );
    assert_eq!(fixture.uploads.load(Ordering::SeqCst), 1);
    engine.sync(false, OperationToken::new()).await.unwrap();
    assert_eq!(fixture.uploads.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn manual_sync_still_honors_server_backpressure() {
    let fixture = Fixture::new(Some("SYNC_BUSY")).await;
    let engine = &fixture.engine;
    for automatic in [true, false] {
        assert!(
            matches!(engine.sync(automatic, OperationToken::new()).await,
            Err(MobileError::Failure { code }) if code == "SYNC_BUSY")
        );
    }
    let paused: Value =
        serde_json::from_str(&engine.sync(true, OperationToken::new()).await.unwrap().json)
            .unwrap();
    assert_eq!(paused["paused"], true);
}

#[tokio::test]
async fn repeated_missing_inbox_keys_use_deltas_without_uploading_or_restarting_exports() {
    let fixture = Fixture::new(None).await;
    let engine = &fixture.engine;
    engine.sync(true, OperationToken::new()).await.unwrap();
    engine
        .host
        .with(|c| {
            c.vault_create(
                "note",
                &dmsg_client::vault::Changes {
                    title: Some("unsent draft".into()),
                    ..Default::default()
                },
            )
        })
        .await
        .unwrap();
    for _ in 0..3 {
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        assert!(matches!(engine.inbox(true).await,
            Err(MobileError::Failure { code }) if code == "RECOVERY_INCOMPLETE"));
    }
    assert_eq!(fixture.exports.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.deltas.load(Ordering::SeqCst), 3);
    assert_eq!(fixture.uploads.load(Ordering::SeqCst), 0);
    assert_eq!(engine.session_state().await.unwrap().pending_sync, 1);
}

#[tokio::test]
async fn inbox_key_recovery_obeys_account_backpressure_and_preserves_failure_status() {
    let fixture = Fixture::new(Some("SYNC_BUSY")).await;
    let engine = &fixture.engine;
    engine
        .sync(true, OperationToken::new())
        .await
        .err()
        .unwrap();
    let failed = engine.content_sync_status().await.unwrap();
    assert!(!failed.initialized && failed.retry_required);
    assert!(failed.last_success_at_ms.is_none());
    assert!(failed.retry_at_ms.unwrap() > dmsg_client::now_ms());
    for _ in 0..3 {
        assert!(engine.inbox(true).await.is_err());
    }
    assert_eq!(fixture.exports.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.deltas.load(Ordering::SeqCst), 0);
    assert_eq!(
        engine
            .content_sync_status()
            .await
            .unwrap()
            .last_attempt_at_ms,
        failed.last_attempt_at_ms
    );
}

#[tokio::test]
async fn manual_request_joins_an_automatic_snapshot_and_status_is_readable_during_network_wait() {
    let fixture = Fixture::new(None).await;
    fixture.delay_page.store(200, Ordering::SeqCst);
    let engine = &fixture.engine;
    let automatic = engine.sync(true, OperationToken::new());
    let manual = async {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let status = engine.content_sync_status().await.unwrap();
                if status.running && status.stage == "export_page" {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        let status = engine.content_sync_status().await.unwrap();
        assert!(!status.initialized && status.last_success_at_ms.is_none());
        serde_json::from_str::<Value>(
            &engine
                .sync(false, OperationToken::new())
                .await
                .unwrap()
                .json,
        )
        .unwrap()
    };
    let (first, joined) = tokio::join!(automatic, manual);
    first.unwrap();
    assert_eq!(joined["coalesced"], true);
    assert_eq!(fixture.exports.load(Ordering::SeqCst), 1);
    let completed = engine.content_sync_status().await.unwrap();
    assert!(completed.initialized && !completed.running && completed.last_success_at_ms.is_some());
    *fixture.fail_delta.lock().unwrap() = Some("SYNC_BUSY".into());
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    engine
        .sync(false, OperationToken::new())
        .await
        .err()
        .unwrap();
    let failed = engine.content_sync_status().await.unwrap();
    assert_eq!(failed.last_success_at_ms, completed.last_success_at_ms);
    assert_eq!(
        engine.session_state().await.unwrap().last_sync_at_ms,
        completed.last_success_at_ms
    );
}

#[tokio::test]
async fn manual_sync_after_inbox_recovery_still_uploads_pending_changes() {
    let fixture = Fixture::new(None).await;
    fixture.delay_page.store(200, Ordering::SeqCst);
    let engine = &fixture.engine;
    engine
        .host
        .with(|c| {
            c.vault_create(
                "note",
                &dmsg_client::vault::Changes {
                    title: Some("owner asked to upload".into()),
                    ..Default::default()
                },
            )
        })
        .await
        .unwrap();
    let manual = async {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while engine.content_sync_status().await.unwrap().stage != "export_page" {
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        engine.sync(false, OperationToken::new()).await.unwrap()
    };
    let (inbox, sent) = tokio::join!(engine.inbox(true), manual);
    assert!(inbox.is_err());
    let report: Value = serde_json::from_str(&sent.json).unwrap();
    assert_eq!(report["upload_error"], "QUOTA_EXCEEDED");
    assert_eq!(fixture.exports.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.uploads.load(Ordering::SeqCst), 1);
}
