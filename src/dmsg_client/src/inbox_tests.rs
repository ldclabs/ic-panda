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
