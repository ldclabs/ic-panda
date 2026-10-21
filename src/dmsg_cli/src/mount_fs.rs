//! Account-bound library operations for the WebDAV handler. HTTP validation
//! stays in dav-server; only catalog and streaming semantics live here.

use dav_server::{
    davpath::DavPath,
    fs::{
        DavDirEntry, DavFile, DavFileSystem, DavMetaData, FsError, FsFuture, FsResult, FsStream,
        OpenOptions, ReadDirMeta,
    },
};
use dmsg_client::{
    Client,
    files::CHUNK,
    library::{
        catalog::{FileView, FolderView, ROOT, validate_name},
        mount::Node,
    },
    now_ms,
};
use dmsg_types::Error;
use futures_util::FutureExt;
use std::{
    io::{SeekFrom, Write},
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;
use zeroize::Zeroizing;

/// Names the systems' file managers leave beside files; accepted and
/// dropped, never listed.
pub(crate) fn noise(name: &str) -> bool {
    name.starts_with("._")
        || matches!(
            name,
            ".DS_Store"
                | "Thumbs.db"
                | "desktop.ini"
                | ".Spotlight-V100"
                | ".Trashes"
                | ".fseventsd"
                | ".TemporaryItems"
                | ".metadata_never_index"
                | ".ql_disablethumbnails"
                | ".ql_disablecache"
                | ".hidden"
                | ".apdisk"
                | ".com.apple.timemachine.donotpresent"
        )
}

fn fs_error(error: Error) -> FsError {
    match error {
        Error::NotFound => FsError::NotFound,
        Error::IdempotencyConflict => FsError::Exists,
        Error::QuotaExceeded => FsError::TooLarge,
        Error::InvalidInput(_) | Error::Locked | Error::AuthRequired | Error::Forbidden => {
            FsError::Forbidden
        }
        _ => FsError::GeneralFailure,
    }
}

/// A library path from a share path.
fn path_of(path: &DavPath) -> FsResult<String> {
    String::from_utf8(path.as_bytes().to_vec()).map_err(|_| FsError::Forbidden)
}

/// A path's parent and name.
pub(super) fn split(path: &str) -> (String, String) {
    let trimmed = path.trim_matches('/');
    match trimmed.rsplit_once('/') {
        Some((parent, name)) => (parent.to_string(), name.to_string()),
        None => (String::new(), trimmed.to_string()),
    }
}

/// The library as a WebDAV filesystem.
#[derive(Clone)]
pub(crate) struct LibraryFs {
    client: Arc<Mutex<Client>>,
    account: String,
    deleting: bool,
    moving: bool,
}

/// What the share says of a folder or file.
#[derive(Debug, Clone)]
struct Meta {
    len: u64,
    /// Unix milliseconds.
    modified: u64,
    dir: bool,
    /// The current version's SHA-256, for files.
    sha256: Option<String>,
}

impl Meta {
    fn of(node: &Node) -> Self {
        match node {
            Node::Folder(folder) => Self::folder(folder),
            Node::File(file) => Self::file(file),
        }
    }

    fn folder(folder: &FolderView) -> Self {
        Meta {
            len: 0,
            modified: folder.updated_at,
            dir: true,
            sha256: None,
        }
    }

    fn file(file: &FileView) -> Self {
        Meta {
            len: file.size,
            modified: file.updated_at,
            dir: false,
            sha256: Some(file.sha256.clone()),
        }
    }
}

impl DavMetaData for Meta {
    fn len(&self) -> u64 {
        self.len
    }

    fn modified(&self) -> FsResult<SystemTime> {
        Ok(UNIX_EPOCH + Duration::from_millis(self.modified))
    }

    fn is_dir(&self) -> bool {
        self.dir
    }

    fn etag(&self) -> Option<String> {
        match &self.sha256 {
            Some(sha256) => Some(sha256[..32].to_string()),
            None => Some(format!("{:x}", self.modified)),
        }
    }
}

struct Entry {
    name: String,
    meta: Meta,
}

impl DavDirEntry for Entry {
    fn name(&self) -> Vec<u8> {
        self.name.as_bytes().to_vec()
    }

    fn metadata(&'_ self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        let meta: Box<dyn DavMetaData> = Box::new(self.meta.clone());
        std::future::ready(Ok(meta)).boxed()
    }
}

impl LibraryFs {
    pub(crate) fn new(client: Arc<Mutex<Client>>, account: String) -> Self {
        Self {
            client,
            account,
            deleting: false,
            moving: false,
        }
    }

    pub(super) fn for_method(mut self, method: &str) -> Self {
        self.deleting = matches!(method, "DELETE" | "MOVE");
        self.moving = method == "MOVE";
        self
    }

    async fn client(&self) -> FsResult<tokio::sync::MutexGuard<'_, Client>> {
        let client = self.client.lock().await;
        if client.session_state().unlocked_account() != Some(self.account.as_str()) {
            return Err(FsError::Forbidden);
        }
        Ok(client)
    }

    async fn lookup(&self, path: &DavPath) -> FsResult<Option<Node>> {
        let path = path_of(path)?;
        let mut client = self.client().await?;
        client.files_lookup(&path).map_err(fs_error)
    }

    /// The live folder at `path`.
    async fn folder(&self, path: &str) -> FsResult<String> {
        folder_at(&mut *self.client().await?, path)
    }
}

