//! Read-only secret mappings with Linux handle grants and macOS file-window grants.
//! Backend callbacks are internal core calls, never owner/MCP plaintext RPCs.
use super::process::{Source, Variable};
use super::*;
use crate::hold::{Hold, Unlocked};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
/// Host monotonic milliseconds including time spent suspended. No OS API enters the reusable core.
pub trait Clock: Send + Sync {
    /// A non-decreasing clock shared with the native filesystem module on this boot.
    fn milliseconds(&self) -> Result<u64>;
}

const CONFIGS: &str = "secret-mounts";

/// Platform-selected authorization; never accepted from a filesystem reader.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Mode {
    /// Exact open handle, including explicitly inherited/transferred references.
    LinuxHandle {
        /// OS-verified mount owner.
        owner_uid: u32,
    },
    /// One mapping/version/UID, including other processes of that UID.
    MacosFileWindow {
        /// OS-verified mount owner.
        owner_uid: u32,
        /// Fixed maximum window starting at approval.
        window_ms: u64,
    },
}
impl Mode {
    fn uid(&self) -> u32 {
        match self {
            Self::LinuxHandle { owner_uid } | Self::MacosFileWindow { owner_uid, .. } => *owner_uid,
        }
    }

    fn lifetime(&self) -> u64 {
        match self {
            Self::LinuxHandle { .. } => 900000,
            Self::MacosFileWindow { window_ms, .. } => *window_ms,
        }
    }
}

/// Nonsecret configuration of an explicitly selected source.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum EntrySource {
    /// Strict dotenv/1 rendering from fixed references.
    RefsDotenv {
        /// Selected names and current-field references.
        variables: Vec<Variable>,
    },
    /// Exact UTF-8 field, no newline appended.
    FieldUtf8 {
        /// Canonical dmsg-secret reference URI.
        reference: String,
    },
    /// Exact immutable local Files version.
    LibraryFile {
        /// Selected local Files object.
        file_id: String,
        /// Exact immutable local version.
        version_id: String,
    },
}

/// One file relative to the virtual root.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EntryInput {
    /// Validated path beneath the virtual root.
    pub relative_path: String,
    /// Selected content source; contains no plaintext.
    pub source: EntrySource,
}

/// An explicitly selected project-root symlink. Existing files are never replaced.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Alias {
    /// Single filename at the project root, for example `.env` or `service.json`.
    pub name: String,
    /// Exact relative path of an entry in the mounted volume.
    pub entry: String,
    /// Verified project directory identity, filled by the OS adapter.
    #[serde(default)]
    pub parent_identity: String,
}

/// Owner-selected paths, pinned again by the OS adapter when starting a mount.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MountInput {
    /// Owner-visible project label.
    pub project_label: String,
    /// Canonical project directory verified by the OS adapter.
    pub project_directory: String,
    /// Canonical, empty mount directory; never a plaintext fallback.
    pub mount_point: String,
    #[serde(default)]
    /// OS device/inode stamp of the mount parent.
    pub parent_identity: String,
    /// At most 64 selected mappings.
    pub entries: Vec<EntryInput>,
    /// Optional project-root links, created only when the approved mount starts.
    #[serde(default)]
    pub aliases: Vec<Alias>,
}

/// An LDK-sealed device-local mapping. Saving never grants a read.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    /// Device-local opaque identifier.
    pub id: String,
    /// Monotonic configuration version.
    pub version: u64,
    /// Owning account.
    pub account_id: String,
    /// Owning device.
    pub device_id: String,
    /// Exact reviewed configuration.
    pub input: MountInput,
    /// OS-selected authorization model.
    pub mode: Mode,
}

/// Immutable source versions and true byte size, without content.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    /// Device-local opaque identifier.
    pub id: String,
    /// Validated path beneath the virtual root.
    pub relative_path: String,
    /// Backend inode generation; not an open-handle or process identity.
    pub generation: u64,
    /// Exact advertised plaintext size.
    pub size: u64,
    /// Selected content source; contains no plaintext.
    pub source: ResolvedSource,
}

/// Resolved sources stored in approvals, not decryption keys.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResolvedSource {
    /// One raw field or file version.
    Raw {
        /// Selected content source; contains no plaintext.
        source: Source,
    },
    /// Fixed dotenv/1 field revisions.
    Dotenv {
        /// Explicit variable names and pinned references.
        variables: Vec<(String, ResolvedSecret)>,
    },
}

/// The scope shown to the owner. macOS never invents a per-fd identity.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Scope {
    /// One kernel open handle.
    LinuxHandle {
        /// Linux FUSE handle minted by the trusted backend.
        handle: u64,
        /// OS-verified user.
        uid: u32,
        /// PID observed at open; not a promise of process lifetime or isolation.
        pid: Option<u32>,
    },
    /// One file/version/UID for a fixed interval.
    MacosFileWindow {
        /// OS-verified user.
        uid: u32,
        /// Fixed approval window in milliseconds; reads never extend it.
        window_ms: u64,
    },
}
impl Scope {
    fn key(&self, entry: &str) -> String {
        match self {
            Self::LinuxHandle { handle, .. } => format!("{entry}:{handle}"),
            Self::MacosFileWindow { .. } => entry.into(),
        }
    }
}

/// Fixed mount/entry/version/session, approved only while a callback still waits.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReadPlan {
    /// Exact configuration at request time.
    pub config: Config,
    /// Pinned mapping and source versions.
    pub entry: Entry,
    /// Handle or file-window scope shown in the approval.
    pub scope: Scope,
    /// Fixed maximum duration starting when the owner approves, in milliseconds.
    pub grant_lifetime_ms: u64,
    /// Unlock session; old sessions cannot revive this request.
    pub session_generation: u64,
    /// Daemon lifetime; never resumed after restart.
    pub boot_id: String,
}

