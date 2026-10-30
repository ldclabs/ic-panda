//! The library in a transfer: every folder, file and version record, and
//! the sealed chunks and thumbnails of the versions kept on the sending
//! device. The receiver commits folders, then each version once its chunks
//! are here, then each file with the versions that arrived.

use super::Entry;
use crate::client::Client;
use crate::codec::{decode, sha256};
use crate::files::BlobStore;
use crate::library::catalog::{
    FILE, FOLDER, FileRecord, Folder, ROOT, VERSION, Version, validate_name,
};
use crate::workspace::MAX_PRIVATE;
use dmsg_types::{Error, Result};

/// The library's record prefixes, in the order the receiver commits them.
pub(super) const PHASES: [&str; 3] = [FOLDER, VERSION, FILE];

/// Every library record as an entry. A version lists its chunks and
/// thumbnail when all of them are on this device; otherwise it is offered
/// without them, as on a device that keeps only its cloud backup.
///
/// # Errors
/// Returns `Error::Locked` while locked.
pub(super) fn entries(c: &mut Client) -> Result<Vec<Entry>> {
    let blobs = c.blobs()?.clone();
    let mut entries = vec![];
    for prefix in PHASES {
        for key in c.workspace.store.sealed_keys(prefix)? {
            let Some(value) = c.workspace.open_sealed(c.keys()?, &key)? else {
                continue;
            };
            let mut digests = vec![];
            let mut blob_bytes = 0;
            if prefix == VERSION {
                let version: Version = decode(&value, MAX_PRIVATE)?;
                let found = |name: &str| match (hex::decode(name), blobs.size(name)) {
                    (Ok(digest), Ok(Some(size))) if digest.len() == 32 => Some((digest, size)),
                    _ => None,
                };
                let chunks: Option<Vec<_>> =
                    version.chunks.iter().map(|c| found(&c.digest)).collect();
                if let Some(mut chunks) = chunks.filter(|_| version.local) {
                    // The thumbnail is optional.
                    chunks.extend(version.thumbnail.as_deref().and_then(found));
                    for (digest, size) in chunks {
                        digests.extend_from_slice(&digest);
                        blob_bytes += size;
                    }
                }
            }
            entries.push(Entry {
                sha256: sha256(&value),
                size: value.len() as u64,
                key,
                blobs: digests.into(),
                blob_bytes,
            });
        }
    }
    entries.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(entries)
}

/// What committing a record did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    /// Added, or merged into this device's record.
    Added,
    /// This device had it and kept its own.
    Kept,
    /// Left out: a new file none of whose versions are here.
    Skipped,
}

fn present(c: &Client, file: &str, version: &str) -> Result<bool> {
    Ok(c.workspace
        .store
        .sealed(&format!("{VERSION}{file}:{version}"))?
        .is_some())
}

/// Commit record `key` received as `value`. A record this device has stays
/// as it is, except a file, which gains the versions that arrived; nothing
/// is trashed again or restored. A version reads as kept here only when
/// every chunk it lists is.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for a record that is not the library's
/// or does not decode as its type.
pub(super) fn commit(c: &mut Client, key: &str, value: &[u8]) -> Result<Outcome> {
    if let Some(id) = key.strip_prefix(FOLDER) {
        let folder: Folder = decode(value, MAX_PRIVATE)?;
        validate_name(&folder.name)?;
        if c.catalog()?.folders.contains_key(id) {
            return Ok(Outcome::Kept);
        }
        c.save_folder(id, folder)?;
        return Ok(Outcome::Added);
    }
    if let Some(rest) = key.strip_prefix(VERSION) {
        let (file, version) = rest.split_once(':').ok_or(Error::IntegrityFailed)?;
        if present(c, file, version)? {
            return Ok(Outcome::Kept);
        }
        let mut record: Version = decode(value, MAX_PRIVATE)?;
        let blobs = c.blobs()?;
        let mut here = true;
        for chunk in &record.chunks {
            here &= blobs.has_chunk(&chunk.digest)?;
        }
        record.local &= here;
        c.save_version(file, version, &record)?;
        return Ok(Outcome::Added);
    }
    let id = key.strip_prefix(FILE).ok_or(Error::IntegrityFailed)?;
    let mut incoming: FileRecord = decode(value, MAX_PRIVATE)?;
    validate_name(&incoming.name)?;
    let mut arrived = vec![];
    for version in incoming.versions.drain(..) {
        if present(c, id, &version.id)? {
            arrived.push(version);
        }
    }
    match c.catalog()?.files.get(id).cloned() {
        Some(mut own) => {
            let before = own.versions.len();
            for version in arrived {
                if own.versions.iter().all(|v| v.id != version.id) {
                    own.versions.push(version);
                }
            }
            if own.versions.len() == before {
                return Ok(Outcome::Kept);
            }
            own.versions.sort_by_key(|v| v.created_at);
            c.save_file(id, own)?;
            Ok(Outcome::Added)
        }
        None if arrived.is_empty() => Ok(Outcome::Skipped),
        None => {
            incoming.versions = arrived;
            if incoming.versions.iter().all(|v| v.id != incoming.current) {
                incoming.current = incoming.versions[incoming.versions.len() - 1].id.clone();
            }
            if incoming.folder != ROOT && !c.catalog()?.folders.contains_key(&incoming.folder) {
                incoming.folder = ROOT.into();
            }
            c.save_file(id, incoming)?;
            Ok(Outcome::Added)
        }
    }
}

/// Whether record `key` is here already; a file always merges.
pub(super) fn held(c: &mut Client, key: &str) -> Result<bool> {
    if let Some(id) = key.strip_prefix(FOLDER) {
        return Ok(c.catalog()?.folders.contains_key(id));
    }
    if key.starts_with(VERSION) {
        return Ok(c.workspace.store.sealed(key)?.is_some());
    }
    Ok(false)
}
