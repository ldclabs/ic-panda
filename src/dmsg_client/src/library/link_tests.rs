use super::*;
use crate::codec::{b64, decode, sha256, unb64};
use crate::content::MAX_MANIFEST;
use crate::crypto::{open, seal_with_nonce};
use crate::files::{FileChunk, chunk_aad, nonce};
use crate::vectors::{bytes, key, load, text};

#[test]
fn password_keys_wrap_and_unwrap_the_share_key() {
    let share_id = "6a".repeat(32);
    let key: Key = [7; 32];
    let a = password_key("correct horse", &share_id).unwrap();
    let b = password_key("correct horse", &share_id).unwrap();
    assert_eq!(&a[..], &b[..]);
    assert_ne!(
        &a[..],
        &password_key("correct horse", &"6b".repeat(32)).unwrap()[..]
    );
    assert_ne!(
        &a[..],
        &password_key("Correct horse", &share_id).unwrap()[..]
    );
    let wrap = wrap_key(&key, "correct horse", &share_id).unwrap();
    assert_eq!(
        &unwrap_key(&wrap, "correct horse", &share_id).unwrap()[..],
        &key
    );
    assert_eq!(
        unwrap_key(&wrap, "wrong", &share_id).err(),
        Some(Error::IntegrityFailed)
    );
    assert!(matches!(
        password_key("", &share_id),
        Err(Error::InvalidInput(_))
    ));
    assert_eq!(password_key("x", "zz").err(), Some(Error::IntegrityFailed));
}

#[test]
fn links_take_the_site_from_the_login_url() {
    let id = "6a".repeat(32);
    assert_eq!(
        share_url("https://dmsg.net/_/login", &id, Some("KEY")),
        format!("https://dmsg.net/s/{id}#KEY")
    );
    assert_eq!(
        share_url("http://localhost:3002/_/login", &id, None),
        format!("http://localhost:3002/s/{id}")
    );
    assert_eq!(
        share_url("https://dmsg.net", &id, None),
        format!("https://dmsg.net/s/{id}")
    );
    assert_eq!(state(10, None, 9), "active");
    assert_eq!(state(10, None, 10), "expired");
    assert_eq!(state(10, Some(3), 9), "revoked");
}

fn file_link_record() -> serde_json::Value {
    serde_json::json!({
        "file": "11".repeat(32),
        "version": "22".repeat(32),
        "name": "fixture.txt",
        "size": 7,
        "share_id": "33".repeat(32),
        "upload_id": "44".repeat(32),
        "key": b64(&[5; 32]),
        "expires_at": crate::now_ms() + DAY,
        "max_downloads": 3,
        "password": false,
        "created_at": 1,
        "revoked_at": null,
        "downloads": 1,
    })
}

#[test]
fn file_links_without_source_remain_readable_by_health() {
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    // The persisted file-link shape before Vault snapshots introduced `source`.
    let record = file_link_record();
    let share_id = record["share_id"].as_str().unwrap();
    let key = format!("{LINK}{share_id}");
    client
        .workspace
        .put_private(client.keys().unwrap(), &key, &record)
        .unwrap();
    let sealed = client.workspace.store.sealed(&key).unwrap();

    let health = client.secret_health().unwrap();
    let finding = health["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|finding| finding["rule"] == "active_share")
        .unwrap();
    assert_eq!(finding["subject"], share_id);
    assert_eq!(finding["evidence"], "cloud_state_not_refreshed");
    assert_eq!(finding["observed_at"], record["created_at"]);
    let links = client.files_share_links().unwrap();
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].file.as_deref(), record["file"].as_str());
    assert_eq!(links[0].version.as_deref(), record["version"].as_str());
    assert_eq!(links[0].downloads, 1);
    assert_eq!(links[0].max_downloads, 3);
    assert_eq!(links[0].last_checked_at, None);
    assert!(
        links[0]
            .url
            .as_ref()
            .unwrap()
            .ends_with(record["key"].as_str().unwrap())
    );
    assert_eq!(client.workspace.store.sealed(&key).unwrap(), sealed);
}

