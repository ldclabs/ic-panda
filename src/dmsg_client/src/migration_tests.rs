use super::*;
use crate::crypto::{Key, hpke_seal_info};
use crate::testing::{TempDir, unlocked};
use crate::vectors::load;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

fn unb64url(text: &serde_json::Value) -> Vec<u8> {
    URL_SAFE_NO_PAD.decode(text.as_str().unwrap()).unwrap()
}

/// The vector offer as accepted by this device at the vector's time.
fn vector_pair(client: &Client, vectors: &serde_json::Value) -> Offer {
    let transfer = &vectors["transfer"];
    let offer: Offer = serde_json::from_value(transfer["offer"].clone()).unwrap();
    let seed: Key = hex::decode(transfer["seed_hex"].as_str().unwrap())
        .unwrap()
        .try_into()
        .unwrap();
    client
        .save_pair(&PairState {
            offer: offer.clone(),
            seed: Some(ByteArray::new(seed)),
            claimed: None,
            opened_at: Some(transfer["now"].as_u64().unwrap()),
            result: None,
        })
        .unwrap();
    offer
}

/// What the old site does: seal `plain` to an offer, one frame per 192 KiB.
fn seal_transfer(plain: &[u8], offer: &Offer) -> Vec<u8> {
    use cbor2::Value;
    #[derive(serde::Serialize)]
    struct Header<'a> {
        format: &'a str,
        offer: &'a Offer,
        transfer: &'a str,
        count: u64,
        size: u64,
    }
    let recipient: Key = hex::decode(&offer.public_key).unwrap().try_into().unwrap();
    let transfer = "ab".repeat(32);
    let count = plain.len().div_ceil(legacy::TRANSFER_CHUNK) as u64;
    let header = Header {
        format: legacy::TRANSFER,
        offer,
        transfer: &transfer,
        count,
        size: plain.len() as u64,
    };
    let mut frames = vec![];
    let mut previous = String::new();
    for (index, block) in plain.chunks(legacy::TRANSFER_CHUNK).enumerate() {
        let context = canonical(&(&header, index as u64, &previous));
        let sealed = hpke_seal_info(&recipient, block, &context).unwrap();
        let frame = Value::Map(vec![
            (
                Value::Text("index".into()),
                Value::Integer((index as u64).into()),
            ),
            (
                Value::Text("previous".into()),
                Value::Text(previous.clone()),
            ),
            (Value::Text("enc".into()), Value::Bytes(sealed.enc)),
            (
                Value::Text("ciphertext".into()),
                Value::Bytes(sealed.ciphertext),
            ),
        ]);
        previous = hex::encode(*sha256(&canonical(&frame)));
        frames.push(frame);
    }
    let mut value = cbor2::from_slice::<Value>(&canonical(&header)).unwrap();
    let Value::Map(entries) = &mut value else {
        panic!("header");
    };
    entries.push((Value::Text("frames".into()), Value::Array(frames)));
    canonical(&value)
}

