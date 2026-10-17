use super::catalog::{safe_name, validate_name, Origin, ROOT};
use super::gc::{COLLECT_MS, MAINTENANCE_MS};
use super::open::OPEN_TTL_MS;
use super::*;
use crate::codec::sha256;
use crate::files::{BlobStore, CHUNK, MAX_FILE};
use crate::testing::{unlocked, TempDir};
use crate::upload::random_hex;
use dmsg_types::DAY;

fn no_progress(_: u64, _: u64) {}

fn write(dir: &TempDir, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.path(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

fn pattern(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| ((i as u64 * 31 + seed as u64) % 251) as u8)
        .collect()
}

#[test]
fn folders_files_and_versions_round_trip_through_a_reload() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let docs = client.files_mkdir(ROOT, "docs").unwrap();
    let deep = client.files_mkdir_path("docs/2026/q4").unwrap();
    assert_eq!(deep.path, ["docs", "2026"]);
    assert_eq!(client.files_mkdir(ROOT, "docs").unwrap().id, docs.id);
    assert_eq!(
        client.files_folder_by_path("docs/2026/q4").unwrap(),
        deep.id
    );
    let data = pattern(CHUNK + 777, 3);
    let path = write(&dir, "report.pdf", &data);
    let imported = client
        .files_import(&path, &docs.id, false, &mut no_progress)
        .unwrap();
    assert_eq!(imported.file.name, "report.pdf");
    assert_eq!(imported.file.size, data.len() as u64);
    assert_eq!(imported.file.sha256, hex::encode(*sha256(&data)));
    assert_eq!(imported.file.status, "local");
    assert_eq!(imported.file.path, ["docs"]);
    assert!(!imported.resumed && imported.duplicate_of.is_empty());
    client
        .files_update(
            &imported.file.id,
            Some(vec!["work".into(), " q4 ".into()]),
            Some("final".into()),
            Some(true),
        )
        .unwrap();

    // The same bytes again under another name: a duplicate is reported,
    // not merged.
    let twin = write(&dir, "copy.pdf", &data);
    let again = client
        .files_import(&twin, &docs.id, false, &mut no_progress)
        .unwrap();
    assert_eq!(again.duplicate_of, std::slice::from_ref(&imported.file.id));

    // The same name again: a new version of the same file.
    let changed = pattern(CHUNK + 777, 4);
    let path = write(&dir, "report.pdf", &changed);
    let second = client
        .files_import(&path, &docs.id, false, &mut no_progress)
        .unwrap();
    assert_eq!(second.file.id, imported.file.id);
    assert_eq!(second.file.versions, 2);
    assert_eq!(second.file.sha256, hex::encode(*sha256(&changed)));
    // Or kept beside it with --keep-both.
    let third = client
        .files_import(&path, &docs.id, true, &mut no_progress)
        .unwrap();
    assert_eq!(third.file.name, "report (2).pdf");
    assert_eq!(third.duplicate_of, std::slice::from_ref(&imported.file.id));

    // Everything reads back after a lock and an unlock.
    client.lock();
    assert_eq!(client.files_tree().err(), Some(Error::Locked));
    client.keys = Some(client.workspace.unlock(&crate::testing::SECRET).unwrap());
    let root = RootKeys_single();
    client.root = Some(root);
    let listing = client.files_list(&docs.id, false).unwrap();
    assert_eq!(listing.folder.as_ref().unwrap().name, "docs");
    assert_eq!(listing.folders.len(), 1);
    let names: Vec<&str> = listing.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["copy.pdf", "report (2).pdf", "report.pdf"]);
    let report = listing
        .files
        .iter()
        .find(|f| f.name == "report.pdf")
        .unwrap();
    assert_eq!(report.tags, ["work", "q4"]);
    assert_eq!(report.note, "final");
    assert!(report.starred);
    assert_eq!(report.versions, 2);
    let details = client.files_show(&report.id).unwrap();
    assert_eq!(details.versions.len(), 2);
    assert!(details.versions[1].current && !details.versions[0].current);
    assert_eq!(details.versions[0].origin, Origin::Imported);
    assert_eq!(client.files_tree().unwrap().len(), 3);
    assert_eq!(
        client
            .files_search("WORK")
            .unwrap()
            .iter()
            .map(|f| f.id.as_str())
            .collect::<Vec<_>>(),
        [report.id.as_str()]
    );

    // Versions: either exports, the current one can be chosen, one can go.
    let old = details.versions[0].id.clone();
    let out = dir.path("old.pdf");
    client.files_export(&report.id, Some(&old), &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), data);
    let out = dir.path("new.pdf");
    client.files_export(&report.id, None, &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), changed);
    assert_eq!(
        client.files_export(&report.id, None, &out).err(),
        Some(Error::InvalidInput("out".into()))
    );
    let view = client.files_version_use(&report.id, &old).unwrap();
    assert_eq!(view.sha256, hex::encode(*sha256(&data)));
    let view = client.files_version_delete(&report.id, &old).unwrap();
    assert_eq!(view.versions, 1);
    assert_eq!(view.current, details.versions[1].id);
    assert_eq!(
        client
            .files_version_delete(&report.id, &details.versions[1].id)
            .err(),
        Some(Error::InvalidInput("version".into()))
    );
    assert_eq!(
        client.version(&report.id, &old).err(),
        Some(Error::NotFound)
    );

    // Renaming and moving check names; a folder never moves into itself.
    client.files_rename(&report.id, "report-final.pdf").unwrap();
    assert_eq!(
        client.files_rename(&report.id, "copy.pdf").err(),
        Some(Error::IdempotencyConflict)
    );
    assert!(client.files_rename(&report.id, "a/b").is_err());
    client
        .files_move(std::slice::from_ref(&report.id), &deep.id)
        .unwrap();
    assert_eq!(
        client.files_show(&report.id).unwrap().file.path,
        ["docs", "2026", "q4"]
    );
    assert_eq!(
        client
            .files_move(std::slice::from_ref(&docs.id), &deep.id)
            .err(),
        Some(Error::InvalidInput("folder".into()))
    );
    let usage = client.files_usage().unwrap();
    assert_eq!((usage.files, usage.versions, usage.trashed), (3, 3, 0));
    assert_eq!(usage.blobs, 6);
}

