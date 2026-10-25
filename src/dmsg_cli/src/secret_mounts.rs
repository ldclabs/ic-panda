//! OS control plane and mount-scoped native IPC. No owner token enters a filesystem.
use crate::{
    daemon::Daemon,
    params::{number, optional, source, text},
};
use dmsg_client::secrets::mounts::{self, Config, Mode, MountInput};
use dmsg_types::{Error, Result, ensure, ensure_valid};
#[cfg(target_os = "macos")]
use serde::Deserialize;
use serde_json::{Value, json};
#[cfg(target_os = "macos")]
use std::path::PathBuf;
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Mutex, OnceLock, Weak},
    time::Duration,
};

#[derive(Default)]
pub(crate) struct Manager {
    host: OnceLock<Weak<Daemon>>,
    active: Mutex<BTreeMap<String, Active>>,
    acknowledgements: Mutex<BTreeMap<(String, String), u64>>,
    faults: Mutex<BTreeMap<String, String>>,
    changed: tokio::sync::Notify,
}
struct Active {
    config: Config,
    #[cfg(target_os = "macos")]
    source: Option<PathBuf>,
    backend: Backend,
    failed: bool,
    starting: bool,
    started_at: std::time::Instant,
}
enum Backend {
    #[cfg(target_os = "macos")]
    Native(tokio::task::JoinHandle<()>),
    #[cfg(target_os = "linux")]
    Fuse(fuse3::raw::MountHandle),
}
impl Manager {
    pub(crate) fn bind(&self, host: Weak<Daemon>) {
        let _ = self.host.set(host);
    }
}

#[cfg(unix)]
fn uid() -> u32 {
    unsafe { libc::geteuid() }
}

#[cfg(not(unix))]
fn uid() -> u32 {
    0
}

fn platform_supported() -> bool {
    #[cfg(target_os = "macos")]
    {
        static SUPPORTED: OnceLock<bool> = OnceLock::new();
        *SUPPORTED.get_or_init(|| {
            std::process::Command::new("/usr/bin/sw_vers")
                .arg("-productVersion")
                .output()
                .is_ok_and(|o| {
                    o.status.success()
                        && std::str::from_utf8(&o.stdout)
                            .ok()
                            .and_then(|s| s.trim().split('.').next()?.parse::<u32>().ok())
                            .is_some_and(|major| major >= 27)
                })
        })
    }
    #[cfg(target_os = "linux")]
    true
}

fn mode(window: Option<u64>) -> Result<Mode> {
    ensure(platform_supported(), Error::UnsupportedProtocol)?;
    if cfg!(target_os = "macos") {
        let window_ms = window.unwrap_or(30000);
        ensure_valid((1..=60000).contains(&window_ms), "window_ms")?;
        Ok(Mode::MacosFileWindow {
            owner_uid: uid(),
            window_ms,
        })
    } else if cfg!(target_os = "linux") {
        ensure_valid(window.is_none(), "Linux uses handle grants")?;
        Ok(Mode::LinuxHandle { owner_uid: uid() })
    } else {
        Err(Error::UnsupportedProtocol)
    }
}

