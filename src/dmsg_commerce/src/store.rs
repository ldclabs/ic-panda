use crate::model::Subject;
use candid::Principal;
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

pub(crate) fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

/// Service configuration and per-period call budgets. The initial catalog lives in CATALOGS.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub environment: Environment,
    pub governance: Principal,
    pub membership_canister: Principal,
    pub user_homes: Vec<Principal>,
    pub max_subjects: u64,
    pub daily_orders: u32,
    pub paused: bool,
    pub day: u64,
    pub orders: u32,
    pub minute: u64,
    pub reads: u32,
    pub refreshes: u32,
    pub authorizations: BTreeMap<Principal, u32>,
}

impl Config {
    pub fn new(init: CommerceInit) -> Self {
        Self {
            environment: init.environment,
            governance: init.governance,
            membership_canister: init.membership_canister,
            user_homes: init.user_homes,
            max_subjects: init.max_subjects,
            daily_orders: init.daily_orders,
            paused: false,
            day: 0,
            orders: 0,
            minute: 0,
            reads: 0,
            refreshes: 0,
            authorizations: BTreeMap::new(),
        }
    }
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
    pub static CATALOGS: RefCell<StableBTreeMap<Vec<u8>, Stored<Catalog>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(6)));
    pub static CERT: RefCell<Certification> = RefCell::new(Certification::default());
}

/// Read configuration fields without cloning the whole record.
pub fn config<R>(f: impl FnOnce(&Config) -> R) -> R {
    CONFIG.with_borrow(|c| f(c.as_ref().expect("initialized")))
}

fn config_mut<R>(f: impl FnOnce(&mut Config) -> R) -> R {
    CONFIG.with_borrow_mut(|c| f(c.as_mut().expect("initialized")))
}

pub fn set_config(c: Config) {
    CONFIG.with_borrow_mut(|value| *value = Some(c));
}

/// Budgets commit at ordinary message boundaries; governance changes and upgrades persist.
pub fn persist_config() {
    CONFIG.with_borrow(|c| {
        STABLE_CONFIG.with_borrow_mut(|t| t.set(CompactStored::new(c)));
    });
}

pub fn set_paused(paused: bool) {
    config_mut(|c| c.paused = paused);
    persist_config();
}

pub fn check_governance(caller: Principal) -> Result<()> {
    ensure(caller == config(|c| c.governance), Error::Forbidden)
}

pub enum CallBudget {
    Funds,
    Refresh,
    Authorization(Principal),
}

pub fn reserve_call(at: u64, kind: CallBudget) -> Result<()> {
    config_mut(|c| {
        let minute = at / MINUTE;
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
    config_mut(|c| {
        if c.day != at / DAY {
            c.day = at / DAY;
            c.orders = 0;
        }
        ensure(c.orders < c.daily_orders, Error::QuotaExceeded)?;
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
pub fn certify<T: Serialize>(key: Vec<u8>, value: &T) {
    let bytes = dmsg_protocol::canonical(value);
    CERT.with_borrow_mut(|c| {
        if c.get(&key) != Some(bytes.as_slice()) {
            c.insert(key, bytes);
        }
    });
}

pub fn rebuild(at: u64) {
    CERT.with_borrow_mut(|c| {
        crate::registrations::rebuild(c);
        crate::checkout::rebuild(c);
        SUBJECTS.with_borrow(|t| {
            t.for_each(|key, s| {
                if let Some(v) = s.view {
                    c.set(key, dmsg_protocol::canonical(&v));
                }
            })
        });
        c.set(
            catalog_key().to_vec(),
            dmsg_protocol::canonical(&catalog(at)),
        );
        c.publish();
    });
}
