//! Ciphertext staging and one atomic Vault/Files/outbox publication.
use crate::{
    Client,
    codec::b64,
    crypto::random_key,
    files::{self, BlobStore, FileChunk, FileManifest},
    library::catalog::{self, FileRecord, Origin, Version, VersionRef},
    vault::{Item, reading::Session},
};
use dmsg_types::{Error, MINUTE, Result, ensure};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

/// An invisible candidate. Only random-key ciphertext reaches the blob directory.
pub struct StagedFile {
    session: Session,
    pub(crate) manifest: FileManifest,
    digest: Sha256,
    finished: bool,
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        self.manifest.key.zeroize();
    }
}

impl StagedFile {
    /// New random local file ID, safe to put in the candidate item's attachment list.
    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    /// Exact candidate version.
    pub fn version(&self) -> &str {
        &self.manifest.version
    }
}

impl Client {
    fn external_import_index(&self, key: &str) -> Result<String> {
        crate::upload::hex_id(key)?;
        // An unkeyed source digest in a plaintext SQLite key would be an offline
        // guessing oracle for low-entropy exports. Scope the index with the LDK.
        Ok(format!(
            "secret-external-import:{}",
            hex::encode(
                self.keys()?
                    .derive(&("dmsg/external-import-index/1", key))
                    .as_slice()
            )
        ))
    }

    /// Original receipt for an exact external-export digest and selection.
    pub fn secret_import_receipt(&self, key: &str) -> Result<Option<Vec<String>>> {
        self.workspace
            .private(self.keys()?, &self.external_import_index(key)?)
    }

    /// Commit an external import and its retry receipt in the same local transaction.
    pub fn secret_import_commit_once(
        &mut self,
        key: &str,
        items: Vec<Item>,
        files: Vec<StagedFile>,
        folder: &str,
    ) -> Result<Vec<String>> {
        if let Some(ids) = self.secret_import_receipt(key)? {
            return Ok(ids);
        }
        let index = self.external_import_index(key)?;
        self.secret_import_commit_with(items, files, folder, |c, ids| {
            c.workspace.put_private(c.keys()?, &index, &ids)
        })
    }

    /// Start staging; no catalog or Vault writes yet.
    pub fn secret_stage_start(&mut self, name: &str, mime: &str, size: u64) -> Result<StagedFile> {
        catalog::validate_name(name)?;
        ensure(size <= files::MAX_FILE, Error::QuotaExceeded)?;
        let session = Session::start(self)?;
        Ok(StagedFile {
            session,
            manifest: FileManifest {
                id: crate::upload::random_hex(),
                version: crate::upload::random_hex(),
                name: name.into(),
                mime: mime.into(),
                size,
                sha256: String::new(),
                chunks: vec![],
                key: b64(&Zeroizing::new(random_key())[..]),
            },
            digest: Sha256::new(),
            finished: false,
        })
    }

    /// Store exactly the next bounded chunk; the producer reads/decodes outside this call.
    pub fn secret_stage_part(&mut self, file: &mut StagedFile, plain: &[u8]) -> Result<()> {
        file.session.check(self)?;
        ensure(!file.finished, Error::Expired)?;
        let m = &mut file.manifest;
        let index = m.chunks.len() as u64;
        let count = files::chunk_count(m.size);
        ensure(
            index < count
                && plain.len() as u64
                    == (m.size - index * files::CHUNK as u64).min(files::CHUNK as u64),
            Error::IntegrityFailed,
        )?;
        let key = m.file_key()?;
        let (sealed, digest) = files::seal_chunk(&key, &m.id, &m.version, index, count, plain);
        self.blobs()?.put_chunk(&digest, &sealed)?;
        file.digest.update(plain);
        m.chunks.push(FileChunk {
            id: format!("{}:{}:{index}", m.id, m.version),
            digest,
            size: plain.len() as u64,
        });
        self.hold_chunks([(
            m.id.clone(),
            crate::now_ms() + 10 * MINUTE,
            m.chunks.iter().map(|c| c.digest.clone()).collect(),
        )])
    }

