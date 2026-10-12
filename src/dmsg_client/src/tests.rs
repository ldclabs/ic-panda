//! Readers of the exported formats no module implements yet: each test
//! rebuilds the contexts and digests from record fields, then opens the
//! ciphertexts. The tuples here are the byte rules the channel and inbox (W5)
//! modules take over; content, files, root bundles and cloud commands are
//! checked by their own modules.

use crate::codec::{b64, canonical, decode, decode_value, digest, sha256, unb64, MAX_RECORD};
use crate::crypto::{derive, hpke_open, hpke_public, open, HpkeEnvelope};
use crate::vectors::{bytes, field, key, load, text, value};
use serde::{Deserialize, Serialize};
use serde_bytes::Bytes;

fn hex_sha256(bytes: &[u8]) -> String {
    hex::encode(sha256(bytes).as_slice())
}

fn json<T: serde::de::DeserializeOwned>(value: &serde_json::Value) -> T {
    serde_json::from_value(value.clone()).unwrap()
}

#[derive(Serialize, Deserialize)]
struct Recipient {
    recipient: String,
    hpke_pub: String,
}

#[derive(Serialize)]
struct EpochManifest<'a> {
    format: &'a str,
    channel: &'a str,
    epoch: u64,
    lease_id: &'a str,
    head: &'a str,
    fencing: u64,
    recipients_digest: &'a str,
    key_confirmation: &'a str,
    group: u64,
}

#[derive(Serialize, Deserialize)]
struct MessageAad {
    channel_id: String,
    epoch: u64,
    control_head: String,
    acl_version: u64,
    message_id: String,
    client_created_at: u64,
}

#[derive(Serialize, Deserialize)]
struct HistoryScope {
    channel: String,
    target: String,
    from: u64,
    to: u64,
    grant_id: String,
    head: String,
    epoch: u64,
    recipients_digest: String,
}

#[test]
fn opens_channel_epoch_message_and_history() {
    let v = load("channel-v1.json");
    let channel = text(&v["channel"]);
    let member_seed = key(&v["member"]["hpke_seed_hex"]);
    let epoch_key = key(&v["epoch_key_hex"]);
    let recipients: Vec<Recipient> = json(&v["recipients"]);
    assert_eq!(
        canonical(&recipients),
        bytes(&v["recipients_digest_input"]["hex"])
    );
    assert_eq!(
        hex::encode(digest("dmsg/channel/recipients/v1", &recipients).as_slice()),
        text(&v["lease"]["recipients_digest"])
    );
    let a = &v["activation"];
    let epoch = a["epoch"].as_u64().unwrap();
    let upload = &v["rotation_uploads"][0];
    for (i, reference) in a["envelopes"].as_array().unwrap().iter().enumerate() {
        let recipient = text(&reference["recipient"]);
        let context = (
            "dmsg/channel-epoch-envelope/1",
            channel,
            epoch,
            text(&a["lease_id"]),
            a["fencing"].as_u64().unwrap(),
            text(&a["expected_head"]),
            text(&a["recipients_digest"]),
            recipient,
        );
        assert_eq!(
            canonical(&context),
            bytes(&v["envelope_contexts"][i]["hex"])
        );
        let chunk = unb64(text(&upload["chunks"][i])).unwrap();
        assert_eq!(hex_sha256(&chunk), text(&reference["digest"]));
        assert_eq!(chunk.len() as u64, reference["size"].as_u64().unwrap());
        let envelope: HpkeEnvelope = decode(&chunk, MAX_RECORD).unwrap();
        if recipient.contains(text(&v["member"]["account"])) {
            assert_eq!(
                *hpke_open(&member_seed, &envelope, &context).unwrap(),
                epoch_key
            );
        }
    }
    let confirmation = (channel, epoch, Bytes::new(&epoch_key));
    assert_eq!(
        canonical(&confirmation),
        bytes(&v["key_confirmation_input"]["hex"])
    );
    assert_eq!(
        hex::encode(digest("dmsg/channel-epoch-confirmation/v1", &confirmation).as_slice()),
        text(&a["key_confirmation"])
    );
    let manifest = EpochManifest {
        format: "dmsg-channel-epoch/1",
        channel,
        epoch,
        lease_id: text(&a["lease_id"]),
        head: text(&a["expected_head"]),
        fencing: a["fencing"].as_u64().unwrap(),
        recipients_digest: text(&a["recipients_digest"]),
        key_confirmation: text(&a["key_confirmation"]),
        group: 0,
    };
    assert_eq!(b64(&canonical(&manifest)), text(&upload["manifest"]));
    assert_eq!(
        hex_sha256(&canonical(&manifest)),
        text(&upload["plan"]["manifest_digest"])
    );

    let m = &v["message"];
    let device = text(&m["device_id"]);
    let message_aad: MessageAad = serde_json::from_value({
        let mut payload = m["payload"].clone();
        payload.as_object_mut().unwrap().remove("ciphertext");
        payload
    })
    .unwrap();
    let key_context = (
        "dmsg/channel-message-key/1",
        channel,
        message_aad.epoch,
        &message_aad.message_id,
        device,
    );
    assert_eq!(canonical(&key_context), bytes(&m["key_context"]["hex"]));
    let message_key = derive(&epoch_key, &key_context);
    assert_eq!(*message_key, key(&m["key_hex"]));
    let aad = (
        "dmsg/channel-message/1",
        text(&m["sender"]),
        device,
        &message_aad,
    );
    assert_eq!(canonical(&aad), bytes(&m["aad"]["hex"]));
    let plaintext = open(&message_key, text(&m["payload"]["ciphertext"]), &aad).unwrap();
    assert_eq!(*plaintext, bytes(&m["plaintext_hex"]));
    assert_eq!(canonical(&value(&m["plaintext"])), *plaintext);

    let h = &v["history"];
    let scope: HistoryScope = json(&h["scope"]);
    let recipient = format!(
        "device:{}:{}",
        text(&v["member"]["account"]),
        text(&v["member"]["device_id"])
    );
    let context = (
        "dmsg/channel-history-envelope/1",
        &scope,
        &recipient,
        1u64,
        1u64,
    );
    assert_eq!(canonical(&context), bytes(&h["context"]["hex"]));
    let chunk = unb64(text(&h["uploads"][0]["chunks"][0])).unwrap();
    let envelope: HpkeEnvelope = decode(&chunk, MAX_RECORD).unwrap();
    let plaintext = hpke_open(&member_seed, &envelope, &context).unwrap();
    assert_eq!(*plaintext, bytes(&h["plaintext_hex"]));
    assert_eq!(*plaintext, canonical(&[(1u64, Bytes::new(&epoch_key))]));
    let history =
        decode_value(&unb64(text(&h["uploads"][0]["manifest"])).unwrap(), 65_536).unwrap();
    assert_eq!(
        field(&history, "format").as_text(),
        Some("dmsg-channel-history/1")
    );
    assert_eq!(canonical(field(&history, "scope")), canonical(&scope));
}

