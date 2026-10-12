//! IC calls through `ic-agent` with Candid-typed helpers.
//!
//! Query failures mean the answer is unknown and map to `Unavailable`. An
//! update that may have reached the replica maps to `ExecutionUnknown`: the
//! caller reconciles by its operation ID and never resubmits blindly.

use crate::config::Config;
use candid::{utils::ArgumentEncoder, CandidType, Principal};
use dmsg_types::{Environment, Error, Result};
use ic_agent::{agent::RejectCode, identity::AnonymousIdentity, Agent, AgentError, Identity};
use serde::de::DeserializeOwned;
use std::sync::Arc;

/// Largest response body accepted from the IC.
const MAX_RESPONSE: usize = 4 * 1024 * 1024;

/// An agent bound to one identity.
#[derive(Clone)]
pub struct Ic {
    agent: Agent,
}

fn unavailable(error: AgentError) -> Error {
    Error::Unavailable(format!("ic: {error}"))
}

impl Ic {
    /// Build an agent for `identity`; a local deployment trusts the
    /// configured or fetched replica root key, every other environment the
    /// built-in mainnet key.
    ///
    /// # Errors
    /// Returns `Error::Unavailable` when the agent cannot be built or the
    /// local root key cannot be fetched.
    pub async fn connect(config: &Config, identity: Arc<dyn Identity>) -> Result<Self> {
        let agent = Agent::builder()
            .with_url(&config.ic_host)
            .with_arc_identity(identity)
            .with_verify_query_signatures(true)
            .with_max_response_body_size(MAX_RESPONSE)
            .build()
            .map_err(unavailable)?;
        if config.environment == Environment::Local {
            match &config.root_key {
                Some(key) => agent.set_root_key(key.clone()),
                None => agent.fetch_root_key().await.map_err(unavailable)?,
            }
        }
        Ok(Self { agent })
    }

    /// An agent without a login, for public queries.
    ///
    /// # Errors
    /// As [`Self::connect`].
    pub async fn anonymous(config: &Config) -> Result<Self> {
        Self::connect(config, Arc::new(AnonymousIdentity)).await
    }

    /// The caller's Principal.
    ///
    /// # Errors
    /// Returns `Error::AuthRequired` when the identity has none.
    pub fn principal(&self) -> Result<Principal> {
        self.agent.get_principal().map_err(|_| Error::AuthRequired)
    }

    /// The DER root key certificates are verified against.
    pub fn root_key(&self) -> Vec<u8> {
        self.agent.read_root_key()
    }

    /// A query with Candid arguments and reply.
    ///
    /// # Errors
    /// Transport failures and rejections return `Error::Unavailable`.
    pub async fn query<A: ArgumentEncoder, R: CandidType + DeserializeOwned>(
        &self,
        canister: Principal,
        method: &str,
        args: A,
    ) -> Result<R> {
        let reply = self
            .agent
            .query(&canister, method)
            .with_arg(encode(args)?)
            .call()
            .await
            .map_err(unavailable)?;
        decode(&reply)
    }

    /// An update with Candid arguments and reply.
    ///
    /// # Errors
    /// A definite rejection returns `Error::Unavailable`; any other failure
    /// returns `Error::ExecutionUnknown`.
    pub async fn update<A: ArgumentEncoder, R: CandidType + DeserializeOwned>(
        &self,
        canister: Principal,
        method: &str,
        args: A,
    ) -> Result<R> {
        let reply = self
            .agent
            .update(&canister, method)
            .with_arg(encode(args)?)
            .call_and_wait()
            .await
            .map_err(|error| match error {
                AgentError::CertifiedReject { ref reject, .. }
                    if reject.reject_code != RejectCode::SysTransient =>
                {
                    unavailable(error)
                }
                _ => Error::ExecutionUnknown,
            })?;
        decode(&reply)
    }
}

fn encode<A: ArgumentEncoder>(args: A) -> Result<Vec<u8>> {
    candid::encode_args(args).map_err(|e| Error::InvalidInput(format!("candid: {e}")))
}

fn decode<R: CandidType + DeserializeOwned>(reply: &[u8]) -> Result<R> {
    candid::decode_one(reply).map_err(|e| Error::Unavailable(format!("candid: {e}")))
}
