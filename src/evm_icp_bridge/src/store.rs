use alloy::{
    consensus::{SignableTransaction, Signed, TxEip1559},
    primitives::{hex, Address, Signature, U256},
};
use candid::{CandidType, Principal};
use ciborium::{from_reader, into_writer};
use ic_cose_types::{format_error, types::PublicKeyOutput};
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
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
};

use crate::{
    ecdsa::{derive_public_key, ecdsa_public_key, sign_with_ecdsa},
    evm::{encode_erc20_transfer, EvmClient},
};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Serialize, Deserialize)]
pub struct State {
    pub key_name: String,
    pub icp_address: Principal,
    pub evm_address: Address,
    pub token_name: String,
    pub token_symbol: String,
    pub token_decimals: u8,
    pub token_logo: String,
    pub token_ledger: Principal,
    // chain_name => (contract_address, decimals, chain_id)
    pub evm_token_contracts: HashMap<String, (Address, u8, u64)>,
    // chain_name => (latest_finalized_block_number, gas_price)
    pub evm_finalized_block: HashMap<String, (u64, u128)>,
    pub evm_providers: HashMap<String, Vec<String>>,
    pub ecdsa_public_key: PublicKeyOutput,
    pub governance_canister: Option<Principal>,
    pub pending: HashMap<String, VecDeque<BridgeLog>>,
}

#[derive(CandidType, Serialize, Deserialize)]
pub struct StateInfo {
    pub key_name: String,
    pub icp_address: Principal,
    pub evm_address: String,
    pub token_name: String,
    pub token_symbol: String,
    pub token_decimals: u8,
    pub token_logo: String,
    pub token_ledger: Principal,
    pub evm_token_contracts: HashMap<String, (String, u8, u64)>,
    pub evm_finalized_block: HashMap<String, (u64, u128)>,
    pub evm_providers: HashMap<String, Vec<String>>,
}

impl From<&State> for StateInfo {
    fn from(s: &State) -> Self {
        Self {
            key_name: s.key_name.clone(),
            icp_address: s.icp_address,
            evm_address: s.evm_address.to_string(),
            token_name: s.token_name.clone(),
            token_symbol: s.token_symbol.clone(),
            token_decimals: s.token_decimals,
            token_logo: s.token_logo.clone(),
            token_ledger: s.token_ledger,
            evm_token_contracts: s
                .evm_token_contracts
                .iter()
                .map(|(k, v)| (k.clone(), (v.0.to_string(), v.1, v.2)))
                .collect(),

            evm_finalized_block: s
                .evm_finalized_block
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            evm_providers: s
                .evm_providers
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
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
            token_decimals: 8,
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
            pending: HashMap::new(),
        }
    }
}

#[derive(Clone, CandidType, Serialize, Deserialize, PartialEq, Eq)]
pub enum BridgeTarget {
    Icp,
    Evm(u64),
}

#[derive(Clone, CandidType, Serialize, Deserialize, PartialEq, Eq)]
pub enum BridgeTx {
    Icp(u64),
    Evm(ByteArray<32>),
}

