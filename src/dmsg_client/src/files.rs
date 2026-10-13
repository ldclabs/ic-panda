//! Files (encryption design §7): at most 100 MiB, sealed in independent
//! 1 MiB chunks under a random file key with the deterministic nonce
//! `"dMsg" || u64_be(index)` and
//! `ChunkAAD = ["dmsg/file-chunk/1", fileId, version, index, count, length]`.
//!
//! The file manifest (name, type, length, SHA-256, chunk list and file key)
//! travels only inside an encrypted vault item and the sealed upload
//! manifest. Importing keeps a plan sealed under the local data key, so an
//! interrupted import resumes with the same key and reuses every chunk it
//! already stored after checking the plaintext again.

use crate::client::Client;
use crate::codec::{b64, sha256, unb64};
use crate::content::VAULT;
use crate::crypto::{open, random_key, seal_with_nonce, Key};
use crate::upload::random_hex;
use crate::vault::{Item, VaultEntry};
use dmsg_types::{ensure, ensure_valid, Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Path;
use zeroize::Zeroizing;

/// Plaintext bytes per chunk.
pub const CHUNK: usize = 1024 * 1024;
/// Largest file.
pub const MAX_FILE: u64 = 100 * 1024 * 1024;
const JOB: &str = "file-job:";

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
    /// File ID, hex; also the vault item ID.
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
    fn file_key(&self) -> Result<Zeroizing<Key>> {
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

/// An unfinished import, sealed under the local data key.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileJob {
    manifest: FileManifest,
    /// SHA-256 of each sealed chunk's plaintext.
    plain: Vec<String>,
}

fn chunk_count(size: u64) -> u64 {
    size.div_ceil(CHUNK as u64)
}

fn nonce(index: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[..4].copy_from_slice(b"dMsg");
    nonce[4..].copy_from_slice(&index.to_be_bytes());
    nonce
}

fn chunk_aad(
    manifest: &FileManifest,
    index: u64,
    count: u64,
    length: u64,
) -> (&'static str, &str, &str, u64, u64, u64) {
    (
        "dmsg/file-chunk/1",
        &manifest.id,
        &manifest.version,
        index,
        count,
        length,
    )
}

/// A media type from the file name's extension.
fn mime(name: &str) -> &'static str {
    let extension = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    match extension.as_deref() {
        Some("txt") => "text/plain",
        Some("md") => "text/markdown",
        Some("csv") => "text/csv",
        Some("json") => "application/json",
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    }
}

fn io(error: std::io::Error) -> Error {
    Error::InvalidInput(format!("file: {error}"))
}

/// Read up to one chunk.
fn read_chunk(file: &mut std::fs::File, buffer: &mut [u8]) -> Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match file.read(&mut buffer[filled..]).map_err(io)? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

