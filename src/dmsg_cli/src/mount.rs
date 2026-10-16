//! The file library as a disk: a loopback WebDAV share the daemon serves at
//! `/dav/<token>/dMsg` and mounts through the operating system
//! (`files/mount`, `files/unmount`, `files/mount_status`). Reading decrypts
//! chunk by chunk, with ranges at 1 MiB boundaries; a write becomes a new
//! version once the whole file has arrived; folders map to folders, a
//! delete goes to the trash, a copy re-encrypts, and an editor's safe save
//! (write a temporary file, move it over the original) becomes a new
//! version of the original. The token is random for each mount and lives
//! only in memory; on macOS the mount also carries a random Basic password,
//! which the system keeps and the person never sees. A lock, an account
//! switch and the daemon's exit unmount.
//!
//! Finder's and Explorer's noise (`.DS_Store`, `._*`, `Thumbs.db`,
//! `desktop.ini` and the like) is accepted and discarded.

use crate::daemon::Daemon;
use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use base64::Engine;
use dav_server::{
    davpath::DavPath,
    fakels::FakeLs,
    fs::{
        DavDirEntry, DavFile, DavFileSystem, DavMetaData, FsError, FsFuture, FsResult, FsStream,
        OpenOptions, ReadDirMeta,
    },
    DavHandler,
};
use dmsg_client::{
    files::CHUNK,
    library::{catalog::validate_name, catalog::ROOT, mount::Node},
    now_ms, Client,
};
use dmsg_types::{Error, Result};
use futures_util::FutureExt;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{SeekFrom, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use zeroize::Zeroizing;

/// The methods this module serves.
pub(crate) const METHODS: [&str; 3] = ["files/mount", "files/unmount", "files/mount_status"];

/// The volume's name: the last segment of the share's URL, which the
/// systems take as the mount's name.
pub(crate) const VOLUME: &str = "dMsg";
/// The Basic user, where a password is used.
const USER: &str = "dmsg";
/// How long a system mount or unmount command may take.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);

/// The events a method's success announces.
pub(crate) fn events_of(method: &str) -> &'static [&'static str] {
    match method {
        "files/mount_status" => &[],
        _ => &["files/mount_changed"],
    }
}

/// A mount in progress: what the route needs and what the status shows.
pub(crate) struct Mount {
    token: String,
    /// The Basic password, on macOS; the token alone elsewhere.
    password: Option<String>,
    /// The account whose library this is; another account's unlock revokes it.
    account: String,
    url: String,
    /// Where the system mounted it, when known.
    point: Option<String>,
    since: u64,
    handler: DavHandler,
}

/// What this system can do with a mount.
fn platform() -> (&'static str, bool, &'static str) {
    if cfg!(target_os = "macos") {
        ("macos", true, "macOS keeps a plaintext cache of opened files in its system cache directory while the disk is mounted.")
    } else if cfg!(windows) {
        ("windows", true, "Windows reads and writes files up to 50,000,000 bytes on WebDAV disks (WebClient FileSizeLimitInBytes); use export for larger files.")
    } else if cfg!(target_os = "linux") {
        ("linux", true, "Mounted through GVFS (gio); file managers built on it see the disk, other programs through its FUSE path.")
    } else {
        ("other", false, "No mount on this system.")
    }
}

impl Mount {
    fn status(&self, enabled: bool) -> Value {
        let (os, supported, note) = platform();
        json!({
            "mounted": true,
            "enabled": enabled,
            "os": os,
            "supported": supported,
            "note": note,
            "url": self.url,
            "mount_point": self.point,
            "since": self.since,
        })
    }
}

fn unmounted_status(enabled: bool) -> Value {
    let (os, supported, note) = platform();
    json!({
        "mounted": false,
        "enabled": enabled,
        "os": os,
        "supported": supported,
        "note": note,
    })
}