#[test]
fn link_records_reject_incomplete_ambiguous_and_unknown_sources() {
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    let original = file_link_record();
    let key = format!("{LINK}{}", original["share_id"].as_str().unwrap());
    let mut missing = original.clone();
    missing.as_object_mut().unwrap().remove("version");
    let mut wrong_type = original.clone();
    wrong_type["file"] = serde_json::Value::Null;
    let mut ambiguous = original.clone();
    ambiguous["source"] = json!({"type":"file_version","file":"other","version":"other"});
    let mut malformed = original.clone();
    malformed["source"] = serde_json::Value::Null;
    let mut unknown = original.clone();
    unknown["unexpected"] = true.into();
    for record in [missing, wrong_type, ambiguous, malformed, unknown] {
        client
            .workspace
            .put_private(client.keys().unwrap(), &key, &record)
            .unwrap();
        assert_eq!(
            client.files_share_links().err(),
            Some(Error::IntegrityFailed)
        );
        assert_eq!(client.secret_health().err(), Some(Error::IntegrityFailed));
    }
}

/// A backup on this account whose version this device's library has only as
/// a record, with the backup's chunks at hand as if just downloaded.
fn cloud_only_fixture(dir: &crate::testing::TempDir) -> (Client, String, FileManifest, String) {
    use super::super::backup::{BACKUP_FORMAT, BackupManifest};
    use super::super::catalog::{Backup, ROOT};
    use super::super::cloud::CloudEntry;

    let mut c = crate::testing::unlocked(&dir.0, false, None);
    let path = dir.path("cloud.txt");
    std::fs::write(&path, b"only in the cloud").unwrap();
    let imported = c.files_import(&path, ROOT, false, &mut |_, _| {}).unwrap();
    let id = imported.file.id;
    let (file, mut version, manifest) = c.library_manifest(&id, None).unwrap();
    let job = c
        .plan_backup(&id, &imported.version, crate::now_ms())
        .unwrap();
    let upload = job.plan.upload_id.clone();
    c.save_cloud_entry(
        &upload,
        &CloudEntry {
            plan: job.plan,
            device: version.device.clone(),
            committed_at: Some(7),
            manifest: Some(BackupManifest {
                format: BACKUP_FORMAT.into(),
                file: manifest.clone(),
                path: vec![],
                tags: file.tags,
                note: file.note,
                created_at: version.created_at,
                modified_at: file.updated_at,
                device: version.device.clone(),
            }),
        },
    )
    .unwrap();
    version.local = false;
    version.backup = Some(Backup {
        upload_id: upload.clone(),
        generation: 0,
        backed_up_at: 7,
        verified: true,
    });
    c.save_version(&id, &imported.version, &version).unwrap();
    (c, id, manifest, upload)
}

#[tokio::test]
async fn an_edition_from_a_backup_is_independent_and_leaves_nothing_behind() {
    let dir = crate::testing::TempDir::new();
    let (mut c, id, manifest, upload) = cloud_only_fixture(&dir);
    let digest = manifest.chunks[0].digest.clone();
    assert!(c.blobs().unwrap().chunk(&digest).unwrap().is_some());
    let before = c.files_list(super::super::catalog::ROOT, false).unwrap();

    let edition = prepare_edition(&c.hold(), &manifest, Some(&upload), &mut |_, _, _| {})
        .await
        .unwrap();

    // Same bytes, but neither a key nor a chunk in common with the backup.
    assert_ne!(edition.id, manifest.id);
    assert_ne!(edition.key, manifest.key);
    assert_ne!(edition.chunks[0].digest, digest);
    assert_eq!(edition.sha256, manifest.sha256);
    // The backup's copy is gone while the edition's stays for its upload.
    assert!(c.blobs().unwrap().chunk(&digest).unwrap().is_none());
    assert!(
        c.blobs()
            .unwrap()
            .chunk(&edition.chunks[0].digest)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        c.held_chunks(crate::now_ms()).unwrap(),
        [edition.chunks[0].digest.clone()].into()
    );
    // The library is as it was.
    assert_eq!(
        c.files_list(super::super::catalog::ROOT, false).unwrap(),
        before
    );
    assert!(!c.files_show(&id).unwrap().file.local);
    let plain = crate::files::read_chunk(c.blobs().unwrap(), &edition, 0).unwrap();
    assert_eq!(plain.as_slice(), b"only in the cloud");
}

