//! Stable state of the hub: configuration, quotes, activations, payouts,
//! wallet records, the member index and wallet modules.
use candid::Principal;
use cbor2::Cbor;
use dmsg_protocol::canonical;
use dmsg_runtime::{
    stable_types::{is_zero, wallet::WalletSpecRepr},
    storage::{CompactStored, StableCodec, Stored},
};
use dmsg_types::{account::account_cbor, wallet::*, *};
use ic_stable_structures::{
    DefaultMemoryImpl, StableBTreeMap, StableCell,
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
};
use icrc_ledger_types::icrc1::account::Account;
use serde_bytes::ByteBuf;
use std::{
    cell::RefCell,
    ops::Bound::{Excluded, Unbounded},
};

// Stable layout: hub=0, quotes=1, orders=2, payouts=3, wallet records=4,
// member index=5, quota holds=6, modules=7, staging=8, uncollected orders=9.
type Memory = VirtualMemory<DefaultMemoryImpl>;

pub(crate) const STABLE_SCHEMA: u16 = 1;

/// Largest wallet module the hub keeps.
pub(crate) const MAX_MODULE: usize = 4 * 1024 * 1024;
/// Versions kept, retired ones included.
pub(crate) const MAX_VERSIONS: usize = 16;

/// The hub record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Hub {
    pub(crate) schema: u16,
    pub(crate) init: HubInit,
    pub(crate) versions: Vec<WalletVersion>,
    pub(crate) current: Option<u32>,
    /// Number of the next registered version; removed numbers are not reused.
    pub(crate) next_version: u32,
    pub(crate) uploader: Option<Principal>,
}

/// An activation without its quote, which is stored once under the same key.
#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub(crate) struct OrderRecord {
    #[cbor(key = 1)]
    spec: WalletSpecRepr,
    #[cbor(key = 2)]
    status: ActivationStatus,
    #[cbor(key = 3)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wallet: Option<Principal>,
    #[cbor(key = 4)]
    #[serde(default, skip_serializing_if = "is_zero")]
    collected: bool,
    #[cbor(key = 5)]
    updated_at_ms: u64,
}

impl OrderRecord {
    pub(crate) fn new(o: &ActivationOrder) -> Self {
        Self {
            spec: o.spec.to_repr(),
            status: o.status.clone(),
            wallet: o.wallet,
            collected: o.collected,
            updated_at_ms: o.updated_at_ms,
        }
    }

    pub(crate) fn order(self, quote: ActivationQuote) -> ActivationOrder {
        ActivationOrder {
            quote,
            spec: WalletSpec::from_repr(self.spec),
            status: self.status,
            wallet: self.wallet,
            collected: self.collected,
            updated_at_ms: self.updated_at_ms,
        }
    }
}

/// One ledger transfer of a payout with its fixed arguments.
#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub(crate) struct PayoutTransfer {
    #[cbor(key = 1)]
    #[serde(with = "account_cbor")]
    pub(crate) to: Account,
    #[cbor(key = 2)]
    pub(crate) amount: u128,
    #[cbor(key = 3)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) memo: Option<ByteBuf>,
    #[cbor(key = 4)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) block: Option<u64>,
}

/// Transfers out of a deposit: a refund, or revenue split between hub
/// cycles and the revenue account.
#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub(crate) struct Payout {
    #[cbor(key = 1)]
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(crate) refund: bool,
    #[cbor(key = 2)]
    pub(crate) created_at_time: u64,
    #[cbor(key = 3)]
    pub(crate) transfers: Vec<PayoutTransfer>,
    /// Cycles the CMC minted for the hub from the first transfer; zero when
    /// it returned the ICP.
    #[cbor(key = 4)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cycles: Option<u128>,
}

pub(crate) fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

