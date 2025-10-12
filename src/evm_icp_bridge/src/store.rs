use alloy::{
    consensus::{SignableTransaction, Signed, TxEip1559},
    eips::Encodable2718,
    primitives::{hex, Address, Bytes, Signature, TxHash, U256},
};
use candid::{CandidType, Nat, Principal};
use ciborium::{from_reader, into_writer};
use ic_cose_types::{format_error, types::PublicKeyOutput};
use ic_http_certification::{
    cel::{create_cel_expr, DefaultCelBuilder},
    HttpCertification, HttpCertificationPath, HttpCertificationTree, HttpCertificationTreeEntry,
};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, StableLog, Storable,
};
use icrc_ledger_types::{
    icrc1::{account::Account, transfer::TransferArg},
    icrc2::transfer_from::{TransferFromArgs, TransferFromError},
};
use num_traits::cast::ToPrimitive;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteArray;
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
    time::Duration,
};

use crate::{
    ecdsa::{derive_public_key, ecdsa_public_key, sign_with_ecdsa},
    evm::{encode_erc20_transfer, EvmClient},
    helper::{call, convert_amount},
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
    pub min_threshold_to_bridge: u128,
    // chain_name => (contract_address, decimals, chain_id)
    pub evm_token_contracts: HashMap<String, (Address, u8, u64)>,
    // chain_name => (latest_finalized_block_number, gas_price)
    pub evm_finalized_block: HashMap<String, (u64, u128)>,
    // chain_name => (max_confirmations, [provider_url])
    pub evm_providers: HashMap<String, (u64, Vec<String>)>,
    pub ecdsa_public_key: PublicKeyOutput,
    pub governance_canister: Option<Principal>,
    pub pending: VecDeque<BridgeLog>,
    // (round, running)
    pub finalize_bridging_round: (u64, bool),
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
    pub min_threshold_to_bridge: u128,
    pub evm_token_contracts: HashMap<String, (String, u8, u64)>,
    pub evm_finalized_block: HashMap<String, (u64, u128)>,
    pub evm_providers: HashMap<String, (u64, Vec<String>)>,
    pub finalize_bridging_round: u64,
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
            min_threshold_to_bridge: s.min_threshold_to_bridge,
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
            finalize_bridging_round: s.finalize_bridging_round.0,
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
            min_threshold_to_bridge: 100_000_000, // 1 Token (8 decimals)
            evm_token_contracts: HashMap::new(),
            evm_providers: HashMap::new(),
            evm_finalized_block: HashMap::new(),
            ecdsa_public_key: PublicKeyOutput {
                public_key: vec![].into(),
                chain_code: vec![].into(),
            },
            governance_canister: None,
            pending: VecDeque::new(),
            finalize_bridging_round: (0, false),
        }
    }
}

#[derive(Clone, CandidType, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum BridgeTarget {
    Icp,
    Evm(String), // chain_name
}

#[derive(Clone, CandidType, Serialize, Deserialize, PartialEq, Eq)]
pub enum BridgeTx {
    Icp(bool, u64),           // (finalized, block_height)
    Evm(bool, ByteArray<32>), // (finalized, tx_hash)
}

impl BridgeTx {
    pub fn is_finalized(&self) -> bool {
        match self {
            BridgeTx::Icp(finalized, _) => *finalized,
            BridgeTx::Evm(finalized, _) => *finalized,
        }
    }

    pub fn same_with(&self, other: &BridgeTx) -> bool {
        match (self, other) {
            (BridgeTx::Icp(_, tx1), BridgeTx::Icp(_, tx2)) => tx1 == tx2,
            (BridgeTx::Evm(_, tx1), BridgeTx::Evm(_, tx2)) => tx1 == tx2,
            _ => false,
        }
    }
}

#[derive(Clone, CandidType, Serialize, Deserialize)]
pub struct BridgeLog {
    pub user: Principal,
    pub from: BridgeTarget,
    pub to: BridgeTarget,
    pub icp_amount: u128,
    pub from_tx: BridgeTx,
    pub to_tx: Option<BridgeTx>,
    pub created_at: u64,
    pub finalized_at: u64,
    pub error: Option<String>,
}

