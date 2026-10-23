use super::*;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Relay {
    status: Value,
    result: Value,
    finalized: AtomicUsize,
}

impl Hold for Relay {
    async fn with<T: Send>(&self, _: impl FnOnce(&mut Client) -> Result<T> + Send) -> Result<T> {
        panic!("a committed upload must not read or upload local chunks")
    }

    async fn find(&self, _: &str) -> Result<Option<Value>> {
        Ok(Some(self.status.clone()))
    }

    async fn post<P: Serialize + Sync>(
        &self,
        path: &str,
        action: &str,
        payload: &P,
        _: &Hash,
    ) -> Result<Value> {
        assert!(path.ends_with("/uploads/finalize"));
        assert_eq!(action, "dmsg/upload/finalize/v1");
        assert_eq!(
            serde_json::to_value(payload).unwrap()["upload_id"],
            self.status["plan"]["upload_id"]
        );
        self.finalized.fetch_add(1, Ordering::SeqCst);
        Ok(self.result.clone())
    }
}

fn fixture(kind: &str, published: bool) -> (UploadPlan, Relay) {
    let plan = UploadPlan {
        management: None,
        upload_id: "01".repeat(32),
        object_id: "02".repeat(32),
        version_id: "03".repeat(32),
        root_generation: 0,
        epoch: 0,
        control_head: None,
        kind: kind.into(),
        chunks: vec![],
        manifest_digest: "04".repeat(32),
        manifest_size: 0,
        expires_at: 1,
    };
    let url = format!(
        "https://assets.dmsg.net/avatars/040g2081040g2081040g/{}",
        plan.upload_id
    );
    let relay = Relay {
        status: json!({"status":"committed", "plan":plan, "public_url":published.then_some(&url)}),
        result: json!({"upload_id":plan.upload_id, "object_id":plan.object_id, "version_id":plan.version_id, "digest":plan.manifest_digest, "public_url":url}),
        finalized: AtomicUsize::new(0),
    };
    (plan, relay)
}

