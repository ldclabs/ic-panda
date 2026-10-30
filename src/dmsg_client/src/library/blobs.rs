//! The library's ciphertext chunks as files: `blobs/<d0d1>/<digest>`, the
//! first two hex digits of the digest naming the shard directory, so the
//! directory is its own index: a chunk exists when its file does, and its
//! name is its digest. A chunk is written to `blobs/tmp/<random>`, synced
//! and then renamed into place, so a crash leaves no half-written chunk
//! under a digest. Directories are the owner's only (0700), files 0600.

use crate::codec::sha256;
use crate::files::BlobStore;
use crate::storage;
use dmsg_types::{Error, Result, ensure};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The temporary files' directory under the blobs directory.
const TMP: &str = "tmp";

/// A content-addressed chunk directory.
#[derive(Clone, Debug)]
pub struct Blobs {
    dir: PathBuf,
}

fn is_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Create a directory only its owner may enter, with any missing parents.
pub(crate) fn create_private_dir(path: &Path) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path).map_err(storage)
}

/// Open a new file only its owner may read.
pub(crate) fn create_private_file(path: &Path) -> Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path).map_err(storage)
}

impl Blobs {
    /// The chunk directory at `dir`; created on the first write.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// The directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, digest: &str) -> Result<PathBuf> {
        ensure(is_digest(digest), Error::IntegrityFailed)?;
        Ok(self.dir.join(&digest[..2]).join(digest))
    }

    /// Copy chunk `digest` from `from` once its bytes match it: a clone
    /// where the file system shares blocks (APFS), so a library imported
    /// from a copy takes no space twice. Returns the bytes copied: none when
    /// the chunk is here already or `from` lacks it.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` for a chunk that does not match its
    /// digest and `Error::Unavailable` when the files cannot be copied.
    pub fn copy_from(&self, from: &Blobs, digest: &str) -> Result<u64> {
        let path = self.path(digest)?;
        if path.is_file() {
            return Ok(0);
        }
        let Some(bytes) = from.chunk(digest)? else {
            return Ok(0);
        };
        let tmp = self.dir.join(TMP);
        create_private_dir(&tmp)?;
        let partial = tmp.join(hex::encode(&crate::crypto::random_id()[..16]));
        let copied = (|| {
            fs::copy(from.path(digest)?, &partial).map_err(storage)?;
            create_private_dir(path.parent().expect("a shard directory"))?;
            fs::rename(&partial, &path).map_err(storage)
        })();
        if copied.is_err() {
            let _ = fs::remove_file(&partial);
        }
        copied.map(|()| bytes.len() as u64)
    }

    /// A stored chunk's length, without reading it.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the file cannot be read.
    pub fn size(&self, digest: &str) -> Result<Option<u64>> {
        match fs::metadata(self.path(digest)?) {
            Ok(metadata) => Ok(Some(metadata.len())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(storage(e)),
        }
    }

    /// Delete a chunk; one already gone is no error.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the file cannot be removed.
    pub fn delete(&self, digest: &str) -> Result<()> {
        match fs::remove_file(self.path(digest)?) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(storage(e)),
        }
    }

    /// Every stored chunk's digest and size.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the directory cannot be read.
    pub fn entries(&self) -> Result<Vec<(String, u64)>> {
        let mut found = Vec::new();
        let shards = match fs::read_dir(&self.dir) {
            Ok(shards) => shards,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(found),
            Err(e) => return Err(storage(e)),
        };
        for shard in shards {
            let shard = shard.map_err(storage)?;
            let name = shard.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if name.len() != 2 || !shard.file_type().map_err(storage)?.is_dir() {
                continue;
            }
            for entry in fs::read_dir(shard.path()).map_err(storage)? {
                let entry = entry.map_err(storage)?;
                let file = entry.file_name();
                let Some(digest) = file
                    .to_str()
                    .filter(|d| is_digest(d) && d.starts_with(name))
                else {
                    continue;
                };
                let metadata = entry.metadata().map_err(storage)?;
                if metadata.is_file() {
                    found.push((digest.to_string(), metadata.len()));
                }
            }
        }
        found.sort();
        Ok(found)
    }

    /// Check every stored chunk against its name; returns how many were
    /// checked and the digests of those that no longer match, unreadable
    /// ones included. Needs no key, and nothing else of the library: it may
    /// run beside it. A chunk the collector removes meanwhile is skipped.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the directory cannot be listed.
    pub fn verify(&self) -> Result<(u64, Vec<String>)> {
        let mut checked = 0;
        let mut bad = Vec::new();
        for (digest, _) in self.entries()? {
            let matches = match fs::read(self.path(&digest)?) {
                Ok(bytes) => hex::encode(*sha256(&bytes)) == digest,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => false,
            };
            checked += 1;
            if !matches {
                bad.push(digest);
            }
        }
        Ok((checked, bad))
    }

    /// Delete temporary files older than `age`; returns how many.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the directory cannot be read.
    pub fn sweep_tmp(&self, age: Duration) -> Result<u64> {
        let entries = match fs::read_dir(self.dir.join(TMP)) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(storage(e)),
        };
        let cutoff = SystemTime::now().checked_sub(age);
        let mut removed = 0;
        for entry in entries {
            let entry = entry.map_err(storage)?;
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .map_err(storage)?;
            if cutoff.is_some_and(|c| modified <= c) {
                fs::remove_file(entry.path()).map_err(storage)?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}

impl BlobStore for Blobs {
    fn chunk(&self, digest: &str) -> Result<Option<Vec<u8>>> {
        let bytes = match fs::read(self.path(digest)?) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(storage(e)),
        };
        ensure(
            hex::encode(*sha256(&bytes)) == digest,
            Error::IntegrityFailed,
        )?;
        Ok(Some(bytes))
    }

    fn has_chunk(&self, digest: &str) -> Result<bool> {
        Ok(self.path(digest)?.is_file())
    }

    fn put_chunk(&self, digest: &str, bytes: &[u8]) -> Result<()> {
        ensure(
            hex::encode(*sha256(bytes)) == digest,
            Error::IntegrityFailed,
        )?;
        let path = self.path(digest)?;
        if path.is_file() {
            return Ok(());
        }
        let tmp = self.dir.join(TMP);
        create_private_dir(&tmp)?;
        let partial = tmp.join(hex::encode(&crate::crypto::random_id()[..16]));
        let written = (|| {
            let mut file = create_private_file(&partial)?;
            file.write_all(bytes).map_err(storage)?;
            file.sync_all().map_err(storage)?;
            create_private_dir(path.parent().expect("a shard directory"))?;
            fs::rename(&partial, &path).map_err(storage)
        })();
        if written.is_err() {
            let _ = fs::remove_file(&partial);
        }
        written
    }
}
