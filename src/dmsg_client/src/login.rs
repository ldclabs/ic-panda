//! dmsg-login/1 on the client side (`docs/protocol/login-v1_zh.md`): a
//! session key held in memory, the login page URL, and the callback check.
//!
//! The delegation's signatures are judged by the IC: the caller hands the
//! identity to [`crate::Client::login`], whose read-only `my_account` queries
//! discard it on any failure.

use crate::codec::{b64, canonical, decode};
use crate::config::Config;
use crate::crypto::{random_id, random_key, signing_public, Key};
use candid::Principal;
use dmsg_protocol::login::{validate_request, ED25519_SPKI_PREFIX};
use dmsg_types::{
    ensure,
    login::{LoginCallback, LoginErrorCode, LoginRequest, LOGIN_VERSION},
    Error, Hash, Result, MINUTE,
};
use ic_agent::identity::{BasicIdentity, DelegatedIdentity, Delegation, SignedDelegation};
use ic_agent::Identity;
use serde::Deserialize;
use std::sync::Arc;
use zeroize::Zeroizing;

/// How long the callback listener lives.
pub const LISTEN_MS: u64 = 10 * MINUTE;
/// Default delegation lifetime (15 minutes).
pub const DEFAULT_TTL_MS: u64 = 15 * MINUTE;
const MAX_CALLBACK: usize = 64 * 1024;
const MAX_CHAIN: usize = 8;

/// One pending login.
pub struct LoginSession {
    request: LoginRequest,
    seed: Zeroizing<Key>,
    /// When the callback listener closes, in Unix milliseconds.
    pub deadline: u64,
}

/// A login the callback delivered.
pub struct Delegated {
    /// The session key acting for the login Principal.
    pub identity: Arc<dyn Identity>,
    /// The login Principal.
    pub principal: Principal,
    /// When the delegation expires, in Unix milliseconds.
    pub expires_at: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ChainJson {
    delegations: Vec<SignedJson>,
    public_key: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedJson {
    delegation: DelegationJson,
    signature: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DelegationJson {
    expiration: String,
    pubkey: String,
    targets: Option<Vec<String>>,
}

fn unhex(text: &str) -> Result<Vec<u8>> {
    hex::decode(text).map_err(|_| Error::IntegrityFailed)
}

impl LoginSession {
    /// Start a login whose callback is `http://127.0.0.1:<port>/auth/callback`.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for an origin, target set or lifetime
    /// the login page would refuse.
    pub fn new(
        targets: &[Principal],
        derivation_origin: &str,
        port: u16,
        ttl_ms: u64,
        now: u64,
    ) -> Result<Self> {
        let seed = Zeroizing::new(random_key());
        let mut session_pubkey = ED25519_SPKI_PREFIX.to_vec();
        session_pubkey.extend(signing_public(&seed));
        let request = LoginRequest {
            v: LOGIN_VERSION,
            state: Hash::new(random_id()),
            session_pubkey: session_pubkey.into(),
            targets: targets.iter().map(Principal::to_text).collect(),
            ttl_ms,
            derivation_origin: derivation_origin.into(),
            next_url: format!("http://127.0.0.1:{port}/auth/callback"),
        };
        validate_request(&request)?;
        Ok(Self {
            request,
            seed,
            deadline: now + LISTEN_MS,
        })
    }

    /// The login page URL with the request in its fragment.
    pub fn url(&self, config: &Config) -> String {
        format!("{}#{}", config.login_url, b64(&canonical(&self.request)))
    }

    /// Check a callback body against this request.
    ///
    /// # Errors
    /// A foreign state, a delegation to another key, a target outside the
    /// request or a lifetime beyond it is refused. A login page error comes
    /// back as `Error::AuthRequired` naming its code.
    pub fn accept(&self, body: &[u8], now: u64) -> Result<Delegated> {
        ensure(
            !body.is_empty() && body.len() <= MAX_CALLBACK,
            Error::QuotaExceeded,
        )?;
        let callback: LoginCallback = decode(body, MAX_CALLBACK)?;
        let result = match callback {
            LoginCallback::Error(error) => {
                ensure(
                    error.v == LOGIN_VERSION && error.state == self.request.state,
                    Error::IntegrityFailed,
                )?;
                let code = match error.error {
                    LoginErrorCode::UserCancelled => "user_cancelled",
                    LoginErrorCode::IiError => "ii_error",
                    LoginErrorCode::DerivationOriginNotAllowed => "derivation_origin_not_allowed",
                    LoginErrorCode::Timeout => "timeout",
                };
                return Err(Error::Unavailable(format!("login: {code}")));
            }
            LoginCallback::Result(result) => result,
        };
        ensure(
            result.v == LOGIN_VERSION && result.state == self.request.state,
            Error::IntegrityFailed,
        )?;
        let chain: ChainJson =
            serde_json::from_slice(&result.delegation).map_err(|_| Error::IntegrityFailed)?;
        ensure(
            !chain.delegations.is_empty() && chain.delegations.len() <= MAX_CHAIN,
            Error::IntegrityFailed,
        )?;
        let targets: Vec<Principal> = self
            .request
            .targets
            .iter()
            .map(|t| Principal::from_text(t).map_err(|_| Error::IntegrityFailed))
            .collect::<Result<_>>()?;
        let mut expires_at = u64::MAX;
        let mut signed = Vec::with_capacity(chain.delegations.len());
        for link in chain.delegations {
            let expiration = u64::from_str_radix(&link.delegation.expiration, 16)
                .map_err(|_| Error::IntegrityFailed)?;
            let link_targets = link
                .delegation
                .targets
                .ok_or(Error::Forbidden)?
                .iter()
                .map(|t| Principal::try_from_slice(&unhex(t)?).map_err(|_| Error::IntegrityFailed))
                .collect::<Result<Vec<_>>>()?;
            ensure(
                link_targets.iter().all(|t| targets.contains(t)),
                Error::Forbidden,
            )?;
            expires_at = expires_at.min(expiration / 1_000_000);
            signed.push(SignedDelegation {
                delegation: Delegation {
                    pubkey: unhex(&link.delegation.pubkey)?,
                    expiration,
                    targets: Some(link_targets),
                    permissions: None,
                },
                signature: unhex(&link.signature)?,
            });
        }
        ensure(
            signed.last().map(|s| s.delegation.pubkey.as_slice())
                == Some(self.request.session_pubkey.as_slice()),
            Error::IntegrityFailed,
        )?;
        ensure(
            expires_at > now && expires_at <= now + self.request.ttl_ms,
            Error::Expired,
        )?;
        let from = unhex(&chain.public_key)?;
        let principal = Principal::self_authenticating(&from);
        let session = BasicIdentity::from_raw_key(&self.seed);
        Ok(Delegated {
            identity: Arc::new(DelegatedIdentity::new_unchecked(
                from,
                Box::new(session),
                signed,
            )),
            principal,
            expires_at,
        })
    }
}

#[cfg(test)]
#[path = "login_tests.rs"]
mod tests;
