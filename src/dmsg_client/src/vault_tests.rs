use super::*;
use crate::vectors::{bytes, key, load, text};

fn item(title: &str) -> Item {
    Item {
        kind: "note".into(),
        title: title.into(),
        tags: vec![],
        body: "body".into(),
        username: String::new(),
        secret: "s3cret".into(),
        url: String::new(),
        favorite: false,
        created_at: 1,
        updated_at: 1,
        resolved_conflicts: None,
    }
}

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
        if case["payload"]["map"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e[0]["text"] != "file")
        {
            let item: Item = decode(&plaintext, MAX_OBJECT).unwrap();
            assert_eq!(canonical(&item), *plaintext);
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
fn opens_exported_vault_manifests() {
    let v = load("content-v1.json");
    let account = text(&v["account"]);
    let root = key(&v["root_hex"]);
    let mut opened = 0;
    for case in v["uploads"].as_array().unwrap() {
        let plan: UploadPlan = serde_json::from_value(case["plan"].clone()).unwrap();
        if plan.kind != VAULT {
            continue;
        }
        let manifest = unb64(text(&case["manifest"])).unwrap();
        open_manifest(&root, account, &plan, &manifest).unwrap();
        let object = unb64(text(&case["object"])).unwrap();
        assert_eq!(hex::encode(*sha256(&object)), plan.chunks[0].digest);
        read_container(&object).unwrap();
        let mut moved = plan.clone();
        moved.root_generation += 1;
        assert!(open_manifest(&root, account, &moved, &manifest).is_err());
        opened += 1;
    }
    assert_eq!(opened, 3);
}

#[test]
fn seals_objects_and_manifests_that_open() {
    let roots = RootKeys::single(2, [5; 32]);
    let payload = canonical(&item("hello"));
    let record = EncryptedObject::seal(
        "local",
        (2, &[5; 32]),
        "040g2081040g2081040g",
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
    let manifest = seal_manifest(&[5; 32], "040g2081040g2081040g", &mut plan);
    open_manifest(&[5; 32], "040g2081040g2081040g", &plan, &manifest).unwrap();
    assert!(open_manifest(&[5; 32], "040g2081040g2081040g", &plan, &manifest[1..]).is_err());
}

#[test]
fn items_have_limits() {
    assert!(item("x").validate().is_ok());
    assert!(item(" ").validate().is_err());
    assert!(item(&"x".repeat(201)).validate().is_err());
    let mut big = item("x");
    big.body = "x".repeat(20_000);
    big.secret = "y".repeat(10_000);
    assert!(big.validate().is_ok());
    big.body = "汉".repeat(12_000);
    assert_eq!(big.validate(), Err(Error::QuotaExceeded));
    let mut kind = item("x");
    kind.kind = "file".into();
    assert!(kind.validate().is_err());
}
