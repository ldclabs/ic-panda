use super::*;
use crate::vectors::{bytes, load, text, value};

#[test]
fn letters_open_with_the_synced_inbox_key() {
    let v = load("inbox-v1.json");
    let account = text(&v["subject_id"]);
    let version = v["key_version"].as_u64().unwrap();
    assert_eq!(
        canonical(&("dmsg/inbox-key/1", account, version)),
        bytes(&v["key_object_id_input"]["hex"])
    );
    assert_eq!(key_object(account, version), text(&v["key_object_id"]));
    let key: InboxKey = decode(&canonical(&value(&v["key_payload"])), MAX_RECORD).unwrap();
    assert_eq!((key.version, key.root_generation), (1, 1));
    let seed: [u8; 32] = unb64(&key.seed).unwrap().try_into().unwrap();
    assert_eq!(hpke_public(&seed).to_vec(), bytes(&v["public_key_hex"]));
    let sealed = unb64(text(&v["envelope"])).unwrap();
    assert_eq!(sealed, bytes(&v["envelope_hex"]));
    let envelope: HpkeEnvelope = decode(&sealed, MAX_RECORD).unwrap();
    let context = letter_context(
        text(&v["sender"]),
        text(&v["recipient"]),
        text(&v["order_id"]),
        version,
    );
    assert_eq!(canonical(&context), bytes(&v["context"]["hex"]));
    let plain = hpke_open(&seed, &envelope, &context).unwrap();
    assert_eq!(*plain, bytes(&v["plaintext_hex"]));
    let body: LetterBody = decode(&plain, 16_384).unwrap();
    assert_eq!(canonical(&body), *plain);
}

fn aborted(recipient: &str, cursor: u64) -> Json {
    json!({"cursor": cursor, "value": {
        "order_id": format!("{cursor:064x}"), "recipient": recipient,
        "sender": "sender", "state": "aborted", "created_at": cursor,
        "read": false, "archived": false, "abort_reason": "expired"
    }})
}

/// A locally signed IC certificate, device command and encrypted letter. No
/// production account, service or certificate-verification bypass is used.
fn incoming(recipient: &str, public: &[u8; 32]) -> (Trust, Json) {
    use crate::cloud::EvidenceEntry;
    use crate::testing::{account, device, security};
    use ic_certification_v3::{Certificate, fork, labeled, leaf};
    use std::collections::BTreeMap;

    let now = now_ms();
    let mut sender = account();
    sender.id = AccountId([2; 12]);
    sender.issuer = format!("https://dmsg.test/u/{}", sender.id);
    let (device_id, device) = device(1, None);
    let mut security = security(3, BTreeMap::from([(device_id, device)]));
    security.snapshot.account_id = sender.id;
    security.snapshot.issuer = sender.issuer.clone();
    security.snapshot.devices_root = digest("dmsg/devices/v1", &security.devices);
    let value = canonical(&security.snapshot);
    let witness = labeled(sender.id.as_slice(), leaf(value.clone()));
    let mut nanos = now * 1_000_000;
    let mut time = Vec::new();
    loop {
        let byte = (nanos & 127) as u8;
        nanos >>= 7;
        time.push(byte | if nanos > 0 { 128 } else { 0 });
        if nanos == 0 {
            break;
        }
    }
    let tree = fork(
        labeled(
            b"canister",
            labeled(
                sender.home.as_slice(),
                labeled(b"certified_data", leaf(witness.digest())),
            ),
        ),
        labeled(b"time", leaf(time)),
    );
    let key = ic_verify_bls_signature::PrivateKey::deserialize(&[1; 32]).unwrap();
    let signature = key.sign(&[b"\x0dic-state-root".as_slice(), &tree.digest()].concat());
    let certificate = Certificate {
        tree,
        signature: signature.serialize().to_vec(),
        delegation: None,
    };
    let mut root =
        hex::decode("308182301d060d2b0601040182dc7c0503010201060c2b0601040182dc7c05030201036100")
            .unwrap();
    root.extend(key.public_key().serialize());
    let certificate = [vec![0xd9, 0xd9, 0xf7], canonical(&certificate)].concat();
    let evidence = Evidence {
        schema: 1,
        canister: sender.home.to_text(),
        certificate: b64(&certificate),
        entries: vec![EvidenceEntry {
            account_id: sender.id.to_string(),
            value: b64(&value),
            witness: b64(&canonical(&witness)),
            devices: b64(&canonical(&security.devices)),
        }],
    };
    let trust = Trust::new(vec![sender.home], "https://dmsg.test/u/".into(), root);
    trust
        .account(&evidence, &sender.id, now, now)
        .expect("valid test certificate");
    let order = "ab".repeat(32);
    let body = LetterBody {
        text: "A verified letter · 已验证来信".into(),
    };
    let envelope = hpke_seal(
        public,
        &canonical(&body),
        &letter_context(&sender.id.to_string(), recipient, &order, 1),
    )
    .unwrap();
    let payload = ContactPayload {
        recipient: recipient.into(),
        order_id: order,
        inbox_key_version: 1,
        ciphertext: b64(&canonical(&envelope)),
        invitation: None,
        payer: None,
    };
    let signer = crate::cloud::Signer {
        issuer: &sender.issuer,
        device_id,
        security_epoch: 3,
        seed: &[1; 32],
        expires_at: now + 60_000,
    };
    let cose = sign_command(&signer, CONTACT, &payload, &"cd".repeat(32), now + 45_000);
    trust
        .command::<ContactPayload>(&cose, &evidence, now, Some(CONTACT), now)
        .expect("valid test contact");
    let row = json!({"cursor":1,"value":{"order_id":payload.order_id,"recipient":recipient,
        "sender":sender.id.to_string(),"state":"visible","created_at":now,"read":false,"archived":false,
        "ciphertext":payload.ciphertext,"signed":{"cose_sign1":cose},"evidence":evidence}});
    (trust, row)
}

