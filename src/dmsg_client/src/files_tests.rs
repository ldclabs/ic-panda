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
        let aad = chunk_aad(&manifest, index, count, length);
        assert_eq!(canonical(&aad), bytes(&case["aad"]["hex"]));
        assert_eq!(nonce(index).to_vec(), bytes(&case["nonce_hex"]));
        let sealed = seal_with_nonce(
            &file_key,
            &bytes(&case["plaintext_hex"]),
            &aad,
            &nonce(index),
        );
        assert_eq!(sealed, text(&case["ciphertext"]));
        assert_eq!(
            hex::encode(*sha256(&unb64(&sealed).unwrap())),
            text(&case["digest"])
        );
    }
    for case in v["chunk_rules"].as_array().unwrap() {
        let index = case["index"].as_u64().unwrap();
        let rule = FileManifest {
            id: text(&case["file_id"]).into(),
            version: text(&case["version"]).into(),
            ..manifest.clone()
        };
        let aad = chunk_aad(
            &rule,
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
        assert_eq!(sealed, text(&case["sealed"]));
    }
    assert_eq!(empty_file(), bytes(&v["empty_file_hex"]));
}

#[test]
fn reads_exported_files() {
    let v = load("content-v1.json");
    let dir = TempDir::new();
    let client = unlocked(&dir.0, false, None);
    let manifest = manifest_of(&v, "file entry");
    for case in v["file_chunks"].as_array().unwrap() {
        let sealed = unb64(text(&case["ciphertext"])).unwrap();
        client
            .workspace
            .store
            .put_chunk(text(&case["digest"]), &sealed)
            .unwrap();
    }
    let mut out = vec![];
    client.read_file(&manifest, Some(&mut out)).unwrap();
    assert_eq!(
        out,
        bytes(&v["file_chunks"][0]["plaintext_hex"])[..manifest.size as usize]
    );
    client
        .read_file(&manifest_of(&v, "empty file entry"), None)
        .unwrap();

    let mut wrong = manifest.clone();
    wrong.sha256 = "00".repeat(32);
    assert_eq!(
        client.read_file(&wrong, None).err(),
        Some(Error::IntegrityFailed)
    );
    let mut missing = manifest.clone();
    missing.chunks[0].digest = "11".repeat(32);
    assert_eq!(
        client.read_file(&missing, None).err(),
        Some(Error::RecoveryIncomplete)
    );
    let mut moved = manifest;
    moved.version = "22".repeat(32);
    assert_eq!(
        client.read_file(&moved, None).err(),
        Some(Error::IntegrityFailed)
    );
}

#[test]
fn imports_and_exports_files() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let data: Vec<u8> = (0..(2 * CHUNK + 1234)).map(|i| (i % 251) as u8).collect();
    let path = dir.path("data.bin");
    std::fs::write(&path, &data).unwrap();
    let entry = client.file_import(&path).unwrap();
    let file = entry.item.file.clone().unwrap();
    assert_eq!(file.size, data.len() as u64);
    assert_eq!(file.chunks.len(), 3);
    assert_eq!(file.sha256, hex::encode(*sha256(&data)));
    assert_eq!(file.name, "data.bin");
    // The job uploads the encrypted chunks and the vault object.
    let job: crate::sync::ContentJob = client
        .workspace
        .store
        .job(&format!("{}:{}", entry.id, entry.revision))
        .unwrap()
        .unwrap();
    let json = serde_json::to_value(job).unwrap();
    assert_eq!(json["file"]["plan"]["chunks"].as_array().unwrap().len(), 3);

    let out = dir.path("copy.bin");
    let checked = client.file_export(&entry.id, &out).unwrap();
    assert_eq!(checked.key, "");
    assert_eq!(std::fs::read(&out).unwrap(), data);
    assert!(client.file_export(&entry.id, &out).is_err());
    assert!(client
        .file_export(&entry.id, Path::new("relative"))
        .is_err());
    client.file_verify(&entry.id).unwrap();

    let empty = dir.path("empty");
    std::fs::write(&empty, b"").unwrap();
    let entry = client.file_import(&empty).unwrap();
    assert!(entry.item.file.as_ref().unwrap().chunks.is_empty());
    let out = dir.path("empty.out");
    client.file_export(&entry.id, &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), b"");

    let big = dir.path("big");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(MAX_FILE + 1)
        .unwrap();
    assert_eq!(client.file_import(&big).err(), Some(Error::QuotaExceeded));
}

#[test]
fn an_interrupted_import_resumes_only_on_the_same_bytes() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let data: Vec<u8> = (0..(CHUNK + 10)).map(|i| (i % 7) as u8).collect();
    let path = dir.path("resume.bin");
    std::fs::write(&path, &data).unwrap();

    // What an import leaves after its first chunk.
    let mut job = FileJob {
        manifest: FileManifest {
            id: random_hex(),
            version: random_hex(),
            name: "resume.bin".into(),
            mime: "application/octet-stream".into(),
            size: data.len() as u64,
            sha256: String::new(),
            chunks: vec![],
            key: b64(&[3; 32]),
        },
        plain: vec![],
    };
    let sealed = unb64(&seal_with_nonce(
        &[3; 32],
        &data[..CHUNK],
        &chunk_aad(&job.manifest, 0, 2, CHUNK as u64),
        &nonce(0),
    ))
    .unwrap();
    let digest = hex::encode(*sha256(&sealed));
    client.workspace.store.put_chunk(&digest, &sealed).unwrap();
    job.manifest.chunks.push(FileChunk {
        id: format!("{}:{}:0", job.manifest.id, job.manifest.version),
        digest: digest.clone(),
        size: CHUNK as u64,
    });
    job.plain.push(hex::encode(*sha256(&data[..CHUNK])));
    let job_key = format!("{JOB}{}", job.manifest.version);
    let keys = client.keys.take().unwrap();
    client.workspace.put_private(&keys, &job_key, &job).unwrap();
    client.keys = Some(keys);

    // A changed file is refused; the job stays for the original bytes.
    let mut changed = data.clone();
    changed[5] ^= 1;
    std::fs::write(&path, &changed).unwrap();
    assert_eq!(
        client.file_import(&path).err(),
        Some(Error::IntegrityFailed)
    );
    std::fs::write(&path, &data).unwrap();
    let entry = client.file_import(&path).unwrap();
    let file = entry.item.file.unwrap();
    assert_eq!(file.id, job.manifest.id);
    assert_eq!(file.chunks[0].digest, digest);
    assert_eq!(file.sha256, hex::encode(*sha256(&data)));
    assert!(client.workspace.store.sealed(&job_key).unwrap().is_none());
    let out = dir.path("resumed.out");
    client.file_export(&entry.id, &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), data);
}
