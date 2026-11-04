use super::*;
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct ReadTurns {
    lanes: Mutex<BTreeMap<String, Arc<tokio::sync::Mutex<()>>>>,
    names: Mutex<NameRetries>,
}

#[derive(Default)]
struct NameRetries {
    session: Option<dmsg_client::display::Session>,
    after: BTreeMap<dmsg_types::AccountId, std::time::Instant>,
}

impl ReadTurns {
    pub(crate) fn lane(&self, resource: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut lanes = self.lanes.lock().expect("read turns");
        lanes.retain(|_, v| Arc::strong_count(v) > 1);
        lanes.entry(resource.into()).or_default().clone()
    }
}

#[uniffi::export]
impl MobileEngine {
    /// Fetch a participant's published image without retaining the client lock.
    pub async fn channel_avatar(&self, account: String, refresh: bool) -> Result<Option<Vec<u8>>> {
        let account = account
            .parse::<dmsg_types::AccountId>()
            .map_err(|_| dmsg_types::invalid("account"))?;
        let turn = self.read_turns.lane("participant-avatars");
        self.held(move |h| {
            Box::pin(async move {
                let session = h.with(|c| c.display_session()).await?;
                let scoped = dmsg_client::display::Scoped { inner: h, session };
                let h = &scoped;
                let url = h
                    .with(|c| {
                        Ok(c.public_names()?
                            .get(&account)
                            .and_then(|n| n.avatar_url.clone()))
                    })
                    .await?;
                let Some(url) = url else { return Ok(None) };
                let image = if refresh {
                    let _turn = turn.lock().await;
                    dmsg_client::display::participant_avatar(h, &url).await?
                } else {
                    h.with(|c| c.participant_avatar_cached(&url)).await?
                };
                h.with(|c| {
                    ensure(
                        c.public_names()?
                            .get(&account)
                            .and_then(|n| n.avatar_url.as_ref())
                            == Some(&url),
                        Error::VersionConflict,
                    )?;
                    Ok(image.map(|image| image.bytes))
                })
                .await
            })
        })
        .await
    }

