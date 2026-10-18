//! dmsg-login/1 on the client side (`docs/protocol/login-v1_zh.md`): a
//! session key held in memory, the login page URL, and the callback check.
//!
//! Internet Identity signs an unscoped delegation to the session key; the
//! session key then delegates to a fresh key limited to the login targets,
//! and only that fresh key is kept, as a [`KeptLogin`] the client may seal
//! to restore the login after a fast unlock with the person present. The
//! delegation's signatures are judged by the IC: the caller hands the login
//! to [`crate::Client::login`], whose read-only `my_account` queries discard
//! it on any failure.

use crate::codec::{b64, canonical, decode};
use crate::config::Config;
use crate::crypto::{Key, random_id, random_key, signing_public};
use candid::Principal;
use dmsg_protocol::login::{ED25519_SPKI_PREFIX, validate_request};
use dmsg_types::{
    DAY, Error, Hash, MINUTE, Result, ensure, ensure_valid,
    login::{LOGIN_VERSION, LoginCallback, LoginErrorCode, LoginRequest},
};
use ic_agent::Identity;
use ic_agent::identity::{BasicIdentity, DelegatedIdentity, Delegation, SignedDelegation};
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, ByteBuf};
use std::sync::Arc;
use zeroize::Zeroizing;

/// How long the callback listener lives.
pub const LISTEN_MS: u64 = 10 * MINUTE;
/// The delegation lifetime a login asks for (30 days, as long as fast unlock
/// lasts after it).
pub const DEFAULT_TTL_MS: u64 = 30 * DAY;
/// Most canisters a login delegation may reach.
pub const MAX_TARGETS: usize = 32;
const MAX_CALLBACK: usize = 64 * 1024;
/// How far this clock may run behind the IC's, which Internet Identity
/// dates the delegation's expiration by.
const CLOCK_SKEW_MS: u64 = MINUTE;
const MAX_CHAIN: usize = 8;

/// One pending login.
pub struct LoginSession {
    request: LoginRequest,
    targets: Vec<Principal>,
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
    /// What restores this login.
    pub kept: KeptLogin,
}

/// A login's scoped key and chain, which the client seals under the local
/// data key to restore the login until it expires.
#[derive(Serialize, Deserialize, Clone)]
pub struct KeptLogin {
    /// The chain's first public key, which names the login Principal.
    from: ByteBuf,
    /// The scoped key's Ed25519 seed.
    key: ByteArray<32>,
    /// The chain as signed, the scoped link last.
    links: Vec<KeptLink>,
    /// When the delegation expires, in Unix milliseconds.
    pub expires_at: u64,
}

#[derive(Serialize, Deserialize, Clone)]
struct KeptLink {
    pubkey: ByteBuf,
    /// Nanoseconds since the Unix epoch.
    expiration: u64,
    targets: Option<Vec<ByteBuf>>,
    signature: ByteBuf,
}

impl KeptLogin {
    fn new(from: Vec<u8>, key: &Key, links: &[SignedDelegation], expires_at: u64) -> Self {
        Self {
            from: from.into(),
            key: ByteArray::new(*key),
            links: links
                .iter()
                .map(|link| KeptLink {
                    pubkey: link.delegation.pubkey.clone().into(),
                    expiration: link.delegation.expiration,
                    targets: link
                        .delegation
                        .targets
                        .as_ref()
                        .map(|t| t.iter().map(|p| p.as_slice().to_vec().into()).collect()),
                    signature: link.signature.clone().into(),
                })
                .collect(),
            expires_at,
        }
    }

