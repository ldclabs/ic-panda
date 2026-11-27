use super::hosted::{BridgeTransport, NearbyHost, SendPayload, SendTarget};
use super::offer::{NearbyOffer, RelayOffer, advert, pk_hint, sanitize_alias, validate_local_addr};
use super::session::{
    ReceiveDecision, SendFileItem, receive_after_handshake, receiver_handshake,
    send_files_after_handshake, send_secret_package_after_handshake, sender_handshake,
};
use super::wire::{
    Body, Channel, ClientHello, ClientReveal, EphemeralKey, OfferKind, RECEIVER, SENDER,
    ServerHello, StreamTransport, commitment, derive_session, handshake_transcript, open_frame,
    qr_proof, recv_plain, seal_frame, send_plain,
};
use super::*;
use crate::client::Client;
use crate::codec::canonical;
use crate::hold::Hold;
use crate::library::catalog::ROOT;
use crate::secrets::receiving::Selection;
use crate::secrets::shares::{AttachmentInput, NearbyTarget, ShareInput};
use crate::testing::{TempDir, unlocked};
use crate::vault::{Changes, FieldChange, Item};
use dmsg_types::{Error, Result};
use serde_bytes::ByteArray;
use serde_json::{Value as Json, json};
use std::sync::Arc;
use tokio::io::duplex;

#[derive(Clone)]
struct TestHold(Arc<tokio::sync::Mutex<Client>>);

impl TestHold {
    fn new(client: Client) -> Self {
        Self(Arc::new(tokio::sync::Mutex::new(client)))
    }
}

impl Hold for TestHold {
    async fn with<T: Send>(&self, step: impl FnOnce(&mut Client) -> Result<T> + Send) -> Result<T> {
        let mut guard = self.0.lock().await;
        step(&mut guard)
    }
}

const VECTORS: &str = "../../tests/vectors/nearby/nearby-v1.json";