/// The live folder at `path`.
fn folder_at(client: &mut Client, path: &str) -> FsResult<String> {
    match client.files_lookup(path).map_err(fs_error)? {
        Some(Node::Folder(folder)) => Ok(folder.id),
        Some(Node::File(_)) => Err(FsError::Forbidden),
        None => Err(FsError::NotFound),
    }
}

impl DavFileSystem for LibraryFs {
    fn open<'a>(
        &'a self,
        path: &'a DavPath,
        options: OpenOptions,
    ) -> FsFuture<'a, Box<dyn DavFile>> {
        async move {
            let text = path_of(path)?;
            if !options.write {
                return match self.lookup(path).await? {
                    Some(Node::File(file)) => {
                        let reader: Box<dyn DavFile> = Box::new(Reader {
                            fs: self.clone(),
                            meta: Meta::file(&file),
                            id: file.id,
                            version: file.current,
                            pos: 0,
                            cache: None,
                        });
                        Ok(reader)
                    }
                    Some(Node::Folder(_)) => Err(FsError::Forbidden),
                    None => Err(FsError::NotFound),
                };
            }
            if options.append {
                return Err(FsError::NotImplemented);
            }
            let (parent, name) = split(&text);
            if noise(&name) {
                let discard: Box<dyn DavFile> = Box::new(Discard);
                return Ok(discard);
            }
            validate_name(&name).map_err(|_| FsError::Forbidden)?;
            let folder = self.folder(&parent).await?;
            let (dir, max_size) = {
                let mut client = self.client().await?;
                match client.files_lookup(&text).map_err(fs_error)? {
                    Some(Node::Folder(_)) => return Err(FsError::Forbidden),
                    Some(Node::File(_)) if options.create_new => return Err(FsError::Exists),
                    None if !options.create => return Err(FsError::NotFound),
                    _ => {
                        let max_size = client.config().files_max_bytes;
                        if options.size.is_some_and(|size| size > max_size) {
                            return Err(FsError::TooLarge);
                        }
                        (client.files_mount_dir().map_err(fs_error)?, max_size)
                    }
                }
            };
            let upload = dir.join(hex::encode(&dmsg_client::crypto::random_id()[..16]));
            let mut open = std::fs::OpenOptions::new();
            open.write(true).create_new(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut open, 0o600);
            let file = open.open(&upload).map_err(|_| FsError::GeneralFailure)?;
            let writer: Box<dyn DavFile> = Box::new(Writer {
                fs: self.clone(),
                folder,
                name,
                path: upload,
                file: Some(file),
                written: 0,
                expected: options.size,
                max_size,
                committed: None,
            });
            Ok(writer)
        }
        .boxed()
    }

    fn read_dir<'a>(
        &'a self,
        path: &'a DavPath,
        _meta: ReadDirMeta,
    ) -> FsFuture<'a, FsStream<Box<dyn DavDirEntry>>> {
        async move {
            let path = path_of(path)?;
            let folder = self.folder(&path).await?;
            if self.deleting {
                // DELETE and MOVE only enumerate a tree to remove it. The
                // catalog trashes that entire tree in remove_dir instead.
                let stream: FsStream<Box<dyn DavDirEntry>> =
                    Box::pin(futures_util::stream::empty());
                return Ok(stream);
            }
            let listing = {
                let mut client = self.client().await?;
                client.files_list(&folder, false).map_err(fs_error)?
            };
            let entries: Vec<FsResult<Box<dyn DavDirEntry>>> = listing
                .folders
                .iter()
                .map(|f| Entry {
                    name: f.name.clone(),
                    meta: Meta::folder(f),
                })
                .chain(listing.files.iter().map(|f| Entry {
                    name: f.name.clone(),
                    meta: Meta::file(f),
                }))
                .map(|e| Ok(Box::new(e) as Box<dyn DavDirEntry>))
                .collect();
            let stream: FsStream<Box<dyn DavDirEntry>> =
                Box::pin(futures_util::stream::iter(entries));
            Ok(stream)
        }
        .boxed()
    }

    fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>> {
        async move {
            let (_, name) = split(&path_of(path)?);
            if noise(&name) {
                return Err(FsError::NotFound);
            }
            match self.lookup(path).await? {
                Some(node) => {
                    let meta: Box<dyn DavMetaData> = Box::new(Meta::of(&node));
                    Ok(meta)
                }
                None => Err(FsError::NotFound),
            }
        }
        .boxed()
    }

    fn create_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        async move {
            let (parent, name) = split(&path_of(path)?);
            if noise(&name) {
                return Err(FsError::Forbidden);
            }
            let folder = self.folder(&parent).await?;
            let mut client = self.client().await?;
            if client
                .files_lookup(&format!("{parent}/{name}"))
                .map_err(fs_error)?
                .is_some()
            {
                return Err(FsError::Exists);
            }
            client.files_mkdir(&folder, &name).map_err(fs_error)?;
            Ok(())
        }
        .boxed()
    }

    fn remove_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        async move {
            let path = path_of(path)?;
            let folder = self.folder(&path).await?;
            if folder == ROOT {
                return Err(FsError::Forbidden);
            }
            let mut client = self.client().await?;
            client
                .files_trash(std::slice::from_ref(&folder))
                .map_err(fs_error)
        }
        .boxed()
    }

    fn remove_file<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        async move {
            if self.moving {
                // The handler deletes an existing destination before rename.
                // Defer this so rename can preserve its identity and history.
                return Ok(());
            }
            match self.lookup(path).await? {
                Some(Node::File(file)) => {
                    let mut client = self.client().await?;
                    client
                        .files_trash(std::slice::from_ref(&file.id))
                        .map_err(fs_error)
                }
                Some(Node::Folder(_)) => Err(FsError::Forbidden),
                None => Err(FsError::NotFound),
            }
        }
        .boxed()
    }

    fn rename<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        async move {
            let (from, to) = (path_of(from)?, path_of(to)?);
            let (parent, name) = split(&to);
            if noise(&name) {
                return Err(FsError::Forbidden);
            }
            validate_name(&name).map_err(|_| FsError::Forbidden)?;
            let mut client = self.client().await?;
            let source = client
                .files_lookup(&from)
                .map_err(fs_error)?
                .ok_or(FsError::NotFound)?;
            let target = client.files_lookup(&to).map_err(fs_error)?;
            if let (Node::File(source), Some(Node::File(target))) = (&source, &target) {
                client
                    .files_replace(&source.id, &target.id)
                    .map_err(fs_error)?;
                return Ok(());
            }
            if target.is_some() {
                return Err(FsError::Exists);
            }
            let id = match source {
                Node::Folder(folder) if folder.id == ROOT => return Err(FsError::Forbidden),
                Node::Folder(folder) => folder.id,
                Node::File(file) => file.id,
            };
            let folder = folder_at(&mut client, &parent)?;
            client
                .files_relocate(&id, &folder, &name)
                .map_err(fs_error)?;
            Ok(())
        }
        .boxed()
    }

    fn copy<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        async move {
            let (parent, name) = split(&path_of(to)?);
            if noise(&name) {
                return Err(FsError::Forbidden);
            }
            let id = match self.lookup(from).await? {
                Some(Node::File(file)) => file.id,
                Some(Node::Folder(_)) => return Err(FsError::Forbidden),
                None => return Err(FsError::NotFound),
            };
            let folder = self.folder(&parent).await?;
            let mut client = self.client().await?;
            client.files_copy(&id, &folder, &name).map_err(fs_error)?;
            Ok(())
        }
        .boxed()
    }

    fn get_quota(&'_ self) -> FsFuture<'_, (u64, Option<u64>)> {
        async move {
            let mut client = self.client().await?;
            Ok((client.files_mount_used().map_err(fs_error)?, None))
        }
        .boxed()
    }
}

