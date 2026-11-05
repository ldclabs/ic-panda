//! The channel byte rules against the vectors the extension exported:
//! envelopes, key confirmation, epoch manifest, message keys and AAD,
//! history envelopes and attachments.

use super::*;
use crate::codec::decode_value;
use crate::crypto::hpke_open;
use crate::vectors::{bytes, field, key, load, text, value};

#[test]
fn notifications_silence_machine_wallet_hints_but_preserve_human_handovers() {
    let wallet = json!({"format":"dmsg-wallet-event/1"});
    assert!(background_packet("", false, Some(&wallet)));
    assert!(!background_packet("Please review", false, Some(&wallet)));
    assert!(!background_packet("", true, Some(&wallet)));
    for format in ["dmsg-channel-owner-transfer/1", "dmsg-channel-sponsor/1"] {
        assert!(!background_packet(
            "",
            false,
            Some(&json!({"format":format}))
        ));
    }
}

#[tokio::test]
async fn ordinary_attachment_import_rejects_a_cached_secret_package_without_network() {
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    struct Refreshed {
        client: Mutex<Client>,
        head: Json,
        reads: AtomicUsize,
    }
    impl Hold for Refreshed {
        async fn with<T: Send>(
            &self,
            step: impl FnOnce(&mut Client) -> Result<T> + Send,
        ) -> Result<T> {
            step(&mut self.client.lock().unwrap())
        }
        async fn trust(&self) -> Result<Trust> {
            Ok(Trust::new(vec![], String::new(), vec![]))
        }
        async fn send(
            &self,
            mut sign: impl FnMut(&mut Client) -> Result<Prepared> + Send,
        ) -> Result<Json> {
            self.with(|c| sign(c)).await?;
            assert_eq!(
                self.reads.fetch_add(1, Ordering::SeqCst),
                0,
                "must not fetch the package"
            );
            Ok(self.head.clone())
        }
    }
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    let (channel, ledger) = active_channel(&mut client);
    let message = Message {
        seq: 1,
        file: Some(FileRef {
            upload_id: "a1".repeat(32),
            file_id: "a2".repeat(32),
            version: "a3".repeat(32),
            epoch: 1,
            manifest_digest: "a4".repeat(32),
            name: "looks-like-a-document.bin".into(),
            size: 100,
            mime: crate::secrets::package::MIME.into(),
            key: b64(&[9; 32]),
        }),
        ..Message::default()
    };
    client
        .workspace
        .put_private(
            client.keys().unwrap(),
            &format!("{INBOX}{channel}:{:020}", 1),
            &message,
        )
        .unwrap();
    let h = Refreshed {
        client: Mutex::new(client),
        head: json(&ledger),
        reads: AtomicUsize::new(0),
    };
    assert!(matches!(
        save_regular_shared(&h, &channel, 1, "root", &mut |_, _| {}).await,
        Err(Error::UnsupportedProtocol)
    ));
    assert_eq!(h.reads.load(Ordering::SeqCst), 0);
    assert!(
        h.client
            .lock()
            .unwrap()
            .files_list("root", false)
            .unwrap()
            .files
            .is_empty()
    );
}

#[test]
fn only_empty_unactivated_legacy_drafts_can_be_deleted() {
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    let id = "c1".repeat(32);
    let ring = Keyring {
        format: FORMAT.into(),
        id: id.clone(),
        name: "legacy draft".into(),
        genesis: "00".repeat(32),
        keys: BTreeMap::new(),
        muted: false,
        hidden: false,
    };
    client.save_keyring(None, &ring).unwrap();
    assert_eq!(
        client.channel_delete_legacy_draft(&id),
        Err(Error::Forbidden)
    );
    client
        .save_job(
            &id,
            "legacy-draft",
            &json!({"format": "dmsg-shared-migration-draft/1"}),
        )
        .unwrap();
    assert!(client.channel_view(&id).unwrap().deletable_legacy_draft);
    client
        .save_job(&id, "legacy-activate", &json!({"pending": true}))
        .unwrap();
    assert_eq!(
        client.channel_delete_legacy_draft(&id),
        Err(Error::Forbidden)
    );
    client.forget_job(&id, "legacy-activate").unwrap();
    client
        .save_local(
            &id,
            &Local {
                message: 1,
                ..Local::default()
            },
        )
        .unwrap();
    assert_eq!(
        client.channel_delete_legacy_draft(&id),
        Err(Error::Forbidden)
    );
    client.save_local(&id, &Local::default()).unwrap();
    client.channel_delete_legacy_draft(&id).unwrap();
    assert!(client.channel_list().unwrap().is_empty());
    assert_eq!(client.channel_view(&id).err(), Some(Error::NotFound));
    assert!(client.current(&id).unwrap().1.tombstone);
    assert!(client.job::<Json>(&id, "legacy-draft").unwrap().is_none());
}

#[test]
fn notifications_timed_channel_mute_preserves_unread_and_never_replays_muted_arrivals() {
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    let channel = "d1".repeat(32);
    client
        .save_keyring(
            None,
            &Keyring {
                format: FORMAT.into(),
                id: channel.clone(),
                name: "private channel".into(),
                genesis: "00".repeat(32),
                keys: BTreeMap::new(),
                muted: false,
                hidden: false,
            },
        )
        .unwrap();
    let local = Local {
        notification_seq: 1,
        message: 1,
        unread: 1,
        ..Default::default()
    };
    client.save_local(&channel, &local).unwrap();
    let now = now_ms();
    let until = now + 60000;
    let view = client
        .channel_notification_mute(&channel, Some(until))
        .unwrap();
    assert!(view.muted && view.notification_pending);
    assert_eq!(view.unread, 1);
    assert_eq!(view.muted_until, Some(until));
    let item = client
        .workspace
        .store
        .notification_item(&crate::notifications::item_id(
            &client.me().unwrap(),
            &crate::notifications::Target::Channel(channel),
        ))
        .unwrap()
        .unwrap();
    assert_eq!(item.notified, item.version);
    assert_eq!(item.observed, 0);
    assert!(!item.is_muted(until + 1));
    assert_eq!(crate::notifications::counts(&[item], until + 1).unread, 1);
}

#[test]
fn epoch_envelopes_confirmation_and_manifest() {
    let v = load("channel-v1.json");
    let channel = text(&v["channel"]);
    let member_seed = key(&v["member"]["hpke_seed_hex"]);
    let epoch_key = key(&v["epoch_key_hex"]);
    let activation: Activation = serde_json::from_value(v["activation"].clone()).unwrap();
    activation.validate().unwrap();
    let upload = &v["rotation_uploads"][0];
    for (i, reference) in activation.envelopes.iter().enumerate() {
        let context = envelope_context(channel, &activation, &reference.recipient);
        assert_eq!(
            canonical(&context),
            bytes(&v["envelope_contexts"][i]["hex"])
        );
        let chunk = unb64(text(&upload["chunks"][i])).unwrap();
        assert_eq!(hex_sha256(&chunk), reference.digest);
        assert_eq!(chunk.len() as u64, reference.size);
        let envelope: HpkeEnvelope = decode(&chunk, MAX_RECORD).unwrap();
        if reference.recipient.contains(text(&v["member"]["account"])) {
            assert_eq!(
                *hpke_open(&member_seed, &envelope, &context).unwrap(),
                epoch_key
            );
        }
    }
    assert_eq!(
        key_confirmation(channel, activation.epoch, &epoch_key),
        activation.key_confirmation
    );
    let manifest = EpochManifest {
        format: "dmsg-channel-epoch/1".into(),
        channel: channel.into(),
        epoch: activation.epoch,
        lease_id: activation.lease_id.clone(),
        head: activation.expected_head.clone(),
        fencing: activation.fencing,
        recipients_digest: activation.recipients_digest.clone(),
        key_confirmation: activation.key_confirmation.clone(),
        group: 0,
    };
    assert_eq!(b64(&canonical(&manifest)), text(&upload["manifest"]));
    assert_eq!(
        hex_sha256(&canonical(&manifest)),
        text(&upload["plan"]["manifest_digest"])
    );
}