#[test]
fn missing_inbox_key_preserves_the_page_for_retry_after_sync() {
    let dir = crate::testing::TempDir::new();
    let mut c = crate::testing::unlocked(&dir.0, false, None);
    let me = c.account().unwrap().id.to_string();
    let seed = [7; 32];
    let (trust, row) = incoming(&me, &hpke_public(&seed));
    let page = json!({"entries":[row],"next_cursor":1});
    assert_eq!(
        c.inbox_page_settle(0, &page, &trust),
        Err(Error::RecoveryIncomplete)
    );
    assert_eq!(c.workspace.store.meta::<u64>(CURSOR).unwrap(), None);
    assert!(c.inbox_cached().unwrap().is_empty());
    let key = InboxKey {
        format: "dmsg-inbox-key/1".into(),
        version: 1,
        root_generation: 1,
        seed: b64(&seed),
    };
    c.write(
        INBOX_KEY,
        key_object(&me, 1),
        None,
        false,
        &canonical(&key),
        None,
    )
    .unwrap();
    assert_eq!(c.inbox_page_settle(0, &page, &trust).unwrap(), Some(1));
    let letters = c.inbox_cached().unwrap();
    assert_eq!(letters.len(), 1);
    assert_eq!(letters[0].text, "A verified letter · 已验证来信");
    let mut changed = page.clone();
    changed["entries"][0]["cursor"] = json!(2);
    changed["entries"][0]["value"]["read"] = json!(true);
    changed["entries"][0]["value"]["archived"] = json!(true);
    changed["next_cursor"] = json!(2);
    c.inbox_page_settle(1, &changed, &trust).unwrap();
    let saved = c.inbox_cached().unwrap();
    assert!(saved[0].read && saved[0].archived);
    assert_eq!(saved[0].text, letters[0].text);
}

#[test]
fn tampered_visible_letters_do_not_commit_a_page_or_skip_valid_letters() {
    let dir = crate::testing::TempDir::new();
    let mut c = crate::testing::unlocked(&dir.0, false, None);
    let me = c.account().unwrap().id.to_string();
    let public = c.inbox_key(1).unwrap();
    let (trust, mut row) = incoming(&me, &public);
    row["cursor"] = json!(2);
    let original = row.clone();
    row["value"]["ciphertext"] = json!("tampered");
    let page = json!({"entries":[aborted(&me, 1), row], "next_cursor":2});
    assert_eq!(
        c.inbox_page_settle(0, &page, &trust),
        Err(Error::IntegrityFailed)
    );
    assert!(c.inbox_cached().unwrap().is_empty());
    assert_eq!(c.workspace.store.meta::<u64>(CURSOR).unwrap(), None);
    let page = json!({"entries":[aborted(&me, 1), original], "next_cursor":2});
    c.inbox_page_settle(0, &page, &trust).unwrap();
    assert_eq!(c.inbox_cached().unwrap().len(), 2);
}

