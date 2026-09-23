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
use std::{cell::RefCell, collections::BTreeMap};

type Memory = VirtualMemory<DefaultMemoryImpl>;

fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub init: CommerceInit,
    pub paused: bool,
    pub ledger_verified: bool,
    pub day: u64,
    pub orders: u32,
    pub minute: u64,
    pub reads: u32,
    pub refreshes: u32,
    pub authorizations: BTreeMap<candid::Principal, u32>,
    pub ledger_fee: u128,
}

thread_local! {
    static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    static STABLE_CONFIG: RefCell<StableCell<CompactStored<Option<Config>>, Memory>> =
        RefCell::new(StableCell::init(memory(0), CompactStored::new(&None)));
    static CONFIG: RefCell<Option<Config>> = RefCell::new(STABLE_CONFIG.with_borrow(|t| t.get().value()));
    static CATALOG_CACHE: RefCell<Vec<Catalog>> = const { RefCell::new(Vec::new()) };
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
    CONFIG.with_borrow(|c| c.clone().expect("initialized"))
}

pub fn save_config(c: &Config) {
    CONFIG.with_borrow_mut(|value| *value = Some(c.clone()));
}

/// Heap configuration/budgets commit at ordinary message boundaries; persist before upgrades.
pub fn persist_config() {
    STABLE_CONFIG.with_borrow_mut(|t| t.set(CompactStored::new(&Some(config()))));
}

pub enum CallBudget {
    Funds,
    Refresh,
    Authorization(candid::Principal),
}

pub fn reserve_call(at: u64, kind: CallBudget) -> Result<()> {
    CONFIG.with_borrow_mut(|value| {
        let c = value.as_mut().expect("initialized");
        let minute = at / dmsg_types::MINUTE;
        if c.minute != minute {
            c.minute = minute;
            c.reads = 0;
            c.refreshes = 0;
            c.authorizations.clear();
        }
        match kind {
            CallBudget::Funds => {
                ensure(c.reads < 400, Error::QuotaExceeded)?;
                c.reads += 1;
            }
            CallBudget::Refresh => {
                ensure(c.refreshes < 200, Error::QuotaExceeded)?;
                c.refreshes += 1;
            }
            CallBudget::Authorization(caller) => {
                ensure(
                    c.authorizations.values().sum::<u32>() < 200,
                    Error::QuotaExceeded,
                )?;
                let used = c.authorizations.entry(caller).or_default();
                ensure(*used < 10, Error::QuotaExceeded)?;
                *used += 1;
            }
        }
        Ok(())
    })
}

pub fn reserve_order(at: u64) -> Result<()> {
    CONFIG.with_borrow_mut(|value| {
        let c = value.as_mut().expect("initialized");
        if c.day != at / dmsg_types::DAY {
            c.day = at / dmsg_types::DAY;
            c.orders = 0;
        }
        ensure(c.orders < c.init.daily_orders, Error::QuotaExceeded)?;
        c.orders += 1;
        Ok(())
    })
}

pub fn load(b: &Beneficiary) -> Result<Subject> {
    SUBJECTS.with_borrow(|t| t.load(entitlement_key(b).as_slice()).ok_or(Error::NotFound))
}

pub fn save_subject(s: &Subject) {
    SUBJECTS.with_borrow_mut(|t| t.put(entitlement_key(&s.beneficiary).as_slice(), s));
}

pub fn save(s: &Subject) {
    save_subject(s);
    if let Some(v) = &s.view {
        certify(entitlement_key(&s.beneficiary).to_vec(), v);
    }
}

pub fn order(id: Hash) -> Result<BillingOrder> {
    ORDERS.with_borrow(|t| t.load(id.as_slice()).ok_or(Error::NotFound))
}

pub fn save_order(o: &BillingOrder) {
    assert!(conserved(o), "merchant funds conservation");
    ORDERS.with_borrow_mut(|t| t.put(o.order_id.as_slice(), o));
    certify(order_key(o.order_id).to_vec(), o);
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

pub fn save_catalog(c: &Catalog) {
    CATALOGS.with_borrow_mut(|t| t.put(&c.version.to_be_bytes(), c));
    CATALOG_CACHE.with_borrow_mut(|cache| cache.push(c.clone()));
}

pub fn load_catalogs() {
    CATALOG_CACHE.with_borrow_mut(|cache| {
        *cache =
            CATALOGS.with_borrow(|t| t.page(vec![], 256).into_iter().map(|(_, c)| c).collect());
    });
}

pub fn latest_catalog() -> Catalog {
    CATALOG_CACHE.with_borrow(|cache| cache.last().expect("initial catalog").clone())
}

pub fn catalog(at: u64) -> Catalog {
    CATALOG_CACHE.with_borrow(|cache| {
        let i = cache.partition_point(|c| c.effective_at_ms <= at);
        cache[i.saturating_sub(1)].clone()
    })
}

/// Internal bookkeeping must not republish an unchanged resource leaf.
fn certify<T: Serialize>(key: Vec<u8>, value: &T) {
    let bytes = dmsg_protocol::canonical(value);
    CERT.with_borrow_mut(|c| {
        if c.0.get(&key) != Some(&bytes) {
            c.0.insert(key, bytes);
            c.publish();
        }
    });
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