#[test]
fn opens_channel_attachment() {
    let v = load("channel-v1.json");
    let channel = text(&v["channel"]);
    let a = &v["attachment"];
    let device = text(&v["owner"]["device_id"]);
    let message_aad: MessageAad = serde_json::from_value({
        let mut payload = a["payload"].clone();
        payload.as_object_mut().unwrap().remove("ciphertext");
        payload
    })
    .unwrap();
    let message_key = derive(
        &key(&v["epoch_key_hex"]),
        &(
            "dmsg/channel-message-key/1",
            channel,
            message_aad.epoch,
            &message_aad.message_id,
            device,
        ),
    );
    assert_eq!(*message_key, key(&a["key_hex"]));
    let aad = (
        "dmsg/channel-message/1",
        text(&v["owner"]["account"]),
        device,
        &message_aad,
    );
    assert_eq!(canonical(&aad), bytes(&a["aad"]["hex"]));
    let plaintext = open(&message_key, text(&a["payload"]["ciphertext"]), &aad).unwrap();
    assert_eq!(*plaintext, bytes(&a["plaintext_hex"]));
    // The message carries the file key; the manifest is sealed under it.
    let message = decode_value(&plaintext, MAX_RECORD).unwrap();
    let reference = field(&message, "file");
    let file_key: [u8; 32] = unb64(field(reference, "key").as_text().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let epoch = u64::try_from(i128::from(field(reference, "epoch").as_integer().unwrap())).unwrap();
    let context = (
        "dmsg/channel-file-manifest/1",
        channel,
        epoch,
        field(reference, "upload_id").as_text().unwrap(),
        field(reference, "file_id").as_text().unwrap(),
        field(reference, "version").as_text().unwrap(),
    );
    assert_eq!(canonical(&context), bytes(&a["manifest_context"]["hex"]));
    let plan = &a["upload"]["plan"];
    let manifest = text(&a["upload"]["manifest"]);
    assert_eq!(
        hex_sha256(&unb64(manifest).unwrap()),
        text(&plan["manifest_digest"])
    );
    assert_eq!(
        field(reference, "manifest_digest").as_text(),
        Some(text(&plan["manifest_digest"]))
    );
    let opened = open(&file_key, manifest, &context).unwrap();
    assert_eq!(*opened, bytes(&a["manifest_plaintext_hex"]));
    let body = decode_value(&opened, MAX_RECORD).unwrap();
    assert_eq!(
        field(&body, "format").as_text(),
        Some("dmsg-channel-file/1")
    );
    assert_eq!(field(field(&body, "file"), "key").as_text(), Some(""));
}

#[test]
fn opens_inbox_letters() {
    let v = load("inbox-v1.json");
    let id_input = (
        "dmsg/inbox-key/1",
        text(&v["subject_id"]),
        v["key_version"].as_u64().unwrap(),
    );
    assert_eq!(
        canonical(&id_input),
        bytes(&v["key_object_id_input"]["hex"])
    );
    assert_eq!(hex_sha256(&canonical(&id_input)), text(&v["key_object_id"]));
    let payload = value(&v["key_payload"]);
    let seed: [u8; 32] = unb64(field(&payload, "seed").as_text().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    assert_eq!(hpke_public(&seed).to_vec(), bytes(&v["public_key_hex"]));
    let sealed = unb64(text(&v["envelope"])).unwrap();
    assert_eq!(sealed, bytes(&v["envelope_hex"]));
    let envelope: HpkeEnvelope = decode(&sealed, MAX_RECORD).unwrap();
    let context = (
        "dmsg/inbox-envelope/1",
        text(&v["sender"]),
        text(&v["recipient"]),
        text(&v["order_id"]),
        v["key_version"].as_u64().unwrap(),
    );
    assert_eq!(canonical(&context), bytes(&v["context"]["hex"]));
    assert_eq!(
        *hpke_open(&seed, &envelope, &context).unwrap(),
        bytes(&v["plaintext_hex"])
    );
}