struct Grant {
    request: String,
    scope: Scope,
    entry: Entry,
    deadline: u64,
    expires_at: u64,
    released: u64,
}
struct Pending {
    request: Option<String>,
    key: String,
    entry: String,
    generation: u64,
    deadline: u64,
    cancelled: Arc<AtomicBool>,
}

/// Active state never persists or survives lock/restart.
pub(crate) struct Runtime {
    config: Config,
    entries: Vec<Entry>,
    session: u64,
    handles: BTreeMap<u64, (String, u32, Option<u32>)>,
    next_handle: u64,
    pending: BTreeMap<String, Pending>,
    grants: BTreeMap<String, Grant>,
    /// Ended macOS windows per entry, which the module confirms clearing from kernel caches.
    revocations: BTreeMap<String, u64>,
}

/// Callback metadata returned to a trusted filesystem backend.
#[derive(Serialize)]
pub struct Ticket {
    /// One internal pending IO token.
    pub callback_id: String,
    /// Approval request when waiting, absent inside an active grant.
    pub request_id: Option<String>,
    /// An active scope exists; every data return still rechecks it.
    pub ready: bool,
    #[serde(skip)]
    cancelled: Arc<AtomicBool>,
}
impl Ticket {
    /// Share only the cancellation flag with a trusted OS adapter's interrupt handler.
    pub fn cancellation(&self) -> Arc<AtomicBool> {
        self.cancelled.clone()
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

/// Validate a relative file path; no normalization or traversal aliases.
pub fn relative_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 1024
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path.split('/').count() <= 8
        && path
            .split('/')
            .all(|c| !c.is_empty() && c != "." && c != ".." && c.len() <= 255)
}

fn render(
    variables: impl IntoIterator<Item = (String, Zeroizing<String>)>,
) -> Result<Zeroizing<Vec<u8>>> {
    let mut out = Zeroizing::new(Vec::new());
    for (name, value) in variables {
        ensure_valid(
            !value
                .chars()
                .any(|c| matches!(c, '\'' | '$' | '\\' | '\n' | '\r' | '\0')),
            "dotenv/1 value",
        )?;
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(b"='");
        out.extend_from_slice(value.as_bytes());
        out.extend_from_slice(b"'\n");
    }
    Ok(out)
}

impl Client {
    /// Install the OS adapter's continuous clock; never configurable through an RPC.
    pub fn set_secret_mount_clock(&mut self, clock: Arc<dyn Clock>) {
        self.secret_mount_clock = Some(clock);
    }

    fn mount_now(&self) -> Result<u64> {
        self.secret_mount_clock
            .as_ref()
            .ok_or(Error::UnsupportedProtocol)?
            .milliseconds()
    }

    /// Owner's device-local configurations.
    pub fn secret_mount_configs(&self) -> Result<Vec<Config>> {
        Ok(self
            .workspace
            .private(self.keys()?, CONFIGS)?
            .unwrap_or_default())
    }

    fn mount_entry(&mut self, input: &EntryInput, index: usize) -> Result<Entry> {
        ensure_valid(relative_path(&input.relative_path), "mount entry path")?;
        let (source, size) = match &input.source {
            EntrySource::FieldUtf8 { reference } => {
                let (source, size) = self.secret_source(reference, crate::files::MAX_FILE)?;
                ensure(matches!(source, Source::Field { .. }), Error::Forbidden)?;
                (ResolvedSource::Raw { source }, size)
            }
            EntrySource::LibraryFile {
                file_id,
                version_id,
            } => {
                let (source, size) = self.secret_source(
                    &format!("dmsg-file://{file_id}/{version_id}"),
                    crate::files::MAX_FILE,
                )?;
                (ResolvedSource::Raw { source }, size)
            }
            EntrySource::RefsDotenv { variables } => {
                ensure_valid(
                    !variables.is_empty() && variables.len() <= 64,
                    "dotenv variables",
                )?;
                let mut names = BTreeSet::new();
                let mut resolved = Vec::new();
                let mut values = Vec::new();
                for v in variables {
                    ensure_valid(
                        process::variable_name(&v.name) && names.insert(&v.name),
                        "dotenv variable",
                    )?;
                    let r = self.secret_resolve(&v.reference, false)?;
                    values.push((v.name.clone(), self.secret_value(&v.reference)?));
                    resolved.push((v.name.clone(), r));
                }
                let size = render(values)?.len() as u64;
                (
                    ResolvedSource::Dotenv {
                        variables: resolved,
                    },
                    size,
                )
            }
        };
        Ok(Entry {
            id: format!("e{index}"),
            relative_path: input.relative_path.clone(),
            generation: 1,
            size,
            source,
        })
    }