#[cfg(unix)]
fn prepare_paths(mut input: MountInput) -> Result<MountInput> {
    use std::os::unix::fs::MetadataExt;
    ensure_valid(
        Path::new(&input.project_directory).is_absolute()
            && Path::new(&input.mount_point).is_absolute(),
        "absolute project paths",
    )?;
    let project = std::fs::canonicalize(&input.project_directory)
        .map_err(|_| Error::InvalidInput("project directory".into()))?;
    ensure(
        project.is_dir(),
        Error::InvalidInput("project directory".into()),
    )?;
    let mount = Path::new(&input.mount_point);
    let name = mount
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or(Error::InvalidInput("mount point".into()))?;
    let parent = std::fs::canonicalize(
        mount
            .parent()
            .ok_or(Error::InvalidInput("mount parent".into()))?,
    )
    .map_err(|_| Error::InvalidInput("mount parent".into()))?;
    ensure_valid(
        parent.starts_with(&project) && parent.join(name) != project,
        "mount must be within project",
    )?;
    let meta =
        std::fs::metadata(&parent).map_err(|_| Error::InvalidInput("mount parent".into()))?;
    ensure(meta.uid() == uid(), Error::Forbidden)?;
    let mount = parent.join(name);
    match std::fs::symlink_metadata(&mount) {
        Ok(m) => {
            ensure(
                m.is_dir() && !m.file_type().is_symlink() && m.uid() == uid(),
                Error::Forbidden,
            )?;
            ensure(
                std::fs::read_dir(&mount)
                    .map_err(|_| Error::Forbidden)?
                    .next()
                    .is_none(),
                Error::VersionConflict,
            )?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(Error::Forbidden),
    }
    input.project_directory = project
        .to_str()
        .ok_or(Error::InvalidInput("UTF-8 project path".into()))?
        .into();
    input.mount_point = mount
        .to_str()
        .ok_or(Error::InvalidInput("UTF-8 mount path".into()))?
        .into();
    input.parent_identity = format!("{}:{}", meta.dev(), meta.ino());
    prepare_aliases(&mut input, false)?;
    Ok(input)
}

#[cfg(unix)]
fn prepare_aliases(input: &mut MountInput, create: bool) -> Result<()> {
    use cap_fs_ext::OsMetadataExt;
    use cap_std::{ambient_authority, fs::Dir};
    let root = Dir::open_ambient_dir(&input.project_directory, ambient_authority())
        .map_err(|_| Error::Forbidden)?;
    let metadata = root.dir_metadata().map_err(|_| Error::Forbidden)?;
    ensure(metadata.uid() == uid(), Error::Forbidden)?;
    let identity = format!("{}:{}", metadata.dev(), metadata.ino());
    let mut seen = std::collections::BTreeSet::new();
    ensure_valid(input.aliases.len() <= 64, "project aliases")?;
    for alias in &mut input.aliases {
        ensure_valid(
            mounts::relative_path(&alias.name)
                && !alias.name.contains('/')
                && seen.insert(alias.name.clone())
                && input.entries.iter().any(|e| e.relative_path == alias.entry),
            "project alias",
        )?;
        ensure(
            Path::new(&input.project_directory).join(&alias.name) != Path::new(&input.mount_point),
            Error::VersionConflict,
        )?;
        if create {
            ensure(alias.parent_identity == identity, Error::VersionConflict)?;
        } else {
            alias.parent_identity = identity.clone();
        }
        let target = Path::new(&input.mount_point).join(&alias.entry);
        match root.symlink_metadata(&alias.name) {
            Ok(meta) => ensure(
                meta.file_type().is_symlink()
                    && root
                        .read_link_contents(&alias.name)
                        .is_ok_and(|v| v == target),
                Error::VersionConflict,
            )?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if create {
                    root.symlink_contents(&target, &alias.name)
                        .map_err(|_| Error::VersionConflict)?;
                }
            }
            Err(_) => return Err(Error::Forbidden),
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn prepare_paths(_input: MountInput) -> Result<MountInput> {
    Err(Error::UnsupportedProtocol)
}

impl Daemon {
    pub(crate) async fn call_secret_mounts(&self, method: &str, p: &Value) -> Result<Value> {
        match method {
            "secrets/mounts/list" => Ok(
                json!({"mounts":self.client().lock().await.secret_mount_configs()?,"platform":std::env::consts::OS,"supported":platform_supported(),"daemon_ready":daemon_ready(),"backend_status":"checked_when_started"}),
            ),
            "secrets/mounts/prepare" | "secrets/mounts/save_request" => {
                let input: MountInput = serde_json::from_value(p["input"].clone())
                    .map_err(|_| Error::InvalidInput("mount input".into()))?;
                let input = tokio::task::spawn_blocking(move || prepare_paths(input))
                    .await
                    .map_err(|_| Error::Unavailable("mount prepare".into()))??;
                let mode = mode(optional(p, "window_ms", number)?)?;
                let mut c = self.client().lock().await;
                if method.ends_with("prepare") {
                    let preview = c.secret_mount_prepare(&input, &mode)?;
                    Ok(
                        json!({"input":input,"preview":preview,"backend":if cfg!(target_os="macos"){"apple_fskit"}else{"linux_fuse"},"backend_status":"verified_when_started","exclude_from_git_index_backup":true,"no_plaintext_fallback":true}),
                    )
                } else {
                    c.secret_mount_request(
                        source(p)?,
                        optional(p, "id", text)?,
                        optional(p, "base_version", number)?,
                        input,
                        mode,
                    )
                }
            }
            "secrets/mounts/remove" => {
                let id = text(p, "id")?;
                self.stop_secret_mount(&id).await?;
                self.client().lock().await.secret_mount_remove(&id)
            }
            "secrets/mounts/start" => self.start_secret_mount(&text(p, "id")?).await,
            "secrets/mounts/stop" => self.stop_secret_mount(&text(p, "id")?).await,
            "secrets/mounts/revoke_reads" => {
                let id = text(p, "id")?;
                let result = self
                    .client()
                    .lock()
                    .await
                    .secret_mount_revoke(&id, p["entry_id"].as_str())?;
                self.confirm_secret_revocation(&id, &result).await?;
                Ok(result)
            }
            "secrets/mounts/status" => {
                let id = text(p, "id")?;
                let active = self
                    .secret_mounts
                    .active
                    .lock()
                    .expect("mounts")
                    .get(&id)
                    .map(|a| (a.config.input.mount_point.clone(), a.failed, a.starting));
                let mut c = self.client().lock().await;
                let path = c
                    .secret_mount_configs()?
                    .into_iter()
                    .find(|c| c.id == id)
                    .ok_or(Error::NotFound)?
                    .input
                    .mount_point;
                let orphan = active.is_none() && is_secret_mount(&path);
                match c.secret_mount_status(&id) {
                    Ok(core) => Ok(
                        json!({"state":if active.as_ref().is_some_and(|(_,failed,_)|*failed){"unmount_unconfirmed"}else if active.as_ref().is_some_and(|(_,_,starting)|*starting){"starting"}else if active.is_some(){"running"}else if orphan{"orphaned_mount"}else{"stopped"},"error":self.secret_mounts.faults.lock().expect("mount faults").get(&id).cloned(),"mount_point":path,"core":core}),
                    ),
                    Err(Error::NotFound) => Ok(
                        json!({"state":if active.as_ref().is_some_and(|(_,failed,_)|*failed){"unmount_unconfirmed"}else if orphan{"orphaned_mount"}else{"stopped"},"error":self.secret_mounts.faults.lock().expect("mount faults").get(&id).cloned(),"mount_point":path}),
                    ),
                    Err(e) => Err(e),
                }
            }
            _ => Err(Error::NotFound),
        }
    }

    async fn start_secret_mount(&self, id: &str) -> Result<Value> {
        let _turn = self.turn("secret-mount-control").await;
        ensure(
            !self
                .secret_mounts
                .active
                .lock()
                .expect("mounts")
                .contains_key(id),
            Error::VersionConflict,
        )?;
        let config = self
            .client()
            .lock()
            .await
            .secret_mount_configs()?
            .into_iter()
            .find(|c| c.id == id)
            .ok_or(Error::NotFound)?;
        let checked = {
            let input = config.input.clone();
            tokio::task::spawn_blocking(move || prepare_paths(input))
                .await
                .map_err(|_| Error::Unavailable("mount prepare".into()))??
        };
        ensure(config.input == checked, Error::VersionConflict)?;
        ensure(
            config.mode
                == mode(match &config.mode {
                    Mode::MacosFileWindow { window_ms, .. } => Some(*window_ms),
                    _ => None,
                })?,
            Error::Forbidden,
        )?;
        let host = self
            .secret_mounts
            .host
            .get()
            .cloned()
            .ok_or(Error::Unavailable("mount host".into()))?;
        self.secret_mounts
            .faults
            .lock()
            .expect("mount faults")
            .remove(id);
        self.secret_mounts
            .acknowledgements
            .lock()
            .expect("mount acknowledgements")
            .retain(|(mount, _), _| mount != id);
        let metadata = self.client().lock().await.secret_mount_start(id)?;
        let prepared=async {
            #[cfg(unix)]
            prepare_aliases(&mut config.input.clone(),true)?;
            #[cfg(unix)]
            {use std::os::unix::fs::DirBuilderExt;let path=Path::new(&config.input.mount_point);if !path.exists(){std::fs::DirBuilder::new().mode(0o700).create(path).map_err(|_|Error::VersionConflict)?;}}
            #[cfg(target_os="macos")]
            {
                let source=native_directory()?;
                let server=match native_server(host.clone(),id.into(),&source){Ok(server)=>server,Err(error)=>{let _=std::fs::remove_dir(&source);return Err(error);}};
                // Track before invoking the OS: a dropped RPC still has a discoverable, stoppable operation.
                self.secret_mounts.active.lock().expect("mounts").insert(id.into(),Active{config:config.clone(),source:Some(source.clone()),backend:Backend::Native(server),failed:false,starting:true,started_at:std::time::Instant::now()});
                let status=tokio::time::timeout(Duration::from_secs(30),tokio::process::Command::new("/sbin/mount").kill_on_drop(true).args(["-F","-k","-t","dmsgsecrets","-o","rdonly,nosuid,nodev,noexec,owners"]).arg(&source).arg(&config.input.mount_point).output()).await;
                if !matches!(&status,Ok(Ok(o)) if o.status.success()) {
                    self.stop_secret_mount(id).await?;
                    let failure=match &status {Ok(Ok(o))=>fskit_failure(&String::from_utf8_lossy(&o.stderr)),_=>"FSKIT_UNAVAILABLE: the system did not mount the volume"};
                    return Err(Error::Unavailable(failure.into()));
                }
                let live=self.client().lock().await.secret_mount_entries(id);
                if let Err(error)=live {self.stop_secret_mount(id).await?;return Err(error);}
                let mut active=self.secret_mounts.active.lock().expect("mounts");
                let active=active.get_mut(id).ok_or(Error::Expired)?;active.starting=false;
            }
            #[cfg(target_os="linux")]
            {
                let entries=self.client().lock().await.secret_mount_entries(id)?;
                let backend=linux::start(host,id.into(),config.clone(),entries).await?;
                self.client().lock().await.secret_mount_entries(id)?;
                self.secret_mounts.active.lock().expect("mounts").insert(id.into(),Active{config:config.clone(),backend:Backend::Fuse(backend),failed:false,starting:false,started_at:std::time::Instant::now()});
            }
            #[cfg(not(unix))]
            return Err(Error::UnsupportedProtocol);
            Ok(json!({"state":"running","metadata":metadata,"mount_point":config.input.mount_point}))
        }.await;
        if let Err(error) = &prepared {
            let _ = self.client().lock().await.secret_mount_stop(id);
            // Kept for the status, so the person sees what to do until the next start.
            if let Error::Unavailable(message) = error
                && let Some(code) = message
                    .split(':')
                    .next()
                    .filter(|c| c.starts_with("FSKIT_"))
            {
                self.secret_mounts
                    .faults
                    .lock()
                    .expect("mount faults")
                    .insert(id.into(), code.into());
            }
        }
        prepared
    }

    pub(crate) async fn stop_secret_mount(&self, id: &str) -> Result<Value> {
        let _ = self.client().lock().await.secret_mount_stop(id);
        let active = self.secret_mounts.active.lock().expect("mounts").remove(id);
        let Some(active) = active else {
            let config = self
                .client()
                .lock()
                .await
                .secret_mount_configs()?
                .into_iter()
                .find(|c| c.id == id)
                .ok_or(Error::NotFound)?;
            unmount_orphan(&config.input.mount_point).await?;
            return Ok(json!({"stopped":id,"os_unmounted":true}));
        };
        match active.backend {
            #[cfg(target_os = "macos")]
            Backend::Native(server) => {
                let unmounted = !is_our_native_mount(&active.config.input.mount_point)
                    || tokio::time::timeout(
                        Duration::from_secs(10),
                        tokio::process::Command::new("/sbin/umount")
                            .kill_on_drop(true)
                            .arg("-f")
                            .arg(&active.config.input.mount_point)
                            .output(),
                    )
                    .await
                    .is_ok_and(|r| r.is_ok_and(|o| o.status.success()));
                server.abort();
                if let Some(source) = active.source {
                    let _ = std::fs::remove_file(source.join(IPC_SOCKET));
                    let _ = std::fs::remove_dir(source);
                }
                if !unmounted {
                    self.secret_mounts.active.lock().expect("mounts").insert(
                        id.into(),
                        Active {
                            config: active.config,
                            source: None,
                            backend: Backend::Native(server),
                            failed: true,
                            starting: false,
                            started_at: active.started_at,
                        },
                    );
                    return Err(Error::Unavailable(
                        "FSKIT_UNMOUNT_UNCONFIRMED: content service stopped".into(),
                    ));
                }
            }
            #[cfg(target_os = "linux")]
            Backend::Fuse(session) => {
                tokio::time::timeout(Duration::from_secs(10), session.unmount())
                    .await
                    .map_err(|_| Error::Unavailable("FUSE_UNMOUNT_UNCONFIRMED".into()))?
                    .map_err(|_| Error::Unavailable("FUSE_UNMOUNT_UNCONFIRMED".into()))?;
                unmount_orphan(&active.config.input.mount_point).await?;
            }
        }
        Ok(json!({"stopped":id,"os_unmounted":true}))
    }

    async fn confirm_secret_revocation(&self, id: &str, result: &Value) -> Result<()> {
        if !cfg!(target_os = "macos") {
            return Ok(());
        }
        let revoked = result["revoked"].as_array().cloned().unwrap_or_default();
        if revoked.is_empty() {
            return Ok(());
        }
        // Each ended window, as (entry, revocation count) the module must reach.
        let expected: Vec<_> = revoked
            .iter()
            .filter(|r| r[0] == id)
            .filter_map(|r| Some((r[1].as_str()?.to_owned(), r[2].as_u64()?)))
            .collect();
        let waiting = async {
            loop {
                let notified = self.secret_mounts.changed.notified();
                if self
                    .secret_mounts
                    .faults
                    .lock()
                    .expect("mount faults")
                    .contains_key(id)
                {
                    return Err(Error::Unavailable("FSKIT_REVOKE_FAILED".into()));
                }
                if expected.iter().all(|(entry, count)| {
                    self.secret_mounts
                        .acknowledgements
                        .lock()
                        .expect("mount acknowledgements")
                        .get(&(id.into(), entry.clone()))
                        .is_some_and(|seen| seen >= count)
                }) {
                    return Ok(());
                }
                notified.await;
            }
        };
        if tokio::time::timeout(Duration::from_secs(3), waiting)
            .await
            .is_ok_and(|r| r.is_ok())
        {
            return Ok(());
        }
        // Failure stops content first, then attempts whole-volume unmount. The caller sees the fallback.
        self.stop_secret_mount(id).await?;
        Err(Error::Unavailable(
            "FSKIT_REVOKE_UNCONFIRMED: volume stopped".into(),
        ))
    }

    pub(crate) async fn check_secret_mounts(&self) {
        let ids: Vec<_> = self
            .secret_mounts
            .active
            .lock()
            .expect("mounts")
            .iter()
            .filter(|(_, a)| !a.failed)
            .map(|(id, a)| {
                (
                    id.clone(),
                    a.starting && a.started_at.elapsed() > Duration::from_secs(35),
                )
            })
            .collect();
        for (id, abandoned) in ids {
            let invalid = self.client().lock().await.secret_mount_invalid_sources(&id);
            if invalid.as_ref().is_ok_and(|entries| !entries.is_empty()) {
                self.secret_mounts
                    .faults
                    .lock()
                    .expect("mount faults")
                    .insert(id.clone(), "SOURCE_CHANGED_RESTART_REQUIRED".into());
            }
            if abandoned
                || invalid.is_err()
                || self
                    .secret_mounts
                    .faults
                    .lock()
                    .expect("mount faults")
                    .contains_key(&id)
            {
                let _ = self.stop_secret_mount(&id).await;
            }
        }
    }

    pub(crate) async fn stop_secret_mounts(&self) {
        let ids: Vec<_> = self
            .secret_mounts
            .active
            .lock()
            .expect("mounts")
            .keys()
            .cloned()
            .collect();
        for id in ids {
            let _ = self.stop_secret_mount(&id).await;
        }
    }
}

#[cfg(target_os = "macos")]
async fn approved_read(
    d: &Daemon,
    id: &str,
    entry: &str,
    generation: u64,
    owner: u32,
    handle: Option<u64>,
    range: (u64, u32),
) -> Result<(zeroize::Zeroizing<Vec<u8>>, Value)> {
    let (offset, length) = range;
    let ticket = d
        .client()
        .lock()
        .await
        .secret_mount_intent(id, entry, generation, owner, handle)?;
    read_ticket(d, id, entry, handle, &ticket, (offset, length)).await
}

#[cfg(unix)]
async fn read_ticket(
    d: &Daemon,
    id: &str,
    entry: &str,
    handle: Option<u64>,
    ticket: &mounts::Ticket,
    range: (u64, u32),
) -> Result<(zeroize::Zeroizing<Vec<u8>>, Value)> {
    let (offset, length) = range;
    let mut events = d.subscribe_events();
    if ticket.request_id.is_some() {
        d.emit("approval/changed", Value::Null);
    }
    let execute = async {
        if let Some(request) = &ticket.request_id {
            loop {
                let view = d.client().lock().await.approval(request)?;
                match view["state"].as_str() {
                    Some("executing") => break,
                    Some("awaiting_user") => {}
                    _ => return Err(Error::Forbidden),
                }
                let _ = events.recv().await;
            }
        }
        let bytes = mounts::read(&d.hold(), id, &ticket.callback_id, offset, length).await?;
        let status = d.client().lock().await.secret_mount_status(id)?;
        let grant = status["grants"]
            .as_array()
            .and_then(|gs| {
                gs.iter().find(|g| {
                    g["entry"] == entry && handle.is_none_or(|h| g["scope"]["handle"] == h)
                })
            })
            .cloned()
            .ok_or(Error::Expired)?;
        Ok((bytes, grant))
    };
    let result = tokio::time::timeout(Duration::from_secs(30), execute)
        .await
        .unwrap_or(Err(Error::Expired));
    let _ = d
        .client()
        .lock()
        .await
        .secret_mount_cancel_io(id, &ticket.callback_id);
    d.emit("approval/changed", Value::Null);
    result
}

#[cfg(target_os = "macos")]
#[derive(Deserialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
enum NativeRequest {
    Metadata,
    /// The module cleared an entry's kernel caches through this many ended windows.
    Acknowledge {
        entry: String,
        revoked: u64,
        #[serde(default)]
        failed: bool,
    },
    Read {
        entry: String,
        generation: u64,
        offset: u64,
        length: u32,
    },
}

/// The module's socket in its IPC directory; short, as `sun_path` holds 104 bytes.
#[cfg(target_os = "macos")]
const IPC_SOCKET: &str = "c";

/// The team's App Group container, where the App Sandbox lets the module
/// connect to a Unix socket, and nowhere else it can see.
#[cfg(target_os = "macos")]
fn ipc_root() -> Result<PathBuf> {
    unsafe extern "C" {
        fn dmsg_fskit_ipc_root(buffer: *mut u8, capacity: isize) -> isize;
    }
    let mut root = vec![0u8; 1024];
    let length = unsafe { dmsg_fskit_ipc_root(root.as_mut_ptr(), root.len() as isize) };
    ensure(
        length > 0,
        Error::Unavailable(
            "FSKIT_DAEMON_UNSIGNED: this daemon lacks the module team's signature or app group"
                .into(),
        ),
    )?;
    root.truncate(length as usize);
    Ok(PathBuf::from(
        String::from_utf8(root).map_err(|_| Error::IntegrityFailed)?,
    ))
}

/// Whether this daemon can serve the native module at all; unsigned builds cannot.
fn daemon_ready() -> bool {
    #[cfg(target_os = "macos")]
    return ipc_root().is_ok();
    #[cfg(not(target_os = "macos"))]
    true
}

/// A fixed cause for a failed `/sbin/mount`, from the tool's own message formats.
#[cfg(target_os = "macos")]
fn fskit_failure(stderr: &str) -> &'static str {
    if stderr.contains(" is disabled") {
        "FSKIT_EXTENSION_DISABLED: enable dMsg secret files in System Settings > General > Login Items & Extensions > File System Extensions"
    } else if stderr.contains(" not found") {
        "FSKIT_EXTENSION_MISSING: this dMsg does not include the secret-files extension; use `dmsg run --file`"
    } else if stderr.contains("Loading resource") {
        "FSKIT_EXTENSION_REFUSED: the extension and this daemon are not from the same signed dMsg"
    } else {
        "FSKIT_UNAVAILABLE: the system did not mount the volume"
    }
}

/// A fresh 0700 IPC directory for one mount.
#[cfg(target_os = "macos")]
fn native_directory() -> Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    let directory = ipc_root()?.join(&hex::encode(dmsg_client::crypto::random_id())[..16]);
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .map_err(|_| Error::Unavailable("mount IPC directory".into()))?;
    Ok(directory)
}

#[cfg(target_os = "macos")]
pub(crate) fn native_server(
    host: Weak<Daemon>,
    id: String,
    directory: &Path,
) -> Result<tokio::task::JoinHandle<()>> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::UnixListener;
    let socket = directory.join(IPC_SOCKET);
    let listener =
        UnixListener::bind(&socket).map_err(|_| Error::Unavailable("mount IPC".into()))?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
        .map_err(|_| Error::Forbidden)?;
    Ok(tokio::spawn(async move {
        let mut jobs = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                accepted=listener.accept()=>{
                    let Ok((stream,_))=accepted else{break};
                    if jobs.len()>=64{drop(stream);continue;}
                    jobs.spawn(native_connection(host.clone(),id.clone(),stream));
                },
                _=jobs.join_next(),if !jobs.is_empty()=>{},
            }
        }
    }))
}