impl Client {
    /// Encrypt the file at `path` into the vault. An unfinished import of a
    /// file with the same name and length resumes; its stored chunks are
    /// reused only when the plaintext still matches.
    ///
    /// # Errors
    /// Returns `Error::QuotaExceeded` beyond 100 MiB and
    /// `Error::IntegrityFailed` when a resumed file changed.
    pub fn file_import(&mut self, path: &Path) -> Result<VaultEntry> {
        self.roots()?;
        let metadata = std::fs::metadata(path).map_err(io)?;
        ensure_valid(metadata.is_file(), "path")?;
        ensure(metadata.len() <= MAX_FILE, Error::QuotaExceeded)?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| dmsg_types::invalid("name"))?
            .to_string();
        ensure_valid(!name.is_empty() && name.chars().count() <= 255, "name")?;
        let size = metadata.len();
        let mut job = match self.import_job(&name, size)? {
            Some(job) => job,
            None => FileJob {
                manifest: FileManifest {
                    id: random_hex(),
                    version: random_hex(),
                    mime: mime(&name).into(),
                    name,
                    size,
                    sha256: String::new(),
                    chunks: vec![],
                    key: b64(&Zeroizing::new(random_key())[..]),
                },
                plain: vec![],
            },
        };
        let job_key = format!("{JOB}{}", job.manifest.version);
        self.workspace.put_private(self.keys()?, &job_key, &job)?;
        let file_key = job.manifest.file_key()?;
        let count = chunk_count(size);
        let mut file = std::fs::File::open(path).map_err(io)?;
        let mut overall = Sha256::new();
        let mut buffer = Zeroizing::new(vec![0u8; CHUNK]);
        for index in 0..count {
            let length = read_chunk(&mut file, &mut buffer)?;
            let expected = (size - index * CHUNK as u64).min(CHUNK as u64);
            ensure(length as u64 == expected, Error::IntegrityFailed)?;
            let plaintext = &buffer[..length];
            overall.update(plaintext);
            let plain = hex::encode(*sha256(plaintext));
            if let Some(stored) = job.manifest.chunks.get(index as usize) {
                ensure(
                    job.plain[index as usize] == plain
                        && self.workspace.store.has_chunk(&stored.digest)?,
                    Error::IntegrityFailed,
                )?;
                continue;
            }
            let sealed = unb64(&seal_with_nonce(
                &file_key,
                plaintext,
                &chunk_aad(&job.manifest, index, count, length as u64),
                &nonce(index),
            ))?;
            let digest = hex::encode(*sha256(&sealed));
            self.workspace.store.put_chunk(&digest, &sealed)?;
            job.manifest.chunks.push(FileChunk {
                id: format!("{}:{}:{index}", job.manifest.id, job.manifest.version),
                digest,
                size: length as u64,
            });
            job.plain.push(plain);
            self.workspace.put_private(self.keys()?, &job_key, &job)?;
        }
        ensure(
            read_chunk(&mut file, &mut buffer[..1])? == 0,
            Error::IntegrityFailed,
        )?;
        job.manifest.sha256 = hex::encode(overall.finalize());
        let manifest = job.manifest;
        // A crash after the item was written leaves the job; finish it.
        let entry = match self.workspace.store.head(&manifest.id)? {
            Some(_) => self.vault_show(&manifest.id)?,
            None => {
                let now = crate::now_ms();
                let item = Item {
                    kind: "file".into(),
                    title: manifest.name.clone(),
                    tags: vec![],
                    body: String::new(),
                    username: String::new(),
                    secret: String::new(),
                    url: String::new(),
                    favorite: false,
                    created_at: now,
                    updated_at: now,
                    resolved_conflicts: None,
                    file: Some(manifest.clone()),
                };
                item.validate()?;
                let payload = Zeroizing::new(crate::codec::canonical(&item));
                let (record, _) = self.write(
                    VAULT,
                    manifest.id.clone(),
                    None,
                    false,
                    &payload,
                    Some(&manifest),
                )?;
                VaultEntry::new(&record, item, false, false)
            }
        };
        self.workspace.delete_private(&job_key)?;
        Ok(entry)
    }

    /// Seal in-memory `bytes` as a file whose chunks go to the store, for
    /// records other than vault items that carry a file.
    ///
    /// # Errors
    /// Returns `Error::QuotaExceeded` beyond 100 MiB.
    pub(crate) fn store_file(&self, name: &str, mime: &str, bytes: &[u8]) -> Result<FileManifest> {
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
            let sealed = unb64(&seal_with_nonce(
                &key,
                plaintext,
                &chunk_aad(&manifest, index, count, plaintext.len() as u64),
                &nonce(index),
            ))?;
            let digest = hex::encode(*sha256(&sealed));
            self.workspace.store.put_chunk(&digest, &sealed)?;
            manifest.chunks.push(FileChunk {
                id: format!("{}:{}:{index}", manifest.id, manifest.version),
                digest,
                size: plaintext.len() as u64,
            });
        }
        Ok(manifest)
    }

    /// The unfinished import of a file with this name and length.
    fn import_job(&self, name: &str, size: u64) -> Result<Option<FileJob>> {
        for key in self.workspace.store.sealed_keys(JOB)? {
            let job: Option<FileJob> = self.workspace.private(self.keys()?, &key)?;
            if let Some(job) = job.filter(|j| j.manifest.name == name && j.manifest.size == size) {
                return Ok(Some(job));
            }
        }
        Ok(None)
    }

    /// The chunks the unfinished imports already stored; a resumed import
    /// reuses them.
    pub(crate) fn import_chunks(&self) -> Result<Vec<String>> {
        let mut digests = Vec::new();
        for key in self.workspace.store.sealed_keys(JOB)? {
            if let Some(job) = self.workspace.private::<FileJob>(self.keys()?, &key)? {
                digests.extend(job.manifest.chunks.into_iter().map(|c| c.digest));
            }
        }
        Ok(digests)
    }

    /// Check every chunk of a file and its SHA-256, writing the plaintext to
    /// `out` when given.
    ///
    /// # Errors
    /// A missing chunk returns `Error::RecoveryIncomplete`; anything changed
    /// returns `Error::IntegrityFailed`.
    pub(crate) fn read_file(
        &self,
        manifest: &FileManifest,
        mut out: Option<&mut dyn Write>,
    ) -> Result<()> {
        let count = chunk_count(manifest.size);
        ensure(
            manifest.size <= MAX_FILE && manifest.chunks.len() as u64 == count,
            Error::IntegrityFailed,
        )?;
        let key = manifest.file_key()?;
        let mut overall = Sha256::new();
        for (index, chunk) in manifest.chunks.iter().enumerate() {
            let index = index as u64;
            let expected = (manifest.size - index * CHUNK as u64).min(CHUNK as u64);
            ensure(
                chunk.id == format!("{}:{}:{index}", manifest.id, manifest.version)
                    && chunk.size == expected,
                Error::IntegrityFailed,
            )?;
            let sealed = self
                .workspace
                .store
                .chunk(&chunk.digest)?
                .ok_or(Error::RecoveryIncomplete)?;
            let plain = open(
                &key,
                &b64(&sealed),
                &chunk_aad(manifest, index, count, chunk.size),
            )?;
            ensure(plain.len() as u64 == chunk.size, Error::IntegrityFailed)?;
            overall.update(plain.as_slice());
            if let Some(out) = out.as_mut() {
                out.write_all(&plain).map_err(io)?;
            }
        }
        ensure(
            hex::encode(overall.finalize()) == manifest.sha256,
            Error::IntegrityFailed,
        )
    }

    fn file_manifest(&self, id: &str) -> Result<FileManifest> {
        self.vault_show(id)?.item.file.ok_or(Error::NotFound)
    }

    /// Verify a file's every chunk and its SHA-256.
    ///
    /// # Errors
    /// As [`Self::file_export`].
    pub fn file_verify(&self, id: &str) -> Result<FileManifest> {
        let manifest = self.file_manifest(id)?;
        self.read_file(&manifest, None)?;
        Ok(manifest.public())
    }

    /// Verify a file of at most `max` bytes and return it.
    ///
    /// # Errors
    /// Returns `Error::QuotaExceeded` for a larger file, otherwise as
    /// [`Self::file_export`].
    pub fn file_bytes(&self, id: &str, max: u64) -> Result<(FileManifest, Zeroizing<Vec<u8>>)> {
        let manifest = self.file_manifest(id)?;
        ensure(manifest.size <= max, Error::QuotaExceeded)?;
        let mut bytes = Zeroizing::new(Vec::with_capacity(manifest.size as usize));
        self.read_file(&manifest, Some(&mut *bytes))?;
        Ok((manifest.public(), bytes))
    }

    /// Verify a file and write it to `out`, which must not exist yet. The
    /// bytes go to a temporary file first and appear at `out` only once the
    /// whole file has been checked.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an item that is not a file,
    /// `Error::InvalidInput` when `out` exists, `Error::RecoveryIncomplete`
    /// for chunks not synced yet and `Error::IntegrityFailed` for anything
    /// changed.
    pub fn file_export(&self, id: &str, out: &Path) -> Result<FileManifest> {
        self.export_manifest(&self.file_manifest(id)?, out)
    }

    /// Verify the file of `manifest`, whose chunks are in the store, and
    /// write it to `out` as [`Self::file_export`] does.
    ///
    /// # Errors
    /// As [`Self::file_export`].
    pub(crate) fn export_manifest(
        &self,
        manifest: &FileManifest,
        out: &Path,
    ) -> Result<FileManifest> {
        ensure_valid(out.is_absolute() && !out.exists(), "out")?;
        let name = out
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| dmsg_types::invalid("out"))?;
        let partial = out.with_file_name(format!(".{name}.dmsg-part"));
        let written = (|| {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&partial)
                .map_err(io)?;
            self.read_file(manifest, Some(&mut file))?;
            file.sync_all().map_err(io)?;
            std::fs::rename(&partial, out).map_err(io)
        })();
        if written.is_err() {
            let _ = std::fs::remove_file(&partial);
        }
        written.map(|_| manifest.public())
    }
}

#[cfg(test)]
#[path = "files_tests.rs"]
mod tests;