#[test]
fn messages_open_with_their_derived_key() {
    let v = load("channel-v1.json");
    let channel = text(&v["channel"]);
    let epoch_key = key(&v["epoch_key_hex"]);
    let m = &v["message"];
    let device = text(&m["device_id"]);
    let payload: MessagePayload = serde_json::from_value(m["payload"].clone()).unwrap();
    let message_key = message_key(
        &epoch_key,
        channel,
        payload.epoch,
        &payload.message_id,
        device,
    );
    assert_eq!(*message_key, key(&m["key_hex"]));
    let aad = message_aad(text(&m["sender"]), device, &payload);
    assert_eq!(canonical(&aad), bytes(&m["aad"]["hex"]));
    let plaintext = open(&message_key, &payload.ciphertext, &aad).unwrap();
    assert_eq!(*plaintext, bytes(&m["plaintext_hex"]));
    let body: Body = decode(&plaintext, MAX_RECORD).unwrap();
    assert_eq!(canonical(&body), canonical(&value(&m["plaintext"])));
}

#[test]
fn history_envelopes_carry_epoch_keys() {
    let v = load("channel-v1.json");
    let h = &v["history"];
    let scope: HistoryScope = serde_json::from_value(h["scope"].clone()).unwrap();
    let recipient = format!(
        "device:{}:{}",
        text(&v["member"]["account"]),
        text(&v["member"]["device_id"])
    );
    let context = history_context(&scope, &recipient, 1, 1);
    assert_eq!(canonical(&context), bytes(&h["context"]["hex"]));
    let chunk = unb64(text(&h["uploads"][0]["chunks"][0])).unwrap();
    let envelope: HpkeEnvelope = decode(&chunk, MAX_RECORD).unwrap();
    let plaintext = hpke_open(&key(&v["member"]["hpke_seed_hex"]), &envelope, &context).unwrap();
    let keys: Vec<(u64, ByteBuf)> = decode(&plaintext, 65_536).unwrap();
    assert_eq!(
        keys,
        vec![(1, ByteBuf::from(key(&v["epoch_key_hex"]).to_vec()))]
    );
    let manifest: HistoryManifest =
        decode(&unb64(text(&h["uploads"][0]["manifest"])).unwrap(), 65_536).unwrap();
    assert_eq!(manifest.format, "dmsg-channel-history/1");
    assert_eq!(manifest.scope, scope);
}

#[test]
fn attachments_carry_the_file_key() {
    let v = load("channel-v1.json");
    let channel = text(&v["channel"]);
    let a = &v["attachment"];
    let device = text(&v["owner"]["device_id"]);
    let payload: MessagePayload = serde_json::from_value(a["payload"].clone()).unwrap();
    let message_key = message_key(
        &key(&v["epoch_key_hex"]),
        channel,
        payload.epoch,
        &payload.message_id,
        device,
    );
    assert_eq!(*message_key, key(&a["key_hex"]));
    let aad = message_aad(text(&v["owner"]["account"]), device, &payload);
    assert_eq!(canonical(&aad), bytes(&a["aad"]["hex"]));
    let plaintext = open(&message_key, &payload.ciphertext, &aad).unwrap();
    assert_eq!(*plaintext, bytes(&a["plaintext_hex"]));
    let body: Body = decode(&plaintext, MAX_RECORD).unwrap();
    let reference = body.file.unwrap();
    let context = manifest_context(
        channel,
        reference.epoch,
        &reference.upload_id,
        &reference.file_id,
        &reference.version,
    );
    assert_eq!(canonical(&context), bytes(&a["manifest_context"]["hex"]));
    let manifest = text(&a["upload"]["manifest"]);
    assert_eq!(
        hex_sha256(&unb64(manifest).unwrap()),
        text(&a["upload"]["plan"]["manifest_digest"])
    );
    assert_eq!(
        reference.manifest_digest,
        text(&a["upload"]["plan"]["manifest_digest"])
    );
    let opened = open(
        &key32(&unb64(&reference.key).unwrap()).unwrap(),
        manifest,
        &context,
    )
    .unwrap();
    assert_eq!(*opened, bytes(&a["manifest_plaintext_hex"]));
    let shared = decode_value(&opened, MAX_RECORD).unwrap();
    assert_eq!(
        field(&shared, "format").as_text(),
        Some("dmsg-channel-file/1")
    );
    assert_eq!(field(field(&shared, "file"), "key").as_text(), Some(""));
}

#[test]
fn messages_page_by_sequence_and_uploads_are_held_until_they_lapse() {
    use crate::testing::{TempDir, unlocked};

    let dir = TempDir::new();
    let client = unlocked(&dir.0, false, None);
    let channel = "c".repeat(64);
    let message = |seq: u64| Message {
        seq,
        account: "a".into(),
        device: "d".into(),
        message_id: format!("{seq:064}"),
        epoch: 1,
        created_at: seq,
        stored_at: seq,
        text: format!("text {seq}"),
        ..Message::default()
    };
    for seq in [1, 2, 3, 10, 11] {
        client
            .workspace
            .put_private(
                client.keys().unwrap(),
                &format!("{INBOX}{channel}:{seq:020}"),
                &message(seq),
            )
            .unwrap();
    }
    // Another channel's messages are not on the page.
    client
        .workspace
        .put_private(
            client.keys().unwrap(),
            &format!("{INBOX}{}:{:020}", "d".repeat(64), 4),
            &message(4),
        )
        .unwrap();
    let seqs = |after, limit| {
        client
            .channel_messages(&channel, after, limit)
            .unwrap()
            .iter()
            .map(|m| m.seq)
            .collect::<Vec<_>>()
    };
    assert_eq!(seqs(0, 10), [1, 2, 3, 10, 11]);
    assert_eq!(seqs(2, 2), [3, 10]);
    assert_eq!(seqs(9, 10), [10, 11]);
    assert!(seqs(11, 10).is_empty());

    let plan = |id: &str, expires_at: u64, digest: &str| UploadPlan {
        management: None,
        upload_id: id.into(),
        object_id: channel.clone(),
        version_id: id.into(),
        root_generation: 0,
        epoch: 1,
        control_head: None,
        kind: "envelopes".into(),
        chunks: vec![ChunkRef {
            digest: digest.into(),
            size: 1,
        }],
        manifest_digest: String::new(),
        manifest_size: 0,
        expires_at,
    };
    client
        .hold_uploads([&plan("u1", 100, "x"), &plan("u2", 200, "y")])
        .unwrap();
    assert_eq!(
        client.held_chunks(50).unwrap(),
        BTreeSet::from(["x".to_string(), "y".to_string()])
    );
    assert_eq!(
        client.held_chunks(150).unwrap(),
        BTreeSet::from(["y".to_string()])
    );
    // The lapsed upload is forgotten for good.
    assert_eq!(
        client.held_chunks(50).unwrap(),
        BTreeSet::from(["y".to_string()])
    );
}

/// A message of `account` stored at `seq` saying `text`.
fn written(seq: u64, account: &str, text: &str) -> Message {
    Message {
        seq,
        account: account.into(),
        device: "d".repeat(64),
        message_id: format!("{seq:064}"),
        epoch: 1,
        created_at: seq,
        stored_at: seq,
        text: text.into(),
        digest: format!("{seq:064x}"),
        ..Message::default()
    }
}

fn keep(client: &Client, channel: &str, local: &mut Local, message: &Message) {
    client
        .workspace
        .put_private(
            client.keys().unwrap(),
            &format!("{INBOX}{channel}:{:020}", message.seq),
            message,
        )
        .unwrap();
    client.amend(channel, local, message).unwrap();
}

fn kept(client: &Client, channel: &str, seq: u64) -> Message {
    client
        .workspace
        .private(
            client.keys().unwrap(),
            &format!("{INBOX}{channel}:{seq:020}"),
        )
        .unwrap()
        .unwrap()
}

