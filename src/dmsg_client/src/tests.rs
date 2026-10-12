//! Readers of every exported client format: each test rebuilds the contexts
//! and digests from record fields, then opens the ciphertexts. The tuples here
//! are the byte rules the vault, files, root, channel and inbox modules take over.

use crate::codec::{b64, canonical, decode, decode_value, digest, sha256, unb64, MAX_RECORD};
use crate::crypto::{
    derive, hpke_open, hpke_public, open, seal_with_nonce, sign, signing_public, HpkeEnvelope,
};
use crate::vectors::{bytes, field, key, load, text, value};
use cbor2::Value;
use dmsg_types::Hash;
use serde::{Deserialize, Serialize};
use serde_bytes::Bytes;
use std::collections::BTreeMap;

fn hex_sha256(bytes: &[u8]) -> String {
    hex::encode(sha256(bytes).as_slice())
}

fn json<T: serde::de::DeserializeOwned>(value: &serde_json::Value) -> T {
    serde_json::from_value(value.clone()).unwrap()
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EncryptedObject {
    key: String,
    id: String,
    revision: String,
    parent: Option<String>,
    subject_id: String,
    device_id: String,
    generation: u64,
    kind: String,
    ciphertext: String,
    key_envelope: String,
    digest: String,
    tombstone: bool,
    conflict: bool,
    created_at: u64,
}

#[test]
fn opens_content_objects() {
    let vectors = load("content-v1.json");
    let root = key(&vectors["root_hex"]);
    let environment = text(&vectors["environment"]);
    for case in vectors["objects"].as_array().unwrap() {
        let r: EncryptedObject = json(&case["record"]);
        let aad = (
            "dmsg/content/1",
            environment,
            &r.subject_id,
            r.generation,
            &r.id,
            &r.revision,
            &r.device_id,
            &r.kind,
            &r.parent,
            r.tombstone,
        );
        let wrap = (
            "dmsg/wrap/1",
            &r.subject_id,
            &r.id,
            r.generation,
            &r.revision,
            "item-version",
        );
        assert_eq!(
            canonical(&aad),
            bytes(&case["content_aad"]["hex"]),
            "{}",
            case["name"]
        );
        assert_eq!(canonical(&wrap), bytes(&case["wrap_aad"]["hex"]));
        // The digest commits to the base64url text of both ciphertexts.
        let input = canonical(&(aad, &r.ciphertext, &r.key_envelope));
        assert_eq!(input, bytes(&case["digest_input_hex"]));
        assert_eq!(hex_sha256(&input), r.digest);
        let version_key: [u8; 32] = open(&root, &r.key_envelope, &wrap)
            .unwrap()
            .as_slice()
            .try_into()
            .unwrap();
        assert_eq!(version_key, key(&case["version_key_hex"]));
        let plaintext = open(&version_key, &r.ciphertext, &aad).unwrap();
        assert_eq!(*plaintext, bytes(&case["plaintext_hex"]));
        assert_eq!(canonical(&value(&case["payload"])), *plaintext);
        decode_value(&plaintext, MAX_RECORD).unwrap();
        let container = BTreeMap::from([
            ("format", Value::Text("dmsg-cloud-object/1".into())),
            ("record", decode_value(&canonical(&r), MAX_RECORD).unwrap()),
        ]);
        assert_eq!(canonical(&container), bytes(&case["object_hex"]));
    }
}

fn chunk_nonce(index: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[..4].copy_from_slice(b"dMsg");
    nonce[4..].copy_from_slice(&index.to_be_bytes());
    nonce
}

#[test]
fn reproduces_file_chunks() {
    let vectors = load("content-v1.json");
    let file = vectors["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == "file entry")
        .unwrap();
    let manifest = field(&value(&file["payload"]), "file").clone();
    let file_key: [u8; 32] = unb64(field(&manifest, "key").as_text().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    let refs = field(&manifest, "chunks").as_array().unwrap();
    for case in vectors["file_chunks"].as_array().unwrap() {
        let index = case["index"].as_u64().unwrap();
        let size = field(&refs[index as usize], "size").as_integer().unwrap();
        let aad = (
            "dmsg/file-chunk/1",
            field(&manifest, "id").as_text().unwrap(),
            field(&manifest, "version").as_text().unwrap(),
            index,
            refs.len() as u64,
            u64::try_from(size).unwrap(),
        );
        assert_eq!(canonical(&aad), bytes(&case["aad"]["hex"]));
        assert_eq!(chunk_nonce(index).to_vec(), bytes(&case["nonce_hex"]));
        let sealed = seal_with_nonce(
            &file_key,
            &bytes(&case["plaintext_hex"]),
            &aad,
            &chunk_nonce(index),
        );
        assert_eq!(sealed, text(&case["ciphertext"]));
        assert_eq!(hex_sha256(&unb64(&sealed).unwrap()), text(&case["digest"]));
    }
    for case in vectors["chunk_rules"].as_array().unwrap() {
        let index = case["index"].as_u64().unwrap();
        let aad = (
            "dmsg/file-chunk/1",
            text(&case["file_id"]),
            text(&case["version"]),
            index,
            case["count"].as_u64().unwrap(),
            case["plaintext_length"].as_u64().unwrap(),
        );
        assert_eq!(canonical(&aad), bytes(&case["aad"]["hex"]));
        assert_eq!(chunk_nonce(index).to_vec(), bytes(&case["nonce_hex"]));
        let sealed = seal_with_nonce(
            &key(&case["key_hex"]),
            &bytes(&case["plaintext_hex"]),
            &aad,
            &chunk_nonce(index),
        );
        assert_eq!(sealed, text(&case["sealed"]));
        assert_eq!(hex_sha256(&unb64(&sealed).unwrap()), text(&case["digest"]));
    }
    assert_eq!(
        canonical(&BTreeMap::from([("format", "dmsg-cloud-empty-file/1")])),
        bytes(&vectors["empty_file_hex"])
    );
}

#[test]
fn opens_upload_manifests() {
    let vectors = load("content-v1.json");
    let root = key(&vectors["root_hex"]);
    let account = text(&vectors["account"]);
    for case in vectors["uploads"].as_array().unwrap() {
        let plan = &case["plan"];
        let aad = (
            "dmsg/cloud-manifest/1",
            account,
            text(&plan["upload_id"]),
            text(&plan["object_id"]),
            text(&plan["version_id"]),
            text(&plan["kind"]),
            plan["root_generation"].as_u64().unwrap(),
        );
        assert_eq!(
            canonical(&aad),
            bytes(&case["manifest_aad"]["hex"]),
            "{}",
            case["name"]
        );
        let manifest = unb64(text(&case["manifest"])).unwrap();
        assert_eq!(hex_sha256(&manifest), text(&plan["manifest_digest"]));
        assert_eq!(
            manifest.len() as u64,
            plan["manifest_size"].as_u64().unwrap()
        );
        let plaintext = open(&root, text(&case["manifest"]), &aad).unwrap();
        assert_eq!(*plaintext, bytes(&case["manifest_plaintext_hex"]));
        let body = decode_value(&plaintext, MAX_RECORD).unwrap();
        assert_eq!(
            field(&body, "format").as_text(),
            Some("dmsg-cloud-manifest/1")
        );
        let content = field(&body, "content");
        match case["content"].is_null() {
            true => assert!(content.is_null()),
            false => assert_eq!(canonical(content), canonical(&value(&case["content"]))),
        }
        if let Some(object) = case["object"].as_str() {
            let object = unb64(object).unwrap();
            assert_eq!(hex_sha256(&object), text(&plan["chunks"][0]["digest"]));
        }
    }
}

#[test]
fn opens_root_bundles() {
    let vectors = load("root-v2.json");
    let account = text(&vectors["account"]);
    let signer = key(&vectors["signer"]["public_hex"]);
    let seeds: BTreeMap<&str, [u8; 32]> = vectors["devices"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| (text(&d["device_id"]), key(&d["hpke_seed_hex"])))
        .collect();
    let mut previous: Option<([u8; 32], String)> = None;
    for case in vectors["generations"].as_array().unwrap() {
        let generation = case["generation"].as_u64().unwrap();
        let root = key(&case["root_hex"]);
        let bundle_bytes = bytes(&case["bundle_hex"]);
        assert_eq!(hex_sha256(&bundle_bytes), text(&case["bundle_sha256"]));
        let bundle = decode_value(&bundle_bytes, 64_000).unwrap();
        let body = field(&bundle, "body");
        assert_eq!(canonical(body), canonical(&value(&case["body"])));
        let signed = digest("dmsg/root-bundle/2", body);
        assert_eq!(signed.to_vec(), bytes(&case["signed_digest_hex"]));
        let signature = field(&bundle, "signature").as_bytes().unwrap();
        dmsg_protocol::verify(&Hash::new(signer), signed.as_slice(), signature).unwrap();
        let body_digest = sha256(&canonical(body));
        assert_eq!(body_digest.to_vec(), bytes(&case["body_digest"]));
        let envelopes = field(body, "envelopes").as_array().unwrap();
        let devices: Vec<Hash> = envelopes
            .iter()
            .map(|e| {
                Hash::new(
                    hex::decode(field(e, "device").as_text().unwrap())
                        .unwrap()
                        .try_into()
                        .unwrap(),
                )
            })
            .collect();
        let recipients = dmsg_protocol::root_recipients_digest(&devices, generation);
        assert_eq!(recipients.to_vec(), bytes(&case["recipients_digest"]));
        assert_eq!(
            dmsg_protocol::root_bundle_digest(recipients, body_digest).to_vec(),
            bytes(&case["bundle_digest"])
        );
        let commitment = derive(&root, &("dmsg/root-commitment/1",));
        assert_eq!(*commitment, key(&case["commitment_input_hex"]));
        assert_eq!(hex_sha256(commitment.as_slice()), text(&case["commitment"]));
        for (i, envelope) in envelopes.iter().enumerate() {
            let device = field(envelope, "device").as_text().unwrap();
            let context = ("dmsg/device-root/1", "local", account, generation, device);
            assert_eq!(
                canonical(&context),
                bytes(&case["device_contexts"][i]["hex"])
            );
            let sealed = HpkeEnvelope {
                enc: unb64(field(envelope, "enc").as_text().unwrap()).unwrap(),
                ciphertext: unb64(field(envelope, "ciphertext").as_text().unwrap()).unwrap(),
            };
            assert_eq!(*hpke_open(&seeds[device], &sealed, &context).unwrap(), root);
        }
        let link = field(body, "previous");
        match &previous {
            None => assert!(link.is_null()),
            Some((prior, prior_sha256)) => {
                let link_digest = field(link, "digest").as_text().unwrap();
                assert_eq!(link_digest, prior_sha256);
                let context = (
                    "dmsg/previous-root/1",
                    account,
                    generation,
                    generation - 1,
                    link_digest,
                );
                assert_eq!(canonical(&context), bytes(&case["previous_context"]["hex"]));
                let opened =
                    open(&root, field(link, "envelope").as_text().unwrap(), &context).unwrap();
                assert_eq!(*opened, *prior);
            }
        }
        previous = Some((root, hex_sha256(&bundle_bytes)));
    }
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

#[test]
fn reproduces_cloud_commands_and_http_pop() {
    let v = load("cloud-v1.json");
    // Seed 7 is the fixture's test-only device key.
    let seed = [7u8; 32];
    assert_eq!(signing_public(&seed).to_vec(), bytes(&v["public_key_hex"]));
    let c = &v["context"];
    for (encoded, typ) in [
        (
            text(&v["command"]["cose_sign1"]),
            "application/vnd.dmsg.command+cose;v=1",
        ),
        (
            text(&v["http_pop"]),
            "application/vnd.dmsg.http-pop+cose;v=1",
        ),
    ] {
        let cose = unb64(encoded).unwrap();
        assert_eq!(cose[0], 0xd2, "COSE_Sign1 tag");
        let Value::Array(items) = decode_value(&cose[1..], MAX_RECORD).unwrap() else {
            panic!("COSE_Sign1 array")
        };
        let [Value::Bytes(protected), Value::Map(unprotected), Value::Bytes(payload), Value::Bytes(signature)] =
            items.as_slice()
        else {
            panic!("COSE_Sign1 items")
        };
        assert!(unprotected.is_empty());
        let header = Value::Map(vec![
            (1.into(), (-19).into()),
            (2.into(), Value::Array(vec![15.into(), 16.into()])),
            (3.into(), "application/cbor".into()),
            (4.into(), Value::Bytes(bytes(&c["deviceId"]))),
            (
                15.into(),
                Value::Map(vec![(1.into(), text(&c["issuer"]).into())]),
            ),
            (16.into(), typ.into()),
        ]);
        assert_eq!(canonical(&header), *protected);
        let message = canonical(&(
            "Signature1",
            Bytes::new(protected),
            Bytes::new(&[]),
            Bytes::new(payload),
        ));
        assert_eq!(sign(&seed, &message).to_vec(), *signature);
        let body = decode_value(payload, MAX_RECORD).unwrap();
        assert_eq!(field(&body, "protocol").as_text(), Some("dmsg-cloud/1"));
    }
    let body = canonical(&BTreeMap::from([(
        "cose_sign1",
        text(&v["command"]["cose_sign1"]),
    )]));
    assert_eq!(body, bytes(&v["body_hex"]));
    assert_eq!(hex_sha256(&body), text(&v["body_digest"]));
}
