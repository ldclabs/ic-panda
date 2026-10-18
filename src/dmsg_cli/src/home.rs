//! The dMsg home directory: `config.toml`, the owner credential, the daemon
//! lock and discovery file, and one workspace directory per environment.
//!
//! Security settings live only in `config.toml`, or without one in the
//! production configuration built into this binary (`production.toml`);
//! nothing a running daemon receives can change them. The desktop's file
//! settings live beside it in `preferences.json`, which only the desktop's
//! main process writes, allowing the disk mount after the person confirms
//! in a system dialog; the daemon reads that file and never takes the
//! values from a request. Secrets that open the
//! workspace live in the system credential store (`secure_storage =
//! "system"`), never in the home; with `"file"` the owner credential is a
//! file only its owner can read and fast unlock is unavailable.

use candid::Principal;
use dmsg_client::config::{
    CONNECT_URL, Config, LOGIN_URL, LegacyConfig, WalletSettings, parse_environment,
};
use dmsg_client::secure::SecureStorage;
use dmsg_types::{Error, MINUTE, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The production deployment, used when the home has no `config.toml`.
const PRODUCTION: &str = include_str!("../production.toml");

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
    connect_url: Option<String>,
    #[serde(default)]
    agent_hosts: Vec<dmsg_agent::Host>,
    #[serde(default)]
    agent_blocked_senders: Vec<String>,
    agent_ca: Option<PathBuf>,
    #[serde(default)]
    background_online: bool,
    idle_lock_minutes: Option<u64>,
    #[serde(default)]
    sync_interval_minutes: u64,
    files_max_bytes: Option<u64>,
    files_trash_days: Option<u64>,
    #[serde(default)]
    files_mount: bool,
    secure_storage: Option<String>,
    legacy: Option<LegacyFile>,
    wallet: Option<WalletFile>,
}

/// `[wallet]`: the team wallet hub and chain endpoints.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WalletFile {
    hub: Option<String>,
    bitcoin_api: Option<String>,
    #[serde(default)]
    evm_rpc: std::collections::BTreeMap<String, String>,
    solana_rpc: Option<String>,
}

fn wallet(file: Option<WalletFile>) -> Result<WalletSettings> {
    let mut settings = WalletSettings::default();
    let Some(file) = file else {
        return Ok(settings);
    };
    settings.hub = file.hub.as_deref().map(principal).transpose()?;
    if let Some(api) = file.bitcoin_api {
        settings.bitcoin_api = api;
    }
    if let Some(rpc) = file.solana_rpc {
        settings.solana_rpc = rpc;
    }
    if !file.evm_rpc.is_empty() {
        settings.evm_rpc = file
            .evm_rpc
            .into_iter()
            .map(|(chain, url)| {
                chain
                    .parse()
                    .map(|chain| (chain, url))
                    .map_err(|_| dmsg_types::invalid("wallet.evm_rpc"))
            })
            .collect::<Result<_>>()?;
    }
    Ok(settings)
}

/// `[legacy]`: the old service's freeze this deployment reviewed.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyFile {
    channels: Vec<String>,
    identity: String,
    #[serde(default)]
    buckets: Vec<String>,
    cutover: String,
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
    /// Whether these are the built-in production settings: no `config.toml`.
    pub built_in: bool,
    /// The deployment.
    pub config: Config,
    /// Inactivity before the daemon locks.
    pub idle_lock_ms: u64,
    /// Interval of the background sync; 0 turns it off.
    pub sync_interval_ms: u64,
    /// Where secrets live.
    pub storage: Storage,
    /// The Agent Protocols hosts the hosted agents may sign for; nothing
    /// else can add one (plan DT02).
    pub agent_hosts: Vec<dmsg_agent::Host>,
    /// Agent IDs whose mail the hosted agents delete unopened.
    pub agent_blocked_senders: Vec<String>,
    /// A PEM file of the CA that replaces the system roots for the agent
    /// hosts; a local deployment only.
    pub agent_ca: Option<PathBuf>,
    /// Whether the file library may be mounted as a disk (`files/mount`);
    /// off by default, since the systems' WebDAV clients cache plaintext
    /// and bound file sizes. `preferences.json` may allow it as well.
    pub files_mount: bool,
    /// `preferences.json` of this home.
    pub preferences: PathBuf,
}

/// `preferences.json`: the file settings the desktop's main process
/// writes, read at start and when the daemon is told to read them again.
#[derive(Deserialize, Default, Clone, Debug, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    /// Allows `files/mount` beside `config.toml`'s `files_mount`.
    pub files_mount: bool,
    /// Days trashed files stay, instead of `config.toml`'s.
    pub files_trash_days: Option<u64>,
}