/// The test account's root, as `testing::unlocked` sets it.
#[allow(non_snake_case)]
fn RootKeys_single() -> crate::root::RootKeys {
    crate::root::RootKeys::single(1, [5; 32])
}

#[test]
fn an_interrupted_import_resumes_only_on_the_same_bytes() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let data = pattern(2 * CHUNK + 10, 7);
    let path = write(&dir, "resume.bin", &data);

    // An import that stops after its first chunk.
    let mut stopped = 0;
    let mut stop = |done: u64, _: u64| {
        stopped = done;
    };
    struct Partial<'a>(&'a [u8], usize);
    impl std::io::Read for Partial<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.1 >= CHUNK {
                return Err(std::io::Error::other("stopped"));
            }
            let n = buf.len().min(self.0.len() - self.1).min(CHUNK - self.1);
            buf[..n].copy_from_slice(&self.0[self.1..self.1 + n]);
            self.1 += n;
            Ok(n)
        }
    }
    let failed = client.import_stream(
        &mut Partial(&data, 0),
        data.len() as u64,
        "resume.bin",
        ROOT,
        None,
        Origin::Imported,
        &mut stop,
    );
    assert!(matches!(failed, Err(Error::InvalidInput(_))));
    assert_eq!(client.import_jobs().unwrap().len(), 1);
    let kept = client.import_jobs().unwrap()[0].chunks.clone();
    assert_eq!(kept.len(), 1);
    assert!(client.blobs().unwrap().has_chunk(&kept[0].digest).unwrap());
    // The unfinished import's chunk survives collection.
    client.collect_blobs().unwrap();
    assert!(client.blobs().unwrap().has_chunk(&kept[0].digest).unwrap());

    // A changed file is refused; the job stays for the original bytes.
    let mut altered = data.clone();
    altered[5] ^= 1;
    std::fs::write(&path, &altered).unwrap();
    assert_eq!(
        client
            .files_import(&path, ROOT, false, &mut no_progress)
            .err(),
        Some(Error::IntegrityFailed)
    );
    std::fs::write(&path, &data).unwrap();
    let imported = client
        .files_import(&path, ROOT, false, &mut no_progress)
        .unwrap();
    assert!(imported.resumed);
    let version = client
        .version(&imported.file.id, &imported.version)
        .unwrap();
    assert_eq!(version.chunks[0], kept[0]);
    assert_eq!(version.chunks.len(), 3);
    assert!(client.import_jobs().unwrap().is_empty());
    let out = dir.path("resumed.bin");
    client.files_export(&imported.file.id, None, &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), data);
}

#[test]
fn the_size_limit_and_names_are_checked() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    client.config.files_max_bytes = 3 * CHUNK as u64;
    let fits = write(&dir, "fits.bin", &pattern(3 * CHUNK, 1));
    client
        .files_import(&fits, ROOT, false, &mut no_progress)
        .unwrap();
    let big = dir.path("big.bin");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(3 * CHUNK as u64 + 1)
        .unwrap();
    assert_eq!(
        client
            .files_import(&big, ROOT, false, &mut no_progress)
            .err(),
        Some(Error::QuotaExceeded)
    );
    assert!(client
        .files_import(
            std::path::Path::new("relative.bin"),
            ROOT,
            false,
            &mut no_progress
        )
        .is_err());
    assert_eq!(
        client
            .files_import(&fits, "nowhere", false, &mut no_progress)
            .err(),
        Some(Error::NotFound)
    );
    assert!(validate_name("").is_err());
    assert!(validate_name("..").is_err());
    assert!(validate_name("a\\b").is_err());
    assert!(validate_name(&"x".repeat(256)).is_err());
    assert!(validate_name("ok name.txt").is_ok());
    assert_eq!(safe_name("a/b\\c\u{0}d"), "a_b_c_d");
    assert_eq!(safe_name("  "), "file");
    assert!(client.files_mkdir(ROOT, "").is_err());
    assert!(client.files_mkdir(ROOT, "x/y").is_err());

    // A 4 GiB limit means a 4 GiB file: the bound is checked before a byte
    // is read, so a sparse file past it is refused at once.
    client.config.files_max_bytes = 4 * 1024 * 1024 * 1024;
    let huge = dir.path("huge.bin");
    std::fs::File::create(&huge)
        .unwrap()
        .set_len(4 * 1024 * 1024 * 1024 + 1)
        .unwrap();
    assert_eq!(
        client
            .files_import(&huge, ROOT, false, &mut no_progress)
            .err(),
        Some(Error::QuotaExceeded)
    );
    // An empty file imports as a version without chunks.
    let empty = write(&dir, "empty.txt", b"");
    let imported = client
        .files_import(&empty, ROOT, false, &mut no_progress)
        .unwrap();
    assert_eq!(imported.file.mime, "text/plain");
    let out = dir.path("empty.out");
    client.files_export(&imported.file.id, None, &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), b"");
}

