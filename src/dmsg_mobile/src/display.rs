use super::*;
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct ReadTurns(Mutex<BTreeMap<String, Arc<tokio::sync::Mutex<()>>>>);

impl ReadTurns {
    pub(crate) fn lane(&self, resource: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut lanes = self.0.lock().expect("read turns");
        lanes.retain(|_, v| Arc::strong_count(v) > 1);
        lanes.entry(resource.into()).or_default().clone()
    }
}

#[uniffi::export]
impl MobileEngine {
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
            transfers: Arc::default(),
        })
    }

    #[tokio::test]
    async fn local_mobile_views_answer_beside_a_stalled_inbox_and_clear_preserves_content() {
        let dir = testing::TempDir::new();
        let mut client = testing::unlocked(&dir.0, false, None);
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