#[test]
fn offer_validation_rejects_public_ips_hostnames_and_wrong_env() {
    let offer = NearbyOffer {
        v: 1,
        pk: ByteArray::from([0x11; 32]),
        qrs: ByteArray::from([0x22; 16]),
        exp: 1_800_000_000_000,
        adv: Some("0123456789abcdef".into()),
        addrs: vec!["192.168.1.42:43210".into(), "[fe80::1]:43210".into()],
        relay: Some(RelayOffer {
            env: "production".into(),
            rid: ByteArray::from([0x33; 16]),
            tok: ByteArray::from([0x44; 16]),
        }),
    };
    let encoded = offer.encode().unwrap();
    assert!(encoded.starts_with(OFFER_PREFIX));
    let decoded = NearbyOffer::parse(&encoded, Some("production")).unwrap();
    assert_eq!(decoded, offer);

    // Wrong environment rejected.
    assert_eq!(
        NearbyOffer::parse(&encoded, Some("staging")),
        Err(Error::Forbidden)
    );

    // Public IP, multicast, loopback in production and hostnames rejected.
    assert_eq!(
        validate_local_addr("8.8.8.8:443", false),
        Err(Error::Forbidden)
    );
    assert_eq!(
        validate_local_addr("127.0.0.1:4321", false),
        Err(Error::Forbidden)
    );
    assert!(validate_local_addr("127.0.0.1:4321", true).is_ok());
    assert_eq!(
        validate_local_addr("224.0.0.251:5353", true),
        Err(Error::Forbidden)
    );
    assert!(matches!(
        validate_local_addr("evil.example.com:443", true),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn alias_sanitization_strips_controls_and_bounds_utf8_bytes() {
    assert_eq!(sanitize_alias("  \x00Alice\n\tMac  "), "AliceMac");
    assert_eq!(sanitize_alias("\x07\x1b"), "dMsg Device");
    let long = "测试设备名称超长截断测试设备名称超长截断";
    let clean = sanitize_alias(long);
    assert!(clean.len() <= offer::MAX_ALIAS_BYTES);
    assert!(std::str::from_utf8(clean.as_bytes()).is_ok());
}

#[tokio::test]
async fn commitment_then_reveal_blocks_tampered_reveal_wrong_r_pk_and_missing_qr_proof() {
    let r_key = EphemeralKey::from_seed([0xaa; 32]);
    let qrs = [0xbb; 16];

    // 1. Valid QR-mode handshake succeeds and matches SAS + session key.
    let (s_io, r_io) = duplex(16 * 1024);
    let mut st = StreamTransport::new(s_io);
    let mut rt = StreamTransport::new(r_io);
    let r_pk = r_key.public_bytes();
    let (s_res, r_res) = tokio::join!(
        sender_handshake(&mut st, Some(&r_pk), Some(&qrs)),
        receiver_handshake(&mut rt, &r_key, &qrs, true, HELLO_TIMEOUT),
    );
    let (s_sec, r_sec) = (s_res.unwrap(), r_res.unwrap());
    assert_eq!(s_sec.sid, r_sec.sid);
    assert_eq!(s_sec.transcript, r_sec.transcript);
    assert_eq!(s_sec.sas, r_sec.sas);
    assert_eq!(*s_sec.key, *r_sec.key);
    assert!(s_sec.qr_bound && r_sec.qr_bound);

    // 2. QR-only receiver rejects browse-mode sender lacking qr_proof.
    let (s_io, r_io) = duplex(16 * 1024);
    let mut st = StreamTransport::new(s_io);
    let mut rt = StreamTransport::new(r_io);
    let (_s_res, r_res) = tokio::join!(
        sender_handshake(&mut st, None, None),
        receiver_handshake(&mut rt, &r_key, &qrs, true, HELLO_TIMEOUT),
    );
    assert_eq!(r_res.unwrap_err(), Error::Forbidden);

    // 3. Sender rejects mismatched receiver public key (MITM swapping R_pk in QR mode).
    let wrong_pk = [0x99; 32];
    let (s_io, r_io) = duplex(16 * 1024);
    let mut st = StreamTransport::new(s_io);
    let mut rt = StreamTransport::new(r_io);
    let (s_res, _r_res) = tokio::join!(
        sender_handshake(&mut st, Some(&wrong_pk), Some(&qrs)),
        receiver_handshake(&mut rt, &r_key, &qrs, false, HELLO_TIMEOUT),
    );
    assert_eq!(s_res.unwrap_err(), Error::IntegrityFailed);

    // 4. Receiver rejects sender that lies in ClientReveal after committing different s_nonce.
    let (s_io, r_io) = duplex(16 * 1024);
    let mut st = StreamTransport::new(s_io);
    let mut rt = StreamTransport::new(r_io);
    let attacker = async {
        let s_key = EphemeralKey::from_seed([0x12; 32]);
        let commit = commitment(&s_key.public_bytes(), &[0x01; 16]);
        send_plain(&mut st, &ClientHello { v: 1, commit })
            .await
            .unwrap();
        let _sh: ServerHello = recv_plain(&mut st).await.unwrap();
        // Send a different nonce than committed.
        send_plain(
            &mut st,
            &ClientReveal {
                s_pk: ByteArray::from(s_key.public_bytes()),
                s_nonce: ByteArray::from([0x02; 16]),
                qr_proof: None,
            },
        )
        .await
        .unwrap();
    };
    let (_, r_res) = tokio::join!(
        attacker,
        receiver_handshake(&mut rt, &r_key, &qrs, false, HELLO_TIMEOUT)
    );
    assert_eq!(r_res.unwrap_err(), Error::IntegrityFailed);
}

#[tokio::test]
async fn browse_mode_leaks_zero_metadata_before_sas_confirmation() {
    let dir = TempDir::new();
    let mut sender = unlocked(&dir.0.join("sender"), false, None);
    let sample = dir.0.join("secret-name.txt");
    std::fs::write(&sample, b"hello nearby").unwrap();
    let imported = sender
        .files_import(&sample, ROOT, true, &mut |_, _| {})
        .unwrap();
    let sender_hold = sender.hold();

    let r_key = EphemeralKey::from_seed([0x33; 32]);
    let qrs = [0x44; 16];
    let (s_io, r_io) = duplex(64 * 1024);
    let mut st = StreamTransport::new(s_io);
    let mut rt = StreamTransport::new(r_io);
    let (s_sec, r_sec) = tokio::join!(
        sender_handshake(&mut st, None, None),
        receiver_handshake(&mut rt, &r_key, &qrs, false, HELLO_TIMEOUT),
    );
    let (s_sec, r_sec) = (s_sec.unwrap(), r_sec.unwrap());
    assert!(!s_sec.qr_bound);

    let s_ch = Channel::new(st, &s_sec, SENDER);
    let mut r_ch = Channel::new(rt, &r_sec, RECEIVER);

    // Sender aborts at SAS confirmation gate -> receiver must see ZERO OfferMeta frames!
    let items = [SendFileItem {
        file_id: imported.file.id,
        version_id: None,
    }];
    let send_task = send_files_after_handshake(
        &sender_hold,
        s_ch,
        s_sec,
        &items,
        Some("Alice".into()),
        false,
        MAX_SESSION_BYTES,
        |_| async { Err(Error::Unavailable("sas mismatch".into())) },
        |_| {},
    );
    let recv_task = async { r_ch.recv(std::time::Duration::from_secs(2)).await };
    let (s_out, r_out) = tokio::join!(send_task, recv_task);
    assert!(s_out.is_err());
    assert!(r_out.is_err());
}

#[tokio::test]
async fn multi_file_transfer_commits_individually_with_credit_window_and_free_name() {
    let dir = TempDir::new();
    let mut sender = unlocked(&dir.0.join("sender"), false, None);
    let mut receiver = unlocked(&dir.0.join("receiver"), false, None);

    // Pre-populate a same-named file on receiver to verify `free_name` keeps them separate.
    let existing = dir.0.join("photo.bin");
    std::fs::write(&existing, b"receiver original").unwrap();
    receiver
        .files_import(&existing, ROOT, true, &mut |_, _| {})
        .unwrap();

    // Create two files on sender: one multi-chunk (3.2 MiB, exercises credit refill > W=4? 4 chunks, so make 5.2 MiB = 6 chunks!).
    let big_bytes = vec![0x5a_u8; 5 * 1024 * 1024 + 12345];
    let big_path = dir.0.join("big.bin");
    std::fs::write(&big_path, &big_bytes).unwrap();
    let f1 = sender
        .files_import(&big_path, ROOT, true, &mut |_, _| {})
        .unwrap();

    let small_path = dir.0.join("photo.bin");
    std::fs::write(&small_path, b"sender photo bytes").unwrap();
    let f2 = sender
        .files_import(&small_path, ROOT, true, &mut |_, _| {})
        .unwrap();

    let sender_hold = TestHold::new(sender);
    let receiver_hold = TestHold::new(receiver);

    let r_key = EphemeralKey::from_seed([0x77; 32]);
    let qrs = [0x88; 16];
    let r_pk = r_key.public_bytes();
    let (s_io, r_io) = duplex(128 * 1024);
    let mut st = StreamTransport::new(s_io);
    let mut rt = StreamTransport::new(r_io);
    let (s_sec, r_sec) = tokio::join!(
        sender_handshake(&mut st, Some(&r_pk), Some(&qrs)),
        receiver_handshake(&mut rt, &r_key, &qrs, true, HELLO_TIMEOUT),
    );
    let (s_sec, r_sec) = (s_sec.unwrap(), r_sec.unwrap());
    let s_ch = Channel::new(st, &s_sec, SENDER);
    let r_ch = Channel::new(rt, &r_sec, RECEIVER);

    let items = vec![
        SendFileItem {
            file_id: f1.file.id.clone(),
            version_id: Some(f1.version.clone()),
        },
        SendFileItem {
            file_id: f2.file.id.clone(),
            version_id: Some(f2.version.clone()),
        },
    ];

    let (send_res, recv_res) = tokio::join!(
        send_files_after_handshake(
            &sender_hold,
            s_ch,
            s_sec,
            &items,
            Some("Alice".into()),
            false,
            MAX_SESSION_BYTES,
            |_| async { Ok(()) },
            |_| {},
        ),
        receive_after_handshake(
            &receiver_hold,
            r_ch,
            r_sec,
            crate::files::MAX_FILE,
            |kind, files, _, _| async move {
                assert_eq!(kind, OfferKind::Files);
                assert_eq!(files.len(), 2);
                Ok(ReceiveDecision::Accept {
                    folder: ROOT.into(),
                    selected: vec![0, 1],
                    share_identity: false,
                })
            },
            |_| {},
        ),
    );

    let send_out = send_res.unwrap();
    let recv_out = recv_res.unwrap();
    assert_eq!(send_out.sent_items, 2);
    assert_eq!(recv_out.files.len(), 2);
    assert_eq!(recv_out.files[0].name, "big.bin");
    // Must not overwrite or join receiver's existing "photo.bin".
    assert_ne!(recv_out.files[1].name, "photo.bin");
    assert!(recv_out.files[1].name.starts_with("photo"));
    assert!(recv_out.identity.is_none());

    // Verify receiver can decrypt and read the exact transferred bytes.
    let (_, _, big_manifest) = receiver_hold
        .with(|c| c.library_manifest(&recv_out.files[0].file_id, None))
        .await
        .unwrap();
    assert_eq!(big_manifest.size, big_bytes.len() as u64);
    let mut restored = Vec::new();
    for i in 0..crate::files::chunk_count(big_manifest.size) {
        let chunk = receiver_hold
            .with(|c| crate::files::read_chunk(c.blobs()?, &big_manifest, i))
            .await
            .unwrap();
        restored.extend_from_slice(&chunk);
    }
    assert_eq!(restored, big_bytes);
}

#[tokio::test]
async fn vault_secret_package_flows_through_secret_share_and_secret_package_import_approvals() {
    let dir = TempDir::new();
    let mut sender = unlocked(&dir.0.join("sender"), false, None);
    let receiver = unlocked(&dir.0.join("receiver"), false, None);

    // Import an attachment on sender and save a Vault entry referencing it.
    let att_path = dir.0.join("recovery.txt");
    std::fs::write(&att_path, b"attachment-secret-content").unwrap();
    let att = sender
        .files_import(&att_path, ROOT, true, &mut |_, _| {})
        .unwrap();

    let saved_entry = sender
        .vault_create(
            "api",
            &Changes {
                title: Some("Production API".into()),
                notes: Some("private notes not shared".into()),
                tags: Some(vec!["internal-tag".into()]),
                files: Some(vec![att.file.id.clone()]),
                fields: Some(vec![
                    FieldChange {
                        id: Some("url".into()),
                        kind: Some("url".into()),
                        label: Some("Endpoint".into()),
                        value: Some("https://api.example.com".into()),
                        ..Default::default()
                    },
                    FieldChange {
                        id: Some("credential".into()),
                        kind: Some("concealed".into()),
                        label: Some("Token".into()),
                        value: Some("sk-live-nearby-999".into()),
                        ..Default::default()
                    },
                ]),
                ..Default::default()
            },
        )
        .unwrap();

    // Reserved `dmsg-` tagged entries must be rejected.
    let mut reserved = Item::new("key", "Agent Key", 1);
    reserved.tags = vec!["dmsg-agent".into()];
    let reserved_entry = sender
        .vault_item_with_id("reserved-agent-key-1", &reserved)
        .unwrap();
    assert!(
        sender
            .secret_nearby_preparation(
                ShareInput {
                    item_id: reserved_entry.id.clone(),
                    revision: reserved_entry.revision.clone(),
                    field_ids: vec![],
                    include_notes: true,
                    attachments: vec![],
                    expires_in_ms: None,
                    max_download_sessions: None,
                },
                NearbyTarget {
                    transcript: "11".repeat(32),
                    sas: "123456".into(),
                    peer_label: "Bob".into(),
                },
            )
            .is_err()
    );

    let sender_hold = TestHold::new(sender);
    let receiver_hold = TestHold::new(receiver);

    let r_key = EphemeralKey::from_seed([0x55; 32]);
    let qrs = [0x66; 16];
    let r_pk = r_key.public_bytes();
    let (s_io, r_io) = duplex(128 * 1024);
    let mut st = StreamTransport::new(s_io);
    let mut rt = StreamTransport::new(r_io);
    let (s_sec, r_sec) = tokio::join!(
        sender_handshake(&mut st, Some(&r_pk), Some(&qrs)),
        receiver_handshake(&mut rt, &r_key, &qrs, true, HELLO_TIMEOUT),
    );
    let (s_sec, r_sec) = (s_sec.unwrap(), r_sec.unwrap());
    let s_ch = Channel::new(st, &s_sec, SENDER);
    let r_ch = Channel::new(rt, &r_sec, RECEIVER);

    let share_input = ShareInput {
        item_id: saved_entry.id.clone(),
        revision: saved_entry.revision.clone(),
        field_ids: vec!["credential".into()],
        include_notes: false,
        attachments: vec![AttachmentInput {
            file_id: att.file.id.clone(),
            version_id: att.version.clone(),
        }],
        expires_in_ms: None,
        max_download_sessions: None,
    };

    // Prepare & compute exact size bound to transcript.
    let (job_id, plain_bytes, approval_id, approval_digest) = sender_hold
        .with(|c| {
            let prepared = c.secret_nearby_preparation(
                share_input,
                NearbyTarget {
                    transcript: hex::encode(*s_sec.transcript),
                    sas: s_sec.sas.clone(),
                    peer_label: "Bob".into(),
                },
            )?;
            let view = c.secret_share_save_preparation(prepared)?;
            let job_id = view["id"].as_str().unwrap().to_string();
            let plain_bytes = view["plan"]["plain_bytes"].as_u64().unwrap();
            let approval = c.secret_share_request("gui", &job_id)?;
            assert_eq!(approval["risk"], "high");
            Ok((
                job_id,
                plain_bytes,
                approval["id"].as_str().unwrap().to_string(),
                approval["digest"].as_str().unwrap().to_string(),
            ))
        })
        .await
        .unwrap();

    let sender_hold_approve = sender_hold.clone();
    let (send_res, recv_res) = tokio::join!(
        send_secret_package_after_handshake(
            &sender_hold,
            s_ch,
            s_sec,
            plain_bytes,
            Some("Alice".into()),
            false,
            |_| async { Ok(()) },
            |_| async move {
                // Approve via high-risk SecretShare flow (no cloud upload!).
                let run = {
                    let mut c = sender_hold_approve.0.lock().await;
                    c.approval_decide(&approval_id, &approval_digest, true, "gui", false)
                        .await?
                };
                match run {
                    crate::approval::Decision::SecretShare(share_run) => {
                        let settled = share_run.run(&sender_hold_approve).await?;
                        assert_eq!(settled["state"], "ready_for_nearby");
                    }
                    _ => panic!("expected SecretShare decision"),
                }
                Ok(job_id)
            },
            |_| {},
        ),
        receive_after_handshake(
            &receiver_hold,
            r_ch,
            r_sec,
            crate::files::MAX_FILE,
            |kind, files, pkg_size, _| async move {
                assert_eq!(kind, OfferKind::SecretPackage);
                assert!(files.is_empty());
                assert_eq!(pkg_size, Some(plain_bytes));
                Ok(ReceiveDecision::Accept {
                    folder: ROOT.into(),
                    selected: vec![],
                    share_identity: false,
                })
            },
            |_| {},
        ),
    );

    let send_out = send_res.unwrap();
    let recv_out = recv_res.unwrap();
    assert_eq!(send_out.sent_items, 1);
    let preview = recv_out.package_preview.expect("staged package preview");
    // Preview must NEVER leak secret values!
    assert!(!preview.to_string().contains("sk-live-nearby-999"));
    assert!(!preview.to_string().contains("private notes not shared"));
    assert!(!preview.to_string().contains("internal-tag"));

    // Receiver imports through existing SecretPackageImport high-risk approval!
    let package_id = preview["package_id"].as_str().unwrap().to_string();
    let digest = preview["digest"].as_str().unwrap().to_string();
    let field_id = preview["preview"]["fields"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let selection = Selection {
        package_id,
        digest,
        fields: vec![field_id],
        include_notes: false,
        attachments: vec![0],
        folder: ROOT.into(),
    };
    let import_req = receiver_hold
        .with(|c| c.secret_package_request("gui", selection))
        .await
        .unwrap();
    assert_eq!(import_req["risk"], "high");
    let import_dec = {
        let mut c = receiver_hold.0.lock().await;
        c.approval_decide(
            import_req["id"].as_str().unwrap(),
            import_req["digest"].as_str().unwrap(),
            true,
            "gui",
            false,
        )
        .await
        .unwrap()
    };
    let crate::approval::Decision::SecretPackageImport(import_run) = import_dec else {
        panic!("expected SecretPackageImport decision");
    };
    let done = import_run.run(&receiver_hold).await.unwrap();
    assert_eq!(done["state"], "done");

    let imported_items = receiver_hold
        .with(|c| c.vault_list(&crate::vault::Filter::default()))
        .await
        .unwrap();
    assert_eq!(imported_items.len(), 1);
    let imported_entry = receiver_hold
        .with(|c| c.vault_show(&imported_items[0].id))
        .await
        .unwrap();
    assert_eq!(imported_entry.item.title, "Production API");
    assert_eq!(imported_entry.item.fields.len(), 1);
    assert_eq!(imported_entry.item.fields[0].value, "sk-live-nearby-999");
    assert!(imported_entry.item.notes.is_empty());
    assert!(imported_entry.item.tags.is_empty());
    assert_eq!(imported_entry.item.files.len(), 1);
}

#[tokio::test]
async fn hosted_discovery_shows_matching_codes_before_offering_files() {
    let dir = TempDir::new();
    let mut sender = unlocked(&dir.0.join("sender"), false, None);
    let receiver = unlocked(&dir.0.join("receiver"), false, None);
    let file_path = dir.0.join("notes.md");
    std::fs::write(&file_path, b"# Nearby Hello\n").unwrap();
    let imported = sender
        .files_import(&file_path, ROOT, true, &mut |_, _| {})
        .unwrap();

    let sender_hold = TestHold::new(sender);
    let receiver_hold = TestHold::new(receiver);
    let recv_host = NearbyHost::new();
    let send_host = NearbyHost::new();
    let noop: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});

    recv_host
        .receive_start(
            receiver_hold.clone(),
            Some("Bob Mac".into()),
            true,
            crate::files::MAX_FILE,
            noop.clone(),
        )
        .await
        .unwrap();
    // Connect over BridgeTransport (simulating Apple AWDL NWConnection bridge).
    let (s_bridge, mut s_handle) = BridgeTransport::pair(16);
    let (r_bridge, mut r_handle) = BridgeTransport::pair(16);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(b) = s_handle.outbound.recv() => {
                    if r_handle.inbound.send(b).await.is_err() { break; }
                }
                Some(b) = r_handle.outbound.recv() => {
                    if s_handle.inbound.send(b).await.is_err() { break; }
                }
                else => break,
            }
        }
    });
    recv_host
        .receive_attach_bridge(
            receiver_hold.clone(),
            r_bridge,
            crate::files::MAX_FILE,
            noop.clone(),
        )
        .unwrap();

    send_host
        .send_start(
            sender_hold.clone(),
            SendTarget::Direct {
                host: String::new(),
                port: 0,
                alias: Some("Bob Mac".into()),
                awdl: Some(s_bridge),
            },
            SendPayload::Files(vec![SendFileItem {
                file_id: imported.file.id.clone(),
                version_id: None,
            }]),
            Some("Alice".into()),
            false,
            "gui".into(),
            noop.clone(),
        )
        .await
        .unwrap();

    until("both verification codes", [&recv_host, &send_host], || {
        recv_host.view()["receive"]["verification_codes"][0].is_string()
            && send_host.view()["send"]["awaiting_sas_confirm"] == true
    })
    .await;
    let recv = &recv_host.view()["receive"];
    assert_eq!(
        recv["verification_codes"][0],
        send_host.view()["send"]["sas"]
    );
    assert_eq!(recv["pending_peers"], json!([]));
    send_host.send_confirm_sas().unwrap();

    let mut peer_id = String::new();
    for _ in 0..100 {
        let view = recv_host.view();
        if let Some(arr) = view["receive"]["pending_peers"].as_array()
            && !arr.is_empty()
        {
            peer_id = arr[0]["peer_id"].as_str().unwrap().to_string();
            assert_eq!(arr[0]["transport"], "awdl");
            assert_eq!(arr[0]["qr_bound"], false);
            assert_eq!(view["receive"]["verification_codes"], json!([]));
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(!peer_id.is_empty());

    recv_host
        .receive_accept(&peer_id, ROOT.into(), vec![0], false)
        .unwrap();

    for _ in 0..100 {
        let rv = recv_host.view();
        let sv = send_host.view();
        if rv["receive"]["state"] == "done" && sv["send"]["state"] == "done" {
            assert_eq!(rv["receive"]["outcome"]["files"][0]["name"], "notes.md");
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!(
        "transfer did not complete: recv={} send={}",
        recv_host.view(),
        send_host.view()
    );
}

#[test]
fn protocol_vectors_match_nearby_v1_json() {
    let r_seed = [0x11u8; 32];
    let s_seed = [0x22u8; 32];
    let r_key = EphemeralKey::from_seed(r_seed);
    let s_key = EphemeralKey::from_seed(s_seed);
    let sid = [0x33u8; 16];
    let s_nonce = [0x44u8; 16];
    let r_nonce = [0x55u8; 16];
    let qrs = [0x66u8; 16];

    let r_pk = r_key.public_bytes();
    let s_pk = s_key.public_bytes();
    let commit = commitment(&s_pk, &s_nonce);
    let transcript = handshake_transcript(&sid, &commit, &s_pk, &s_nonce, &r_pk, &r_nonce);
    let proof = qr_proof(&qrs, &transcript);
    let shared_s = s_key.diffie_hellman(&r_pk).unwrap();
    let shared_r = r_key.diffie_hellman(&s_pk).unwrap();
    assert_eq!(*shared_s, *shared_r);

    let sec_qr = derive_session(sid, &shared_s, transcript, Some(&qrs));
    let sec_browse = derive_session(sid, &shared_s, transcript, None);
    assert_eq!(sec_qr.sas, sec_browse.sas);
    assert_ne!(*sec_qr.key, *sec_browse.key);

    let offer = NearbyOffer {
        v: 1,
        pk: ByteArray::from(r_pk),
        qrs: ByteArray::from(qrs),
        exp: 1_800_000_000_000,
        adv: Some(advert(&sid)),
        addrs: vec!["192.168.1.10:42424".into()],
        relay: Some(RelayOffer {
            env: "production".into(),
            rid: ByteArray::from([0x77u8; 16]),
            tok: ByteArray::from([0x88u8; 16]),
        }),
    };
    let offer_encoded = offer.encode().unwrap();

    let sample_body = Body::OfferMeta {
        kind: OfferKind::Files,
        files: vec![super::wire::FileSummary {
            index: 0,
            name: "hello.txt".into(),
            mime: "text/plain".into(),
            size: 5,
        }],
        package_size: None,
        alias: Some("Alice".into()),
    };
    let sealed = seal_frame(&sec_qr.key, &sid, SENDER, 0, &sample_body).unwrap();
    assert_eq!(
        open_frame(&sec_qr.key, &sid, SENDER, 0, &sealed).unwrap(),
        sample_body
    );

    let generated = json!({
        "format": FORMAT,
        "service": SERVICE,
        "r_seed": hex::encode(r_seed),
        "s_seed": hex::encode(s_seed),
        "r_pk": hex::encode(r_pk),
        "s_pk": hex::encode(s_pk),
        "sid": hex::encode(sid),
        "s_nonce": hex::encode(s_nonce),
        "r_nonce": hex::encode(r_nonce),
        "qrs": hex::encode(qrs),
        "advert": advert(&sid),
        "pk_hint": pk_hint(&r_pk),
        "offer_encoded": offer_encoded,
        "client_hello_cbor": hex::encode(canonical(&ClientHello { v: 1, commit })),
        "server_hello_cbor": hex::encode(canonical(&ServerHello {
            v: 1,
            sid: ByteArray::from(sid),
            r_pk: ByteArray::from(r_pk),
            r_nonce: ByteArray::from(r_nonce),
        })),
        "client_reveal_cbor": hex::encode(canonical(&ClientReveal {
            s_pk: ByteArray::from(s_pk),
            s_nonce: ByteArray::from(s_nonce),
            qr_proof: Some(proof),
        })),
        "commit": hex::encode(*commit),
        "transcript": hex::encode(*transcript),
        "qr_proof": hex::encode(*proof),
        "shared": hex::encode(*shared_s),
        "sas": sec_qr.sas,
        "key_qr": hex::encode(*sec_qr.key),
        "key_browse": hex::encode(*sec_browse.key),
        "offer_meta_frame_hex": hex::encode(&sealed),
    });

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(VECTORS);
    if std::env::var_os("UPDATE_VECTORS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&generated).unwrap()),
        )
        .unwrap();
    }
    let expected: Json =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("nearby-v1.json")).unwrap();
    assert_eq!(generated, expected);
}

#[tokio::test]
async fn ws_blind_relay_transport_transfers_end_to_end_when_lan_unavailable() {
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::tungstenite::Message;

    let rid = [0x42u8; 16];
    let recv_tok = [0x10u8; 16];
    let send_tok = [0x20u8; 16];
    let expected_recv_auth = format!("Bearer {}", offer::bearer_token(&recv_tok));
    let expected_send_auth = format!("Bearer {}", offer::bearer_token(&send_tok));

    // Minimal local blind-relay WS server: authenticates two peers by Bearer header and forwards binary frames blindly.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let origin = format!("http://{addr}");

    let relay_server = tokio::spawn(async move {
        let mut sockets = Vec::new();
        for _ in 0..2 {
            let (stream, _) = listener.accept().await.unwrap();
            let mut role = String::new();
            #[allow(clippy::result_large_err)]
            let ws = tokio_tungstenite::accept_hdr_async(
                stream,
                |req: &tokio_tungstenite::tungstenite::handshake::server::Request,
                 resp: tokio_tungstenite::tungstenite::handshake::server::Response| {
                    assert!(req.headers().get("origin").is_none());
                    assert_eq!(
                        req.uri().path(),
                        format!("/v1/nearby-relays/{}/ws", hex::encode(rid))
                    );
                    let auth = req
                        .headers()
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or_default();
                    if auth == expected_recv_auth {
                        role = "receiver".into();
                    } else if auth == expected_send_auth {
                        role = "sender".into();
                    } else {
                        panic!("unexpected bearer: {auth}");
                    }
                    Ok(resp)
                },
            )
            .await
            .unwrap();
            sockets.push((role, ws));
        }
        let ((_, ws_a), (_, ws_b)) = (sockets.remove(0), sockets.remove(0));
        let (mut sink_a, mut stream_a) = ws_a.split();
        let (mut sink_b, mut stream_b) = ws_b.split();
        let a_to_b = async {
            while let Some(Ok(Message::Binary(data))) = stream_a.next().await {
                if sink_b.send(Message::Binary(data)).await.is_err() {
                    break;
                }
            }
            let _ = sink_b.close().await;
        };
        let b_to_a = async {
            while let Some(Ok(Message::Binary(data))) = stream_b.next().await {
                if sink_a.send(Message::Binary(data)).await.is_err() {
                    break;
                }
            }
            let _ = sink_a.close().await;
        };
        tokio::join!(a_to_b, b_to_a);
    });

    let dir = TempDir::new();
    let mut sender = unlocked(&dir.0.join("sender"), false, None);
    let receiver = unlocked(&dir.0.join("receiver"), false, None);
    let file_path = dir.0.join("relay-doc.txt");
    std::fs::write(&file_path, b"relayed end-to-end encrypted bytes").unwrap();
    let imported = sender
        .files_import(&file_path, ROOT, true, &mut |_, _| {})
        .unwrap();

    let sender_hold = TestHold::new(sender);
    let receiver_hold = TestHold::new(receiver);

    let r_key = EphemeralKey::from_seed([0x91; 32]);
    let r_pk = r_key.public_bytes();
    let qrs = [0x92; 16];

    let (mut recv_ws, mut send_ws) = tokio::try_join!(
        relay::WsFrameTransport::connect(&origin, &rid, &recv_tok),
        relay::WsFrameTransport::connect(&origin, &rid, &send_tok),
    )
    .unwrap();

    let (s_sec, r_sec) = tokio::join!(
        sender_handshake(&mut send_ws, Some(&r_pk), Some(&qrs)),
        receiver_handshake(&mut recv_ws, &r_key, &qrs, true, HELLO_TIMEOUT),
    );
    let (s_sec, r_sec) = (s_sec.unwrap(), r_sec.unwrap());
    assert_eq!(s_sec.sas, r_sec.sas);
    let s_ch = Channel::new(send_ws, &s_sec, SENDER);
    let r_ch = Channel::new(recv_ws, &r_sec, RECEIVER);

    let items = [SendFileItem {
        file_id: imported.file.id,
        version_id: None,
    }];
    let (s_out, r_out) = tokio::join!(
        send_files_after_handshake(
            &sender_hold,
            s_ch,
            s_sec,
            &items,
            Some("Alice".into()),
            false,
            RELAY_MAX_BYTES,
            |_| async { Ok(()) },
            |_| {},
        ),
        receive_after_handshake(
            &receiver_hold,
            r_ch,
            r_sec,
            RELAY_MAX_BYTES,
            |kind, files, _, _| async move {
                assert_eq!(kind, OfferKind::Files);
                assert_eq!(files.len(), 1);
                Ok(ReceiveDecision::Accept {
                    folder: ROOT.into(),
                    selected: vec![0],
                    share_identity: false,
                })
            },
            |_| {},
        ),
    );
    assert_eq!(s_out.unwrap().sent_items, 1);
    assert_eq!(r_out.unwrap().files[0].name, "relay-doc.txt");
    relay_server.abort();
}

fn standalone(dir: &std::path::Path) -> Client {
    let mut c = Client::open(crate::testing::config(false), dir, None).unwrap();
    let request = c.prepare_local_create().unwrap();
    c.complete_local_create(
        request
            .authenticate(Some("nearby fixture password"))
            .unwrap(),
    )
    .unwrap();
    c
}

/// Pump frames both ways between two in-memory bridges, as the Apple byte bridges do.
fn bridge_pair() -> (BridgeTransport, BridgeTransport) {
    let (s_bridge, mut s_handle) = BridgeTransport::pair(16);
    let (r_bridge, mut r_handle) = BridgeTransport::pair(16);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(b) = s_handle.outbound.recv() => {
                    if r_handle.inbound.send(b).await.is_err() { break; }
                }
                Some(b) = r_handle.outbound.recv() => {
                    if s_handle.inbound.send(b).await.is_err() { break; }
                }
                else => break,
            }
        }
    });
    (s_bridge, r_bridge)
}