    /// Certified participant display fields; local reads stay available offline.
    pub async fn channel_names(
        &self,
        accounts: Vec<String>,
        refresh: bool,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        ensure_valid(accounts.len() <= 100, "at most 100 accounts")?;
        let accounts = accounts
            .into_iter()
            .map(|s| {
                s.parse::<dmsg_types::AccountId>()
                    .map_err(|_| dmsg_types::invalid("account"))
            })
            .collect::<dmsg_types::Result<Vec<_>>>()?;
        let turn = self.read_turns.lane("channel-names");
        let reads = self.read_turns.clone();
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let session = h.with(|c| c.display_session()).await?;
                    let scoped = dmsg_client::display::Scoped { inner: h, session };
                    let h = &scoped;
                    if refresh {
                        let _turn = turn.lock().await;
                        let mut due = h
                            .with(|c| {
                                let known = c.public_names()?;
                                let now = dmsg_client::now_ms();
                                Ok(accounts
                                    .iter()
                                    .filter(|id| known.get(id).is_none_or(|n| n.stale(now)))
                                    .copied()
                                    .collect::<Vec<_>>())
                            })
                            .await?;
                        {
                            let mut retries = reads.names.lock().expect("name retries");
                            if retries.session != Some(session) {
                                *retries = NameRetries {
                                    session: Some(session),
                                    after: BTreeMap::new(),
                                };
                            }
                            let now = std::time::Instant::now();
                            retries.after.retain(|_, after| *after > now);
                            due.retain(|id| !retries.after.contains_key(id));
                            for id in &due {
                                retries.after.insert(
                                    *id,
                                    now + std::time::Duration::from_millis(
                                        dmsg_client::display::ABSENT_TTL,
                                    ),
                                );
                            }
                        }
                        if !due.is_empty() {
                            let found = dmsg_client::principal::names(h, &due).await?;
                            h.with(|c| {
                                let now = dmsg_client::now_ms();
                                let names = found
                                    .into_iter()
                                    .filter_map(|(id, result)| {
                                        result.ok().map(|mut n| {
                                            n.observed_at_ms = now;
                                            (id, n)
                                        })
                                    })
                                    .collect();
                                c.keep_public_names(names).map(|_| ())
                            })
                            .await?;
                        }
                    }
                    h.with(|c| {
                        let known = c.public_names()?;
                        Ok(accounts
                            .iter()
                            .filter_map(|id| known.get(id).map(|n| (id.to_string(), n.clone())))
                            .collect::<BTreeMap<_, _>>())
                    })
                    .await
                })
            }))
            .await
    }

    /// Pure local display read, including a missing observation.
    pub async fn cached_display(&self, kind: String, id: String) -> Result<ObjectSnapshot> {
        let kind = dmsg_client::snapshot::kind(&kind)?;
        self.held_object(move |h| {
            Box::pin(async move { h.with(|c| dmsg_client::snapshot::read(c, kind, &id)).await })
        })
        .await
    }

    /// Explicit refresh for one visible display; no entitlement update or command.
    pub async fn refresh_display(
        &self,
        kind: String,
        id: String,
        force: bool,
        operation: Arc<OperationToken>,
    ) -> Result<ObjectSnapshot> {
        let kind = dmsg_client::snapshot::kind(&kind)?;
        let turn = self
            .read_turns
            .lane(&format!("display:{}:{id}", kind.name()));
        let requested = dmsg_client::now_ms();
        operation
            .run(self.held_object(move |h| {
                Box::pin(async move {
                    let session = h.with(|c| c.display_session()).await?;
                    let scoped = dmsg_client::display::Scoped { inner: h, session };
                    let h = &scoped;
                    let _turn = turn.lock().await;
                    let before = h
                        .with(|c| dmsg_client::snapshot::read(c, kind, &id))
                        .await?;
                    if (force || before["stale"] == true)
                        && before["observed_at_ms"]
                            .as_u64()
                            .is_none_or(|at| at < requested)
                    {
                        dmsg_client::snapshot::refresh(h, kind, &id).await?;
                    }
                    h.with(|c| dmsg_client::snapshot::read(c, kind, &id)).await
                })
            }))
            .await
    }

    pub async fn display_cache_status(&self) -> Result<ObjectSnapshot> {
        self.held_object(|h| Box::pin(async { h.with(|c| c.cache_status()).await }))
            .await
    }

    pub async fn clear_display_cache(&self) -> Result<()> {
        self.held(|h| Box::pin(async { h.with(|c| c.cache_clear()).await }))
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dmsg_client::{cache::Kind, testing};
    use serde_json::{Value, json};
    use std::time::Duration;

    fn engine(client: Client, dir: &testing::TempDir) -> Arc<MobileEngine> {
        Arc::new(MobileEngine {
            config: client.config().clone(),
            host: ScopedHost::new(Host::start(client, runtime().handle()), 0),
            platform: Arc::new(crate::platform::tests::PasscodeFallback::default()),
            login: Arc::new(Mutex::new(LoginState::default())),
            staging: dir.path("staging"),
            approval_turn: Arc::default(),
            read_turns: Arc::default(),
            sync_tracker: Arc::default(),
            transfers: Arc::default(),
        })
    }

    #[tokio::test]
    async fn local_mobile_views_answer_beside_a_stalled_inbox_and_clear_preserves_content() {
        let dir = testing::TempDir::new();
        let mut client = testing::unlocked(&dir.0, false, None);
        let path = dir.path("report.txt");
        std::fs::write(&path, b"keep these bytes").unwrap();
        let file = client
            .files_import(&path, "root", false, &mut |_, _| {})
            .unwrap()
            .file;
        client
            .vault_create(
                "note",
                &dmsg_client::vault::Changes {
                    title: Some("kept locally".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        client
            .cache_save(Kind::Account, "", &json!({"known":true}), 1)
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        testing::relay_to(
            &mut client,
            &format!("http://{}", listener.local_addr().unwrap()),
        );
        testing::warm(&mut client);
        // Source bindings follow this fixture's relay.
        client
            .cache_save(Kind::Account, "", &json!({"known":true}), 1)
            .unwrap();
        let engine = engine(client, &dir);
        let reading = tokio::spawn({
            let engine = engine.clone();
            async move { engine.inbox(true).await }
        });
        let (_socket, _) = tokio::time::timeout(Duration::from_secs(3), listener.accept())
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(Duration::from_millis(500), async {
            let snapshot = engine
                .cached_display("account".into(), "".into())
                .await
                .unwrap();
            let value: Value = serde_json::from_str(&snapshot.json).unwrap();
            assert_eq!(value["data"]["known"], true);
            assert_eq!(
                engine
                    .vault_page("".into(), None, false, 0, 20)
                    .await
                    .unwrap()
                    .items
                    .len(),
                1
            );
            engine.files("root".into()).await.unwrap();
            let folder = engine
                .create_folder("root".into(), "Projects".into())
                .await
                .unwrap();
            let folder: Value = serde_json::from_str(&folder.json).unwrap();
            let destination = folder["id"].as_str().unwrap().to_string();
            engine
                .move_file(file.id.clone(), destination.clone())
                .await
                .unwrap();
            let root = engine.files("root".into()).await.unwrap();
            let root: Value = serde_json::from_str(&root.json).unwrap();
            assert!(root["files"].as_array().unwrap().is_empty());
            let moved = engine.files(destination.clone()).await.unwrap();
            let moved: Value = serde_json::from_str(&moved.json).unwrap();
            assert_eq!(moved["files"][0]["id"], file.id);
            assert_eq!(moved["files"][0]["folder"], destination);
            assert_eq!(moved["files"][0]["sha256"], file.sha256);
            assert_eq!(moved["files"][0]["current"], file.current);
            engine.clear_display_cache().await.unwrap();
            let snapshot = engine
                .cached_display("account".into(), "".into())
                .await
                .unwrap();
            let value: Value = serde_json::from_str(&snapshot.json).unwrap();
            assert_eq!(value["state"], "missing");
            assert_eq!(
                engine
                    .vault_page("".into(), None, false, 0, 20)
                    .await
                    .unwrap()
                    .items
                    .len(),
                1
            );
        })
        .await
        .expect("local APIs must not wait for the network");
        assert!(!reading.is_finished());
        engine.lock_now();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), reading)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        assert!(
            engine
                .cached_display("account".into(), "".into())
                .await
                .is_err()
        );
        engine.close().await.unwrap();
    }

    #[tokio::test]
    async fn inbox_recovers_missing_keys_with_a_read_only_content_sync() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let dir = testing::TempDir::new();
        let mut client = testing::unlocked(&dir.0, false, None);
        client
            .vault_create(
                "note",
                &dmsg_client::vault::Changes {
                    title: Some("must stay local".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        testing::relay_to(
            &mut client,
            &format!("http://{}", listener.local_addr().unwrap()),
        );
        testing::warm(&mut client);
        let account = testing::account().id.to_string();
        let server = tokio::spawn(async move {
            let inbox = format!("/v1/inboxes/{account}?after=0&limit=20");
            let base = format!("/v1/accounts/{account}");
            let replies = [
                (
                    "GET",
                    inbox.clone(),
                    json!({"ok":false,"error":{"code":"RECOVERY_INCOMPLETE"}}),
                ),
                (
                    "POST",
                    format!("{base}/exports"),
                    json!({"ok":true,"data":{
                    "account":account,"scope":"cloud_snapshot","through":0,"counts":[],
                    "content_sync":1,"export_id":"aa".repeat(32),"expires_at":dmsg_client::now_ms()+60_000}}),
                ),
                (
                    "GET",
                    format!("{base}/exports/{}?after=0&limit=100", "aa".repeat(32)),
                    json!({"ok":true,"data":{"entries":[],"next_cursor":0,"inventory_end":true}}),
                ),
                (
                    "POST",
                    format!("{base}/exports/{}/close", "aa".repeat(32)),
                    json!({"ok":true,"data":{"closed":true}}),
                ),
                (
                    "GET",
                    inbox,
                    json!({"ok":true,"data":{"entries":[],"next_cursor":0}}),
                ),
            ];
            for (method, path, body) in replies {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut chunk = [0; 8192];
                    let n = socket.read(&mut chunk).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&chunk[..n]);
                    if let Some(end) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                        let length = headers
                            .lines()
                            .find_map(|s| s.strip_prefix("content-length:"))
                            .map(|s| s.trim().parse::<usize>().unwrap())
                            .unwrap_or(0);
                        if bytes.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                let request = String::from_utf8_lossy(&bytes);
                assert_eq!(
                    request.lines().next().unwrap(),
                    format!("{method} {path} HTTP/1.1")
                );
                let body = body.to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
        });
        let engine = engine(client, &dir);
        let result = tokio::time::timeout(Duration::from_secs(10), engine.inbox(true))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.json, "[]");
        let status: Value =
            serde_json::from_str(&engine.cached_inbox_status().await.unwrap().json).unwrap();
        assert_eq!(status["complete"], true);
        assert_eq!(engine.session_state().await.unwrap().pending_sync, 1);
        assert_eq!(engine.inbox(false).await.unwrap().json, "[]");
        server.await.unwrap();
        engine.close().await.unwrap();
    }

    #[tokio::test]
    async fn notification_reconciliation_never_fetches_inbox_bodies() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let dir = testing::TempDir::new();
        let mut client = testing::unlocked(&dir.0, false, None);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        testing::relay_to(&mut client, &origin);
        testing::ic_to(&mut client, &origin);
        testing::warm(&mut client);
        let paths = Arc::new(Mutex::new(Vec::new()));
        let server = tokio::spawn({
            let paths = paths.clone();
            async move {
                while let Ok((mut socket, _)) = listener.accept().await {
                    let mut bytes = vec![0; 16_384];
                    if let Ok(n) = socket.read(&mut bytes).await {
                        paths.lock().unwrap().push(
                            String::from_utf8_lossy(&bytes[..n])
                                .lines()
                                .next()
                                .unwrap_or_default()
                                .to_string(),
                        );
                        let body =
                            r#"{"ok":false,"error":{"code":"NOT_FOUND","message":"fixture"}}"#;
                        let _ = socket.write_all(format!("HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await;
                    }
                }
            }
        });
        let engine = engine(client, &dir);
        let _ = tokio::time::timeout(Duration::from_secs(5), engine.notification_refresh())
            .await
            .expect("bounded notification pass");
        assert!(
            !paths
                .lock()
                .unwrap()
                .iter()
                .any(|p| p.contains("/v1/inboxes/"))
        );
        engine.close().await.unwrap();
        server.abort();
    }
}