#[tokio::test]
async fn committed_avatar_retries_publication_without_uploading_or_reserving_again() {
    let (plan, relay) = fixture("avatar", false);
    let result = upload(
        &relay,
        "/v1/accounts/040g2081040g2081040g",
        &plan,
        &[],
        0,
        Source::Store,
        |_, _| Ok(()),
    )
    .await
    .unwrap();
    assert_eq!(result.as_deref(), relay.result["public_url"].as_str());
    assert_eq!(relay.finalized.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn published_avatar_reuses_the_exact_server_url_and_private_uploads_need_no_publication() {
    for (kind, published) in [("avatar", true), ("file", false)] {
        let (plan, relay) = fixture(kind, published);
        let result = upload(
            &relay,
            "/v1/accounts/040g2081040g2081040g",
            &plan,
            &[],
            0,
            Source::Store,
            |_, _| Ok(()),
        )
        .await
        .unwrap();
        assert_eq!(result.as_deref(), relay.status["public_url"].as_str());
        assert_eq!(relay.finalized.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn avatar_finalize_requires_a_matching_object_and_a_public_url() {
    for field in ["public_url", "digest", "upload_id"] {
        let (plan, mut relay) = fixture("avatar", false);
        relay.result.as_object_mut().unwrap().remove(field);
        assert_eq!(
            finalize(&relay, "/v1/accounts/040g2081040g2081040g", &plan).await,
            Err(Error::IntegrityFailed)
        );
    }
}

#[test]
#[ignore = "exports the public storage-management interoperability vector"]
fn print_storage_management_vector() {
    let (mut plan, _) = fixture("share", false);
    plan.chunks = vec![ChunkRef {
        digest: "05".repeat(32),
        size: 4,
    }];
    plan.manifest_size = 4;
    let account = "040g2081040g2081040g";
    let plaintext = json!({"name":"fixture.txt","source_file":"06".repeat(32),"source_version":"07".repeat(32),"intent":{"kind":"link","key":"test-only"}});
    let ciphertext = crate::codec::b64(&crate::crypto::seal_with_nonce(
        &[9; 32],
        &serde_json::to_vec(&plaintext).unwrap(),
        &(
            "dmsg/storage-management/1",
            account,
            &plan.upload_id,
            &plan.object_id,
            &plan.version_id,
            1u64,
        ),
        &[1; 12],
    ));
    plan.management = Some(ManagementEnvelope {
        generation: 1,
        ciphertext,
    });
    let seed = [7; 32];
    let signer = crate::cloud::Signer {
        issuer: "https://dmsg.test/u/040g2081040g2081040g",
        device_id: Hash::new([8; 32]),
        security_epoch: 1,
        seed: &seed,
        expires_at: u64::MAX,
    };
    let message = json!({"channel_id":"0a".repeat(32),"epoch":1,"control_head":"0b".repeat(32),"acl_version":1,"message_id":"0c".repeat(32),"client_created_at":1700000000000u64,"attachment_upload":plan.upload_id,"ciphertext":"Y2lwaGVy"});
    let operations = [
        (
            "dmsg/upload/reserve/v1",
            serde_json::to_value(&plan).unwrap(),
        ),
        ("dmsg/channel/message/v1", message),
        (
            "dmsg/storage/retry/v1",
            json!({"upload_id":plan.upload_id,"generation":1}),
        ),
    ];
    let cases:Vec<_>=operations.into_iter().map(|(action,payload)|json!({"action":action,"payload":payload,"cose_sign1":crate::cloud::sign_command(&signer,action,&payload,&"0d".repeat(32),1700000045000)})).collect();
    let vector = json!({"test_only":true,"namespace":"https://dmsg.test/u/","public_key":hex::encode(ed25519_dalek::SigningKey::from_bytes(&seed).verifying_key().to_bytes()),"cases":cases});
    if let Ok(path) = std::env::var("DMSG_STORAGE_VECTOR_OUT") {
        std::fs::write(path, serde_json::to_vec_pretty(&vector).unwrap()).unwrap();
    } else {
        println!("{}", serde_json::to_string_pretty(&vector).unwrap());
    }
}

#[tokio::test]
async fn device_envelopes_upload_together_and_finalize_only_after_all_acks() {
    use crate::codec::hex_sha256;
    use crate::testing::{TempDir, relay_to, unlocked, warm};
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};

    struct Uploading {
        client: Mutex<Client>,
        plan: UploadPlan,
        acknowledged: Arc<AtomicUsize>,
    }
    impl Hold for Uploading {
        async fn with<T: Send>(
            &self,
            step: impl FnOnce(&mut Client) -> Result<T> + Send,
        ) -> Result<T> {
            step(&mut self.client.lock().unwrap())
        }

        async fn find(&self, _: &str) -> Result<Option<Value>> {
            Ok(Some(json!({"status": "staging", "plan": self.plan})))
        }

        async fn post<P: Serialize + Sync>(
            &self,
            path: &str,
            _: &str,
            _: &P,
            _: &Hash,
        ) -> Result<Value> {
            assert!(path.ends_with("/uploads/finalize"));
            assert_eq!(self.acknowledged.load(Ordering::SeqCst), 4);
            Ok(
                json!({"upload_id": self.plan.upload_id, "object_id": self.plan.object_id,
                "version_id": self.plan.version_id, "digest": self.plan.manifest_digest}),
            )
        }
    }
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    relay_to(
        &mut client,
        &format!("http://{}", listener.local_addr().unwrap()),
    );
    warm(&mut client);
    let (mut plan, _) = fixture("envelopes", false);
    plan.expires_at = crate::now_ms() + 600_000;
    for bytes in [b"one".as_slice(), b"two", b"three"] {
        let digest = hex_sha256(bytes);
        client.workspace.store.put_chunk(&digest, bytes).unwrap();
        plan.chunks.push(ChunkRef {
            digest,
            size: bytes.len() as u64,
        });
    }
    let acknowledged = Arc::new(AtomicUsize::new(0));
    let counted = acknowledged.clone();
    let server = tokio::spawn(async move {
        let mut sockets = vec![];
        let mut paths = std::collections::BTreeSet::new();
        // No request gets an answer until all three envelopes and the
        // manifest arrive. Sequential uploads would time out here.
        for _ in 0..4 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = vec![];
            loop {
                let mut buffer = [0; 4096];
                let n = socket.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let length = headers
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse::<usize>()
                    .unwrap();
                if bytes.len() < end + 4 + length {
                    continue;
                }
                assert!(headers.starts_with("put "));
                paths.insert(
                    headers
                        .lines()
                        .next()
                        .unwrap()
                        .split_whitespace()
                        .nth(1)
                        .unwrap()
                        .to_string(),
                );
                break;
            }
            sockets.push(socket);
        }
        assert_eq!(paths.len(), 4);
        assert!(paths.iter().any(|p| p.ends_with("/manifest")));
        for mut socket in sockets {
            counted.fetch_add(1, Ordering::SeqCst);
            let body = r#"{"ok":true,"data":{}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let h = Uploading {
        client: Mutex::new(client),
        plan: plan.clone(),
        acknowledged,
    };
    let mut progress = vec![];
    timeout(
        Duration::from_secs(5),
        upload(
            &h,
            "/v1/channels/test",
            &plan,
            b"manifest",
            0,
            Source::Store,
            |_, through| {
                progress.push(through);
                Ok(())
            },
        ),
    )
    .await
    .expect("envelopes must not wait for each other's ACK")
    .unwrap();
    server.await.unwrap();
    assert_eq!(progress, vec![3]);
}
