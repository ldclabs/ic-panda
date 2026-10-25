use super::*;
use dmsg_client::{
    cache::Kind,
    display::{Avatar, Profile},
    testing,
};

fn fixture(dir: &testing::TempDir) -> (Arc<Daemon>, Arc<testing::MemoryStorage>) {
    let secure = Arc::new(testing::MemoryStorage::default());
    let mut client = testing::unlocked(&dir.path("workspace"), false, Some(secure.clone()));
    client.enable_fast_unlock().unwrap();
    client
        .keep_profile_display(Profile {
            name: Some("Cached Person".into()),
            avatar_url: Some("https://example.test/avatar".into()),
            updated_at: Some(9),
            observed_at_ms: now_ms(),
        })
        .unwrap();
    let png = base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a2xkAAAAASUVORK5CYII=").unwrap();
    client
        .keep_profile_avatar(Avatar {
            url: "https://example.test/avatar".into(),
            mime: "image/png".into(),
            bytes: png,
            fetched_at_ms: now_ms(),
        })
        .unwrap();
    client.cache_save(Kind::Account,"",&json!({"logins":[],"policy":{"frozen":false,"allowed_purposes":[],"daily_executions":5},"recovery_delay_ms":259200000,"pending_recovery":null,"recovered_device":null}),now_ms()).unwrap();
    let home = Home {
        dir: dir.path("home"),
    };
    (
        Arc::new(Daemon::new(
            "fixture-owner".into(),
            0,
            home.settings().unwrap(),
            client,
            dmsg_agent::Runtime::new(&dmsg_agent::Options::default()).unwrap(),
            crate::mcp_events::McpEvents::new(&[]).unwrap(),
        )),
        secure,
    )
}

#[tokio::test]
async fn local_display_is_offline_and_locked_reads_are_refused() {
    let dir = testing::TempDir::new();
    let (d, _) = fixture(&dir);
    let activity = d.activity.load(Ordering::Relaxed);
    let results = futures_util::future::join_all((0..20).map(|_| d.profile_display())).await;
    for result in results {
        let result = result.unwrap();
        assert_eq!(result["name"], "Cached Person");
        assert!(
            result["avatar"]["url"]
                .as_str()
                .unwrap()
                .starts_with("data:image/png;base64,")
        );
        assert_eq!(result["refreshing"], false);
    }
    assert_eq!(d.activity.load(Ordering::Relaxed), activity);
    assert_eq!(
        d.snapshot("account/security", &json!({"mode":"local"}))
            .await
            .unwrap()["data"]["policy"]["daily_executions"],
        5
    );
    d.client.lock().await.lock();
    assert_eq!(d.profile_display().await.unwrap_err(), Error::Locked);
    assert_eq!(
        d.display_names(&[testing::account().id.to_string()])
            .await
            .unwrap_err(),
        Error::Locked
    );
    assert_eq!(
        d.snapshot("account/security", &json!({"mode":"local"}))
            .await
            .unwrap_err(),
        Error::Locked
    );
}

#[tokio::test]
async fn queued_refresh_cannot_commit_after_same_account_reunlock_or_clear() {
    let dir = testing::TempDir::new();
    let (d, _) = fixture(&dir);
    let shared = d.hold();
    let old = Unlocked::new(&shared).await.unwrap();
    let session = old.with(|c| c.display_session()).await.unwrap();
    d.client.lock().await.lock();
    d.under_lock(async |c| c.unlock_fast().await).await.unwrap();
    assert!(
        old.with(|c| c.keep_profile_display(Profile::from_document(None, now_ms())))
            .await
            .is_err()
    );
    assert!(
        d.hold()
            .with(|c| c.check_display_session(session))
            .await
            .is_err()
    );
    let current = d.hold().with(|c| c.display_session()).await.unwrap();
    d.call_daemon("cache/clear", &Value::Null).await.unwrap();
    assert!(
        d.hold()
            .with(|c| c.check_display_session(current))
            .await
            .is_err()
    );
    assert_eq!(d.profile_display().await.unwrap()["state"], "missing");
}