#[tokio::test]
async fn sharing_a_backup_preserves_existing_download_jobs() {
    for stage in ["done", "queued", "retrying"] {
        let dir = crate::testing::TempDir::new();
        let (mut c, id, manifest, upload) = cloud_only_fixture(&dir);
        let mut job = c.files_download_start(&upload, None, false).unwrap();
        job.stage = stage.into();
        if stage == "done" {
            job.done = job.total;
            job.bytes = job.total_bytes;
        } else if stage == "retrying" {
            job.error = Some("UNAVAILABLE".into());
            job.retry_at = Some(crate::now_ms() + dmsg_types::MINUTE);
            job.attempts = 1;
        }
        c.save_fetch(&job).unwrap();
        let before = serde_json::to_value(c.files_download_status().unwrap()).unwrap();

        prepare_edition(&c.hold(), &manifest, Some(&upload), &mut |_, _, _| {})
            .await
            .unwrap();

        assert_eq!(
            serde_json::to_value(c.files_download_status().unwrap()).unwrap(),
            before,
            "sharing must not change a {stage} download"
        );
        assert!(!c.files_show(&id).unwrap().file.local);
        if stage == "done" {
            let report = super::super::fetch::run(&c.hold(), &mut |_| {})
                .await
                .unwrap();
            assert_eq!(report.completed, 0);
            assert_eq!(report.pending, 0);
            assert!(!c.files_show(&id).unwrap().file.local);
        }
    }
}

#[tokio::test]
async fn links_from_the_cloud_refuse_what_they_cannot_read_before_anything_moves() {
    let dir = crate::testing::TempDir::new();
    let (mut c, id, _, upload) = cloud_only_fixture(&dir);
    let terms = LinkTerms::default();
    let mut progress = |_: &'static str, _: u64, _: u64| {};
    assert_eq!(
        share_backup_link(&c.hold(), &"ff".repeat(32), terms, &mut progress)
            .await
            .err(),
        Some(Error::NotFound)
    );
    for bad in [
        LinkTerms {
            expires_in_ms: Some(LINK_MAX_MS + 1),
            ..terms
        },
        LinkTerms {
            max_downloads: 1_000_001,
            ..terms
        },
    ] {
        assert!(matches!(
            share_backup_link(&c.hold(), &upload, bad, &mut progress).await,
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            share_link(&c.hold(), &id, None, bad, &mut progress).await,
            Err(Error::InvalidInput(_))
        ));
    }

    // A backup past the link limit stays out, whatever the membership allowed.
    let mut entry = c.cloud_entry(&upload).unwrap().unwrap();
    entry.manifest.as_mut().unwrap().file.size = MAX_FILE + 1;
    c.save_cloud_entry(&upload, &entry).unwrap();
    assert_eq!(
        share_backup_link(&c.hold(), &upload, terms, &mut progress)
            .await
            .err(),
        Some(Error::QuotaExceeded)
    );

    // A record with neither chunks nor a backup has nothing to read from.
    let version = c.files_show(&id).unwrap().file.current;
    let mut record = c.version(&id, &version).unwrap();
    record.backup = None;
    c.save_version(&id, &version, &record).unwrap();
    assert_eq!(
        share_link(&c.hold(), &id, None, terms, &mut progress)
            .await
            .err(),
        Some(Error::RecoveryIncomplete)
    );
}

