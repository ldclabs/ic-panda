//! dMsg Files: the local encrypted file library (plan `misc/dMsg_files_*`).
//!
//! The library on this device is the authority on its files and their only
//! necessary copy. A file is a folder entry with metadata and a set of
//! versions; each version has its own random key and its own chunks,
//! sealed with the file format of [`crate::files`] and kept as files in the
//! workspace's `blobs/` directory ([`blobs::Blobs`]), never in the vault's
//! chunk table. The catalog of folders, files and versions is sealed under
//! the local data key ([`catalog`]) and indexed in memory while unlocked.
//!
//! Nothing here reaches the relay by itself: a backup is an explicit action
//! on a version, deleting a local file leaves its backup, and deleting a
//! backup leaves the local file. Versions form a set, not a chain, so two
//! devices never conflict. Importing streams the file through one 1 MiB
//! buffer and resumes an interrupted import on the same bytes
//! ([`import`]). An opened copy is a plaintext file in `open/` that is
//! cleaned up after a while, on lock and on exit ([`open`]); chunks nothing
//! references any more are collected from the blob directory ([`gc`]).

pub mod blobs;
pub mod catalog;
pub mod gc;
pub mod import;
pub mod open;
pub mod share;

use crate::client::Client;
use crate::codec::b64;
use crate::files::{export_manifest, read_file, FileManifest};
use crate::now_ms;
use crate::upload::random_hex;
use blobs::Blobs;
use catalog::{
    validate_name, validate_tags, version_view, Catalog, FileRecord, FileView, Folder, FolderView,
    Version, VersionView, FILE, FOLDER, MAX_NOTE, ROOT, VERSION,
};
use dmsg_types::{ensure, ensure_valid, Error, Result};
use serde::Serialize;
use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use zeroize::Zeroizing;

pub use catalog::{Backup, Origin};
pub use import::Imported;

/// The job of a version's backup, `backup:<file>:<version>`.
pub(crate) const BACKUP_JOB: &str = "backup:";
/// The job of a fetch from the cloud, `fetch:<upload>`.
pub(crate) const FETCH_JOB: &str = "fetch:";
/// Largest image returned whole by a preview.
pub const MAX_PREVIEW_IMAGE: u64 = 8 * 1024 * 1024;
/// How much of a text file a preview returns.
pub const PREVIEW_TEXT: usize = 64 * 1024;
/// Largest text file a preview reads through.
const MAX_PREVIEW_TEXT_FILE: u64 = 256 * 1024 * 1024;

/// A folder's contents.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Listing {
    /// The folder, or none for the root and the trash.
    pub folder: Option<FolderView>,
    /// Folders in it.
    pub folders: Vec<FolderView>,
    /// Files in it.
    pub files: Vec<FileView>,
}

/// A file with its versions.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Details {
    /// The file.
    pub file: FileView,
    /// Its versions, oldest first.
    pub versions: Vec<VersionView>,
}

/// What a preview holds.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    /// `image`, `text` or `none`.
    pub kind: &'static str,
    /// The file's media type.
    pub mime: String,
    /// The file's length.
    pub size: u64,
    /// The whole image, the first [`PREVIEW_TEXT`] bytes of a text, or
    /// nothing; base64url.
    pub content_base64url: Option<String>,
    /// The content is a thumbnail, not the file.
    pub thumbnail: bool,
}

/// One version's check.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct VersionCheck {
    /// File ID.
    pub file: String,
    /// Version ID.
    pub version: String,
    /// Every chunk opened and the digest matched.
    pub ok: bool,
    /// The error code otherwise.
    pub error: Option<String>,
}

/// A verification report.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct VerifyReport {
    /// Versions checked.
    pub versions: Vec<VersionCheck>,
    /// Chunk files whose bytes were compared with their names.
    pub blobs_checked: u64,
    /// Chunk files that no longer match their names.
    pub blobs_bad: Vec<String>,
}

/// What the library occupies.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    /// Files not in the trash.
    pub files: u64,
    /// Files in the trash.
    pub trashed: u64,
    /// Versions of every file.
    pub versions: u64,
    /// Versions whose chunks are on this device.
    pub local_versions: u64,
    /// Chunk files.
    pub blobs: u64,
    /// Bytes of chunk files.
    pub blob_bytes: u64,
    /// Opened plaintext copies.
    pub open_copies: u64,
    /// The configured limit of one file.
    pub max_file_bytes: u64,
    /// Days a trashed file is kept.
    pub trash_days: u64,
}

