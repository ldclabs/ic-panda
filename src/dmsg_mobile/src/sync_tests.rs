use super::*;
use dmsg_client::testing;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Fixture {
    engine: MobileEngine,
    uploads: Arc<AtomicU64>,
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
                let body = if path.ends_with("/exports") {
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
                    json!({"ok":true,"data":{"entries":[],"next_cursor":0,"inventory_end":true}})
                } else if path.contains("/content/changes?") {
                    json!({"ok":true,"data":{"content_sync":1,"account":testing::account().id.to_string(),
                        "entries":[],"through":0,"next_cursor":0,"end":true}})
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
                transfers: Arc::default(),
            },
            uploads,
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