fn reference(message: &Message) -> MessageRef {
    MessageRef {
        seq: message.seq,
        digest: message.digest.clone(),
    }
}

#[test]
fn attachment_search_pages_preserve_all_matches_and_reply_targets() {
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    let channel = "ab".repeat(32);
    crate::testing::joined_channel(&mut client, &channel);
    let me = client.me().unwrap();
    let mut local = Local::default();
    for seq in 1..=105 {
        let message = Message {
            file: Some(FileRef {
                upload_id: "a".repeat(64),
                file_id: "file".into(),
                version: "version".into(),
                epoch: 1,
                manifest_digest: "b".repeat(64),
                name: "project-plan.pdf".into(),
                size: 1,
                mime: "application/pdf".into(),
                key: "key".into(),
            }),
            ..written(seq, &me, "No keyword in the body")
        };
        keep(&client, &channel, &mut local, &message);
    }
    let first = client
        .channel_search_before(&channel, " plan.PDF ", u64::MAX, 100)
        .unwrap();
    assert_eq!(first.len(), 100);
    let second = client
        .channel_search_before(&channel, "plan.pdf", first.last().unwrap().seq, 100)
        .unwrap();
    assert_eq!(
        second.iter().map(|m| m.seq).collect::<Vec<_>>(),
        [5, 4, 3, 2, 1]
    );
    let original = kept(&client, &channel, 1);
    let path = dir.0.join("attachment.txt");
    std::fs::write(&path, "attachment").unwrap();
    let file = client
        .files_import(&path, "root", true, &mut |_, _| {})
        .unwrap();
    let intent = client
        .queue_attachment_reply(
            &channel,
            &file.file.id,
            "reply body",
            Some(reference(&original)),
        )
        .unwrap();
    assert_eq!(intent.reply_to, Some(reference(&original)));
    let wrong = MessageRef {
        seq: 1,
        digest: "ff".repeat(32),
    };
    assert!(matches!(
        client.queue_attachment_reply(&channel, &file.file.id, "reply", Some(wrong)),
        Err(Error::VersionConflict)
    ));
}

