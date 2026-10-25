//! Deployment parameters of one environment. They come from the operator's
//! configuration file and stay fixed for the life of a client; no request,
//! tool or remote party can change them.

use candid::Principal;
use dmsg_types::{Environment, Result, ensure_valid};
use url::Url;

/// The dmsg-login/1 page.
pub const LOGIN_URL: &str = "https://dmsg.net/_/login";
/// The dmsg-connect/1 bridge origin.
pub const CONNECT_URL: &str = "https://connect.dmsg.net";
/// One production trust configuration, shared by all client hosts.
pub const PRODUCTION: &str = include_str!("../production.toml");
/// Largest file the file library takes by default: 4 GiB.
pub const DEFAULT_FILES_MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// Days a trashed library file is kept by default.
pub const DEFAULT_FILES_TRASH_DAYS: u64 = 30;

/// Public parameters of one deployment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Deployment domain; also part of every key derivation context.
    pub environment: Environment,
    /// IC API boundary, such as `https://icp-api.io`.
    pub ic_host: String,
    /// Origin of the dmsg-cloud relay: the device API, and also the Agent
    /// Delegation and Agent Profile service of dMsg principals.
    pub relay: String,
    /// Name registry, the authority on the deployment's user homes.
    pub handle: Principal,
    /// User homes pinned by the configuration; the registry may list more.
    pub user_homes: Vec<Principal>,
    /// DER root key of a local replica; mainnet roots are built in.
    pub root_key: Option<Vec<u8>>,
    /// Login page; only a local deployment may replace [`LOGIN_URL`].
    pub login_url: String,
    /// Bridge origin of web applications; only a local deployment may
    /// replace [`CONNECT_URL`].
    pub connect_url: String,
    /// Keep the device signing key while locked ("后台在线", plan D-7).
    pub background_online: bool,
    /// Largest file the file library takes, in bytes.
    pub files_max_bytes: u64,
    /// Days a trashed library file or folder is kept before it is deleted.
    pub files_trash_days: u64,
    /// The reviewed freeze of the old service, once there is one.
    pub legacy: Option<LegacyConfig>,
    /// Team wallets: the hub and the chain endpoints this client reads and
    /// broadcasts through.
    pub wallet: WalletSettings,
}

/// Team wallet settings. The chain endpoints only read and broadcast:
/// wallets sign on the IC, and the client checks what it reads against
/// what the wallet built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletSettings {
    /// The wallet hub; team wallets are off without one.
    pub hub: Option<Principal>,
    /// An Esplora API, such as `https://mempool.space/api`.
    pub bitcoin_api: String,
    /// The JSON-RPC endpoint of each EVM chain, by chain ID.
    pub evm_rpc: Vec<(u64, String)>,
    /// A Solana JSON-RPC endpoint.
    pub solana_rpc: String,
}

impl Default for WalletSettings {
    fn default() -> Self {
        Self {
            hub: None,
            bitcoin_api: "https://mempool.space/api".into(),
            evm_rpc: vec![
                (1, "https://ethereum-rpc.publicnode.com".into()),
                (8453, "https://mainnet.base.org".into()),
            ],
            solana_rpc: "https://api.mainnet-beta.solana.com".into(),
        }
    }
}

/// The freeze of the old service a deployment reviewed: frozen evidence of
/// any other canister or cutover is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyConfig {
    /// Old channel canisters.
    pub channels: Vec<String>,
    /// Old name-identity canister.
    pub identity: String,
    /// Old OSS buckets of channel attachments.
    pub buckets: Vec<String>,
    /// The reviewed cutover ID.
    pub cutover: [u8; 32],
}