#[test]
fn the_trash_expires_and_collection_keeps_what_is_referenced() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    client.config.files_trash_days = 1;
    let folder = client.files_mkdir(ROOT, "bin").unwrap();
    let a = client
        .files_import(
            &write(&dir, "a.bin", &pattern(CHUNK + 1, 1)),
            &folder.id,
            false,
            &mut no_progress,
        )
        .unwrap();
    let b = client
        .files_import(
            &write(&dir, "b.bin", &pattern(CHUNK + 2, 2)),
            ROOT,
            false,
            &mut no_progress,
        )
        .unwrap();
    let a_chunks = client.version(&a.file.id, &a.version).unwrap().chunks;
    let b_chunks = client.version(&b.file.id, &b.version).unwrap().chunks;
    let has = |client: &Client, digest: &str| client.blobs().unwrap().has_chunk(digest).unwrap();

    // A stray chunk and a stale temporary file go; referenced chunks stay.
    let stray = hex::encode(*sha256(b"stray"));
    client.blobs().unwrap().put_chunk(&stray, b"stray").unwrap();
    let tmp = client.blobs().unwrap().dir().join("tmp").join("old");
    std::fs::write(&tmp, b"x").unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_millis(2 * DAY);
    std::fs::File::options()
        .write(true)
        .open(&tmp)
        .unwrap()
        .set_modified(old)
        .unwrap();
    assert_eq!(client.collect_blobs().unwrap(), (1, 1));
    assert!(!has(&client, &stray) && !tmp.exists());
    assert!(a_chunks
        .iter()
        .chain(&b_chunks)
        .all(|c| has(&client, &c.digest)));

    // Trashing a folder hides its files; the trash lists the folder, not them.
    let now = now_ms();
    client
        .files_trash(&[folder.id.clone(), b.file.id.clone()])
        .unwrap();
    assert_eq!(
        client.files_list(&folder.id, false).err(),
        Some(Error::NotFound)
    );
    assert!(client.files_list(ROOT, false).unwrap().files.is_empty());
    let trash = client.files_list(ROOT, true).unwrap();
    assert_eq!(trash.folders.len(), 1);
    assert_eq!(trash.files.len(), 1);
    assert!(client.files_search("a.bin").unwrap().is_empty());
    assert_eq!(client.files_usage().unwrap().trashed, 2);
    // Restoring brings the folder and its file back.
    client
        .files_restore(std::slice::from_ref(&folder.id))
        .unwrap();
    assert_eq!(client.files_list(&folder.id, false).unwrap().files.len(), 1);
    client
        .files_trash(std::slice::from_ref(&folder.id))
        .unwrap();

    // Maintenance before the deadline purges nothing; after it, everything
    // trashed goes with its chunks, and a backup-less chunk set is freed.
    let report = client.files_maintain(now).unwrap().unwrap();
    assert_eq!((report.purged_files, report.purged_folders), (0, 0));
    assert!(client.files_maintain(now + 1).unwrap().is_none());
    // Without expired trash, unreferenced chunks wait for the hourly pass.
    let late = hex::encode(*sha256(b"late"));
    client.blobs().unwrap().put_chunk(&late, b"late").unwrap();
    let report = client
        .files_maintain(now + MAINTENANCE_MS)
        .unwrap()
        .unwrap();
    assert_eq!(report.collected, 0);
    assert!(has(&client, &late));
    let report = client
        .files_maintain(now + COLLECT_MS + MAINTENANCE_MS)
        .unwrap()
        .unwrap();
    assert_eq!(report.collected, 1);
    assert!(!has(&client, &late));
    let later = now + DAY + MAINTENANCE_MS;
    let report = client.files_maintain(later).unwrap().unwrap();
    assert_eq!((report.purged_files, report.purged_folders), (2, 1));
    assert_eq!(report.collected, 4);
    assert!(a_chunks
        .iter()
        .chain(&b_chunks)
        .all(|c| !has(&client, &c.digest)));
    assert!(client.files_list(ROOT, true).unwrap().files.is_empty());
    assert_eq!(client.files_show(&a.file.id).err(), Some(Error::NotFound));
    let usage = client.files_usage().unwrap();
    assert_eq!((usage.files, usage.trashed, usage.blobs), (0, 0, 0));

    // Purging by ID needs the trash; emptying it takes everything there.
    let c = client
        .files_import(&write(&dir, "c.bin", b"c"), ROOT, false, &mut no_progress)
        .unwrap();
    assert_eq!(
        client
            .files_purge(Some(std::slice::from_ref(&c.file.id)))
            .err(),
        Some(Error::NotFound)
    );
    client
        .files_trash(std::slice::from_ref(&c.file.id))
        .unwrap();
    assert_eq!(client.files_purge(None).unwrap().purged_files, 1);
}

#[test]
fn opened_copies_report_changes_and_are_cleaned_up() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let data = b"hello opened file".to_vec();
    let imported = client
        .files_import(
            &write(&dir, "note.txt", &data),
            ROOT,
            false,
            &mut no_progress,
        )
        .unwrap();
    let opened = client.files_open(&imported.file.id, None).unwrap();
    assert_eq!(std::fs::read(&opened.path).unwrap(), data);
    assert!(opened.path.ends_with("note.txt"));
    let now = now_ms();
    assert!(client.files_open_sweep(now).unwrap().is_empty());
    assert_eq!(client.files_open_list().unwrap().len(), 1);

    // Unchanged past its time: removed, directory included.
    assert!(client
        .files_open_sweep(now + OPEN_TTL_MS)
        .unwrap()
        .is_empty());
    assert!(client.files_open_list().unwrap().is_empty());
    assert!(!std::path::Path::new(&opened.path)
        .parent()
        .unwrap()
        .exists());

    // Changed: reported once, kept past its time, and saved as a version
    // on request.
    let opened = client.files_open(&imported.file.id, None).unwrap();
    std::fs::write(&opened.path, b"hello edited file").unwrap();
    let changed = client.files_open_sweep(now + OPEN_TTL_MS).unwrap();
    assert_eq!(changed.len(), 1);
    assert!(changed[0].changed);
    assert!(client
        .files_open_sweep(now + OPEN_TTL_MS)
        .unwrap()
        .is_empty());
    assert_eq!(client.files_open_list().unwrap().len(), 1);
    // Locking keeps a changed copy for the next unlock to decide.
    client.open_copies_on_lock().unwrap();
    assert!(std::path::Path::new(&opened.path).exists());
    let removed = client
        .files_open_cleanup(Some(&opened.nonce), true, &mut no_progress)
        .unwrap();
    assert_eq!(removed.len(), 1);
    assert!(!std::path::Path::new(&opened.path).exists());
    let details = client.files_show(&imported.file.id).unwrap();
    assert_eq!(details.versions.len(), 2);
    assert_eq!(details.versions[1].origin, Origin::Edited);
    assert_eq!(
        details.file.sha256,
        hex::encode(*sha256(b"hello edited file"))
    );
    assert_eq!(
        client
            .files_open_cleanup(Some("missing"), false, &mut no_progress)
            .err(),
        Some(Error::NotFound)
    );

    // An unchanged copy goes on lock.
    let opened = client.files_open(&imported.file.id, None).unwrap();
    client.lock();
    assert!(!std::path::Path::new(&opened.path).exists());
}