#[test]
fn edits_and_withdrawals_reach_their_authors_messages_in_any_order() {
    use crate::testing::{TempDir, unlocked};

    let dir = TempDir::new();
    let client = unlocked(&dir.0, false, None);
    let channel = "c".repeat(64);
    let me = client.me().unwrap();
    let mut local = Local::default();
    let first = written(1, &me, "first");
    keep(&client, &channel, &mut local, &first);
    let edit = Message {
        edit_of: Some(reference(&first)),
        ..written(2, &me, "first, edited")
    };
    keep(&client, &channel, &mut local, &edit);
    let shown = kept(&client, &channel, 1);
    assert_eq!(
        (shown.text.as_str(), shown.edited_at),
        ("first, edited", Some(2))
    );
    // Another account's edit, or one naming other bytes, changes nothing.
    let foreign = Message {
        edit_of: Some(reference(&first)),
        ..written(3, "other", "hijacked")
    };
    keep(&client, &channel, &mut local, &foreign);
    let forged = Message {
        edit_of: Some(MessageRef {
            seq: 1,
            digest: "0".repeat(64),
        }),
        ..written(4, &me, "forged")
    };
    keep(&client, &channel, &mut local, &forged);
    assert_eq!(kept(&client, &channel, 1).text, "first, edited");
    // A withdrawal read before its message waits for it, then drops it.
    let secret = Message {
        file: Some(FileRef {
            upload_id: "u".into(),
            file_id: "f".into(),
            version: "v".into(),
            epoch: 1,
            manifest_digest: "m".into(),
            name: "plan.pdf".into(),
            size: 1,
            mime: "application/pdf".into(),
            key: "k".into(),
        }),
        ..written(10, &me, "secret")
    };
    let withdrawal = Message {
        retract: Some(reference(&secret)),
        ..written(11, &me, "")
    };
    keep(&client, &channel, &mut local, &withdrawal);
    assert_eq!(local.amends.get(&10), Some(&vec![11]));
    keep(&client, &channel, &mut local, &secret);
    let dropped = kept(&client, &channel, 10);
    assert!(dropped.retracted && dropped.text.is_empty() && dropped.file.is_none());
    assert!(local.amends.is_empty());
    // An edit after the withdrawal brings nothing back.
    let late = Message {
        edit_of: Some(reference(&secret)),
        ..written(12, &me, "back")
    };
    keep(&client, &channel, &mut local, &late);
    assert!(kept(&client, &channel, 10).text.is_empty());
    // Pages show the messages, not their edits and withdrawals.
    let seqs = |list: Vec<Message>| list.iter().map(|m| m.seq).collect::<Vec<_>>();
    assert_eq!(
        seqs(client.channel_messages(&channel, 0, 10).unwrap()),
        [1, 10]
    );
    assert_eq!(seqs(client.channel_messages(&channel, 0, 1).unwrap()), [1]);
    assert_eq!(
        seqs(client.channel_messages_before(&channel, 13, 1).unwrap()),
        [10]
    );
    assert_eq!(
        seqs(client.channel_messages_before(&channel, 10, 5).unwrap()),
        [1]
    );
    // Search finds text and file names, never what was withdrawn.
    assert_eq!(
        seqs(client.channel_search(&channel, "EDITED", 5).unwrap()),
        [1]
    );
    assert!(
        client
            .channel_search(&channel, "secret", 5)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn drafts_name_messages_read_here_and_amend_only_their_own() {
    use crate::testing::{TempDir, unlocked};

    let dir = TempDir::new();
    let client = unlocked(&dir.0, false, None);
    let channel = "c".repeat(64);
    let me = client.me().unwrap();
    let mut local = Local::default();
    let mine = written(1, &me, "mine");
    let theirs = written(2, "other", "theirs");
    keep(&client, &channel, &mut local, &mine);
    keep(&client, &channel, &mut local, &theirs);
    let check = |draft: Draft, file: bool| client.check_draft(&channel, &me, &draft, file);
    let text = |t: &str| Draft {
        text: t.into(),
        ..Draft::default()
    };
    assert!(check(text("hello"), false).is_ok());
    assert!(matches!(
        check(text(""), false),
        Err(Error::InvalidInput(_))
    ));
    assert!(check(text(""), true).is_ok());
    assert!(
        check(
            Draft {
                reply_to: Some(reference(&theirs)),
                ..text("yes")
            },
            false
        )
        .is_ok()
    );
    assert_eq!(
        check(
            Draft {
                reply_to: Some(MessageRef {
                    seq: 9,
                    digest: "0".repeat(64)
                }),
                ..text("yes")
            },
            false
        ),
        Err(Error::NotFound)
    );
    assert_eq!(
        check(
            Draft {
                edit_of: Some(reference(&theirs)),
                ..text("mine now")
            },
            false
        ),
        Err(Error::Forbidden)
    );
    for (draft, file) in [
        (
            Draft {
                edit_of: Some(reference(&mine)),
                ..text("")
            },
            false,
        ),
        (
            Draft {
                edit_of: Some(reference(&mine)),
                ..text("with a file")
            },
            true,
        ),
        (
            Draft {
                retract: Some(reference(&mine)),
                ..text("why")
            },
            false,
        ),
        (
            Draft {
                packet: Some(
                    json!({"format": "dmsg-channel-sponsor/1", "channel": "d".repeat(64)}),
                ),
                ..text("")
            },
            false,
        ),
    ] {
        assert!(matches!(check(draft, file), Err(Error::InvalidInput(_))));
    }
    assert!(
        check(
            Draft {
                retract: Some(reference(&mine)),
                ..Draft::default()
            },
            false
        )
        .is_ok()
    );
    assert!(
        check(
            Draft {
                packet: Some(
                    json!({"format": "dmsg-channel-owner-transfer/1", "channel": channel})
                ),
                ..Draft::default()
            },
            false
        )
        .is_ok()
    );
}

#[test]
fn a_body_that_does_not_open_leaves_its_message_unreadable() {
    use crate::testing::{TempDir, unlocked};

    let dir = TempDir::new();
    let client = unlocked(&dir.0, false, None);
    let channel = "c".repeat(64);
    let me = client.me().unwrap();
    let root = [7u8; 32];
    let ring = Keyring {
        format: FORMAT.into(),
        id: channel.clone(),
        name: "c".into(),
        genesis: "0".repeat(64),
        keys: BTreeMap::from([(1, b64(&root))]),
        muted: false,
        hidden: false,
    };
    let device = "d".repeat(64);
    let message_id = "1".repeat(64);
    let mut payload = MessagePayload {
        attachment_upload: None,
        channel_id: channel.clone(),
        epoch: 1,
        control_head: "0".repeat(64),
        acl_version: 0,
        message_id: message_id.clone(),
        client_created_at: 0,
        ciphertext: b64(&[1, 2, 3]),
    };
    let message = Message {
        account: me.clone(),
        device: device.clone(),
        ..written(1, &me, "")
    };
    let opened = open_message(&channel, &ring, message.clone(), &payload);
    assert!(opened.unreadable && opened.text.is_empty());
    // A body that opens but says something malformed is unreadable too.
    let key = message_key(&root, &channel, 1, &message_id, &device);
    let header = payload.clone();
    let aad = message_aad(&me, &device, &header);
    payload.ciphertext = seal(&key, &canonical(&json!({"text": 7})), &aad);
    let opened = open_message(&channel, &ring, message.clone(), &payload);
    assert!(opened.unreadable);
    let body = Body {
        text: "hello".into(),
        file: None,
        reply_to: None,
        edit_of: None,
        retract: None,
        agent: Some(AgentRef {
            id: "did:agent:x".into(),
            name: "claude".into(),
        }),
        packet: None,
    };
    payload.ciphertext = seal(&key, &canonical(&body), &aad);
    let opened = open_message(&channel, &ring, message, &payload);
    assert!(!opened.unreadable);
    assert_eq!(opened.text, "hello");
    assert_eq!(opened.agent.unwrap().name, "claude");
}

#[tokio::test]
async fn reads_of_one_channel_settle_in_any_order() {
    use crate::channel_log::Member;
    use crate::testing::{TempDir, unlocked, warm};

    // No page here carries evidence to check.
    let trust = crate::evidence::Trust::new(vec![], String::new(), vec![]);

    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    warm(&mut client);
    let channel = "c".repeat(64);
    let me = client.me().unwrap();
    let head = "a".repeat(64);
    let genesis = Genesis {
        channel_id: channel.clone(),
        nonce: "b".repeat(64),
        kind: ChannelType::Collaboration,
    };
    // This account joined in epoch 2: the relay skips epoch 1's messages
    // for it, without their ciphertext.
    let mut ledger = start(&genesis, "owner", &head).unwrap();
    ledger.epoch = 2;
    ledger.status = Status::Active;
    ledger.members.insert(
        me.clone(),
        Member {
            account: me.clone(),
            role: Role::Member,
            joined_epoch: 2,
            can_rotate: false,
            history: vec![],
        },
    );
    client
        .save_keyring(
            None,
            &Keyring {
                format: FORMAT.into(),
                id: channel.clone(),
                name: "c".into(),
                genesis: head.clone(),
                keys: BTreeMap::new(),
                muted: false,
                hidden: false,
            },
        )
        .unwrap();
    let mut local = Local {
        control: 5,
        ledger: Some(ledger),
        ..Local::default()
    };
    client.save_local(&channel, &local).unwrap();

    // Two reads ask for the same page of the log, and the other one keeps
    // it and the controls after it first. This one asks again from there,
    // without looking at the page.
    let mut slower = Reading::new(&channel);
    client.channel_read_request(&mut slower).unwrap();
    local.control = 9;
    client.save_local(&channel, &local).unwrap();
    let page = |cursor: u64| {
        json!({
            "entries": [{"cursor": cursor, "value": {}}],
            "next_cursor": cursor,
            "head": "d".repeat(64),
        })
    };
    client
        .channel_read_settle(&mut slower, page(6), &trust)
        .unwrap();
    assert_eq!(
        client.channel_read_path(&mut slower).unwrap(),
        format!("/v1/channels/{channel}/sync?kind=control&after=9&limit=5")
    );
    // A page that does not continue the cursor it was asked against is
    // still refused.
    for back in [page(6), page(9)] {
        assert_eq!(
            client.channel_read_settle(&mut slower, back, &trust),
            Err(Error::IntegrityFailed)
        );
    }
    // A head asked for before newer controls were kept is asked for again
    // after the log.
    let mut reading = Reading {
        step: Step::Head,
        ..Reading::controls(&channel)
    };
    client.channel_read_request(&mut reading).unwrap();
    local.control = 10;
    client.save_local(&channel, &local).unwrap();
    client
        .channel_read_settle(&mut reading, json!({"head": head}), &trust)
        .unwrap();
    assert_eq!(reading.step, Step::Control);

    // Two reads of the messages: the slower one settles pages the other
    // kept, and the cursor stays where the other took it.
    let page = |after: u64, through: u64| {
        json!({
            "head": head,
            "entries": [],
            "skipped": (after + 1..=through)
                .map(|seq| json!({"seq": seq, "epoch": 1}))
                .collect::<Vec<_>>(),
            "next_seq": through,
            "latest_seq": 30,
        })
    };
    let mut faster = client.message_reading(&channel).unwrap();
    let mut slower = client.message_reading(&channel).unwrap();
    for (after, through) in [(0, 20), (20, 30)] {
        client
            .channel_read_settle(&mut faster, page(after, through), &trust)
            .unwrap();
    }
    assert!(faster.done());
    for (after, through) in [(0, 20), (20, 30)] {
        client
            .channel_read_settle(&mut slower, page(after, through), &trust)
            .unwrap();
        assert_eq!(client.local(&channel).unwrap().message, 30);
    }
    assert!(slower.done());
}

#[test]
fn a_keyring_sealed_before_muting_existed_still_opens() {
    // The shape devices sealed before `muted` and `hidden` (213f6d4e).
    #[derive(Serialize)]
    struct Older {
        format: String,
        id: String,
        name: String,
        genesis: String,
        keys: BTreeMap<u64, String>,
    }
    let older = Older {
        format: FORMAT.into(),
        id: "c".repeat(64),
        name: "c".into(),
        genesis: "0".repeat(64),
        keys: BTreeMap::from([(1, b64(&[7u8; 32]))]),
    };
    let ring: Keyring = decode(&canonical(&older), MAX_RECORD).unwrap();
    assert_eq!(
        (ring.name.as_str(), ring.muted, ring.hidden),
        ("c", false, false)
    );
}

pub(crate) fn active_channel(client: &mut Client) -> (String, Ledger) {
    crate::testing::warm(client);
    let channel = "c1".repeat(32);
    let head = "a1".repeat(32);
    let mut ledger = start(
        &Genesis {
            channel_id: channel.clone(),
            nonce: "b1".repeat(32),
            kind: ChannelType::Collaboration,
        },
        &client.me().unwrap(),
        &head,
    )
    .unwrap();
    ledger.epoch = 1;
    ledger.status = Status::Active;
    client
        .save_keyring(
            None,
            &Keyring {
                format: FORMAT.into(),
                id: channel.clone(),
                name: "test".into(),
                genesis: head.clone(),
                keys: BTreeMap::from([(1, b64(&[7; 32]))]),
                muted: false,
                hidden: false,
            },
        )
        .unwrap();
    client
        .save_local(
            &channel,
            &Local {
                ledger: Some(ledger.clone()),
                heads: BTreeMap::from([(head, 0)]),
                ..Local::default()
            },
        )
        .unwrap();
    (channel, ledger)
}

#[test]
fn unchanged_head_skips_the_log_but_changed_heads_and_pins_still_verify() {
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    let (channel, ledger) = active_channel(&mut client);
    let trust = Trust::new(vec![], String::new(), vec![]);
    let head = json(&ledger);
    let mut reading = Reading::new(&channel);
    assert_eq!(
        client.channel_read_path(&mut reading).unwrap(),
        base(&channel).unwrap()
    );
    client
        .channel_read_settle(&mut reading, head.clone(), &trust)
        .unwrap();
    assert_eq!(reading.step, Step::Messages);
    assert_eq!(
        client.channel_read_path(&mut reading).unwrap(),
        format!("{}/sync?after=0&limit=20", base(&channel).unwrap())
    );
    client
        .channel_read_settle(
            &mut reading,
            json!({
                "head": ledger.head, "entries": [], "skipped": [], "next_seq": 0, "latest_seq": 0,
            }),
            &trust,
        )
        .unwrap();
    assert!(reading.done()); // Two requests, no empty control page.

    let mut controls = Reading::controls(&channel);
    client.channel_read_request(&mut controls).unwrap();
    client
        .channel_read_settle(&mut controls, head.clone(), &trust)
        .unwrap();
    assert!(
        controls.done(),
        "background control maintenance must not fetch message bodies"
    );

    let mut reading = Reading::controls(&channel);
    client.channel_read_request(&mut reading).unwrap();
    let mut newer = head.clone();
    newer["head"] = "d1".repeat(32).into();
    newer["control_seq"] = (ledger.control_seq + 1).into();
    client
        .channel_read_settle(&mut reading, newer.clone(), &trust)
        .unwrap();
    assert_eq!(reading.step, Step::Control);
    // A missing control cannot be replaced by the relay's unverified head.
    assert_eq!(
        client.channel_read_settle(
            &mut reading,
            json!({
                "head": newer["head"], "entries": [], "next_cursor": 0,
            }),
            &trust
        ),
        Err(Error::IntegrityFailed)
    );

    for field in ["head", "epoch", "acl_version"] {
        let mut reading = Reading::controls(&channel);
        client.channel_read_request(&mut reading).unwrap();
        let mut forged = head.clone();
        forged[field] = if field == "head" {
            Json::String("d1".repeat(32))
        } else {
            99.into()
        };
        assert_eq!(
            client.channel_read_settle(&mut reading, forged, &trust),
            Err(Error::IntegrityFailed)
        );
    }
    let mut reading = Reading {
        pinned: Some("d1".repeat(32)),
        ..Reading::controls(&channel)
    };
    client.channel_read_request(&mut reading).unwrap();
    assert_eq!(
        client.channel_read_settle(&mut reading, head, &trust),
        Err(Error::IntegrityFailed)
    );
}

fn outgoing_receipt(
    client: &Client,
    channel: &str,
    ledger: &Ledger,
    draft: Draft,
    id: &str,
    seq: u64,
) -> Json {
    let outgoing = client.seal_message(channel, ledger, &draft, id).unwrap();
    let operation = client
        .sign_now(
            MESSAGE,
            format!("{}/messages", base(channel).unwrap()),
            json(&outgoing.payload),
            random_hex(),
        )
        .unwrap();
    let receipt = json!({"seq": seq, "stored_at": now_ms(), "digest": hex_sha256(&unb64(&operation.cose).unwrap())});
    client
        .save_job(channel, &format!("send:{id}"), &operation)
        .unwrap();
    receipt
}

#[test]
fn durable_ack_shows_the_message_without_skipping_gaps_or_losing_edits() {
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    let (channel, ledger) = active_channel(&mut client);
    let id = "e1".repeat(32);
    let receipt = outgoing_receipt(
        &client,
        &channel,
        &ledger,
        Draft {
            text: "hello".into(),
            ..Draft::default()
        },
        &id,
        3,
    );
    let mut forged = receipt.clone();
    forged["digest"] = "ff".repeat(32).into();
    assert_eq!(
        client.channel_send_settle(&channel, &id, Ok(forged)),
        Err(Error::IntegrityFailed)
    );
    assert_eq!(client.channel_outbox(&channel).unwrap().len(), 1);
    assert!(
        client
            .channel_messages_before(&channel, u64::MAX, 10)
            .unwrap()
            .is_empty()
    );
    client
        .channel_send_settle(&channel, &id, Ok(receipt))
        .unwrap();
    assert!(client.channel_outbox(&channel).unwrap().is_empty());
    let messages = client
        .channel_messages_before(&channel, u64::MAX, 10)
        .unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].text, "hello");
    assert_eq!(client.local(&channel).unwrap().message, 0);
    assert_eq!(client.channel_mark(&channel, 3).unwrap().read, 0);

    let id = "e2".repeat(32);
    let receipt = outgoing_receipt(
        &client,
        &channel,
        &ledger,
        Draft {
            text: "edited".into(),
            edit_of: Some(reference(&messages[0])),
            ..Draft::default()
        },
        &id,
        4,
    );
    client
        .channel_send_settle(&channel, &id, Ok(receipt))
        .unwrap();
    assert_eq!(kept(&client, &channel, 3).text, "edited");
    assert_eq!(client.local(&channel).unwrap().last.unwrap().text, "edited");
    let id = "e3".repeat(32);
    let receipt = outgoing_receipt(
        &client,
        &channel,
        &ledger,
        Draft {
            retract: Some(reference(&messages[0])),
            ..Draft::default()
        },
        &id,
        5,
    );
    client
        .channel_send_settle(&channel, &id, Ok(receipt))
        .unwrap();
    assert!(kept(&client, &channel, 3).retracted);
    assert_eq!(client.local(&channel).unwrap().message, 0);
}