async fn until(what: &str, hosts: [&NearbyHost; 2], mut ready: impl FnMut() -> bool) {
    for _ in 0..250 {
        if ready() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!(
        "timed out waiting for {what}: {} {}",
        hosts[0].view(),
        hosts[1].view()
    );
}

/// Two Standalone workspaces whose Vault send the receiver accepted; the
/// sender waits on its `SecretShare` approval.
async fn standalone_vault_send(
    dir: &std::path::Path,
) -> (TestHold, TestHold, NearbyHost, NearbyHost, String) {
    let (mut sender, receiver) = {
        let _fixture = crate::testing::PASSWORD_FIXTURE.lock().await;
        (
            standalone(&dir.join("sender")),
            standalone(&dir.join("receiver")),
        )
    };
    assert!(sender.is_local() && receiver.is_local());
    let entry = sender
        .vault_create(
            "api",
            &Changes {
                title: Some("Router".into()),
                fields: Some(vec![FieldChange {
                    id: Some("credential".into()),
                    kind: Some("concealed".into()),
                    label: Some("Password".into()),
                    value: Some("router-secret-1".into()),
                    ..Default::default()
                }]),
                ..Default::default()
            },
        )
        .unwrap();
    let sender_hold = TestHold::new(sender);
    let receiver_hold = TestHold::new(receiver);
    let recv_host = NearbyHost::new();
    let send_host = NearbyHost::new();
    let noop: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});

    let started = recv_host
        .receive_start(
            receiver_hold.clone(),
            Some("Bob".into()),
            false,
            crate::files::MAX_FILE,
            noop.clone(),
        )
        .await
        .unwrap();
    let offer =
        NearbyOffer::parse(started["receive"]["offer"].as_str().unwrap(), Some("local")).unwrap();
    let (s_bridge, r_bridge) = bridge_pair();
    recv_host
        .receive_attach_bridge(
            receiver_hold.clone(),
            r_bridge,
            crate::files::MAX_FILE,
            noop.clone(),
        )
        .unwrap();
    send_host
        .send_start(
            sender_hold.clone(),
            SendTarget::Offer {
                offer,
                resolved: None,
                awdl: Some(s_bridge),
            },
            SendPayload::VaultItem(ShareInput {
                item_id: entry.id.clone(),
                revision: entry.revision.clone(),
                field_ids: vec!["credential".into()],
                include_notes: false,
                attachments: vec![],
                expires_in_ms: None,
                max_download_sessions: None,
            }),
            Some("Alice".into()),
            false,
            "gui".into(),
            noop.clone(),
        )
        .await
        .unwrap();

    until("the pending peer", [&recv_host, &send_host], || {
        recv_host.view()["receive"]["pending_peers"]
            .as_array()
            .is_some_and(|p| !p.is_empty())
    })
    .await;
    // No share job or approval exists before the receiver accepts.
    assert!(send_host.view()["send"]["approval_id"].is_null());
    let listed = sender_hold.with(|c| c.secret_shares()).await.unwrap();
    assert_eq!(listed["shares"], json!([]));
    let peer_id = recv_host.view()["receive"]["pending_peers"][0]["peer_id"]
        .as_str()
        .unwrap()
        .to_string();
    recv_host
        .receive_accept(&peer_id, ROOT.into(), vec![], false)
        .unwrap();

    until("the share approval", [&recv_host, &send_host], || {
        send_host.view()["send"]["approval_id"].is_string()
    })
    .await;
    let approval_id = send_host.view()["send"]["approval_id"]
        .as_str()
        .unwrap()
        .to_string();
    (
        sender_hold,
        receiver_hold,
        recv_host,
        send_host,
        approval_id,
    )
}

