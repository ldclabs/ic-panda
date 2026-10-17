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
        file: None,
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
