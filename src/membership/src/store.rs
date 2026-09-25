//! Service pins, admission configuration and per-minute call counters; claims live in `claims`.
use candid::Principal;
use dmsg_runtime::{storage::Stored, Certification};
use dmsg_types::{integration_membership::PandaServiceConfig, membership::MembershipInit, *};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableCell,
};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeMap};
type Memory = VirtualMemory<DefaultMemoryImpl>;

/// A successful SNS verification is reused for one hour.
const SNS_FRESH_MS: u64 = 60 * MINUTE;

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub init: MembershipInit,
    pub service: Option<PandaServiceConfig>,
    pub sns_verified: bool,
    pub sns_verified_at_ms: u64,
    pub paused: bool,
    /// UTC hour of `applications`, the successful new applications in that hour.
    pub application_hour: u64,
    pub applications: u64,
}

impl Config {
    pub fn sns_fresh(&self, at: u64) -> bool {
        self.sns_verified && at < self.sns_verified_at_ms.saturating_add(SNS_FRESH_MS)
    }

    pub fn service(&self) -> Result<&PandaServiceConfig> {
        self.service
            .as_ref()
            .ok_or(Error::Unavailable("PANDA service unconfigured".into()))
    }
}

/// Heap-only window; an upgrade simply starts a new minute.
#[derive(Default)]
struct Limits {
    minute: u64,
    qualifications: u32,
    refreshes: u32,
    authorizations: u32,
    actors: BTreeMap<Principal, u32>,
}

thread_local! {
    static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init_with_bucket_size(DefaultMemoryImpl::default(), 16));
    static CONFIG: RefCell<StableCell<Stored<Option<Config>>, Memory>> =
        RefCell::new(StableCell::init(memory(0), Stored(None)));
    static LIMITS: RefCell<Limits> = RefCell::new(Limits::default());
    pub static CERT: RefCell<Certification> = RefCell::new(Certification::default());
}

pub(crate) fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

pub fn config() -> Config {
    CONFIG
        .with_borrow(|c| c.get().0.clone())
        .expect("membership initialized")
}

pub fn save_config(c: &Config) {
    CONFIG.with_borrow_mut(|m| m.set(Stored(Some(c.clone()))));
}

/// Configuration for a call made by the pinned governance canister.
pub fn governance(caller: Principal) -> Result<Config> {
    let c = config();
    ensure(caller == c.init.governance, Error::Forbidden)?;
    Ok(c)
}

pub enum CallBudget {
    Authorization(Principal),
    Qualification,
    Refresh,
}

fn take(used: &mut u32, limit: u32) -> Result<()> {
    ensure(*used < limit, Error::QuotaExceeded)?;
    *used += 1;
    Ok(())
}

pub fn reserve_call(at: u64, kind: CallBudget) -> Result<()> {
    LIMITS.with_borrow_mut(|c| {
        if c.minute != at / MINUTE {
            *c = Limits {
                minute: at / MINUTE,
                ..Default::default()
            };
        }
        match kind {
            CallBudget::Authorization(actor) => {
                ensure(c.authorizations < 200, Error::QuotaExceeded)?;
                take(c.actors.entry(actor).or_default(), 10)?;
                c.authorizations += 1;
                Ok(())
            }
            CallBudget::Qualification => take(&mut c.qualifications, 200),
            CallBudget::Refresh => take(&mut c.refreshes, 200),
        }
    })
}

pub fn rebuild() {
    CERT.with_borrow_mut(|c| {
        *c = Certification::default();
        crate::claims::certify_all(c);
        c.publish();
    });
}
