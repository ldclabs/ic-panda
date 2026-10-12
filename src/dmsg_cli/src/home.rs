//! The dMsg home directory: `config.toml`, the owner credential, the daemon
//! lock and discovery file, and one workspace directory per environment.
//!
//! Security settings live only in `config.toml`; nothing a running daemon
//! receives can change them. Secrets that open the workspace live in the
//! system credential store (`secure_storage = "system"`), never in the home;
//! with `"file"` the owner credential is a file only its owner can read and
//! fast unlock is unavailable.

use candid::Principal;
use dmsg_client::config::{parse_environment, Config, LOGIN_URL};
use dmsg_client::secure::SecureStorage;
use dmsg_types::{Error, Result, MINUTE};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// `config.toml`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    environment: String,
    ic_host: String,
    relay: String,
    handle: String,
    #[serde(default)]
    user_homes: Vec<String>,
    root_key: Option<String>,
    login_url: Option<String>,
    #[serde(default)]
    background_online: bool,
    idle_lock_minutes: Option<u64>,
    #[serde(default)]
    sync_interval_minutes: u64,
    secure_storage: Option<String>,
}

/// Where the home keeps the secrets that open it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Storage {
    /// The system credential store.
    System,
    /// Owner-only files; no fast unlock.
    File,
}

/// `config.toml`: the deployment and the daemon's policies.
#[derive(Clone, Debug)]
pub struct Settings {
    /// The deployment.
    pub config: Config,
    /// Inactivity before the daemon locks.
    pub idle_lock_ms: u64,
    /// Interval of the background sync; 0 turns it off.
    pub sync_interval_ms: u64,
    /// Where secrets live.
    pub storage: Storage,
}

/// Where the running daemon listens.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Discovery {
    /// Loopback port.
    pub port: u16,
    /// Process ID.
    pub pid: u32,
    /// Random ID of this daemon run.
    pub instance: String,
}

/// A home directory.
pub struct Home {
    /// Its path.
    pub dir: PathBuf,
}

fn io(error: std::io::Error) -> Error {
    Error::Unavailable(format!("home: {error}"))
}

fn principal(text: &str) -> Result<Principal> {
    Principal::from_text(text).map_err(|_| dmsg_types::invalid("principal"))
}

impl Home {
    /// `--home`, else `$DMSG_HOME`, else `~/.dmsg`.
    pub fn resolve(flag: Option<PathBuf>) -> Self {
        let dir = flag
            .or_else(|| std::env::var_os("DMSG_HOME").map(PathBuf::from))
            .unwrap_or_else(|| {
                std::env::var_os("HOME")
                    .or_else(|| std::env::var_os("USERPROFILE"))
                    .map(PathBuf::from)
                    .unwrap_or_default()
                    .join(".dmsg")
            });
        Self { dir }
    }

    /// Read and check `config.toml`.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` naming a missing or invalid setting.
    pub fn settings(&self) -> Result<Settings> {
        let path = self.dir.join("config.toml");
        let text = fs::read_to_string(&path)
            .map_err(|e| Error::InvalidInput(format!("{}: {e}", path.display())))?;
        let file: ConfigFile =
            toml::from_str(&text).map_err(|e| Error::InvalidInput(format!("config.toml: {e}")))?;
        let config = Config {
            environment: parse_environment(&file.environment)?,
            ic_host: file.ic_host,
            relay: file.relay,
            handle: principal(&file.handle)?,
            user_homes: file
                .user_homes
                .iter()
                .map(|t| principal(t))
                .collect::<Result<_>>()?,
            root_key: file
                .root_key
                .map(|k| hex::decode(k).map_err(|_| dmsg_types::invalid("root_key")))
                .transpose()?,
            login_url: file.login_url.unwrap_or_else(|| LOGIN_URL.into()),
            background_online: file.background_online,
        };
        config.validate()?;
        let idle = file.idle_lock_minutes.unwrap_or(15);
        if !(1..=24 * 60).contains(&idle) {
            return Err(dmsg_types::invalid("idle_lock_minutes"));
        }
        if file.sync_interval_minutes > 24 * 60 {
            return Err(dmsg_types::invalid("sync_interval_minutes"));
        }
        let storage = match file.secure_storage.as_deref() {
            Some("system") => Storage::System,
            Some("file") => Storage::File,
            None if cfg!(any(target_os = "macos", windows)) => Storage::System,
            None => Storage::File,
            Some(_) => return Err(dmsg_types::invalid("secure_storage")),
        };
        Ok(Settings {
            config,
            idle_lock_ms: idle * MINUTE,
            sync_interval_ms: file.sync_interval_minutes * MINUTE,
            storage,
        })
    }

