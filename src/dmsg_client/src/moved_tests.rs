use super::*;
use crate::files::{BlobStore, CHUNK};
use crate::library::blobs::Blobs;
use crate::library::catalog::ROOT;
use crate::testing::{SECRET, TempDir, unlocked};

fn import_file(c: &mut Client, files: &TempDir, name: &str, bytes: &[u8]) -> String {
    let path = files.path(name);
    std::fs::write(&path, bytes).unwrap();
    c.files_import(&path, ROOT, false, &mut |_, _| {})
        .unwrap()
        .file
        .id
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
    if from.is_dir() {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            copy_tree(&entry.path(), &to.join(entry.file_name()));
        }
    } else {
        std::fs::copy(from, to).unwrap();
    }
}

#[test]
fn a_copy_opens_as_its_device_and_reads_its_library() {
    let (here_dir, old_dir, files) = (TempDir::new(), TempDir::new(), TempDir::new());
    let mut old = unlocked(&old_dir.0, false, None);
    let id = import_file(&mut old, &files, "a.bin", &vec![7; CHUNK + 3]);
    let device = old.workspace.meta.device_id;
    let slot = old.slot_dir(&old.slot.clone());
    drop(old);
    let here = unlocked(&here_dir.0, false, None);
    let moved = here.dir.join(MOVED).join("old");
    copy_tree(&slot, &moved);

    let mut copy = here.moved_client("old", &SECRET).unwrap();
    assert_eq!(copy.workspace.meta.device_id, device);
    let entries = library::entries(&mut copy).unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().any(|e| e.key.contains(&id)));
    assert!(
        copy.workspace
            .store
            .set_meta("source-must-not-change", &true)
            .is_err()
    );
    assert_eq!(copy.reset().err(), Some(Error::Forbidden));
    assert!(moved.join(crate::client::WORKSPACE).exists());
    // Signing has its own durable journal, without importing old work.
    let working = here.moved_signing_client("old", &SECRET).unwrap();
    working
        .workspace
        .put_private(working.keys().unwrap(), "journal", &"handover")
        .unwrap();
    assert!(copy.workspace.store.sealed("journal").unwrap().is_none());
    drop(working);
    let working = here.moved_signing_client("old", &SECRET).unwrap();
    assert_eq!(
        working
            .workspace
            .private::<String>(working.keys().unwrap(), "journal")
            .unwrap()
            .as_deref(),
        Some("handover")
    );
    // Another secret does not open it, nor does a name outside moved/.
    assert_eq!(
        here.moved_client("old", &[8; 32]).err(),
        Some(Error::IntegrityFailed)
    );
    assert_eq!(
        here.moved_client("../x", &SECRET).err(),
        Some(Error::NotFound)
    );
}

#[test]
fn chunks_clone_once_and_only_when_they_match() {
    let (a, b) = (TempDir::new(), TempDir::new());
    let (from, to) = (Blobs::new(a.0.clone()), Blobs::new(b.0.clone()));
    let bytes = vec![3; 100];
    let digest = hex::encode(*crate::codec::sha256(&bytes));
    from.put_chunk(&digest, &bytes).unwrap();
    assert_eq!(to.copy_from(&from, &digest).unwrap(), 100);
    assert_eq!(to.chunk(&digest).unwrap(), Some(bytes.clone()));
    assert_eq!(to.copy_from(&from, &digest).unwrap(), 0);
    // A chunk the copy lacks, or one changed on disk, does not come.
    let other = hex::encode(*crate::codec::sha256(b"other"));
    assert_eq!(to.copy_from(&from, &other).unwrap(), 0);
    let path = a.0.join(&other[..2]).join(&other);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"changed").unwrap();
    assert_eq!(to.copy_from(&from, &other), Err(Error::IntegrityFailed));
    assert!(!to.has_chunk(&other).unwrap());
}
