use super::*;
use crate::vectors::load;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

fn unb64url(text: &serde_json::Value) -> Vec<u8> {
    URL_SAFE_NO_PAD.decode(text.as_str().unwrap()).unwrap()
}

fn session(vectors: &serde_json::Value) -> (Offer, Key) {
    let transfer = &vectors["transfer"];
    let offer: Offer = serde_json::from_value(transfer["offer"].clone()).unwrap();
    let seed: Key = hex::decode(transfer["seed_hex"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    (offer, seed)
}

fn opened_archive(vectors: &serde_json::Value) -> Archive {
    let (offer, seed) = session(vectors);
    let now = vectors["transfer"]["now"].as_u64().unwrap();
    let opened = open_transfer(
        &unb64url(&vectors["transfer"]["file_b64"]),
        &offer,
        &seed,
        now,
    )
    .unwrap();
    Archive::decode(&opened.plain).unwrap()
}

#[test]
fn opens_the_page_offer_and_refuses_other_offers_and_frames() {
    let vectors = load("legacy-v1.json");
    let transfer = &vectors["transfer"];
    let (offer, seed) = session(&vectors);
    let now = transfer["now"].as_u64().unwrap();
    assert_eq!(
        hex::encode(canonical(&offer)),
        transfer["offer_cbor_hex"].as_str().unwrap()
    );
    assert_eq!(
        offer.fingerprint(),
        transfer["fingerprint"].as_str().unwrap()
    );
    assert_eq!(offer.link(), transfer["link"].as_str().unwrap());
    let opened = open_transfer(&unb64url(&transfer["file_b64"]), &offer, &seed, now).unwrap();
    assert_eq!(
        hex_sha256(&opened.plain),
        transfer["plain_sha256"].as_str().unwrap()
    );
    // The offer's validity is checked at the time the device accepted it.
    assert_eq!(
        open_transfer(
            &unb64url(&transfer["file_b64"]),
            &offer,
            &seed,
            offer.expires_at
        )
        .err(),
        Some(Error::Expired)
    );
    let other = crate::crypto::random_key();
    assert_eq!(
        open_transfer(&unb64url(&transfer["file_b64"]), &offer, &other, now).err(),
        Some(Error::Forbidden)
    );
    for rejected in transfer["rejected"].as_array().unwrap() {
        let result = open_transfer(&unb64url(&rejected["file_b64"]), &offer, &seed, now);
        assert_eq!(result.err(), Some(Error::Forbidden), "{}", rejected["case"]);
    }
    let framed = unb64url(&transfer["framed"]["file_b64"]);
    let opened = open_transfer(&framed, &offer, &seed, now).unwrap();
    assert_eq!(
        hex_sha256(&opened.plain),
        transfer["framed"]["plain_sha256"].as_str().unwrap()
    );
    for mutation in transfer["framed"]["mutations"].as_array().unwrap() {
        let Value::Map(mut entries) = cbor2::from_slice::<Value>(&framed).unwrap() else {
            panic!("transfer map");
        };
        let entry = |entries: &mut Vec<(Value, Value)>, name: &str| -> usize {
            entries
                .iter()
                .position(|(k, _)| matches!(k, Value::Text(t) if t == name))
                .unwrap()
        };
        let frames = entry(&mut entries, "frames");
        let size = entry(&mut entries, "size");
        let Value::Array(list) = &mut entries[frames].1 else {
            panic!("frames");
        };
        match mutation.as_str().unwrap() {
            "reverse" => list.reverse(),
            "duplicate" => list[1] = list[0].clone(),
            "drop_last" => {
                list.pop();
            }
            "flip" => {
                let Value::Map(frame) = &mut list[1] else {
                    panic!("frame");
                };
                let ciphertext = frame
                    .iter_mut()
                    .find(|(k, _)| matches!(k, Value::Text(t) if t == "ciphertext"))
                    .unwrap();
                let Value::Bytes(bytes) = &mut ciphertext.1 else {
                    panic!("ciphertext");
                };
                bytes[0] ^= 1;
            }
            "shrink" => {
                let Value::Integer(n) = entries[size].1 else {
                    panic!("size");
                };
                entries[size].1 = Value::Integer((u64::try_from(n).unwrap() - 1).into());
            }
            other => panic!("unknown mutation {other}"),
        }
        let changed = canonical(&Value::Map(entries));
        assert_eq!(
            open_transfer(&changed, &offer, &seed, now).err(),
            Some(Error::IntegrityFailed),
            "{mutation}"
        );
    }
}

#[test]
fn verifies_the_old_sample_offline() {
    let vectors = load("legacy-v1.json");
    let archive = opened_archive(&vectors);
    let expected = &vectors["archive"];
    assert_eq!(
        archive.content_id(),
        expected["content_id"].as_str().unwrap()
    );
    let checked = verify(&archive);
    assert_eq!(checked.partial, expected["partial"].as_bool().unwrap());
    assert_eq!(
        serde_json::to_value(&checked.checks).unwrap(),
        expected["checks"]
    );
    let gaps: Vec<_> = checked
        .gaps
        .iter()
        .map(|g| serde_json::json!({ "source": g.source, "code": g.code }))
        .collect();
    assert_eq!(serde_json::Value::Array(gaps), expected["gaps"]);
    assert_eq!(
        serde_json::to_value(&checked.messages).unwrap(),
        expected["messages"]
    );
    let file = open_file(&archive, expected["file"]["source"].as_str().unwrap()).unwrap();
    assert_eq!(file.name, expected["file"]["name"].as_str().unwrap());
    assert_eq!(
        hex_sha256(&file.bytes),
        expected["file"]["sha256"].as_str().unwrap()
    );
    let sources: Vec<&str> = archive
        .inventory
        .objects
        .iter()
        .map(|o| o.key.as_str())
        .collect();
    let incomplete = sources.iter().find(|s| s.ends_with("/file/8")).unwrap();
    assert_eq!(
        open_file(&archive, incomplete).err(),
        Some(Error::RecoveryIncomplete)
    );
    let avatar = sources.iter().find(|s| s.contains("/avatar/")).unwrap();
    let opened = open_file(&archive, avatar).unwrap();
    assert_eq!(image_mime(&opened.bytes), Some("image/png"));
    let summary = summary(&archive, &checked);
    assert_eq!(
        (summary.name.as_str(), summary.bio.as_str()),
        ("Old Panda", "Building on ICP.")
    );
    assert_eq!(summary.resources[0]["paid"], "1000000000");
    assert_eq!(summary.entitlements[0]["value"]["balance"], "123");
    // Re-encoding keeps the archive, and a substituted object is refused.
    let encoded = archive.encode().unwrap();
    assert_eq!(
        Archive::decode(&encoded).unwrap().content_id(),
        archive.content_id()
    );
    let mut changed = archive.clone();
    changed.inventory.objects[0].bytes = ByteBuf::from(vec![0xa0]);
    assert_eq!(changed.encode().err(), Some(Error::IntegrityFailed));
    let mut repeated = archive.clone();
    let first = repeated.inventory.objects[0].clone();
    repeated.inventory.objects.push(first);
    assert_eq!(repeated.encode().err(), Some(Error::IntegrityFailed));
    let mut foreign = archive.clone();
    foreign.keys[0].principal = "2vxsx-fae".into();
    assert_eq!(foreign.encode().err(), Some(Error::IntegrityFailed));
}

#[test]
fn cuts_one_frozen_channel_with_its_keys() {
    let vectors = load("legacy-v1.json");
    let archive = opened_archive(&vectors);
    let shared = &vectors["shared"];
    let source = &shared["source"];
    let scope = Scope {
        source: source["source"].as_str().unwrap(),
        channel: source["channel"].as_u64().unwrap() as u32,
        dek_digest: source["dekDigest"].as_str().unwrap(),
        messages: (
            source["messageStart"].as_u64().unwrap() as u32,
            source["messageEnd"].as_u64().unwrap() as u32,
        ),
        digest: source["digest"].as_str().unwrap(),
    };
    let cut = scoped(&archive, &scope, 1).unwrap();
    let expected = &shared["scoped"];
    assert_eq!(cut.content_id(), expected["content_id"].as_str().unwrap());
    let objects: Vec<_> = cut
        .inventory
        .objects
        .iter()
        .map(|o| o.key.clone())
        .collect();
    assert_eq!(serde_json::to_value(objects).unwrap(), expected["objects"]);
    let gaps: Vec<_> = cut
        .inventory
        .gaps
        .iter()
        .map(|g| serde_json::json!({ "source": g.source, "code": g.code }))
        .collect();
    assert_eq!(serde_json::Value::Array(gaps), expected["gaps"]);
    for (key, want) in cut.keys.iter().zip(expected["keys"].as_array().unwrap()) {
        assert_eq!(key.purpose, want["purpose"].as_str().unwrap());
        assert_eq!(
            hex::encode(&key.parameters),
            want["parameters_hex"].as_str().unwrap()
        );
    }
    // Cutting the cut again keeps every object and key; another frozen DEK
    // is refused.
    let again = scoped(&cut, &scope, 2).unwrap();
    assert_eq!(again.inventory.objects.len(), cut.inventory.objects.len());
    assert_eq!(again.keys.len(), cut.keys.len());
    let other = Scope {
        dek_digest: &"00".repeat(32),
        ..scope.clone()
    };
    assert_eq!(
        scoped(&archive, &other, 1).err(),
        Some(Error::IntegrityFailed)
    );
}

#[test]
fn refuses_malformed_cbor_before_decoding() {
    for bytes in [
        vec![0x81; 10_000],
        vec![0x9a, 0xff, 0xff, 0xff, 0xff],
        vec![0x9f, 0xff],
        vec![0xa2, 0x61, 0x78, 1, 0x61, 0x78, 2],
        vec![0xc2, 0x41, 1],
        vec![0xf9, 0, 0],
    ] {
        assert!(
            unpack(&bytes, MAX_ARCHIVE).is_err(),
            "{}",
            hex::encode(&bytes)
        );
    }
}
