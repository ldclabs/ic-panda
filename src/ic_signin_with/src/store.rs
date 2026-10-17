use candid::{CandidType, Principal};
use cbor2::{from_slice, to_vec};
use ic_canister_sig_creation::{
    DELEGATION_SIG_DOMAIN,
    signature_map::{CanisterSigInputs, LABEL_SIG, SignatureMap},
};
use ic_cdk::api::certified_data_set;
use ic_certification::labeled_hash;
use ic_stable_structures::{DefaultMemoryImpl, StableCell, Storable, storable::Bound};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, cell::RefCell, collections::BTreeMap};

#[derive(CandidType, Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct State {
    pub domains: BTreeMap<String, String>,
    pub statement: String,
    pub session_expires_in_ms: u64,
    pub governance_canister: Option<Principal>,
}

impl Storable for State {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(encode_state(self))
    }

    fn into_bytes(self) -> Vec<u8> {
        encode_state(&self)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_slice(bytes.as_ref()).expect("failed to decode stable canister state")
    }
}

fn encode_state(state: &State) -> Vec<u8> {
    to_vec(state).expect("failed to encode stable canister state")
}

thread_local! {
    static SIGNATURES: RefCell<SignatureMap> = RefCell::new(SignatureMap::default());
    // The cell is the only stable structure, so it owns stable memory from page 0.
    static STATE: RefCell<StableCell<State, DefaultMemoryImpl>> = RefCell::new(
        StableCell::init(DefaultMemoryImpl::default(), State::default())
    );
}

/// Releases up to v3.0.1 kept the state in a `MemoryManager`, whose header
/// `StableCell::init` would replace with the default state. Refuse to upgrade
/// such a canister: it must be reinstalled with Init args.
pub fn refuse_memory_manager_layout() {
    let mut magic = [0u8; 3];
    if ic_cdk::stable::stable_size() > 0 {
        ic_cdk::stable::stable_read(0, &mut magic);
    }
    if &magic == b"MGR" {
        ic_cdk::trap("the stable memory layout changed; reinstall the canister with Init args");
    }
}

pub mod state {
    use super::*;

    pub fn with<R>(f: impl FnOnce(&State) -> R) -> R {
        STATE.with_borrow(|cell| f(cell.get()))
    }

    pub fn mutate<R>(f: impl FnOnce(&mut State) -> R) -> R {
        STATE.with_borrow_mut(|cell| {
            let mut state = cell.get().clone();
            let result = f(&mut state);
            cell.set(state);
            result
        })
    }

    pub fn try_mutate<R, E>(f: impl FnOnce(&mut State) -> Result<R, E>) -> Result<R, E> {
        STATE.with_borrow_mut(|cell| {
            let mut state = cell.get().clone();
            let result = f(&mut state)?;
            cell.set(state);
            Ok(result)
        })
    }

    /// Add a signature and certify the new root. Only the signature map is
    /// certified, so `get_signature` needs no other subtree.
    pub fn add_signature(seed: &[u8], message: &[u8]) {
        let root = SIGNATURES.with_borrow_mut(|signatures| {
            signatures.add_signature(&CanisterSigInputs {
                domain: DELEGATION_SIG_DOMAIN,
                seed,
                message,
            });
            signatures.root_hash()
        });
        certified_data_set(labeled_hash(LABEL_SIG, &root));
    }

    pub fn get_signature(seed: &[u8], message: &[u8]) -> Result<Vec<u8>, String> {
        SIGNATURES.with_borrow(|signatures| {
            signatures
                .get_signature_as_cbor(
                    &CanisterSigInputs {
                        domain: DELEGATION_SIG_DOMAIN,
                        seed,
                        message,
                    },
                    None,
                )
                .map_err(|err| format!("failed to get signature: {err}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ic_stable_structures::VectorMemory;

    fn sample_state() -> State {
        State {
            domains: BTreeMap::from([(
                "example.com".to_string(),
                "https://example.com".to_string(),
            )]),
            statement: "Sign in".to_string(),
            session_expires_in_ms: 3_600_000,
            governance_canister: Some(Principal::from_slice(&[1, 2, 3])),
        }
    }

    #[test]
    fn stable_state_encoding_is_pinned() {
        // Upgrades read state written by the previous release, so this pins the
        // bytes rather than round-tripping through whichever encoder is current:
        // a round trip passes even if the format silently changes.
        const ENCODED: &str = "a467646f6d61696e73a16b6578616d706c652e636f6d7368747470733a2f2f6578616d\
706c652e636f6d6973746174656d656e74675369676e20696e7573657373696f6e5f657870697265735f696e5f6d73\
1a0036ee8073676f7665726e616e63655f63616e697374657243010203";

        let expected: Vec<u8> = (0..ENCODED.len() / 2)
            .map(|i| u8::from_str_radix(&ENCODED[i * 2..i * 2 + 2], 16).unwrap())
            .collect();

        assert_eq!(encode_state(&sample_state()), expected);
        assert_eq!(State::from_bytes(Cow::Owned(expected)), sample_state());
    }

    #[test]
    fn state_cell_round_trips_through_stable_memory() {
        let memory = VectorMemory::default();
        let mut cell = StableCell::init(memory.clone(), State::default());
        cell.set(sample_state());
        drop(cell);

        let cell = StableCell::<State, _>::init(memory, State::default());
        assert_eq!(cell.get(), &sample_state());
    }
}