    /// Prepare validated metadata; no filesystem is mounted and no source value is returned.
    pub fn secret_mount_prepare(&mut self, input: &MountInput, mode: &Mode) -> Result<Value> {
        ensure_valid(
            !input.project_label.is_empty()
                && input.project_label.chars().count() <= 100
                && std::path::Path::new(&input.project_directory).is_absolute()
                && std::path::Path::new(&input.mount_point).is_absolute()
                && !input.parent_identity.is_empty()
                && input.parent_identity.len() <= 128
                && !input.entries.is_empty()
                && input.entries.len() <= 64,
            "mount configuration",
        )?;
        if let Mode::MacosFileWindow { window_ms, .. } = mode {
            ensure_valid((1..=60000).contains(window_ms), "file window")?;
        }
        let mut paths = BTreeSet::new();
        let mut entries = Vec::new();
        for (i, entry) in input.entries.iter().enumerate() {
            ensure_valid(paths.insert(&entry.relative_path), "duplicate mapping")?;
            entries.push(self.mount_entry(entry, i)?);
        }
        // A file cannot also be another entry's parent directory.
        for p in &paths {
            ensure_valid(
                !paths
                    .iter()
                    .any(|q| *p != *q && q.starts_with(&format!("{p}/"))),
                "mapping path collision",
            )?;
        }
        let mut aliases = BTreeSet::new();
        ensure_valid(input.aliases.len() <= 64, "project aliases")?;
        for alias in &input.aliases {
            ensure_valid(
                relative_path(&alias.name)
                    && !alias.name.contains('/')
                    && aliases.insert(&alias.name)
                    && paths.contains(&alias.entry)
                    && !alias.parent_identity.is_empty(),
                "project alias",
            )?;
        }
        ensure(canonical(input).len() <= 256 * 1024, Error::QuotaExceeded)?;
        Ok(
            json!({"entries":entries,"authorization":mode,"readonly":true,"mode":256,"same_user_other_programs":matches!(mode,Mode::MacosFileWindow{..}),"approval_timeout_ms":30000}),
        )
    }

    /// Save only by high-risk approval; platform mode/UID must be supplied by the OS adapter.
    pub fn secret_mount_request(
        &mut self,
        source: &str,
        id: Option<String>,
        base: Option<u64>,
        input: MountInput,
        mode: Mode,
    ) -> Result<Value> {
        ensure(matches!(source, "gui" | "cli"), Error::Forbidden)?;
        self.secret_mount_prepare(&input, &mode)?;
        let configs = self.secret_mount_configs()?;
        let (id, version) = match id {
            Some(id) => {
                let current = configs.iter().find(|c| c.id == id).ok_or(Error::NotFound)?;
                ensure(Some(current.version) == base, Error::VersionConflict)?;
                (
                    id,
                    current.version.checked_add(1).ok_or(Error::QuotaExceeded)?,
                )
            }
            None => {
                ensure(base.is_none() && configs.len() < 16, Error::QuotaExceeded)?;
                (hex::encode(random_id()), 1)
            }
        };
        let config = Config {
            id,
            version,
            account_id: self.account()?.id.to_string(),
            device_id: self.secret_device(),
            input,
            mode,
        };
        let now = now_ms();
        let r = self.approval_create_at(
            source,
            Action::SecretMountConfig {
                config: Box::new(config),
            },
            now,
            now + crate::approval::HIGH_RISK_TTL_MS,
        )?;
        self.approval_view(&r)
    }

    /// Remove a stopped device configuration. This never removes project files or aliases.
    pub fn secret_mount_remove(&mut self, id: &str) -> Result<Value> {
        ensure(
            !self.secret_mount_runtime.contains_key(id),
            Error::VersionConflict,
        )?;
        let mut configs = self.secret_mount_configs()?;
        ensure(configs.iter().any(|c| c.id == id), Error::NotFound)?;
        configs.retain(|c| c.id != id);
        self.workspace
            .put_private(self.keys()?, CONFIGS, &configs)?;
        Ok(json!({"removed":id,"project_files_unchanged":true}))
    }

    pub(crate) fn secret_mount_apply(&mut self, config: &Config, request_id: &str) -> Result<()> {
        ensure(
            config.account_id == self.account()?.id.to_string()
                && config.device_id == self.secret_device(),
            Error::Forbidden,
        )?;
        self.secret_mount_prepare(&config.input, &config.mode)?;
        let mut configs = self.secret_mount_configs()?;
        let old = configs
            .iter()
            .find(|c| c.id == config.id)
            .map(|c| c.version)
            .unwrap_or(0);
        ensure(config.version == old + 1, Error::VersionConflict)?;
        self.secret_mount_stop(&config.id)?;
        configs.retain(|c| c.id != config.id);
        configs.push(config.clone());
        self.workspace.store.atomic(|_| {
            self.workspace
                .put_private(self.keys()?, CONFIGS, &configs)?;
            let mut request = self.approval_record(request_id)?;
            request.started = true;
            request.state = State::Done;
            self.write_approval(&request)
        })
    }

    /// Attach an already verified backend to the unlocked account. Does not grant reads.
    pub fn secret_mount_start(&mut self, id: &str) -> Result<Value> {
        self.mount_now()?;
        let config = self
            .secret_mount_configs()?
            .into_iter()
            .find(|c| c.id == id)
            .ok_or(Error::NotFound)?;
        ensure(
            config.account_id == self.account()?.id.to_string()
                && config.device_id == self.secret_device(),
            Error::Forbidden,
        )?;
        let metadata = self.secret_mount_prepare(&config.input, &config.mode)?;
        self.secret_mount_stop(id)?;
        let mut entries: Vec<Entry> = serde_json::from_value(metadata["entries"].clone())
            .map_err(|_| Error::IntegrityFailed)?;
        for entry in &mut entries {
            entry.generation = (u64::from_be_bytes(random_id()[..8].try_into().expect("8 bytes"))
                & ((1 << 53) - 1))
                .max(1);
        }
        let mut metadata = metadata;
        metadata["entries"] = json!(entries);
        self.secret_mount_runtime.insert(
            id.into(),
            Runtime {
                config: config.clone(),
                entries,
                session: self.session_generation,
                handles: BTreeMap::new(),
                next_handle: 0,
                pending: BTreeMap::new(),
                grants: BTreeMap::new(),
                revocations: BTreeMap::new(),
            },
        );
        Ok(json!({"config":config,"metadata":metadata}))
    }

