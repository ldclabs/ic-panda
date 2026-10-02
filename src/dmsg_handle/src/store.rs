use dmsg_protocol::*;
use dmsg_runtime::storage::{CompactStored, MapExt};
use dmsg_runtime::Certification;
use dmsg_types::{handle::*, *};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap, StableBTreeSet, StableCell, StableLog,
};
use std::cell::RefCell;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Config {
    pub(crate) schema: u16,
    pub(crate) init: HandleInit,
    pub(crate) progress: SnapshotProgress,
    pub(crate) event_tip: Hash,
    pub(crate) pending: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnershipReceipt {
    pub(crate) digest: Hash,
    pub(crate) record: HandleRecord,
}

type Memory = VirtualMemory<DefaultMemoryImpl>;

pub(crate) fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

thread_local! {
    // Allocate in 1 MiB buckets instead of the default 8 MiB per active memory.
    // The manager's 32,768 buckets then address up to 32 GiB of stable data.
    pub(crate) static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(
        MemoryManager::init_with_bucket_size(DefaultMemoryImpl::default(), 16),
    );
    static STABLE_CONFIG: RefCell<StableCell<CompactStored<Option<Config>>, Memory>> =
        RefCell::new(StableCell::init(memory(0), CompactStored::new(&None)));
    // Read-only paths borrow the decoded value; every change is persisted below.
    static CONFIG: RefCell<Option<Config>> =
        RefCell::new(STABLE_CONFIG.with_borrow(|t| t.get().value()));
    pub(crate) static NAMES: RefCell<StableBTreeMap<Vec<u8>, CompactStored<HandleRecord>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(1)));
    pub(crate) static LEGACY: RefCell<
        StableBTreeMap<Vec<u8>, CompactStored<LegacyReservation>, Memory>,
    > = RefCell::new(StableBTreeMap::init(memory(2)));
    pub(crate) static OPS: RefCell<
        StableBTreeMap<[u8; 32], CompactStored<HandleOperation>, Memory>,
    > = RefCell::new(StableBTreeMap::init(memory(3)));
    // Name -> operation key of the Charging/ChargeUnknown registration holding it.
    pub(crate) static LOCKS: RefCell<StableBTreeMap<Vec<u8>, [u8; 32], Memory>> =
        RefCell::new(StableBTreeMap::init(memory(4)));
    pub(crate) static EVENTS: StableLog<CompactStored<HandleEvent>, Memory, Memory> =
        StableLog::init(memory(5), memory(8));
    // Accounts with one unresolved registration charge.
    pub(crate) static ACTIVE_ACCOUNTS: RefCell<StableBTreeSet<[u8; 12], Memory>> =
        RefCell::new(StableBTreeSet::init(memory(6)));
    pub(crate) static OWNERSHIP: RefCell<
        StableBTreeMap<[u8; 32], CompactStored<OwnershipReceipt>, Memory>,
    > = RefCell::new(StableBTreeMap::init(memory(7)));
    pub(crate) static CERT: RefCell<Certification> = RefCell::new(Certification::default());
}

pub(crate) fn cfg() -> Config {
    with_cfg(Clone::clone)
}

pub(crate) fn with_cfg<R>(f: impl FnOnce(&Config) -> R) -> R {
    CONFIG.with_borrow(|c| f(c.as_ref().expect("initialized")))
}

pub(crate) fn save_cfg(c: Config) {
    STABLE_CONFIG.with_borrow_mut(|t| t.set(CompactStored::some(&c)));
    CONFIG.with_borrow_mut(|value| *value = Some(c));
}

pub(crate) fn record(name: &str) -> Option<HandleRecord> {
    NAMES.with_borrow(|t| t.load(name.as_bytes()))
}

pub(crate) fn op(key: &Hash) -> Result<HandleOperation> {
    OPS.with_borrow(|t| {
        t.get(&key.into_array())
            .map(CompactStored::into_inner)
            .ok_or(Error::NotFound)
    })
}

pub(crate) fn save_op(key: &Hash, o: &HandleOperation) {
    OPS.with_borrow_mut(|t| t.insert(key.into_array(), CompactStored::new(o)));
}

pub(crate) fn ownership_replay(key: &Hash, fingerprint: &Hash) -> Result<Option<HandleRecord>> {
    OWNERSHIP.with_borrow(|t| {
        let Some(receipt) = t.get(&key.into_array()).map(CompactStored::into_inner) else {
            return Ok(None);
        };
        ensure(receipt.digest == *fingerprint, Error::IdempotencyConflict)?;
        Ok(Some(receipt.record))
    })
}

pub(crate) fn save_ownership(key: &Hash, fingerprint: Hash, record: HandleRecord) {
    OWNERSHIP.with_borrow_mut(|t| {
        t.insert(
            key.into_array(),
            CompactStored::new(&OwnershipReceipt {
                digest: fingerprint,
                record,
            }),
        )
    });
}

pub(crate) fn op_key(account_id: &AccountId, op: Hash) -> Hash {
    digest("dmsg/handle-operation/v1", &(account_id, op))
}

pub(crate) const STABLE_SCHEMA: u16 = 7;

// Largest active-name population verified by the Wasm upgrade capacity profile.
pub(crate) const MAX_ACTIVE_NAMES: u64 = 100_000;
