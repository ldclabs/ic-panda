use cbor2::{from_slice, to_vec};
use ic_auth_types::ByteBufB64;
use ic_http_certification::utils::skip_certification_certified_data;
use ic_stable_structures::{
    DefaultMemoryImpl, StableCell, Storable,
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, cell::RefCell, collections::HashMap};

type Memory = VirtualMemory<DefaultMemoryImpl>;

pub const MAX_PUBLIC_KEY_BYTES: usize = 48;
pub const MAX_SIGNED_DELEGATION_BYTES: usize = 16 * 1_024;

const RELAY_TTL_MS: u64 = 60 * 1_000;
// Together with MAX_SIGNED_DELEGATION_BYTES, this bounds the relayed payloads to 32 MiB.
const MAX_CACHED_DELEGATIONS: usize = 2_048;
const MAX_ALLOWED_ORIGINS: usize = 256;
const MAX_ORIGIN_BYTES: usize = 2_048;

/// The fields of a sign-in response that the relay reads.
///
/// The relay does not verify the delegation chain. The app that polls for the response
/// verifies it against its own session key, and anyone can sign a valid chain for any
/// session key with a key of their own, so a check here would only cost cycles.
#[derive(Deserialize)]
struct SignInResponse {
    #[serde(rename = "d")]
    delegations: Vec<SignedDelegation>,
}

#[derive(Deserialize)]
struct SignedDelegation {
    #[serde(rename = "d", alias = "delegation")]
    delegation: Delegation,
}

#[derive(Deserialize)]
struct Delegation {
    #[serde(rename = "p", alias = "pubkey")]
    pubkey: ByteBufB64,
    #[serde(rename = "e", alias = "expiration")]
    expiration: u64,
}

#[derive(Default)]
struct State {
    delegations: HashMap<Vec<u8>, StoredDelegation>,
}

struct StoredDelegation {
    payload: Vec<u8>,
    expires_at: u64,
}

impl State {
    fn with_delegation<R>(
        &self,
        pubkey: &[u8],
        now_ms: u64,
        f: impl FnOnce(&[u8]) -> R,
    ) -> Option<R> {
        let stored = self.delegations.get(pubkey)?;
        (stored.expires_at > now_ms).then(|| f(&stored.payload))
    }

    fn put_delegation(
        &mut self,
        signed_delegation: Vec<u8>,
        now_ms: u64,
    ) -> Result<Vec<u8>, String> {
        if signed_delegation.is_empty() || signed_delegation.len() > MAX_SIGNED_DELEGATION_BYTES {
            return Err("invalid signed delegation length".to_string());
        }

        let mut response: SignInResponse = from_slice(&signed_delegation)
            .map_err(|err| format!("failed to decode signed delegation: {err}"))?;
        let now_ns = now_ms * 1_000_000;
        if response
            .delegations
            .iter()
            .any(|link| link.delegation.expiration <= now_ns)
        {
            return Err("delegation has expired".to_string());
        }
        let session_pubkey = response
            .delegations
            .pop()
            .ok_or_else(|| "no delegation found".to_string())?
            .delegation
            .pubkey
            .into_vec();
        if session_pubkey.is_empty() || session_pubkey.len() > MAX_PUBLIC_KEY_BYTES {
            return Err("invalid session public key length".to_string());
        }

        // The store holds at most MAX_CACHED_DELEGATIONS short-lived entries, so a full scan
        // is cheaper than keeping an expiration index up to date.
        self.delegations
            .retain(|_, stored| stored.expires_at > now_ms);

        if let Some(stored) = self.delegations.get(&session_pubkey) {
            // HTTP gateways and clients may retry an update. Every sign-in uses a fresh
            // session key, so a different payload for a live key is never a retry.
            return if stored.payload == signed_delegation {
                Ok(session_pubkey)
            } else {
                Err("a delegation is already stored for this session public key".to_string())
            };
        }
        if self.delegations.len() >= MAX_CACHED_DELEGATIONS {
            return Err("delegation store is at capacity".to_string());
        }

        self.delegations.insert(
            session_pubkey.clone(),
            StoredDelegation {
                payload: signed_delegation,
                expires_at: now_ms + RELAY_TTL_MS,
            },
        );
        Ok(session_pubkey)
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
struct Config {
    allowed_origins: Vec<String>,
}

impl Config {
    fn new(allowed_origins: Vec<String>) -> Result<Self, String> {
        if allowed_origins.len() > MAX_ALLOWED_ORIGINS {
            return Err(format!(
                "too many allowed origins: {} (maximum {MAX_ALLOWED_ORIGINS})",
                allowed_origins.len()
            ));
        }

        let mut allowed_origins = allowed_origins
            .into_iter()
            .map(|origin| {
                if origin.len() > MAX_ORIGIN_BYTES {
                    return Err(format!(
                        "allowed origin is too long: {} bytes",
                        origin.len()
                    ));
                }
                let mut normalized = normalize_origin(&origin).to_string();
                normalized.make_ascii_lowercase();
                Ok(normalized)
            })
            .collect::<Result<Vec<_>, String>>()?;
        allowed_origins.sort_unstable();
        allowed_origins.dedup();

        Ok(Self { allowed_origins })
    }

    fn origin_is_allowed(&self, origin: Option<&str>) -> bool {
        if self.allowed_origins.is_empty() {
            return true;
        }

        // Preserve support for non-browser GET clients, which do not send an Origin header.
        let Some(origin) = origin else {
            return true;
        };
        let origin = normalize_origin(origin);
        self.allowed_origins
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(origin))
    }
}

impl Storable for Config {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(encode_config(self))
    }

