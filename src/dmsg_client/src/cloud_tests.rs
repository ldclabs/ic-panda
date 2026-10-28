use super::*;
use crate::vectors::{bytes, load, text};
use dmsg_types::Hash;
use std::collections::BTreeMap;

#[test]
fn notifications_publication_sidecar_preserves_command_and_binds_every_send_path() {
    let seed = [7u8; 32];
    let signer = Signer {
        issuer: "https://dmsg.test/u/040g2081040g2081040g",
        device_id: Hash::new([7; 32]),
        security_epoch: 1,
        seed: &seed,
        expires_at: u64::MAX,
    };
    let cloud = Cloud::new("https://api.dmsg.test", false).unwrap();
    let path = format!("/v1/channels/{}/messages", "ab".repeat(32));
    let request_id = Hash::new([3; 32]);
    let payload = serde_json::json!({"ciphertext":"original-ciphertext"});
    let action = "dmsg/channel/message/v1";
    let notice = PublishNotification {
        version: 1,
        kind: PublishKind::Message,
    };
    let original = cloud
        .prepare_post(&signer, &path, action, &payload, &request_id, None, 1_000)
        .unwrap();
    let new = cloud
        .prepare_post_notification(
            &signer,
            &path,
            action,
            &payload,
            &request_id,
            None,
            Some(notice),
            1_000,
        )
        .unwrap();
    let old_body: Json = cbor2::from_slice(original.body.as_ref().unwrap()).unwrap();
    let new_body: Json = cbor2::from_slice(new.body.as_ref().unwrap()).unwrap();
    assert_eq!(old_body["cose_sign1"], new_body["cose_sign1"]);
    assert_eq!(old_body.as_object().unwrap().len(), 1);
    assert_eq!(
        new_body["notification"],
        serde_json::json!({"version":1,"kind":"message"})
    );
    let cose = old_body["cose_sign1"].as_str().unwrap();
    let repost = cloud
        .prepare_signed_notification(
            &signer,
            &path,
            cose,
            &hex::encode(*request_id),
            46_000,
            Some(notice),
            1_000,
        )
        .unwrap();
    let presigned = cloud
        .presign_notification(
            &signer,
            &path,
            action,
            &payload,
            &request_id,
            "/v1/operations/id",
            Some(notice),
            1_000,
        )
        .unwrap();
    assert_eq!(repost.body, new.body);
    assert_eq!(presigned.body.as_ref(), new.body.as_ref().unwrap());
    assert_eq!(repost.pop, new.pop);
    assert_eq!(presigned.post, new.pop);
    let bare = command_body(&path, cose, None, None).unwrap();
    assert_eq!(bare, *original.body.as_ref().unwrap());
    assert_eq!(bare, canonical(&BTreeMap::from([("cose_sign1", cose)])));

    // The signature covers the body digest; changing only the sidecar cannot
    // satisfy that digest even though the original command signature is intact.
    let proof_bytes = unb64(&new.pop).unwrap();
    let Value::Array(parts) = decode_value(&proof_bytes[1..], 262_144).unwrap() else {
        panic!("COSE");
    };
    let protected = parts[0].as_bytes().unwrap();
    let proof_payload = parts[2].as_bytes().unwrap();
    let signature = parts[3].as_bytes().unwrap();
    verify(
        &Hash::new(crate::crypto::signing_public(&seed)),
        &sig_structure(protected, proof_payload),
        signature,
    )
    .unwrap();
    let proof: Json = cbor2::from_slice(proof_payload).unwrap();
    assert_eq!(
        proof["body_digest"],
        hex::encode(*sha256(new.body.as_ref().unwrap()))
    );
    let changed = command_body(
        &path,
        cose,
        None,
        Some(PublishNotification {
            kind: PublishKind::Update,
            ..notice
        }),
    )
    .unwrap();
    assert_ne!(proof["body_digest"], hex::encode(*sha256(&changed)));
    assert!(command_body("/v1/inbox/orders", cose, None, Some(notice)).is_err());
    assert!(
        command_body(
            &path,
            cose,
            None,
            Some(PublishNotification {
                version: 2,
                ..notice
            })
        )
        .is_err()
    );
}

#[test]
fn reproduces_commands_and_http_proofs() {
    // Preserve the historical fixture's exact encoding; the retired profile
    // command is no longer accepted by the relay's business protocol.
    let v = load("cloud-v1.json");
    let c = &v["context"];
    // Seed 7 is the fixture's test-only device key.
    let seed = [7u8; 32];
    let signer = Signer {
        issuer: text(&c["issuer"]),
        device_id: Hash::new(bytes(&c["deviceId"]).try_into().unwrap()),
        security_epoch: c["securityEpoch"].as_u64().unwrap(),
        seed: &seed,
        expires_at: u64::MAX,
    };
    let request_id = text(&c["requestId"]);
    let deadline = c["deadline"].as_u64().unwrap();
    let profile: BTreeMap<String, Json> = serde_json::from_value(v["profile"].clone()).unwrap();
    let command = sign_command(&signer, "dmsg/profile/v1", &profile, request_id, deadline);
    assert_eq!(command, text(&v["command"]["cose_sign1"]));

    let body = canonical(&BTreeMap::from([("cose_sign1", command.as_str())]));
    assert_eq!(body, bytes(&v["body_hex"]));
    assert_eq!(hex::encode(*sha256(&body)), text(&v["body_digest"]));
    let url = Url::parse(text(&v["url"])).unwrap();
    let pop = sign_http(&signer, &url, "POST", &body, request_id, deadline).unwrap();
    assert_eq!(pop, text(&v["http_pop"]));
}

