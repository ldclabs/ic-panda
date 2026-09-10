use crate::model;
use dmsg_runtime::storage::{CompactStored, MapExt};
use dmsg_runtime::Budget;
use dmsg_types::{cose::*, *};
use ic_cose_chain_key::PublicKey;
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, RestrictedMemory, StableBTreeMap, StableCell,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct Config {
    pub(crate) schema: u16,
    pub(crate) state: KeyState,
    pub(crate) keys: Vec<PublicKey>,
}

type Memory = VirtualMemory<DefaultMemoryImpl>;
type CellMemory = RestrictedMemory<Memory>;
// Share one default 128-page bucket: keep the small budget in its last page.
// This avoids allocating another 8 MiB just to persist three counters.
const BUDGET_PAGE: u64 = 127;
pub(crate) fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

thread_local! {
    pub(crate) static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    pub(crate) static CONFIG: RefCell<StableCell<CompactStored<Option<Config>>, CellMemory>> = RefCell::new(StableCell::init(RestrictedMemory::new(memory(0), 0..BUDGET_PAGE), CompactStored(None)));
    static EXECUTIONS: RefCell<StableBTreeMap<Vec<u8>, CompactStored<ExecutionResult>, Memory>> = RefCell::new(StableBTreeMap::init(memory(2)));
    pub(crate) static HOMES: RefCell<StableBTreeMap<Vec<u8>, CompactStored<model::Home>, Memory>> = RefCell::new(StableBTreeMap::init(memory(1)));
    static BUDGET: RefCell<StableCell<CompactStored<Budget>, CellMemory>> = RefCell::new(StableCell::init(RestrictedMemory::new(memory(0), BUDGET_PAGE..BUDGET_PAGE + 1), CompactStored(Budget::default())));
}

pub(crate) fn cfg() -> Config {
    CONFIG.with_borrow(|t| t.get().0.clone().expect("initialized"))
}

pub(crate) fn save_cfg(c: &Config) {
    CONFIG.with_borrow_mut(|t| t.set(CompactStored(Some(c.clone()))));
}

pub(crate) fn home(account_id: &AccountId) -> Result<model::Home> {
    HOMES.with_borrow(|t| t.load(account_id.as_slice()).ok_or(Error::NotFound))
}

/// Commit the bounded metadata and the one result changed in this message.
pub(crate) fn save_execution(
    account_id: &AccountId,
    h: &model::Home,
    sequence: u64,
    result: &ExecutionResult,
    removed: &[u64],
) {
    EXECUTIONS.with_borrow_mut(|t| {
        for seq in removed {
            t.delete(&execution_key(account_id, *seq));
        }
        t.put(&execution_key(account_id, sequence), result);
    });
    HOMES.with_borrow_mut(|t| t.put(account_id.as_slice(), h));
}

pub(crate) fn execution(account_id: &AccountId, sequence: u64) -> Result<ExecutionResult> {
    EXECUTIONS.with_borrow(|t| {
        t.load(&execution_key(account_id, sequence))
            .ok_or(Error::NotFound)
    })
}

pub(crate) fn reserve_budget(now: u64, cycles: u128, config: &CoseInit) -> Result<()> {
    BUDGET.with_borrow_mut(|t| {
        let mut budget = t.get().0.clone();
        budget.reserve(now, cycles, config.daily_executions, config.daily_cycles)?;
        t.set(CompactStored(budget));
        Ok(())
    })
}

pub(crate) const STABLE_SCHEMA: u16 = 4;

fn execution_key(account_id: &AccountId, sequence: u64) -> Vec<u8> {
    [account_id.as_slice(), &sequence.to_be_bytes()].concat()
}
