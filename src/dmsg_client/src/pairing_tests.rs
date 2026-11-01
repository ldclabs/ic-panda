use super::*;
use crate::codec::digest;
use crate::crypto::sign;
use crate::testing::{TempDir, unlocked};
use base64::Engine;
use dmsg_types::user::{AccountCommand, AccountMutation};
use dmsg_types::{Approval, Ed25519Signature};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn receiver(client: &Client) -> Receiver {
    let account = client.account().unwrap();
    Receiver {
        invitation: Invitation {
            home: account.home,
            account: account.id,
            relay: client.config.relay.clone(),
            invite_id: Hash::new([7; 32]),
            expires_at: now_ms() + LIFETIME,
            key: Hash::new([71; 32]),
            submit_token: Hash::new([72; 32]),
        },
        generation: client.session_generation,
    }
}

fn request(client: &Client, account: AccountId) -> String {
    #[derive(Serialize)]
    struct Packet {
        format: &'static str,
        home: Principal,
        request: AccountMutation,
    }
    let home = client.account().unwrap().home;
    let device = client.workspace.device_input(ControllerRole::Administrator);
    let request_id = Hash::new([31; 32]);
    let message = digest(
        "dmsg/add-device/v1",
        &(home, &account, &device, 1u64, request_id),
    );
    let proof = Ed25519Signature::new(sign(&client.keys().unwrap().signing, message.as_slice()));
    b64(&canonical(&Packet {
        format: "dmsg-device-request/2",
        home,
        request: AccountMutation {
            account_id: account,
            expected_version: 1,
            command: AccountCommand::AddDevice { device, proof },
            approval: Approval {
                device_id: client.workspace.meta.device_id,
                security_epoch: 1,
                sequence: 0,
                request_id,
                expires_at: now_ms() + 270_000,
                signature: Ed25519Signature::new([0; 64]),
                hardware: None,
            },
        },
    }))
}

fn delivery(host: &Receiver, token: &str) -> Value {
    json!({"id": host.id(), "expires_at": host.invitation.expires_at,
        "sealed": b64(&host.invitation.seal(token, &[17; 12]).unwrap())})
}

#[test]
fn encrypted_request_is_verified_without_authorizing_the_device() {
    let old_dir = TempDir::new();
    let new_dir = TempDir::new();
    let mut old = unlocked(&old_dir.0, false, None);
    let new = unlocked(&new_dir.0, false, None);
    let host = receiver(&old);
    let token = request(&new, new.account().unwrap().id);
    let qr = crate::qr::png_data(&token).unwrap();
    let image = base64::engine::general_purpose::STANDARD
        .decode(qr.strip_prefix("data:image/png;base64,").unwrap())
        .unwrap();
    assert_eq!(crate::qr::scan(&image).unwrap(), token);
    assert_eq!(
        old.pairing_preview(&token).unwrap()["device_id"],
        hex::encode(*new.workspace.meta.device_id)
    );
    let value = delivery(&host, &token);
    let preview = host.candidate(&old, &value, now_ms()).unwrap();
    assert_eq!(preview["candidate"]["token"], token);
    assert_eq!(
        preview["candidate"]["device_id"],
        hex::encode(*new.workspace.meta.device_id)
    );
    assert_eq!(preview["candidate"]["role"], "Administrator");
    assert!(old.approvals(true).unwrap().is_empty());
    assert_eq!(host.candidate(&old, &value, now_ms()).unwrap(), preview);
}

