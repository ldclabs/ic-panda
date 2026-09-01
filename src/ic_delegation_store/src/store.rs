use cbor2::{from_slice, to_vec};
use ic_auth_verifier::{verify_delegation_chain, SignInResponse};
use ic_http_certification::utils::skip_certification_certified_data;
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableCell, Storable,
};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{HashMap, VecDeque},
    rc::Rc,
};

type Memory = VirtualMemory<DefaultMemoryImpl>;

pub const MAX_PUBLIC_KEY_BYTES: usize = 48;
pub const MAX_SIGNED_DELEGATION_BYTES: usize = 256 * 1_024;

const RELAY_TTL_MS: u64 = 60 * 1_000;
const MAX_TO_PRUNE: usize = 64;
const MAX_CACHED_DELEGATIONS: usize = 10_000;
const MAX_CACHED_PAYLOAD_BYTES: usize = 32 * 1_024 * 1_024;
const MAX_ALLOWED_ORIGINS: usize = 256;
const MAX_ORIGIN_BYTES: usize = 2_048;

#[derive(Default)]
struct State {
    delegations: HashMap<Rc<[u8]>, StoredDelegation>,
    expiration_queue: VecDeque<DelegationExpiration>,
    payload_bytes: usize,
    next_generation: u64,
}

struct StoredDelegation {
    payload: Vec<u8>,
    expires_at: u64,
    generation: u64,
}

struct DelegationExpiration {
    expires_at: u64,
    generation: u64,
    pubkey: Rc<[u8]>,
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

        let session_pubkey = response
            .delegations
            .last()
            .ok_or_else(|| "no delegation found".to_string())?
            .delegation
            .pubkey
            .as_slice();
        if session_pubkey.is_empty() || session_pubkey.len() > MAX_PUBLIC_KEY_BYTES {
            return Err("invalid session public key length".to_string());
        }

        self.prune_expired(now_ms);

        // HTTP gateways and clients may retry an update. An identical, live payload was already
        // verified, so avoid repeating the cryptographic verification and growing the expiry queue.
        let existing = self.delegations.get(session_pubkey);
        let is_live_replay = existing.is_some_and(|stored| {
            stored.expires_at > now_ms && stored.payload == signed_delegation
        });
        if is_live_replay {
            return Ok(std::mem::take(
                &mut response
                    .delegations
                    .last_mut()
                    .expect("the delegation was checked above")
                    .delegation
                    .pubkey,
            )
            .into_vec());
        }

        let existing_payload_bytes = existing.map_or(0, |stored| stored.payload.len());
        if existing.is_none() && self.delegations.len() >= MAX_CACHED_DELEGATIONS {
            return Err("delegation store is at capacity".to_string());
        }
        let projected_payload_bytes = self
            .payload_bytes
            .saturating_sub(existing_payload_bytes)
            .checked_add(signed_delegation.len())
            .ok_or_else(|| "delegation store capacity overflow".to_string())?;
        if projected_payload_bytes > MAX_CACHED_PAYLOAD_BYTES {
            return Err("delegation store is at capacity".to_string());
        }

        verify_delegation_chain(
            response.user_pubkey.as_slice(),
            session_pubkey,
            &response.delegations,
            now_ms,
            None,
        )?;

        let session_pubkey = std::mem::take(
            &mut response
                .delegations
                .last_mut()
                .expect("the delegation was checked above")
                .delegation
                .pubkey,
        )
        .into_vec();
        let cache_key: Rc<[u8]> = Rc::from(session_pubkey.as_slice());
        let generation = self.next_generation;
        self.next_generation = self.next_generation.wrapping_add(1);
        let expires_at = now_ms.saturating_add(RELAY_TTL_MS);

        self.delegations.insert(
            cache_key.clone(),
            StoredDelegation {
                payload: signed_delegation,
                expires_at,
                generation,
            },
        );
        self.expiration_queue.push_back(DelegationExpiration {
            expires_at,
            generation,
            pubkey: cache_key,
        });
        self.payload_bytes = projected_payload_bytes;

        Ok(session_pubkey)
    }

    fn prune_expired(&mut self, now_ms: u64) {
        for _ in 0..MAX_TO_PRUNE {
            let Some(expiration) = self.expiration_queue.front() else {
                break;
            };
            if expiration.expires_at > now_ms {
                break;
            }

            let expiration = self
                .expiration_queue
                .pop_front()
                .expect("the queue front was checked above");
            let is_current = self
                .delegations
                .get(expiration.pubkey.as_ref())
                .is_some_and(|stored| stored.generation == expiration.generation);
            if is_current {
                let removed = self
                    .delegations
                    .remove(expiration.pubkey.as_ref())
                    .expect("the current delegation must exist");
                self.payload_bytes = self.payload_bytes.saturating_sub(removed.payload.len());
            }
        }
    }

    #[cfg(test)]
    fn insert_for_test(&mut self, pubkey: &[u8], payload: &[u8], expires_at: u64) {
        let generation = self.next_generation;
        self.next_generation += 1;
        let key: Rc<[u8]> = Rc::from(pubkey);
        if let Some(previous) = self.delegations.insert(
            key.clone(),
            StoredDelegation {
                payload: payload.to_vec(),
                expires_at,
                generation,
            },
        ) {
            self.payload_bytes -= previous.payload.len();
        }
        self.payload_bytes += payload.len();
        self.expiration_queue.push_back(DelegationExpiration {
            expires_at,
            generation,
            pubkey: key,
        });
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

    pub fn normalize_allowed_origins() -> Result<(), String> {
        let allowed_origins = CONFIG.with_borrow(|cell| cell.get().allowed_origins.clone());
        set_allowed_origins(allowed_origins)
    }

    pub(super) fn origin_is_allowed(origin: Option<&str>) -> bool {
        CONFIG.with_borrow(|cell| cell.get().origin_is_allowed(origin))
    }
}