    /// The credential store of these settings, if it is the system one.
    pub fn secure(settings: &Settings) -> Option<std::sync::Arc<dyn SecureStorage>> {
        match settings.storage {
            Storage::System => Some(std::sync::Arc::new(crate::secure::SystemStorage)),
            Storage::File => None,
        }
    }

    /// The workspace directory of an environment.
    pub fn workspace(&self, config: &Config) -> PathBuf {
        self.dir
            .join(dmsg_client::config::environment_name(&config.environment))
    }

    /// The owner credential, created on first use: in the system
    /// credential store, or a file only its owner can read.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when it cannot be read or written.
    pub fn owner_token(&self, settings: &Settings, create: bool) -> Result<String> {
        let existing = match settings.storage {
            Storage::System => crate::secure::SystemStorage
                .get(&self.owner_name()?)?
                .map(|t| String::from_utf8_lossy(&t).into_owned()),
            Storage::File => match fs::read_to_string(self.dir.join("owner.token")) {
                Ok(token) => Some(token.trim().to_string()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(io(e)),
            },
        };
        if let Some(token) = existing {
            return Ok(token);
        }
        if !create {
            return Err(Error::AuthRequired);
        }
        fs::create_dir_all(&self.dir).map_err(io)?;
        let token = hex::encode(dmsg_client::crypto::random_key());
        match settings.storage {
            Storage::System => {
                crate::secure::SystemStorage.set(&self.owner_name()?, token.as_bytes())?
            }
            Storage::File => write_private(&self.dir.join("owner.token"), token.as_bytes())?,
        }
        Ok(token)
    }

    /// The credential-store name of this home's owner credential.
    fn owner_name(&self) -> Result<String> {
        let dir = fs::canonicalize(&self.dir).map_err(io)?;
        Ok(format!("owner:{}", dir.display()))
    }

    /// The daemon's discovery file, if a daemon runs.
    pub fn discovery(&self) -> Option<Discovery> {
        let text = fs::read(self.dir.join("daemon.json")).ok()?;
        serde_json::from_slice(&text).ok()
    }

    /// Publish where the daemon listens.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the file cannot be written.
    pub fn publish(&self, discovery: &Discovery) -> Result<()> {
        let temporary = self.dir.join("daemon.json.tmp");
        write_private(
            &temporary,
            &serde_json::to_vec(discovery).expect("discovery serializes"),
        )?;
        fs::rename(temporary, self.dir.join("daemon.json")).map_err(io)
    }

    /// Remove the discovery file.
    pub fn unpublish(&self) {
        let _ = fs::remove_file(self.dir.join("daemon.json"));
    }

    /// Hold `dmsg-daemon.lock` for this process: one daemon per home.
    ///
    /// # Errors
    /// Returns `Error::Locked` while another daemon holds it.
    pub fn lock(&self) -> Result<fs::File> {
        fs::create_dir_all(&self.dir).map_err(io)?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.dir.join("dmsg-daemon.lock"))
            .map_err(io)?;
        file.try_lock().map_err(|_| Error::Locked)?;
        Ok(file)
    }
}

/// Write a file only its owner can read.
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = fs::OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path).map_err(io)?;
    file.write_all(bytes).map_err(io)?;
    file.sync_all().map_err(io)
}