#[test]
fn ack_keeps_a_message_already_read_and_does_not_replace_a_newer_preview() {
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    let (channel, ledger) = active_channel(&mut client);
    let id = "e1".repeat(32);
    let receipt = outgoing_receipt(
        &client,
        &channel,
        &ledger,
        Draft {
            text: "first".into(),
            ..Draft::default()
        },
        &id,
        1,
    );
    let mut already_read = written(1, &client.me().unwrap(), "edited elsewhere");
    already_read.digest = receipt["digest"].as_str().unwrap().into();
    already_read.hidden = true;
    let mut local = client.local(&channel).unwrap();
    local.message = 3;
    local.last = Some(preview(&written(3, "other", "newer")));
    keep(&client, &channel, &mut local, &already_read);
    client.save_local(&channel, &local).unwrap();
    client
        .channel_send_settle(&channel, &id, Ok(receipt))
        .unwrap();
    let kept = kept(&client, &channel, 1);
    assert_eq!(kept.text, "edited elsewhere");
    assert!(kept.hidden);
    assert_eq!(client.local(&channel).unwrap().message, 3);
    assert_eq!(client.local(&channel).unwrap().last.unwrap().text, "newer");
}

#[tokio::test]
async fn a_warm_send_finishes_at_the_ack_without_waiting_for_a_sync() {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::{Duration, timeout};

    struct Sender {
        client: Mutex<Client>,
        head: Json,
        reads: AtomicUsize,
    }
    impl Hold for Sender {
        async fn with<T: Send>(
            &self,
            step: impl FnOnce(&mut Client) -> Result<T> + Send,
        ) -> Result<T> {
            step(&mut self.client.lock().unwrap())
        }

        async fn trust(&self) -> Result<Trust> {
            Ok(Trust::new(vec![], String::new(), vec![]))
        }

        async fn send(
            &self,
            mut sign: impl FnMut(&mut Client) -> Result<Prepared> + Send,
        ) -> Result<Json> {
            self.with(|c| sign(c)).await?;
            if self.reads.fetch_add(1, Ordering::SeqCst) == 0 {
                return Ok(self.head.clone());
            }
            // The control view is current; any further read is unrelated
            // to the durable send and must not hold up its completion.
            std::future::pending().await
        }
    }
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    let (channel, ledger) = active_channel(&mut client);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    crate::testing::relay_to(
        &mut client,
        &format!("http://{}", listener.local_addr().unwrap()),
    );
    let h = Arc::new(Sender {
        client: Mutex::new(client),
        head: json(&ledger),
        reads: AtomicUsize::new(0),
    });
    let id = "f1".repeat(32);
    // An interrupted preparation is retried by the UI without the Agent
    // display metadata. The queued message must retain its original author.
    h.with(|c| {
        c.queue_channel_send(
            &channel,
            &id,
            &Draft {
                text: "hello".into(),
                agent: Some(AgentRef {
                    id: "test-agent".into(),
                    name: "Original writer".into(),
                }),
                ..Draft::default()
            },
            None,
            now_ms(),
        )
    })
    .await
    .unwrap();
    let server = tokio::spawn({
        let h = h.clone();
        let channel = channel.clone();
        let id = id.clone();
        async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 8192];
            let n = socket.read(&mut buffer).await.unwrap();
            assert!(
                String::from_utf8_lossy(&buffer[..n])
                    .starts_with(&format!("POST /v1/channels/{channel}/messages "))
            );
            let receipt = h.with(|c| {
                let op = c.job::<Operation>(&channel, &format!("send:{id}"))?.unwrap();
                Ok(json!({"seq": 3, "stored_at": now_ms(), "digest": hex_sha256(&unb64(&op.cose)?)}))
            }).await.unwrap();
            let body = json!({"ok":true, "data":receipt}).to_string();
            socket.write_all(format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        }
    });
    let result = timeout(
        Duration::from_secs(5),
        send(
            &*h,
            &channel,
            &Draft {
                text: "hello".into(),
                ..Draft::default()
            },
            Some(&id),
        ),
    )
    .await
    .expect("a stored send must not wait for message sync")
    .unwrap();
    assert_eq!(result["receipt"]["seq"], 3);
    assert_eq!(result["message"]["text"], "hello");
    assert_eq!(result["message"]["message_id"], id);
    assert_eq!(result["message"]["seq"], 3);
    assert_eq!(result["message"]["agent"]["name"], "Original writer");
    assert!(
        h.with(|c| c.channel_outbox(&channel))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(h.reads.load(Ordering::SeqCst), 1);
    assert_eq!(
        h.with(|c| c.channel_messages_before(&channel, u64::MAX, 10))
            .await
            .unwrap()[0]
            .text,
        "hello"
    );
    server.await.unwrap();
}

