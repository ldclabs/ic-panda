use alloy::primitives::Address;
use candid::{CandidType, Principal};
use ciborium::{from_reader, into_writer};
use ic_cose_types::types::PublicKeyOutput;
use ic_http_certification::{
    cel::{create_cel_expr, DefaultCelBuilder},
    HttpCertification, HttpCertificationPath, HttpCertificationTree, HttpCertificationTreeEntry,
};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableCell,
};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::HashMap};

use crate::ecdsa::{derive_public_key, ecdsa_public_key};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Serialize, Deserialize)]
pub struct State {
    pub key_name: String,
    pub icp_address: Principal,
    pub evm_address: Address,
    pub token_name: String,
    pub token_symbol: String,
    pub token_logo: String,
    pub token_ledger: Principal,
    pub evm_token_contracts: HashMap<String, (Address, u8, u64)>,
    pub evm_providers: HashMap<String, Vec<String>>,
    pub evm_finalized_block: HashMap<String, u128>,
    pub ecdsa_public_key: PublicKeyOutput,
    pub governance_canister: Option<Principal>,
}

#[derive(CandidType, Serialize, Deserialize)]
pub struct StateInfo {
    pub key_name: String,
    pub icp_address: Principal,
    pub evm_address: String,
    pub token_name: String,
    pub token_symbol: String,
    pub token_logo: String,
    pub token_ledger: Principal,
    pub evm_token_contracts: HashMap<String, (String, u8, u64)>,
    pub evm_providers: HashMap<String, Vec<String>>,
    pub evm_finalized_block: HashMap<String, String>,
}

impl From<&State> for StateInfo {
    fn from(s: &State) -> Self {
        Self {
            key_name: s.key_name.clone(),
            icp_address: s.icp_address,
            evm_address: s.evm_address.to_string(),
            token_name: s.token_name.clone(),
            token_symbol: s.token_symbol.clone(),
            token_logo: s.token_logo.clone(),
            token_ledger: s.token_ledger,
            evm_token_contracts: s
                .evm_token_contracts
                .iter()
                .map(|(k, v)| (k.clone(), (v.0.to_string(), v.1, v.2)))
                .collect(),
            evm_providers: s
                .evm_providers
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            evm_finalized_block: s
                .evm_finalized_block
                .iter()
                .map(|(k, v)| (k.clone(), v.to_string()))
                .collect(),
        }
    }
}

impl State {
    fn new() -> Self {
        Self {
            key_name: "dfx_test_key".to_string(),
            icp_address: ic_cdk::api::canister_self(),
            evm_address: [0u8; 20].into(),
            token_name: "ICPanda".to_string(),
            token_symbol: "PANDA".to_string(),
            token_logo: "https://532er-faaaa-aaaaj-qncpa-cai.icp0.io/f/374?inline&filename=1734188626561.webp".to_string(),
            token_ledger: Principal::from_text("druyg-tyaaa-aaaaq-aactq-cai").unwrap(), // mainnet ledger
            evm_token_contracts: HashMap::new(),
            evm_providers: HashMap::new(),
            evm_finalized_block: HashMap::new(),
            ecdsa_public_key: PublicKeyOutput {
                public_key: vec![].into(),
                chain_code: vec![].into(),
            },
            governance_canister: None,
        }
    }

    fn set_public_key(&mut self, pk: PublicKeyOutput) {
        self.evm_address = pubkey_bytes_to_address(&pk.public_key);
        self.ecdsa_public_key = pk;
    }
}

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::new());
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

    pub async fn init_public_key() {
        let key_name = STATE.with_borrow(|r| r.key_name.clone());
        match ecdsa_public_key(key_name, vec![]).await {
            Ok(pk) => {
                STATE.with_borrow_mut(|s| s.set_public_key(pk));
            }
            Err(err) => {
                ic_cdk::api::debug_print(format!("failed to retrieve ECDSA public key: {err}"));
                return;
            }
        }
    }

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
        STATE_STORE.with_borrow(|r| {
            STATE.with_borrow_mut(|h| {
                let v: State =
                    from_reader(&r.get()[..]).expect("failed to decode STATE_STORE data");
                *h = v;
            });
        });
    }

    pub fn save() {
        STATE.with_borrow(|h| {
            STATE_STORE.with_borrow_mut(|r| {
                let mut buf = vec![];
                into_writer(h, &mut buf).expect("failed to encode STATE_STORE data");
                r.set(buf);
            });
        });
    }

    pub fn info() -> StateInfo {
        STATE.with_borrow(|s| StateInfo::from(s))
    }

    pub fn evm_address(user: &Principal) -> Address {
        STATE.with_borrow(|s| {
            let pk = derive_public_key(&s.ecdsa_public_key, vec![user.as_slice().to_vec()])
                .expect("derive_public_key failed");
            pubkey_bytes_to_address(&pk.public_key)
        })
    }
}

pub fn pubkey_bytes_to_address(pubkey_bytes: &[u8]) -> Address {
    use k256::elliptic_curve::sec1::ToEncodedPoint;
    let key = k256::PublicKey::from_sec1_bytes(pubkey_bytes)
        .expect("failed to parse the public key as SEC1");
    let point = key.to_encoded_point(false);
    Address::from_raw_public_key(&point.as_bytes()[1..])
}
