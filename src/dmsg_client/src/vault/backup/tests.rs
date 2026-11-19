use super::*;
use crate::{
    testing::{TempDir, unlocked},
    vault::{Changes, Filter, Item},
};

#[test]
fn password_rules_count_user_visible_characters_only_when_creating() {
    for s in ["123456", "临时2026", "👨‍👩‍👧‍👦12345", "e\u{301}12345", " 1234 "] {
        assert!(wire::password(s, true).is_ok());
    }
    assert!(wire::password("👨‍👩‍👧‍👦1234", true).is_err());
    assert!(wire::password("12345", false).is_ok());
    assert_eq!(
        wire::password("e\u{301}12345", false).unwrap().as_str(),
        "é12345"
    );
}

async fn archive(c: &mut Client, path: &Path, password: &str, include: bool) -> Value {
    let id = begin(c, false).unwrap();
    let view = inventory(&c.hold(), &id).await.unwrap();
    let picks = view["inventory"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let approval = request(c, &id, &picks, include, password, password).unwrap();
    let request_id = approval["id"].as_str().unwrap();
    assert!(export(&c.hold(), &id, request_id).await.is_err());
    c.approval_respond(
        request_id,
        approval["digest"].as_str().unwrap(),
        true,
        "gui",
        true,
    )
    .await
    .unwrap();
    let result = export(&c.hold(), &id, request_id).await.unwrap();
    save(&c.hold(), &id, path).await.unwrap();
    assert!(export(&c.hold(), &id, request_id).await.is_err());
    result
}

#[tokio::test]
async fn portable_restore_preserves_secrets_shared_attachments_and_retry_receipts() {
    let from = TempDir::new();
    let to = TempDir::new();
    let mut source = unlocked(&from.0, false, None);
    let mut target = unlocked(&to.0, false, None);
    crate::testing::allow_vault_backup(&mut source);
    crate::testing::allow_vault_backup(&mut target);
    let payload = vec![0x7a; crate::files::CHUNK + 31];
    let mut f = source
        .secret_stage_start(
            "contract.bin",
            "application/octet-stream",
            payload.len() as u64,
        )
        .unwrap();
    for chunk in payload.chunks(crate::files::CHUNK) {
        source.secret_stage_part(&mut f, chunk).unwrap();
    }
    source.secret_stage_finish(&mut f, None).unwrap();
    let mut item = Item::new("wallet", "Personal wallet", 20);
    item.fields[0].value = "synthetic mnemonic for roundtrip only".into();
    item.notes = "first\nsecond".into();
    item.files = vec![f.id().into()];
    let mut second = item.clone();
    second.title = "Second".into();
    let ids = source
        .secret_import_commit(vec![item.clone(), second], vec![f], "root")
        .unwrap();
    let mut managed = Item::new("key", "dMsg internal", 1);
    managed.tags = vec!["dmsg-agent".into()];
    source.vault_item_with_id(&random_hex(), &managed).unwrap();
    let trashed = source
        .vault_create(
            "note",
            &Changes {
                title: Some("trash".into()),
                ..Default::default()
            },
        )
        .unwrap();
    source.vault_delete(&trashed.id).unwrap();
    let path = from.0.join("backup.dmsgvault");
    let saved = archive(&mut source, &path, "123456", true).await;
    assert_eq!(
        (saved["items"].as_u64(), saved["files"].as_u64()),
        (Some(2), Some(1))
    );
    let raw = std::fs::read(&path).unwrap();
    assert!(!raw.windows(9).any(|w| w == b"synthetic"));
    let token = begin(&mut target, true).unwrap();
    assert!(inspect(&target.hold(), &token, &path, "bad").await.is_err());
    let preview = inspect(&target.hold(), &token, &path, "123456")
        .await
        .unwrap();
    assert!(!preview.to_string().contains("mnemonic"));
    let digest = preview["digest"].as_str().unwrap();
    let result = commit(&mut target, &token, digest, vec![0, 1], "root").unwrap();
    assert_eq!(receipt(&target, &token).unwrap(), result);
    assert_eq!(
        commit(&mut target, &token, digest, vec![1, 0], "root").unwrap(),
        result
    );
    assert!(commit(&mut target, &token, digest, vec![0], "root").is_err());
    let rows = target.vault_list(&Filter::default()).unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| !ids.contains(&r.id)
        && r.item.fields == item.fields
        && r.item.notes == item.notes));
    assert_eq!(rows[0].item.files, rows[1].item.files);
    assert_ne!(rows[0].item.files, item.files);
    let fresh = begin(&mut target, true).unwrap();
    let again = inspect(&target.hold(), &fresh, &path, "123456")
        .await
        .unwrap();
    assert!(
        again["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["status"] == "same")
    );
    target.vault_delete(&rows[0].id).unwrap();
    let fresh = begin(&mut target, true).unwrap();
    let preview = inspect(&target.hold(), &fresh, &path, "123456")
        .await
        .unwrap();
    assert!(
        preview["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["status"] == "deleted")
    );
    target.lock();
    assert!(commit(&mut target, &fresh, digest, vec![0], "root").is_err());

    let mut truncated = raw;
    truncated.pop();
    std::fs::write(&path, truncated).unwrap();
    let id = begin(&mut source, true).unwrap();
    assert!(inspect(&source.hold(), &id, &path, "123456").await.is_err());
    assert!(commit(&mut source, &id, digest, vec![0], "root").is_err());
}

#[tokio::test]
async fn corrections_and_uncommitted_receipts_never_overwrite_a_head() {
    let dir = TempDir::new();
    let mut c = unlocked(&dir.0, false, None);
    assert_eq!(begin(&mut c, true).unwrap_err(), Error::PolicyStale);
    crate::testing::allow_vault_backup(&mut c);
    let path = dir.0.join("reference.dmsgvault");
    std::fs::write(
        &path,
        include_bytes!("../../../../../tests/vectors/client/vault-backup-v1.dmsgvault"),
    )
    .unwrap();
    let id = begin(&mut c, true).unwrap();
    let data = inspect(&c.hold(), &id, &path, "123456").await.unwrap();
    let digest = data["digest"].as_str().unwrap();
    let row = &data["items"][0];
    let changed = c
        .vault_create(
            "login",
            &Changes {
                title: Some(row["title"].as_str().unwrap().into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        commit(&mut c, &id, digest, vec![0], "root").unwrap_err(),
        Error::VersionConflict
    );
    assert!(receipt(&c, &id).unwrap().is_null());
    let updated = inspect(&c.hold(), &id, &path, "123456").await.unwrap();
    assert_eq!(updated["items"][0]["status"], "changed");
    commit(&mut c, &id, digest, vec![0], "root").unwrap();
    assert_eq!(
        c.vault_show(&changed.id).unwrap().revision,
        changed.revision
    );
    let next = begin(&mut c, true).unwrap();
    inspect(&c.hold(), &next, &path, "123456").await.unwrap();
    assert!(inspect(&c.hold(), &next, &path, "wrong!").await.is_err());
    assert!(commit(&mut c, &next, digest, vec![1], "root").is_err());
    cancel(&mut c, &next);
    assert_eq!(progress(&mut c, &next).unwrap()["state"], "uncommitted");
}

/// Run the same signed/approved local export and import entry points on the
/// desktop host, iOS Simulator and Android emulator. Only synthetic fixtures
/// reach the directory explicitly supplied by the operator.
#[tokio::test]
#[ignore = "requires archives exchanged between three target runtimes"]
async fn exchange_platform_backups() {
    let folder = std::path::PathBuf::from(
        std::env::var_os("DMSG_VAULT_INTEROP_DIR").expect("fixture directory"),
    );
    let platform = match std::env::consts::OS {
        "ios" => "ios",
        "android" => "android",
        _ => "desktop",
    };
    let data = include_bytes!("../../../../../tests/vectors/client/vault-backup-v1.dmsgvault");
    let phase = std::env::var("DMSG_VAULT_INTEROP_PHASE").expect("export or import");
    let inputs = match phase.as_str() {
        "export" => vec!["reference"],
        "import" => vec!["desktop", "ios", "android"],
        _ => panic!("expected export or import"),
    };
    for name in inputs {
        let home = TempDir::new();
        let mut c = unlocked(&home.0, false, None);
        crate::testing::allow_vault_backup(&mut c);
        let input = if name == "reference" {
            let path = home.0.join("reference.dmsgvault");
            std::fs::write(&path, data).unwrap();
            path
        } else {
            folder.join(format!("{name}.dmsgvault"))
        };
        let id = begin(&mut c, true).unwrap();
        let review = inspect(&c.hold(), &id, &input, "123456").await.unwrap();
        let digest = review["digest"].as_str().unwrap();
        let result = commit(&mut c, &id, digest, vec![0, 1], "root").unwrap();
        assert_eq!(result["files"], 1);
        let entries = c.vault_list(&Filter::default()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].item.files, entries[1].item.files);
        let wallet = entries.iter().find(|e| e.item.kind == "wallet").unwrap();
        assert_eq!(
            wallet.item.value("phrase"),
            "synthetic non-wallet test words"
        );
        let file = &wallet.item.files[0];
        let version = c.files_show(file).unwrap().file.current;
        assert_eq!(
            c.files_chunk(file, &version, 0).unwrap().as_slice(),
            b"Portable attachment for tests only.\n"
        );
        if phase == "export" {
            archive(
                &mut c,
                &folder.join(format!("{platform}.dmsgvault")),
                "123456",
                true,
            )
            .await;
        }
    }
}
