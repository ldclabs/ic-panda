use super::*;
use crate::codec::canonical;
use crate::content::empty_file;
use crate::testing::{unlocked, TempDir};
use crate::vectors::{bytes, field, key, load, text, value};

fn manifest_of(v: &serde_json::Value, name: &str) -> FileManifest {
    let case = v["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == name)
        .unwrap();
    let file = field(&value(&case["payload"]), "file").clone();
    crate::codec::decode(&canonical(&file), 65_536).unwrap()
}

#[test]
fn reproduces_file_chunks() {
    let v = load("content-v1.json");
    let manifest = manifest_of(&v, "file entry");
    let file_key = manifest.file_key().unwrap();
    let count = manifest.chunks.len() as u64;
    for case in v["file_chunks"].as_array().unwrap() {
        let index = case["index"].as_u64().unwrap();
        let length = manifest.chunks[index as usize].size;
        let aad = chunk_aad(&manifest.id, &manifest.version, index, count, length);
        assert_eq!(canonical(&aad), bytes(&case["aad"]["hex"]));
        assert_eq!(nonce(index).to_vec(), bytes(&case["nonce_hex"]));
        let sealed = seal_with_nonce(
            &file_key,
            &bytes(&case["plaintext_hex"]),
            &aad,
            &nonce(index),
        );
        assert_eq!(b64(&sealed), text(&case["ciphertext"]));
        assert_eq!(hex::encode(*sha256(&sealed)), text(&case["digest"]));
        let (bytes_sealed, digest) = seal_chunk(
            &file_key,
            &manifest.id,
            &manifest.version,
            index,
            count,
            &bytes(&case["plaintext_hex"]),
        );
        assert_eq!(b64(&bytes_sealed), text(&case["ciphertext"]));
        assert_eq!(digest, text(&case["digest"]));
    }
    for case in v["chunk_rules"].as_array().unwrap() {
        let index = case["index"].as_u64().unwrap();
        let aad = chunk_aad(
            text(&case["file_id"]),
            text(&case["version"]),
            index,
            case["count"].as_u64().unwrap(),
            case["plaintext_length"].as_u64().unwrap(),
        );
        assert_eq!(canonical(&aad), bytes(&case["aad"]["hex"]));
        assert_eq!(nonce(index).to_vec(), bytes(&case["nonce_hex"]));
        let sealed = seal_with_nonce(
            &key(&case["key_hex"]),
            &bytes(&case["plaintext_hex"]),
            &aad,
            &nonce(index),
        );
        assert_eq!(b64(&sealed), text(&case["sealed"]));
    }
    assert_eq!(empty_file(), bytes(&v["empty_file_hex"]));
}

#[test]
fn reads_exported_files() {
    let v = load("content-v1.json");
    let dir = TempDir::new();
    let client = unlocked(&dir.0, false, None);
    let store = &client.workspace.store;
    let manifest = manifest_of(&v, "file entry");
    for case in v["file_chunks"].as_array().unwrap() {
        let sealed = unb64(text(&case["ciphertext"])).unwrap();
        store.put_chunk(text(&case["digest"]), &sealed).unwrap();
    }
    let mut out = vec![];
    read_file(store, &manifest, MAX_FILE, Some(&mut out)).unwrap();
    assert_eq!(
        out,
        bytes(&v["file_chunks"][0]["plaintext_hex"])[..manifest.size as usize]
    );
    read_file(store, &manifest_of(&v, "empty file entry"), MAX_FILE, None).unwrap();

    let mut wrong = manifest.clone();
    wrong.sha256 = "00".repeat(32);
    assert_eq!(
        read_file(store, &wrong, MAX_FILE, None).err(),
        Some(Error::IntegrityFailed)
    );
    let mut missing = manifest.clone();
    missing.chunks[0].digest = "11".repeat(32);
    assert_eq!(
        read_file(store, &missing, MAX_FILE, None).err(),
        Some(Error::RecoveryIncomplete)
    );
    let mut moved = manifest.clone();
    moved.version = "22".repeat(32);
    assert_eq!(
        read_file(store, &moved, MAX_FILE, None).err(),
        Some(Error::IntegrityFailed)
    );
    // A bound below the file's size refuses it before any chunk is read.
    assert_eq!(
        read_file(store, &manifest, manifest.size - 1, None).err(),
        Some(Error::IntegrityFailed)
    );

    // The same chunks exported through the library's blob directory.
    let blobs = client.blobs().unwrap();
    for case in v["file_chunks"].as_array().unwrap() {
        let sealed = unb64(text(&case["ciphertext"])).unwrap();
        blobs.put_chunk(text(&case["digest"]), &sealed).unwrap();
    }
    let out = dir.path("vector.bin");
    let checked = export_manifest(blobs, &manifest, MAX_FILE, &out).unwrap();
    assert_eq!(checked.key, "");
    assert_eq!(
        std::fs::read(&out).unwrap(),
        bytes(&v["file_chunks"][0]["plaintext_hex"])[..manifest.size as usize]
    );
    assert!(export_manifest(blobs, &manifest, MAX_FILE, &out).is_err());
    assert!(export_manifest(blobs, &manifest, MAX_FILE, Path::new("relative")).is_err());
}

#[test]
fn stores_in_memory_files() {
    let dir = TempDir::new();
    let client = unlocked(&dir.0, false, None);
    let data: Vec<u8> = (0..(2 * CHUNK + 1234)).map(|i| (i % 251) as u8).collect();
    let manifest = store_file(
        &client.workspace.store,
        "data.bin",
        "application/octet-stream",
        &data,
    )
    .unwrap();
    assert_eq!(manifest.chunks.len(), 3);
    assert_eq!(manifest.sha256, hex::encode(*sha256(&data)));
    let mut out = vec![];
    read_file(&client.workspace.store, &manifest, MAX_FILE, Some(&mut out)).unwrap();
    assert_eq!(out, data);
    let empty = store_file(&client.workspace.store, "empty", "text/plain", b"").unwrap();
    assert!(empty.chunks.is_empty());
    read_file(&client.workspace.store, &empty, MAX_FILE, None).unwrap();
    let big = vec![0u8; MAX_FILE as usize + 1];
    assert_eq!(
        store_file(
            &client.workspace.store,
            "big",
            "application/octet-stream",
            &big
        )
        .err(),
        Some(Error::QuotaExceeded)
    );
}
