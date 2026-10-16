//! Stable state of the hub: configuration, quotes, activations, payouts,
//! wallet records, the member index and wallet modules.
use candid::Principal;
use dmsg_protocol::canonical;
use dmsg_runtime::storage::Stored;
use dmsg_types::{account::account_cbor, wallet::*, *};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use icrc_ledger_types::icrc1::account::Account;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::cell::RefCell;

type Memory = VirtualMemory<DefaultMemoryImpl>;

/// Largest wallet module the hub keeps.
pub(crate) const MAX_MODULE: usize = 4 * 1024 * 1024;
/// Versions kept, retired ones included.
pub(crate) const MAX_VERSIONS: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Hub {
    pub(crate) init: HubInit,
    pub(crate) versions: Vec<WalletVersion>,
    pub(crate) current: Option<u32>,
    pub(crate) uploader: Option<Principal>,
    pub(crate) quotes_issued: u64,
}

/// One ledger transfer of a payout with its fixed arguments.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PayoutTransfer {
    #[serde(with = "account_cbor")]
    pub(crate) to: Account,
    pub(crate) amount: u128,
    pub(crate) memo: Option<ByteBuf>,
    pub(crate) block: Option<u64>,
}

/// Transfers out of a deposit: a refund, or revenue split between hub
/// cycles and the revenue account.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Payout {
    pub(crate) refund: bool,
    pub(crate) created_at_time: u64,
    pub(crate) transfers: Vec<PayoutTransfer>,
    /// Cycles the CMC minted for the hub from the first transfer.
    pub(crate) cycles: Option<u128>,
}

pub(crate) fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

thread_local! {
    static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    static STABLE_HUB: RefCell<StableCell<Stored<Option<Hub>>, Memory>> =
        RefCell::new(StableCell::init(memory(0), Stored(None)));
    static HUB: RefCell<Option<Hub>> = RefCell::new(STABLE_HUB.with_borrow(|c| c.get().0.clone()));
    static QUOTES: RefCell<StableBTreeMap<[u8; 32], Stored<ActivationQuote>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(1)));
    static ORDERS: RefCell<StableBTreeMap<[u8; 32], Stored<ActivationOrder>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(2)));
    static PAYOUTS: RefCell<StableBTreeMap<[u8; 32], Stored<Payout>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(3)));
    static WALLETS: RefCell<StableBTreeMap<Vec<u8>, Stored<WalletRecord>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(4)));
    /// canonical(MemberId) length, canonical(MemberId), wallet bytes.
    static INDEX: RefCell<StableBTreeMap<Vec<u8>, u8, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(5)));
    /// Creating account, then quote ID, of every activation holding quota.
    static CREATED: RefCell<StableBTreeMap<Vec<u8>, u8, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(6)));
    static MODULES: RefCell<StableBTreeMap<u32, Stored<ByteBuf>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(7)));
    static STAGING: RefCell<StableBTreeMap<u32, Stored<ByteBuf>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(8)));
}

pub(crate) fn with<R>(f: impl FnOnce(&Hub) -> R) -> R {
    HUB.with_borrow(|h| f(h.as_ref().expect("installed")))
}

pub(crate) fn hub() -> Hub {
    with(Clone::clone)
}

pub(crate) fn save(h: Hub) {
    STABLE_HUB.with_borrow_mut(|c| c.set(Stored(Some(h.clone()))));
    HUB.with_borrow_mut(|cell| *cell = Some(h));
}

pub(crate) fn quote(id: &Hash) -> Option<ActivationQuote> {
    QUOTES.with_borrow(|t| t.get(&id.into_array())).map(|q| q.0)
}

pub(crate) fn save_quote(q: &ActivationQuote) {
    QUOTES.with_borrow_mut(|t| t.insert(q.quote_id.into_array(), Stored(q.clone())));
}

pub(crate) fn order(id: &Hash) -> Option<ActivationOrder> {
    ORDERS.with_borrow(|t| t.get(&id.into_array())).map(|o| o.0)
}

pub(crate) fn save_order(o: &ActivationOrder) {
    ORDERS.with_borrow_mut(|t| t.insert(o.quote.quote_id.into_array(), Stored(o.clone())));
}