impl Preferences {
    /// The preferences at `path`; none when there is no file.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for an unreadable or invalid file.
    pub fn read(path: &Path) -> Result<Self> {
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| Error::InvalidInput(format!("preferences.json: {e}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(Error::InvalidInput(format!("{}: {e}", path.display()))),
        }
    }
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

fn legacy(file: LegacyFile) -> Result<LegacyConfig> {
    let canonical = |t: &String| principal(t).is_ok_and(|p| p.to_text() == *t);
    if file.channels.is_empty()
        || !file.channels.iter().all(canonical)
        || !file.buckets.iter().all(canonical)
        || !canonical(&file.identity)
    {
        return Err(dmsg_types::invalid("legacy"));
    }
    let cutover = hex::decode(&file.cutover)
        .ok()
        .and_then(|c| c.try_into().ok())
        .ok_or_else(|| dmsg_types::invalid("legacy.cutover"))?;
    Ok(LegacyConfig {
        channels: file.channels,
        identity: file.identity,
        buckets: file.buckets,
        cutover,
    })
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

    /// Read and check `config.toml`, or the built-in production settings
    /// when the home has none.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` naming an unreadable file or a missing
    /// or invalid setting.
    pub fn settings(&self) -> Result<Settings> {
        let path = self.dir.join("config.toml");
        let (text, built_in) = match fs::read_to_string(&path) {
            Ok(text) => (text, false),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (PRODUCTION.to_string(), true),
            Err(e) => return Err(Error::InvalidInput(format!("{}: {e}", path.display()))),
        };
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
            connect_url: file.connect_url.unwrap_or_else(|| CONNECT_URL.into()),
            background_online: file.background_online,
            files_max_bytes: file
                .files_max_bytes
                .unwrap_or(dmsg_client::config::DEFAULT_FILES_MAX_BYTES),
            files_trash_days: file
                .files_trash_days
                .unwrap_or(dmsg_client::config::DEFAULT_FILES_TRASH_DAYS),
            legacy: file.legacy.map(legacy).transpose()?,
            wallet: wallet(file.wallet)?,
        };
        config.validate()?;
        let idle = file.idle_lock_minutes.unwrap_or(15);
        if !(1..=24 * 60).contains(&idle) {
            return Err(dmsg_types::invalid("idle_lock_minutes"));
        }
        if file.sync_interval_minutes > 24 * 60 {
            return Err(dmsg_types::invalid("sync_interval_minutes"));
        }
        if file.agent_ca.is_some() && config.environment != dmsg_types::Environment::Local {
            return Err(dmsg_types::invalid("agent_ca"));
        }
        let storage = match file.secure_storage.as_deref() {
            Some("system") => Storage::System,
            Some("file") => Storage::File,
            None if cfg!(any(target_os = "macos", windows)) => Storage::System,
            None => Storage::File,
            Some(_) => return Err(dmsg_types::invalid("secure_storage")),
        };
        Ok(Settings {
            built_in,
            config,
            idle_lock_ms: idle * MINUTE,
            sync_interval_ms: file.sync_interval_minutes * MINUTE,
            storage,
            agent_hosts: file.agent_hosts,
            agent_blocked_senders: file.agent_blocked_senders,
            agent_ca: file.agent_ca,
            files_mount: file.files_mount,
            preferences: self.dir.join("preferences.json"),
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
            // A home that does not exist yet holds no credential.
            Storage::System if !self.dir.exists() => None,
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

    /// Hold `dmsg-daemon.lock` for this process: one daemon per home. The
    /// home becomes its owner's only: the workspaces' metadata and the
    /// hosted agents' room records and letters are not encrypted.
    ///
    /// # Errors
    /// Returns `Error::Locked` while another daemon holds it.
    pub fn lock(&self) -> Result<fs::File> {
        fs::create_dir_all(&self.dir).map_err(io)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.dir, fs::Permissions::from_mode(0o700)).map_err(io)?;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_production_without_config() {
        let dir = std::env::temp_dir().join(format!("dmsg-config-{}", std::process::id()));
        let home = Home { dir: dir.clone() };
        let settings = home.settings().unwrap();
        assert!(settings.built_in);
        // A fresh home: no owner credential yet, and no daemon to reach.
        assert!(matches!(
            home.owner_token(&settings, false),
            Err(Error::AuthRequired)
        ));
        assert_eq!(
            crate::remote::Remote::connect(&home).err().unwrap().code,
            "DAEMON_STOPPED"
        );
        assert_eq!(
            settings.config.environment,
            dmsg_types::Environment::Production
        );
        assert_eq!(settings.config.relay, "https://api.dmsg.net");
        assert_eq!(
            settings.config.handle.to_text(),
            "wfxrt-2iaaa-aaaaj-qsjwa-cai"
        );
        assert_eq!(settings.config.login_url, LOGIN_URL);
        let legacy = settings.config.legacy.as_ref().unwrap();
        assert_eq!(legacy.channels.len(), 3);
        assert_eq!(legacy.identity, "2rgax-kyaaa-aaaap-anvba-cai");
        assert!(legacy.buckets.is_empty());
        assert_eq!(
            hex::encode(legacy.cutover),
            "4d8f8b5ba5814440efd03dd3170aa14ba2873c40496f4a916790cd0befb38b2a"
        );
        assert_eq!(settings.agent_hosts.len(), 4);
        // The relay is the account principal's delegation and profile service,
        // which the connector reaches only as an allowed host.
        assert!(
            settings
                .agent_hosts
                .iter()
                .any(|h| h.host == settings.config.relay)
        );
        dmsg_agent::Runtime::new(&dmsg_agent::Options {
            hosts: settings.agent_hosts.clone(),
            ..dmsg_agent::Options::default()
        })
        .unwrap();
        // No preferences.json reads as none; the desktop writes one.
        assert_eq!(
            Preferences::read(&settings.preferences).unwrap(),
            Preferences::default()
        );
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            &settings.preferences,
            r#"{"files_mount": true, "files_trash_days": 7}"#,
        )
        .unwrap();
        assert_eq!(
            Preferences::read(&settings.preferences).unwrap(),
            Preferences {
                files_mount: true,
                files_trash_days: Some(7)
            }
        );
        fs::write(&settings.preferences, r#"{"relay": "x"}"#).unwrap();
        assert!(Preferences::read(&settings.preferences).is_err());
        fs::remove_file(&settings.preferences).unwrap();
        // A config.toml replaces the built-in settings whole.
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.toml"), "environment = \"production\"\n").unwrap();
        assert!(matches!(home.settings(), Err(Error::InvalidInput(_))));
        fs::remove_dir_all(dir).unwrap();
    }
}
