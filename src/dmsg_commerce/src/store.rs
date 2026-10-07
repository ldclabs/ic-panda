use crate::model::Subject;
use candid::Principal;
use dmsg_protocol::billing::*;
use dmsg_runtime::{
    cert_map::{leaf_hash, CertMap},
    storage::{CompactStored, MapExt, Stored},
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

/// Calls admitted in the current minute, in total and per caller.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    pub calls: u32,
    pub callers: BTreeMap<Principal, u32>,
}

/// Service configuration and per-period call budgets. The initial catalog lives in CATALOGS.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub environment: Environment,
    pub governance: Principal,
    pub membership_canister: Principal,
    pub user_homes: Vec<Principal>,
    pub limits: CommerceLimits,
    pub paused: bool,
    pub day: u64,
    pub orders: u32,
    pub minute: u64,
    pub reads: Budget,
    pub refreshes: Budget,
    pub authorizations: Budget,
}

impl Config {
    pub fn new(init: CommerceInit) -> Self {
        Self {
            environment: init.environment,
            governance: init.governance,
            membership_canister: init.membership_canister,
            user_homes: init.user_homes,
            limits: init.limits,
            paused: false,
            day: 0,
            orders: 0,
            minute: 0,
            reads: Budget::default(),
            refreshes: Budget::default(),
            authorizations: Budget::default(),
        }
    }
}

thread_local! {
    // Stable memory itself; on the host a shared vector the capacity image exports.
    pub(crate) static RAW: DefaultMemoryImpl = DefaultMemoryImpl::default();
    static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> =
        // 8 MiB buckets; 32,768 buckets address up to 256 GiB of stable data.
        RefCell::new(MemoryManager::init(RAW.with(Clone::clone)));
    static STABLE_CONFIG: RefCell<StableCell<CompactStored<Option<Config>>, Memory>> =
        RefCell::new(StableCell::init(memory(0), CompactStored::new(&None)));
    static CONFIG: RefCell<Option<Config>> = RefCell::new(STABLE_CONFIG.with_borrow(|t| t.get().value()));
    static CATALOG_CACHE: RefCell<Vec<Catalog>> = const { RefCell::new(Vec::new()) };
    pub static SUBJECTS: RefCell<StableBTreeMap<Vec<u8>, CompactStored<Subject>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(1)));
    pub static CATALOGS: RefCell<StableBTreeMap<Vec<u8>, Stored<Catalog>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(6)));
    // Hashes only: each certified value is rebuilt from its record for a witness.
    pub static CERT: RefCell<CertMap<Memory>> = RefCell::new(CertMap::new(memory(28), memory(29)));
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

pub fn check_admin(caller: Principal) -> Result<()> {
    dmsg_runtime::admin::check_admin(caller, config(|c| c.governance))
}

/// Returns false when the home is already listed. Beneficiary subjects name
/// their home as authority, so homes need no account-ID routing here.
pub fn check_user_home(homes: &[Principal], home: Principal) -> Result<bool> {
    if homes.contains(&home) {
        return Ok(false);
    }
    dmsg_protocol::authenticated(home)?;
    ensure(
        homes.len() < dmsg_protocol::agent::MAX_USER_HOMES,
        Error::QuotaExceeded,
    )?;
    Ok(true)
}

pub fn add_user_home(home: Principal) {
    config_mut(|c| c.user_homes.push(home));
    persist_config();
}

/// Upper bounds a governance limit may take; they bound stable memory and
/// per-minute work to what the capacity profile has measured.
pub const MAX_SUBJECTS: u64 = 10_000_000;
pub const MAX_ORDERS: u64 = 10_000_000;
pub const MAX_DAILY_ORDERS: u32 = 1_000_000;
pub const MAX_CALLS_PER_MINUTE: u32 = 100_000;

pub fn check_limits(l: &CommerceLimits) -> Result<()> {
    ensure_valid(
        (1..=MAX_SUBJECTS).contains(&l.max_subjects)
            && (1..=MAX_ORDERS).contains(&l.max_orders)
            && (1..=l.max_orders).contains(&l.max_hot_orders)
            && (1..=MAX_DAILY_ORDERS).contains(&l.daily_orders)
            && [
                l.calls_per_minute,
                l.authorizations_per_minute,
                l.refreshes_per_minute,
            ]
            .iter()
            .all(|n| (1..=MAX_CALLS_PER_MINUTE).contains(n))
            && (1..=l.calls_per_minute).contains(&l.calls_per_caller),
        "commerce limits",
    )
}

