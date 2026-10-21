//! The file library as a disk: a loopback WebDAV share the daemon serves at
//! `/dav/<token>/<localized-name>` and mounts through the operating system
//! (`files/mount`, `files/unmount`, `files/mount_status`). Reading decrypts
//! chunk by chunk, with ranges at 1 MiB boundaries; a write becomes a new
//! version once the whole file has arrived; folders map to folders, a
//! delete goes to the trash, a copy re-encrypts, and an editor's safe save
//! (write a temporary file, move it over the original) becomes a new
//! version of the original. The token is random for each mount and lives
//! only in memory; on macOS the mount also carries a random Basic password,
//! which NetFS keeps and the person never sees. A lock, an account switch
//! and the daemon's exit unmount.
//!
//! Finder's and Explorer's noise (`.DS_Store`, `._*`, `Thumbs.db`,
//! `desktop.ini` and the like) is accepted and discarded.

use crate::daemon::Daemon;
use crate::home::Preferences;
use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::Engine;
use dav_server::{DavConfig, DavHandler, fakels::FakeLs};
use dmsg_client::{library::catalog::validate_name, now_ms};
use dmsg_types::{Error, Result};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

#[path = "mount_fs.rs"]
mod filesystem;
use filesystem::LibraryFs;

/// The methods this module serves.
pub(crate) const METHODS: [&str; 5] = [
    "files/mount",
    "files/unmount",
    "files/mount_status",
    "files/preferences",
    "files/preferences_reload",
];

/// The Basic user, where a password is used.
pub(crate) const USER: &str = "dmsg";
/// How long a system mount or unmount command may take.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);

/// The events a method's success announces.
pub(crate) fn events_of(method: &str) -> &'static [&'static str] {
    match method {
        "files/mount_status" | "files/preferences" => &[],
        "files/preferences_reload" => &["files/changed", "files/mount_changed"],
        _ => &["files/mount_changed"],
    }
}

/// A mount in progress: what the route needs and what the status shows.
pub(crate) struct Mount {
    token: String,
    /// The caller's localized volume name, fixed until the next mount.
    name: String,
    /// The Basic password, on macOS; the token alone elsewhere.
    password: Option<String>,
    /// The account whose library this is; another account's unlock revokes it.
    account: String,
    url: String,
    /// Where the system mounted it, when known.
    point: Option<String>,
    since: u64,
    ready: bool,
    handler: DavHandler,
}

/// What this system can do with a mount.
fn platform() -> (&'static str, bool, &'static str) {
    if cfg!(target_os = "macos") {
        (
            "macos",
            true,
            "macOS keeps a plaintext cache of opened files in its system cache directory while the disk is mounted.",
        )
    } else if cfg!(windows) {
        (
            "windows",
            true,
            "Windows reads and writes files up to 50,000,000 bytes on WebDAV disks (WebClient FileSizeLimitInBytes); use export for larger files.",
        )
    } else if cfg!(target_os = "linux") {
        (
            "linux",
            true,
            "Mounted through GVFS (gio); file managers built on it see the disk, other programs through its FUSE path.",
        )
    } else {
        ("other", false, "No mount on this system.")
    }
}

impl Mount {
    fn status(&self, enabled: bool) -> Value {
        let (os, supported, note) = platform();
        json!({
            "mounted": self.ready,
            "mounting": !self.ready,
            "enabled": enabled,
            "os": os,
            "supported": supported,
            "note": note,
            "url": self.url,
            "name": self.name,
            "mount_point": self.point,
            "since": self.since,
        })
    }
}

/// A localized name supplied by the desktop or CLI. Direct RPC callers
/// fall back to the daemon's language.
fn volume_name(params: &Value) -> Result<String> {
    let name = match params.get("name") {
        None | Some(Value::Null) => crate::locale::tr!("dMsg Files", "dMsg 文件"),
        Some(Value::String(name)) => name.clone(),
        Some(_) => return Err(Error::InvalidInput("name".into())),
    };
    validate_name(&name)?;
    if name.len() > 255 {
        return Err(Error::InvalidInput("name".into()));
    }
    Ok(name)
}

/// DAV needs a decoded prefix while the system mount needs a URL with
/// spaces and non-ASCII characters encoded as one path segment.
fn share_location(port: u16, token: &str, name: &str) -> (String, String) {
    let prefix = format!("/dav/{token}/{name}");
    let mut url = reqwest::Url::parse(&format!("http://127.0.0.1:{port}")).expect("loopback URL");
    url.path_segments_mut()
        .expect("HTTP path")
        .clear()
        .extend(["dav", token, name]);
    (prefix, url.into())
}

fn unmounted_status(enabled: bool) -> Value {
    let (os, supported, note) = platform();
    json!({
        "mounted": false,
        "mounting": false,
        "enabled": enabled,
        "os": os,
        "supported": supported,
        "note": note,
    })
}