/// A file open for reading: one decrypted chunk at a time, all of the
/// version current when it was opened.
struct Reader {
    fs: LibraryFs,
    id: String,
    version: String,
    meta: Meta,
    pos: u64,
    /// The chunk the position is in.
    cache: Option<(u64, Zeroizing<Vec<u8>>)>,
}

impl std::fmt::Debug for Reader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reader")
            .field("id", &self.id)
            .field("pos", &self.pos)
            .finish_non_exhaustive()
    }
}

impl DavFile for Reader {
    fn metadata(&'_ mut self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        let meta: Box<dyn DavMetaData> = Box::new(self.meta.clone());
        std::future::ready(Ok(meta)).boxed()
    }

    fn write_buf(&'_ mut self, _buf: Box<dyn bytes::Buf + Send>) -> FsFuture<'_, ()> {
        std::future::ready(Err(FsError::Forbidden)).boxed()
    }

    fn write_bytes(&'_ mut self, _buf: bytes::Bytes) -> FsFuture<'_, ()> {
        std::future::ready(Err(FsError::Forbidden)).boxed()
    }

    fn read_bytes(&'_ mut self, count: usize) -> FsFuture<'_, bytes::Bytes> {
        async move {
            if self.pos >= self.meta.len || count == 0 {
                return Ok(bytes::Bytes::new());
            }
            let mut client = self.fs.client().await?;
            let index = self.pos / CHUNK as u64;
            if self.cache.as_ref().is_none_or(|(i, _)| *i != index) {
                let chunk = {
                    client
                        .files_chunk(&self.id, &self.version, index)
                        .map_err(fs_error)?
                };
                self.cache = Some((index, chunk));
            }
            let (_, chunk) = self.cache.as_ref().expect("cached chunk");
            let offset = (self.pos - index * CHUNK as u64) as usize;
            if offset >= chunk.len() {
                return Err(FsError::GeneralFailure);
            }
            let take = count.min(chunk.len() - offset);
            let bytes = bytes::Bytes::copy_from_slice(&chunk[offset..offset + take]);
            self.pos += take as u64;
            Ok(bytes)
        }
        .boxed()
    }

    fn seek(&'_ mut self, pos: SeekFrom) -> FsFuture<'_, u64> {
        let next = match pos {
            SeekFrom::Start(offset) => Some(offset),
            SeekFrom::End(delta) => self.meta.len.checked_add_signed(delta),
            SeekFrom::Current(delta) => self.pos.checked_add_signed(delta),
        };
        match next {
            Some(next) => {
                self.pos = next;
                std::future::ready(Ok(next)).boxed()
            }
            None => std::future::ready(Err(FsError::GeneralFailure)).boxed(),
        }
    }

    fn flush(&'_ mut self) -> FsFuture<'_, ()> {
        std::future::ready(Ok(())).boxed()
    }
}

/// A file open for writing: the bytes wait in the mount's upload
/// directory and become a version at `flush`, which a PUT calls once the
/// body has arrived. Nothing arrives in the library before that.
struct Writer {
    fs: LibraryFs,
    folder: String,
    name: String,
    path: PathBuf,
    file: Option<std::fs::File>,
    written: u64,
    expected: Option<u64>,
    max_size: u64,
    committed: Option<Meta>,
}

impl std::fmt::Debug for Writer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Writer")
            .field("name", &self.name)
            .field("written", &self.written)
            .finish_non_exhaustive()
    }
}

impl Writer {
    fn write(&mut self, bytes: &[u8]) -> FsResult<()> {
        let size = self.written.saturating_add(bytes.len() as u64);
        if size > self.max_size || self.expected.is_some_and(|expected| size > expected) {
            return Err(FsError::TooLarge);
        }
        let file = self.file.as_mut().ok_or(FsError::GeneralFailure)?;
        file.write_all(bytes).map_err(|e| match e.kind() {
            std::io::ErrorKind::StorageFull => FsError::InsufficientStorage,
            _ => FsError::GeneralFailure,
        })?;
        self.written += bytes.len() as u64;
        Ok(())
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.file = None;
        let _ = std::fs::remove_file(&self.path);
    }
}

impl DavFile for Writer {
    fn metadata(&'_ mut self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        let meta: Box<dyn DavMetaData> = Box::new(self.committed.clone().unwrap_or(Meta {
            len: self.written,
            modified: now_ms(),
            dir: false,
            sha256: None,
        }));
        std::future::ready(Ok(meta)).boxed()
    }