#[derive(Clone, CandidType, Serialize, Deserialize)]
pub struct BridgeLog {
    pub user: Principal,
    pub from: BridgeTarget,
    pub to: BridgeTarget,
    pub icp_value: u128,
    pub from_tx: Option<BridgeTx>,
    pub to_tx: Option<BridgeTx>,
    pub timestamp: u64,
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
            Ok(root_pk) => {
                STATE.with_borrow_mut(|s| {
                    let self_pk =
                        derive_public_key(&root_pk, vec![s.icp_address.as_slice().to_vec()])
                            .expect("derive_public_key failed");
                    s.ecdsa_public_key = root_pk;
                    s.evm_address = pubkey_bytes_to_address(&self_pk.public_key);
                });
            }
            Err(err) => {
                ic_cdk::api::debug_print(format!("failed to retrieve ECDSA public key: {err}"));
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

    pub fn evm_client(chain: &str) -> EvmClient {
        STATE.with_borrow(|s| {
            s.evm_providers
                .get(chain)
                .map(|providers| EvmClient {
                    providers: providers.clone(),
                    api_token: None,
                })
                .unwrap_or_else(|| EvmClient {
                    providers: vec![],
                    api_token: None,
                })
        })
    }

    pub async fn build_erc20_transfer_tx(
        chain: &str,
        from: &Principal,
        to: &Principal,
        icp_value: u128,
        now_ms: u64,
    ) -> Result<Signed<TxEip1559>, String> {
        if from == to {
            return Err("from and to cannot be the same".to_string());
        }

        let (key_name, from_pk, mut tx) = STATE.with_borrow(|s| {
            let (contract, dec, chain_id) = s
                .evm_token_contracts
                .get(chain)
                .cloned()
                .ok_or_else(|| "chain not found".to_string())?;
            let value = icp_value
                .checked_mul(10u128.pow((dec - s.token_decimals) as u32))
                .ok_or_else(|| "value overflow".to_string())?;

            let from_pk = derive_public_key(&s.ecdsa_public_key, vec![from.as_slice().to_vec()])
                .expect("derive_public_key failed");

            let to_addr = if to == &s.icp_address {
                s.evm_address
            } else {
                let pk = derive_public_key(&s.ecdsa_public_key, vec![to.as_slice().to_vec()])
                    .expect("derive_public_key failed");
                pubkey_bytes_to_address(&pk.public_key)
            };

            let input = encode_erc20_transfer(&to_addr, value);
            let gas_price = s
                .evm_finalized_block
                .get(chain)
                .map(|(_, gas_price)| *gas_price)
                .unwrap_or(1_000_000_000u128); // default 1 Gwei
            Ok::<_, String>((
                s.key_name.clone(),
                from_pk,
                TxEip1559 {
                    chain_id,
                    nonce: 0u64,
                    gas_limit: 84_000u64, // sample: ~53,696
                    max_fee_per_gas: gas_price + gas_price / 2,
                    max_priority_fee_per_gas: gas_price / 2,
                    to: contract.into(),
                    input: input.into(),
                    ..Default::default()
                },
            ))
        })?;

        let from_addr = pubkey_bytes_to_address(&from_pk.public_key);
        let client = evm_client(chain);
        let nonce = client.get_transaction_count(now_ms, &from_addr).await?;
        tx.nonce = nonce;

        let msg_hash = tx.signature_hash();
        let sig =
            sign_with_ecdsa(key_name, vec![from.as_slice().to_vec()], msg_hash.to_vec()).await?;
        let signature = Signature::new(
            U256::from_be_slice(&sig[0..32]),  // r
            U256::from_be_slice(&sig[32..64]), // s
            y_parity(msg_hash.as_slice(), &sig, from_pk.public_key.as_slice())?,
        );

        let signed_tx = tx.into_signed(signature);
        Ok(signed_tx)
    }
}

pub fn pubkey_bytes_to_address(pubkey_bytes: &[u8]) -> Address {
    use k256::elliptic_curve::sec1::ToEncodedPoint;
    let key = k256::PublicKey::from_sec1_bytes(pubkey_bytes)
        .expect("failed to parse the public key as SEC1");
    let point = key.to_encoded_point(false);
    Address::from_raw_public_key(&point.as_bytes()[1..])
}

fn y_parity(prehash: &[u8], sig: &[u8], pubkey: &[u8]) -> Result<bool, String> {
    use alloy::signers::k256::ecdsa::{RecoveryId, Signature, VerifyingKey};

    let orig_key = VerifyingKey::from_sec1_bytes(pubkey).map_err(format_error)?;
    let signature = Signature::try_from(sig).map_err(format_error)?;
    for parity in [0u8, 1] {
        let recid = RecoveryId::try_from(parity).map_err(format_error)?;
        let recovered_key = VerifyingKey::recover_from_prehash(prehash, &signature, recid)
            .expect("failed to recover key");
        if recovered_key == orig_key {
            match parity {
                0 => return Ok(false),
                1 => return Ok(true),
                _ => unreachable!(),
            }
        }
    }

    Err(format!(
        "failed to recover the parity bit from a signature; sig: {}, pubkey: {}",
        hex::encode(sig),
        hex::encode(pubkey)
    ))
}
