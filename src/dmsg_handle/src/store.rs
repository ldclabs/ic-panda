use dmsg_protocol::*;
use dmsg_runtime::storage::{CompactStored, MapExt, Stored};
use dmsg_runtime::Certification;
use dmsg_types::{handle::*, *};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct Config {
    pub(crate) schema: u16,
    pub(crate) init: HandleInit,
    pub(crate) progress: SnapshotProgress,
    pub(crate) event_count: u64,
    pub(crate) event_tip: Hash,
    pub(crate) pending: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TransferReceipt {
    pub(crate) digest: Hash,
    pub(crate) record: HandleRecord,
}

type Memory = VirtualMemory<DefaultMemoryImpl>;

pub(crate) fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

thread_local! {
    pub(crate) static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    pub(crate) static CONFIG: RefCell<StableCell<CompactStored<Option<Config>>, Memory>> = RefCell::new(StableCell::init(memory(0), CompactStored(None)));
    pub(crate) static NAMES: RefCell<StableBTreeMap<Vec<u8>, CompactStored<HandleRecord>, Memory>> = RefCell::new(StableBTreeMap::init(memory(1)));
    pub(crate) static LEGACY: RefCell<StableBTreeMap<Vec<u8>, CompactStored<LegacyReservation>, Memory>> = RefCell::new(StableBTreeMap::init(memory(2)));
    pub(crate) static OPS: RefCell<StableBTreeMap<Vec<u8>, CompactStored<HandleOperation>, Memory>> = RefCell::new(StableBTreeMap::init(memory(3)));
    pub(crate) static LOCKS: RefCell<StableBTreeMap<Vec<u8>, Stored<Hash>, Memory>> = RefCell::new(StableBTreeMap::init(memory(4)));
    pub(crate) static EVENTS: RefCell<StableBTreeMap<Vec<u8>, CompactStored<HandleEvent>, Memory>> = RefCell::new(StableBTreeMap::init(memory(5)));
    pub(crate) static SUBJECT_OPS: RefCell<StableBTreeMap<Vec<u8>, Stored<Hash>, Memory>> = RefCell::new(StableBTreeMap::init(memory(6)));
    pub(crate) static TRANSFERS: RefCell<StableBTreeMap<Vec<u8>, CompactStored<TransferReceipt>, Memory>> = RefCell::new(StableBTreeMap::init(memory(7)));
    pub(crate) static CERT:RefCell<Certification>=RefCell::new(Certification::default());
}

pub(crate) fn cfg() -> Config {
    CONFIG.with_borrow(|t| t.get().0.clone().expect("initialized"))
}

pub(crate) fn save_cfg(c: &Config) {
    CONFIG.with_borrow_mut(|t| t.set(CompactStored(Some(c.clone()))));
}

pub(crate) fn record(name: &str) -> Option<HandleRecord> {
    NAMES.with_borrow(|t| t.load(name.as_bytes()))
}

pub(crate) fn op(key: &Hash) -> Result<HandleOperation> {
    OPS.with_borrow(|t| t.load(key.as_slice()).ok_or(Error::NotFound))
}

pub(crate) fn save_op(key: &Hash, o: &HandleOperation) {
    OPS.with_borrow_mut(|t| t.put(key.as_slice(), o));
}

pub(crate) fn op_key(account_id: &AccountId, op: Hash) -> Hash {
    digest("dmsg/handle-operation/v1", &(account_id, op))
}

pub(crate) const STABLE_SCHEMA: u16 = 3;