#[cfg(target_os = "macos")]
async fn native_connection(host: Weak<Daemon>, id: String, mut stream: tokio::net::UnixStream) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let Some(d) = host.upgrade() else { return };
    let Ok(peer) = stream.peer_cred() else { return };
    if peer.uid() != uid() {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        let Some(pid) = peer.pid() else { return };
        let verified = tokio::task::spawn_blocking(move || {
            unsafe extern "C" {
                fn dmsg_fskit_peer_allowed(pid: i32, identifier: *const std::ffi::c_char) -> i32;
            }
            // The module PID authenticates IPC. It is never presented as the reader's PID.
            unsafe { dmsg_fskit_peer_allowed(pid, c"net.dmsg.desktop.SecretsFS".as_ptr()) == 1 }
        })
        .await
        .unwrap_or(false);
        if !verified {
            return;
        }
    }
    let request = tokio::time::timeout(Duration::from_secs(5), async {
        let size = stream.read_u32().await.map_err(|_| Error::Forbidden)?;
        ensure((1..=65536).contains(&size), Error::QuotaExceeded)?;
        let mut bytes = vec![0; size as usize];
        stream
            .read_exact(&mut bytes)
            .await
            .map_err(|_| Error::Forbidden)?;
        serde_json::from_slice::<NativeRequest>(&bytes)
            .map_err(|_| Error::InvalidInput("mount request".into()))
    })
    .await;
    let answer = match request {
        // The module polls this often. Every read rechecks its sources, and the
        // daemon's periodic check stops a volume whose sources changed.
        Ok(Ok(NativeRequest::Metadata)) => d
            .client()
            .lock()
            .await
            .secret_mount_status(&id)
            .map(|v| {
                let entries: Vec<_> = v["entries"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|e| json!({"id":e["id"],"path":e["relative_path"],"generation":e["generation"],"size":e["size"],"revoked":v["revocations"][e["id"].as_str().unwrap_or_default()].as_u64().unwrap_or(0)}))
                    .collect();
                json!({"owner_uid":uid(),"authorization":v["config"]["mode"],"entries":entries})
            }),
        Ok(Ok(NativeRequest::Acknowledge {
            entry,
            revoked,
            failed,
        })) => {
            let current = d.client().lock().await.secret_mount_status(&id);
            current.and_then(|status| {
                ensure(
                    status["entries"]
                        .as_array()
                        .is_some_and(|es| es.iter().any(|e| e["id"] == entry.as_str()))
                        && revoked <= status["revocations"][&entry].as_u64().unwrap_or(0),
                    Error::VersionConflict,
                )?;
                if failed {
                    d.secret_mounts
                        .faults
                        .lock()
                        .expect("mount faults")
                        .insert(id.clone(), "FSKIT_REVOKE_FAILED".into());
                } else {
                    d.secret_mounts
                        .acknowledgements
                        .lock()
                        .expect("mount acknowledgements")
                        .entry((id.clone(), entry))
                        .and_modify(|seen| *seen = (*seen).max(revoked))
                        .or_insert(revoked);
                }
                d.secret_mounts.changed.notify_waiters();
                Ok(Value::Null)
            })
        }
        Ok(Ok(NativeRequest::Read {
            entry,
            generation,
            offset,
            length,
        })) => {
            let mut byte = [0];
            let result = tokio::select! {
                result=approved_read(&d,&id,&entry,generation,peer.uid(),None,(offset,length))=>result.map(|(bytes,grant)|{
                    use base64::Engine;
                    json!({"base64url":base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&*bytes),"grant":grant})
                }),
                _=stream.read(&mut byte)=>Err(Error::Expired),
            };
            d.emit("approval/changed", Value::Null);
            result
        }
        _ => Err(Error::InvalidInput("mount request".into())),
    };
    let mut response = match answer {
        Ok(value) => json!({"ok":true,"result":value}),
        Err(error) => json!({"ok":false,"code":dmsg_client::error_code(&error)}),
    };
    use zeroize::{Zeroize, Zeroizing};
    let bytes = Zeroizing::new(serde_json::to_vec(&response).unwrap_or_default());
    if let Some(Value::String(value)) = response
        .get_mut("result")
        .and_then(|r| r.get_mut("base64url"))
    {
        value.zeroize();
    }
    let _ = tokio::time::timeout(Duration::from_secs(5), async {
        stream.write_u32(bytes.len() as u32).await?;
        stream.write_all(&bytes).await
    })
    .await;
}

