//! What the daemon's loopback WebDAV mount needs of the library: names
//! resolved along a path, one chunk of a version at a time, a finished
//! upload written as a new version, a copy, and the replacement an editor's
//! safe save makes (write a temporary file, move it over the original).
//! No WebDAV and no operating system here; the mount itself lives in the
//! daemon.

use super::Client;
use super::blobs::create_private_dir;
use super::catalog::{FILE, FileRecord, FolderView, Origin, Version, VersionRef, validate_name};
use super::catalog::{FileView, ROOT};
use super::import::{Imported, thumb_aad};
use super::share::reencrypt;
use crate::codec::{b64, sha256};
use crate::crypto::{random_key, seal_bytes};
use crate::files::{BlobStore, io, read_chunk};
use crate::now_ms;
use crate::upload::{ChunkRef, random_hex};
use dmsg_types::{Error, Result, ensure};
use serde::Serialize;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

/// The mount's upload directory in the workspace directory: a PUT's bytes
/// wait here, owner-only, until the whole file has arrived and is imported.
pub(crate) const MOUNT_DIR: &str = "mount";

/// What a path names.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum Node {
    /// A folder; the root has the ID [`ROOT`] and no name.
    Folder(FolderView),
    /// A file.
    File(FileView),
}

impl Client {
    /// The folder or file a `/`-separated path from the root names, outside
    /// the trash; a folder first when a folder and a file share a name.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn files_lookup(&mut self, path: &str) -> Result<Option<Node>> {
        let mut names = path.split('/').filter(|n| !n.is_empty()).peekable();
        let mut folder = ROOT.to_string();
        while let Some(name) = names.next() {
            let catalog = self.catalog()?;
            if let Some(id) = catalog.find_folder(&folder, name) {
                folder = id;
                continue;
            }
            if names.peek().is_some() {
                return Ok(None);
            }
            return match catalog.find(&folder, name) {
                Some(id) => Ok(Some(Node::File(self.file_view(&id)?))),
                None => Ok(None),
            };
        }
        let catalog = self.catalog()?;
        let view = match catalog.folders.get(&folder) {
            Some(record) => catalog.folder_view(&folder, record),
            None => FolderView {
                id: ROOT.into(),
                parent: String::new(),
                name: String::new(),
                created_at: 0,
                updated_at: catalog
                    .folders
                    .values()
                    .map(|f| f.updated_at)
                    .chain(catalog.files.values().map(|f| f.updated_at))
                    .max()
                    .unwrap_or(0),
                trashed_at: None,
                path: vec![],
            },
        };
        Ok(Some(Node::Folder(view)))
    }

    /// Chunk `index` of a file's version, decrypted on its own; for reads at
    /// an offset, which name the version they began with so that a version
    /// made meanwhile never mixes in. The chunk is checked, the whole file
    /// is not.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown file or version, otherwise
    /// as [`read_chunk`].
    pub fn files_chunk(
        &mut self,
        id: &str,
        version: &str,
        index: u64,
    ) -> Result<Zeroizing<Vec<u8>>> {
        let (_, _, manifest) = self.library_manifest(id, Some(version))?;
        read_chunk(self.blobs()?, &manifest, index)
    }

    /// What the mounted disk reports as used: the current versions of the
    /// files outside the trash, from the catalog alone, since the systems
    /// ask for it with many listings.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn files_mount_used(&mut self) -> Result<u64> {
        let catalog = self.catalog()?;
        Ok(catalog
            .files
            .values()
            .filter(|f| !catalog.file_trashed(f))
            .filter_map(|f| f.versions.iter().find(|v| v.id == f.current))
            .map(|v| v.size)
            .sum())
    }

    /// The mount's upload directory, created owner-only when missing.
    ///
    /// # Errors
    /// Returns `Error::AuthRequired` before an account is selected.
    pub fn files_mount_dir(&self) -> Result<PathBuf> {
        let dir = self
            .blobs()?
            .dir()
            .parent()
            .ok_or(Error::AuthRequired)?
            .join(MOUNT_DIR);
        create_private_dir(&dir)?;
        Ok(dir)
    }

    /// Remove the mount's upload directory with whatever a stopped upload
    /// left in it; on lock and before a mount.
    pub fn files_mount_clean(&self) {
        if let Some(dir) = self.blobs().ok().and_then(|b| b.dir().parent()) {
            let _ = std::fs::remove_dir_all(dir.join(MOUNT_DIR));
        }
    }

    /// Import the bytes at `path` as file `name` in `folder`, or as a new
    /// version of the file of that name there, with the origin `mounted`.
    /// The file at `path` stays; the caller removes it.
    ///
    /// # Errors
    /// As [`Self::files_import`].
    pub fn files_write_mounted(
        &mut self,
        folder: &str,
        name: &str,
        path: &Path,
    ) -> Result<Imported> {
        let mut file = std::fs::File::open(path).map_err(io)?;
        let size = file.metadata().map_err(io)?.len();
        let into = self.files_find(folder, name)?;
        self.import_stream(
            &mut file,
            size,
            name,
            folder,
            into,
            Origin::Mounted,
            &mut |_, _| {},
        )
    }

    /// Copy a file's current version as file `name` in `folder`: the same
    /// bytes under a new key and a new identity, with the tags and the
    /// note; for the mount's COPY.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown file or folder,
    /// `Error::IdempotencyConflict` when the name is taken there and
    /// `Error::InvalidInput` for a bad name.
    pub fn files_copy(&mut self, id: &str, folder: &str, name: &str) -> Result<FileView> {
        validate_name(name)?;
        let catalog = self.catalog()?;
        catalog.live_folder(folder)?;
        ensure(
            catalog.find(folder, name).is_none() && catalog.find_folder(folder, name).is_none(),
            Error::IdempotencyConflict,
        )?;
        let source = self.file_record(id)?;
        let now = now_ms();
        let new_id = random_hex();
        let (version_id, version) = self.reencrypted_version(id, &new_id, now)?;
        let file = FileRecord {
            folder: folder.into(),
            name: name.into(),
            mime: source.mime,
            tags: source.tags,
            note: source.note,
            starred: false,
            created_at: now,
            updated_at: now,
            current: version_id.clone(),
            versions: vec![VersionRef {
                id: version_id.clone(),
                size: version.size,
                sha256: version.sha256.clone(),
                created_at: now,
            }],
            trashed_at: None,
        };
        self.workspace.store.atomic(|_| {
            self.save_version(&new_id, &version_id, &version)?;
            self.workspace
                .put_private(self.keys()?, &format!("{FILE}{new_id}"), &file)
        })?;
        self.catalog()?.put_file(&new_id, file);
        self.file_view(&new_id)
    }

    /// An editor's safe save through the mount: `from`'s current version
    /// becomes a new version of `into`, re-encrypted under `into`'s
    /// identity; `into` leaves the trash if the editor's move put it there
    /// first, and `from` goes away for good, its bytes living on in `into`.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown file.
    pub fn files_replace(&mut self, from: &str, into: &str) -> Result<FileView> {
        ensure(from != into, Error::InvalidInput("into".into()))?;
        let source = self.file_record(from)?;
        let mut target = self.file_record(into)?;
        let now = now_ms();
        let (version_id, version) = self.reencrypted_version(from, into, now)?;
        target.mime = source.mime;
        target.current = version_id.clone();
        target.updated_at = now;
        target.trashed_at = None;
        target.versions.push(VersionRef {
            id: version_id.clone(),
            size: version.size,
            sha256: version.sha256.clone(),
            created_at: now,
        });
        self.workspace.store.atomic(|_| {
            self.save_version(into, &version_id, &version)?;
            self.workspace
                .put_private(self.keys()?, &format!("{FILE}{into}"), &target)?;
            for old in &source.versions {
                self.workspace.delete_private(&format!(
                    "{}{from}:{}",
                    super::catalog::VERSION,
                    old.id
                ))?;
            }
            self.workspace.delete_private(&format!("{FILE}{from}"))
        })?;
        let catalog = self.catalog()?;
        catalog.put_file(into, target);
        catalog.drop_file(from);
        self.collect_blobs()?;
        self.file_view(into)
    }

    /// `from`'s current version re-encrypted as a new version of file
    /// `into_id`, thumbnail included; not yet saved.
    fn reencrypted_version(
        &mut self,
        from: &str,
        into_id: &str,
        now: u64,
    ) -> Result<(String, Version)> {
        let (_, record, source) = self.library_manifest(from, None)?;
        let key = Zeroizing::new(random_key());
        let version_id = random_hex();
        let blobs = self.blobs()?;
        let chunks = reencrypt(
            blobs,
            &source,
            self.config.files_max_bytes,
            blobs,
            into_id,
            &version_id,
            &key,
        )?;
        let thumbnail = match &record.thumbnail {
            Some(digest) => match self.open_thumbnail(&source, digest)? {
                Some(png) => {
                    let sealed = seal_bytes(&key, &png, &thumb_aad(into_id, &version_id));
                    let digest = hex::encode(*sha256(&sealed));
                    blobs.put_chunk(&digest, &sealed)?;
                    Some(digest)
                }
                None => None,
            },
            None => None,
        };
        let version = Version {
            size: source.size,
            sha256: source.sha256,
            created_at: now,
            device: hex::encode(*self.workspace.meta.device_id),
            origin: Origin::Mounted,
            key: b64(&key[..]),
            chunks: chunks
                .into_iter()
                .map(|c| ChunkRef {
                    digest: c.digest,
                    size: c.size,
                })
                .collect(),
            thumbnail,
            local: true,
            backup: None,
        };
        Ok((version_id, version))
    }
}
