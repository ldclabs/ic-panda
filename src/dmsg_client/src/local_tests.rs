use super::*;
use crate::{
    testing::{TempDir, config},
    vault::Changes,
};

#[test]
fn ending_a_creation_resumes_only_the_current_service_transport() {
    let dir = TempDir::new();
    let mut c = Client::open(config(false), &dir.0, None).unwrap();
    let old_transport = c.relay();
    let first = c.prepare_local_create().unwrap();
    assert!(c.offline_mode());
    drop(first);
    assert!(!c.offline_mode());
    assert!(!old_transport.network_enabled());

    let interrupted = c.prepare_local_create().unwrap();
    c.lock();
    assert!(!c.offline_mode());
    let retry = c.prepare_local_create().unwrap();
    drop(interrupted);
    assert!(c.offline_mode());
    assert!(retry.authenticate(Some("short")).is_err());
    assert!(!c.offline_mode());
}

#[test]
fn standalone_workspace_survives_restart_without_identity_or_outbox() {
    let _fixture = crate::testing::PASSWORD_FIXTURE.blocking_lock();
    let dir = TempDir::new();
    let mut c = Client::open(config(true), &dir.0, None).unwrap();
    assert!(matches!(
        c.switch_account(&LOCAL_ACCOUNT_ID),
        Err(Error::NotFound)
    ));
    let password = "only in this workspace";
    let previous_transport = c.relay();
    let request = c.prepare_local_create().unwrap();
    let status = c
        .complete_local_create(request.authenticate(Some(password)).unwrap())
        .unwrap();
    assert_eq!(status.account_kind, "local");
    assert_eq!(status.account.as_deref(), Some("00000000000000000000"));
    assert!(status.home.is_none() && status.principal.is_none() && !status.background_online);
    let entry = c
        .vault_create(
            "note",
            &Changes {
                title: Some("test note".into()),
                ..Default::default()
            },
        )
        .unwrap();
    let input = dir.path("example.txt");
    std::fs::write(&input, b"private contents").unwrap();
    let file = c
        .files_import(&input, "root", true, &mut |_, _| {})
        .unwrap();
    assert!(!entry.synced && entry.save_state == "local_only");
    assert_eq!(c.workspace.store.count_in(crate::store::LOCAL).unwrap(), 0);
    assert!(
        c.workspace
            .store
            .meta::<ByteArray<32>>("provisional")
            .unwrap()
            .is_none()
    );
    assert!(c.account().is_err());
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let blocked: Result<()> = previous_transport
            .while_online(async { panic!("stale transport started") })
            .await;
        assert_eq!(blocked, Err(only()));
        let target = crate::testing::account().id;
        assert_eq!(
            crate::principal::resolve_account(&c.hold(), &target)
                .await
                .unwrap_err(),
            only()
        );
        assert_eq!(
            crate::principal::resolve(&c.config, &LOCAL_ACCOUNT_ID)
                .await
                .unwrap_err(),
            only()
        );
        assert!(
            matches!(crate::principal::names(&c.hold(), &[target]).await, Err(e) if e == only())
        );
    });
    c.lock();
    assert!(c.signing().is_err() && c.vault_show(&entry.id).is_err());
    drop(c);
    let mut c = Client::open(config(true), &dir.0, None).unwrap();
    assert_eq!(c.status().unwrap().state, "locked");
    let q = c.prepare_local_unlock().unwrap();
    let bad = q.authenticate(Some("another password")).unwrap();
    assert_eq!(
        c.complete_local_unlock(bad).unwrap_err(),
        dmsg_types::invalid("LOCAL_UNLOCK_FAILED")
    );
    let q = c.prepare_local_unlock().unwrap();
    let stale = q.authenticate(Some(password)).unwrap();
    c.lock();
    assert_eq!(c.complete_local_unlock(stale).unwrap_err(), Error::Locked);
    let q = c.prepare_local_unlock().unwrap();
    c.complete_local_unlock(q.authenticate(Some(password)).unwrap())
        .unwrap();
    assert_eq!(c.vault_show(&entry.id).unwrap().item.title, "test note");
    let output = dir.path("out.txt");
    c.files_export(&file.file.id, None, &output).unwrap();
    assert_eq!(std::fs::read(output).unwrap(), b"private contents");
    assert_eq!(c.status().unwrap().pending_sync, 0);
    let q = c.prepare_local_unlock().unwrap();
    let change = q
        .authenticate(Some(password))
        .unwrap()
        .change_password("my replacement password")
        .unwrap();
    c.complete_password_change(change).unwrap();
    let second = c
        .vault_create(
            "note",
            &Changes {
                title: Some("another note".into()),
                files: Some(vec![file.file.id.clone()]),
                ..Default::default()
            },
        )
        .unwrap();
    let discarded_input = dir.path("discarded.txt");
    std::fs::write(&discarded_input, b"discarded").unwrap();
    let discarded = c
        .files_import(&discarded_input, "root", true, &mut |_, _| {})
        .unwrap()
        .file
        .id;
    c.vault_edit(
        &entry.id,
        Some(&entry.revision),
        &Changes {
            files: Some(vec![file.file.id.clone(), discarded.clone()]),
            ..Default::default()
        },
    )
    .unwrap();
    c.files_trash(&[discarded]).unwrap();
    let device = c.device_id().unwrap();
    drop(c);
    let mut target = crate::testing::unlocked(&dir.0, false, None);
    crate::testing::allow_vault_backup(&mut target);
    let notifications = target
        .notification_status(&Default::default(), now_ms())
        .unwrap();
    assert_eq!(notifications["accounts"].as_array().unwrap().len(), 1);
    assert_eq!(
        notifications["accounts"][0]["account"],
        crate::testing::account().id.to_string()
    );
    target.mark_all_notifications_read().unwrap();
    assert_eq!(
        target.with_notification_store(&LOCAL_ACCOUNT_ID, |_| Ok(())),
        Err(Error::NotFound)
    );
    let request = target.prepare_local_import().unwrap();
    let preview = crate::vault::backup::direct::preview(
        &mut target,
        request
            .authenticate(Some("my replacement password"))
            .unwrap(),
    )
    .unwrap();
    let id = preview["id"].as_str().unwrap();
    assert_eq!(preview["items"].as_array().unwrap().len(), 2);
    assert!(
        preview["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["attachments"] == serde_json::json!([0, null]))
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let copy = |c: &mut Client| {
        runtime.block_on(async {
            crate::vault::backup::direct::commit(&c.hold(), id, vec![0, 1], vec![0], "root")
                .await
                .unwrap()
        })
    };
    let result = copy(&mut target);
    assert_eq!(result["items"], 2);
    assert_eq!(result["files"], 1);
    assert_eq!(result["omitted"], 1);
    assert_eq!(copy(&mut target), result);
    let imported = target.vault_list(&Default::default()).unwrap();
    assert_eq!(imported.len(), 2);
    assert!(
        imported
            .iter()
            .all(|e| e.save_state == "pending" && e.id != entry.id && e.id != second.id)
    );
    let new_file = &imported[0].item.files[0];
    assert_ne!(*new_file, file.file.id);
    assert_eq!(imported[1].item.files[0], *new_file);
    let result_path = dir.path("restored.txt");
    target.files_export(new_file, None, &result_path).unwrap();
    assert_eq!(std::fs::read(result_path).unwrap(), b"private contents");
    target.switch_account(&LOCAL_ACCOUNT_ID).unwrap();
    let q = target.prepare_local_unlock().unwrap();
    target
        .complete_local_unlock(q.authenticate(Some("my replacement password")).unwrap())
        .unwrap();
    assert_eq!(target.device_id().unwrap(), device);
    assert_eq!(
        target.vault_show(&second.id).unwrap().item.files,
        vec![file.file.id]
    );
    let mut c = target;
    let old_device = c.device_id().unwrap();
    assert!(
        c.remove_local_account(&LOCAL_ACCOUNT_ID.to_string())
            .is_err()
    );
    let q = c.prepare_local_unlock().unwrap();
    c.delete_local(q.authenticate(Some("my replacement password")).unwrap())
        .unwrap();
    assert!(!dir.path(DIRECTORY).exists());
    let q = c.prepare_local_create().unwrap();
    c.complete_local_create(q.authenticate(Some(password)).unwrap())
        .unwrap();
    assert_ne!(old_device, c.device_id().unwrap());
}

/// Allocates over 640 MiB of encrypted temporary files. Run on the configured build disk.
#[tokio::test]
#[ignore = "large standalone file migration"]
async fn migrate_independent_file_larger_than_portable_archive() {
    let _fixture = crate::testing::PASSWORD_FIXTURE.lock().await;
    let dir = TempDir::new();
    let mut source = Client::open(config(false), &dir.0, None).unwrap();
    let request = source.prepare_local_create().unwrap();
    source
        .complete_local_create(
            request
                .authenticate(Some("standalone test password"))
                .unwrap(),
        )
        .unwrap();
    let file_path = dir.path("large-independent-file.bin");
    const SIZE: u64 = 321 * 1024 * 1024;
    std::fs::File::create(&file_path)
        .unwrap()
        .set_len(SIZE)
        .unwrap();
    let original = source
        .files_import(&file_path, "root", true, &mut |_, _| {})
        .unwrap();
    let (_, _, old) = source.library_manifest(&original.file.id, None).unwrap();
    drop(source);
    let mut target = crate::testing::unlocked(&dir.0, false, None);
    crate::testing::allow_vault_backup(&mut target);
    let access = target
        .prepare_local_import()
        .unwrap()
        .authenticate(Some("standalone test password"))
        .unwrap();
    let preview = crate::vault::backup::direct::preview(&mut target, access).unwrap();
    let result = crate::vault::backup::direct::commit(
        &target.cooperative_hold(),
        preview["id"].as_str().unwrap(),
        vec![],
        vec![0],
        "root",
    )
    .await
    .unwrap();
    assert_eq!(result["files"], 1);
    let files = target
        .files_search("large-independent-file.bin", &Default::default())
        .unwrap();
    assert_eq!(files.len(), 1);
    let (_, _, new) = target.library_manifest(&files[0].id, None).unwrap();
    assert_eq!((new.size, &new.sha256), (SIZE, &old.sha256));
    assert_ne!(new.id, old.id);
    assert_ne!(new.version, old.version);
    assert!(new.key != old.key);
    let output = dir.path("export.bin");
    target.files_export(&files[0].id, None, &output).unwrap();
    assert_eq!(std::fs::metadata(output).unwrap().len(), SIZE);
    assert!(target.files_backup_status().unwrap().is_empty());
}