#[tokio::test]
async fn lost_or_stalled_ack_is_reconciled_without_a_second_publication() {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::io::AsyncReadExt;
    use tokio::time::{Duration, timeout};

    struct Sender {
        client: Mutex<Client>,
        channel: String,
        id: String,
        head: Json,
        reads: AtomicUsize,
        stored: bool,
        bad_digest: bool,
    }
    impl Hold for Sender {
        async fn with<T: Send>(
            &self,
            step: impl FnOnce(&mut Client) -> Result<T> + Send,
        ) -> Result<T> {
            step(&mut self.client.lock().unwrap())
        }

        async fn trust(&self) -> Result<Trust> {
            Ok(Trust::new(vec![], String::new(), vec![]))
        }

        async fn send(
            &self,
            mut sign: impl FnMut(&mut Client) -> Result<Prepared> + Send,
        ) -> Result<Json> {
            let request = self.with(|c| sign(c)).await?;
            assert!(request.is_read(), "reconciliation must not publish again");
            if self.reads.fetch_add(1, Ordering::SeqCst) == 0 {
                return Ok(self.head.clone());
            }
            self.with(|c| {
                let op = c.job::<Operation>(&self.channel, &format!("send:{}", self.id))?.unwrap();
                Ok(json!({"found":self.stored, "result":{
                    "seq":3, "stored_at":now_ms(),
                    "digest": if self.bad_digest { "00".repeat(32) } else { hex_sha256(&unb64(&op.cose)?) }
                }}))
            }).await
        }
    }

    for (stall, stored, bad_digest) in [
        (true, true, false),
        (false, true, false),
        (false, false, false),
        (false, true, true),
    ] {
        let dir = crate::testing::TempDir::new();
        let mut client = crate::testing::unlocked(&dir.0, false, None);
        let (channel, ledger) = active_channel(&mut client);
        let id = client
            .queue_message(&channel, "one publication")
            .unwrap()
            .id;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        crate::testing::relay_to(
            &mut client,
            &format!("http://{}", listener.local_addr().unwrap()),
        );
        let h = Arc::new(Sender {
            client: Mutex::new(client),
            channel: channel.clone(),
            id: id.clone(),
            head: json(&ledger),
            reads: AtomicUsize::new(0),
            stored,
            bad_digest,
        });
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = [0; 8192];
            let n = socket.read(&mut bytes).await.unwrap();
            assert!(String::from_utf8_lossy(&bytes[..n]).starts_with("POST /v1/channels/"));
            if stall {
                std::future::pending::<()>().await;
            }
            // Otherwise drop the ACK connection after accepting the message.
        });
        let result = timeout(
            Duration::from_secs(6),
            crate::outbox::send(&*h, &channel, &id),
        )
        .await
        .expect("a durable receipt must not wait for the 30-second POST timeout");
        let confirmed = stored && !bad_digest;
        if confirmed {
            assert_eq!(result.unwrap()["message"]["text"], "one publication");
        } else {
            assert_eq!(
                result.unwrap_err(),
                if bad_digest {
                    Error::IntegrityFailed
                } else {
                    Error::ExecutionUnknown
                }
            );
        }
        h.with(|c| {
            assert_eq!(c.message_intents(&channel)?.is_empty(), confirmed);
            assert_eq!(c.channel_outbox(&channel)?.is_empty(), confirmed);
            assert_eq!(
                c.channel_messages_before(&channel, u64::MAX, 10)?.len(),
                usize::from(confirmed)
            );
            assert_eq!(
                c.local(&channel)?.message,
                0,
                "ACK must not skip unread gaps"
            );
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(h.reads.load(Ordering::SeqCst), 2);
        server.abort();
    }
}

#[tokio::test]
async fn text_sync_finishes_without_waiting_for_attachment_maintenance() {
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    struct Reader {
        client: Mutex<Client>,
        head: Json,
        reads: AtomicUsize,
    }
    impl Hold for Reader {
        async fn with<T: Send>(
            &self,
            step: impl FnOnce(&mut Client) -> Result<T> + Send,
        ) -> Result<T> {
            step(&mut self.client.lock().unwrap())
        }

        async fn trust(&self) -> Result<Trust> {
            Ok(Trust::new(vec![], String::new(), vec![]))
        }

        async fn send(
            &self,
            mut sign: impl FnMut(&mut Client) -> Result<Prepared> + Send,
        ) -> Result<Json> {
            self.with(|c| sign(c)).await?;
            match self.reads.fetch_add(1, Ordering::SeqCst) {
                0 => Ok(self.head.clone()),
                1 => Ok(
                    json!({"head":self.head["head"],"entries":[],"skipped":[],"next_seq":0,"latest_seq":0}),
                ),
                _ => std::future::pending().await,
            }
        }
    }
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    let (channel, ledger) = active_channel(&mut client);
    let h = Reader {
        client: Mutex::new(client),
        head: json(&ledger),
        reads: AtomicUsize::new(0),
    };
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        sync_messages(&h, &channel, false),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        h.with(|c| c.channel_observed(&channel))
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(h.reads.load(Ordering::SeqCst), 2);
}

#[test]
fn message_end_is_terminal_and_an_incomplete_page_must_advance() {
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    let (channel, ledger) = active_channel(&mut client);
    let trust = Trust::new(vec![], String::new(), vec![]);
    for latest in [0, 1] {
        let mut reading = client.message_reading(&channel).unwrap();
        let result = client.channel_read_settle(
            &mut reading,
            json!({
                "head":ledger.head, "entries":[], "skipped":[], "next_seq":0, "latest_seq":latest
            }),
            &trust,
        );
        if latest == 0 {
            assert!(result.is_ok());
            assert!(reading.done());
        } else {
            assert_eq!(result, Err(Error::IntegrityFailed));
        }
    }
}