#[tokio::test]
async fn background_profile_network_does_not_block_local_reads() {
    let dir = testing::TempDir::new();
    let (d, _) = fixture(&dir);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    {
        let mut c = d.client.lock().await;
        testing::ic_to(
            &mut c,
            &format!("http://{}", listener.local_addr().unwrap()),
        );
        let mut p = c.profile_display().unwrap().unwrap();
        p.observed_at_ms = 0;
        c.keep_profile_display(p).unwrap();
    }
    let worker = tokio::spawn(crate::display::run(d.clone()));
    assert_eq!(d.profile_display().await.unwrap()["name"], "Cached Person");
    let (_connection, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
        .await
        .unwrap()
        .unwrap();
    for _ in 0..20 {
        let value = tokio::time::timeout(Duration::from_millis(100), d.profile_display())
            .await
            .expect("local read must not await upstream")
            .unwrap();
        assert_eq!(value["name"], "Cached Person");
    }
    // One resolver may make multiple IC transport requests; logical refreshes
    // are single-flight and repeated views have not queued another one.
    assert_eq!(d.display.profile_work(), (false, true));
    worker.abort();
}

#[tokio::test]
async fn display_modes_cannot_turn_an_entitlement_update_or_command_into_a_read() {
    let dir = testing::TempDir::new();
    let (d, _) = fixture(&dir);
    assert!(
        d.snapshot("plan/status", &json!({"mode":"swr","refresh":true}))
            .await
            .is_err()
    );
    assert!(
        d.snapshot("wallet/execute", &json!({"mode":"local"}))
            .await
            .is_err()
    );
    assert_eq!(route("cache/status"), Some(Module::Daemon));
    assert!(!crate::agents::AGENT_METHODS.contains(&"profile/display"));
    assert!(!crate::agents::AGENT_METHODS.contains(&"cache/clear"));
}

#[tokio::test]
async fn a_lock_inside_background_work_notifies_the_owner_even_on_error() {
    let dir = testing::TempDir::new();
    let (d, _) = fixture(&dir);
    let mut events = d.events.subscribe();
    let result: Result<()> = d
        .hold()
        .with(|c| {
            c.lock();
            Err(Error::DeviceNotApproved)
        })
        .await;
    assert!(result.is_err());
    let event = events.try_recv().unwrap();
    assert_eq!(event["type"], "session/changed");
    assert_eq!(event["data"]["state"], "locked");
    assert!(d.profile_display().await.is_err());
}

#[tokio::test]
async fn a_failed_unlock_after_locking_still_clears_owner_views() {
    let dir = testing::TempDir::new();
    let (d, _) = fixture(&dir);
    let mut events = d.events.subscribe();
    let result: Result<()> = d
        .under_lock(async |c| {
            c.lock();
            Err(Error::Forbidden)
        })
        .await;
    assert_eq!(result.unwrap_err(), Error::Forbidden);
    assert_eq!(events.try_recv().unwrap()["type"], "session/changed");
    assert!(d.profile_display().await.is_err());
}

#[tokio::test]
async fn a_native_policy_read_is_fenced_by_clear_or_its_own_invalidation_only() {
    for case in ["clear", "policy", "wallet"] {
        let dir = testing::TempDir::new();
        let (d, _) = fixture(&dir);
        let entered = Arc::new(Notify::new());
        let finish = Arc::new(Notify::new());
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let route = Router::new().fallback(axum::routing::get({
            let entered = entered.clone();
            let finish = finish.clone();
            move || {
                let entered = entered.clone();
                let finish = finish.clone();
                async move {
                    entered.notify_one();
                    finish.notified().await;
                    Json(json!({"ok":true,"data":{"mode":"paid","version":1}}))
                }
            }
        }));
        let server = tokio::spawn(axum::serve(listener, route).into_future());
        {
            let mut c = d.client.lock().await;
            testing::relay_to(&mut c, &origin);
            testing::warm(&mut c);
        }
        let read = tokio::spawn({
            let d = d.clone();
            async move { dmsg_client::inbox::policy(&d.hold()).await }
        });
        tokio::time::timeout(Duration::from_secs(3), entered.notified())
            .await
            .unwrap();
        {
            let mut c = d.client.lock().await;
            match case {
                "clear" => c.cache_clear().unwrap(),
                "policy" => {
                    c.cache_invalidate(&[Kind::Inbox]).unwrap();
                    c.cache_save(
                        Kind::Inbox,
                        "",
                        &json!({"mode":"free","version":2}),
                        now_ms(),
                    )
                    .unwrap();
                }
                // Another kind's invalidation neither fails nor fences this read.
                _ => c.cache_invalidate(&[Kind::Wallet, Kind::History]).unwrap(),
            }
        }
        finish.notify_one();
        assert_eq!(read.await.unwrap().is_err(), case != "wallet");
        let saved = d.client.lock().await.cache_read(Kind::Inbox, "").unwrap();
        match case {
            "clear" => assert!(saved.is_none()),
            "policy" => assert_eq!(saved.unwrap().data["version"], 2),
            _ => assert_eq!(saved.unwrap().data["version"], 1),
        }
        server.abort();
    }
}

#[tokio::test]
async fn an_avatar_reply_from_before_a_b_a_publications_is_discarded() {
    let dir = testing::TempDir::new();
    let (d, _) = fixture(&dir);
    let entered = Arc::new(Notify::new());
    let finish = Arc::new(Notify::new());
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let endpoint = Router::new().route(
        "/a",
        axum::routing::get({
            let entered = entered.clone();
            let finish = finish.clone();
            let calls = calls.clone();
            move || {
                let entered = entered.clone();
                let finish = finish.clone();
                let calls = calls.clone();
                async move {
                    if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                        entered.notify_one();
                        finish.notified().await;
                        b"RIFF0000WEBPold".to_vec()
                    } else {
                        b"RIFF0000WEBPnew".to_vec()
                    }
                }
            }
        }),
    );
    let server = tokio::spawn(axum::serve(listener, endpoint).into_future());
    {
        let mut c = d.client.lock().await;
        testing::relay_to(&mut c, &origin);
        c.keep_profile_display(Profile {
            name: Some("new".into()),
            avatar_url: Some(format!("{origin}/a")),
            updated_at: Some(10),
            observed_at_ms: now_ms(),
        })
        .unwrap();
    }
    let worker = tokio::spawn(crate::display::run(d.clone()));
    d.profile_display().await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), entered.notified())
        .await
        .unwrap();
    {
        let c = d.client.lock().await;
        for (version, path) in [(11, "b"), (12, "a")] {
            c.keep_profile_display(Profile {
                name: Some("new".into()),
                avatar_url: Some(format!("{origin}/{path}")),
                updated_at: Some(version),
                observed_at_ms: now_ms(),
            })
            .unwrap();
        }
    }
    finish.notify_one();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let avatar = d
                .client
                .lock()
                .await
                .profile_avatar(&format!("{origin}/a"))
                .unwrap();
            if let Some(avatar) = avatar {
                assert_eq!(avatar.bytes, b"RIFF0000WEBPnew");
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    worker.abort();
    server.abort();
}