#[test]
fn previews_verification_and_shared_editions() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let text = "line one\nline two\n".repeat(5000);
    let note = client
        .files_import(
            &write(&dir, "notes.md", text.as_bytes()),
            ROOT,
            false,
            &mut no_progress,
        )
        .unwrap();
    assert_eq!(note.file.mime, "text/markdown");
    let preview = client.files_preview(&note.file.id, None).unwrap();
    assert_eq!(preview.kind, "text");
    let head = crate::codec::unb64(preview.content_base64url.as_deref().unwrap()).unwrap();
    assert_eq!(head.len(), PREVIEW_TEXT);
    assert_eq!(&head[..], &text.as_bytes()[..PREVIEW_TEXT]);

    // A PNG is sniffed from its bytes, gets a thumbnail, and previews as it.
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(640, 400, image::Rgba([10, 200, 30, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let picture = client
        .files_import(
            &write(&dir, "picture.bin", png.get_ref()),
            ROOT,
            false,
            &mut no_progress,
        )
        .unwrap();
    assert_eq!(picture.file.mime, "image/png");
    let version = client.version(&picture.file.id, &picture.version).unwrap();
    let thumbnail = version.thumbnail.clone().unwrap();
    let preview = client.files_preview(&picture.file.id, None).unwrap();
    assert_eq!((preview.kind, preview.thumbnail), ("image", true));
    let small = image::load_from_memory(
        &crate::codec::unb64(preview.content_base64url.as_deref().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!((small.width(), small.height()), (256, 160));
    // The thumbnail chunk is referenced: collection keeps it.
    assert_eq!(client.collect_blobs().unwrap(), (0, 0));
    assert!(client.blobs().unwrap().has_chunk(&thumbnail).unwrap());
    let binary = client
        .files_import(
            &write(&dir, "blob.dat", &pattern(100, 9)),
            ROOT,
            false,
            &mut no_progress,
        )
        .unwrap();
    assert_eq!(binary.file.mime, "application/octet-stream");
    assert_eq!(
        client.files_preview(&binary.file.id, None).unwrap().kind,
        "none"
    );

    // Verification: every version, by file and across the library; a
    // chunk file damaged on disk is found by name without a key, and by
    // its version with one.
    let report = client.files_verify(&note.file.id, None).unwrap();
    assert_eq!(report.versions.len(), 1);
    assert!(report.versions[0].ok);
    let (count, bad) = client.blobs().unwrap().verify().unwrap();
    let (check, versions) = client.files_check_record(now_ms(), count, bad).unwrap();
    assert_eq!(versions.len(), 3);
    assert!(versions.iter().all(|v| v.ok) && check.blobs_bad.is_empty());
    assert_eq!(check.blobs_checked, 4);
    // A check of the whole library is recorded, and the next is due a day on.
    let check = client.files_last_check().unwrap().unwrap();
    assert_eq!((check.blobs_checked, check.versions_checked), (4, 3));
    assert!(check.ok() && check.at > 0);
    assert!(!client.files_check_due(check.at + DAY - 1).unwrap());
    assert!(client.files_check_due(check.at + DAY).unwrap());
    assert_eq!(
        client.files_usage().unwrap().last_check,
        Some(check.clone())
    );
    let digest = version.chunks[0].digest.clone();
    let path = client
        .blobs()
        .unwrap()
        .dir()
        .join(&digest[..2])
        .join(&digest);
    let mut damaged = std::fs::read(&path).unwrap();
    damaged[20] ^= 1;
    std::fs::write(&path, &damaged).unwrap();
    client.lock();
    // The check takes a scan done beside the library; locked, it checks no
    // version, and the record says so.
    let (count, bad) = client.blobs().unwrap().verify().unwrap();
    assert_eq!((count, &bad), (4, &vec![digest.clone()]));
    let (recorded, versions) = client
        .files_check_record(check.at + DAY, count, bad)
        .unwrap();
    assert!(versions.is_empty());
    assert_eq!(
        (
            recorded.at,
            recorded.versions_checked,
            recorded.blobs_bad.len()
        ),
        (check.at + DAY, 0, 1)
    );
    assert!(!recorded.ok());
    assert_eq!(client.files_last_check().unwrap(), Some(recorded));
    assert_eq!(
        client.files_verify(&note.file.id, None).err(),
        Some(Error::Locked)
    );
    client.keys = Some(client.workspace.unlock(&crate::testing::SECRET).unwrap());
    client.root = Some(RootKeys_single());
    let report = client.files_verify(&picture.file.id, None).unwrap();
    assert!(!report.versions[0].ok);
    // Unlocked, the record names the failed version.
    let (count, bad) = client.blobs().unwrap().verify().unwrap();
    let (_, full) = client.files_check_record(now_ms(), count, bad).unwrap();
    assert_eq!(full.len(), 3);
    let recorded = client.files_last_check().unwrap().unwrap();
    assert_eq!(recorded.versions_checked, 3);
    assert_eq!(recorded.versions_bad.len(), 1);
    assert_eq!(recorded.versions_bad[0].file, picture.file.id);
    assert_eq!(
        report.versions[0].error.as_deref(),
        Some("INTEGRITY_FAILED")
    );
    // The thumbnail still previews, checked on its own; the file does not read.
    assert!(
        client
            .files_preview(&picture.file.id, None)
            .unwrap()
            .thumbnail
    );
    assert_eq!(
        client
            .files_read_range(&picture.file.id, None, 0, u64::MAX)
            .err(),
        Some(Error::IntegrityFailed)
    );

    // A shared edition has the same bytes under another key and identity:
    // no chunk digest in common with the library's copy.
    let (_, _, manifest) = client.library_manifest(&note.file.id, None).unwrap();
    let edition = client.share_edition(&manifest).unwrap();
    assert_ne!(edition.id, manifest.id);
    assert_ne!(edition.key, manifest.key);
    assert_eq!(edition.sha256, manifest.sha256);
    assert_eq!(edition.chunks.len(), manifest.chunks.len());
    assert!(edition
        .chunks
        .iter()
        .zip(&manifest.chunks)
        .all(|(e, m)| e.digest != m.digest));
    let mut out = Vec::new();
    crate::files::read_file(client.blobs().unwrap(), &edition, MAX_FILE, Some(&mut out)).unwrap();
    assert_eq!(out, text.as_bytes());
    // Nothing references the edition: the collector takes it.
    assert_eq!(client.collect_blobs().unwrap().0, 1);
    assert!(!client
        .blobs()
        .unwrap()
        .has_chunk(&edition.chunks[0].digest)
        .unwrap());

    // A version over the cloud bound cannot be shared.
    let big = FileManifest {
        size: MAX_FILE + 1,
        ..manifest.clone()
    };
    assert_eq!(client.share_edition(&big).err(), Some(Error::QuotaExceeded));
    let _ = random_hex();
}

#[test]
fn the_mount_primitives_look_up_read_copy_and_replace() {
    use super::mount::Node;
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let docs = client.files_mkdir_path("docs/2026").unwrap();
    let data = pattern(CHUNK + 77, 4);
    let report = client
        .files_write(&docs.id, "report.bin", &data)
        .unwrap()
        .file;
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(320, 200, image::Rgba([200, 10, 30, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let picture = client
        .files_write(&docs.id, "picture.png", png.get_ref())
        .unwrap()
        .file;
    client
        .files_update(
            &picture.id,
            Some(vec!["art".into()]),
            Some("red".into()),
            Some(true),
        )
        .unwrap();

    // Paths resolve to live folders and files; the root is a folder.
    assert!(matches!(client.files_lookup("").unwrap(), Some(Node::Folder(f)) if f.id == ROOT));
    assert!(
        matches!(client.files_lookup("/docs/2026/").unwrap(), Some(Node::Folder(f)) if f.id == docs.id)
    );
    assert!(
        matches!(client.files_lookup("docs/2026/report.bin").unwrap(), Some(Node::File(f)) if f.id == report.id)
    );
    assert_eq!(client.files_lookup("docs/2026/report.bin/x").unwrap(), None);
    assert_eq!(client.files_lookup("docs/nothing").unwrap(), None);
    client
        .files_trash(std::slice::from_ref(&report.id))
        .unwrap();
    assert_eq!(client.files_lookup("docs/2026/report.bin").unwrap(), None);
    client
        .files_restore(std::slice::from_ref(&report.id))
        .unwrap();

    // Chunks on their own, bounded by the file; the disk reports the
    // current versions' bytes as used.
    assert_eq!(
        &client.files_chunk(&report.id, &report.current, 0).unwrap()[..],
        &data[..CHUNK]
    );
    assert_eq!(
        &client.files_chunk(&report.id, &report.current, 1).unwrap()[..],
        &data[CHUNK..]
    );
    assert!(matches!(
        client.files_chunk(&report.id, &report.current, 2),
        Err(Error::InvalidInput(_))
    ));
    // A range reads across a chunk boundary and stops at the end.
    let at = (CHUNK - 3) as u64;
    let range = |client: &mut Client, offset, length| {
        client
            .files_read_range(&report.id, None, offset, length)
            .unwrap()
            .1
            .to_vec()
    };
    assert_eq!(range(&mut client, at, 7), &data[CHUNK - 3..CHUNK + 4]);
    assert_eq!(range(&mut client, at, u64::MAX), &data[CHUNK - 3..]);
    assert!(range(&mut client, data.len() as u64 + 5, 9).is_empty());
    assert_eq!(
        client.files_mount_used().unwrap(),
        (data.len() + png.get_ref().len()) as u64
    );

    // A copy: same bytes, new key, new identity, its own thumbnail, the
    // tags and the note but not the star.
    let copy = client
        .files_copy(&picture.id, ROOT, "picture (copy).png")
        .unwrap();
    assert_ne!(copy.id, picture.id);
    assert_eq!(
        (
            copy.sha256.as_str(),
            copy.mime.as_str(),
            copy.folder.as_str()
        ),
        (picture.sha256.as_str(), "image/png", ROOT)
    );
    assert_eq!(
        (copy.tags.as_slice(), copy.note.as_str(), copy.starred),
        (["art".to_string()].as_slice(), "red", false)
    );
    assert_eq!(
        client
            .files_read_range(&copy.id, None, 0, u64::MAX)
            .unwrap()
            .1
            .as_slice(),
        png.get_ref().as_slice()
    );
    let original = client.version(&picture.id, &picture.current).unwrap();
    let copied = client.version(&copy.id, &copy.current).unwrap();
    assert_ne!(original.key, copied.key);
    assert_ne!(original.chunks[0].digest, copied.chunks[0].digest);
    assert!(copied.thumbnail.is_some() && copied.thumbnail != original.thumbnail);
    assert_eq!(copied.origin, Origin::Mounted);
    assert_eq!(client.files_preview(&copy.id, None).unwrap().kind, "image");
    assert!(matches!(
        client.files_copy(&picture.id, ROOT, "picture (copy).png"),
        Err(Error::IdempotencyConflict)
    ));
    assert!(matches!(
        client.files_copy(&picture.id, ROOT, "docs"),
        Err(Error::IdempotencyConflict)
    ));

    // A safe save: the temporary file's bytes become a version of the
    // original, which leaves the trash; the temporary file is gone with
    // its chunks.
    let edited = pattern(CHUNK * 2, 8);
    let temp = client
        .files_write(&docs.id, "report.bin.tmp", &edited)
        .unwrap()
        .file;
    let temp_chunks = client.version(&temp.id, &temp.current).unwrap().chunks;
    client
        .files_trash(std::slice::from_ref(&report.id))
        .unwrap();
    let replaced = client.files_replace(&temp.id, &report.id).unwrap();
    assert_eq!(
        (
            replaced.id.as_str(),
            replaced.size,
            replaced.versions,
            replaced.trashed_at
        ),
        (report.id.as_str(), edited.len() as u64, 2, None)
    );
    assert_eq!(
        client
            .files_read_range(&report.id, None, 0, u64::MAX)
            .unwrap()
            .1
            .as_slice(),
        edited.as_slice()
    );
    assert_eq!(
        client.files_versions(&report.id).unwrap()[0].size,
        data.len() as u64
    );
    // A read that began on the old version keeps reading it.
    assert_eq!(
        &client.files_chunk(&report.id, &report.current, 1).unwrap()[..],
        &data[CHUNK..]
    );
    assert!(matches!(client.files_show(&temp.id), Err(Error::NotFound)));
    for chunk in &temp_chunks {
        assert!(!client.blobs().unwrap().has_chunk(&chunk.digest).unwrap());
    }
    assert!(matches!(
        client.files_replace(&report.id, &report.id),
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        client.files_replace(&temp.id, &report.id),
        Err(Error::NotFound)
    ));

    // The upload directory is owner-only and goes on lock.
    let mount = client.files_mount_dir().unwrap();
    assert!(mount.is_dir() && mount.ends_with("mount"));
    std::fs::write(mount.join("partial"), b"...").unwrap();
    client.lock();
    assert!(!mount.exists());
}

#[test]
fn a_reset_removes_the_library_with_the_workspace() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let imported = client
        .files_import(&write(&dir, "x.bin", b"xyz"), ROOT, false, &mut no_progress)
        .unwrap();
    client.files_open(&imported.file.id, None).unwrap();
    let blobs = client.blobs().unwrap().dir().to_path_buf();
    let open = blobs.parent().unwrap().join("open");
    assert!(blobs.exists() && open.exists());
    client.reset().unwrap();
    assert!(!blobs.exists() && !open.exists());
}

#[test]
fn backups_are_planned_under_the_root_and_settle_on_the_version() {
    use super::backup::{BackupJob, BACKUP, BACKUP_FORMAT};
    use crate::content::{open_manifest, Opened};
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let docs = client.files_mkdir_path("docs/2026").unwrap();
    let data = pattern(CHUNK + 5, 2);
    let imported = client
        .files_import(
            &write(&dir, "plan.pdf", &data),
            &docs.id,
            false,
            &mut no_progress,
        )
        .unwrap();
    client
        .files_update(
            &imported.file.id,
            Some(vec!["work".into()]),
            Some("final".into()),
            None,
        )
        .unwrap();
    let job = client
        .plan_backup(&imported.file.id, &imported.version, 7)
        .unwrap();
    assert_eq!(job.plan.kind, BACKUP);
    assert_eq!(job.plan.root_generation, 1);
    assert_ne!(job.plan.object_id, imported.file.id);
    assert_ne!(job.plan.version_id, imported.version);
    assert_eq!(job.plan.chunks.len(), 2);
    // Plan sizes are ciphertext sizes: the sealed chunk files' lengths.
    let version = client
        .version(&imported.file.id, &imported.version)
        .unwrap();
    for (planned, own) in job.plan.chunks.iter().zip(&version.chunks) {
        assert_eq!(planned.digest, own.digest);
        assert_eq!(
            planned.size,
            client.blobs().unwrap().size(&own.digest).unwrap().unwrap()
        );
        assert!(planned.size > own.size);
    }
    let opened = open_manifest(&[5; 32], "040g2081040g2081040g", &job.plan, &job.manifest).unwrap();
    let Opened::Backup(record) = opened else {
        panic!("a backup manifest");
    };
    assert_eq!(record.format, BACKUP_FORMAT);
    assert_eq!(record.file.id, imported.file.id);
    assert_eq!(record.file.version, imported.version);
    assert_eq!(record.file.key, version.key);
    assert_eq!(record.path, ["docs", "2026"]);
    assert_eq!(record.tags, ["work"]);
    assert_eq!(record.note, "final");
    assert_eq!(record.device, version.device);

    // A committed job settles on the version while unlocked, and the file
    // shows the backup; locked, it waits. An empty file plans a placeholder.
    let key = format!(
        "{}{}:{}",
        super::BACKUP_JOB,
        imported.file.id,
        imported.version
    );
    let committed = BackupJob {
        committed: true,
        ..job.clone()
    };
    client
        .workspace
        .store
        .set_library_job(&key, &committed)
        .unwrap();
    assert_eq!(client.files_backup_status().unwrap().len(), 1);
    assert_eq!(
        client.files_show(&imported.file.id).unwrap().file.status,
        "backing_up"
    );
    assert!(client.library_jobs_running().unwrap());
    assert_eq!(client.collect_blobs().unwrap(), (0, 0));
    client.lock();
    assert!(!client.settle_backup(&key, &committed, 9).unwrap());
    client.keys = Some(client.workspace.unlock(&crate::testing::SECRET).unwrap());
    client.root = Some(RootKeys_single());
    assert!(client.settle_backup(&key, &committed, 9).unwrap());
    let details = client.files_show(&imported.file.id).unwrap();
    assert_eq!(details.file.status, "backed_up");
    let backup = details.versions[0].backup.clone().unwrap();
    assert_eq!(backup.upload_id, job.plan.upload_id);
    assert!(backup.verified);
    assert!(client.files_backup_status().unwrap().is_empty());
    let empty = client
        .files_import(
            &write(&dir, "empty.txt", b""),
            ROOT,
            false,
            &mut no_progress,
        )
        .unwrap();
    let placeholder = client
        .plan_backup(&empty.file.id, &empty.version, 9)
        .unwrap();
    assert_eq!(placeholder.plan.chunks.len(), 1);
    assert_eq!(
        placeholder.plan.chunks[0].digest,
        hex::encode(*sha256(&crate::content::empty_file()))
    );

    // A verified backup lets the version leave this device; reading it then
    // asks for a fetch. The thumbnail-less chunks are collected.
    let view = client.files_evict(&imported.file.id, None).unwrap();
    assert_eq!(view.status, "cloud_only");
    assert!(!client
        .blobs()
        .unwrap()
        .has_chunk(&version.chunks[0].digest)
        .unwrap());
    assert_eq!(
        client
            .files_read_range(&imported.file.id, None, 0, u64::MAX)
            .err(),
        Some(Error::RecoveryIncomplete)
    );
    assert!(matches!(
        client.files_evict(&empty.file.id, None).err(),
        Some(Error::InvalidInput(_))
    ));
    // Evicted versions are skipped by a new backup request's planning.
    assert_eq!(
        client
            .version(&imported.file.id, &imported.version)
            .unwrap()
            .status(),
        "cloud_only"
    );
}

#[test]
fn the_cloud_inventory_marks_local_versions_and_lists_from_the_cache() {
    use super::backup::BackupManifest;
    use super::cloud::CloudEntry;
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let imported = client
        .files_import(
            &write(&dir, "notes.txt", b"notes"),
            ROOT,
            false,
            &mut no_progress,
        )
        .unwrap();
    let (file, version, manifest) = client.library_manifest(&imported.file.id, None).unwrap();
    let job = client
        .plan_backup(&imported.file.id, &imported.version, 7)
        .unwrap();
    let record = BackupManifest {
        format: super::backup::BACKUP_FORMAT.into(),
        file: manifest.clone(),
        path: vec![],
        tags: file.tags.clone(),
        note: file.note.clone(),
        created_at: version.created_at,
        modified_at: file.updated_at,
        device: version.device.clone(),
    };
    // What a refresh caches for a backup another device made of this very
    // version, and for one whose root this device lacks.
    client
        .save_cloud_entry(
            &job.plan.upload_id,
            &CloudEntry {
                plan: job.plan.clone(),
                device: "aa".repeat(32),
                committed_at: Some(7),
                manifest: Some(record.clone()),
            },
        )
        .unwrap();
    let mut foreign = job.plan.clone();
    foreign.upload_id = random_hex();
    foreign.root_generation = 9;
    client
        .save_cloud_entry(
            &foreign.upload_id,
            &CloudEntry {
                plan: foreign.clone(),
                device: "bb".repeat(32),
                committed_at: Some(8),
                manifest: None,
            },
        )
        .unwrap();
    client
        .mark_backed_up(&record, &job.plan.upload_id, 1, 7)
        .unwrap();
    let marked = client
        .version(&imported.file.id, &imported.version)
        .unwrap();
    let backup = marked.backup.clone().unwrap();
    assert_eq!(
        (
            backup.upload_id.as_str(),
            backup.generation,
            backup.backed_up_at,
            backup.verified
        ),
        (job.plan.upload_id.as_str(), 1, 7, false)
    );
    // Not verified here: the version stays.
    assert!(matches!(
        client.files_evict(&imported.file.id, None).err(),
        Some(Error::InvalidInput(_))
    ));

    let list = client.files_cloud_list().unwrap();
    assert_eq!(list.len(), 2);
    let known = list
        .iter()
        .find(|c| c.upload_id == job.plan.upload_id)
        .unwrap();
    assert_eq!(known.name.as_deref(), Some("notes.txt"));
    assert_eq!(known.file_id.as_deref(), Some(imported.file.id.as_str()));
    assert!(known.local && !known.needs_root);
    assert_eq!(
        known.bytes,
        job.plan.chunks[0].size + job.plan.manifest_size
    );
    let unknown = list
        .iter()
        .find(|c| c.upload_id == foreign.upload_id)
        .unwrap();
    assert!(unknown.needs_root && !unknown.local && unknown.name.is_none());
    assert_eq!(unknown.generation, 9);

    // Forgetting a cloud copy clears the mark; the file stays.
    client.forget_cloud(&job.plan.upload_id).unwrap();
    assert!(client
        .version(&imported.file.id, &imported.version)
        .unwrap()
        .backup
        .is_none());
    assert_eq!(client.files_cloud_list().unwrap().len(), 1);
    assert_eq!(
        client.files_show(&imported.file.id).unwrap().file.status,
        "local"
    );
}

#[tokio::test]
async fn backup_jobs_stay_apart_from_the_vault_jobs_and_keep_their_chunks() {
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let kept = client
        .files_import(
            &write(&dir, "kept.bin", &pattern(CHUNK + 3, 5)),
            ROOT,
            false,
            &mut no_progress,
        )
        .unwrap();
    let gone = client
        .files_import(
            &write(&dir, "gone.bin", &pattern(9, 6)),
            ROOT,
            false,
            &mut no_progress,
        )
        .unwrap();
    // A backup whose last attempt failed a moment ago.
    let mut job = client.plan_backup(&kept.file.id, &kept.version, 7).unwrap();
    job.error = Some("UNAVAILABLE".into());
    job.failed_at = Some(crate::now_ms());
    let key = format!("{}{}:{}", super::BACKUP_JOB, kept.file.id, kept.version);
    client.workspace.store.set_library_job(&key, &job).unwrap();

    // The vault's sync jobs read as before, and the failed backup waits for
    // its next attempt instead of going again at once.
    assert_eq!(client.flush().await, Ok(0));
    let run = client
        .files_backup_run(None, &mut |_, _, _| {})
        .await
        .unwrap();
    assert_eq!((run.failed, run.pending), (0, 1));

    // Collection goes on beside it and keeps the job's chunks after their
    // version went.
    let gone_chunks = client.version(&gone.file.id, &gone.version).unwrap().chunks;
    client
        .files_trash(&[kept.file.id.clone(), gone.file.id.clone()])
        .unwrap();
    client.files_purge(None).unwrap();
    let blobs = client.blobs().unwrap();
    assert!(gone_chunks
        .iter()
        .all(|c| !blobs.has_chunk(&c.digest).unwrap()));
    assert!(job
        .plan
        .chunks
        .iter()
        .all(|c| blobs.has_chunk(&c.digest).unwrap()));
}

#[tokio::test]
async fn the_last_copy_of_a_released_version_stays_in_the_cloud() {
    use super::backup::{BackupManifest, BACKUP_FORMAT};
    use super::cloud::CloudEntry;
    let dir = TempDir::new();
    let mut client = unlocked(&dir.0, false, None);
    let imported = client
        .files_import(
            &write(&dir, "notes.txt", b"notes"),
            ROOT,
            false,
            &mut no_progress,
        )
        .unwrap();
    let (file, version, manifest) = client.library_manifest(&imported.file.id, None).unwrap();
    let job = client
        .plan_backup(&imported.file.id, &imported.version, 7)
        .unwrap();
    let upload = job.plan.upload_id.clone();
    client
        .save_cloud_entry(
            &upload,
            &CloudEntry {
                plan: job.plan.clone(),
                device: "aa".repeat(32),
                committed_at: Some(7),
                manifest: Some(BackupManifest {
                    format: BACKUP_FORMAT.into(),
                    file: manifest,
                    path: vec![],
                    tags: file.tags.clone(),
                    note: file.note.clone(),
                    created_at: version.created_at,
                    modified_at: file.updated_at,
                    device: version.device.clone(),
                }),
            },
        )
        .unwrap();
    // The version left this device for that backup: deleting the backup
    // would delete the version too, so nothing goes.
    let released = Version {
        local: false,
        backup: Some(Backup {
            upload_id: upload.clone(),
            generation: 1,
            backed_up_at: 7,
            verified: true,
        }),
        ..version
    };
    client
        .save_version(&imported.file.id, &imported.version, &released)
        .unwrap();
    assert!(matches!(
        client
            .files_cloud_delete(std::slice::from_ref(&upload))
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(client.cloud_entry(&upload).unwrap().is_some());
}

#[test]
fn cloud_entries_verify_only_against_their_reservation_evidence() {
    use super::backup::BACKUP;
    use super::cloud::verify_entry;
    use crate::cloud::{sign_command, Evidence, Signer};
    use crate::evidence::Trust;
    use crate::upload::{ChunkRef, UploadPlan};
    use candid::Principal;
    use serde_json::json;

    let account = crate::testing::account().id;
    let issuer = format!("dmsg:{account}");
    let seed = [3u8; 32];
    let device = dmsg_types::Hash::new([4; 32]);
    let signer = Signer {
        issuer: &issuer,
        device_id: device,
        security_epoch: 1,
        seed: &seed,
        expires_at: u64::MAX,
    };
    let plan = UploadPlan {
        upload_id: "11".repeat(32),
        object_id: "22".repeat(32),
        version_id: "33".repeat(32),
        root_generation: 1,
        epoch: 0,
        control_head: None,
        kind: BACKUP.into(),
        chunks: vec![ChunkRef {
            digest: "44".repeat(32),
            size: 1,
        }],
        manifest_digest: "55".repeat(32),
        manifest_size: 1,
        expires_at: 10,
    };
    let entry = json!({
        "upload_id": plan.upload_id,
        "plan": plan,
        "signature": {
            "cose_sign1": sign_command(&signer, "dmsg/upload/reserve/v1", &plan, "01", 10),
        },
        "device_id": hex::encode(*device),
    });
    let trust = Trust::new(vec![Principal::from_slice(&[1])], "dmsg:".into(), vec![]);
    // No device map stands in for the reservation's evidence: without it,
    // a well-formed signature is refused.
    assert_eq!(
        verify_entry(&trust, &account, &entry, 20).unwrap_err(),
        Error::IntegrityFailed
    );
    // Evidence from a home outside this deployment is refused as well.
    let mut foreign = entry.clone();
    foreign["evidence"] = json!(Evidence {
        schema: 1,
        canister: Principal::from_slice(&[2]).to_text(),
        certificate: String::new(),
        entries: vec![],
    });
    foreign["stored_at"] = json!(15);
    assert_eq!(
        verify_entry(&trust, &account, &foreign, 20).unwrap_err(),
        Error::IntegrityFailed
    );
}
