//! Files (encryption design §7): sealed in independent 1 MiB chunks under a
//! random file key with the deterministic nonce `"dMsg" || u64_be(index)`
//! and `ChunkAAD = ["dmsg/file-chunk/1", fileId, version, index, count,
//! length]`. A file's manifest (name, type, length, SHA-256, chunk list and
//! file key) names its chunks by digest; where the chunks are kept is the
//! [`BlobStore`]'s business: the workspace's `chunks` table for content
//! that syncs with the vault, the file library's `blobs/` directory for the
//! library ([`crate::library`]).

use crate::codec::{b64, sha256, unb64};
use crate::crypto::{open_bytes, random_key, seal_with_nonce, Key};
use crate::store::Store;
use crate::upload::random_hex;
use dmsg_types::{ensure, ensure_valid, Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::Path;
use zeroize::Zeroizing;

/// Plaintext bytes per chunk.
pub const CHUNK: usize = 1024 * 1024;
/// Largest file the cloud contract carries: a vault attachment of the
/// legacy migration, a backup or a shared file.
pub const MAX_FILE: u64 = 100 * 1024 * 1024;

/// Where ciphertext chunks live, by the SHA-256 of their bytes.
pub trait BlobStore {
    /// A chunk, checked against its digest.
    ///
    /// # Errors
    /// Stored bytes that no longer match return `Error::IntegrityFailed`.
    fn chunk(&self, digest: &str) -> Result<Option<Vec<u8>>>;

    /// Whether a chunk is stored.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when storage fails.
    fn has_chunk(&self, digest: &str) -> Result<bool>;

    /// Store a chunk under the SHA-256 of its bytes; one already stored
    /// stays.
    ///
    /// # Errors
    /// A digest that does not match returns `Error::IntegrityFailed`.
    fn put_chunk(&self, digest: &str, bytes: &[u8]) -> Result<()>;
}

impl BlobStore for Store {
    fn chunk(&self, digest: &str) -> Result<Option<Vec<u8>>> {
        Store::chunk(self, digest)
    }

    fn has_chunk(&self, digest: &str) -> Result<bool> {
        Store::has_chunk(self, digest)
    }

    fn put_chunk(&self, digest: &str, bytes: &[u8]) -> Result<()> {
        Store::put_chunk(self, digest, bytes)
    }
}

/// One encrypted chunk of a file.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileChunk {
    /// `fileId:version:index`.
    pub id: String,
    /// SHA-256 of the sealed chunk bytes, hex.
    pub digest: String,
    /// Plaintext length.
    pub size: u64,
}

/// A file version's manifest; it carries the file key.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileManifest {
    /// File ID, hex.
    pub id: String,
    /// Version ID, hex.
    pub version: String,
    /// File name.
    pub name: String,
    /// Media type.
    pub mime: String,
    /// Length in bytes.
    pub size: u64,
    /// SHA-256 of the whole plaintext, hex.
    pub sha256: String,
    /// The chunks, in order.
    pub chunks: Vec<FileChunk>,
    /// The file key, base64url.
    pub key: String,
}

impl FileManifest {
    /// The file key.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` for a malformed key.
    pub(crate) fn file_key(&self) -> Result<Zeroizing<Key>> {
        let bytes = Zeroizing::new(unb64(&self.key)?);
        let key: Key = bytes
            .as_slice()
            .try_into()
            .map_err(|_| Error::IntegrityFailed)?;
        Ok(Zeroizing::new(key))
    }

    /// The manifest as it may leave the daemon: without the file key.
    pub fn public(&self) -> Self {
        Self {
            key: String::new(),
            ..self.clone()
        }
    }
}

/// How many chunks a file of `size` bytes has.
pub(crate) fn chunk_count(size: u64) -> u64 {
    size.div_ceil(CHUNK as u64)
}

pub(crate) fn nonce(index: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[..4].copy_from_slice(b"dMsg");
    nonce[4..].copy_from_slice(&index.to_be_bytes());
    nonce
}

pub(crate) fn chunk_aad<'a>(
    file_id: &'a str,
    version: &'a str,
    index: u64,
    count: u64,
    length: u64,
) -> (&'static str, &'a str, &'a str, u64, u64, u64) {
    ("dmsg/file-chunk/1", file_id, version, index, count, length)
}

/// Seal one chunk of the file `id`/`version`; returns the sealed bytes and
/// their digest.
pub(crate) fn seal_chunk(
    key: &Key,
    id: &str,
    version: &str,
    index: u64,
    count: u64,
    plaintext: &[u8],
) -> (Vec<u8>, String) {
    let sealed = seal_with_nonce(
        key,
        plaintext,
        &chunk_aad(id, version, index, count, plaintext.len() as u64),
        &nonce(index),
    );
    let digest = hex::encode(*sha256(&sealed));
    (sealed, digest)
}

pub(crate) fn io(error: std::io::Error) -> Error {
    Error::InvalidInput(format!("file: {error}"))
}