pub fn set_limits(limits: CommerceLimits) {
    config_mut(|c| c.limits = limits);
    persist_config();
}

pub enum CallBudget {
    Funds(Principal),
    Refresh(Principal),
    Authorization(Principal),
}

pub fn reserve_call(at: u64, kind: CallBudget) -> Result<()> {
    config_mut(|c| {
        let minute = at / MINUTE;
        if c.minute != minute {
            c.minute = minute;
            c.reads = Budget::default();
            c.refreshes = Budget::default();
            c.authorizations = Budget::default();
        }
        let l = c.limits.clone();
        // A user home serves all of its accounts, so it may use the whole refresh budget.
        let (budget, caller, global, per_caller) = match kind {
            CallBudget::Funds(caller) => {
                (&mut c.reads, caller, l.calls_per_minute, l.calls_per_caller)
            }
            CallBudget::Refresh(caller) if c.user_homes.contains(&caller) => (
                &mut c.refreshes,
                caller,
                l.refreshes_per_minute,
                l.refreshes_per_minute,
            ),
            CallBudget::Refresh(caller) => (&mut c.refreshes, caller, l.refreshes_per_minute, 20),
            CallBudget::Authorization(caller) => (
                &mut c.authorizations,
                caller,
                l.authorizations_per_minute,
                10,
            ),
        };
        ensure(budget.calls < global, Error::QuotaExceeded)?;
        let used = budget.callers.entry(caller).or_default();
        ensure(*used < per_caller.min(global), Error::QuotaExceeded)?;
        *used += 1;
        budget.calls += 1;
        Ok(())
    })
}

pub fn reserve_order(at: u64) -> Result<()> {
    config_mut(|c| {
        if c.day != at / DAY {
            c.day = at / DAY;
            c.orders = 0;
        }
        ensure(c.orders < c.limits.daily_orders, Error::QuotaExceeded)?;
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

pub fn next_catalog_at(at: u64) -> Option<u64> {
    CATALOG_CACHE.with_borrow(|cache| {
        cache
            .iter()
            .find(|c| c.effective_at_ms > at)
            .map(|c| c.effective_at_ms)
    })
}

pub fn same_catalog(a: u64, b: u64) -> bool {
    CATALOG_CACHE.with_borrow(|cache| {
        cache.partition_point(|c| c.effective_at_ms <= a)
            == cache.partition_point(|c| c.effective_at_ms <= b)
    })
}

/// Free quota history through the current observation. A scheduled future policy
/// does not rewrite an already issued month until that policy actually takes effect.
pub fn month_catalogs(month: u32, at: u64) -> Result<Vec<Catalog>> {
    let start = month_bounds(month)?.0;
    Ok(CATALOG_CACHE.with_borrow(|cache| {
        let first = cache
            .partition_point(|c| c.effective_at_ms <= start)
            .saturating_sub(1);
        let end = cache.partition_point(|c| c.effective_at_ms <= at);
        cache[first..end].to_vec()
    }))
}

/// Certify a public view; an unchanged leaf leaves the root untouched.
pub fn certify<T: Serialize>(key: Vec<u8>, value: &T) {
    CERT.with_borrow_mut(|c| c.insert(key, &dmsg_protocol::canonical(value)));
}

/// The catalog the certified leaf holds: the one effective at its last refresh.
pub fn certified_catalog() -> Option<Vec<u8>> {
    let hash = CERT.with_borrow(|c| c.get(catalog_key().as_slice()))?;
    CATALOG_CACHE.with_borrow(|cache| {
        cache
            .iter()
            .rev()
            .map(dmsg_protocol::canonical)
            .find(|bytes| leaf_hash(bytes) == hash)
    })
}

/// Certification nodes persist in stable memory. Only the catalog leaf, which
/// follows time, is refreshed before the root is republished.
pub fn publish_certification(at: u64) {
    certify(catalog_key().to_vec(), &catalog(at));
    CERT.with_borrow(|c| c.publish());
}