#[test]
fn deadlines_stay_within_the_evidence() {
    let seed = [7u8; 32];
    let signer = Signer {
        issuer: "https://dmsg.test/u/040g2081040g2081040g",
        device_id: Hash::new([7; 32]),
        security_epoch: 1,
        seed: &seed,
        expires_at: 1_000_030_000,
    };
    assert_eq!(signer.deadline(1_000_000_000).unwrap(), 1_000_030_000);
    assert_eq!(signer.deadline(1_000_030_000), Err(Error::PolicyStale));
    let fresh = Signer {
        expires_at: u64::MAX,
        ..signer
    };
    assert_eq!(fresh.deadline(5).unwrap(), 45_005);
}

#[test]
fn targets_sort_queries_and_refuse_ambiguity() {
    let target = |u: &str| canonical_target(&Url::parse(u).unwrap());
    assert_eq!(
        target("https://a.test/v1/x?limit=100&after=0").unwrap(),
        "/v1/x?after=0&limit=100"
    );
    assert_eq!(target("https://a.test/v1/x").unwrap(), "/v1/x");
    assert!(target("https://a.test/v1/x?a=1&a=2").is_err());
    assert!(target("https://a.test/v1/a%2Fb").is_err());
    assert!(target("https://a.test/v1/x#f").is_err());
}

#[test]
fn relay_origins_are_https_or_local_loopback() {
    assert!(Cloud::new("https://api.dmsg.test", false).is_ok());
    assert!(Cloud::new("http://127.0.0.1:8787", true).is_ok());
    assert!(Cloud::new("http://127.0.0.1:8787", false).is_err());
    assert!(Cloud::new("http://api.dmsg.test", true).is_err());
    assert!(Cloud::new("https://api.dmsg.test/base", false).is_err());
}

#[tokio::test]
async fn an_edge_error_page_is_a_lost_reply() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            // Read the whole request, then answer with a gateway's HTML page.
            let mut request = Vec::new();
            let mut buffer = [0u8; 4096];
            loop {
                let n = socket.read(&mut buffer).await.unwrap_or(0);
                request.extend_from_slice(&buffer[..n]);
                let text = String::from_utf8_lossy(&request).to_ascii_lowercase();
                let Some(end) = text.find("\r\n\r\n") else {
                    if n == 0 {
                        break;
                    }
                    continue;
                };
                let length = text
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .and_then(|v| v.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if n == 0 || request.len() >= end + 4 + length {
                    break;
                }
            }
            let page = "<html><body>502 Bad Gateway</body></html>";
            let reply = format!(
                "HTTP/1.1 502 Bad Gateway\r\ncontent-type: text/html\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{page}",
                page.len()
            );
            let _ = socket.write_all(reply.as_bytes()).await;
        }
    });
    let cloud = Cloud::new(&origin, true).unwrap();
    assert!(matches!(
        cloud.public("/public/accounts/x/avatar/y").await,
        Err(Error::Unavailable(_))
    ));
    let evidence = Evidence {
        schema: 1,
        canister: String::new(),
        certificate: String::new(),
        entries: vec![],
    };
    assert_eq!(
        cloud.publish_security(&evidence).await,
        Err(Error::ExecutionUnknown)
    );
}

#[tokio::test]
async fn direct_downloads_are_bounded_verified_and_send_no_device_credentials() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let server = tokio::spawn(async move {
        for (status, body) in [(200, "abc"), (200, "bad"), (404, ""), (403, "")] {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 8192];
            let n = socket.read(&mut buffer).await.unwrap();
            let request = String::from_utf8_lossy(&buffer[..n]).to_lowercase();
            assert!(request.starts_with("get /__r2/"));
            assert!(
                !request.contains("x-dmsg-pop")
                    && !request.contains("cookie:")
                    && !request.contains("authorization:")
            );
            let response = format!(
                "HTTP/1.1 {status} Test\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        }
    });
    let cloud = Cloud::new(&origin, true).unwrap();
    let slot = crate::downloads::Slot {
        url: format!("{origin}/__r2/bucket/key?X-Amz-Signature=secret"),
        digest: hex::encode(*sha256(b"abc")),
        size: 3,
    };
    assert_eq!(
        cloud.download_slot(&slot, &slot.digest).await.unwrap(),
        b"abc"
    );
    assert_eq!(
        cloud.download_slot(&slot, &slot.digest).await,
        Err(Error::IntegrityFailed)
    );
    assert_eq!(
        cloud.download_slot(&slot, &slot.digest).await,
        Err(Error::NotFound)
    );
    assert_eq!(
        cloud.download_slot(&slot, &slot.digest).await,
        Err(Error::Expired)
    );
    server.await.unwrap();
    for url in [
        "http://169.254.169.254/key",
        "https://evil.r2.cloudflarestorage.com/key",
        "https://user@11111111111111111111111111111111.r2.cloudflarestorage.com/key",
    ] {
        let bad = crate::downloads::Slot {
            url: url.into(),
            ..slot.clone()
        };
        assert_eq!(
            cloud.download_slot(&bad, &bad.digest).await,
            Err(Error::IntegrityFailed)
        );
    }
}
