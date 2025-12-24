use candid::{CandidType, Principal};
use ciborium::{from_reader_with_buffer, into_writer};
use ic_canister_sig_creation::{
    signature_map::{CanisterSigInputs, SignatureMap, LABEL_SIG},
    DELEGATION_SIG_DOMAIN,
};
use ic_cdk::api::certified_data_set;
use ic_certification::labeled_hash;
use ic_http_certification::{
    cel::{create_cel_expr, DefaultCelBuilder},
    HttpCertification, HttpCertificationPath, HttpCertificationTree, HttpCertificationTreeEntry,
};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableCell,
};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteArray;
use std::{cell::RefCell, collections::BTreeMap};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct State {
    pub domains: BTreeMap<String, String>, // domain -> url
    pub nonce_iv: ByteArray<32>,
    pub statement: String,
    pub session_expires_in_ms: u64,
    pub governance_canister: Option<Principal>,
}

#[derive(CandidType, Clone, Default, Deserialize, Serialize)]
pub struct StateInfo {
    pub domains: BTreeMap<String, String>, // domain -> url
    pub statement: String,
    pub session_expires_in_ms: u64,
    pub governance_canister: Option<Principal>,
}

impl From<&State> for StateInfo {
    fn from(state: &State) -> Self {
        StateInfo {
            domains: state.domains.clone(),
            statement: state.statement.clone(),
            session_expires_in_ms: state.session_expires_in_ms,
            governance_canister: state.governance_canister,
        }
    }
}

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
    static SIGNATURES : RefCell<SignatureMap> = RefCell::new(SignatureMap::default());
    static HTTP_TREE: RefCell<HttpCertificationTree> = RefCell::new(HttpCertificationTree::default());


    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));

    static STATE_STORE: RefCell<StableCell<Vec<u8>, Memory>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(STATE_MEMORY_ID)),
            Vec::new()
        )
    );
}

pub mod state {
    use super::*;
    use lazy_static::lazy_static;
    use once_cell::sync::Lazy;

    lazy_static! {
        pub static ref DEFAULT_EXPR_PATH: HttpCertificationPath<'static> =
            HttpCertificationPath::wildcard("");
        pub static ref DEFAULT_CERTIFICATION: HttpCertification = HttpCertification::skip();
        pub static ref DEFAULT_CEL_EXPR: String =
            create_cel_expr(&DefaultCelBuilder::skip_certification());
    }

    pub static DEFAULT_CERT_ENTRY: Lazy<HttpCertificationTreeEntry> =
        Lazy::new(|| HttpCertificationTreeEntry::new(&*DEFAULT_EXPR_PATH, *DEFAULT_CERTIFICATION));

    pub fn with<R>(f: impl FnOnce(&State) -> R) -> R {
        STATE.with_borrow(f)
    }

    pub fn with_mut<R>(f: impl FnOnce(&mut State) -> R) -> R {
        STATE.with_borrow_mut(f)
    }

    pub fn http_tree_with<R>(f: impl FnOnce(&HttpCertificationTree) -> R) -> R {
        HTTP_TREE.with(|r| f(&r.borrow()))
    }

    pub fn init_http_certified_data() {
        HTTP_TREE.with(|r| {
            let mut tree = r.borrow_mut();
            tree.insert(&DEFAULT_CERT_ENTRY);
            ic_cdk::api::certified_data_set(tree.root_hash())
        });
    }

    pub fn load() {
        let mut scratch = [0; 4096];
        STATE_STORE.with(|r| {
            STATE.with(|h| {
                let v: State = from_reader_with_buffer(&r.borrow().get()[..], &mut scratch)
                    .expect("failed to decode STATE_STORE data");
                *h.borrow_mut() = v;
            });
        });
    }

    pub fn save() {
        STATE.with(|h| {
            STATE_STORE.with(|r| {
                let mut buf = vec![];
                into_writer(&(*h.borrow()), &mut buf).expect("failed to encode STATE_STORE data");
                r.borrow_mut().set(buf);
            });
        });
    }

    pub async fn init() {
        let mut data = ic_cdk::management_canister::raw_rand()
            .await
            .expect("failed to generate IV");
        data.truncate(32);
        let nonce_iv: [u8; 32] = data.try_into().expect("failed to generate IV");
        STATE.with_borrow_mut(|r| {
            r.nonce_iv = nonce_iv.into();
        });
    }

    pub fn add_signature(seed: &[u8], message: &[u8]) {
        SIGNATURES.with_borrow_mut(|sigs| {
            let sig_inputs = CanisterSigInputs {
                domain: DELEGATION_SIG_DOMAIN,
                seed,
                message,
            };
            sigs.add_signature(&sig_inputs);

            certified_data_set(labeled_hash(LABEL_SIG, &sigs.root_hash()));
        });
    }

    pub fn get_signature(seed: &[u8], message: &[u8]) -> Result<Vec<u8>, String> {
        SIGNATURES.with_borrow(|sigs| {
            let sig_inputs = CanisterSigInputs {
                domain: DELEGATION_SIG_DOMAIN,
                seed,
                message,
            };
            sigs.get_signature_as_cbor(&sig_inputs, None)
                .map_err(|err| format!("failed to get signature: {:?}", err))
        })
    }
}