/// The share vector: a browser page that follows `content-v1.json` opens
/// the same manifest, key wrap and chunk.
#[test]
fn opens_the_share_vector() {
    let v = load("content-v1.json");
    for case in v["shares"].as_array().unwrap() {
        let share_id = text(&case["share_id"]);
        let upload_id = text(&case["upload_id"]);
        let version_id = text(&case["version_id"]);
        let key = key(&case["key_hex"]);
        let aad = share_manifest_aad(share_id, upload_id, share_id, version_id);
        assert_eq!(canonical(&aad), bytes(&case["manifest_aad"]));
        let plain = open(&key, text(&case["manifest"]), &aad).unwrap();
        assert_eq!(*plain, bytes(&case["manifest_plaintext_hex"]));
        let manifest: ShareFile = decode(&plain, MAX_MANIFEST).unwrap();
        assert_eq!(manifest.format, SHARE_FORMAT);
        assert_eq!(manifest.share_id, share_id);
        assert_eq!(manifest.file.id, share_id);
        assert_eq!(manifest.file.version, version_id);
        assert!(manifest.file.key.is_empty());
        let sealed = unb64(text(&case["manifest"])).unwrap();
        assert_eq!(
            hex::encode(*sha256(&sealed)),
            text(&case["manifest_digest"])
        );
        // The password wrap yields the same key.
        let wrap = text(&case["key_wrap"]);
        assert_eq!(
            &unwrap_key(wrap, text(&case["password"]), share_id).unwrap()[..],
            &key
        );
        // The chunks follow the chunk rules under the edition's identity.
        let count = manifest.file.chunks.len() as u64;
        let mut whole = Vec::new();
        for (index, chunk) in case["chunks"].as_array().unwrap().iter().enumerate() {
            let expected: &FileChunk = &manifest.file.chunks[index];
            let plaintext = bytes(&chunk["plaintext_hex"]);
            let aad = chunk_aad(
                share_id,
                version_id,
                index as u64,
                count,
                plaintext.len() as u64,
            );
            assert_eq!(expected.size, plaintext.len() as u64);
            let sealed = seal_with_nonce(&key, &plaintext, &aad, &nonce(index as u64));
            assert_eq!(b64(&sealed), text(&chunk["ciphertext"]));
            assert_eq!(hex::encode(*sha256(&sealed)), expected.digest);
            assert_eq!(expected.digest, text(&chunk["digest"]));
            whole.extend_from_slice(&plaintext);
        }
        assert_eq!(hex::encode(*sha256(&whole)), manifest.file.sha256);
        assert_eq!(whole.len() as u64, manifest.file.size);
    }
}

/// Prints the share vector; run once to make it, then kept for the record.
#[test]
#[ignore]
fn print_share_vector() {
    use crate::files::FileManifest;
    let share_id = "6c".repeat(32);
    let version_id = "6d".repeat(32);
    let upload_id = "6e".repeat(32);
    let key: Key = [0x3e; 32];
    let plaintext = b"A small file. ".repeat(20);
    let aad = chunk_aad(&share_id, &version_id, 0, 1, plaintext.len() as u64);
    let sealed_chunk = seal_with_nonce(&key, &plaintext, &aad, &nonce(0));
    let digest = hex::encode(*sha256(&sealed_chunk));
    let file = FileManifest {
        id: share_id.clone(),
        version: version_id.clone(),
        name: "small.txt".into(),
        mime: "text/plain".into(),
        size: plaintext.len() as u64,
        sha256: hex::encode(*sha256(&plaintext)),
        chunks: vec![FileChunk {
            id: format!("{share_id}:{version_id}:0"),
            digest: digest.clone(),
            size: plaintext.len() as u64,
        }],
        key: String::new(),
    };
    let manifest_plain = canonical(&ShareFile {
        format: SHARE_FORMAT.into(),
        share_id: share_id.clone(),
        file,
    });
    let manifest_aad = share_manifest_aad(&share_id, &upload_id, &share_id, &version_id);
    let manifest = seal(&key, &manifest_plain, &manifest_aad);
    let wrap = wrap_key(&key, "correct horse battery staple", &share_id).unwrap();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "name": "a small text file shared by link, with a password",
            "share_id": share_id,
            "upload_id": upload_id,
            "version_id": version_id,
            "key_hex": hex::encode(key),
            "password": "correct horse battery staple",
            "argon2id": {"m_kib": PASSWORD_M_KIB, "t": PASSWORD_T, "p": PASSWORD_P, "salt": "the share ID's 32 bytes"},
            "key_wrap": wrap,
            "key_wrap_aad": hex::encode(canonical(&share_key_aad(&share_id))),
            "manifest": manifest,
            "manifest_digest": hex::encode(*sha256(&unb64(&manifest).unwrap())),
            "manifest_aad": hex::encode(canonical(&manifest_aad)),
            "manifest_plaintext_hex": hex::encode(&manifest_plain),
            "chunks": [{
                "index": 0,
                "plaintext_hex": hex::encode(&plaintext),
                "aad": hex::encode(canonical(&aad)),
                "nonce_hex": hex::encode(nonce(0)),
                "ciphertext": b64(&sealed_chunk),
                "digest": digest,
            }],
            "generator": "dmsg_client (Rust), 2026-10-16",
        }))
        .unwrap()
    );
    let _ = b64;
}