#[test]
fn imports_one_archive_once_and_reads_it_offline() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let vectors = load("legacy-v1.json");
    let offer = vector_pair(&client, &vectors);
    let file = dir.path("legacy.dmsg-migration");
    std::fs::write(&file, unb64url(&vectors["transfer"]["file_b64"])).unwrap();
    let fingerprint = offer.fingerprint();
    let owner = "kw6ia-hibai-bq";
    assert_eq!(
        client
            .legacy_import(&file, &offer.nonce, &"00".repeat(32), owner)
            .err(),
        Some(Error::Forbidden)
    );
    assert_eq!(
        client
            .legacy_import(&file, &offer.nonce, &fingerprint, "2vxsx-fae")
            .err(),
        Some(Error::Forbidden)
    );
    let report = client
        .legacy_import(&file, &offer.nonce, &fingerprint, owner)
        .unwrap();
    assert_eq!(report["stage"], "partial");
    assert_eq!(report["profile"]["name"], "Old Panda");
    assert_eq!(report["total_messages"], 4);
    let id = report["id"].as_str().unwrap().to_string();
    let page = client.legacy_report(&id, 1, 1).unwrap();
    assert_eq!(page["messages"][0]["text"], "Hello from the old app");
    // The same file returns the kept archive; another file is refused.
    assert_eq!(
        client
            .legacy_import(&file, &offer.nonce, &fingerprint, owner)
            .unwrap()["id"],
        id.as_str()
    );
    let other = dir.path("other.dmsg-migration");
    std::fs::write(&other, unb64url(&vectors["transfer"]["framed"]["file_b64"])).unwrap();
    assert_eq!(
        client
            .legacy_import(&other, &offer.nonce, &fingerprint, owner)
            .err(),
        Some(Error::IdempotencyConflict)
    );
    // The archive is account content: one part with its file, one record,
    // both waiting to sync; the receiving seed is gone.
    let parts = client.workspace.store.heads(MIGRATION_PART).unwrap();
    assert_eq!(parts.len(), 1);
    let (row, _) = client.current(&parts[0].0).unwrap();
    let job: cbor2::Value = client.workspace.store.job(&row.key).unwrap().unwrap();
    let cbor2::Value::Map(fields) = job else {
        panic!("job");
    };
    assert!(
        fields
            .iter()
            .any(|(k, v)| *k == cbor2::Value::Text("file".into()) && *v != cbor2::Value::Null)
    );
    assert!(client.pair_state(&offer.nonce).unwrap().seed.is_none());
    assert_eq!(client.legacy_list().unwrap().len(), 1);
    assert!(client.legacy_pairs().unwrap().is_empty());
    // A new offer from this device, exported again with the same content,
    // keeps the same archive.
    let fresh = client.legacy_pair("https://panda.fans").unwrap();
    assert_eq!(
        fresh["target"].as_str().unwrap(),
        hex::encode(*client.workspace.meta.device_id)
    );
    let nonce = fresh["nonce"].as_str().unwrap();
    let state = client.pair_state(nonce).unwrap();
    let (_, plain) = client.kept_archive(&id).unwrap();
    let mut again = Archive::decode(&plain).unwrap();
    again.created_at += 1000;
    for object in &mut again.inventory.objects {
        object.observed_at += 1000;
    }
    let resealed = dir.path("again.dmsg-migration");
    std::fs::write(
        &resealed,
        seal_transfer(&again.encode().unwrap(), &state.offer),
    )
    .unwrap();
    let report = client
        .legacy_import(&resealed, nonce, &state.offer.fingerprint(), owner)
        .unwrap();
    assert_eq!(report["id"], id.as_str());
    assert_eq!(client.legacy_list().unwrap().len(), 1);
    // Old attachments come back verified.
    let out = dir.path("notes.txt");
    let written = client
        .legacy_file(&id, "532er-faaaa-aaaaj-qncpa-cai/file/7", &out)
        .unwrap();
    assert_eq!(written["name"], "notes.txt");
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        "Notes kept since 2024.\n"
    );
    assert!(
        client
            .legacy_file(&id, "532er-faaaa-aaaaj-qncpa-cai/file/7", &out)
            .is_err()
    );
}

#[test]
fn imports_after_a_forty_five_minute_export() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let vectors = load("legacy-v1.json");
    let original = vector_pair(&client, &vectors);
    let state = client.pair_state(&original.nonce).unwrap();
    let archive = legacy::open_transfer(
        &unb64url(&vectors["transfer"]["file_b64"]),
        &original,
        state.seed.as_ref().unwrap(),
        state.opened_at.unwrap(),
    )
    .unwrap();
    let (offer, seed) = legacy::new_offer(
        "https://dmsg.net",
        &hex::encode(*client.workspace.meta.device_id),
        crate::now_ms() - 45 * 60_000,
    )
    .unwrap();
    client
        .save_pair(&PairState {
            offer: offer.clone(),
            seed: Some(ByteArray::new(*seed)),
            claimed: None,
            opened_at: None,
            result: None,
        })
        .unwrap();
    let file = dir.path("slow-export.dmsg-migration");
    std::fs::write(&file, seal_transfer(&archive.plain, &offer)).unwrap();
    let report = client
        .legacy_import(&file, &offer.nonce, &offer.fingerprint(), "kw6ia-hibai-bq")
        .unwrap();
    assert_eq!(report["profile"]["name"], "Old Panda");
    assert!(client.pair_state(&offer.nonce).unwrap().seed.is_none());
}

#[test]
fn offers_are_bounded_and_cancelled() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    assert!(client.legacy_pair("https://evil.example").is_err());
    let mut nonces = vec![];
    for _ in 0..MAX_PAIRS {
        let offer = client.legacy_pair("https://dmsg.net").unwrap();
        assert!(
            offer["link"]
                .as_str()
                .unwrap()
                .starts_with("https://dmsg.net/legacy#pair=")
        );
        nonces.push(offer["nonce"].as_str().unwrap().to_string());
    }
    assert_eq!(
        client.legacy_pair("https://dmsg.net").err(),
        Some(Error::QuotaExceeded)
    );
    client.legacy_cancel(&nonces[0]).unwrap();
    assert_eq!(client.legacy_pairs().unwrap().len(), MAX_PAIRS - 1);
    assert_eq!(
        client.legacy_cancel(&nonces[0]).err(),
        Some(Error::NotFound)
    );
    client.legacy_pair("https://dmsg.net").unwrap();
    // An expired offer no longer accepts a first file.
    let mut state = client.pair_state(&nonces[1]).unwrap();
    state.offer.expires_at = crate::now_ms() - 1;
    client.save_pair(&state).unwrap();
    let file = dir.path("late.dmsg-migration");
    std::fs::write(&file, seal_transfer(b"late", &state.offer)).unwrap();
    assert_eq!(
        client
            .legacy_import(&file, &nonces[1], &state.offer.fingerprint(), "aaaaa-aa")
            .err(),
        Some(Error::Expired)
    );
}
