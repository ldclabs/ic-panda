//! Small immutable service pins and bounded operational counters; claims live in the v2 tables.
use dmsg_runtime::{storage::Stored, Certification};
use dmsg_types::{membership::MembershipInit, *};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableCell,
};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeMap};
type Memory = VirtualMemory<DefaultMemoryImpl>;
#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub init: MembershipInit,
    pub sns_verified: bool,
    pub sns_verified_at_ms: u64,
    pub paused: bool,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Limits {
    minute: u64,
    qualifications: u32,
    refreshes: u32,
    authorizations: BTreeMap<candid::Principal, u32>,
}

thread_local! {
 static MEMORY:RefCell<MemoryManager<DefaultMemoryImpl>>=RefCell::new(MemoryManager::init_with_bucket_size(DefaultMemoryImpl::default(),16));
 static CONFIG:RefCell<StableCell<Stored<Option<Config>>,Memory>>=RefCell::new(StableCell::init(memory(0),Stored(None)));
 static STABLE_LIMITS:RefCell<StableCell<Stored<Limits>,Memory>>=RefCell::new(StableCell::init(memory(1),Stored(Limits::default())));
 static LIMITS:RefCell<Limits>=RefCell::new(STABLE_LIMITS.with_borrow(|c|c.get().0.clone()));
 pub static CERT:RefCell<Certification>=RefCell::new(Certification::default());
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

pub fn persist_limits() {
    LIMITS.with_borrow(|v| STABLE_LIMITS.with_borrow_mut(|m| m.set(Stored(v.clone()))));
}

pub enum CallBudget {
    Authorization(candid::Principal),
    Qualification,
    Refresh,
}

pub fn reserve_call(at: u64, kind: CallBudget) -> Result<()> {
    LIMITS.with_borrow_mut(|c| {
        if c.minute != at / MINUTE {
            *c = Limits {
                minute: at / MINUTE,
                ..Default::default()
            };
        }
        let used = match kind {
            CallBudget::Authorization(actor) => {
                ensure(
                    c.authorizations.values().sum::<u32>() < 200,
                    Error::QuotaExceeded,
                )?;
                let v = c.authorizations.entry(actor).or_default();
                ensure(*v < 10, Error::QuotaExceeded)?;
                v
            }
            CallBudget::Qualification => &mut c.qualifications,
            CallBudget::Refresh => &mut c.refreshes,
        };
        ensure(*used < 200, Error::QuotaExceeded)?;
        *used += 1;
        Ok(())
    })
}

pub fn rebuild() {
    CERT.with_borrow_mut(|c| {
        *c = Certification::default();
        crate::v2::rebuild(c);
        c.publish();
    });
}