#[tokio::test]
async fn standalone_workspaces_share_a_vault_snapshot_and_approve_only_after_acceptance() {
    let dir = TempDir::new();
    let (sender_hold, receiver_hold, recv_host, send_host, approval_id) =
        standalone_vault_send(&dir.0).await;
    let decision = {
        let mut c = sender_hold.0.lock().await;
        let shown = c
            .approval_view(&c.approval_record(&approval_id).unwrap())
            .unwrap();
        c.approval_decide(
            &approval_id,
            shown["digest"].as_str().unwrap(),
            true,
            "gui",
            false,
        )
        .await
        .unwrap()
    };
    let crate::approval::Decision::SecretShare(run) = decision else {
        panic!("expected a SecretShare decision");
    };
    let settled = run.run(&sender_hold).await.unwrap();
    assert_eq!(settled["state"], "ready_for_nearby");

    until("both sides to finish", [&recv_host, &send_host], || {
        let (rv, sv) = (recv_host.view(), send_host.view());
        let ended = |s: &Json| matches!(s.as_str(), Some("done" | "failed" | "cancelled"));
        ended(&rv["receive"]["state"]) && ended(&sv["send"]["state"])
    })
    .await;
    let (rv, sv) = (recv_host.view(), send_host.view());
    assert_eq!(rv["receive"]["state"], "done", "{rv}");
    assert_eq!(sv["send"]["state"], "done", "{sv}");

    let preview = rv["receive"]["outcome"]["package_preview"].clone();
    let selection = Selection {
        package_id: preview["package_id"].as_str().unwrap().into(),
        digest: preview["digest"].as_str().unwrap().into(),
        fields: vec![
            preview["preview"]["fields"][0]["id"]
                .as_str()
                .unwrap()
                .into(),
        ],
        include_notes: false,
        attachments: vec![],
        folder: ROOT.into(),
    };
    let request = receiver_hold
        .with(|c| c.secret_package_request("gui", selection))
        .await
        .unwrap();
    let decision = {
        let mut c = receiver_hold.0.lock().await;
        c.approval_decide(
            request["id"].as_str().unwrap(),
            request["digest"].as_str().unwrap(),
            true,
            "gui",
            false,
        )
        .await
        .unwrap()
    };
    let crate::approval::Decision::SecretPackageImport(run) = decision else {
        panic!("expected a SecretPackageImport decision");
    };
    assert_eq!(run.run(&receiver_hold).await.unwrap()["state"], "done");
    let items = receiver_hold
        .with(|c| c.vault_list(&crate::vault::Filter::default()))
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    let imported = receiver_hold
        .with(|c| c.vault_show(&items[0].id))
        .await
        .unwrap();
    assert_eq!(imported.item.fields[0].value, "router-secret-1");
}