#[test]
fn another_account_tampering_and_missing_data_are_rejected() {
    let old_dir = TempDir::new();
    let new_dir = TempDir::new();
    let old = unlocked(&old_dir.0, false, None);
    let new = unlocked(&new_dir.0, false, None);
    let host = receiver(&old);
    for token in [
        request(&new, "00000000000000000000".parse().unwrap()),
        "invalid-device-request".into(),
    ] {
        assert!(
            host.candidate(&old, &delivery(&host, &token), now_ms())
                .is_err()
        );
    }
    let token = request(&new, new.account().unwrap().id);
    let mut value = delivery(&host, &token);
    let mut cipher = unb64(value["sealed"].as_str().unwrap()).unwrap();
    cipher[20] ^= 1;
    value["sealed"] = json!(b64(&cipher));
    assert_eq!(
        host.candidate(&old, &value, now_ms()),
        Err(Error::IntegrityFailed)
    );
    value.as_object_mut().unwrap().remove("sealed");
    assert_eq!(
        host.candidate(&old, &value, now_ms()),
        Err(Error::IntegrityFailed)
    );
    value["sealed"] = Value::Null;
    assert!(host.candidate(&old, &value, now_ms()).unwrap()["candidate"].is_null());
    value["id"] = json!("other-invitation");
    assert_eq!(
        host.candidate(&old, &value, now_ms()),
        Err(Error::IntegrityFailed)
    );
}

#[test]
fn submit_credential_is_not_a_decryption_key_and_context_is_bound() {
    let dir = TempDir::new();
    let client = unlocked(&dir.0, false, None);
    let mut host = receiver(&client);
    let sealed = host.invitation.seal("request", &[9; 12]).unwrap();
    assert_eq!(host.invitation.open(&sealed).unwrap(), "request");
    let key = host.invitation.key;
    host.invitation.key = host.invitation.submit_token;
    assert!(host.invitation.open(&sealed).is_err());
    host.invitation.key = key;
    host.invitation.invite_id = Hash::new([8; 32]);
    assert!(host.invitation.open(&sealed).is_err());
    assert_eq!(
        host.invitation.open(&vec![0; MAX_SEALED + 1]),
        Err(Error::QuotaExceeded)
    );
    assert_eq!(
        host.invitation.seal(&"x".repeat(4097), &[9; 12]),
        Err(Error::QuotaExceeded)
    );
}

#[test]
fn expiry_relay_change_and_reunlock_invalidate_the_invitation() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let host = receiver(&client);
    let token = host.view().unwrap()["token"].as_str().unwrap().to_owned();
    assert!(Invitation::parse(&token, now_ms()).is_ok());
    assert!(Invitation::parse(&token.replacen("/2:", "/1:", 1), now_ms()).is_err());
    assert_eq!(
        host.check_client(&client, host.invitation.expires_at),
        Err(Error::Expired)
    );
    client.config.relay = "https://elsewhere.example".into();
    assert_eq!(
        host.check_client(&client, now_ms()),
        Err(Error::VersionConflict)
    );
    client.config.relay = host.invitation.relay.clone();
    client.lock();
    client.keys = Some(client.workspace.unlock(&crate::testing::SECRET).unwrap());
    assert_eq!(host.check_client(&client, now_ms()), Err(Error::Locked));
}

async fn http_request(stream: &mut tokio::net::TcpStream) -> (String, Vec<u8>) {
    let mut data = Vec::new();
    loop {
        let mut buffer = [0; 1024];
        let n = stream.read(&mut buffer).await.unwrap();
        assert!(n > 0);
        data.extend_from_slice(&buffer[..n]);
        if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            let header = String::from_utf8(data[..at].to_vec()).unwrap();
            let length = header
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|v| v.parse::<usize>().ok())
                })
                .unwrap_or(0);
            if data.len() >= at + 4 + length {
                return (header, data[at + 4..at + 4 + length].to_vec());
            }
        }
    }
}

async fn respond(stream: &mut tokio::net::TcpStream, data: Value) {
    let body = json!({"ok": true, "data": data}).to_string();
    stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).as_bytes()).await.unwrap();
}

