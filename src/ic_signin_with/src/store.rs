use candid::{CandidType, Principal};
use ciborium::{from_reader_with_buffer, into_writer};
use ic_canister_sig_creation::{
    signature_map::{CanisterSigInputs, SignatureMap, LABEL_SIG},
    DELEGATION_SIG_DOMAIN,
};
use ic_cdk::api::certified_data_set;
use ic_certification::{fork, fork_hash, labeled_hash, pruned, Hash, HashTree};
use ic_http_certification::{
    cel::{create_cel_expr, DefaultCelBuilder},
    HttpCertification, HttpCertificationPath, HttpCertificationTree, HttpCertificationTreeEntry,
};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableCell, Storable,
};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteArray;
use std::{borrow::Cow, cell::RefCell, collections::BTreeMap, sync::LazyLock};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct State {
    pub domains: BTreeMap<String, String>,
    pub nonce_iv: ByteArray<32>,
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
        let mut scratch = [0u8; 4_096];
        from_reader_with_buffer(bytes.as_ref(), &mut scratch)
            .expect("failed to decode stable canister state")
    }
}

fn encode_state(state: &State) -> Vec<u8> {
    let mut bytes = Vec::new();
    into_writer(state, &mut bytes).expect("failed to encode stable canister state");
    bytes
}

#[derive(CandidType, Clone, Default, Deserialize, Serialize)]
pub struct StateInfo {
    pub domains: BTreeMap<String, String>,
    pub statement: String,
    pub session_expires_in_ms: u64,
    pub governance_canister: Option<Principal>,
}

impl From<&State> for StateInfo {
    fn from(state: &State) -> Self {
        Self {
            domains: state.domains.clone(),
            statement: state.statement.clone(),
            session_expires_in_ms: state.session_expires_in_ms,
            governance_canister: state.governance_canister,
        }
    }
}

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);

thread_local! {
    static SIGNATURES: RefCell<SignatureMap> = RefCell::new(SignatureMap::default());
    static HTTP_TREE: RefCell<HttpCertificationTree> = RefCell::new(HttpCertificationTree::default());
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    static STATE: RefCell<StableCell<State, Memory>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with_borrow(|manager| manager.get(STATE_MEMORY_ID)),
            State::default(),
        )
    );
}

pub mod state {
    use super::*;

    pub static DEFAULT_EXPR_PATH: LazyLock<HttpCertificationPath<'static>> =
        LazyLock::new(|| HttpCertificationPath::wildcard(""));
    pub static DEFAULT_CERT_ENTRY: LazyLock<HttpCertificationTreeEntry<'static>> =
        LazyLock::new(|| {
            HttpCertificationTreeEntry::new(
                HttpCertificationPath::wildcard(""),
                HttpCertification::skip(),
            )
        });
    pub static DEFAULT_CEL_EXPR: LazyLock<String> =
        LazyLock::new(|| create_cel_expr(&DefaultCelBuilder::skip_certification()));

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

    pub async fn initialize_nonce() {
        let nonce: [u8; 32] = ic_cdk::management_canister::raw_rand()
            .await
            .expect("failed to generate nonce IV")
            .try_into()
            .expect("raw_rand returned an invalid nonce IV length");
        mutate(|state| state.nonce_iv = nonce.into());
    }

    pub fn init_certified_data() {
        HTTP_TREE.with_borrow_mut(|tree| tree.insert(&DEFAULT_CERT_ENTRY));
        refresh_certified_data();
    }

    pub fn http_witness(request_url: &str) -> Result<HashTree, String> {
        let http_witness = HTTP_TREE.with_borrow(|tree| {
            tree.witness(&DEFAULT_CERT_ENTRY, request_url)
                .map_err(|err| format!("failed to create HTTP witness: {err}"))
        })?;
        let signature_root =
            SIGNATURES.with_borrow(|signatures| labeled_hash(LABEL_SIG, &signatures.root_hash()));
        Ok(fork(http_witness, pruned(signature_root)))
    }

    pub fn add_signature(seed: &[u8], message: &[u8]) {
        SIGNATURES.with_borrow_mut(|signatures| {
            signatures.add_signature(&CanisterSigInputs {
                domain: DELEGATION_SIG_DOMAIN,
                seed,
                message,
            });
        });
        refresh_certified_data();
    }

    pub fn get_signature(seed: &[u8], message: &[u8]) -> Result<Vec<u8>, String> {
        let http_root = HTTP_TREE.with_borrow(HttpCertificationTree::root_hash);
        SIGNATURES.with_borrow(|signatures| {
            signatures
                .get_signature_as_cbor(
                    &CanisterSigInputs {
                        domain: DELEGATION_SIG_DOMAIN,
                        seed,
                        message,
                    },
                    Some(http_root),
                )
                .map_err(|err| format!("failed to get signature: {err}"))
        })
    }

    fn refresh_certified_data() {
        certified_data_set(combined_certified_root());
    }

    fn combined_certified_root() -> Hash {
        let http_root = HTTP_TREE.with_borrow(HttpCertificationTree::root_hash);
        let signature_root =
            SIGNATURES.with_borrow(|signatures| labeled_hash(LABEL_SIG, &signatures.root_hash()));
        fork_hash(&http_root, &signature_root)
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
            nonce_iv: [7; 32].into(),
            statement: "Sign in".to_string(),
            session_expires_in_ms: 3_600_000,
            governance_canister: Some(Principal::from_slice(&[1, 2, 3])),
        }
    }

    #[test]
    fn stable_state_encoding_round_trips() {
        let state = sample_state();
        assert_eq!(State::from_bytes(state.to_bytes()), state);
    }

    #[test]
    fn state_cell_reads_the_previous_vec_encoding() {
        let memory = VectorMemory::default();
        let state = sample_state();
        let mut old_cell = StableCell::init(memory.clone(), Vec::<u8>::new());
        old_cell.set(encode_state(&state));
        drop(old_cell);

        let new_cell = StableCell::<State, _>::init(memory, State::default());
        assert_eq!(new_cell.get(), &state);
    }

    #[test]
    fn http_witness_can_be_forked_with_the_signature_tree() {
        let mut tree = HttpCertificationTree::default();
        tree.insert(&state::DEFAULT_CERT_ENTRY);
        let http_root = tree.root_hash();
        let http_witness = tree
            .witness(&state::DEFAULT_CERT_ENTRY, "/verify_envelope")
            .unwrap();
        let signature_root = labeled_hash(LABEL_SIG, &[9; 32]);
        let witness = fork(http_witness, pruned(signature_root));

        assert_eq!(witness.digest(), fork_hash(&http_root, &signature_root));
    }
}
