//! Shared editions: a version re-encrypted under a fresh key with a fresh
//! file identity, so what a channel member (or, later, a link holder)
//! receives shares neither a key nor a chunk byte with the library's own
//! copy or with a backup. The relay and the recipients cannot relate an
//! edition to anything else of the account. An edition's chunks live in the
//! blob directory until its message is stored, then the collector takes
//! them.

use super::Client;
use crate::codec::b64;
use crate::crypto::random_key;
use crate::files::{
    BlobStore, FileChunk, FileManifest, MAX_FILE, chunk_count, read_chunks, seal_chunk,
};
use crate::upload::random_hex;
use dmsg_types::{Error, Result, ensure};
use zeroize::Zeroizing;

/// Re-encrypt the file of `source`, whose chunks are in `from`, under `key`
/// as file `id`/`version` into `to`; returns the new chunks. The plaintext
/// is checked against the source's SHA-256 on the way.
///
/// # Errors
/// As [`read_chunks`].
pub(crate) fn reencrypt(
    from: &dyn BlobStore,
    source: &FileManifest,
    max: u64,
    to: &dyn BlobStore,
    id: &str,
    version: &str,
    key: &[u8; 32],
) -> Result<Vec<FileChunk>> {
    let count = chunk_count(source.size);
    let mut chunks = Vec::with_capacity(count as usize);
    read_chunks(from, source, max, |index, plain| {
        let (sealed, digest) = seal_chunk(key, id, version, index, count, plain);
        to.put_chunk(&digest, &sealed)?;
        chunks.push(FileChunk {
            id: format!("{id}:{version}:{index}"),
            digest,
            size: plain.len() as u64,
        });
        Ok(())
    })?;
    Ok(chunks)
}

impl Client {
    /// A shared edition of a library version: the same bytes under a new
    /// key and a new identity, in the blob directory. The returned manifest
    /// carries the edition's key.
    ///
    /// # Errors
    /// Returns `Error::QuotaExceeded` for a version over 100 MiB, the cloud
    /// contract's bound, otherwise as [`read_chunks`].
    pub(crate) fn share_edition(&self, source: &FileManifest) -> Result<FileManifest> {
        ensure(source.size <= MAX_FILE, Error::QuotaExceeded)?;
        let key = Zeroizing::new(random_key());
        let id = random_hex();
        let version = random_hex();
        let blobs = self.blobs()?;
        let chunks = reencrypt(
            blobs,
            source,
            self.config.files_max_bytes,
            blobs,
            &id,
            &version,
            &key,
        )?;
        Ok(FileManifest {
            id,
            version,
            name: source.name.clone(),
            mime: source.mime.clone(),
            size: source.size,
            sha256: source.sha256.clone(),
            chunks,
            key: b64(&key[..]),
        })
    }
}
