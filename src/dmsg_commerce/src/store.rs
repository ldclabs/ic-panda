use crate::model::{conserved, Subject};
use dmsg_protocol::billing::*;
use dmsg_runtime::{
    storage::{CompactStored, MapExt, Stored},
    Certification,
};
use dmsg_types::{billing::*, membership::*, *};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

type Memory = VirtualMemory<DefaultMemoryImpl>;

fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub init: CommerceInit,
    pub paused: bool,
    pub ledger_verified: bool,
    pub day: u64,
    pub orders: u32,
    pub minute: u64,
    pub reads: u32,
}

thread_local! {
    static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    pub static CONFIG: RefCell<StableCell<CompactStored<Option<Config>>, Memory>> =
        RefCell::new(StableCell::init(memory(0), CompactStored::new(&None)));
    pub static SUBJECTS: RefCell<StableBTreeMap<Vec<u8>, CompactStored<Subject>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(1)));
    pub static ORDERS: RefCell<StableBTreeMap<Vec<u8>, Stored<BillingOrder>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(2)));
    pub static DEPOSITS: RefCell<StableBTreeMap<Vec<u8>, Stored<MerchantDeposit>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(3)));
    pub static TRANSFERS: RefCell<StableBTreeMap<Vec<u8>, Stored<MerchantTransfer>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(4)));
    pub static DECISIONS: RefCell<
        StableBTreeMap<Vec<u8>, Stored<MembershipDecisionReceipt>, Memory>,
    > = RefCell::new(StableBTreeMap::init(memory(5)));
    pub static CATALOGS: RefCell<StableBTreeMap<Vec<u8>, Stored<Catalog>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(6)));
    pub static OUTGOING: RefCell<StableBTreeMap<Vec<u8>, Stored<Vec<u8>>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(7)));
    pub static CERT: RefCell<Certification> = RefCell::new(Certification::default());
}

pub fn config() -> Config {
    CONFIG.with_borrow(|t| t.get().value().expect("initialized"))
}

pub fn save_config(c: &Config) {
    CONFIG.with_borrow_mut(|t| t.set(CompactStored::new(&Some(c.clone()))));
}

pub fn load(b: &Beneficiary) -> Result<Subject> {
    SUBJECTS.with_borrow(|t| t.load(entitlement_key(b).as_slice()).ok_or(Error::NotFound))
}

pub fn save(s: &Subject) {
    SUBJECTS.with_borrow_mut(|t| t.put(entitlement_key(&s.beneficiary).as_slice(), s));
    if let Some(v) = &s.view {
        CERT.with_borrow_mut(|c| c.put(entitlement_key(&s.beneficiary).to_vec(), v));
    }
}

pub fn order(id: Hash) -> Result<BillingOrder> {
    ORDERS.with_borrow(|t| t.load(id.as_slice()).ok_or(Error::NotFound))
}

pub fn save_order(o: &BillingOrder) {
    assert!(conserved(o), "merchant funds conservation");
    ORDERS.with_borrow_mut(|t| t.put(o.order_id.as_slice(), o));
    CERT.with_borrow_mut(|c| c.put(order_key(o.order_id).to_vec(), o));
}

pub fn key(id: Hash, n: u64) -> Vec<u8> {
    [id.as_slice(), &n.to_be_bytes()].concat()
}

pub fn transfer(id: Hash, n: u64) -> Result<MerchantTransfer> {
    TRANSFERS.with_borrow(|t| t.load(&key(id, n)).ok_or(Error::NotFound))
}

pub fn save_transfer(t: &MerchantTransfer) {
    TRANSFERS.with_borrow_mut(|m| m.put(&key(t.order_id, t.transfer_id), t));
}

pub fn catalog(at: u64) -> Catalog {
    let mut c = config().init.catalog;
    CATALOGS.with_borrow(|t| {
        t.for_each(|_, v| {
            if v.effective_at_ms <= at && v.effective_at_ms > c.effective_at_ms {
                c = v;
            }
        })
    });
    c
}

pub fn rebuild(at: u64) {
    CERT.with_borrow_mut(|c| {
        SUBJECTS.with_borrow(|t| {
            t.for_each(|key, s| {
                if let Some(v) = s.view {
                    c.0.insert(key, dmsg_protocol::canonical(&v));
                }
            })
        });
        ORDERS.with_borrow(|t| {
            t.for_each(|_, o| {
                c.0.insert(order_key(o.order_id).to_vec(), dmsg_protocol::canonical(&o));
            })
        });
        c.0.insert(
            catalog_key().to_vec(),
            dmsg_protocol::canonical(&catalog(at)),
        );
        c.publish();
    });
}