    /// The login again, as the callback delivered it.
    ///
    /// # Errors
    /// Returns `Error::IntegrityFailed` for a malformed record.
    pub fn delegated(&self) -> Result<Delegated> {
        let links = self
            .links
            .iter()
            .map(|link| {
                let targets = link
                    .targets
                    .as_ref()
                    .map(|targets| {
                        targets
                            .iter()
                            .map(|t| {
                                Principal::try_from_slice(t).map_err(|_| Error::IntegrityFailed)
                            })
                            .collect::<Result<Vec<_>>>()
                    })
                    .transpose()?;
                Ok(SignedDelegation {
                    delegation: Delegation {
                        pubkey: link.pubkey.to_vec(),
                        expiration: link.expiration,
                        targets,
                        permissions: None,
                    },
                    signature: link.signature.to_vec(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        ensure(!links.is_empty(), Error::IntegrityFailed)?;
        let key = BasicIdentity::from_raw_key(&Zeroizing::new(*self.key));
        Ok(Delegated {
            identity: Arc::new(DelegatedIdentity::new_unchecked(
                self.from.to_vec(),
                Box::new(key),
                links,
            )),
            principal: Principal::self_authenticating(&self.from),
            expires_at: self.expires_at,
            kept: self.clone(),
        })
    }
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

/// `permissions` is refused as an unknown field.
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
    /// Start a login whose delegation reaches only `targets` and whose
    /// callback is `http://127.0.0.1:<port>/auth/callback`.
    ///
    /// # Errors
    /// Returns `Error::InvalidInput` for an empty or oversized target set,
    /// or an origin or lifetime the login page would refuse.
    pub fn new(
        targets: &[Principal],
        derivation_origin: &str,
        port: u16,
        ttl_ms: u64,
        now: u64,
    ) -> Result<Self> {
        ensure_valid(
            !targets.is_empty() && targets.len() <= MAX_TARGETS,
            "targets",
        )?;
        let seed = Zeroizing::new(random_key());
        let mut session_pubkey = ED25519_SPKI_PREFIX.to_vec();
        session_pubkey.extend(signing_public(&seed));
        let request = LoginRequest {
            v: LOGIN_VERSION,
            state: Hash::new(random_id()),
            session_pubkey: session_pubkey.into(),
            ttl_ms,
            derivation_origin: derivation_origin.into(),
            next_url: format!("http://127.0.0.1:{port}/auth/callback"),
        };
        validate_request(&request)?;
        Ok(Self {
            request,
            targets: targets.to_vec(),
            seed,
            deadline: now + LISTEN_MS,
        })
    }

    /// The login page URL with the request in its fragment.
    pub fn url(&self, config: &Config) -> String {
        format!("{}#{}", config.login_url, b64(&canonical(&self.request)))
    }

    /// Check a callback body against this request, then limit the chain to
    /// the targets by a last delegation from the session key to a fresh key.
    ///
    /// # Errors
    /// A foreign state, a delegation to another key, a restricted
    /// `permissions` or a lifetime beyond the request is refused. A login
    /// page error comes back as `Error::Unavailable` naming its code.
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
        let mut expiration = u64::MAX;
        let mut signed = Vec::with_capacity(chain.delegations.len() + 1);
        for link in chain.delegations {
            let link_expiration = u64::from_str_radix(&link.delegation.expiration, 16)
                .map_err(|_| Error::IntegrityFailed)?;
            // Kept exactly as signed; the last link narrows the chain anyway.
            let targets = link
                .delegation
                .targets
                .map(|targets| {
                    targets
                        .iter()
                        .map(|t| {
                            Principal::try_from_slice(&unhex(t)?)
                                .map_err(|_| Error::IntegrityFailed)
                        })
                        .collect::<Result<Vec<_>>>()
                })
                .transpose()?;
            expiration = expiration.min(link_expiration);
            signed.push(SignedDelegation {
                delegation: Delegation {
                    pubkey: unhex(&link.delegation.pubkey)?,
                    expiration: link_expiration,
                    targets,
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
        let expires_at = expiration / 1_000_000;
        ensure(
            expires_at > now && expires_at <= now + self.request.ttl_ms + CLOCK_SKEW_MS,
            Error::Expired,
        )?;
        // Internet Identity ignores targets, so the session key, which
        // drops with this login, scopes the chain for the key that stays.
        let seed = Zeroizing::new(random_key());
        let key = BasicIdentity::from_raw_key(&seed);
        let scoped = Delegation {
            pubkey: key.public_key().ok_or(Error::IntegrityFailed)?,
            expiration,
            targets: Some(self.targets.clone()),
            permissions: None,
        };
        let signature = BasicIdentity::from_raw_key(&self.seed)
            .sign_delegation(&scoped)
            .ok()
            .and_then(|s| s.signature)
            .ok_or(Error::IntegrityFailed)?;
        signed.push(SignedDelegation {
            delegation: scoped,
            signature,
        });
        KeptLogin::new(unhex(&chain.public_key)?, &seed, &signed, expires_at).delegated()
    }
}

/// A login of the key `[seed; 32]`, delegated to a scoped key, for tests.
#[cfg(test)]
pub(crate) fn kept_for_tests(seed: u8, expires_at: u64) -> KeptLogin {
    let root = BasicIdentity::from_raw_key(&[seed; 32]);
    let scoped = [seed.wrapping_add(1); 32];
    let delegation = Delegation {
        pubkey: BasicIdentity::from_raw_key(&scoped).public_key().unwrap(),
        expiration: expires_at * 1_000_000,
        targets: None,
        permissions: None,
    };
    let signature = root
        .sign_delegation(&delegation)
        .unwrap()
        .signature
        .unwrap();
    KeptLogin::new(
        root.public_key().unwrap(),
        &scoped,
        &[SignedDelegation {
            delegation,
            signature,
        }],
        expires_at,
    )
}

#[cfg(test)]
#[path = "login_tests.rs"]
mod tests;