#[tokio::test]
async fn a_large_cold_channel_reads_the_tail_then_one_history_page_without_claiming_delivery() {
    use std::collections::VecDeque;
    use std::sync::Mutex;
    struct Relay {
        client: Mutex<Client>,
        replies: Mutex<VecDeque<Json>>,
    }
    impl Hold for Relay {
        async fn with<T: Send>(
            &self,
            step: impl FnOnce(&mut Client) -> Result<T> + Send,
        ) -> Result<T> {
            step(&mut self.client.lock().unwrap())
        }

        async fn trust(&self) -> Result<Trust> {
            Ok(Trust::new(vec![], String::new(), vec![]))
        }

        async fn send(
            &self,
            mut sign: impl FnMut(&mut Client) -> Result<Prepared> + Send,
        ) -> Result<Json> {
            self.with(|c| sign(c)).await?;
            Ok(self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("must not fetch the old backlog"))
        }
    }
    for with_hint in [true, false] {
        let dir = crate::testing::TempDir::new();
        let mut client = crate::testing::unlocked(&dir.0, false, None);
        let (channel, mut ledger) = active_channel(&mut client);
        let me = client.me().unwrap();
        ledger.epoch = 2;
        ledger.members.get_mut(&me).unwrap().joined_epoch = 2;
        let mut local = client.local(&channel).unwrap();
        local.ledger = Some(ledger.clone());
        client.save_local(&channel, &local).unwrap();
        // No synthetic certificate is accepted here: these old-epoch rows
        // are explicitly inaccessible, and carry no body or signature.
        let page = |after: u64, through: u64, latest: u64| {
            json!({
                "head":ledger.head, "entries":[], "skipped":(after+1..=through).map(|seq| json!({"seq":seq,"epoch":1})).collect::<Vec<_>>(),
                "next_seq":through,"latest_seq":latest,
            })
        };
        let mut head = json(&ledger);
        if with_hint {
            head["capacity"] = json!({"messages":{"used":1_000_000}});
        }
        let mut replies = VecDeque::from([head.clone()]);
        if !with_hint {
            replies.push_back(page(0, 20, 1_000_000));
        }
        replies.push_back(page(999_980, 1_000_000, 1_000_000));
        let h = Relay {
            client: Mutex::new(client),
            replies: Mutex::new(replies),
        };
        let result = sync_recent(&h, &channel).await.unwrap();
        assert_eq!(result.through, 1_000_000);
        assert!(h.replies.lock().unwrap().is_empty());
        h.with(|c| {
            let local = c.local(&channel)?;
            assert_eq!(local.message, 0);
            assert_eq!(local.recent, Some((999_980, 1_000_000)));
            assert_eq!(local.skipped[&1], (999_981, 1_000_000));
            assert_eq!(c.channel_cursor_due(&channel)?, None);
            let page = c.channel_message_page(&channel, u64::MAX, 100)?;
            assert!(page.items.is_empty() && page.has_more);
            assert_eq!(page.next_before, Some(999_981));
            assert_eq!(c.channel_mark(&channel, 1_000_000)?.read, 1_000_000);
            Ok(())
        })
        .await
        .unwrap();
        h.replies
            .lock()
            .unwrap()
            .extend([head.clone(), page(999_960, 999_980, 1_000_000)]);
        let history = earlier_messages(&h, &channel, 999_981, 100).await.unwrap();
        assert!(history.items.is_empty() && history.has_more);
        assert_eq!(
            history.next_before,
            Some(999_961),
            "an empty display page still makes progress"
        );
        assert!(h.replies.lock().unwrap().is_empty());
        assert_eq!(
            h.with(|c| Ok(c.local(&channel)?.recent)).await.unwrap(),
            Some((999_960, 1_000_000))
        );

        if with_hint {
            head["capacity"]["messages"]["used"] = json!(1_000_003);
        }
        h.replies
            .lock()
            .unwrap()
            .extend([head.clone(), page(1_000_000, 1_000_003, 1_000_003)]);
        assert_eq!(sync_recent(&h, &channel).await.unwrap().through, 1_000_003);
        assert!(h.replies.lock().unwrap().is_empty());
        assert_eq!(
            h.with(|c| c.channel_cursor_due(&channel)).await.unwrap(),
            Some((0, 1_000_000))
        );
        if !with_hint {
            continue;
        }

        // The list knows of messages before they arrive, and a tail read
        // takes one page however many came.
        let view = h.with(|c| c.channel_view(&channel)).await.unwrap();
        assert!(!view.news);
        let mut ahead = head.clone();
        ahead["capacity"]["messages"]["used"] = json!(1_000_100);
        h.replies.lock().unwrap().push_back(ahead.clone());
        read(&h, &mut Reading::controls(&channel)).await.unwrap();
        assert!(h.replies.lock().unwrap().is_empty());
        let view = h.with(|c| c.channel_view(&channel)).await.unwrap();
        assert!(view.news && view.active_at > 0);
        h.replies
            .lock()
            .unwrap()
            .extend([ahead, page(1_000_080, 1_000_100, 1_000_100)]);
        assert_eq!(sync_tail(&h, &channel).await.unwrap().through, 1_000_100);
        assert!(h.replies.lock().unwrap().is_empty());
        let view = h.with(|c| c.channel_view(&channel)).await.unwrap();
        assert!(!view.news);
        assert_eq!(view.history_before, Some(1_000_081));

        // Another device read beyond our cached tail. Taking that account's
        // position clears the unread dot without claiming we fetched the gap.
        head["capacity"]["messages"]["used"] = json!(1_000_200);
        head["seen"] = json!(1_000_200);
        h.replies.lock().unwrap().push_back(head.clone());
        read(&h, &mut Reading::controls(&channel)).await.unwrap();
        let view = h.with(|c| c.channel_view(&channel)).await.unwrap();
        assert_eq!(view.read, 1_000_200);
        assert_eq!(view.latest_cursor, 1_000_100);
        assert!(!view.news);

        head["capacity"]["messages"]["used"] = json!(1_000_201);
        h.replies.lock().unwrap().push_back(head);
        read(&h, &mut Reading::controls(&channel)).await.unwrap();
        assert!(h.with(|c| c.channel_view(&channel)).await.unwrap().news);
    }
}

#[test]
fn cached_history_does_not_bridge_a_gap_and_merges_into_the_complete_prefix() {
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    let (channel, _) = active_channel(&mut client);
    let mut local = client.local(&channel).unwrap();
    for seq in (1..=20).chain(81..=100) {
        keep(
            &client,
            &channel,
            &mut local,
            &written(seq, "peer", "history"),
        );
    }
    local.message = 20;
    local.received(80, 100);
    client.save_local(&channel, &local).unwrap();
    let page = client
        .channel_message_page(&channel, u64::MAX, 100)
        .unwrap();
    assert_eq!(
        page.items.iter().map(|m| m.seq).collect::<Vec<_>>(),
        (81..=100).collect::<Vec<_>>()
    );
    assert_eq!(page.next_before, Some(81));
    assert!(
        client
            .channel_message_page(&channel, 81, 100)
            .unwrap()
            .items
            .is_empty()
    );
    assert_eq!(client.channel_cursor_due(&channel).unwrap(), Some((20, 0)));

    local.received(60, 80);
    assert_eq!(local.recent, Some((60, 100)));
    local.received(20, 60);
    assert_eq!(local.message, 100);
    assert!(local.recent.is_none());
    local.received(20, 40); // A late page cannot move either cursor backwards.
    assert_eq!(local.message, 100);
    for seq in 21..=80 {
        keep(
            &client,
            &channel,
            &mut local,
            &written(seq, "peer", "history"),
        );
    }
    client.save_local(&channel, &local).unwrap();
    let page = client.channel_message_page(&channel, 81, 100).unwrap();
    assert_eq!(page.items.len(), 80);
    assert!(!page.has_more);
    assert_eq!(page.next_before, None);
    assert_eq!(client.channel_cursor_due(&channel).unwrap(), Some((100, 0)));
}