thread_local! {
    static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    static STABLE_HUB: RefCell<StableCell<CompactStored<Option<Hub>>, Memory>> =
        RefCell::new(StableCell::init(memory(0), CompactStored::new(&None)));
    // Every message reads the record, so it stays decoded on the heap.
    static HUB: RefCell<Option<Hub>> =
        RefCell::new(STABLE_HUB.with_borrow(|c| c.get().value()));
    static QUOTES: RefCell<StableBTreeMap<[u8; 32], CompactStored<ActivationQuote>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(1)));
    static ORDERS: RefCell<StableBTreeMap<[u8; 32], Stored<OrderRecord>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(2)));
    static PAYOUTS: RefCell<StableBTreeMap<[u8; 32], Stored<Payout>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(3)));
    static WALLETS: RefCell<StableBTreeMap<Vec<u8>, CompactStored<WalletRecord>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(4)));
    /// canonical(MemberId) length, canonical(MemberId), wallet bytes.
    static INDEX: RefCell<StableBTreeMap<Vec<u8>, (), Memory>> =
        RefCell::new(StableBTreeMap::init(memory(5)));
    /// Creating account, then quote ID, of every activation holding quota.
    static HOLDS: RefCell<StableBTreeMap<[u8; 44], (), Memory>> =
        RefCell::new(StableBTreeMap::init(memory(6)));
    static MODULES: RefCell<StableBTreeMap<u32, Vec<u8>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(7)));
    static STAGING: RefCell<StableBTreeMap<u32, Vec<u8>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(8)));
    /// Quote IDs of ready activations whose payment is not yet revenue.
    static UNCOLLECTED: RefCell<StableBTreeMap<[u8; 32], (), Memory>> =
        RefCell::new(StableBTreeMap::init(memory(9)));
}

pub(crate) fn with<R>(f: impl FnOnce(&Hub) -> R) -> R {
    HUB.with_borrow(|h| f(h.as_ref().expect("installed")))
}

pub(crate) fn hub() -> Hub {
    with(Clone::clone)
}

pub(crate) fn save(h: Hub) {
    STABLE_HUB.with_borrow_mut(|c| c.set(CompactStored::some(&h)));
    HUB.with_borrow_mut(|cell| *cell = Some(h));
}

pub(crate) fn quote(id: &Hash) -> Option<ActivationQuote> {
    QUOTES
        .with_borrow(|t| t.get(&id.into_array()))
        .map(CompactStored::into_inner)
}

pub(crate) fn save_quote(q: &ActivationQuote) {
    QUOTES.with_borrow_mut(|t| t.insert(q.quote_id.into_array(), CompactStored::new(q)));
}

/// Quotes issued; none is ever removed.
pub(crate) fn quote_count() -> u64 {
    QUOTES.with_borrow(|t| t.len())
}

pub(crate) fn order(id: &Hash) -> Option<ActivationOrder> {
    let record = ORDERS.with_borrow(|t| t.get(&id.into_array()))?.0;
    Some(record.order(quote(id).expect("quote of an order")))
}

/// Save an order and keep the uncollected index in step with it.
pub(crate) fn save_order(o: &ActivationOrder) {
    let id = o.quote.quote_id.into_array();
    UNCOLLECTED.with_borrow_mut(|t| {
        if o.status == ActivationStatus::Ready && !o.collected {
            t.insert(id, ());
        } else {
            t.remove(&id);
        }
    });
    ORDERS.with_borrow_mut(|t| t.insert(id, Stored(OrderRecord::new(o))));
}

pub(crate) fn order_count() -> u64 {
    ORDERS.with_borrow(|t| t.len())
}

/// Quote IDs of uncollected ready activations after `after`.
pub(crate) fn uncollected(after: Option<Hash>, limit: usize) -> Vec<Hash> {
    UNCOLLECTED.with_borrow(|t| {
        let start = after.map_or(Unbounded, |h| Excluded(h.into_array()));
        t.range((start, Unbounded))
            .take(limit)
            .map(|e| Hash::new(*e.key()))
            .collect()
    })
}

