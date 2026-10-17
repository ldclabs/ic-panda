use super::*;
use crate::codec::{decode_value, unb64};
use crate::files::FileChunk;
use crate::upload::ChunkRef;
use crate::vault::Item;
use crate::vectors::{bytes, key, load, text, value};

#[test]
fn opens_exported_objects() {
    let v = load("content-v1.json");
    let environment = text(&v["environment"]);
    let roots = RootKeys::single(v["root_generation"].as_u64().unwrap(), key(&v["root_hex"]));
    for case in v["objects"].as_array().unwrap() {
        let record: EncryptedObject = serde_json::from_value(case["record"].clone()).unwrap();
        let plaintext = record.open(environment, &roots).unwrap();
        assert_eq!(
            *plaintext,
            bytes(&case["plaintext_hex"]),
            "{}",
            case["name"]
        );
        assert_eq!(object_container(&record), bytes(&case["object_hex"]));
        assert_eq!(read_container(&bytes(&case["object_hex"])).unwrap(), record);
        // Items re-encode byte for byte. The file entries stay as object
        // and chunk vectors; the vault has no file items any more.
        if !text(&case["name"]).contains("file") {
            let item: Item = decode(&plaintext, MAX_OBJECT).unwrap();
            item.validate().unwrap();
            assert_eq!(canonical(&item), *plaintext);
            assert_eq!(canonical(&item), canonical(&value(&case["payload"])));
        }
        let mut changed = record.clone();
        changed.tombstone = !changed.tombstone;
        assert_eq!(
            changed.open(environment, &roots).err(),
            Some(Error::IntegrityFailed)
        );
        let other = RootKeys::single(record.generation + 1, [1; 32]);
        assert_eq!(
            record.open(environment, &other).err(),
            Some(Error::RecoveryIncomplete)
        );
    }
}

#[test]
fn opens_exported_manifests() {
    let v = load("content-v1.json");
    let account = text(&v["account"]);
    let root = key(&v["root_hex"]);
    let mut kinds = vec![];
    for case in v["uploads"].as_array().unwrap() {
        let plan: UploadPlan = serde_json::from_value(case["plan"].clone()).unwrap();
        let manifest = unb64(text(&case["manifest"])).unwrap();
        let content = open_manifest(&root, account, &plan, &manifest).unwrap();
        match &content {
            Opened::File(file) => {
                assert_eq!(plan.kind, "file");
                assert_eq!(canonical(file), canonical(&value(&case["content"])));
            }
            Opened::Backup(_) => panic!("no backup vectors here"),
            Opened::None => {
                assert_eq!(plan.kind, VAULT);
                let object = unb64(text(&case["object"])).unwrap();
                assert_eq!(hex::encode(*sha256(&object)), plan.chunks[0].digest);
                read_container(&object).unwrap();
            }
        }
        // The manifest struct re-encodes the plaintext byte for byte.
        let plain = open(
            &root,
            text(&case["manifest"]),
            &manifest_context(account, &plan),
        )
        .unwrap();
        assert_eq!(*plain, bytes(&case["manifest_plaintext_hex"]));
        let parsed: Manifest = decode(&plain, MAX_MANIFEST).unwrap();
        assert_eq!(canonical(&parsed), *plain);
        assert_eq!(
            decode_value(&plain, MAX_MANIFEST).unwrap(),
            decode_value(&canonical(&parsed), MAX_MANIFEST).unwrap()
        );
        let mut moved = plan.clone();
        moved.root_generation += 1;
        assert!(open_manifest(&root, account, &moved, &manifest).is_err());
        kinds.push(plan.kind);
    }
    assert_eq!(kinds, ["vault", "file", "vault", "file", "vault"]);
}