#[tokio::test]
async fn read_archive_and_restore_acknowledgements_update_the_cached_letter() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let dir = crate::testing::TempDir::new();
    let mut c = crate::testing::unlocked(&dir.0, false, None);
    let me = c.account().unwrap().id.to_string();
    let public = c.inbox_key(1).unwrap();
    let (trust, row) = incoming(&me, &public);
    c.inbox_page_settle(0, &json!({"entries":[row],"next_cursor":1}), &trust)
        .unwrap();
    let letter = c.inbox_cached().unwrap().remove(0);
    let replies: Vec<_> = [(true, false), (true, true), (true, false), (true, false)]
        .into_iter()
        .map(|(read, archived)| {
            (
                "POST",
                format!("/v1/inboxes/{me}/mark"),
                Some(json!({"ok":true,"data":{
                    "order_id":letter.order_id,"read":read,"archived":archived
                }})),
            )
        })
        .collect();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    crate::testing::relay_to(
        &mut c,
        &format!("http://{}", listener.local_addr().unwrap()),
    );
    crate::testing::warm(&mut c);
    let server = tokio::spawn(async move {
        for (method, path, reply) in replies {
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
            assert_eq!(
                String::from_utf8_lossy(&bytes).lines().next().unwrap(),
                format!("{method} {path} HTTP/1.1")
            );
            let body = reply.unwrap().to_string();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    for (read, archived) in [(true, false), (true, true), (true, false)] {
        mark_contact(&c.hold(), &letter.order_id, read, archived, false)
            .await
            .unwrap();
        let saved = c.inbox_cached().unwrap().remove(0);
        assert_eq!((saved.read, saved.archived), (read, archived));
        assert_eq!(saved.text, letter.text);
        let job: Operation = c
            .inbox_job(&format!("mark:{}:{read}:{archived}:false", letter.order_id))
            .unwrap()
            .unwrap();
        assert_eq!(job.payload["reopen_contact"], false);
    }
    mark_contact(&c.hold(), &letter.order_id, true, false, true)
        .await
        .unwrap();
    let reopened: Operation = c
        .inbox_job(&format!("mark:{}:true:false:true", letter.order_id))
        .unwrap()
        .unwrap();
    assert_eq!(reopened.payload["reopen_contact"], true);
    server.await.unwrap();
}

#[test]
fn inbox_pages_advance_past_full_and_filtered_pages_and_commit_atomically() {
    let dir = crate::testing::TempDir::new();
    let mut c = crate::testing::unlocked(&dir.0, false, None);
    let me = c.account().unwrap().id.to_string();
    let trust = Trust::new(vec![], String::new(), vec![]);
    let first = json!({"entries": (1..=20).map(|i| aborted(&me, i)).collect::<Vec<_>>(), "next_cursor": 20});
    assert_eq!(c.inbox_page_settle(0, &first, &trust).unwrap(), Some(20));
    // Twenty quoted/staged orders can be filtered out without ending the list.
    assert_eq!(
        c.inbox_page_settle(20, &json!({"entries":[],"next_cursor":40}), &trust)
            .unwrap(),
        Some(40)
    );
    assert_eq!(
        c.inbox_page_settle(
            40,
            &json!({"entries":[aborted(&me,41)],"next_cursor":41}),
            &trust
        )
        .unwrap(),
        Some(41)
    );
    assert_eq!(
        c.inbox_page_settle(41, &json!({"entries":[],"next_cursor":41}), &trust)
            .unwrap(),
        None
    );
    assert_eq!(c.inbox_cached().unwrap().len(), 21);
    // A bad later row cannot commit earlier rows or advance the cursor.
    let bad = json!({"entries":[aborted(&me,42),aborted("wrong-recipient",43)],"next_cursor":43});
    assert_eq!(c.inbox_page_settle(41, &bad, &trust), Err(Error::Forbidden));
    assert_eq!(c.workspace.store.meta::<u64>(CURSOR).unwrap(), Some(41));
    assert_eq!(c.inbox_cached().unwrap().len(), 21);
    assert_eq!(
        c.inbox_page_settle(41, &json!({"entries":[],"next_cursor":40}), &trust),
        Err(Error::IntegrityFailed)
    );
    // Another reader cannot overwrite a newer cursor with an old page.
    assert_eq!(
        c.inbox_page_settle(0, &first, &trust),
        Err(Error::VersionConflict)
    );
    c.lock();
    assert_eq!(c.inbox_cached().unwrap_err(), Error::Locked);
    assert_eq!(c.inbox_cached_policy().unwrap_err(), Error::Locked);
    assert_eq!(c.inbox_outgoing().unwrap_err(), Error::Locked);
}

#[tokio::test]
async fn cached_inbox_survives_reopen_and_a_refresh_only_reads_new_pages() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let dir = crate::testing::TempDir::new();
    let mut c = crate::testing::unlocked(&dir.0, false, None);
    let me = c.account().unwrap().id.to_string();
    let server = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    crate::testing::relay_to(&mut c, &format!("http://{}", server.local_addr().unwrap()));
    crate::testing::warm(&mut c);
    c.anonymous = Some(crate::ic::Ic::anonymous(&c.config).await.unwrap());
    let serve = tokio::spawn(async move {
        let pages = [
            (
                0,
                json!({"entries":(1..=20).map(|i|aborted(&me,i)).collect::<Vec<_>>(),"next_cursor":20}),
            ),
            (20, json!({"entries":[],"next_cursor":40})),
            (40, json!({"entries":[aborted(&me,41)],"next_cursor":41})),
            (41, json!({"entries":[],"next_cursor":41})),
            // The second refresh has no old pages to download.
            (41, json!({"entries":[],"next_cursor":41})),
        ];
        for (after, body) in pages {
            let (mut socket, _) = server.accept().await.unwrap();
            let mut request = vec![0; 16384];
            let n = socket.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..n]);
            assert!(
                request.starts_with(&format!("GET /v1/inboxes/{me}?after={after}&limit=20 ")),
                "{request}"
            );
            let body = json!({"ok":true,"data":body}).to_string();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
        }
    });
    let slice = sync(&c.hold(), 1).await.unwrap();
    assert!(slice.more);
    assert_eq!(slice.cursor, 20);
    assert_eq!(slice.changed.len(), 20);
    assert_eq!(c.inbox_cached().unwrap().len(), 20);
    assert_eq!(list(&c.hold()).await.unwrap().len(), 21);
    assert_eq!(list(&c.hold()).await.unwrap().len(), 21);
    serve.await.unwrap();
    drop(c);
    let mut c = Client::open(crate::testing::config(false), &dir.0, None).unwrap();
    c.keys = Some(c.workspace.unlock(&crate::testing::SECRET).unwrap());
    assert_eq!(c.inbox_cached().unwrap().len(), 21);
}

