use super::*;
use crate::testing::{TempDir, relay_to, unlocked, warm};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn policy_fixture(c: &mut Client) -> Json {
    let (_, mut device) = crate::testing::device(1, None);
    device.input.device_id = c.workspace.meta.device_id;
    device.input.signing_pub = c.workspace.meta.signing_pub;
    c.security
        .as_mut()
        .unwrap()
        .devices
        .insert(device.input.device_id, device);
    let public = c.inbox_key(1).unwrap();
    let state = c.fresh().unwrap();
    let signer = c.signer(&state).unwrap();
    let descriptor = sign_command(
        &signer,
        "dmsg/inbox/key/v1",
        &KeyDescriptor {
            inbox_key_version: 1,
            inbox_hpke_pub: hex::encode(public),
            root_generation: 1,
        },
        &random_hex(),
        now_ms() + 40_000,
    );
    let payload = PolicyPayload {
        expected_version: 0,
        prev_hash: None,
        mode: "contacts".into(),
        block: vec![AccountId([3; 12]).to_string()],
        allow: vec![AccountId([4; 12]).to_string()],
        invitations: vec![ContactInvitation {
            id: "ab".repeat(32),
            account: AccountId([5; 12]).to_string(),
            expires_at: now_ms() + 60_000,
        }],
        capacity: 17,
        inbox_key_version: 1,
        inbox_hpke_pub: hex::encode(public),
        root_generation: 1,
        key_descriptor: CoseRef {
            cose_sign1: descriptor,
        },
        offer: None,
    };
    let cose = sign_command(&signer, POLICY, &payload, &random_hex(), now_ms() + 40_000);
    stored_policy(&cose, &state)
}

fn stored_policy(cose: &str, state: &Security) -> Json {
    let payload: PolicyPayload =
        crate::cloud::verify_command(cose, &state.snapshot.issuer, &state.devices, POLICY).unwrap();
    let mut policy = serde_json::to_value(&payload).unwrap();
    policy["version"] = json!(payload.expected_version + 1);
    policy["hash"] = json!(hex::encode(*sha256(&unb64(cose).unwrap())));
    policy["security_epoch"] = json!(state.snapshot.security_epoch);
    policy["signed"] = json!({"cose_sign1":cose});
    policy
}

#[test]
fn renewal_preserves_signed_rules_and_rejects_relay_edits() {
    let dir = TempDir::new();
    let mut c = unlocked(&dir.0, false, None);
    warm(&mut c);
    let old = policy_fixture(&mut c);
    let state = c.fresh().unwrap();
    let rules = input(&old, &state).unwrap();
    assert_eq!(rules.mode, "contacts");
    assert_eq!(json!(rules.block.unwrap()), old["block"]);
    assert_eq!(json!(rules.allow), old["allow"]);
    assert_eq!(json!(rules.invitations.unwrap()), old["invitations"]);
    assert_eq!(rules.capacity, Some(17));
    for (field, value) in [
        ("allow", json!([])),
        ("mode", json!("free")),
        ("capacity", json!(100)),
        ("hash", json!("ff".repeat(32))),
    ] {
        let mut tampered = old.clone();
        tampered[field] = value;
        assert_eq!(
            input(&tampered, &state).unwrap_err(),
            Error::IntegrityFailed
        );
    }
}