impl BridgeLog {
    pub fn is_finalized(&self) -> bool {
        self.from_tx.is_finalized() && self.to_tx.as_ref().is_some_and(|tx| tx.is_finalized())
    }

    pub fn same_with(&self, other: &BridgeLog) -> bool {
        self.user == other.user
            && self.from == other.from
            && self.to == other.to
            && self.icp_amount == other.icp_amount
            && self.from_tx.same_with(&other.from_tx)
    }
}

impl Storable for BridgeLog {
    const BOUND: Bound = Bound::Unbounded;

    fn into_bytes(self) -> Vec<u8> {
        let mut buf = vec![];
        into_writer(&self, &mut buf).expect("failed to encode BridgeLog data");
        buf
    }

    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut buf = vec![];
        into_writer(&self, &mut buf).expect("failed to encode BridgeLog data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode BridgeLog data")
    }
}

#[derive(Clone, CandidType, Default, Serialize, Deserialize)]
pub struct UserLogs {
    pub logs: BTreeSet<u64>, // finalized_at timestamps
}

impl Storable for UserLogs {
    const BOUND: Bound = Bound::Unbounded;

    fn into_bytes(self) -> Vec<u8> {
        let mut buf = vec![];
        into_writer(&self, &mut buf).expect("failed to encode UserLogs data");
        buf
    }

    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut buf = vec![];
        into_writer(&self, &mut buf).expect("failed to encode UserLogs data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode UserLogs data")
    }
}

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);
const USER_LOGS_MEMORY_ID: MemoryId = MemoryId::new(1);
const BRIDGE_LOGS_INDEX_MEMORY_ID: MemoryId = MemoryId::new(2);
const BRIDGE_LOGS_DATA_MEMORY_ID: MemoryId = MemoryId::new(3);

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

    static USER_LOGS: RefCell<StableBTreeMap<Principal, UserLogs, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(USER_LOGS_MEMORY_ID)),
        )
    );

    static BRIDGE_LOGS: RefCell<StableLog<BridgeLog, Memory, Memory>> = RefCell::new(
        StableLog::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(BRIDGE_LOGS_INDEX_MEMORY_ID)),
            MEMORY_MANAGER.with_borrow(|m| m.get(BRIDGE_LOGS_DATA_MEMORY_ID)),
        )
    );
}

pub mod state {
    use std::u64;

    use super::*;