#[test]
fn empty_recent_head_stops_without_a_message_request_and_rollback_stays_rejected() {
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    let (channel, ledger) = active_channel(&mut client);
    let trust = Trust::new(vec![], String::new(), vec![]);
    let mut head = json(&ledger);
    head["capacity"] = json!({"messages":{"used":0}});
    let mut reading = Reading {
        recent: true,
        ..Reading::new(&channel)
    };
    client.channel_read_request(&mut reading).unwrap();
    client
        .channel_read_settle(&mut reading, head.clone(), &trust)
        .unwrap();
    assert!(reading.done());
    let mut local = client.local(&channel).unwrap();
    local.received(80, 100);
    client.save_local(&channel, &local).unwrap();
    let mut reading = Reading {
        recent: true,
        ..Reading::new(&channel)
    };
    client.channel_read_request(&mut reading).unwrap();
    assert_eq!(
        client.channel_read_settle(&mut reading, head, &trust),
        Err(Error::IntegrityFailed)
    );
    assert_eq!(client.local(&channel).unwrap().recent, Some((80, 100)));
}

#[test]
fn desktop_queue_clears_only_its_submitted_draft_and_keeps_the_original_id() {
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    let (channel, _) = active_channel(&mut client);
    let id = "f9".repeat(32);
    let draft = Draft {
        text: "  keep spacing  ".into(),
        ..Draft::default()
    };
    client.channel_draft(&channel, &draft.text).unwrap();
    client
        .channel_queue(&channel, &id, &draft, None, None)
        .unwrap();
    assert!(client.channel_view(&channel).unwrap().draft.is_empty());
    assert_eq!(
        client.channel_outbox(&channel).unwrap()[0]["message_id"],
        id
    );
    client.channel_draft(&channel, "next draft").unwrap();
    client
        .channel_queue(&channel, &id, &draft, None, None)
        .unwrap();
    assert_eq!(client.channel_view(&channel).unwrap().draft, "next draft");
    let changed = Draft {
        text: "different message".into(),
        ..Draft::default()
    };
    assert_eq!(
        client.channel_queue(&channel, &id, &changed, None, None),
        Err(Error::IdempotencyConflict)
    );
    assert_eq!(client.channel_outbox(&channel).unwrap().len(), 1);
}

#[test]
fn reopening_granted_history_does_not_replace_the_recent_timeline_or_delivery_cursor() {
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    let (channel, mut ledger) = active_channel(&mut client);
    let me = client.me().unwrap();
    ledger.epoch = 2;
    ledger.members.get_mut(&me).unwrap().joined_epoch = 2;
    let mut local = client.local(&channel).unwrap();
    local.ledger = Some(ledger.clone());
    local.recent = Some((80, 100));
    local.granted = vec![(1, 1)];
    client.save_local(&channel, &local).unwrap();
    let mut reading = Reading {
        recent: true,
        ledger: Some(ledger.clone()),
        ..Reading::new(&channel)
    };
    client.grants_opened(&mut reading, &[(11, 20)]).unwrap();
    let page = json!({"head":ledger.head,"entries":[],"skipped":(11..=20).map(|seq|json!({"seq":seq,"epoch":1})).collect::<Vec<_>>(),"next_seq":20,"latest_seq":100});
    client
        .channel_read_settle(
            &mut reading,
            page,
            &Trust::new(vec![], String::new(), vec![]),
        )
        .unwrap();
    let local = client.local(&channel).unwrap();
    assert_eq!(local.recent, Some((80, 100)));
    assert_eq!(local.message, 0);
    assert!(local.granted.is_empty());
    assert!(reading.done());
}

#[tokio::test]
async fn a_send_is_durable_before_its_first_network_read() {
    use std::sync::{Arc, Mutex};

    struct Offline(Mutex<Client>);
    impl Hold for Offline {
        async fn with<T: Send>(
            &self,
            step: impl FnOnce(&mut Client) -> Result<T> + Send,
        ) -> Result<T> {
            step(&mut self.0.lock().unwrap())
        }

        async fn trust(&self) -> Result<Trust> {
            Err(Error::Unavailable("offline".into()))
        }
    }

    let dir = crate::testing::TempDir::new();
    let storage = Arc::new(crate::testing::MemoryStorage::default());
    let mut client = crate::testing::unlocked(&dir.0, false, Some(storage.clone()));
    let (channel, _) = active_channel(&mut client);
    client.enable_fast_unlock().unwrap();
    let h = Offline(Mutex::new(client));
    let id = "f2".repeat(32);
    let draft = Draft {
        text: "Keep me through restart".into(),
        ..Default::default()
    };
    assert!(matches!(
        send(&h, &channel, &draft, Some(&id)).await,
        Err(Error::Unavailable(_))
    ));
    let changed = Draft {
        text: "different content".into(),
        ..Default::default()
    };
    assert_eq!(
        send(&h, &channel, &changed, Some(&id)).await,
        Err(Error::IdempotencyConflict)
    );
    let path = dir.path("queued.txt");
    std::fs::write(&path, "attachment draft").unwrap();
    let file = h
        .with(|c| c.files_import(&path, "root", false, &mut |_, _| {}))
        .await
        .unwrap()
        .file;
    let file_id = "f3".repeat(32);
    h.with(|c| c.channel_queue(&channel, &file_id, &draft, Some(&file.id), None))
        .await
        .unwrap();
    assert!(matches!(
        send_file(&h, &channel, &file.id, None, &draft, Some(&file_id)).await,
        Err(Error::Unavailable(_))
    ));
    assert_eq!(
        send(&h, &channel, &draft, Some(&file_id)).await,
        Err(Error::IdempotencyConflict)
    );
    // Desktop retries can omit the version. A newer library revision must
    // not replace the version already queued for this message ID.
    std::fs::write(&path, "a newer attachment draft").unwrap();
    let updated = h
        .with(|c| c.files_import(&path, "root", false, &mut |_, _| {}))
        .await
        .unwrap()
        .file;
    assert_eq!(updated.id, file.id);
    assert_ne!(updated.current, file.current);
    h.with(|c| c.channel_queue(&channel, &file_id, &draft, Some(&file.id), None))
        .await
        .unwrap();
    assert_eq!(
        h.with(|c| c.channel_queue(
            &channel,
            &file_id,
            &draft,
            Some(&file.id),
            Some(&updated.current)
        ))
        .await,
        Err(Error::IdempotencyConflict)
    );
    assert!(matches!(
        send_file(&h, &channel, &file.id, None, &draft, Some(&file_id)).await,
        Err(Error::Unavailable(_))
    ));
    assert_eq!(
        send_file(
            &h,
            &channel,
            &file.id,
            Some(&updated.current),
            &draft,
            Some(&file_id),
        )
        .await,
        Err(Error::IdempotencyConflict)
    );
    drop(h);
    let mut reopened = Client::open(crate::testing::config(false), &dir.0, Some(storage)).unwrap();
    reopened.unlock_fast().await.unwrap();
    let pending = reopened.channel_outbox(&channel).unwrap();
    assert_eq!(pending.len(), 2);
    let text = pending
        .iter()
        .find(|item| item["message_id"] == id)
        .unwrap();
    assert_eq!(text["draft"]["text"], draft.text);
    let attachment = pending
        .iter()
        .find(|item| item["message_id"] == file_id)
        .unwrap();
    assert_eq!(attachment["draft"]["text"], draft.text);
    assert_eq!(
        attachment["file"]["source"],
        format!("{}@{}", file.id, file.current)
    );
    assert_eq!(attachment["file"]["name"], "queued.txt");
    reopened.channel_discard(&channel, &id).unwrap();
    reopened.channel_discard(&channel, &file_id).unwrap();
    assert!(reopened.channel_outbox(&channel).unwrap().is_empty());
}