#[cfg(target_os = "linux")]
#[path = "secret_mounts_linux.rs"]
mod linux;

#[cfg(target_os = "macos")]
fn is_our_native_mount(path: &str) -> bool {
    let Ok(path) = std::ffi::CString::new(path) else {
        return false;
    };
    let mut info = std::mem::MaybeUninit::<libc::statfs>::uninit();
    if unsafe { libc::statfs(path.as_ptr(), info.as_mut_ptr()) } != 0 {
        return false;
    }
    let info = unsafe { info.assume_init() };
    let kind: Vec<_> = info
        .f_fstypename
        .iter()
        .map(|b| *b as u8)
        .take_while(|b| *b != 0)
        .collect();
    kind == b"dmsgsecrets"
}

#[cfg(target_os = "macos")]
fn is_secret_mount(path: &str) -> bool {
    is_our_native_mount(path)
}

#[cfg(target_os = "linux")]
fn is_secret_mount(path: &str) -> bool {
    std::fs::read_to_string("/proc/self/mountinfo").is_ok_and(|info| {
        info.lines().any(|line| {
            let Some((left, right)) = line.split_once(" - ") else {
                return false;
            };
            let mut fields = right.split_whitespace();
            let kind = fields.next();
            let source = fields.next();
            let target = left
                .split_whitespace()
                .nth(4)
                .unwrap_or("")
                .replace("\\040", " ")
                .replace("\\011", "\t")
                .replace("\\012", "\n")
                .replace("\\134", "\\");
            matches!(kind, Some("fuse" | "fuse.dmsg-secrets"))
                && source == Some("dmsg-secrets")
                && target == path
        })
    })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn is_secret_mount(_path: &str) -> bool {
    false
}

async fn unmount_orphan(path: &str) -> Result<()> {
    if !is_secret_mount(path) {
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = tokio::process::Command::new("/sbin/umount");
        c.arg("-f");
        c
    };
    #[cfg(target_os = "linux")]
    let mut command = {
        let binary = [
            "/usr/bin/fusermount3",
            "/bin/fusermount3",
            "/usr/bin/fusermount",
        ]
        .into_iter()
        .find(|p| Path::new(p).is_file())
        .ok_or(Error::Unavailable("FUSE_UNMOUNT_UNCONFIRMED".into()))?;
        let mut c = tokio::process::Command::new(binary);
        c.arg("-u");
        c
    };
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            command.kill_on_drop(true).arg(path).output(),
        )
        .await;
        ensure(
            result.is_ok_and(|r| r.is_ok_and(|o| o.status.success())) && !is_secret_mount(path),
            Error::Unavailable("SECRET_UNMOUNT_UNCONFIRMED".into()),
        )
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    Err(Error::UnsupportedProtocol)
}

