use crate::state::Escrow;
use candid::Principal;
use dmsg_runtime::storage::{MapExt, Stored};
use dmsg_runtime::Certification;
use dmsg_types::{payment::*, *};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct Config {
    pub(crate) schema: u16,
    pub(crate) init: PaymentInit,
    pub(crate) day: u64,
    pub(crate) orders_today: u32,
    pub(crate) ledger_minute: u64,
    pub(crate) ledger_reads: u32,
}
type Memory = VirtualMemory<DefaultMemoryImpl>;
pub(crate) fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}
thread_local! {
    pub(crate) static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    pub(crate) static CONFIG: RefCell<StableCell<Stored<Option<Config>>, Memory>> = RefCell::new(StableCell::init(memory(0), Stored(None)));
    pub(crate) static ESCROWS: RefCell<StableBTreeMap<Vec<u8>, Stored<Escrow>, Memory>> = RefCell::new(StableBTreeMap::init(memory(1)));
    pub(crate) static QUOTES: RefCell<StableBTreeMap<Vec<u8>, Stored<Hash>, Memory>> = RefCell::new(StableBTreeMap::init(memory(2)));
    pub(crate) static FUNDING: RefCell<StableBTreeMap<Vec<u8>, Stored<Hash>, Memory>> = RefCell::new(StableBTreeMap::init(memory(3)));
    pub(crate) static DEPOSITS: RefCell<StableBTreeMap<Vec<u8>, Stored<Deposit>, Memory>> = RefCell::new(StableBTreeMap::init(memory(4)));
    pub(crate) static LEGS: RefCell<StableBTreeMap<Vec<u8>, Stored<TransferLeg>, Memory>> = RefCell::new(StableBTreeMap::init(memory(5)));
    pub(crate) static SIGNERS: RefCell<StableBTreeMap<Vec<u8>, Stored<ReceiptSigner>, Memory>> = RefCell::new(StableBTreeMap::init(memory(6)));
    pub(crate) static PAYER_OPEN: RefCell<StableBTreeMap<Vec<u8>, Stored<u32>, Memory>> = RefCell::new(StableBTreeMap::init(memory(7)));
    pub(crate) static OUTGOING: RefCell<StableBTreeMap<Vec<u8>, Stored<Vec<u8>>, Memory>> = RefCell::new(StableBTreeMap::init(memory(8)));
    pub(crate) static PAYER_INDEX: RefCell<StableBTreeMap<Vec<u8>, Stored<Hash>, Memory>> = RefCell::new(StableBTreeMap::init(memory(9)));
    pub(crate) static CERT:RefCell<Certification>=RefCell::new(Certification::default());
}
pub(crate) fn cfg() -> Config {
    CONFIG.with_borrow(|t| t.get().0.clone().expect("initialized"))
}
pub(crate) fn save_cfg(c: &Config) {
    CONFIG.with_borrow_mut(|t| t.set(Stored(Some(c.clone()))));
}

pub(crate) fn load(id: &Hash) -> Result<Escrow> {
    ESCROWS.with_borrow(|t| t.load(id.as_slice()).ok_or(Error::NotFound))
}

pub(crate) fn save(e: &Escrow) {
    assert!(e.conserved(), "funds conservation");
    ESCROWS.with_borrow_mut(|t| t.put(e.escrow_id.as_slice(), e));
    CERT.with_borrow_mut(|c| c.put(e.escrow_id.to_vec(), &e.info()));
}

pub(crate) fn key(id: Hash, n: u64) -> Vec<u8> {
    [id.as_slice(), n.to_be_bytes().as_slice()].concat()
}

pub(crate) fn signer(epoch: u64) -> Result<ReceiptSigner> {
    SIGNERS.with_borrow(|t| t.load(&epoch.to_be_bytes()).ok_or(Error::NotFound))
}

pub(crate) fn open_count(p: Principal) -> u32 {
    PAYER_OPEN.with_borrow(|t| t.load(p.as_slice()).unwrap_or(0))
}

pub(crate) fn release_payer(e: &Escrow) {
    let n = open_count(e.payer_principal)
        .checked_sub(1)
        .expect("open accounting");
    PAYER_OPEN.with_borrow_mut(|t| t.put(e.payer_principal.as_slice(), &n));
}

pub(crate) fn put_leg(l: &TransferLeg) {
    LEGS.with_borrow_mut(|t| t.put(&key(l.escrow_id, l.leg_id), l));
}

pub(crate) fn get_leg(id: Hash, n: u64) -> Result<TransferLeg> {
    LEGS.with_borrow(|t| t.load(&key(id, n)).ok_or(Error::NotFound))
}

pub(crate) const STABLE_SCHEMA: u16 = 2;
