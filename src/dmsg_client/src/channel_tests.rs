//! The channel byte rules against the vectors the extension exported:
//! envelopes, key confirmation, epoch manifest, message keys and AAD,
//! history envelopes and attachments.

use super::*;
use crate::codec::decode_value;
use crate::crypto::hpke_open;
use crate::vectors::{bytes, field, key, load, text, value};

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

#[tokio::test]
async fn a_body_that_does_not_open_leaves_its_message_unreadable() {
    use crate::testing::{TempDir, unlocked};

    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let channel = "c".repeat(64);
    let me = client.me().unwrap();
    let root = [7u8; 32];
    let mut ring = Keyring {
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
    let opened = client
        .open_message(&channel, &mut ring, message.clone(), &payload)
        .await
        .unwrap();
    assert!(opened.unreadable && opened.text.is_empty());
    // A body that opens but says something malformed is unreadable too.
    let key = message_key(&root, &channel, 1, &message_id, &device);
    let header = payload.clone();
    let aad = message_aad(&me, &device, &header);
    payload.ciphertext = seal(&key, &canonical(&json!({"text": 7})), &aad);
    let opened = client
        .open_message(&channel, &mut ring, message.clone(), &payload)
        .await
        .unwrap();
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
    let opened = client
        .open_message(&channel, &mut ring, message, &payload)
        .await
        .unwrap();
    assert!(!opened.unreadable);
    assert_eq!(opened.text, "hello");
    assert_eq!(opened.agent.unwrap().name, "claude");
}

#[tokio::test]
async fn reads_of_one_channel_settle_in_any_order() {
    use crate::channel_log::Member;
    use crate::testing::{TempDir, unlocked, warm};

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
        .channel_read_settle(&mut slower, page(6))
        .await
        .unwrap();
    assert_eq!(
        client.channel_read_path(&mut slower).unwrap(),
        format!("/v1/channels/{channel}/sync?kind=control&after=9&limit=5")
    );
    // A page that does not continue the cursor it was asked against is
    // still refused.
    for back in [page(6), page(9)] {
        assert_eq!(
            client.channel_read_settle(&mut slower, back).await,
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
        .channel_read_settle(&mut reading, json!({"head": head}))
        .await
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
            .channel_read_settle(&mut faster, page(after, through))
            .await
            .unwrap();
    }
    assert!(faster.done());
    for (after, through) in [(0, 20), (20, 30)] {
        client
            .channel_read_settle(&mut slower, page(after, through))
            .await
            .unwrap();
        assert_eq!(client.local(&channel).unwrap().message, 30);
    }
    assert!(slower.done());
}
