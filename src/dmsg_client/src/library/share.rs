//! Shared editions: a version re-encrypted under a fresh key with a fresh
//! file identity, so what a channel member (or, later, a link holder)
//! receives shares neither a key nor a chunk byte with the library's own
//! copy or with a backup. The relay and the recipients cannot relate an
//! edition to anything else of the account. An edition's chunks live in the
//! blob directory until its message is stored, then the collector takes
//! them.

use crate::codec::b64;
use crate::crypto::random_key;
use crate::files::{
    BlobStore, FileChunk, FileManifest, MAX_FILE, chunk_count, read_chunks, seal_chunk,
};
use crate::upload::random_hex;
use dmsg_types::{Error, Result, ensure};
use zeroize::Zeroizing;

/// How long a prepared edition's temporary chunk references last without the
/// next chunk refreshing them. An interrupted preparation then stops pinning
/// its ciphertext soon instead of for an hour.
const EDITION_HOLD_MS: u64 = 10 * dmsg_types::MINUTE;

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

/// Re-encrypt a shared edition one chunk at a time. Temporary references keep
/// completed chunks alive until the caller journals the final upload plan.
pub(crate) async fn edition<H: crate::hold::Hold>(
    h: &H,
    source: &FileManifest,
) -> Result<FileManifest> {
    use sha2::{Digest, Sha256};
    ensure(source.size <= MAX_FILE, Error::QuotaExceeded)?;
    ensure(
        source.chunks.len() as u64 == chunk_count(source.size),
        Error::IntegrityFailed,
    )?;
    let generation = h
        .with(|c| {
            c.keys()?;
            Ok(c.session_generation)
        })
        .await?;
    let key = Zeroizing::new(random_key());
    let id = random_hex();
    let version = random_hex();
    let count = chunk_count(source.size);
    let mut chunks = Vec::with_capacity(count as usize);
    let mut overall = Sha256::new();
    for index in 0..count {
        h.with(|c| {
            c.keys()?;
            ensure(c.session_generation == generation, Error::Locked)?;
            let plain = crate::files::read_chunk(c.blobs()?, source, index)?;
            overall.update(plain.as_slice());
            let (sealed, digest) = seal_chunk(&key, &id, &version, index, count, &plain);
            c.blobs()?.put_chunk(&digest, &sealed)?;
            chunks.push(FileChunk {
                id: format!("{id}:{version}:{index}"),
                digest,
                size: plain.len() as u64,
            });
            c.hold_chunks([(
                id.clone(),
                crate::now_ms() + EDITION_HOLD_MS,
                chunks.iter().map(|c| c.digest.clone()).collect(),
            )])
        })
        .await?;
    }
    ensure(
        hex::encode(overall.finalize()) == source.sha256,
        Error::IntegrityFailed,
    )?;
    h.with(|c| {
        c.keys()?;
        ensure(c.session_generation == generation, Error::Locked)
    })
    .await?;
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