impl Config {
    /// The compiled production deployment. Host policy stays outside URLs.
    ///
    /// # Errors
    /// Returns `InvalidInput` if the compiled configuration is malformed.
    pub fn production() -> Result<Self> {
        let file: toml::Value =
            toml::from_str(PRODUCTION).map_err(|_| dmsg_types::invalid("production config"))?;
        let value = |name: &str| file[name].as_str().ok_or_else(|| dmsg_types::invalid(name));
        let principal =
            |name: &str| Principal::from_text(value(name)?).map_err(|_| dmsg_types::invalid(name));
        let legacy = &file["legacy"];
        let cutover = hex::decode(
            legacy["cutover"]
                .as_str()
                .ok_or_else(|| dmsg_types::invalid("cutover"))?,
        )
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| dmsg_types::invalid("cutover"))?;
        let config = Self {
            environment: parse_environment(value("environment")?)?,
            ic_host: value("ic_host")?.into(),
            relay: value("relay")?.into(),
            handle: principal("handle")?,
            user_homes: vec![],
            root_key: None,
            login_url: LOGIN_URL.into(),
            connect_url: CONNECT_URL.into(),
            background_online: false,
            files_max_bytes: DEFAULT_FILES_MAX_BYTES,
            files_trash_days: DEFAULT_FILES_TRASH_DAYS,
            legacy: Some(LegacyConfig {
                channels: legacy["channels"]
                    .as_array()
                    .ok_or_else(|| dmsg_types::invalid("channels"))?
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(String::from)
                            .ok_or_else(|| dmsg_types::invalid("channel"))
                    })
                    .collect::<Result<_>>()?,
                identity: legacy["identity"]
                    .as_str()
                    .ok_or_else(|| dmsg_types::invalid("identity"))?
                    .into(),
                buckets: vec![],
                cutover,
            }),
            wallet: WalletSettings {
                hub: Some(
                    Principal::from_text(
                        file["wallet"]["hub"]
                            .as_str()
                            .ok_or_else(|| dmsg_types::invalid("wallet hub"))?,
                    )
                    .map_err(|_| dmsg_types::invalid("wallet hub"))?,
                ),
                ..WalletSettings::default()
            },
        };
        config.validate()?;
        Ok(config)
    }

    /// Check that every endpoint is HTTPS, or loopback HTTP in a local
    /// deployment, and that local-only settings stay local.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` naming the offending setting.
    pub fn validate(&self) -> Result<()> {
        let local = self.environment == Environment::Local;
        let endpoints = [
            ("ic_host", &self.ic_host),
            ("relay", &self.relay),
            ("login_url", &self.login_url),
            ("connect_url", &self.connect_url),
            ("wallet.bitcoin_api", &self.wallet.bitcoin_api),
            ("wallet.solana_rpc", &self.wallet.solana_rpc),
        ];
        let evm = self
            .wallet
            .evm_rpc
            .iter()
            .map(|(_, url)| ("wallet.evm_rpc", url));
        for (name, value) in endpoints.into_iter().chain(evm) {
            let url = Url::parse(value).map_err(|_| dmsg_types::invalid(name))?;
            let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
            ensure_valid(
                url.username().is_empty()
                    && url.password().is_none()
                    && (url.scheme() == "https" || (local && loopback && url.scheme() == "http")),
                name,
            )?;
        }
        ensure_valid(
            local
                || (self.root_key.is_none()
                    && self.login_url == LOGIN_URL
                    && self.connect_url == CONNECT_URL),
            "local-only setting",
        )?;
        ensure_valid(
            self.files_max_bytes >= crate::files::CHUNK as u64,
            "files_max_bytes",
        )?;
        ensure_valid(
            (1..=365).contains(&self.files_trash_days),
            "files_trash_days",
        )
    }

    /// The origin of the relay, without a trailing slash.
    pub fn relay_origin(&self) -> String {
        self.relay.trim_end_matches('/').to_string()
    }
}

/// The lowercase name of an environment, as contexts and file paths spell it.
pub fn environment_name(environment: &Environment) -> &'static str {
    match environment {
        Environment::Local => "local",
        Environment::Staging => "staging",
        Environment::Production => "production",
    }
}

/// Parse a lowercase environment name.
///
/// # Errors
/// Returns `Error::InvalidInput` for any other spelling.
pub fn parse_environment(name: &str) -> Result<Environment> {
    match name {
        "local" => Ok(Environment::Local),
        "staging" => Ok(Environment::Staging),
        "production" => Ok(Environment::Production),
        _ => Err(dmsg_types::invalid("environment")),
    }
}