#[tokio::test]
async fn a_new_publication_downloads_its_avatar_after_an_older_download_finishes() {
    let dir = testing::TempDir::new();
    let (d, _) = fixture(&dir);
    let entered = Arc::new(Notify::new());
    let finish = Arc::new(Notify::new());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = Router::new()
        .route(
            "/old",
            axum::routing::get({
                let entered = entered.clone();
                let finish = finish.clone();
                move || {
                    let entered = entered.clone();
                    let finish = finish.clone();
                    async move {
                        entered.notify_one();
                        finish.notified().await;
                        b"RIFF0000WEBPold".to_vec()
                    }
                }
            }),
        )
        .route(
            "/new",
            axum::routing::get(|| async { b"RIFF0000WEBPnew".to_vec() }),
        );
    let server = tokio::spawn(axum::serve(listener, server).into_future());
    {
        let mut c = d.client.lock().await;
        testing::relay_to(&mut c, &origin);
        let mut p = c.profile_display().unwrap().unwrap();
        p.updated_at = Some(10);
        p.avatar_url = Some(format!("{origin}/old"));
        c.keep_profile_display(p).unwrap();
    }
    let worker = tokio::spawn(crate::display::run(d.clone()));
    assert!(d.profile_display().await.unwrap()["avatar"].is_null());
    tokio::time::timeout(Duration::from_secs(3), entered.notified())
        .await
        .unwrap();
    {
        let c = d.client.lock().await;
        let mut p = c.profile_display().unwrap().unwrap();
        p.updated_at = Some(11);
        p.avatar_url = Some(format!("{origin}/new"));
        c.keep_profile_display(p).unwrap();
    }
    assert!(d.profile_display().await.unwrap()["avatar"].is_null());
    finish.notify_one();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if d.client
                .lock()
                .await
                .profile_avatar(&format!("{origin}/new"))
                .unwrap()
                .is_some()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("new URL must not inherit the old URL's 24-hour backoff");
    assert_eq!(
        d.client
            .lock()
            .await
            .profile_avatar(&format!("{origin}/new"))
            .unwrap()
            .unwrap()
            .bytes,
        b"RIFF0000WEBPnew"
    );
    worker.abort();
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a built Electron desktop and its Playwright dependency"]
async fn real_electron_cached_profile_and_reconnection() {
    let dir = testing::TempDir::new();
    let (d, secure) = fixture(&dir);
    // Reopen the SQLite workspace with no content keys, as on daemon restart.
    {
        let mut client = d.client.lock().await;
        *client = Client::open(
            client.config().clone(),
            &dir.path("workspace"),
            Some(secure),
        )
        .unwrap();
        assert_eq!(client.session_state().state, "locked");
    }
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(axum::serve(listener, app(d.clone())).into_future());
    let profiles = tokio::spawn(crate::display::run(d.clone()));
    let snapshots = tokio::spawn(crate::snapshots::run(d.clone()));
    let desktop = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../dmsg_desktop");
    let output = tokio::process::Command::new("node")
        .arg("scripts/cache-real.mjs")
        .arg(&origin)
        .arg("fixture-owner")
        .current_dir(desktop)
        .output()
        .await
        .unwrap();
    server.abort();
    profiles.abort();
    snapshots.abort();
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn pending_connect_checks_measure_the_existing_serialized_wait() {
    let dir = testing::TempDir::new();
    let (d, _) = fixture(&dir);
    let relay = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .unwrap();
    {
        let mut client = d.client.lock().await;
        testing::relay_to(
            &mut client,
            &format!("http://{}", relay.local_addr().unwrap()),
        );
        testing::warm(&mut client);
        testing::pending_connect_entry(&client);
    }
    let checking = tokio::spawn({
        let d = d.clone();
        async move { d.under_lock(async |c| c.connect_entries(0).await).await }
    });
    let (_socket, _) = tokio::time::timeout(Duration::from_secs(3), relay.accept())
        .await
        .unwrap()
        .unwrap();
    let start = std::time::Instant::now();
    let read = tokio::spawn({
        let d = d.clone();
        async move { d.profile_display().await }
    });
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert!(!read.is_finished(), "this legacy path is still serialized");
    checking.abort();
    assert_eq!(read.await.unwrap().unwrap()["name"], "Cached Person");
    println!(
        "existing connect-entry network wait: local profile read waited {} ms (network wait cancelled after 120 ms)",
        start.elapsed().as_millis()
    );
}