    fn mount_source_check(&mut self, entry: &Entry) -> Result<()> {
        match &entry.source {
            ResolvedSource::Raw { source } => self.secret_source_check(source),
            ResolvedSource::Dotenv { variables } => {
                for (_, r) in variables {
                    ensure(
                        self.secret_resolve(&r.reference, false)? == *r,
                        failure("SECRET_VERSION_STALE"),
                    )?;
                }
                Ok(())
            }
        }
    }

    /// A true Linux FUSE handle, minted inside the backend, never by a reader/MCP.
    pub fn secret_mount_open(
        &mut self,
        id: &str,
        entry: &str,
        uid: u32,
        pid: Option<u32>,
    ) -> Result<u64> {
        self.keys()?;
        let rt = self
            .secret_mount_runtime
            .get_mut(id)
            .ok_or(Error::NotFound)?;
        ensure(
            matches!(rt.config.mode, Mode::LinuxHandle { .. })
                && uid == rt.config.mode.uid()
                && rt.entries.iter().any(|e| e.id == entry)
                && rt.handles.len() < 64
                && rt.session == self.session_generation,
            Error::Forbidden,
        )?;
        rt.next_handle = rt.next_handle.checked_add(1).ok_or(Error::QuotaExceeded)?;
        let handle = rt.next_handle;
        rt.handles.insert(handle, (entry.into(), uid, pid));
        Ok(handle)
    }

    /// Metadata only. Backends must reject stale generation/file identities.
    pub fn secret_mount_entries(&self, id: &str) -> Result<Vec<Entry>> {
        self.keys()?;
        Ok(self
            .secret_mount_runtime
            .get(id)
            .ok_or(Error::NotFound)?
            .entries
            .clone())
    }

    /// Register one waiting IO, merging macOS requests only within the same entry/version/UID.
    pub fn secret_mount_intent(
        &mut self,
        id: &str,
        entry_id: &str,
        generation: u64,
        uid: u32,
        handle: Option<u64>,
    ) -> Result<Ticket> {
        let now = now_ms();
        let at = self.mount_now()?;
        self.mount_tick_at(at, now)?;
        let rt = self.secret_mount_runtime.get(id).ok_or(Error::NotFound)?;
        ensure(
            rt.session == self.session_generation
                && uid == rt.config.mode.uid()
                && rt.pending.len() < 64,
            Error::Forbidden,
        )?;
        let entry = rt
            .entries
            .iter()
            .find(|e| e.id == entry_id && e.generation == generation)
            .cloned()
            .ok_or(Error::VersionConflict)?;
        let scope = match &rt.config.mode {
            Mode::LinuxHandle { .. } => {
                let h = handle.ok_or(Error::Forbidden)?;
                let (e, u, p) = rt.handles.get(&h).ok_or(Error::Forbidden)?;
                ensure(e == entry_id && *u == uid, Error::Forbidden)?;
                Scope::LinuxHandle {
                    handle: h,
                    uid,
                    pid: *p,
                }
            }
            Mode::MacosFileWindow { window_ms, .. } => {
                ensure(handle.is_none(), Error::Forbidden)?;
                Scope::MacosFileWindow {
                    uid,
                    window_ms: *window_ms,
                }
            }
        };
        let key = scope.key(entry_id);
        let ready = rt
            .grants
            .get(&key)
            .is_some_and(|g| at < g.deadline && g.entry == entry);
        let existing = rt
            .pending
            .values()
            .find(|p| p.key == key && p.deadline > at && !p.cancelled.load(Ordering::Acquire))
            .and_then(|p| p.request.clone());
        let config = rt.config.clone();
        self.mount_source_check(&entry)?;
        let request = if ready {
            None
        } else if let Some(id) = existing {
            Some(id)
        } else {
            let grant_lifetime_ms = config.mode.lifetime();
            let plan = ReadPlan {
                config,
                grant_lifetime_ms,
                entry: entry.clone(),
                scope,
                session_generation: self.session_generation,
                boot_id: self.secret_boot.clone(),
            };
            Some(
                self.approval_create_at(
                    &format!("mount:{id}"),
                    Action::SecretMountRead {
                        plan: Box::new(plan),
                    },
                    now,
                    now + 30000,
                )?
                .id,
            )
        };
        let callback = hex::encode(random_id());
        let cancelled = Arc::new(AtomicBool::new(false));
        self.secret_mount_runtime
            .get_mut(id)
            .ok_or(Error::NotFound)?
            .pending
            .insert(
                callback.clone(),
                Pending {
                    request: request.clone(),
                    key,
                    entry: entry_id.into(),
                    generation,
                    deadline: at + 30000,
                    cancelled: cancelled.clone(),
                },
            );
        Ok(Ticket {
            callback_id: callback,
            request_id: request,
            ready,
            cancelled,
        })
    }

    pub(crate) fn secret_mount_check(&mut self, p: &ReadPlan) -> Result<()> {
        self.keys()?;
        let at = self.mount_now()?;
        let rt = self
            .secret_mount_runtime
            .get(&p.config.id)
            .ok_or(Error::Expired)?;
        ensure(
            p.boot_id == self.secret_boot
                && p.session_generation == self.session_generation
                && rt.session == self.session_generation
                && rt.config == p.config
                && rt.entries.contains(&p.entry)
                && rt.pending.values().any(|io| {
                    io.key == p.scope.key(&p.entry.id)
                        && io.deadline > at
                        && !io.cancelled.load(Ordering::Acquire)
                }),
            Error::Expired,
        )?;
        if let Scope::LinuxHandle { handle, .. } = &p.scope {
            ensure(rt.handles.contains_key(handle), Error::Expired)?;
        }
        self.mount_source_check(&p.entry)
    }