    /// Verify a complete candidate. The producer must also reject excess source bytes.
    pub fn secret_stage_finish(
        &mut self,
        file: &mut StagedFile,
        expected: Option<&str>,
    ) -> Result<()> {
        file.session.check(self)?;
        let m = &mut file.manifest;
        ensure(
            !file.finished && m.chunks.len() as u64 == files::chunk_count(m.size),
            Error::IntegrityFailed,
        )?;
        m.sha256 = hex::encode(file.digest.clone().finalize());
        ensure(
            expected.is_none_or(|e| e == m.sha256),
            Error::IntegrityFailed,
        )?;
        file.finished = true;
        Ok(())
    }

    /// Publish selected items and all their new attachments in the same SQLite transaction.
    pub fn secret_import_commit(
        &mut self,
        items: Vec<Item>,
        files: Vec<StagedFile>,
        folder: &str,
    ) -> Result<Vec<String>> {
        self.secret_import_commit_with(items, files, folder, |_, _| Ok(()))
    }

    /// Include the durable import receipt in the content transaction.
    pub(crate) fn secret_import_commit_with(
        &mut self,
        items: Vec<Item>,
        files: Vec<StagedFile>,
        folder: &str,
        finish: impl FnOnce(&Self, &[String]) -> Result<()>,
    ) -> Result<Vec<String>> {
        self.catalog()?.live_folder(folder)?;
        let mut names = std::collections::BTreeSet::new();
        let now = crate::now_ms();
        let mut records = Vec::new();
        ensure(
            files.iter().map(|f| f.manifest.size).sum::<u64>() <= 256 * 1024 * 1024,
            Error::QuotaExceeded,
        )?;
        for f in &files {
            f.session.check(self)?;
            ensure(f.finished, Error::IntegrityFailed)?;
            let m = &f.manifest;
            let base = self.catalog()?.free_name(folder, &m.name);
            let mut name = base.clone();
            let mut n = 2;
            while !names.insert(name.clone()) || self.catalog()?.find(folder, &name).is_some() {
                name = format!("{} ({n})", base.chars().take(200).collect::<String>());
                n += 1;
            }
            let file = FileRecord {
                folder: folder.into(),
                name,
                mime: m.mime.clone(),
                tags: vec![],
                note: String::new(),
                starred: false,
                created_at: now,
                updated_at: now,
                current: m.version.clone(),
                versions: vec![VersionRef {
                    id: m.version.clone(),
                    size: m.size,
                    sha256: m.sha256.clone(),
                    created_at: now,
                }],
                trashed_at: None,
            };
            let version = Version {
                size: m.size,
                sha256: m.sha256.clone(),
                created_at: now,
                device: hex::encode(*self.workspace.meta.device_id),
                origin: Origin::Imported,
                key: m.key.clone(),
                chunks: m
                    .chunks
                    .iter()
                    .map(|c| crate::upload::ChunkRef {
                        digest: c.digest.clone(),
                        size: c.size,
                    })
                    .collect(),
                thumbnail: None,
                local: true,
                backup: None,
            };
            records.push((m.id.clone(), m.version.clone(), file, version));
        }
        // No input can smuggle an attachment reference to an unrelated existing file.
        let ids: std::collections::BTreeSet<_> = records.iter().map(|r| r.0.as_str()).collect();
        ensure(
            items
                .iter()
                .flat_map(|i| &i.files)
                .all(|id| ids.contains(id.as_str())),
            Error::Forbidden,
        )?;
        let added = self.workspace.store.atomic(|_| {
            for (id, version, file, record) in &records {
                self.save_version(id, version, record)?;
                self.workspace.put_private(
                    self.keys()?,
                    &format!("{}{id}", catalog::FILE),
                    file,
                )?;
            }
            let ids = self.vault_write_items(items, now)?;
            finish(self, &ids)?;
            Ok(ids)
        })?;
        for (id, _, file, mut version) in records {
            version.key.zeroize();
            self.catalog()?.put_file(&id, file);
            let _ = self.release_chunks(&id);
        }
        Ok(added)
    }
}
