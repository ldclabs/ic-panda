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
use dmsg_types::{Error, Result, ensure};

/// The library's record prefixes, in the order the receiver commits them.
pub(crate) const PHASES: [&str; 3] = [FOLDER, VERSION, FILE];

/// Every library record as an entry. A version lists its chunks and
/// thumbnail when all of them are on this device; otherwise it is offered
/// without them, as on a device that keeps only its cloud backup.
///
/// # Errors
/// Returns `Error::Locked` while locked.
pub(crate) fn entries(c: &mut Client) -> Result<Vec<Entry>> {
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
                ensure(
                    !version.local || chunks.is_some(),
                    Error::RecoveryIncomplete,
                )?;
                ensure(
                    version.local || version.backup.is_some(),
                    Error::RecoveryIncomplete,
                )?;
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
pub(crate) enum Outcome {
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
pub(crate) fn commit(c: &mut Client, key: &str, value: &[u8]) -> Result<Outcome> {
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
        let mut record: Version = decode(value, MAX_PRIVATE)?;
        let incoming_local = record.local;
        if present(c, file, version)? {
            // A backup-only record, or a transfer interrupted after its
            // metadata arrived, must still be able to acquire its chunks.
            let own = c.version(file, version)?;
            ensure(
                own.key == record.key && own.chunks == record.chunks,
                Error::IntegrityFailed,
            )?;
            record = own;
        }
        let blobs = c.blobs()?;
        let mut here = true;
        for chunk in &record.chunks {
            here &= blobs.has_chunk(&chunk.digest)?;
        }
        ensure(!incoming_local || here, Error::RecoveryIncomplete)?;
        let local = (record.local || incoming_local) && here;
        if present(c, file, version)? && record.local == local {
            return Ok(Outcome::Kept);
        }
        record.local = local;
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
pub(crate) fn held(c: &mut Client, key: &str) -> Result<bool> {
    if let Some(id) = key.strip_prefix(FOLDER) {
        return Ok(c.catalog()?.folders.contains_key(id));
    }
    if let Some(rest) = key.strip_prefix(VERSION) {
        let (file, version) = rest.split_once(':').ok_or(Error::IntegrityFailed)?;
        if !present(c, file, version)? {
            return Ok(false);
        }
        let version = c.version(file, version)?;
        if !version.local {
            return Ok(false);
        }
        for chunk in &version.chunks {
            if !c.blobs()?.has_chunk(&chunk.digest)? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    Ok(false)
}