struct ContinuousClock;
impl mounts::Clock for ContinuousClock {
    fn milliseconds(&self) -> Result<u64> {
        #[cfg(target_os = "macos")]
        let clock = libc::CLOCK_MONOTONIC_RAW;
        #[cfg(target_os = "linux")]
        let clock = libc::CLOCK_BOOTTIME;
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        {
            let mut time = std::mem::MaybeUninit::<libc::timespec>::uninit();
            ensure(
                unsafe { libc::clock_gettime(clock, time.as_mut_ptr()) } == 0,
                Error::Unavailable("continuous clock".into()),
            )?;
            let time = unsafe { time.assume_init() };
            ensure(
                time.tv_sec >= 0 && (0..1_000_000_000).contains(&time.tv_nsec),
                Error::IntegrityFailed,
            )?;
            Ok(time.tv_sec as u64 * 1000 + time.tv_nsec as u64 / 1_000_000)
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        Err(Error::UnsupportedProtocol)
    }
}

pub(crate) fn configure_clock(client: &mut dmsg_client::Client) {
    client.set_secret_mount_clock(std::sync::Arc::new(ContinuousClock));
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn mount_failures_name_what_the_person_can_do() {
        // The formats `/sbin/mount` prints for FSKit modules.
        for (stderr, code) in [
            (
                "Module net.dmsg.desktop.SecretsFS is disabled!\nmount: Unable to invoke task\n",
                "FSKIT_EXTENSION_DISABLED",
            ),
            (
                "mount: File system named dmsgsecrets not found\n",
                "FSKIT_EXTENSION_MISSING",
            ),
            (
                "mount: Loading resource: The operation couldn’t be completed. Permission denied\nmount: Unable to invoke task\n",
                "FSKIT_EXTENSION_REFUSED",
            ),
            ("mount: FSKit unavailable\n", "FSKIT_UNAVAILABLE"),
        ] {
            assert_eq!(fskit_failure(stderr).split(':').next(), Some(code));
        }
    }
}