#[tokio::test]
async fn anonymous_submission_sends_only_the_capability_and_ciphertext() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let dir = TempDir::new();
    let client = unlocked(&dir.0, false, None);
    let host = receiver(&client);
    let sealed = host.invitation.seal("request", &[9; 12]).unwrap();
    let server = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let (header, body) = http_request(&mut stream).await;
        assert!(header.starts_with(&format!(
            "POST /v1/device-invites/{}/{} ",
            host.invitation.account,
            host.id()
        )));
        assert!(!header.to_lowercase().contains("x-dmsg-pop"));
        let fields: std::collections::BTreeMap<String, serde_bytes::ByteBuf> =
            decode(&body, 8192).unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(
            fields["submit_token"].as_ref(),
            host.invitation.submit_token.as_slice()
        );
        assert_eq!(fields["sealed"].as_ref(), sealed);
        assert!(
            !body
                .windows(32)
                .any(|v| v == host.invitation.key.as_slice())
        );
        respond(
            &mut stream,
            json!({"digest": hex::encode(*sha256(&sealed))}),
        )
        .await;
    };
    let cloud = crate::cloud::Cloud::new(&origin, true).unwrap();
    let send = cloud.pairing_submit(
        &host.invitation.account,
        &host.invitation.invite_id,
        &host.invitation.submit_token,
        &sealed,
    );
    let ((), sent) = tokio::join!(server, send);
    assert_eq!(sent.unwrap()["digest"], hex::encode(*sha256(&sealed)));
}

#[tokio::test]
async fn relay_wait_does_not_hold_the_client_and_late_reads_cannot_cross_a_lock() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    crate::testing::relay_to(&mut client, &origin);
    crate::testing::warm(&mut client);
    let host = receiver(&client);
    let h = client.hold();
    let server = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let (header, _) = http_request(&mut stream).await;
        assert!(header.to_lowercase().contains("x-dmsg-pop:"));
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            h.with(|c| {
                c.lock();
                Ok(())
            }),
        )
        .await
        .unwrap()
        .unwrap();
        respond(
            &mut stream,
            json!({"id": host.id(), "expires_at": host.invitation.expires_at, "sealed": null}),
        )
        .await;
    };
    let read = host.status(&h);
    let ((), result) = tokio::join!(server, read);
    assert_eq!(result, Err(Error::Locked));
}

#[test]
fn invitation_qr_and_encrypted_relay_vector_round_trip() {
    let dir = TempDir::new();
    let client = unlocked(&dir.0, false, None);
    let mut host = receiver(&client);
    host.invitation.expires_at = 1_800_000_300_000;
    host.invitation.relay = "https://api.dmsg.test".into();
    let plaintext = "dmsg-device-request-fixture";
    let sealed = host.invitation.seal(plaintext, &[17; 12]).unwrap();
    let mut view = host.view().unwrap();
    view["test_only"] = json!(true);
    view["now"] = json!(1_800_000_000_000u64);
    view["registration_cbor"] = json!(b64(&canonical(&host.invitation.registration())));
    view["submission_cbor"] = json!(b64(&canonical(&std::collections::BTreeMap::from([
        (
            "submit_token",
            serde_bytes::Bytes::new(host.invitation.submit_token.as_slice())
        ),
        ("sealed", serde_bytes::Bytes::new(&sealed)),
    ]))));
    view["plaintext"] = json!(plaintext);
    view["request_qr"] = json!(crate::qr::png_data(plaintext).unwrap());
    view["sealed_digest"] = json!(hex::encode(*sha256(&sealed)));
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/vectors/client/device-invite-v2.json");
    if std::env::var_os("DMSG_WRITE_CLIENT_VECTORS").is_some() {
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&view).unwrap()),
        )
        .unwrap();
    }
    let saved: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(saved, view);
    assert_eq!(host.invitation.open(&sealed).unwrap(), plaintext);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(
            view["qr"]
                .as_str()
                .unwrap()
                .strip_prefix("data:image/png;base64,")
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        crate::qr::scan(&bytes).unwrap(),
        view["token"].as_str().unwrap()
    );
}