#[tokio::test]
async fn a_locked_inbox_read_does_not_start_network_work() {
    let dir = crate::testing::TempDir::new();
    let mut c = crate::testing::unlocked(&dir.0, false, None);
    c.lock();
    assert_eq!(list(&c.hold()).await.unwrap_err(), Error::Locked);
    assert_eq!(policy(&c.hold()).await.unwrap_err(), Error::Locked);
}

#[test]
fn a_sent_letter_keeps_its_text_for_its_sender() {
    #[derive(Serialize)]
    struct Before {
        recipient: String,
        text_digest: String,
        payload: ContactPayload,
    }
    let dir = crate::testing::TempDir::new();
    let c = crate::testing::unlocked(&dir.0, false, None);
    let payload = ContactPayload {
        recipient: "them".into(),
        order_id: "aa".repeat(32),
        inbox_key_version: 1,
        ciphertext: "sealed".into(),
        invitation: None,
        payer: None,
    };
    c.save_inbox_job(
        &format!("outgoing:{}", "aa".repeat(32)),
        &Outgoing {
            recipient: "them".into(),
            text_digest: "digest".into(),
            payload: payload.clone(),
            text: "see you soon".into(),
            sent_at: 7,
        },
    )
    .unwrap();
    // A letter from before the text was kept still reads, without one.
    c.save_inbox_job(
        &format!("outgoing:{}", "bb".repeat(32)),
        &Before {
            recipient: "them".into(),
            text_digest: "digest".into(),
            payload,
        },
    )
    .unwrap();
    let sent = c.inbox_outgoing().unwrap();
    assert_eq!(sent[0]["text"], "see you soon");
    assert_eq!(sent[0]["sent_at"], 7);
    assert_eq!(sent[1]["text"], "");
    assert_eq!(sent[1]["sent_at"], 0);
}