/// Run a system command, feeding `stdin`, within [`COMMAND_TIMEOUT`];
/// returns its standard output.
async fn run(command: &mut tokio::process::Command, stdin: Option<&str>) -> Result<String> {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    command
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|e| Error::Unavailable(format!("mount: {e}")))?;
    if let Some(text) = stdin {
        let mut pipe = child.stdin.take().expect("piped stdin");
        pipe.write_all(text.as_bytes())
            .await
            .map_err(|e| Error::Unavailable(format!("mount: {e}")))?;
        drop(pipe);
    }
    let output = tokio::time::timeout(COMMAND_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| Error::Unavailable("mount: the system command timed out".into()))?
        .map_err(|e| Error::Unavailable(format!("mount: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(Error::Unavailable(format!("mount: {stderr}")));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Mount `url` through the system; returns the mount point when known.
#[cfg(target_os = "macos")]
async fn system_mount(url: &str, token: &str, password: Option<&str>) -> Result<Option<String>> {
    let script = format!(
        "mount volume {url:?} as user name {USER:?} with password {:?}",
        password.unwrap_or_default()
    );
    run(
        &mut tokio::process::Command::new("osascript"),
        Some(&script),
    )
    .await?;
    let table = run(&mut tokio::process::Command::new("mount"), None).await?;
    let marker = format!("/dav/{token}/");
    Ok(table
        .lines()
        .find(|line| line.contains(&marker))
        .and_then(|line| line.split(" on ").nth(1))
        .and_then(|rest| rest.rsplit_once(" (").map(|(point, _)| point.to_string())))
}

#[cfg(target_os = "macos")]
async fn system_unmount(_url: &str, point: Option<&str>) -> Result<()> {
    let Some(point) = point else { return Ok(()) };
    if run(
        tokio::process::Command::new("diskutil").args(["unmount", point]),
        None,
    )
    .await
    .is_ok()
    {
        return Ok(());
    }
    run(
        tokio::process::Command::new("diskutil").args(["unmount", "force", point]),
        None,
    )
    .await
    .map(|_| ())
}

#[cfg(windows)]
async fn system_mount(url: &str, _token: &str, _password: Option<&str>) -> Result<Option<String>> {
    let letter = ('D'..='Z')
        .rev()
        .find(|l| !std::path::Path::new(&format!("{l}:\\")).exists())
        .ok_or_else(|| Error::Unavailable("mount: no free drive letter".into()))?;
    run(
        tokio::process::Command::new("net").args([
            "use",
            &format!("{letter}:"),
            url,
            "/persistent:no",
        ]),
        None,
    )
    .await?;
    Ok(Some(format!("{letter}:\\")))
}

#[cfg(windows)]
async fn system_unmount(_url: &str, point: Option<&str>) -> Result<()> {
    let Some(point) = point else { return Ok(()) };
    let drive = point.trim_end_matches('\\');
    run(
        tokio::process::Command::new("net").args(["use", drive, "/delete", "/y"]),
        None,
    )
    .await
    .map(|_| ())
}

#[cfg(target_os = "linux")]
fn gvfs_uri(url: &str) -> String {
    url.replacen("http://", "dav://", 1)
}

#[cfg(target_os = "linux")]
async fn system_mount(url: &str, token: &str, _password: Option<&str>) -> Result<Option<String>> {
    run(
        tokio::process::Command::new("gio").args(["mount", &gvfs_uri(url)]),
        None,
    )
    .await?;
    let port = url
        .rsplit_once(':')
        .and_then(|(_, rest)| rest.split('/').next())
        .unwrap_or_default();
    let point = std::env::var_os("XDG_RUNTIME_DIR").map(|runtime| {
        PathBuf::from(runtime).join("gvfs").join(format!(
            "dav:host=127.0.0.1,port={port},ssl=false,prefix=%2Fdav%2F{token}%2F{VOLUME}"
        ))
    });
    Ok(point
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned()))
}

#[cfg(target_os = "linux")]
async fn system_unmount(url: &str, _point: Option<&str>) -> Result<()> {
    run(
        tokio::process::Command::new("gio").args(["mount", "-u", &gvfs_uri(url)]),
        None,
    )
    .await
    .map(|_| ())
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
async fn system_mount(_url: &str, _token: &str, _password: Option<&str>) -> Result<Option<String>> {
    Err(Error::Unavailable(
        "mount: not supported on this system".into(),
    ))
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
async fn system_unmount(_url: &str, _point: Option<&str>) -> Result<()> {
    Ok(())
}

impl Daemon {
    /// Run one of [`METHODS`]. The client is not held while the system
    /// mounts, since the system reads the share to do so.
    pub(crate) async fn call_mount(&self, method: &str, _params: &Value) -> Result<Value> {
        let enabled = self.settings().files_mount;
        match method {
            "files/mount_status" => Ok(self.mount_status()),
            "files/unmount" => {
                self.unmount_library().await;
                Ok(unmounted_status(enabled))
            }
            "files/mount" => {
                if !enabled {
                    return Err(Error::Forbidden);
                }
                if !platform().1 {
                    return Err(Error::Unavailable(
                        "mount: not supported on this system".into(),
                    ));
                }
                if let Some(status) = self
                    .mount
                    .lock()
                    .expect("mount state")
                    .as_ref()
                    .map(|m| m.status(enabled))
                {
                    return Ok(status);
                }
                let account = {
                    let client = self.client().lock().await;
                    let status = client.status()?;
                    if status.state != "unlocked" {
                        return Err(Error::Locked);
                    }
                    client.files_mount_clean();
                    client.files_mount_dir()?;
                    status.account.ok_or(Error::AuthRequired)?
                };
                let token = hex::encode(dmsg_client::crypto::random_id());
                let password = cfg!(target_os = "macos")
                    .then(|| hex::encode(dmsg_client::crypto::random_id()));
                let prefix = format!("/dav/{token}/{VOLUME}");
                let url = format!("http://127.0.0.1:{}{prefix}", self.port());
                let handler = DavHandler::builder()
                    .strip_prefix(&prefix)
                    .filesystem(Box::new(LibraryFs::new(self.client_handle())))
                    .locksystem(FakeLs::new())
                    .build_handler();
                *self.mount.lock().expect("mount state") = Some(Mount {
                    token: token.clone(),
                    password: password.clone(),
                    account,
                    url: url.clone(),
                    point: None,
                    since: now_ms(),
                    handler,
                });
                let mounted = system_mount(&url, &token, password.as_deref()).await;
                let mut state = self.mount.lock().expect("mount state");
                match mounted {
                    Ok(point) => {
                        let mount = state.as_mut().filter(|m| m.token == token);
                        let Some(mount) = mount else {
                            return Err(Error::Locked);
                        };
                        mount.point = point;
                        crate::logs::write(
                            "files/mount",
                            &[("point", &mount.point.as_deref().unwrap_or_default())],
                        );
                        Ok(mount.status(enabled))
                    }
                    Err(error) => {
                        if state.as_ref().is_some_and(|m| m.token == token) {
                            *state = None;
                        }
                        Err(error)
                    }
                }
            }
            _ => Err(Error::NotFound),
        }
    }

    /// The mount's status.
    pub(crate) fn mount_status(&self) -> Value {
        let enabled = self.settings().files_mount;
        self.mount
            .lock()
            .expect("mount state")
            .as_ref()
            .map_or_else(|| unmounted_status(enabled), |m| m.status(enabled))
    }

    /// Unmount if mounted: the system first, while the share still answers,
    /// then the token. Announces `files/mount_changed` when something was
    /// mounted.
    pub(crate) async fn unmount_library(&self) {
        let mounted = self
            .mount
            .lock()
            .expect("mount state")
            .as_ref()
            .map(|m| (m.url.clone(), m.point.clone()));
        let Some((url, point)) = mounted else { return };
        if let Err(error) = system_unmount(&url, point.as_deref()).await {
            crate::logs::write("files/unmount", &[("error", &error.to_string())]);
        } else {
            crate::logs::write("files/unmount", &[]);
        }
        *self.mount.lock().expect("mount state") = None;
        self.emit("files/mount_changed", self.mount_status());
    }

    /// Unmount when the library it serves is no longer the unlocked one;
    /// the background pass.
    pub(crate) async fn check_mount(&self) {
        let account = self
            .mount
            .lock()
            .expect("mount state")
            .as_ref()
            .map(|m| m.account.clone());
        let Some(account) = account else { return };
        let current = {
            let client = self.client().lock().await;
            client
                .status()
                .ok()
                .filter(|s| s.state == "unlocked")
                .and_then(|s| s.account)
        };
        if current.as_deref() != Some(&account) {
            self.unmount_library().await;
        }
    }

    /// The handler of a token, with what the route checks first.
    fn mount_of(&self, token: &str) -> Option<(DavHandler, Option<String>, String, String)> {
        self.mount
            .lock()
            .expect("mount state")
            .as_ref()
            .filter(|m| m.token == token)
            .map(|m| {
                (
                    m.handler.clone(),
                    m.password.clone(),
                    m.account.clone(),
                    format!("/dav/{}/{VOLUME}", m.token),
                )
            })
    }
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// `Authorization: Basic` with [`USER`] and `password`.
fn authorized(headers: &HeaderMap, password: &str) -> bool {
    let Some(encoded) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
    else {
        return false;
    };
    let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(encoded.trim()) else {
        return false;
    };
    let Ok(decoded) = String::from_utf8(decoded) else {
        return false;
    };
    match decoded.split_once(':') {
        Some((user, pass)) => user == USER && constant_time_eq(pass, password),
        None => false,
    }
}

fn status(code: StatusCode) -> Response {
    code.into_response()
}

/// The decoded library path the request's `Destination` names, if it is
/// on this share.
fn destination(headers: &HeaderMap, prefix: &str) -> Option<String> {
    let value = headers.get("destination")?.to_str().ok()?;
    let rest = value.find(prefix).map(|i| &value[i + prefix.len()..])?;
    let path = DavPath::new(if rest.is_empty() { "/" } else { rest }).ok()?;
    String::from_utf8(path.as_bytes().to_vec()).ok()
}

/// `/dav/{token}/dMsg` and `/dav/{token}/dMsg/{*rest}`: every WebDAV
/// method. A folder's DELETE and a move onto an existing file are answered
/// here, as one trash and one new version; the rest goes to the handler.
pub(crate) async fn serve(
    State(daemon): State<Arc<Daemon>>,
    Path(params): Path<HashMap<String, String>>,
    request: Request,
) -> Response {
    let token = params.get("token").cloned().unwrap_or_default();
    let Some((handler, password, account, prefix)) = daemon.mount_of(&token) else {
        return status(StatusCode::NOT_FOUND);
    };
    if let Some(password) = &password {
        if !authorized(request.headers(), password) {
            return (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Basic realm=\"dMsg\"")],
            )
                .into_response();
        }
    }
    let serving = {
        let client = daemon.client().lock().await;
        client
            .status()
            .is_ok_and(|s| s.state == "unlocked" && s.account.as_deref() == Some(&account))
    };
    if !serving {
        return status(StatusCode::FORBIDDEN);
    }
    let rest = params.get("rest").cloned().unwrap_or_default();
    let intercepted = {
        let mut client = daemon.client().lock().await;
        intercept(
            &mut client,
            request.method().as_str(),
            &rest,
            request.headers(),
            &prefix,
        )
    };
    match intercepted {
        Some(code) => status(code),
        None => handler.handle(request).await.map(Body::new),
    }
}

/// What the share answers itself, before the handler: a folder's DELETE as
/// one trash (the handler would delete file by file, each into the trash on
/// its own), and a MOVE of a file onto an existing file as a new version of
/// that file (an editor's safe save; the handler would trash the original
/// and leave a new file without its history).
pub(crate) fn intercept(
    client: &mut Client,
    method: &str,
    path: &str,
    headers: &HeaderMap,
    prefix: &str,
) -> Option<StatusCode> {
    match method {
        "DELETE" => match client.files_lookup(path) {
            Ok(Some(Node::Folder(folder))) if folder.id == ROOT => Some(StatusCode::FORBIDDEN),
            Ok(Some(Node::Folder(folder))) => {
                Some(match client.files_trash(std::slice::from_ref(&folder.id)) {
                    Ok(()) => StatusCode::NO_CONTENT,
                    Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
                })
            }
            _ => None,
        },
        "MOVE" => {
            let overwrite = headers
                .get("overwrite")
                .and_then(|v| v.to_str().ok())
                .is_none_or(|v| !v.trim().eq_ignore_ascii_case("F"));
            let to = destination(headers, prefix).filter(|_| overwrite)?;
            let from = client.files_lookup(path);
            let into = client.files_lookup(&to);
            let (Ok(Some(Node::File(from))), Ok(Some(Node::File(into)))) = (from, into) else {
                return None;
            };
            if from.id == into.id {
                return Some(StatusCode::FORBIDDEN);
            }
            Some(match client.files_replace(&from.id, &into.id) {
                Ok(_) => StatusCode::NO_CONTENT,
                Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
            })
        }
        _ => None,
    }
}

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
fn split(path: &str) -> (String, String) {
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
            Node::Folder(folder) => Meta {
                len: 0,
                modified: folder.updated_at,
                dir: true,
                sha256: None,
            },
            Node::File(file) => Meta {
                len: file.size,
                modified: file.updated_at,
                dir: false,
                sha256: Some(file.sha256.clone()),
            },
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
    pub(crate) fn new(client: Arc<Mutex<Client>>) -> Self {
        Self { client }
    }

    async fn lookup(&self, path: &DavPath) -> FsResult<Option<Node>> {
        let path = path_of(path)?;
        let mut client = self.client.lock().await;
        client.files_lookup(&path).map_err(fs_error)
    }

    /// The live folder at `path`.
    async fn folder(&self, path: &str) -> FsResult<String> {
        let mut client = self.client.lock().await;
        match client.files_lookup(path).map_err(fs_error)? {
            Some(Node::Folder(folder)) => Ok(folder.id),
            Some(Node::File(_)) => Err(FsError::Forbidden),
            None => Err(FsError::NotFound),
        }
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
                        let meta = Meta::of(&Node::File(file.clone()));
                        let reader: Box<dyn DavFile> = Box::new(Reader {
                            client: self.client.clone(),
                            id: file.id,
                            meta,
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
            let dir = {
                let mut client = self.client.lock().await;
                match client.files_lookup(&text).map_err(fs_error)? {
                    Some(Node::Folder(_)) => return Err(FsError::Forbidden),
                    Some(Node::File(_)) if options.create_new => return Err(FsError::Exists),
                    None if !options.create => return Err(FsError::NotFound),
                    _ => client.files_mount_dir().map_err(fs_error)?,
                }
            };
            let upload = dir.join(hex::encode(&dmsg_client::crypto::random_id()[..16]));
            let mut open = std::fs::OpenOptions::new();
            open.write(true).create_new(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut open, 0o600);
            let file = open.open(&upload).map_err(|_| FsError::GeneralFailure)?;
            let writer: Box<dyn DavFile> = Box::new(Writer {
                client: self.client.clone(),
                folder,
                name,
                path: upload,
                file: Some(file),
                written: 0,
                done: false,
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
            let listing = {
                let mut client = self.client.lock().await;
                client.files_list(&folder, false).map_err(fs_error)?
            };
            let entries: Vec<FsResult<Box<dyn DavDirEntry>>> = listing
                .folders
                .iter()
                .map(|f| Entry {
                    name: f.name.clone(),
                    meta: Meta::of(&Node::Folder(f.clone())),
                })
                .chain(listing.files.iter().map(|f| Entry {
                    name: f.name.clone(),
                    meta: Meta::of(&Node::File(f.clone())),
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
            let mut client = self.client.lock().await;
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
            let mut client = self.client.lock().await;
            client
                .files_trash(std::slice::from_ref(&folder))
                .map_err(fs_error)
        }
        .boxed()
    }

    fn remove_file<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        async move {
            match self.lookup(path).await? {
                Some(Node::File(file)) => {
                    let mut client = self.client.lock().await;
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
            let (_, name) = split(&path_of(to)?);
            let (parent, _) = split(&path_of(to)?);
            if noise(&name) {
                return Err(FsError::Forbidden);
            }
            validate_name(&name).map_err(|_| FsError::Forbidden)?;
            let id = match self.lookup(from).await? {
                Some(Node::Folder(folder)) if folder.id == ROOT => return Err(FsError::Forbidden),
                Some(Node::Folder(folder)) => folder.id,
                Some(Node::File(file)) => file.id,
                None => return Err(FsError::NotFound),
            };
            let folder = self.folder(&parent).await?;
            let mut client = self.client.lock().await;
            let current_parent = match client.files_lookup(&path_of(from)?).map_err(fs_error)? {
                Some(Node::Folder(f)) => f.parent,
                Some(Node::File(f)) => f.folder,
                None => return Err(FsError::NotFound),
            };
            if current_parent != folder {
                client
                    .files_move(std::slice::from_ref(&id), &folder)
                    .map_err(fs_error)?;
            }
            let current_name = match client.files_lookup(&path_of(to)?).map_err(fs_error)? {
                Some(Node::Folder(f)) if f.id == id => Some(f.name),
                Some(Node::File(f)) if f.id == id => Some(f.name),
                Some(_) => return Err(FsError::Exists),
                None => None,
            };
            if current_name.as_deref() != Some(&name) {
                client.files_rename(&id, &name).map_err(fs_error)?;
            }
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
            let mut client = self.client.lock().await;
            client.files_copy(&id, &folder, &name).map_err(fs_error)?;
            Ok(())
        }
        .boxed()
    }

    fn get_quota(&'_ self) -> FsFuture<'_, (u64, Option<u64>)> {
        async move {
            let mut client = self.client.lock().await;
            let usage = client.files_usage().map_err(fs_error)?;
            Ok((usage.blob_bytes, None))
        }
        .boxed()
    }
}

/// A file open for reading: one decrypted chunk at a time.
struct Reader {
    client: Arc<Mutex<Client>>,
    id: String,
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
            let index = self.pos / CHUNK as u64;
            if self.cache.as_ref().is_none_or(|(i, _)| *i != index) {
                let chunk = {
                    let mut client = self.client.lock().await;
                    client.files_chunk(&self.id, index).map_err(fs_error)?
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
    client: Arc<Mutex<Client>>,
    folder: String,
    name: String,
    path: PathBuf,
    file: Option<std::fs::File>,
    written: u64,
    done: bool,
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
        let file = self.file.as_mut().ok_or(FsError::GeneralFailure)?;
        file.write_all(bytes).map_err(|e| {
            if e.raw_os_error() == Some(28) {
                FsError::InsufficientStorage
            } else {
                FsError::GeneralFailure
            }
        })?;
        self.written += bytes.len() as u64;
        Ok(())
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        if !self.done {
            self.file = None;
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

impl DavFile for Writer {
    fn metadata(&'_ mut self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        let meta: Box<dyn DavMetaData> = Box::new(Meta {
            len: self.written,
            modified: now_ms(),
            dir: false,
            sha256: None,
        });
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
            if self.done {
                return Ok(());
            }
            if let Some(file) = self.file.take() {
                file.sync_all().map_err(|_| FsError::GeneralFailure)?;
            }
            let imported = {
                let mut client = self.client.lock().await;
                client.files_write_mounted(&self.folder, &self.name, &self.path)
            };
            let _ = std::fs::remove_file(&self.path);
            self.done = true;
            imported.map(|_| ()).map_err(fs_error)
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

#[cfg(test)]
#[path = "mount_tests.rs"]
mod tests;
