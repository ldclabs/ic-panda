//! Deployment parameters of one environment. They come from the operator's
//! configuration file and stay fixed for the life of a client; no request,
//! tool or remote party can change them.

use candid::Principal;
use dmsg_types::{ensure_valid, Environment, Result};
use url::Url;

/// The dmsg-login/1 page.
pub const LOGIN_URL: &str = "https://dmsg.net/_/login";

/// Public parameters of one deployment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Deployment domain; also part of every key derivation context.
    pub environment: Environment,
    /// IC API boundary, such as `https://icp-api.io`.
    pub ic_host: String,
    /// Origin of the dmsg-cloud device API.
    pub relay: String,
    /// Name registry, the authority on the deployment's user homes.
    pub handle: Principal,
    /// User homes pinned by the configuration; the registry may list more.
    pub user_homes: Vec<Principal>,
    /// DER root key of a local replica; mainnet roots are built in.
    pub root_key: Option<Vec<u8>>,
    /// Login page; only a local deployment may replace [`LOGIN_URL`].
    pub login_url: String,
}

impl Config {
    /// Check that every endpoint is HTTPS, or loopback HTTP in a local
    /// deployment, and that local-only settings stay local.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` naming the offending setting.
    pub fn validate(&self) -> Result<()> {
        let local = self.environment == Environment::Local;
        for (name, value) in [
            ("ic_host", &self.ic_host),
            ("relay", &self.relay),
            ("login_url", &self.login_url),
        ] {
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
            local || (self.root_key.is_none() && self.login_url == LOGIN_URL),
            "local-only setting",
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
