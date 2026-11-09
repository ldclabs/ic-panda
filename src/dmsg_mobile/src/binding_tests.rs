use super::*;
use dmsg_client::testing;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn fixture(origin: &str) -> (MobileEngine, testing::TempDir, String) {
    let dir = testing::TempDir::new();
    let mut client = testing::unlocked(&dir.0, false, None);
    testing::ic_to(&mut client, origin);
    let delegated = testing::login(&mut client, 42).await;
    testing::login(&mut client, 7).await;
    let engine = MobileEngine {
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
    };
    let mut session = LoginSession::ios(
        &[testing::account().home],
        "https://dmsg.net",
        dmsg_types::MINUTE,
        dmsg_client::now_ms(),
    )
    .unwrap();
    session.deadline = 0; // Already verified; binding recovery must not restart authentication.
    let bytes = vec![1, 2, 3];
    engine.login.lock().unwrap().pending = Some(PendingLogin {
        session: session.into(),
        binding_generation: Some(0),
        resetting: false,
        callback: Some(bytes.clone()),
        delegated: Some(delegated),
    });
    let callback = format!(
        "{}#{}",
        dmsg_protocol::login::IOS_CALLBACK,
        dmsg_client::codec::b64(&bytes)
    );
    (engine, dir, callback)
}

#[tokio::test]
async fn binding_read_failure_keeps_the_verified_identity_and_rejects_foreign_retries() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let count = Arc::new(AtomicU64::new(0));
    let reads = count.clone();
    let server = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 8192];
            let size = socket.read(&mut bytes).await.unwrap();
            if size == 0 {
                continue;
            }
            let request = String::from_utf8_lossy(&bytes[..size]);
            let path = request.lines().next().unwrap();
            assert!(!path.contains("/call "), "Retry must only read: {path}");
            if path.contains("/query ") {
                reads.fetch_add(1, Ordering::Relaxed);
            }
            socket
                .write_all(
                    b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
        }
    });
    let (engine, _dir, callback) = fixture(&origin).await;
    for _ in 0..2 {
        assert!(
            matches!(engine.complete_binding(callback.clone()).await, Err(MobileError::Failure { code }) if code == "UNAVAILABLE")
        );
        assert!(
            engine
                .login
                .lock()
                .unwrap()
                .pending
                .as_ref()
                .unwrap()
                .delegated
                .is_some()
        );
        assert_eq!(engine.session_state().await.unwrap().state, "unlocked");
    }
    assert_eq!(count.load(Ordering::Relaxed), 2);
    let foreign = format!("{}#BAUG", dmsg_protocol::login::IOS_CALLBACK);
    assert!(
        matches!(engine.complete_binding(foreign).await, Err(MobileError::Failure { code }) if code == "INTEGRITY_FAILED")
    );
    assert!(engine.login.lock().unwrap().pending.is_some());
    engine.lock_now();
    assert!(engine.login.lock().unwrap().pending.is_none());
    engine.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn cancelling_binding_stops_the_pending_network_step_without_restoring_it() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (engine, _dir, callback) =
        fixture(&format!("http://{}", listener.local_addr().unwrap())).await;
    let engine = Arc::new(engine);
    let run = engine.clone();
    let task = tokio::spawn(async move { run.complete_binding(callback).await });
    let (_socket, _) = listener.accept().await.unwrap();
    engine.cancel_login();
    assert!(
        matches!(tokio::time::timeout(std::time::Duration::from_secs(1), task).await.unwrap().unwrap(), Err(MobileError::Failure { code }) if code == "LOCKED")
    );
    assert!(engine.login.lock().unwrap().pending.is_none());
    engine.close().await.unwrap();
}

#[tokio::test]
async fn capacity_reset_preserves_the_current_account_and_rejects_a_stale_completion() {
    let (engine, _dir, _) = fixture("http://127.0.0.1:9").await;
    let before = engine.session_state().await.unwrap();
    let after = engine
        .finish_passkey_logout(Some(before.generation))
        .await
        .unwrap();
    assert_eq!(after.state, "unlocked");
    assert_eq!(after.account, before.account);
    assert_eq!(after.generation, before.generation);
    engine.suspend();
    engine.resume(engine.control_generation()).await.unwrap();
    assert!(
        matches!(engine.finish_passkey_logout(Some(before.generation)).await,
        Err(MobileError::Failure { code }) if code == "LOCKED")
    );
    assert_eq!(engine.session_state().await.unwrap().state, "unlocked");
    engine.close().await.unwrap();
}

#[tokio::test]
async fn ordinary_passkey_logout_still_ends_the_current_local_session() {
    let (engine, _dir, _) = fixture("http://127.0.0.1:9").await;
    let after = engine.finish_passkey_logout(None).await.unwrap();
    assert_ne!(after.state, "unlocked");
    engine.close().await.unwrap();
}