    fn into_bytes(self) -> Vec<u8> {
        encode_config(&self)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        // Releases before v2.15.1 initialized the cell with an empty value, and a canister
        // that never set its origins still holds it.
        if bytes.is_empty() {
            return Self::default();
        }
        from_slice(bytes.as_ref()).expect("failed to decode stable canister config")
    }
}

fn encode_config(config: &Config) -> Vec<u8> {
    to_vec(config).expect("failed to encode stable canister config")
}

const CONFIG_MEMORY_ID: MemoryId = MemoryId::new(0);

thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    static CONFIG: RefCell<StableCell<Config, Memory>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with_borrow(|manager| manager.get(CONFIG_MEMORY_ID)),
            Config::default(),
        )
    );
    static STATE: RefCell<State> = RefCell::new(State::default());
}

pub mod config {
    use super::*;

    pub fn set_allowed_origins(allowed_origins: Vec<String>) -> Result<(), String> {
        let config = Config::new(allowed_origins)?;
        CONFIG.with_borrow_mut(|cell| {
            if cell.get() != &config {
                cell.set(config);
            }
        });
        Ok(())
    }

    pub fn origin_is_allowed(origin: Option<&str>) -> bool {
        CONFIG.with_borrow(|cell| cell.get().origin_is_allowed(origin))
    }
}

pub mod state {
    use super::*;

    pub fn init_certified_data() {
        ic_cdk::api::certified_data_set(skip_certification_certified_data());
    }

    pub fn with_delegation<R>(pubkey: &[u8], f: impl FnOnce(&[u8]) -> R) -> Option<R> {
        let now_ms = ic_cdk::api::time() / 1_000_000;
        STATE.with_borrow(|state| state.with_delegation(pubkey, now_ms, f))
    }

    pub fn put_delegation(signed_delegation: Vec<u8>) -> Result<Vec<u8>, String> {
        let now_ms = ic_cdk::api::time() / 1_000_000;
        STATE.with_borrow_mut(|state| state.put_delegation(signed_delegation, now_ms))
    }
}

