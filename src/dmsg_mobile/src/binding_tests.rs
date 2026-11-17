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
async fn system_export_policy_is_local_and_requires_the_unlocked_workspace() {
    let (engine, _dir, _) = fixture("http://127.0.0.1:9").await;
    let snapshot = engine.system_export_policy().await.unwrap();
    let policy: serde_json::Value = serde_json::from_str(&snapshot.json).unwrap();
    assert_eq!(policy["biometry_only"], false);
    assert_eq!(policy["biometry_available"], false);
    engine.lock_now();
    assert!(
        matches!(engine.system_export_policy().await, Err(MobileError::Failure { code }) if code == "LOCKED")
    );
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
    engine.shutdown().await.unwrap();
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
    engine.shutdown().await.unwrap();
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
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn reminder_acknowledgement_clears_badges_but_preserves_registration_health() {
    let (engine, _dir, _) = fixture("http://127.0.0.1:9").await;
    let selected = testing::account().id;
    let id = engine
        .host
        .run(move |c| {
            Box::pin(async move {
                c.with_notification_store(&selected, |s| {
                    s.set_meta("notification:registration_required", &true)?;
                    let mut item = dmsg_client::notifications::Item::new(
                        selected.to_string(),
                        dmsg_client::notifications::Target::Service("registration:1".into()),
                        1,
                        dmsg_client::now_ms(),
                    );
                    item.actionable = true;
                    let id = item.id.clone();
                    dmsg_client::notifications::upsert(s, item)?;
                    Ok(id)
                })
            })
        })
        .await
        .unwrap();
    let before = engine.session_state().await.unwrap();
    engine
        .notification_mark_read(selected.to_string(), id.clone(), "1".into())
        .await
        .unwrap();
    let status: serde_json::Value =
        serde_json::from_str(&engine.notification_status().await.unwrap().json).unwrap();
    assert_eq!(status["badge"], 0);
    let preferences: serde_json::Value = serde_json::from_str(
        &engine
            .notification_preferences(Some(selected.to_string()))
            .await
            .unwrap()
            .json,
    )
    .unwrap();
    assert_eq!(preferences["registration_required"], true);
    let updated = id.clone();
    engine
        .host
        .run(move |c| {
            Box::pin(async move {
                c.with_notification_store(&selected, |s| {
                    let mut item = s.notification_item(&updated)?.unwrap();
                    item.version = 2;
                    dmsg_client::notifications::upsert(s, item)
                })
            })
        })
        .await
        .unwrap();
    engine
        .notification_mark_read(selected.to_string(), id, "1".into())
        .await
        .unwrap();
    let status: serde_json::Value =
        serde_json::from_str(&engine.notification_status().await.unwrap().json).unwrap();
    assert_eq!(
        status["badge"], 1,
        "A stale row must not acknowledge the newer version"
    );
    engine.notification_mark_all_read().await.unwrap();
    let items: serde_json::Value = serde_json::from_str(
        &engine
            .notification_items(
                selected.to_string(),
                crate::notifications::ReminderScope::NeedsAction,
                0,
            )
            .await
            .unwrap()
            .json,
    )
    .unwrap();
    assert_eq!(items["items"][0]["open"], true);
    assert_eq!(items["items"][0]["read_version"], "2");
    assert_eq!(
        engine.session_state().await.unwrap().account,
        before.account
    );
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn ordinary_passkey_logout_still_ends_the_current_local_session() {
    let (engine, _dir, _) = fixture("http://127.0.0.1:9").await;
    let after = engine.finish_passkey_logout(None).await.unwrap();
    assert_ne!(after.state, "unlocked");
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn file_edits_compare_original_fields_and_version_deletion_never_targets_a_new_head() {
    let (engine, _dir, _) = fixture("http://127.0.0.1:9").await;
    let (id, first, second) = engine
        .host
        .run(|c| {
            Box::pin(async {
                let first = c.files_write("root", "Reviewed.txt", b"First version")?;
                let second = c.files_write("root", "Reviewed.txt", b"Second version")?;
                Ok((first.file.id, first.version, second.version))
            })
        })
        .await
        .unwrap();
    let before = FileMetadata {
        tags: vec![],
        note: String::new(),
        starred: false,
    };
    engine
        .update_file_metadata(
            id.clone(),
            before.clone(),
            FileMetadata {
                tags: vec!["review".into()],
                note: "Kept note".into(),
                starred: true,
            },
        )
        .await
        .unwrap();
    assert!(
        matches!(engine.update_file_metadata(id.clone(), before.clone(), before).await, Err(MobileError::Failure { code }) if code == "VERSION_CONFLICT")
    );
    engine
        .use_file_version(id.clone(), first.clone(), second.clone())
        .await
        .unwrap();
    assert!(
        matches!(engine.delete_file_version(id.clone(), second.clone(), second.clone()).await, Err(MobileError::Failure { code }) if code == "VERSION_CONFLICT")
    );
    assert!(
        matches!(engine.delete_file_version(id.clone(), first.clone(), first.clone()).await, Err(MobileError::Failure { code }) if code == "VERSION_CONFLICT")
    );
    let result = engine
        .delete_file_version(id.clone(), second, first)
        .await
        .unwrap();
    assert!(result.json.contains("Kept note"));
    assert!(engine.purge_file(id.clone()).await.is_err());
    assert!(engine.file_details(id.clone()).await.is_ok());
    engine.trash_file(id.clone()).await.unwrap();
    engine.purge_file(id.clone()).await.unwrap();
    assert!(engine.file_details(id).await.is_err());
    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn locking_after_a_letter_queue_commit_does_not_revive_it_or_drop_other_drafts() {
    let (engine, _dir, _) = fixture("http://127.0.0.1:9").await;
    let account = testing::account().id.to_string();
    let sent = "a1".repeat(32);
    let unfinished = "b2".repeat(32);
    engine
        .queue_free_letter(
            sent.clone(),
            account.clone(),
            "Original letter".into(),
            None,
        )
        .await
        .unwrap();
    engine
        .lock_with_drafts(
            account.clone(),
            vec![],
            vec![
                LetterDraftInput {
                    id: sent.clone(),
                    recipient: account.clone(),
                    text: "Late editor".into(),
                    invitation: None,
                },
                LetterDraftInput {
                    id: unfinished.clone(),
                    recipient: account,
                    text: "Last authored line".into(),
                    invitation: None,
                },
            ],
            None,
        )
        .await
        .unwrap();
    let generation = engine.host.inner.generation();
    engine
        .host
        .inner
        .with_at(generation, |c| {
            testing::unlock_fixture(c);
            let drafts = c.letter_drafts()?;
            assert_eq!(drafts.len(), 1);
            assert_eq!(drafts[0].id, unfinished);
            assert_eq!(drafts[0].text, "Last authored line");
            let history = c.letter_history()?;
            assert_eq!(history[0]["id"], sent);
            assert_eq!(history[0]["text"], "Original letter");
            Ok(())
        })
        .await
        .unwrap();
    engine.shutdown().await.unwrap();
}

#[test]
fn channel_links_require_exact_routes_and_keep_expiry_as_unsigned_integer() {
    let value = serde_json::json!({
        "format":"dmsg-channel-invitation/1", "channel":"ab".repeat(32), "genesis":"bc".repeat(32),
        "head":"cd".repeat(32), "invitation":"de".repeat(32), "account":"040g2081040g2081040g",
        "name":"Review this channel", "expires_at":u64::MAX
    });
    let encoded = dmsg_client::codec::b64(&serde_json::to_vec(&value).unwrap());
    let link = format!("dmsg://channel/join#{encoded}");
    assert_eq!(
        super::parse_channel_invitation(&link).unwrap().expires_at,
        u64::MAX
    );
    for link in [
        format!("dmsg://channel/join?approve=true#{encoded}"),
        format!("dmsg://channel.evil/join#{encoded}"),
        format!("dmsg://channel/join#{encoded}="),
        format!("dmsg://channel/join/{encoded}"),
    ] {
        assert!(super::parse_channel_invitation(&link).is_err());
    }
    let mut wrong = value.clone();
    wrong["account"] = "invalid-account".into();
    assert!(super::parse_channel_invitation(&wrong.to_string()).is_err());
}