/// A writer that keeps only the first `limit` bytes.
struct Head {
    bytes: Zeroizing<Vec<u8>>,
    limit: usize,
}

impl Write for Head {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let room = self.limit.saturating_sub(self.bytes.len());
        self.bytes.extend_from_slice(&buf[..buf.len().min(room)]);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn is_text(mime: &str) -> bool {
    mime.starts_with("text/")
        || matches!(
            mime,
            "application/json" | "application/xml" | "application/toml" | "application/yaml"
        )
}

impl Client {
    /// The library's chunk directory.
    ///
    /// # Errors
    /// Returns `Error::AuthRequired` before a login selects a workspace.
    pub(crate) fn blobs(&self) -> Result<&Blobs> {
        self.blobs.as_ref().ok_or(Error::AuthRequired)
    }

    /// The catalog, read on first use after an unlock.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub(crate) fn catalog(&mut self) -> Result<&mut Catalog> {
        if self.library.is_none() {
            let catalog = Catalog::load(&self.workspace, self.keys()?)?;
            self.library = Some(catalog);
        }
        self.library.as_mut().ok_or(Error::Locked)
    }

    fn save_folder(&mut self, id: &str, folder: Folder) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{FOLDER}{id}"), &folder)?;
        self.catalog()?.folders.insert(id.into(), folder);
        Ok(())
    }

    pub(crate) fn save_file(&mut self, id: &str, file: FileRecord) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{FILE}{id}"), &file)?;
        self.catalog()?.files.insert(id.into(), file);
        Ok(())
    }

    pub(crate) fn save_version(&self, file: &str, version: &str, record: &Version) -> Result<()> {
        self.workspace
            .put_private(self.keys()?, &format!("{VERSION}{file}:{version}"), record)
    }

    /// A version record.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown version.
    pub(crate) fn version(&self, file: &str, version: &str) -> Result<Version> {
        self.workspace
            .private(self.keys()?, &format!("{VERSION}{file}:{version}"))?
            .ok_or(Error::NotFound)
    }

    /// A file's record.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown file.
    pub(crate) fn file_record(&mut self, id: &str) -> Result<FileRecord> {
        self.catalog()?
            .files
            .get(id)
            .cloned()
            .ok_or(Error::NotFound)
    }

    /// A file's version: the one named, else its current one.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown file or version.
    pub(crate) fn pick_version(
        &mut self,
        id: &str,
        version: Option<&str>,
    ) -> Result<(FileRecord, String, Version)> {
        let file = self.file_record(id)?;
        let version_id = version.unwrap_or(&file.current).to_string();
        ensure(
            file.versions.iter().any(|v| v.id == version_id),
            Error::NotFound,
        )?;
        let version = self.version(id, &version_id)?;
        Ok((file, version_id, version))
    }

    /// The manifest of a version, key included; for reading its chunks
    /// from the blob directory.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown file or version.
    pub(crate) fn library_manifest(
        &mut self,
        id: &str,
        version: Option<&str>,
    ) -> Result<(FileRecord, Version, FileManifest)> {
        let (file, version_id, record) = self.pick_version(id, version)?;
        let manifest = record.manifest(id, &version_id, &file);
        Ok((file, record, manifest))
    }

    fn file_view(&mut self, id: &str) -> Result<FileView> {
        let file = self.file_record(id)?;
        let current = self.version(id, &file.current)?;
        Ok(self.catalog()?.file_view(id, &file, &current))
    }

    /// Every folder that is not in the trash, with its path.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn files_tree(&mut self) -> Result<Vec<FolderView>> {
        let catalog = self.catalog()?;
        let mut folders: Vec<FolderView> = catalog
            .folders
            .iter()
            .filter(|(id, _)| !catalog.folder_trashed(id))
            .map(|(id, folder)| catalog.folder_view(id, folder))
            .collect();
        folders.sort_by(|a, b| (&a.path, &a.name).cmp(&(&b.path, &b.name)));
        Ok(folders)
    }

    /// The folders and files in `folder`, or with `trash` everything in the
    /// trash: trashed folders and files whose folder is not itself trashed.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown or trashed folder.
    pub fn files_list(&mut self, folder: &str, trash: bool) -> Result<Listing> {
        let (folder_view, folder_ids, file_ids) = {
            let catalog = self.catalog()?;
            if trash {
                let folders: Vec<String> = catalog
                    .folders
                    .iter()
                    .filter(|(_, f)| f.trashed_at.is_some() && !catalog.folder_trashed(&f.parent))
                    .map(|(id, _)| id.clone())
                    .collect();
                let files: Vec<String> = catalog
                    .files
                    .iter()
                    .filter(|(_, f)| f.trashed_at.is_some() && !catalog.folder_trashed(&f.folder))
                    .map(|(id, _)| id.clone())
                    .collect();
                (None, folders, files)
            } else {
                catalog.live_folder(folder)?;
                let view = catalog
                    .folders
                    .get(folder)
                    .map(|f| catalog.folder_view(folder, f));
                let folders = catalog
                    .children(folder)
                    .into_iter()
                    .filter(|id| catalog.folders[id].trashed_at.is_none())
                    .collect();
                let files = catalog
                    .files_in(folder)
                    .into_iter()
                    .filter(|id| catalog.files[id].trashed_at.is_none())
                    .collect();
                (view, folders, files)
            }
        };
        let mut folders = Vec::with_capacity(folder_ids.len());
        for id in &folder_ids {
            let catalog = self.catalog()?;
            folders.push(catalog.folder_view(id, &catalog.folders[id]));
        }
        folders.sort_by(|a, b| a.name.cmp(&b.name));
        let mut files = Vec::with_capacity(file_ids.len());
        for id in &file_ids {
            files.push(self.file_view(id)?);
        }
        files.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Listing {
            folder: folder_view,
            folders,
            files,
        })
    }

    /// A file with its versions.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown file.
    pub fn files_show(&mut self, id: &str) -> Result<Details> {
        let file = self.file_view(id)?;
        let record = self.file_record(id)?;
        let mut versions = Vec::with_capacity(record.versions.len());
        for reference in &record.versions {
            let version = self.version(id, &reference.id)?;
            versions.push(version_view(
                &reference.id,
                &version,
                reference.id == record.current,
            ));
        }
        Ok(Details { file, versions })
    }

    /// Files not in the trash whose name, tags or note contain `query`.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn files_search(&mut self, query: &str) -> Result<Vec<FileView>> {
        let query = query.trim().to_lowercase();
        ensure_valid(!query.is_empty(), "query")?;
        let ids: Vec<String> = {
            let catalog = self.catalog()?;
            catalog
                .files
                .iter()
                .filter(|(_, f)| !catalog.file_trashed(f))
                .filter(|(_, f)| {
                    f.name.to_lowercase().contains(&query)
                        || f.note.to_lowercase().contains(&query)
                        || f.tags.iter().any(|t| t.to_lowercase().contains(&query))
                })
                .map(|(id, _)| id.clone())
                .collect()
        };
        let mut files = Vec::with_capacity(ids.len());
        for id in &ids {
            files.push(self.file_view(id)?);
        }
        files.sort_by(|a, b| (&a.path, &a.name).cmp(&(&b.path, &b.name)));
        Ok(files)
    }

    /// Create a folder under `parent`; an existing one of that name is
    /// returned as it is.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown parent and
    /// `Error::InvalidInput` for a bad name.
    pub fn files_mkdir(&mut self, parent: &str, name: &str) -> Result<FolderView> {
        validate_name(name)?;
        let now = now_ms();
        let existing = {
            let catalog = self.catalog()?;
            catalog.live_folder(parent)?;
            catalog.find_folder(parent, name)
        };
        let id = match existing {
            Some(id) => id,
            None => {
                let id = random_hex();
                self.save_folder(
                    &id,
                    Folder {
                        parent: parent.into(),
                        name: name.into(),
                        created_at: now,
                        updated_at: now,
                        trashed_at: None,
                    },
                )?;
                id
            }
        };
        let catalog = self.catalog()?;
        Ok(catalog.folder_view(&id, &catalog.folders[&id]))
    }

    /// Create the folders of a `/`-separated path under the root, those
    /// already there included; returns the last one.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for an empty path.
    pub fn files_mkdir_path(&mut self, path: &str) -> Result<FolderView> {
        let names: Vec<&str> = path
            .split('/')
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .collect();
        ensure_valid(!names.is_empty(), "path")?;
        let mut parent = ROOT.to_string();
        let mut view = None;
        for name in names {
            let folder = self.files_mkdir(&parent, name)?;
            parent = folder.id.clone();
            view = Some(folder);
        }
        view.ok_or_else(|| dmsg_types::invalid("path"))
    }

    /// The folder a `/`-separated path names, from the root.
    ///
    /// # Errors
    /// Returns `Error::NotFound` when a name along the way is missing.
    pub fn files_folder_by_path(&mut self, path: &str) -> Result<String> {
        let catalog = self.catalog()?;
        let mut folder = ROOT.to_string();
        for name in path.split('/').map(str::trim).filter(|n| !n.is_empty()) {
            folder = catalog.find_folder(&folder, name).ok_or(Error::NotFound)?;
        }
        Ok(folder)
    }

    /// Rename a file or a folder.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown ID and
    /// `Error::IdempotencyConflict` when the name is taken in its folder.
    pub fn files_rename(&mut self, id: &str, name: &str) -> Result<()> {
        validate_name(name)?;
        let now = now_ms();
        if let Some(mut file) = self.catalog()?.files.get(id).cloned() {
            let taken = self.catalog()?.find(&file.folder, name);
            ensure(
                taken.as_deref().is_none_or(|t| t == id),
                Error::IdempotencyConflict,
            )?;
            file.name = name.into();
            file.updated_at = now;
            return self.save_file(id, file);
        }
        let mut folder = self
            .catalog()?
            .folders
            .get(id)
            .cloned()
            .ok_or(Error::NotFound)?;
        let taken = self.catalog()?.find_folder(&folder.parent, name);
        ensure(
            taken.as_deref().is_none_or(|t| t == id),
            Error::IdempotencyConflict,
        )?;
        folder.name = name.into();
        folder.updated_at = now;
        self.save_folder(id, folder)
    }

    /// Move files and folders into `folder`.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown ID or folder,
    /// `Error::InvalidInput` for a folder moved into itself and
    /// `Error::IdempotencyConflict` when a name is taken there.
    pub fn files_move(&mut self, ids: &[String], folder: &str) -> Result<()> {
        let now = now_ms();
        self.catalog()?.live_folder(folder)?;
        for id in ids {
            if let Some(mut file) = self.catalog()?.files.get(id).cloned() {
                let taken = self.catalog()?.find(folder, &file.name);
                ensure(
                    taken.as_deref().is_none_or(|t| t == id),
                    Error::IdempotencyConflict,
                )?;
                file.folder = folder.into();
                file.updated_at = now;
                self.save_file(id, file)?;
                continue;
            }
            let mut moved = self
                .catalog()?
                .folders
                .get(id)
                .cloned()
                .ok_or(Error::NotFound)?;
            ensure_valid(!self.catalog()?.within(folder, id), "folder")?;
            let taken = self.catalog()?.find_folder(folder, &moved.name);
            ensure(
                taken.as_deref().is_none_or(|t| t == id),
                Error::IdempotencyConflict,
            )?;
            moved.parent = folder.into();
            moved.updated_at = now;
            self.save_folder(id, moved)?;
        }
        Ok(())
    }

    /// Change a file's tags, note or star; fields given as `None` stay.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown file and
    /// `Error::InvalidInput` for too many tags or too long a note.
    pub fn files_update(
        &mut self,
        id: &str,
        tags: Option<Vec<String>>,
        note: Option<String>,
        starred: Option<bool>,
    ) -> Result<FileView> {
        let mut file = self.file_record(id)?;
        if let Some(tags) = tags {
            validate_tags(&tags)?;
            file.tags = tags.into_iter().map(|t| t.trim().to_string()).collect();
        }
        if let Some(note) = note {
            ensure_valid(note.chars().count() <= MAX_NOTE, "note")?;
            file.note = note;
        }
        if let Some(starred) = starred {
            file.starred = starred;
        }
        file.updated_at = now_ms();
        self.save_file(id, file)?;
        self.file_view(id)
    }

    /// Verify a version and write it to `out`, which must not exist yet.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown file or version,
    /// `Error::RecoveryIncomplete` for a version whose chunks are not on
    /// this device and `Error::IntegrityFailed` for anything changed.
    pub fn files_export(
        &mut self,
        id: &str,
        version: Option<&str>,
        out: &Path,
    ) -> Result<FileManifest> {
        let (_, _, manifest) = self.library_manifest(id, version)?;
        export_manifest(self.blobs()?, &manifest, self.config.files_max_bytes, out)
    }

    /// Verify a version and return its bytes, up to `max`.
    ///
    /// # Errors
    /// Returns `Error::QuotaExceeded` for a larger file, otherwise as
    /// [`Self::files_export`].
    pub fn files_read(
        &mut self,
        id: &str,
        version: Option<&str>,
        max: u64,
    ) -> Result<(FileManifest, Zeroizing<Vec<u8>>)> {
        let (_, _, manifest) = self.library_manifest(id, version)?;
        ensure(manifest.size <= max, Error::QuotaExceeded)?;
        let mut bytes = Zeroizing::new(Vec::with_capacity(manifest.size as usize));
        read_file(self.blobs()?, &manifest, max, Some(&mut *bytes))?;
        Ok((manifest.public(), bytes))
    }

    /// A preview of a version: an image's thumbnail, else the image itself
    /// up to [`MAX_PREVIEW_IMAGE`]; the first [`PREVIEW_TEXT`] bytes of a
    /// text; nothing for other types. The whole file is checked first.
    ///
    /// # Errors
    /// As [`Self::files_export`].
    pub fn files_preview(&mut self, id: &str, version: Option<&str>) -> Result<Preview> {
        let (_, record, manifest) = self.library_manifest(id, version)?;
        let mut preview = Preview {
            kind: "none",
            mime: manifest.mime.clone(),
            size: manifest.size,
            content_base64url: None,
            thumbnail: false,
        };
        if manifest.mime.starts_with("image/") {
            if let Some(digest) = &record.thumbnail {
                if let Some(thumbnail) = self.open_thumbnail(&manifest, digest)? {
                    preview.kind = "image";
                    preview.mime = "image/png".into();
                    preview.thumbnail = true;
                    preview.content_base64url = Some(b64(&thumbnail));
                    return Ok(preview);
                }
            }
            if manifest.size <= MAX_PREVIEW_IMAGE {
                let mut bytes = Zeroizing::new(Vec::with_capacity(manifest.size as usize));
                read_file(
                    self.blobs()?,
                    &manifest,
                    MAX_PREVIEW_IMAGE,
                    Some(&mut *bytes),
                )?;
                preview.kind = "image";
                preview.content_base64url = Some(b64(&bytes));
            }
            return Ok(preview);
        }
        if is_text(&manifest.mime) && manifest.size <= MAX_PREVIEW_TEXT_FILE {
            let mut head = Head {
                bytes: Zeroizing::new(Vec::new()),
                limit: PREVIEW_TEXT,
            };
            read_file(
                self.blobs()?,
                &manifest,
                MAX_PREVIEW_TEXT_FILE,
                Some(&mut head),
            )?;
            preview.kind = "text";
            preview.content_base64url = Some(b64(&head.bytes));
        }
        Ok(preview)
    }

    /// A file's versions.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown file.
    pub fn files_versions(&mut self, id: &str) -> Result<Vec<VersionView>> {
        Ok(self.files_show(id)?.versions)
    }

    /// Make a version the one the file reads as.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown file or version.
    pub fn files_version_use(&mut self, id: &str, version: &str) -> Result<FileView> {
        let (mut file, version_id, _) = self.pick_version(id, Some(version))?;
        file.current = version_id;
        file.updated_at = now_ms();
        self.save_file(id, file)?;
        self.file_view(id)
    }

    /// Delete a version on this device; its backup, if any, stays. The
    /// last version cannot go: trash the file instead.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown version and
    /// `Error::InvalidInput` for a file's only version.
    pub fn files_version_delete(&mut self, id: &str, version: &str) -> Result<FileView> {
        let (mut file, version_id, _) = self.pick_version(id, Some(version))?;
        ensure_valid(file.versions.len() > 1, "version")?;
        file.versions.retain(|v| v.id != version_id);
        if file.current == version_id {
            file.current = file.versions.last().expect("a version left").id.clone();
        }
        file.updated_at = now_ms();
        let key = format!("{VERSION}{id}:{version_id}");
        self.workspace.store.atomic(|_| {
            self.workspace
                .put_private(self.keys()?, &format!("{FILE}{id}"), &file)?;
            self.workspace.delete_private(&key)
        })?;
        self.catalog()?.files.insert(id.into(), file);
        self.collect_blobs()?;
        self.file_view(id)
    }

    /// Move files and folders to the trash; their backups stay.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown ID.
    pub fn files_trash(&mut self, ids: &[String]) -> Result<()> {
        let now = now_ms();
        for id in ids {
            if let Some(mut file) = self.catalog()?.files.get(id).cloned() {
                if file.trashed_at.is_none() {
                    file.trashed_at = Some(now);
                    file.updated_at = now;
                    self.save_file(id, file)?;
                }
                continue;
            }
            let mut folder = self
                .catalog()?
                .folders
                .get(id)
                .cloned()
                .ok_or(Error::NotFound)?;
            if folder.trashed_at.is_none() {
                folder.trashed_at = Some(now);
                folder.updated_at = now;
                self.save_folder(id, folder)?;
            }
        }
        Ok(())
    }

    /// Take files and folders out of the trash. A file whose folder is
    /// gone or trashed returns to the root.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an ID that is not in the trash.
    pub fn files_restore(&mut self, ids: &[String]) -> Result<()> {
        let now = now_ms();
        for id in ids {
            if let Some(mut file) = self.catalog()?.files.get(id).cloned() {
                ensure(file.trashed_at.is_some(), Error::NotFound)?;
                let catalog = self.catalog()?;
                if !catalog.has_folder(&file.folder) || catalog.folder_trashed(&file.folder) {
                    file.folder = ROOT.into();
                }
                file.name = catalog.free_name(&file.folder, &file.name);
                file.trashed_at = None;
                file.updated_at = now;
                self.save_file(id, file)?;
                continue;
            }
            let mut folder = self
                .catalog()?
                .folders
                .get(id)
                .cloned()
                .ok_or(Error::NotFound)?;
            ensure(folder.trashed_at.is_some(), Error::NotFound)?;
            let catalog = self.catalog()?;
            if !catalog.has_folder(&folder.parent) || catalog.folder_trashed(&folder.parent) {
                folder.parent = ROOT.into();
            }
            if catalog
                .find_folder(&folder.parent, &folder.name)
                .is_some_and(|f| f != *id)
            {
                folder.name = format!("{} ({})", folder.name, &id[..8]);
            }
            folder.trashed_at = None;
            folder.updated_at = now;
            self.save_folder(id, folder)?;
        }
        Ok(())
    }

    /// Delete trashed files and folders for good, with every version's
    /// chunks on this device; `None` empties the trash. Backups stay.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an ID that is not in the trash.
    pub fn files_purge(&mut self, ids: Option<&[String]>) -> Result<gc::Maintenance> {
        let ids: Vec<String> = match ids {
            Some(ids) => ids.to_vec(),
            None => {
                let catalog = self.catalog()?;
                catalog
                    .folders
                    .iter()
                    .filter(|(_, f)| f.trashed_at.is_some())
                    .map(|(id, _)| id.clone())
                    .chain(
                        catalog
                            .files
                            .iter()
                            .filter(|(_, f)| f.trashed_at.is_some())
                            .map(|(id, _)| id.clone()),
                    )
                    .collect()
            }
        };
        let mut report = gc::Maintenance::default();
        for id in &ids {
            let catalog = self.catalog()?;
            let trashed_file = catalog.files.get(id).map(|f| f.trashed_at.is_some());
            let trashed_folder = catalog.folders.get(id).map(|f| f.trashed_at.is_some());
            match (trashed_file, trashed_folder) {
                (Some(true), _) => {
                    self.remove_file(id)?;
                    report.purged_files += 1;
                }
                (_, Some(true)) => {
                    let (files, folders) = self.remove_folder(id)?;
                    report.purged_files += files;
                    report.purged_folders += folders;
                }
                // Already gone with its folder.
                (None, None) => {}
                _ => return Err(Error::NotFound),
            }
        }
        let (collected, tmp_removed) = self.collect_blobs()?;
        report.collected = collected;
        report.tmp_removed = tmp_removed;
        Ok(report)
    }

    /// Delete a file's records.
    pub(crate) fn remove_file(&mut self, id: &str) -> Result<()> {
        let file = self.file_record(id)?;
        self.workspace.store.atomic(|_| {
            for version in &file.versions {
                self.workspace
                    .delete_private(&format!("{VERSION}{id}:{}", version.id))?;
            }
            self.workspace.delete_private(&format!("{FILE}{id}"))
        })?;
        self.catalog()?.files.remove(id);
        Ok(())
    }

    /// Delete a folder with everything under it; returns how many files and
    /// folders went.
    fn remove_folder(&mut self, id: &str) -> Result<(u64, u64)> {
        let folders = self.catalog()?.subtree(id);
        let (mut files, mut removed) = (0, 0);
        for folder in &folders {
            for file in self.catalog()?.files_in(folder) {
                self.remove_file(&file)?;
                files += 1;
            }
            self.workspace
                .delete_private(&format!("{FOLDER}{folder}"))?;
            self.catalog()?.folders.remove(folder);
            removed += 1;
        }
        Ok((files, removed))
    }

    /// Check versions: of one file (its current version, or the one named),
    /// or without an ID every chunk file against its name and, while
    /// unlocked, every local version through its key.
    ///
    /// # Errors
    /// Returns `Error::NotFound` for an unknown file and `Error::Locked`
    /// for a named file while locked.
    pub fn files_verify(
        &mut self,
        id: Option<&str>,
        version: Option<&str>,
    ) -> Result<VerifyReport> {
        let mut report = VerifyReport::default();
        let targets: Vec<(String, String)> = match id {
            Some(id) => {
                let (file, version_id, _) = self.pick_version(id, version)?;
                match version {
                    Some(_) => vec![(id.to_string(), version_id)],
                    None => file
                        .versions
                        .iter()
                        .map(|v| (id.to_string(), v.id.clone()))
                        .collect(),
                }
            }
            None => {
                let blobs = self.blobs()?;
                report.blobs_checked = blobs.entries()?.len() as u64;
                report.blobs_bad = blobs.verify()?;
                if self.keys.is_none() {
                    return Ok(report);
                }
                let catalog = self.catalog()?;
                catalog
                    .files
                    .iter()
                    .flat_map(|(id, f)| f.versions.iter().map(move |v| (id.clone(), v.id.clone())))
                    .collect()
            }
        };
        let max = self.config.files_max_bytes;
        for (file, version_id) in targets {
            let (_, record, manifest) = self.library_manifest(&file, Some(&version_id))?;
            if !record.local {
                continue;
            }
            let checked = read_file(self.blobs()?, &manifest, max, None);
            report.versions.push(VersionCheck {
                file,
                version: version_id,
                ok: checked.is_ok(),
                error: checked.err().as_ref().map(crate::error_code),
            });
        }
        Ok(report)
    }

    /// What the library occupies on this device.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn files_usage(&mut self) -> Result<Usage> {
        let mut usage = Usage {
            max_file_bytes: self.config.files_max_bytes,
            trash_days: self.config.files_trash_days,
            ..Usage::default()
        };
        for (_, size) in self.blobs()?.entries()? {
            usage.blobs += 1;
            usage.blob_bytes += size;
        }
        usage.open_copies = self.files_open_list()?.len() as u64;
        let ids: Vec<(String, Vec<String>, bool)> = {
            let catalog = self.catalog()?;
            catalog
                .files
                .iter()
                .map(|(id, f)| {
                    (
                        id.clone(),
                        f.versions.iter().map(|v| v.id.clone()).collect(),
                        catalog.file_trashed(f),
                    )
                })
                .collect()
        };
        for (id, versions, trashed) in ids {
            match trashed {
                true => usage.trashed += 1,
                false => usage.files += 1,
            }
            for version in versions {
                usage.versions += 1;
                if self.version(&id, &version)?.local {
                    usage.local_versions += 1;
                }
            }
        }
        Ok(usage)
    }

    /// The file of this name in `folder`, if one is there and not trashed.
    ///
    /// # Errors
    /// Returns `Error::Locked` while locked.
    pub fn files_find(&mut self, folder: &str, name: &str) -> Result<Option<String>> {
        Ok(self.catalog()?.find(folder, name))
    }

    /// Write `bytes` as file `name` in `folder`, or as a new version of the
    /// file of that name there; for agents and programs that hand the
    /// daemon content instead of a path.
    ///
    /// # Errors
    /// As [`Self::files_import`].
    pub fn files_write(
        &mut self,
        folder: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<import::Imported> {
        let into = self.files_find(folder, name)?;
        self.import_stream(
            &mut std::io::Cursor::new(bytes),
            bytes.len() as u64,
            name,
            folder,
            into,
            Origin::Edited,
            &mut |_, _| {},
        )
    }

    /// Whether `grantee` holds `<name>:<folder>` for `folder` or a folder
    /// above it, or `<name>:*`.
    ///
    /// # Errors
    /// Returns `Error::Forbidden` without such a grant and
    /// `Error::NotFound` for an unknown folder.
    pub fn files_grant_check(&mut self, grantee: &str, name: &str, folder: &str) -> Result<()> {
        let chain: Vec<String> = {
            let catalog = self.catalog()?;
            ensure(catalog.has_folder(folder), Error::NotFound)?;
            let mut chain = vec![folder.to_string()];
            let mut cursor = folder;
            while let Some(found) = catalog.folders.get(cursor) {
                chain.push(found.parent.clone());
                cursor = &found.parent;
                if chain.len() > catalog.folders.len() + 1 {
                    break;
                }
            }
            chain
        };
        let granted = chain.iter().any(|f| {
            self.capability_check(grantee, &format!("{name}:{f}"))
                .is_ok()
        });
        match granted {
            true => Ok(()),
            false => self.capability_check(grantee, &format!("{name}:*")),
        }
    }

    /// The IDs of files, not in the trash, with a version of this digest.
    pub(crate) fn files_with_digest(&mut self, sha256: &str, except: &str) -> Result<Vec<String>> {
        let catalog = self.catalog()?;
        let mut found: BTreeSet<String> = catalog
            .files
            .iter()
            .filter(|(id, f)| *id != except && !catalog.file_trashed(f))
            .filter(|(_, f)| f.versions.iter().any(|v| v.sha256 == sha256))
            .map(|(id, _)| id.clone())
            .collect();
        found.remove(except);
        Ok(found.into_iter().collect())
    }

    /// The chunk digests the library's cloud work still needs: shared
    /// editions of attachments whose upload plan has not lapsed.
    pub(crate) fn library_job_blobs(&self) -> Result<BTreeSet<String>> {
        let keys = self.keys()?;
        let now = now_ms();
        let mut keep = BTreeSet::new();
        for key in self.workspace.store.sealed_keys(crate::channel::JOB)? {
            let Some(bytes) = self.workspace.open_sealed(keys, &key)? else {
                continue;
            };
            let mut plans = Vec::new();
            crate::sync::plans_in(
                &crate::codec::decode_value(&bytes, 2 * 1024 * 1024)?,
                &mut plans,
            );
            for plan in plans
                .iter()
                .filter(|p| p.kind == "file" && p.expires_at > now)
            {
                keep.extend(plan.chunks.iter().map(|c| c.digest.clone()));
            }
        }
        Ok(keep)
    }

    /// Whether a backup or a fetch of the library is in flight.
    pub(crate) fn library_jobs_running(&self) -> Result<bool> {
        Ok(self
            .workspace
            .store
            .jobs::<cbor2::Value>()?
            .iter()
            .any(|(key, _)| key.starts_with(BACKUP_JOB) || key.starts_with(FETCH_JOB)))
    }

    /// Forget the library's index and clean up opened copies; on lock.
    pub(crate) fn library_lock(&mut self) {
        if self.keys.is_some() {
            let _ = self.open_copies_on_lock();
        }
        self.library = None;
    }
}

#[cfg(test)]
#[path = "library_tests.rs"]
mod tests;