pub mod state {
    use super::*;

    pub fn init_certified_data() {
        ic_cdk::api::certified_data_set(skip_certification_certified_data());
    }

    pub fn with_delegation<R>(
        pubkey: &[u8],
        origin: Option<&str>,
        f: impl FnOnce(&[u8]) -> R,
    ) -> Option<R> {
        if !config::origin_is_allowed(origin) {
            return None;
        }
        let now_ms = ic_cdk::api::time() / 1_000_000;
        STATE.with_borrow(|state| state.with_delegation(pubkey, now_ms, f))
    }

    pub fn put_delegation(signed_delegation: Vec<u8>, origin: &str) -> Result<Vec<u8>, String> {
        if !config::origin_is_allowed(Some(origin)) {
            return Err(format!("origin not allowed: {origin}"));
        }
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
    use ed25519_dalek::{Signer, SigningKey};
    use ic_auth_types::{DelegationCompact, SignedDelegationCompact};
    use ic_canister_sig_creation::delegation_signature_msg;
    use ic_stable_structures::VectorMemory;

    const ED25519_DER_PREFIX: &[u8] = &[
        0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
    ];

    fn signed_delegation(now_ms: u64) -> (Vec<u8>, Vec<u8>) {
        let signing_key = SigningKey::from_bytes(&[7; 32]);
        let mut user_pubkey = ED25519_DER_PREFIX.to_vec();
        user_pubkey.extend_from_slice(signing_key.verifying_key().as_bytes());

        let mut session_pubkey = ED25519_DER_PREFIX.to_vec();
        session_pubkey.extend_from_slice(&[9; 32]);
        let expiration = now_ms.saturating_add(10 * 60 * 1_000) * 1_000_000;
        let mut message = ic_auth_verifier::IC_REQUEST_AUTH_DELEGATION_DOMAIN_SEPARATOR.to_vec();
        message.extend_from_slice(&delegation_signature_msg(&session_pubkey, expiration, None));
        let signature = signing_key.sign(&message).to_bytes().to_vec();
        let response = SignInResponse {
            user_pubkey: user_pubkey.into(),
            delegations: vec![SignedDelegationCompact {
                delegation: DelegationCompact {
                    pubkey: session_pubkey.clone().into(),
                    expiration,
                    targets: None,
                    permissions: None,
                },
                signature: signature.into(),
            }],
            authn_method: "test".to_string(),
            origin: "https://example.com".to_string(),
        };

        (to_vec(&response).unwrap(), session_pubkey)
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
    }

    #[test]
    fn config_cell_reads_the_previous_vec_encoding() {
        let memory = VectorMemory::default();
        let config = Config {
            allowed_origins: vec!["https://example.com".to_string()],
        };
        let mut old_cell = StableCell::init(memory.clone(), Vec::<u8>::new());
        old_cell.set(encode_config(&config));
        drop(old_cell);

        let new_cell = StableCell::<Config, _>::init(memory, Config::default());
        assert_eq!(new_cell.get(), &config);
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
    fn expired_delegations_are_not_returned() {
        let mut state = State::default();
        state.insert_for_test(b"key", b"payload", 100);

        assert_eq!(
            state.with_delegation(b"key", 99, |payload| payload.to_vec()),
            Some(b"payload".to_vec())
        );
        assert_eq!(state.with_delegation(b"key", 100, |_| ()), None);
    }

    #[test]
    fn valid_delegation_is_verified_once_and_replays_are_idempotent() {
        let now_ms = 1_000_000;
        let (payload, session_pubkey) = signed_delegation(now_ms);
        let mut state = State::default();

        assert_eq!(
            state.put_delegation(payload.clone(), now_ms).unwrap(),
            session_pubkey
        );
        assert_eq!(state.expiration_queue.len(), 1);
        assert_eq!(
            state.put_delegation(payload, now_ms + 1).unwrap(),
            session_pubkey
        );
        assert_eq!(state.expiration_queue.len(), 1);
    }

    #[test]
    fn stale_expiration_does_not_remove_a_newer_value() {
        let mut state = State::default();
        state.insert_for_test(b"key", b"old", 100);
        state.insert_for_test(b"key", b"new", 200);

        state.prune_expired(100);
        assert_eq!(
            state.with_delegation(b"key", 100, |payload| payload.to_vec()),
            Some(b"new".to_vec())
        );
        assert_eq!(state.payload_bytes, 3);

        state.prune_expired(200);
        assert!(state.delegations.is_empty());
        assert_eq!(state.payload_bytes, 0);
    }

    #[test]
    fn pruning_an_empty_queue_stops_immediately() {
        let mut state = State::default();
        state.prune_expired(u64::MAX);
        assert!(state.expiration_queue.is_empty());
    }
}