/// Seal in-memory `bytes` as a file whose chunks go to `blobs`, for
/// records that carry a file, such as legacy archive parts.
///
/// # Errors
/// Returns `Error::QuotaExceeded` beyond 100 MiB.
pub(crate) fn store_file(
    blobs: &dyn BlobStore,
    name: &str,
    mime: &str,
    bytes: &[u8],
) -> Result<FileManifest> {
    ensure(bytes.len() as u64 <= MAX_FILE, Error::QuotaExceeded)?;
    let mut manifest = FileManifest {
        id: random_hex(),
        version: random_hex(),
        name: name.into(),
        mime: mime.into(),
        size: bytes.len() as u64,
        sha256: hex::encode(*sha256(bytes)),
        chunks: vec![],
        key: b64(&Zeroizing::new(random_key())[..]),
    };
    let key = manifest.file_key()?;
    let count = chunk_count(manifest.size);
    for (index, plaintext) in bytes.chunks(CHUNK).enumerate() {
        let index = index as u64;
        let (sealed, digest) = seal_chunk(
            &key,
            &manifest.id,
            &manifest.version,
            index,
            count,
            plaintext,
        );
        blobs.put_chunk(&digest, &sealed)?;
        manifest.chunks.push(FileChunk {
            id: format!("{}:{}:{index}", manifest.id, manifest.version),
            digest,
            size: plaintext.len() as u64,
        });
    }
    Ok(manifest)
}

/// Which chunk store a client reads an upload from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    /// The workspace's `chunks` table: vault content, the root, channels.
    Store,
    /// The file library's blob directory.
    Library,
}

/// Open chunk `index` of a file in `blobs`: that chunk's plaintext alone,
/// checked against the manifest's chunk list but not against the whole
/// file's SHA-256; for reads at an offset.
///
/// # Errors
/// Returns `Error::InvalidInput` for an index past the end,
/// `Error::RecoveryIncomplete` for a missing chunk and
/// `Error::IntegrityFailed` for anything changed.
pub(crate) fn read_chunk(
    blobs: &dyn BlobStore,
    manifest: &FileManifest,
    index: u64,
) -> Result<Zeroizing<Vec<u8>>> {
    let count = chunk_count(manifest.size);
    ensure(
        manifest.chunks.len() as u64 == count,
        Error::IntegrityFailed,
    )?;
    ensure_valid(index < count, "index")?;
    let chunk = &manifest.chunks[index as usize];
    let expected = (manifest.size - index * CHUNK as u64).min(CHUNK as u64);
    ensure(
        chunk.id == format!("{}:{}:{index}", manifest.id, manifest.version)
            && chunk.size == expected,
        Error::IntegrityFailed,
    )?;
    let sealed = blobs
        .chunk(&chunk.digest)?
        .ok_or(Error::RecoveryIncomplete)?;
    let key = manifest.file_key()?;
    let plain = open_bytes(
        &key,
        &sealed,
        &chunk_aad(&manifest.id, &manifest.version, index, count, chunk.size),
    )?;
    ensure(plain.len() as u64 == chunk.size, Error::IntegrityFailed)?;
    Ok(plain)
}

/// Open every chunk of a file in `blobs` in order, handing each plaintext
/// to `each`, and check the whole against the manifest's SHA-256. `max`
/// bounds the file size.
///
/// # Errors
/// A missing chunk returns `Error::RecoveryIncomplete`; anything changed
/// returns `Error::IntegrityFailed`.
pub(crate) fn read_chunks(
    blobs: &dyn BlobStore,
    manifest: &FileManifest,
    max: u64,
    mut each: impl FnMut(u64, &[u8]) -> Result<()>,
) -> Result<()> {
    let count = chunk_count(manifest.size);
    ensure(
        manifest.size <= max && manifest.chunks.len() as u64 == count,
        Error::IntegrityFailed,
    )?;
    let mut overall = Sha256::new();
    for index in 0..count {
        let plain = read_chunk(blobs, manifest, index)?;
        overall.update(plain.as_slice());
        each(index, &plain)?;
    }
    ensure(
        hex::encode(overall.finalize()) == manifest.sha256,
        Error::IntegrityFailed,
    )
}

/// Check every chunk of a file in `blobs` and its SHA-256, writing the
/// plaintext to `out` when given. `max` bounds the file size.
///
/// # Errors
/// As [`read_chunks`].
pub(crate) fn read_file(
    blobs: &dyn BlobStore,
    manifest: &FileManifest,
    max: u64,
    mut out: Option<&mut dyn Write>,
) -> Result<()> {
    read_chunks(blobs, manifest, max, |_, plain| match out.as_mut() {
        Some(out) => out.write_all(plain).map_err(io),
        None => Ok(()),
    })
}

/// Verify the file of `manifest`, whose chunks are in `blobs`, and write it
/// to `out`, which must not exist yet. The bytes go to a temporary file
/// first and appear at `out` only once the whole file has been checked.
///
/// # Errors
/// Returns `Error::InvalidInput` when `out` exists or is relative,
/// `Error::RecoveryIncomplete` for a missing chunk and
/// `Error::IntegrityFailed` for anything changed.
pub(crate) fn export_manifest(
    blobs: &dyn BlobStore,
    manifest: &FileManifest,
    max: u64,
    out: &Path,
) -> Result<FileManifest> {
    ensure_valid(out.is_absolute() && !out.exists(), "out")?;
    let name = out
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| dmsg_types::invalid("out"))?;
    let partial = out.with_file_name(format!(".{name}.dmsg-part"));
    let written = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&partial).map_err(io)?;
        read_file(blobs, manifest, max, Some(&mut file))?;
        file.sync_all().map_err(io)?;
        std::fs::rename(&partial, out).map_err(io)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    written.map(|_| manifest.public())
}

#[cfg(test)]
#[path = "files_tests.rs"]
mod tests;