    pub(crate) fn secret_mount_approve(&mut self, r: &mut Request, now: u64) -> Result<Value> {
        let Action::SecretMountRead { plan } = &r.action else {
            return Err(Error::Forbidden);
        };
        self.secret_mount_check(plan)?;
        let plan = plan.clone();
        let at = self.mount_now()?;
        let lifetime = plan.config.mode.lifetime();
        let chunks = match &plan.entry.source {
            ResolvedSource::Raw {
                source:
                    Source::Library {
                        file_id,
                        version_id,
                        ..
                    },
            } => {
                let (_, _, mut m) = self.library_manifest(file_id, Some(version_id))?;
                m.key.zeroize();
                m.chunks.into_iter().map(|c| c.digest).collect()
            }
            _ => Vec::new(),
        };
        r.started = true;
        r.delivery_report = Some(
            json!({"released_bytes":0,"scope":plan.scope,"expires_at":now+lifetime,"boundary":"released_to_filesystem_backend"}),
        );
        self.workspace.store.atomic(|_| {
            self.write_approval(r)?;
            self.hold_chunks([(r.id.clone(), now + lifetime, chunks)])
        })?;
        let rt = self
            .secret_mount_runtime
            .get_mut(&plan.config.id)
            .ok_or(Error::Expired)?;
        rt.grants.insert(
            plan.scope.key(&plan.entry.id),
            Grant {
                request: r.id.clone(),
                scope: plan.scope.clone(),
                entry: plan.entry.clone(),
                deadline: at + lifetime,
                expires_at: now + lifetime,
                released: 0,
            },
        );
        self.running.insert(r.id.clone(), now);
        self.approval_view(r)
    }

    /// Cancel a callback. A late decision cannot create a window without a waiting IO.
    pub fn secret_mount_cancel_io(&mut self, id: &str, callback: &str) -> Result<()> {
        if let Some(rt) = self.secret_mount_runtime.get_mut(id) {
            rt.pending.remove(callback);
        }
        self.expire_approvals(now_ms())
    }

    /// End a Linux handle, including pending callbacks and any grant for it.
    pub fn secret_mount_close(&mut self, id: &str, handle: u64) -> Result<()> {
        let at = self.mount_now()?;
        if let Some(rt) = self.secret_mount_runtime.get_mut(id) {
            rt.handles.remove(&handle);
            let suffix = format!(":{handle}");
            rt.pending.retain(|_, p| !p.key.ends_with(&suffix));
            for (k, g) in &mut rt.grants {
                if k.ends_with(&suffix) {
                    g.deadline = at;
                }
            }
        }
        let now = now_ms();
        self.mount_tick_at(at, now)?;
        self.expire_approvals(now)
    }

    /// Revoke selected read scopes without deleting the configuration.
    pub fn secret_mount_revoke(&mut self, id: &str, entry: Option<&str>) -> Result<Value> {
        let at = if self.secret_mount_runtime.contains_key(id) {
            self.mount_now()?
        } else {
            0
        };
        if let Some(rt) = self.secret_mount_runtime.get_mut(id) {
            for g in rt.grants.values_mut() {
                if entry.is_none_or(|e| e == g.entry.id) {
                    g.deadline = at;
                }
            }
            rt.pending
                .retain(|_, p| entry.is_some_and(|e| e != p.entry));
        }
        let now = now_ms();
        let revoked = self.mount_tick_at(at, now)?;
        self.expire_approvals(now)?;
        Ok(json!({"revoked":revoked}))
    }

    /// Stop supplying data before the OS adapter attempts unmounting.
    pub fn secret_mount_stop(&mut self, id: &str) -> Result<Value> {
        self.secret_mount_revoke(id, None)?;
        self.secret_mount_runtime.remove(id);
        Ok(json!({"stopped":id,"os_unmounted":false}))
    }

    /// Expire scopes using a monotonic clock. Returns the macOS entries whose kernel
    /// caches must be cleared, each with its revocation count for the module to confirm.
    pub fn secret_mount_tick(&mut self) -> Result<Vec<(String, String, u64)>> {
        self.keys()?;
        if self.secret_mount_runtime.is_empty() {
            return Ok(Vec::new());
        }
        let at = self.mount_now()?;
        self.mount_tick_at(at, now_ms())
    }

    fn mount_tick_at(&mut self, at: u64, now: u64) -> Result<Vec<(String, String, u64)>> {
        let mut ended = Vec::new();
        let mut revoked = Vec::new();
        for (id, rt) in &mut self.secret_mount_runtime {
            rt.pending
                .retain(|_, p| p.deadline > at && !p.cancelled.load(Ordering::Acquire));
            let expired: Vec<_> = rt
                .grants
                .iter()
                .filter(|(_, g)| g.deadline <= at)
                .map(|(k, _)| k.clone())
                .collect();
            for key in expired {
                let g = rt.grants.remove(&key).expect("grant");
                // The item keeps its identity: macOS hands new opens of a path to the
                // item it already holds, so a later read must reach a new approval.
                if matches!(g.scope, Scope::MacosFileWindow { .. }) {
                    let count = rt.revocations.entry(g.entry.id.clone()).or_default();
                    *count += 1;
                    revoked.push((id.clone(), g.entry.id.clone(), *count));
                }
                ended.push(g);
            }
            for g in rt.grants.values() {
                self.running.insert(g.request.clone(), now);
            }
        }
        for g in ended {
            let mut r = self.approval_record(&g.request)?;
            if r.state == State::Executing {
                r.state = State::Done;
                r.updated_at = now;
                r.delivery_report = Some(
                    json!({"released_bytes":g.released,"scope_ended":true,"expires_at":g.expires_at,"boundary":"released_to_filesystem_backend"}),
                );
                self.save_approval(&r)?;
            }
            self.running.remove(&g.request);
            self.release_chunks(&g.request)?;
        }
        Ok(revoked)
    }