#[tokio::test]
async fn cancelling_a_vault_send_cancels_its_share_job_and_approval() {
    let dir = TempDir::new();
    let (sender_hold, _receiver_hold, _recv_host, send_host, approval_id) =
        standalone_vault_send(&dir.0).await;
    let job_id = send_host.view()["send"]["share_job_id"]
        .as_str()
        .unwrap()
        .to_string();
    send_host.send_cancel();

    let mut job = Json::Null;
    for _ in 0..250 {
        job = sender_hold
            .with(|c| c.secret_share_get(&job_id, false))
            .await
            .unwrap();
        if job["state"] == "cancelled" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(job["state"], "cancelled", "{job}");
    let c = sender_hold.0.lock().await;
    let approval = c
        .approval_view(&c.approval_record(&approval_id).unwrap())
        .unwrap();
    assert_eq!(approval["state"], "cancelled");
    assert_eq!(send_host.view()["send"]["state"], "cancelled");
}

#[tokio::test]
async fn rejecting_a_nearby_share_ends_the_send_and_preserves_the_decision() {
    let dir = TempDir::new();
    let (sender, _receiver, recv_host, send_host, approval_id) =
        standalone_vault_send(&dir.0).await;
    let mut c = sender.0.lock().await;
    let request = c.approval_record(&approval_id).unwrap();
    c.approval_decide(
        &approval_id,
        &hex::encode(*request.digest()),
        false,
        "gui",
        false,
    )
    .await
    .unwrap();
    drop(c);
    until("rejection", [&recv_host, &send_host], || {
        send_host.view()["send"]["state"] == "failed"
    })
    .await;
    let c = sender.0.lock().await;
    assert_eq!(
        c.approval_record(&approval_id).unwrap().state,
        crate::approval::State::Rejected
    );
    assert_eq!(c.secret_shares().unwrap()["shares"][0]["state"], "failed");
}

#[tokio::test(start_paused = true)]
async fn unused_nearby_offer_expires_and_closes_its_listener() {
    let dir = TempDir::new();
    let receiver = TestHold::new(unlocked(&dir.0, false, None));
    let host = NearbyHost::new();
    let started = host
        .receive_start(
            receiver,
            None,
            true,
            crate::files::MAX_FILE,
            Arc::new(|| {}),
        )
        .await
        .unwrap();
    let port = started["receive"]["port"].as_u64().unwrap() as u16;
    tokio::task::yield_now().await;
    tokio::time::advance(std::time::Duration::from_millis(super::OFFER_TTL_MS + 1)).await;
    tokio::task::yield_now().await;
    assert_eq!(host.view()["receive"]["error"]["code"], "EXPIRED");
    assert!(
        tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn leaving_the_workspace_invalidates_a_receive_start_still_waiting_on_the_client() {
    let dir = TempDir::new();
    let receiver = TestHold::new(unlocked(&dir.0, false, None));
    let host = NearbyHost::new();
    let locked = receiver.0.lock().await;
    let starting = host.receive_start(
        receiver.clone(),
        None,
        false,
        crate::files::MAX_FILE,
        Arc::new(|| {}),
    );
    tokio::pin!(starting);
    tokio::select! {
        biased;
        _ = &mut starting => panic!("start must wait for the client"),
        _ = std::future::ready(()) => {}
    }
    host.clear();
    drop(locked);
    assert_eq!(starting.await, Err(Error::Expired));
    assert!(host.view()["receive"].is_null());
}

#[tokio::test]
async fn receiver_drops_a_sender_that_leaves_and_a_file_unlike_its_offer() {
    let dir = TempDir::new();
    let receiver_hold = TestHold::new(unlocked(&dir.0.join("receiver"), false, None));
    let r_key = EphemeralKey::from_seed([0x31; 32]);
    let qrs = [0x32; 16];
    let r_pk = r_key.public_bytes();
    let offered = Body::OfferMeta {
        kind: OfferKind::Files,
        files: vec![super::wire::FileSummary {
            index: 0,
            name: "report.pdf".into(),
            mime: "application/pdf".into(),
            size: 3,
        }],
        package_size: None,
        alias: Some("Al\u{7}ice".into()),
    };

    // A sender that leaves while the person decides ends the receive at once.
    let (s_io, r_io) = duplex(64 * 1024);
    let (mut st, mut rt) = (StreamTransport::new(s_io), StreamTransport::new(r_io));
    let (s_sec, r_sec) = tokio::join!(
        sender_handshake(&mut st, Some(&r_pk), Some(&qrs)),
        receiver_handshake(&mut rt, &r_key, &qrs, true, HELLO_TIMEOUT),
    );
    let (s_sec, r_sec) = (s_sec.unwrap(), r_sec.unwrap());
    let mut s_ch = Channel::new(st, &s_sec, SENDER);
    s_ch.send(&offered).await.unwrap();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        drop(s_ch);
    });
    let left = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        receive_after_handshake(
            &receiver_hold,
            Channel::new(rt, &r_sec, RECEIVER),
            r_sec,
            crate::files::MAX_FILE,
            |_, _, _, alias| {
                assert_eq!(alias.as_deref(), Some("Alice"));
                std::future::pending::<Result<ReceiveDecision>>()
            },
            |_| {},
        ),
    )
    .await
    .expect("the receiver notices the sender left");
    assert!(left.is_err());

    // A file whose name differs from the accepted offer is refused.
    let (s_io, r_io) = duplex(64 * 1024);
    let (mut st, mut rt) = (StreamTransport::new(s_io), StreamTransport::new(r_io));
    let (s_sec, r_sec) = tokio::join!(
        sender_handshake(&mut st, Some(&r_pk), Some(&qrs)),
        receiver_handshake(&mut rt, &r_key, &qrs, true, HELLO_TIMEOUT),
    );
    let (s_sec, r_sec) = (s_sec.unwrap(), r_sec.unwrap());
    let mut s_ch = Channel::new(st, &s_sec, SENDER);
    s_ch.send(&offered).await.unwrap();
    let (received, _sender) = tokio::join!(
        receive_after_handshake(
            &receiver_hold,
            Channel::new(rt, &r_sec, RECEIVER),
            r_sec,
            crate::files::MAX_FILE,
            |_, _, _, _| async {
                Ok(ReceiveDecision::Accept {
                    folder: ROOT.into(),
                    selected: vec![0],
                    share_identity: false,
                })
            },
            |_| {},
        ),
        async {
            let accept = s_ch.recv(CONFIRM_TIMEOUT).await.unwrap();
            assert!(matches!(accept, Body::Accept { .. }));
            s_ch.send(&Body::FileBegin {
                index: 0,
                name: "payload.exe".into(),
                mime: "application/octet-stream".into(),
                size: 3,
                sha256: hex::encode(*crate::codec::sha256(b"abc")),
            })
            .await
            .unwrap();
            s_ch
        },
    );
    assert_eq!(received.unwrap_err(), Error::IntegrityFailed);
    let jobs = receiver_hold.with(|c| c.import_jobs()).await.unwrap();
    assert!(jobs.is_empty());
}