pub(crate) fn uncollected_count() -> u64 {
    UNCOLLECTED.with_borrow(|t| t.len())
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
        .map(CompactStored::into_inner)
}

pub(crate) fn wallet_count() -> u64 {
    WALLETS.with_borrow(|t| t.len())
}

/// Wallet records by canister ID after `after`.
pub(crate) fn wallets_after(after: Option<Principal>, limit: usize) -> Vec<WalletRecord> {
    WALLETS.with_borrow(|t| {
        let start = after.map_or(Unbounded, |p| Excluded(p.as_slice().to_vec()));
        t.range((start, Unbounded))
            .take(limit)
            .map(|e| e.value().into_inner())
            .collect()
    })
}

fn member_prefix(m: &MemberId) -> Vec<u8> {
    let bytes = canonical(m);
    [vec![bytes.len() as u8], bytes].concat()
}

fn index_key(m: &MemberId, wallet: Principal) -> Vec<u8> {
    [member_prefix(m), wallet.as_slice().to_vec()].concat()
}

/// Save a wallet record; only members it adds or drops touch the index.
pub(crate) fn save_wallet(old: Option<&WalletRecord>, w: &WalletRecord) {
    let before = old.map_or(&[][..], |o| o.members.as_slice());
    INDEX.with_borrow_mut(|t| {
        for m in before.iter().filter(|m| !w.members.contains(m)) {
            t.remove(&index_key(m, w.wallet));
        }
        for m in w.members.iter().filter(|m| !before.contains(m)) {
            t.insert(index_key(m, w.wallet), ());
        }
    });
    WALLETS.with_borrow_mut(|t| t.insert(w.wallet.as_slice().to_vec(), CompactStored::new(w)));
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
    WALLETS.with_borrow(|t| {
        ids.iter()
            .filter_map(|id| t.get(id).map(CompactStored::into_inner))
            .collect()
    })
}

fn hold_key(account: &AccountId, quote: &[u8; 32]) -> [u8; 44] {
    let mut key = [0; 44];
    key[..12].copy_from_slice(account.as_slice());
    key[12..].copy_from_slice(quote);
    key
}

pub(crate) fn hold_quota(account: &AccountId, quote: &Hash) {
    HOLDS.with_borrow_mut(|t| t.insert(hold_key(account, &quote.into_array()), ()));
}

pub(crate) fn release_quota(account: &AccountId, quote: &Hash) {
    HOLDS.with_borrow_mut(|t| t.remove(&hold_key(account, &quote.into_array())));
}

/// Activations of `account` holding quota.
pub(crate) fn quota_used(account: &AccountId) -> u32 {
    let range = hold_key(account, &[0; 32])..=hold_key(account, &[0xff; 32]);
    HOLDS.with_borrow(|t| t.range(range).count() as u32)
}

pub(crate) fn module(version: u32) -> Option<Vec<u8>> {
    MODULES.with_borrow(|t| t.get(&version))
}

pub(crate) fn save_module(version: u32, bytes: Vec<u8>) {
    MODULES.with_borrow_mut(|t| t.insert(version, bytes));
}

pub(crate) fn remove_module(version: u32) {
    MODULES.with_borrow_mut(|t| t.remove(&version));
}

pub(crate) fn staged() -> Vec<u8> {
    STAGING.with_borrow(|t| t.iter().flat_map(|e| e.value()).collect())
}

pub(crate) fn staged_size() -> usize {
    STAGING.with_borrow(|t| t.iter().map(|e| e.value().len()).sum())
}

pub(crate) fn stage(chunk: Vec<u8>) {
    STAGING.with_borrow_mut(|t| {
        let next = t.last_key_value().map_or(0, |(k, _)| k + 1);
        t.insert(next, chunk);
    });
}

pub(crate) fn clear_staging() {
    STAGING.with_borrow_mut(|t| t.clear_new());
}