    fn write_buf(&'_ mut self, mut buf: Box<dyn bytes::Buf + Send>) -> FsFuture<'_, ()> {
        let result = (|| {
            while buf.has_remaining() {
                let chunk = buf.chunk();
                let len = chunk.len();
                self.write(chunk)?;
                buf.advance(len);
            }
            Ok(())
        })();
        std::future::ready(result).boxed()
    }

    fn write_bytes(&'_ mut self, buf: bytes::Bytes) -> FsFuture<'_, ()> {
        std::future::ready(self.write(&buf)).boxed()
    }

    fn read_bytes(&'_ mut self, _count: usize) -> FsFuture<'_, bytes::Bytes> {
        std::future::ready(Err(FsError::Forbidden)).boxed()
    }

    fn seek(&'_ mut self, _pos: SeekFrom) -> FsFuture<'_, u64> {
        std::future::ready(Err(FsError::NotImplemented)).boxed()
    }

    fn flush(&'_ mut self) -> FsFuture<'_, ()> {
        async move {
            if self.committed.is_some() {
                return Ok(());
            }
            if self
                .expected
                .is_some_and(|expected| self.written != expected)
            {
                return Err(FsError::GeneralFailure);
            }
            if let Some(file) = self.file.take() {
                file.sync_all().map_err(|_| FsError::GeneralFailure)?;
            }
            let imported = {
                let mut client = self.fs.client().await?;
                client.files_write_mounted(&self.folder, &self.name, &self.path)
            };
            let _ = std::fs::remove_file(&self.path);
            self.committed = Some(Meta::file(&imported.map_err(fs_error)?.file));
            Ok(())
        }
        .boxed()
    }
}

/// Where a file manager's noise goes.
#[derive(Debug)]
struct Discard;

impl DavFile for Discard {
    fn metadata(&'_ mut self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        let meta: Box<dyn DavMetaData> = Box::new(Meta {
            len: 0,
            modified: now_ms(),
            dir: false,
            sha256: None,
        });
        std::future::ready(Ok(meta)).boxed()
    }

    fn write_buf(&'_ mut self, _buf: Box<dyn bytes::Buf + Send>) -> FsFuture<'_, ()> {
        std::future::ready(Ok(())).boxed()
    }

    fn write_bytes(&'_ mut self, _buf: bytes::Bytes) -> FsFuture<'_, ()> {
        std::future::ready(Ok(())).boxed()
    }

    fn read_bytes(&'_ mut self, _count: usize) -> FsFuture<'_, bytes::Bytes> {
        std::future::ready(Err(FsError::NotFound)).boxed()
    }

    fn seek(&'_ mut self, _pos: SeekFrom) -> FsFuture<'_, u64> {
        std::future::ready(Ok(0)).boxed()
    }

    fn flush(&'_ mut self) -> FsFuture<'_, ()> {
        std::future::ready(Ok(())).boxed()
    }
}