pub(crate) fn order_count() -> u64 {
    ORDERS.with_borrow(|t| t.len())
}

pub(crate) fn payout(id: &Hash) -> Option<Payout> {
    PAYOUTS
        .with_borrow(|t| t.get(&id.into_array()))
        .map(|p| p.0)
}

pub(crate) fn save_payout(id: &Hash, p: &Payout) {
    PAYOUTS.with_borrow_mut(|t| t.insert(id.into_array(), Stored(p.clone())));
}

pub(crate) fn wallet(id: Principal) -> Option<WalletRecord> {
    WALLETS
        .with_borrow(|t| t.get(&id.as_slice().to_vec()))
        .map(|w| w.0)
}

pub(crate) fn wallet_count() -> u64 {
    WALLETS.with_borrow(|t| t.len())
}

fn member_prefix(m: &MemberId) -> Vec<u8> {
    let bytes = canonical(m);
    [vec![bytes.len() as u8], bytes].concat()
}

/// Save a wallet record and replace its member index entries.
pub(crate) fn save_wallet(old: Option<&WalletRecord>, w: &WalletRecord) {
    INDEX.with_borrow_mut(|t| {
        for m in old.map(|o| o.members.as_slice()).unwrap_or_default() {
            t.remove(&[member_prefix(m), w.wallet.as_slice().to_vec()].concat());
        }
        for m in &w.members {
            t.insert([member_prefix(m), w.wallet.as_slice().to_vec()].concat(), 1);
        }
    });
    WALLETS.with_borrow_mut(|t| t.insert(w.wallet.as_slice().to_vec(), Stored(w.clone())));
}

/// Wallets listing `member`, at most 256.
pub(crate) fn wallets_of(member: &MemberId) -> Vec<WalletRecord> {
    let prefix = member_prefix(member);
    let ids: Vec<Vec<u8>> = INDEX.with_borrow(|t| {
        t.range(prefix.clone()..)
            .take_while(|e| e.key().starts_with(&prefix))
            .take(256)
            .map(|e| e.key()[prefix.len()..].to_vec())
            .collect()
    });
    WALLETS.with_borrow(|t| ids.iter().filter_map(|id| t.get(id).map(|w| w.0)).collect())
}

fn created_key(account: &AccountId, quote: &Hash) -> Vec<u8> {
    [account.as_slice(), &quote[..]].concat()
}

pub(crate) fn hold_quota(account: &AccountId, quote: &Hash) {
    CREATED.with_borrow_mut(|t| t.insert(created_key(account, quote), 1));
}

pub(crate) fn release_quota(account: &AccountId, quote: &Hash) {
    CREATED.with_borrow_mut(|t| t.remove(&created_key(account, quote)));
}

/// Activations of `account` holding quota.
pub(crate) fn quota_used(account: &AccountId) -> u32 {
    let prefix = account.as_slice().to_vec();
    CREATED.with_borrow(|t| {
        t.range(prefix.clone()..)
            .take_while(|e| e.key().starts_with(&prefix))
            .count() as u32
    })
}

pub(crate) fn module(version: u32) -> Option<Vec<u8>> {
    MODULES
        .with_borrow(|t| t.get(&version))
        .map(|m| m.0.into_vec())
}

pub(crate) fn save_module(version: u32, bytes: Vec<u8>) {
    MODULES.with_borrow_mut(|t| t.insert(version, Stored(ByteBuf::from(bytes))));
}

pub(crate) fn staged() -> Vec<u8> {
    STAGING.with_borrow(|t| t.iter().flat_map(|e| e.value().0.into_vec()).collect())
}

pub(crate) fn staged_size() -> usize {
    STAGING.with_borrow(|t| t.iter().map(|e| e.value().0.len()).sum())
}

pub(crate) fn stage(chunk: Vec<u8>) {
    STAGING.with_borrow_mut(|t| {
        let next = t.last_key_value().map_or(0, |(k, _)| k + 1);
        t.insert(next, Stored(ByteBuf::from(chunk)));
    });
}

pub(crate) fn clear_staging() {
    STAGING.with_borrow_mut(|t| {
        let keys: Vec<u32> = t.iter().map(|e| *e.key()).collect();
        for k in keys {
            t.remove(&k);
        }
    });
}
