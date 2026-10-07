use crate::http;
use candid::Principal;
use dmsg_runtime::storage::{CompactStored, MapExt};
use dmsg_types::{agent::*, *};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use serde_bytes::ByteBuf;
use std::cell::RefCell;

// Stable layout: config=0, published documents=1, certified segment hashes=2,
// certification nodes=3.
pub(crate) type Memory = VirtualMemory<DefaultMemoryImpl>;

pub(crate) const STABLE_SCHEMA: u16 = 5;

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) schema: u16,
    pub(crate) init: DirectoryInit,
}

/// One published account. The exact document bytes are served as stored; the
/// certification map keeps only hashes in stable memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Record {
    pub(crate) home_user: Principal,
    pub(crate) version: u64,
    pub(crate) updated_at: u64,
    pub(crate) state_digest: Hash,
    pub(crate) document_digest: Hash,
    pub(crate) document: ByteBuf,
}

pub(crate) fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

thread_local! {
    static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    static CONFIG: RefCell<StableCell<CompactStored<Option<Config>>, Memory>> =
        RefCell::new(StableCell::init(memory(0), CompactStored::new(&None)));
    static RECORDS: RefCell<StableBTreeMap<Vec<u8>, CompactStored<Record>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(1)));
}

pub(crate) fn config() -> Config {
    CONFIG.with_borrow(|t| t.get().value().expect("initialized"))
}

pub(crate) fn save_config(c: &Config) {
    CONFIG.with_borrow_mut(|t| t.set(CompactStored::some(c)));
}

pub(crate) fn load(id: &AccountId) -> Option<Record> {
    RECORDS.with_borrow(|t| t.load(id.as_slice()))
}

pub(crate) fn count() -> u64 {
    RECORDS.with_borrow(|t| t.len())
}

/// Persist a publication and certify its response in the same message.
pub(crate) fn save(id: &AccountId, record: &Record) {
    RECORDS.with_borrow_mut(|t| t.put(id.as_slice(), record));
    http::certify_document(id, record.document_digest);
}
