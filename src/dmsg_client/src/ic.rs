//! IC calls through `ic-agent` with Candid-typed helpers.
//!
//! Query failures mean the answer is unknown and map to `Unavailable`. An
//! update that may have reached the replica maps to `ExecutionUnknown`: the
//! caller reconciles by its operation ID and never resubmits blindly.

use crate::config::Config;
use candid::{CandidType, Principal, utils::ArgumentEncoder};
use dmsg_types::{Environment, Error, Result};
use futures_timer::Delay;
use futures_util::future::{Either, select};
use ic_agent::{Agent, AgentError, Identity, agent::RejectCode, identity::AnonymousIdentity};
use serde::de::DeserializeOwned;
use std::sync::Arc;
use std::time::Duration;

/// Largest response body accepted from the IC.
const MAX_RESPONSE: usize = 4 * 1024 * 1024;

/// An agent bound to one identity.
#[derive(Clone)]
pub struct Ic {
    agent: Agent,
}

fn unavailable(error: AgentError) -> Error {
    let cause = diagnostic(&error);
    cause.emit();
    if cause.reason == "certificate" {
        return Error::IntegrityFailed;
    }
    use ic_agent::agent::agent_error::TransportError;
    if matches!(&error, AgentError::TransportError(TransportError::Reqwest(e)) if crate::transport::transient(e))
        || matches!(&error, AgentError::HttpError(e) if crate::transport::transient_status(e.status))
        || matches!(&error, AgentError::TimeoutWaitingForResponse())
    {
        return crate::transport::unavailable("ic", diagnostic(&error).reason);
    }
    Error::Unavailable(format!("ic: {error}"))
}

fn diagnostic(error: &AgentError) -> crate::diagnostic::Diagnostic {
    use crate::diagnostic::Diagnostic;
    use ic_agent::agent::agent_error::TransportError;
    match error {
        AgentError::TransportError(TransportError::Reqwest(error)) => {
            Diagnostic::request("ic", error)
        }
        AgentError::HttpError(error) => Diagnostic::http("ic", error.status),
        AgentError::TimeoutWaitingForResponse() => Diagnostic::new("ic", "timeout"),
        AgentError::CertifiedReject { .. } | AgentError::UncertifiedReject { .. } => {
            Diagnostic::new("ic", "rejected")
        }
        AgentError::CertificateOutdated(_) => Diagnostic::new("ic", "certificate_stale"),
        AgentError::CertificateVerificationFailed()
        | AgentError::QuerySignatureVerificationFailed
        | AgentError::CertificateNotAuthorized()
        | AgentError::CertificateHasTooManyDelegations
        | AgentError::MissingSignature
        | AgentError::MalformedSignature
        | AgentError::MalformedPublicKey => Diagnostic::new("ic", "certificate"),
        AgentError::InvalidCborData(_)
        | AgentError::CandidError(_)
        | AgentError::InvalidHttpResponse(_) => Diagnostic::new("ic", "decode"),
        AgentError::ResponseSizeExceededLimit() => Diagnostic::new("ic", "response_too_large"),
        _ => Diagnostic::new("ic", "request"),
    }
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
            .with_arc_http_middleware(Arc::new(crate::transport::IcTransport(
                crate::transport::client().map_err(|_| Error::Unavailable("http client".into()))?,
            )))
            .with_max_polling_time(Duration::from_secs(60))
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
        let reply = bounded(
            self.agent
                .query(&canister, method)
                .with_arg(encode(args)?)
                .call(),
            Duration::from_secs(15),
        )
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
        let reply = bounded(
            self.agent
                .update(&canister, method)
                .with_arg(encode(args)?)
                .call_and_wait(),
            Duration::from_secs(90),
        )
        .await
        .map_err(|error| match error {
            AgentError::CertifiedReject { ref reject, .. }
                if reject.reject_code != RejectCode::SysTransient =>
            {
                unavailable(error)
            }
            _ => {
                diagnostic(&error).emit();
                Error::ExecutionUnknown
            }
        })?;
        decode(&reply)
    }
}

async fn bounded<T>(
    work: impl std::future::Future<Output = std::result::Result<T, AgentError>>,
    timeout: Duration,
) -> std::result::Result<T, AgentError> {
    match select(Box::pin(work), Box::pin(Delay::new(timeout))).await {
        Either::Left((result, _)) => result,
        Either::Right(_) => Err(AgentError::TimeoutWaitingForResponse()),
    }
}

fn encode<A: ArgumentEncoder>(args: A) -> Result<Vec<u8>> {
    candid::encode_args(args).map_err(|e| Error::InvalidInput(format!("candid: {e}")))
}

fn decode<R: CandidType + DeserializeOwned>(reply: &[u8]) -> Result<R> {
    candid::decode_one(reply).map_err(|e| {
        crate::diagnostic::Diagnostic::new("ic", "decode").emit();
        Error::Unavailable(format!("candid: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certificate_and_local_storage_failures_are_not_transient_network_errors() {
        assert_eq!(
            unavailable(AgentError::CertificateVerificationFailed()),
            Error::IntegrityFailed
        );
        assert_eq!(
            unavailable(AgentError::QuerySignatureVerificationFailed),
            Error::IntegrityFailed
        );
        assert_eq!(
            crate::error_code(&crate::storage(std::io::Error::from(
                std::io::ErrorKind::StorageFull
            ))),
            "LOCAL_STORAGE_UNAVAILABLE"
        );
        assert_eq!(
            crate::error_code(&unavailable(AgentError::TimeoutWaitingForResponse())),
            "NETWORK_UNAVAILABLE"
        );
    }
}