#[tokio::test]
async fn renewal_uploads_first_preserves_rules_and_reconciles_a_lost_reply() {
    for failure in [None, Some("EXECUTION_UNKNOWN"), Some("VERSION_CONFLICT")] {
        let dir = TempDir::new();
        let mut c = unlocked(&dir.0, false, None);
        warm(&mut c);
        let old = policy_fixture(&mut c);
        c.security.as_mut().unwrap().snapshot.security_epoch = 5;
        c.security
            .as_mut()
            .unwrap()
            .snapshot
            .content_root_generation = 3;
        c.root = Some(crate::root::RootKeys::single(3, [8; 32]));
        let state = c.fresh().unwrap();
        let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        relay_to(&mut c, &format!("http://{}", server.local_addr().unwrap()));
        let expected = old.clone();
        let serve = tokio::spawn(async move {
            let mut current = old;
            for step in 0..if failure == Some("EXECUTION_UNKNOWN") {
                3
            } else {
                2
            } {
                let (mut socket, _) = server.accept().await.unwrap();
                let mut request = vec![];
                let end = loop {
                    let mut chunk = [0; 16384];
                    let n = socket.read(&mut chunk).await.unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&chunk[..n]);
                    if let Some(end) = request.windows(4).position(|b| b == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&request[..end]).to_lowercase();
                        let length = headers
                            .lines()
                            .find_map(|s| s.strip_prefix("content-length:"))
                            .map(|s| s.trim().parse::<usize>().unwrap())
                            .unwrap_or(0);
                        if request.len() >= end + 4 + length {
                            break end + 4;
                        }
                    }
                };
                let response = if step == 1 {
                    assert!(request.starts_with(b"POST "));
                    let body: CoseRef = decode(&request[end..], MAX_RECORD).unwrap();
                    current = stored_policy(&body.cose_sign1, &state);
                    for field in ["mode", "allow", "block", "invitations", "capacity"] {
                        assert_eq!(current[field], expected[field], "{field}");
                    }
                    assert_eq!(current["expected_version"], expected["version"]);
                    assert_eq!(current["prev_hash"], expected["hash"]);
                    assert_eq!(current["security_epoch"], 5);
                    assert_eq!(current["root_generation"], 3);
                    assert_eq!(current["inbox_key_version"], 2);
                    assert_ne!(current["inbox_hpke_pub"], expected["inbox_hpke_pub"]);
                    if let Some(code) = failure {
                        json!({"ok":false,"error":{"code":code}})
                    } else {
                        json!({"ok":true,"data":{"version":2,"hash":current["hash"],"in_flight":0}})
                    }
                } else {
                    assert!(request.starts_with(b"GET "));
                    json!({"ok":true,"data":current})
                }
                .to_string();
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).as_bytes()).await.unwrap();
            }
        });
        let h = c.hold();
        let refresh = prepare(&h).await.unwrap().unwrap();
        // Security can change while the key uploads. Do not publish the
        // descriptor prepared under that earlier observation.
        h.with(|c| {
            c.security.as_mut().unwrap().snapshot.security_epoch = 6;
            Ok(())
        })
        .await
        .unwrap();
        finish(&h, refresh.clone()).await.unwrap();
        h.with(|c| {
            c.security.as_mut().unwrap().snapshot.security_epoch = 5;
            Ok(())
        })
        .await
        .unwrap();
        // A local or quota-blocked key must not become publicly active.
        finish(&h, refresh.clone()).await.unwrap();
        h.with(|c| {
            let (row, _) = c.current(&key_object(&c.account()?.id.to_string(), 2))?;
            c.workspace
                .store
                .set_object(&row.key, crate::store::SYNCED, false)
        })
        .await
        .unwrap();
        let result = finish(&h, refresh).await;
        if failure == Some("EXECUTION_UNKNOWN") {
            assert_eq!(result, Err(Error::ExecutionUnknown));
        } else {
            result.unwrap();
        }
        // A successful publish needs no read; an unknown result reads the
        // committed policy instead of creating or publishing key version 3.
        if failure == Some("VERSION_CONFLICT") {
            assert_eq!(
                h.with(|c| c.workspace.store.meta::<(u64, u64)>(CHECKED))
                    .await
                    .unwrap(),
                None
            );
        } else {
            assert!(prepare(&h).await.unwrap().is_none());
        }
        serve.await.unwrap();
    }
}

#[tokio::test]
async fn unconfigured_accounts_do_not_publish_or_fetch_contact_rules() {
    let dir = TempDir::new();
    let mut c = unlocked(&dir.0, false, None);
    warm(&mut c);
    assert!(prepare(&c.hold()).await.unwrap().is_none());
}