    use alloy::eips::Encodable2718;
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
                .map(|(max_confirmations, providers)| EvmClient {
                    providers: providers.clone(),
                    max_confirmations: *max_confirmations,
                    api_token: None,
                })
                .unwrap_or_else(|| EvmClient {
                    providers: vec![],
                    max_confirmations: 1,
                    api_token: None,
                })
        })
    }

    pub async fn bridge(
        from_chain: String,
        to_chain: String,
        icp_amount: u128,
        user: Principal,
        now_ms: u64,
    ) -> Result<BridgeTx, String> {
        if from_chain == to_chain {
            return Err("from_chain and to_chain cannot be the same".to_string());
        }

        let (from, to, token_ledger) = STATE.with_borrow(|s| {
            if icp_amount < s.min_threshold_to_bridge {
                return Err(format!(
                    "amount {} is below the minimum threshold to bridge {}",
                    icp_amount, s.min_threshold_to_bridge
                ));
            }
            let from = if from_chain == "ICP" {
                BridgeTarget::Icp
            } else {
                if !s.evm_token_contracts.contains_key(&from_chain) {
                    return Err(format!(
                        "from_chain {} not found or not supported",
                        from_chain
                    ));
                }
                BridgeTarget::Evm(from_chain)
            };
            let to = if to_chain == "ICP" {
                BridgeTarget::Icp
            } else {
                if !s.evm_token_contracts.contains_key(&to_chain) {
                    return Err(format!("to_chain {} not found or not supported", to_chain));
                }

                BridgeTarget::Evm(to_chain)
            };

            let mut user_pending_task: HashSet<(Principal, BridgeTarget)> = HashSet::new();
            for log in s.pending.iter() {
                if log.user == user
                    && matches!(log.from_tx, BridgeTx::Evm(false, _))
                    && !user_pending_task.insert((log.user, log.from.clone()))
                {
                    return Err(format!(
                        "there is already a pending bridging task from {:?} for user {:?}",
                        log.from, log.user
                    ));
                }
            }

            Ok((from, to, s.token_ledger))
        })?;

        let from_tx = match &from {
            BridgeTarget::Icp => from_icp(token_ledger, user, icp_amount).await?,
            BridgeTarget::Evm(chain) => from_evm(chain, user, icp_amount, now_ms).await?,
        };

        let delay = if from == BridgeTarget::Icp { 0 } else { 7 };
        let round = STATE.with_borrow_mut(|s| {
            s.pending.push_back(BridgeLog {
                user,
                from,
                to,
                icp_amount,
                from_tx: from_tx.clone(),
                to_tx: None,
                created_at: now_ms,
                finalized_at: 0,
                error: None,
            });
            s.finalize_bridging_round.0
        });

        ic_cdk_timers::set_timer(Duration::from_secs(delay), move || {
            ic_cdk::futures::spawn(finalize_bridging(round))
        });

        Ok(from_tx)
    }

    pub fn logs(user: Principal, prev: Option<u64>, take: usize) -> Vec<BridgeLog> {
        USER_LOGS.with_borrow(|r| {
            let item = r.get(&user).unwrap_or_default();
            if item.logs.is_empty() {
                return vec![];
            }
            let ids = item
                .logs
                .range(..prev.unwrap_or(u64::MAX))
                .rev()
                .take(take)
                .cloned()
                .collect::<Vec<u64>>();

            if ids.is_empty() {
                return vec![];
            }

            BRIDGE_LOGS.with_borrow(|log_store| {
                let mut logs: Vec<BridgeLog> = Vec::with_capacity(ids.len());
                for id in ids {
                    if let Some(log) = log_store.get(id) {
                        logs.push(log);
                    }
                }
                logs
            })
        })
    }

    async fn finalize_bridging(round: u64) {
        let tasks = STATE.with_borrow_mut(|s| {
            if s.finalize_bridging_round.1 || round < s.finalize_bridging_round.0 {
                // already running or old round
                return None;
            }

            if s.pending.is_empty() {
                return None;
            }

            s.finalize_bridging_round.1 = true;
            // take up to 3 pending tasks to process in parallel
            let mut tasks = Vec::with_capacity(3);
            let mut locker_pending_task: HashSet<(Principal, BridgeTarget)> = HashSet::new();
            for task in s.pending.iter().take(3) {
                if matches!(task.to_tx, Some(BridgeTx::Evm(false, _)))
                    && !locker_pending_task.insert((s.icp_address, task.to.clone()))
                {
                    // another task for the same evm chain is still pending
                    break;
                }

                tasks.push(task.clone());
            }
            Some(tasks)
        });

        if let Some(tasks) = tasks {
            let tasks = try_finalize_tasks(tasks).await;
            let now_ms = ic_cdk::api::time() / 1_000_000;
            let round = STATE.with_borrow_mut(|s| {
                for task in tasks {
                    for t in s.pending.iter_mut() {
                        if t.same_with(&task) {
                            *t = task;
                            if t.to_tx.as_ref().is_some_and(|tx| tx.is_finalized()) {
                                t.error = None;
                                t.finalized_at = now_ms;
                                let idx = BRIDGE_LOGS
                                    .with_borrow_mut(|r| r.append(t))
                                    .expect("failed to append to BRIDGE_LOGS");
                                USER_LOGS.with_borrow_mut(|r| {
                                    let mut logs = r.get(&t.user).unwrap_or_default();
                                    logs.logs.insert(idx);
                                    r.insert(t.user, logs)
                                        .expect("failed to insert to USER_LOGS");
                                });
                            }
                            break;
                        }
                    }
                }
                s.pending.retain(|t| !t.is_finalized());
                s.finalize_bridging_round = (s.finalize_bridging_round.0 + 1, false);

                if s.pending.is_empty() {
                    0
                } else {
                    s.finalize_bridging_round.0
                }
            });

            if round > 0 {
                ic_cdk_timers::set_timer(Duration::from_secs(1), move || {
                    ic_cdk::futures::spawn(finalize_bridging(round))
                });
            }
        }
    }

    async fn try_finalize_tasks(tasks: Vec<BridgeLog>) -> Vec<BridgeLog> {
        let now_ms = ic_cdk::api::time() / 1_000_000;
        futures::future::join_all(tasks.into_iter().map(|task| process_task(task, now_ms))).await
    }

    async fn process_task(mut task: BridgeLog, now_ms: u64) -> BridgeLog {
        let rt = async {
            let from_finalized = match (&task.from, &mut task.from_tx) {
                (BridgeTarget::Evm(chain), BridgeTx::Evm(finalized, tx_hash)) if !*finalized => {
                    let tx_hash: TxHash = (**tx_hash).into();
                    let from_finalized = check_evm_tx_finalized(chain, &tx_hash, now_ms).await?;
                    if from_finalized {
                        *finalized = true;
                    }
                    from_finalized
                }
                _ => true,
            };

            if from_finalized {
                match (&task.to, &mut task.to_tx) {
                    (BridgeTarget::Icp, None) => {
                        let token_ledger = STATE.with_borrow(|s| s.token_ledger);
                        let to_tx = to_icp(token_ledger, task.user, task.icp_amount).await?;
                        task.to_tx = Some(to_tx);
                    }
                    (BridgeTarget::Evm(chain), None) => {
                        let to_tx = to_evm(chain, task.user, task.icp_amount, now_ms).await?;
                        task.to_tx = Some(to_tx);
                    }
                    (BridgeTarget::Evm(chain), Some(BridgeTx::Evm(finalized, tx_hash)))
                        if !*finalized =>
                    {
                        let tx_hash: TxHash = (**tx_hash).into();
                        let to_finalized = check_evm_tx_finalized(chain, &tx_hash, now_ms).await?;
                        if to_finalized {
                            *finalized = true;
                        }
                    }
                    _ => {}
                }
            }

            Ok::<(), String>(())
        }
        .await;

        if let Err(err) = rt {
            ic_cdk::api::debug_print(format!("finalize_tasks failed: {err}"));
            task.error = Some(err);
        }

        task
    }

    async fn from_icp(
        token_ledger: Principal,
        user: Principal,
        icp_amount: u128,
    ) -> Result<BridgeTx, String> {
        let res: Result<Nat, TransferFromError> = call(
            token_ledger,
            "icrc2_transfer_from",
            (TransferFromArgs {
                spender_subaccount: None,
                from: Account {
                    owner: user,
                    subaccount: None,
                },
                to: Account {
                    owner: ic_cdk::api::canister_self(),
                    subaccount: None,
                },
                fee: None,
                created_at_time: None,
                memo: None,
                amount: icp_amount.into(),
            },),
            0,
        )
        .await?;
        let res =
            res.map_err(|err| format!("failed to transfer ICP from user, error: {:?}", err))?;
        let idx = res
            .0
            .to_u64()
            .ok_or_else(|| "block height too large".to_string())?;
        Ok(BridgeTx::Icp(true, idx))
    }

    async fn to_icp(
        token_ledger: Principal,
        user: Principal,
        icp_amount: u128,
    ) -> Result<BridgeTx, String> {
        let res: Result<Nat, TransferFromError> = call(
            token_ledger,
            "icrc1_transfer",
            (TransferArg {
                from_subaccount: None,
                to: Account {
                    owner: user,
                    subaccount: None,
                },
                fee: None,
                created_at_time: None,
                memo: None,
                amount: icp_amount.into(),
            },),
            0,
        )
        .await?;
        let res = res.map_err(|err| format!("failed to transfer ICP to user, error: {:?}", err))?;
        let idx = res
            .0
            .to_u64()
            .ok_or_else(|| "block height too large".to_string())?;
        Ok(BridgeTx::Icp(true, idx))
    }

    async fn from_evm(
        chain: &str,
        user: Principal,
        icp_amount: u128,
        now_ms: u64,
    ) -> Result<BridgeTx, String> {
        let to_addr = STATE.with_borrow(|s| s.evm_address);
        let (client, signed_tx) =
            build_erc20_transfer_tx(chain, &user, &to_addr, icp_amount, now_ms).await?;
        let tx_hash: [u8; 32] = (*signed_tx.hash()).into();
        let data = signed_tx.encoded_2718();

        let _ = client
            .send_raw_transaction(now_ms, Bytes::from(data).to_string())
            .await?;
        Ok(BridgeTx::Evm(false, tx_hash.into()))
    }

    async fn to_evm(
        chain: &str,
        user: Principal,
        icp_amount: u128,
        now_ms: u64,
    ) -> Result<BridgeTx, String> {
        let to_addr = evm_address(&user);
        let (client, signed_tx) = build_erc20_transfer_tx(
            chain,
            &ic_cdk::api::canister_self(),
            &to_addr,
            icp_amount,
            now_ms,
        )
        .await?;

        let tx_hash: [u8; 32] = (*signed_tx.hash()).into();
        let data = signed_tx.encoded_2718();

        let _ = client
            .send_raw_transaction(now_ms, Bytes::from(data).to_string())
            .await?;
        Ok(BridgeTx::Evm(false, tx_hash.into()))
    }

    pub async fn build_erc20_transfer_tx(
        chain: &str,
        from: &Principal,
        to_addr: &Address,
        icp_amount: u128,
        now_ms: u64,
    ) -> Result<(EvmClient, Signed<TxEip1559>), String> {
        let (key_name, from_pk, mut tx) = STATE.with_borrow(|s| {
            let (contract, decimals, chain_id) = s
                .evm_token_contracts
                .get(chain)
                .cloned()
                .ok_or_else(|| "chain not found".to_string())?;

            let value = convert_amount(icp_amount, s.token_decimals, decimals)?;
            let from_pk = derive_public_key(&s.ecdsa_public_key, vec![from.as_slice().to_vec()])
                .expect("derive_public_key failed");

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
        if &from_addr == to_addr {
            return Err("from and to cannot be the same".to_string());
        }
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
        Ok((client, signed_tx))
    }

    async fn check_evm_tx_finalized(
        chain: &str,
        tx_hash: &TxHash,
        now_ms: u64,
    ) -> Result<bool, String> {
        let client = evm_client(chain);
        let (number, receipt) = futures::future::join(
            client.block_number(now_ms),
            client.get_transaction_receipt(now_ms, tx_hash),
        )
        .await;
        match (number, receipt) {
            (Ok(number), Ok(Some(receipt))) => {
                if let Some(block_number) = receipt.block_number {
                    if block_number + client.max_confirmations <= number {
                        // TODO: validate receipt.logs
                        // log.address == 代币合约地址
                        // log.topics[0] == keccak256("Transfer(address,address,uint256)") = 0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef
                        // log.topics[1] == from 地址（32 字节左填充）
                        // log.topics[2] == to 地址（32 字节左填充）
                        // log.data 为 uint256 的转账数量（ABI 编码）
                        return Ok(receipt.status());
                    }
                }
                Ok(false)
            }
            (Err(err), _) | (_, Err(err)) => {
                Err(format!("failed to check evm tx finalized, error: {err}"))
            }
            _ => Ok(false),
        }
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