#[inline]
fn normalize_origin(origin: &str) -> &str {
    let origin = origin.trim().trim_end_matches('/');
    if origin
        .get(..7)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("http://"))
        && origin.ends_with(":80")
    {
        &origin[..origin.len() - 3]
    } else if origin
        .get(..8)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
        && origin.ends_with(":443")
    {
        &origin[..origin.len() - 4]
    } else {
        origin
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ic_auth_types::{DelegationCompact, SignedDelegationCompact};

    const NOW_MS: u64 = 1_000_000;

    /// The wire format of `ic_auth_verifier::deeplink::SignInResponse`.
    #[derive(Serialize)]
    struct WireSignInResponse {
        #[serde(rename = "u")]
        user_pubkey: ByteBufB64,
        #[serde(rename = "d")]
        delegations: Vec<SignedDelegationCompact>,
        #[serde(rename = "a")]
        authn_method: String,
        #[serde(rename = "o")]
        origin: String,
    }

    fn session_pubkey(seed: u8) -> Vec<u8> {
        let mut pubkey = vec![
            0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
        ];
        pubkey.extend_from_slice(&[seed; 32]);
        pubkey
    }

    fn sign_in_response(pubkeys: &[&[u8]], expiration_ms: u64, signature: u8) -> Vec<u8> {
        let delegations = pubkeys
            .iter()
            .map(|pubkey| SignedDelegationCompact {
                delegation: DelegationCompact {
                    pubkey: pubkey.to_vec().into(),
                    expiration: expiration_ms * 1_000_000,
                    targets: None,
                    permissions: None,
                },
                signature: vec![signature; 64].into(),
            })
            .collect();
        to_vec(&WireSignInResponse {
            user_pubkey: vec![1; 44].into(),
            delegations,
            authn_method: "passkey".to_string(),
            origin: "https://example.com".to_string(),
        })
        .unwrap()
    }

    fn payload(seed: u8) -> Vec<u8> {
        sign_in_response(&[&session_pubkey(seed)], NOW_MS + 600_000, 0)
    }

    #[test]
    fn config_encoding_matches_the_deployed_layout() {
        let config = Config {
            allowed_origins: vec!["https://example.com".to_string()],
        };
        let expected = hex::decode(
            "a16f616c6c6f7765645f6f726967696e73817368747470733a2f2f6578616d706c652e636f6d",
        )
        .unwrap();

        assert_eq!(encode_config(&config), expected);
        assert_eq!(Config::from_bytes(Cow::Owned(expected)), config);
        assert_eq!(Config::from_bytes(Cow::Borrowed(&[])), Config::default());
    }

    #[test]
    fn config_normalizes_and_deduplicates_origins() {
        let config = Config::new(vec![
            " HTTPS://EXAMPLE.COM:443/ ".to_string(),
            "https://example.com".to_string(),
        ])
        .unwrap();

        assert_eq!(config.allowed_origins, vec!["https://example.com"]);
        assert!(config.origin_is_allowed(Some("https://EXAMPLE.com/")));
        assert!(config.origin_is_allowed(None));
        assert!(!config.origin_is_allowed(Some("https://other.example")));
    }

    #[test]
    fn a_stored_delegation_is_served_until_it_expires() {
        let mut state = State::default();
        let payload = payload(1);

        assert_eq!(
            state.put_delegation(payload.clone(), NOW_MS).unwrap(),
            session_pubkey(1)
        );
        assert_eq!(
            state.with_delegation(&session_pubkey(1), NOW_MS + RELAY_TTL_MS - 1, |p| p
                .to_vec()),
            Some(payload)
        );
        assert_eq!(
            state.with_delegation(&session_pubkey(1), NOW_MS + RELAY_TTL_MS, |_| ()),
            None
        );
    }

    #[test]
    fn the_last_delegation_names_the_session_key() {
        let mut state = State::default();
        let payload = sign_in_response(
            &[&session_pubkey(1), &session_pubkey(2)],
            NOW_MS + 600_000,
            0,
        );

        assert_eq!(
            state.put_delegation(payload, NOW_MS).unwrap(),
            session_pubkey(2)
        );
    }

    #[test]
    fn a_retry_is_idempotent_and_a_different_payload_is_rejected() {
        let mut state = State::default();
        let first = payload(1);
        state.put_delegation(first.clone(), NOW_MS).unwrap();

        assert_eq!(
            state.put_delegation(first.clone(), NOW_MS + 1).unwrap(),
            session_pubkey(1)
        );
        let other = sign_in_response(&[&session_pubkey(1)], NOW_MS + 600_000, 9);
        assert_eq!(
            state.put_delegation(other.clone(), NOW_MS + 1).unwrap_err(),
            "a delegation is already stored for this session public key"
        );
        assert_eq!(
            state.with_delegation(&session_pubkey(1), NOW_MS + 1, |p| p.to_vec()),
            Some(first)
        );

        // Once the first one expires, the key can be used again.
        state
            .put_delegation(other.clone(), NOW_MS + RELAY_TTL_MS)
            .unwrap();
        assert_eq!(
            state.with_delegation(&session_pubkey(1), NOW_MS + RELAY_TTL_MS, |p| p.to_vec()),
            Some(other)
        );
    }

    #[test]
    fn expired_entries_are_dropped_on_the_next_put() {
        let mut state = State::default();
        state.put_delegation(payload(1), NOW_MS).unwrap();
        state.put_delegation(payload(2), NOW_MS + 1).unwrap();

        state
            .put_delegation(payload(3), NOW_MS + RELAY_TTL_MS)
            .unwrap();
        assert_eq!(state.delegations.len(), 2);
        assert!(!state.delegations.contains_key(&session_pubkey(1)));
    }

    #[test]
    fn the_store_is_bounded() {
        let mut state = State::default();
        for i in 0..MAX_CACHED_DELEGATIONS {
            state.delegations.insert(
                i.to_be_bytes().to_vec(),
                StoredDelegation {
                    payload: Vec::new(),
                    expires_at: NOW_MS + RELAY_TTL_MS,
                },
            );
        }

        assert_eq!(
            state.put_delegation(payload(1), NOW_MS).unwrap_err(),
            "delegation store is at capacity"
        );
        state
            .put_delegation(payload(1), NOW_MS + RELAY_TTL_MS)
            .unwrap();
        assert_eq!(state.delegations.len(), 1);
    }

    #[test]
    fn invalid_delegations_are_rejected() {
        let mut state = State::default();
        let cases = [
            (Vec::new(), "invalid signed delegation length"),
            (
                vec![0; MAX_SIGNED_DELEGATION_BYTES + 1],
                "invalid signed delegation length",
            ),
            (
                sign_in_response(&[&session_pubkey(1)], NOW_MS, 0),
                "delegation has expired",
            ),
            (
                sign_in_response(&[], NOW_MS + 600_000, 0),
                "no delegation found",
            ),
            (
                sign_in_response(&[&[7; MAX_PUBLIC_KEY_BYTES + 1]], NOW_MS + 600_000, 0),
                "invalid session public key length",
            ),
        ];
        for (payload, expected) in cases {
            assert_eq!(state.put_delegation(payload, NOW_MS).unwrap_err(), expected);
        }

        let err = state.put_delegation(vec![0xff], NOW_MS).unwrap_err();
        assert!(
            err.starts_with("failed to decode signed delegation"),
            "{err}"
        );
        assert!(state.delegations.is_empty());
    }
}