#[test]
fn seals_objects_and_manifests_that_open() {
    let account = "040g2081040g2081040g";
    let roots = RootKeys::single(2, [5; 32]);
    let payload = b"payload".to_vec();
    let record = EncryptedObject::seal(
        "local",
        (2, &[5; 32]),
        account,
        &"07".repeat(32),
        VAULT,
        "ab".repeat(32),
        None,
        false,
        &payload,
        9,
    );
    assert_eq!(record.key, format!("{}:{}", record.id, record.revision));
    assert_eq!(*record.open("local", &roots).unwrap(), payload);
    assert_eq!(
        record.open("staging", &roots).err(),
        Some(Error::IntegrityFailed)
    );

    let object = object_container(&record);
    let mut plan = UploadPlan {
        upload_id: random_hex(),
        object_id: record.id.clone(),
        version_id: record.revision.clone(),
        root_generation: 2,
        epoch: 0,
        control_head: None,
        kind: VAULT.into(),
        chunks: vec![ChunkRef {
            digest: hex::encode(*sha256(&object)),
            size: object.len() as u64,
        }],
        manifest_digest: String::new(),
        manifest_size: 0,
        expires_at: 1,
    };
    let manifest = seal_manifest(&[5; 32], account, &mut plan, ManifestContent::None);
    assert_eq!(
        open_manifest(&[5; 32], account, &plan, &manifest).unwrap(),
        Opened::None
    );
    assert!(open_manifest(&[5; 32], account, &plan, &manifest[1..]).is_err());

    // A file upload carries its manifest, and only a file upload may.
    let file = FileManifest {
        id: random_hex(),
        version: random_hex(),
        name: "a.txt".into(),
        mime: "text/plain".into(),
        size: 3,
        sha256: hex::encode(*sha256(b"abc")),
        chunks: vec![FileChunk {
            id: "x".into(),
            digest: "00".repeat(32),
            size: 3,
        }],
        key: crate::codec::b64(&[1; 32]),
    };
    let mut file_plan = UploadPlan {
        upload_id: random_hex(),
        object_id: file.id.clone(),
        version_id: file.version.clone(),
        kind: "file".into(),
        ..plan.clone()
    };
    let sealed = seal_manifest(
        &[5; 32],
        account,
        &mut file_plan,
        ManifestContent::File(&file),
    );
    assert_eq!(
        open_manifest(&[5; 32], account, &file_plan, &sealed).unwrap(),
        Opened::File(file.clone())
    );
    let mut vault_plan = plan.clone();
    let wrong = seal_manifest(
        &[5; 32],
        account,
        &mut vault_plan,
        ManifestContent::File(&file),
    );
    assert_eq!(
        open_manifest(&[5; 32], account, &vault_plan, &wrong).err(),
        Some(Error::IntegrityFailed)
    );
}

#[test]
fn opens_backup_manifests() {
    use crate::library::backup::{BACKUP, BACKUP_FORMAT, BackupManifest};
    let v = load("content-v1.json");
    let account = text(&v["account"]);
    let root = key(&v["root_hex"]);
    for case in v["backups"].as_array().unwrap() {
        let plan: UploadPlan = serde_json::from_value(case["plan"].clone()).unwrap();
        assert_eq!(plan.kind, BACKUP);
        assert_eq!(
            canonical(&manifest_context(account, &plan)),
            bytes(&case["manifest_aad"])
        );
        let manifest = unb64(text(&case["manifest"])).unwrap();
        let Opened::Backup(record) = open_manifest(&root, account, &plan, &manifest).unwrap()
        else {
            panic!("a backup manifest");
        };
        assert_eq!(record.format, BACKUP_FORMAT);
        assert_eq!(canonical(&record), bytes(&case["content_plaintext_hex"]));
        let typed: BackupManifest =
            decode(&bytes(&case["content_plaintext_hex"]), MAX_MANIFEST).unwrap();
        assert_eq!(typed, record);
        // The file inside is the file entry's, so its chunks are the file
        // chunk vectors, sealed under the same key.
        let file_case = v["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["name"] == "file entry")
            .unwrap();
        let file = crate::vectors::field(&value(&file_case["payload"]), "file").clone();
        assert_eq!(canonical(&record.file), canonical(&file));
        assert_eq!(record.path, ["docs", "2026"]);
        let plain = open(
            &root,
            text(&case["manifest"]),
            &manifest_context(account, &plan),
        )
        .unwrap();
        assert_eq!(*plain, bytes(&case["manifest_plaintext_hex"]));
        let parsed: Manifest = decode(&plain, MAX_MANIFEST).unwrap();
        assert_eq!(canonical(&parsed), *plain);
        // The same bytes under another kind do not open as a backup.
        let mut as_file = plan.clone();
        as_file.kind = "file".into();
        assert!(open_manifest(&root, account, &as_file, &manifest).is_err());
    }
}