/// Run a system command, feeding `stdin`, within [`COMMAND_TIMEOUT`];
/// returns its standard output.
pub(crate) async fn run(
    command: &mut tokio::process::Command,
    stdin: Option<&str>,
) -> Result<String> {
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
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        // `dmsg --json` reports errors on stdout; system tools may use either.
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        return Err(Error::Unavailable(format!(
            "mount: {}: {detail}",
            output.status
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Mount `url` through NetFS; returns the mount point.
#[cfg(target_os = "macos")]
async fn system_mount(url: &str, _token: &str, password: Option<&str>) -> Result<Option<String>> {
    let point = crate::mount_netfs::helper_mount(url, password.unwrap_or_default()).await?;
    Ok(Some(point))
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
async fn system_mount(url: &str, _token: &str, _password: Option<&str>) -> Result<Option<String>> {
    run(
        tokio::process::Command::new("gio").args(["mount", &gvfs_uri(url)]),
        None,
    )
    .await?;
    let parsed = reqwest::Url::parse(url).expect("mount URL");
    let port = parsed.port().expect("loopback port");
    let prefix = parsed.path().replace('/', "%2F");
    let point = std::env::var_os("XDG_RUNTIME_DIR").map(|runtime| {
        std::path::PathBuf::from(runtime).join("gvfs").join(format!(
            "dav:host=127.0.0.1,port={port},ssl=false,prefix={prefix}"
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
    pub(crate) async fn call_mount(&self, method: &str, params: &Value) -> Result<Value> {
        let enabled = self.files_mount.load(Ordering::SeqCst);
        match method {
            "files/mount_status" => Ok(self.mount_status()),
            "files/preferences" => {
                let days = self.client().lock().await.config().files_trash_days;
                Ok(self.preferences_view(days))
            }
            "files/preferences_reload" => self.read_preferences().await,
            "files/unmount" => {
                self.unmount_library().await?;
                Ok(unmounted_status(enabled))
            }
            "files/mount" => {
                let name = volume_name(params)?;
                let _turn = self.turn("mount").await;
                let enabled = self.files_mount.load(Ordering::SeqCst);
                if !enabled {
                    return Err(Error::Forbidden);
                }
                if !platform().1 {
                    return Err(Error::Unavailable(
                        "mount: not supported on this system".into(),
                    ));
                }
                let existing = self
                    .mount
                    .lock()
                    .expect("mount state")
                    .as_ref()
                    .map(|m| (m.account.clone(), m.status(enabled)));
                if let Some((account, status)) = existing {
                    let session = self.client().lock().await.session_state();
                    if session.unlocked_account() == Some(account.as_str()) {
                        return Ok(status);
                    }
                    self.unmount_current().await?;
                }
                let account = {
                    let client = self.client().lock().await;
                    let session = client.session_state();
                    let account = session.unlocked_account().ok_or(Error::Locked)?;
                    client.files_mount_clean();
                    account.to_string()
                };
                let token = hex::encode(dmsg_client::crypto::random_id());
                let password = cfg!(target_os = "macos")
                    .then(|| hex::encode(dmsg_client::crypto::random_id()));
                let (prefix, url) = share_location(self.port(), &token, &name);
                let handler = DavHandler::builder()
                    .strip_prefix(&prefix)
                    .filesystem(Box::new(LibraryFs::new(
                        self.client_handle(),
                        account.clone(),
                    )))
                    .locksystem(FakeLs::new())
                    .build_handler();
                *self.mount.lock().expect("mount state") = Some(Mount {
                    token: token.clone(),
                    name,
                    password: password.clone(),
                    account: account.clone(),
                    url: url.clone(),
                    point: None,
                    since: now_ms(),
                    ready: false,
                    handler,
                });
                let mounted = system_mount(&url, &token, password.as_deref()).await;
                // The account or preference can change while NetFS is waiting.
                let valid = self.files_mount.load(Ordering::SeqCst)
                    && self
                        .client()
                        .lock()
                        .await
                        .session_state()
                        .unlocked_account()
                        == Some(account.as_str());
                let point = match mounted {
                    Ok(point) => point,
                    Err(error) => {
                        *self.mount.lock().expect("mount state") = None;
                        return Err(error);
                    }
                };
                {
                    let mut state = self.mount.lock().expect("mount state");
                    let mount = state.as_mut().expect("mount attempt owns the state");
                    mount.point = point.clone();
                    mount.ready = true;
                }
                if !valid {
                    // Keep failed unmounts available for the background retry.
                    self.unmount_current().await?;
                    return Err(Error::Locked);
                }
                crate::logs::write(
                    "files/mount",
                    &[("point", &point.as_deref().unwrap_or_default())],
                );
                Ok(self.mount_status())
            }
            _ => Err(Error::NotFound),
        }
    }

    /// Take `preferences.json` again, which only the desktop's main process
    /// writes: whether the mount is allowed beside `config.toml`, and how
    /// long trashed files stay. A mount no longer allowed is undone.
    pub(crate) async fn read_preferences(&self) -> Result<Value> {
        let preferences = Preferences::read(&self.settings().preferences)?;
        let days = preferences
            .files_trash_days
            .unwrap_or(self.settings().config.files_trash_days);
        self.client().lock().await.files_set_trash_days(days)?;
        let allowed = self.settings().files_mount || preferences.files_mount;
        self.files_mount.store(allowed, Ordering::SeqCst);
        if !allowed {
            self.unmount_library().await?;
        }
        Ok(self.preferences_view(days))
    }

    /// The file settings as the desktop shows them; `files_mount_config`
    /// says `config.toml` allows the mount, which no setting takes back.
    fn preferences_view(&self, trash_days: u64) -> Value {
        json!({
            "files_mount": self.files_mount.load(Ordering::SeqCst),
            "files_mount_config": self.settings().files_mount,
            "files_trash_days": trash_days,
            "supported": platform().1,
        })
    }

    /// The mount's status.
    pub(crate) fn mount_status(&self) -> Value {
        let enabled = self.files_mount.load(Ordering::SeqCst);
        self.mount
            .lock()
            .expect("mount state")
            .as_ref()
            .map_or_else(|| unmounted_status(enabled), |m| m.status(enabled))
    }

    /// Unmount if mounted: the system first, while the share still answers,
    /// then the token. Announces `files/mount_changed` when something was
    /// mounted.
    pub(crate) async fn unmount_library(&self) -> Result<()> {
        let _turn = self.turn("mount").await;
        self.unmount_current().await
    }

    /// The caller owns the mount turn. A failed system unmount remains
    /// tracked so a later attempt can retry it.
    async fn unmount_current(&self) -> Result<()> {
        let mounted = self
            .mount
            .lock()
            .expect("mount state")
            .as_ref()
            .map(|m| (m.url.clone(), m.point.clone()));
        let Some((url, point)) = mounted else {
            return Ok(());
        };
        if let Err(error) = system_unmount(&url, point.as_deref()).await {
            crate::logs::write("files/unmount", &[("error", &error.to_string())]);
            return Err(error);
        }
        crate::logs::write("files/unmount", &[]);
        *self.mount.lock().expect("mount state") = None;
        self.emit("files/mount_changed", self.mount_status());
        Ok(())
    }

    /// Unmount when the library it serves is no longer the unlocked one;
    /// the background pass.
    pub(crate) async fn check_mount(&self) {
        let _turn = self.turn("mount").await;
        let account = self
            .mount
            .lock()
            .expect("mount state")
            .as_ref()
            .map(|m| m.account.clone());
        let Some(account) = account else { return };
        let session = self.client().lock().await.session_state();
        if session.unlocked_account() != Some(account.as_str())
            || !self.files_mount.load(Ordering::SeqCst)
        {
            let _ = self.unmount_current().await;
        }
    }

    /// The handler of a token, with what the route checks first.
    fn mount_of(&self, token: &str, name: &str) -> Option<(DavHandler, Option<String>, String)> {
        self.mount
            .lock()
            .expect("mount state")
            .as_ref()
            .filter(|m| m.token == token && m.name == name)
            .map(|m| (m.handler.clone(), m.password.clone(), m.account.clone()))
    }
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
        Some((user, pass)) => user == USER && crate::daemon::constant_time_eq(pass, password),
        None => false,
    }
}

fn status(code: StatusCode) -> Response {
    code.into_response()
}

/// Every WebDAV request goes through the DAV handler, including its
/// conditional headers and lock checks.
pub(crate) async fn serve(
    State(daemon): State<Arc<Daemon>>,
    Path(params): Path<HashMap<String, String>>,
    request: Request,
) -> Response {
    let token = params.get("token").cloned().unwrap_or_default();
    let name = params.get("volume").cloned().unwrap_or_default();
    let Some((handler, password, account)) = daemon.mount_of(&token, &name) else {
        return status(StatusCode::NOT_FOUND);
    };
    if let Some(password) = &password
        && !authorized(request.headers(), password)
    {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"dMsg\"")],
        )
            .into_response();
    }
    {
        let client = daemon.client().lock().await;
        if client.session_state().unlocked_account() != Some(account.as_str()) {
            return status(StatusCode::FORBIDDEN);
        }
    }
    if !daemon.files_mount.load(Ordering::SeqCst) {
        return status(StatusCode::FORBIDDEN);
    }
    let method = request.method().as_str().to_string();
    let changed = matches!(
        method.as_str(),
        "PUT" | "MKCOL" | "DELETE" | "MOVE" | "COPY"
    );
    if changed || method == "GET" {
        daemon.note_activity();
    }
    let fs = LibraryFs::new(daemon.client_handle(), account).for_method(&method);
    let response = handler
        .handle_with(DavConfig::new().filesystem(Box::new(fs)), request)
        .await
        .map(Body::new);
    if changed && response.status().is_success() {
        daemon.emit("files/changed", json!({"method": method}));
    }
    response
}

#[cfg(test)]
#[path = "mount_tests.rs"]
mod tests;