#[tokio::test]
async fn same_account_identity_needs_a_registered_device_key() {
    let dir = TempDir::new();
    let sender = unlocked(&dir.0.join("sender"), false, None);
    let mut receiver = unlocked(&dir.0.join("receiver"), false, None);
    // Both test devices sign as the same account.
    let mut devices = crate::testing::account_state(&sender).security.devices;
    let sender_device = sender.workspace.meta.device_id;
    let sender_hold = TestHold::new(sender);
    let transcript = dmsg_types::Hash::new([0x5c; 32]);
    let frame = super::session::build_identity(&sender_hold, &transcript, "sender")
        .await
        .unwrap();

    let status = |receiver: Client, frame: Body| async move {
        let hold = TestHold::new(receiver);
        let identity = super::session::verify_identity(&hold, &transcript, "sender", frame).await;
        (
            identity.unwrap().status,
            Arc::try_unwrap(hold.0).ok().unwrap().into_inner(),
        )
    };
    // The account ID alone is public.
    let (seen, mut receiver_back) = status(receiver, frame.clone()).await;
    assert_eq!(seen, "signature_only");
    receiver_back.security = Some(crate::testing::security(1, devices.clone()));
    let (seen, mut receiver_back) = status(receiver_back, frame.clone()).await;
    assert_eq!(seen, "same_account");
    devices.get_mut(&sender_device).unwrap().revoked_at = Some(1);
    receiver_back.security = Some(crate::testing::security(1, devices));
    receiver = receiver_back;
    let (seen, _) = status(receiver, frame).await;
    assert_eq!(seen, "signature_only");
}