    /// Detect source changes without resolving any new version into an existing grant.
    pub fn secret_mount_invalid_sources(&mut self, id: &str) -> Result<Vec<String>> {
        let entries = self.secret_mount_entries(id)?;
        let invalid: Vec<_> = entries
            .into_iter()
            .filter_map(|e| self.mount_source_check(&e).err().map(|_| e.id))
            .collect();
        for entry in &invalid {
            self.secret_mount_revoke(id, Some(entry))?;
        }
        Ok(invalid)
    }

    /// Owner status contains paths/versions/countdowns only, never values.
    pub fn secret_mount_status(&mut self, id: &str) -> Result<Value> {
        let at = self.mount_now()?;
        self.mount_tick_at(at, now_ms())?;
        let rt = self.secret_mount_runtime.get(id).ok_or(Error::NotFound)?;
        Ok(
            json!({"config":rt.config,"entries":rt.entries,"revocations":rt.revocations,"pending_io":rt.pending.len(),"handles":rt.handles.len(),"grants":rt.grants.values().map(|g|json!({"request_id":g.request,"entry":g.entry.id,"scope":g.scope,"remaining_ms":g.deadline.saturating_sub(at),"monotonic_deadline_ms":g.deadline,"released_bytes":g.released})).collect::<Vec<_>>()}),
        )
    }

    fn mount_ticket(&mut self, id: &str, callback: &str) -> Result<Entry> {
        let at = self.mount_now()?;
        self.mount_tick_at(at, now_ms())?;
        let rt = self.secret_mount_runtime.get(id).ok_or(Error::Expired)?;
        let p = rt.pending.get(callback).ok_or(Error::Expired)?;
        ensure(
            rt.session == self.session_generation
                && p.deadline > at
                && !p.cancelled.load(Ordering::Acquire),
            Error::Expired,
        )?;
        let grant = rt.grants.get(&p.key).ok_or(Error::Forbidden)?;
        ensure(
            grant.deadline > at
                && grant.entry.id == p.entry
                && grant.entry.generation == p.generation,
            Error::Expired,
        )?;
        let entry = grant.entry.clone();
        let request = grant.request.clone();
        ensure(
            self.approval_record(&request)?.state == State::Executing,
            Error::Expired,
        )?;
        self.mount_source_check(&entry)?;
        Ok(entry)
    }
}

/// One bounded authorized range. Partial Files reads verify each selected block's AEAD.
pub async fn read<H: Hold>(
    h: &H,
    id: &str,
    callback: &str,
    offset: u64,
    length: u32,
) -> Result<Zeroizing<Vec<u8>>> {
    ensure(length as usize <= crate::files::CHUNK, Error::QuotaExceeded)?;
    let bound = Unlocked::new(h).await?;
    let entry = bound.with(|c| c.mount_ticket(id, callback)).await?;
    let end = offset
        .checked_add(length as u64)
        .ok_or(Error::InvalidInput("range".into()))?
        .min(entry.size);
    let start = offset.min(entry.size);
    let mut bytes = Zeroizing::new(Vec::new());
    match &entry.source {
        ResolvedSource::Raw {
            source:
                Source::Library {
                    file_id,
                    version_id,
                    ..
                },
        } => {
            if start < end {
                for index in
                    start / crate::files::CHUNK as u64..=(end - 1) / crate::files::CHUNK as u64
                {
                    let block = bound
                        .with(|c| {
                            c.mount_ticket(id, callback)?;
                            let (_, _, mut m) = c.library_manifest(file_id, Some(version_id))?;
                            let b = crate::files::read_chunk(c.blobs()?, &m, index);
                            m.key.zeroize();
                            b
                        })
                        .await?;
                    let base = index * crate::files::CHUNK as u64;
                    bytes.extend_from_slice(
                        &block[(start.saturating_sub(base)) as usize
                            ..(end - base).min(block.len() as u64) as usize],
                    );
                }
            }
        }
        source => {
            let all = bound
                .with(|c| {
                    c.mount_ticket(id, callback)?;
                    match source {
                        ResolvedSource::Raw {
                            source: Source::Field { resolved },
                        } => Ok(Zeroizing::new(
                            c.secret_value(&resolved.reference)?.as_bytes().to_vec(),
                        )),
                        ResolvedSource::Dotenv { variables } => render(
                            variables
                                .iter()
                                .map(|(n, r)| Ok((n.clone(), c.secret_value(&r.reference)?)))
                                .collect::<Result<Vec<_>>>()?,
                        ),
                        _ => Err(Error::Forbidden),
                    }
                })
                .await?;
            ensure(all.len() as u64 == entry.size, Error::IntegrityFailed)?;
            bytes.extend_from_slice(&all[start as usize..end as usize]);
        }
    }
    bound.with(|c|{
        c.mount_ticket(id,callback)?;let rt=c.secret_mount_runtime.get_mut(id).ok_or(Error::Expired)?;let p=rt.pending.remove(callback).ok_or(Error::Expired)?;let g=rt.grants.get_mut(&p.key).ok_or(Error::Expired)?;g.released=g.released.saturating_add(bytes.len() as u64);let(request,released,expires_at)=(g.request.clone(),g.released,g.expires_at);
        let mut r=c.approval_record(&request)?;r.delivery_report=Some(json!({"released_bytes":released,"expires_at":expires_at,"boundary":"released_to_filesystem_backend"}));c.save_approval(&r)
    }).await?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        testing::{TempDir, unlocked},
        vault::{Changes, FieldChange},
    };
    async fn configured(mode: Mode) -> (TempDir, Client, String, SecretRef) {
        let dir = TempDir::new();
        let mut c = unlocked(&dir.0, false, None);
        struct TestClock(std::time::Instant);
        impl Clock for TestClock {
            fn milliseconds(&self) -> Result<u64> {
                Ok(1000 + self.0.elapsed().as_millis() as u64)
            }
        }
        c.set_secret_mount_clock(Arc::new(TestClock(std::time::Instant::now())));
        let item = c
            .vault_create(
                "api",
                &Changes {
                    title: Some("mount fixture".into()),
                    set: Some(vec![FieldChange {
                        id: Some("credential".into()),
                        value: Some("SYNTHETIC_MOUNT_VALUE".into()),
                        ..Default::default()
                    }]),
                    ..Default::default()
                },
            )
            .unwrap();
        let reference = SecretRef {
            schema_version: 1,
            account_id: c.account().unwrap().id.to_string(),
            item_id: item.id,
            field_id: "credential".into(),
        };
        let input = MountInput {
            project_label: "fixture".into(),
            aliases: vec![],
            project_directory: "/tmp/project".into(),
            mount_point: "/tmp/project/.dmsg-secrets".into(),
            parent_identity: "1:2".into(),
            entries: vec![
                EntryInput {
                    relative_path: ".env".into(),
                    source: EntrySource::RefsDotenv {
                        variables: vec![Variable {
                            name: "TOKEN".into(),
                            reference: reference.clone(),
                        }],
                    },
                },
                EntryInput {
                    relative_path: "key.txt".into(),
                    source: EntrySource::FieldUtf8 {
                        reference: reference.uri(),
                    },
                },
            ],
        };
        let r = c
            .secret_mount_request("gui", None, None, input, mode)
            .unwrap();
        c.approval_respond(
            r["id"].as_str().unwrap(),
            r["digest"].as_str().unwrap(),
            true,
            "gui",
            false,
        )
        .await
        .unwrap();
        let id = c.secret_mount_configs().unwrap()[0].id.clone();
        c.secret_mount_start(&id).unwrap();
        (dir, c, id, reference)
    }

    async fn approve(c: &mut Client, id: &str) {
        let r = c.approval(id).unwrap();
        c.approval_respond(id, r["digest"].as_str().unwrap(), true, "gui", false)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn filesystem_requests_leave_room_for_owner_approvals() {
        let (_dir, mut c, id, _) = configured(Mode::LinuxHandle { owner_uid: 501 }).await;
        let generation = c.secret_mount_entries(&id).unwrap()[0].generation;
        let mut pending = Vec::new();
        for _ in 0..crate::approval::MAX_PER_SOURCE {
            let handle = c.secret_mount_open(&id, "e0", 501, Some(1)).unwrap();
            pending.push(
                c.secret_mount_intent(&id, "e0", generation, 501, Some(handle))
                    .unwrap(),
            );
        }
        let handle = c.secret_mount_open(&id, "e0", 501, Some(1)).unwrap();
        assert!(matches!(
            c.secret_mount_intent(&id, "e0", generation, 501, Some(handle)),
            Err(Error::QuotaExceeded)
        ));
        let config = c.secret_mount_configs().unwrap()[0].clone();
        let now = now_ms();
        assert!(
            c.approval_create(
                "gui",
                Action::SecretMountConfig {
                    config: Box::new(config)
                },
                now + 30000
            )
            .is_ok()
        );
    }

    #[tokio::test]
    async fn continuous_deadline_expires_without_wall_clock_or_read_renewal() {
        use std::sync::atomic::AtomicU64;
        struct FakeClock(Arc<AtomicU64>);
        impl Clock for FakeClock {
            fn milliseconds(&self) -> Result<u64> {
                Ok(self.0.load(Ordering::SeqCst))
            }
        }
        let (_dir, mut c, id, _) = configured(Mode::MacosFileWindow {
            owner_uid: 501,
            window_ms: 30000,
        })
        .await;
        let time = Arc::new(AtomicU64::new(1000));
        c.set_secret_mount_clock(Arc::new(FakeClock(time.clone())));
        let generation = c.secret_mount_entries(&id).unwrap()[0].generation;
        let pending = c
            .secret_mount_intent(&id, "e0", generation, 501, None)
            .unwrap();
        approve(&mut c, pending.request_id.as_ref().unwrap()).await;
        time.store(31000, Ordering::SeqCst);
        assert!(
            read(&c.hold(), &id, &pending.callback_id, 0, 1)
                .await
                .is_err()
        );
        assert!(
            c.secret_mount_status(&id).unwrap()["grants"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        // The item keeps its identity; its next read waits for a new approval.
        assert_eq!(
            c.secret_mount_entries(&id).unwrap()[0].generation,
            generation
        );
        assert_eq!(c.secret_mount_status(&id).unwrap()["revocations"]["e0"], 1);
        assert!(
            !c.secret_mount_intent(&id, "e0", generation, 501, None)
                .unwrap()
                .ready
        );
    }

    #[tokio::test]
    async fn mac_window_is_one_entry_revision_uid_and_never_renews() {
        let (_dir, mut c, id, _) = configured(Mode::MacosFileWindow {
            owner_uid: 501,
            window_ms: 30000,
        })
        .await;
        let entries = c.secret_mount_entries(&id).unwrap();
        let generation = entries[0].generation;
        assert_eq!(entries[0].size, 30);
        assert!(
            !serde_json::to_string(&entries)
                .unwrap()
                .contains("SYNTHETIC_MOUNT_VALUE")
        );
        let first = c
            .secret_mount_intent(&id, "e0", generation, 501, None)
            .unwrap();
        let second = c
            .secret_mount_intent(&id, "e0", generation, 501, None)
            .unwrap();
        assert_eq!(first.request_id, second.request_id);
        assert!(
            read(&c.hold(), &id, &first.callback_id, 0, 100)
                .await
                .is_err()
        );
        approve(&mut c, first.request_id.as_ref().unwrap()).await;
        let deadline = c.secret_mount_runtime[&id].grants["e0"].deadline;
        assert_eq!(
            &*read(&c.hold(), &id, &first.callback_id, 0, 100)
                .await
                .unwrap(),
            b"TOKEN='SYNTHETIC_MOUNT_VALUE'\n"
        );
        let later = c
            .secret_mount_intent(&id, "e0", generation, 501, None)
            .unwrap();
        assert!(later.ready);
        assert_eq!(deadline, c.secret_mount_runtime[&id].grants["e0"].deadline);
        assert!(
            c.secret_mount_intent(&id, "e0", generation, 502, None)
                .is_err()
        );
        let other = c
            .secret_mount_intent(&id, "e1", entries[1].generation, 501, None)
            .unwrap();
        assert!(!other.ready);
        c.secret_mount_revoke(&id, Some("e0")).unwrap();
        assert!(
            read(&c.hold(), &id, &later.callback_id, 0, 1)
                .await
                .is_err()
        );
        let after = c
            .secret_mount_intent(&id, "e0", generation, 501, None)
            .unwrap();
        assert!(!after.ready && after.request_id.is_some());
        assert_eq!(c.secret_mount_status(&id).unwrap()["revocations"]["e0"], 1);
    }

    #[tokio::test]
    async fn cancelled_reads_late_approval_restart_and_changed_source_cannot_revive() {
        let (_dir, mut c, id, reference) = configured(Mode::MacosFileWindow {
            owner_uid: 501,
            window_ms: 30000,
        })
        .await;
        let generation = c.secret_mount_entries(&id).unwrap()[0].generation;
        let first = c
            .secret_mount_intent(&id, "e0", generation, 501, None)
            .unwrap();
        let r = c.approval(first.request_id.as_ref().unwrap()).unwrap();
        c.secret_mount_cancel_io(&id, &first.callback_id).unwrap();
        assert!(
            c.approval_respond(
                r["id"].as_str().unwrap(),
                r["digest"].as_str().unwrap(),
                true,
                "gui",
                false
            )
            .await
            .is_err()
        );
        assert!(c.secret_mount_runtime[&id].grants.is_empty());
        let first = c
            .secret_mount_intent(&id, "e0", generation, 501, None)
            .unwrap();
        approve(&mut c, first.request_id.as_ref().unwrap()).await;
        c.vault_edit(
            &reference.item_id,
            None,
            &Changes {
                set: Some(vec![FieldChange {
                    id: Some(reference.field_id.clone()),
                    value: Some("replacement".into()),
                    ..Default::default()
                }]),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            read(&c.hold(), &id, &first.callback_id, 0, 1)
                .await
                .is_err()
        );
        c.secret_mount_start(&id).unwrap();
        assert!(
            c.secret_mount_intent(&id, "e0", generation, 501, None)
                .is_err()
        );
        c.lock();
        assert!(c.secret_mount_runtime.is_empty());
    }

    #[tokio::test]
    async fn linux_approval_is_bound_to_the_original_handle() {
        let (_dir, mut c, id, _) = configured(Mode::LinuxHandle { owner_uid: 501 }).await;
        let generation = c.secret_mount_entries(&id).unwrap()[0].generation;
        let a = c.secret_mount_open(&id, "e0", 501, Some(10)).unwrap();
        let b = c.secret_mount_open(&id, "e0", 501, Some(20)).unwrap();
        let first = c
            .secret_mount_intent(&id, "e0", generation, 501, Some(a))
            .unwrap();
        let other = c
            .secret_mount_intent(&id, "e0", generation, 501, Some(b))
            .unwrap();
        assert_ne!(first.request_id, other.request_id);
        approve(&mut c, first.request_id.as_ref().unwrap()).await;
        assert!(
            read(&c.hold(), &id, &other.callback_id, 0, 1)
                .await
                .is_err()
        );
        assert_eq!(
            &*read(&c.hold(), &id, &first.callback_id, 7, 3)
                .await
                .unwrap(),
            b"SYN"
        );
        let pending = c
            .secret_mount_intent(&id, "e0", generation, 501, Some(a))
            .unwrap();
        c.secret_mount_close(&id, a).unwrap();
        assert!(
            read(&c.hold(), &id, &pending.callback_id, 0, 1)
                .await
                .is_err()
        );
    }

    #[test]
    fn dotenv_renderer_and_paths_refuse_ambiguous_or_executable_syntax() {
        for bad in ["..", "/root", "a/../b", "a//b", "a\\b", "a\n"] {
            assert!(!relative_path(bad));
        }
        for bad in ["quote'", "$HOME", "a\\b", "a\n", "a\0"] {
            assert!(render([(String::from("TOKEN"), Zeroizing::new(bad.into()))]).is_err());
        }
        assert_eq!(
            &*render([(String::from("TOKEN"), Zeroizing::new("hello world".into()))]).unwrap(),
            b"TOKEN='hello world'\n"
        );
    }
}
