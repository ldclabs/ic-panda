use crate::model;
use dmsg_runtime::storage::{CompactStored, MapExt};
use dmsg_runtime::Budget;
use dmsg_types::{cose::*, *};
use ic_cose_chain_key::PublicKey;
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct Config {
    pub(crate) schema: u16,
    pub(crate) state: KeyState,
    pub(crate) keys: Vec<PublicKey>,
    pub(crate) budget: Budget,
}

type Memory = VirtualMemory<DefaultMemoryImpl>;
pub(crate) fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

thread_local! {
    pub(crate) static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    pub(crate) static CONFIG: RefCell<StableCell<CompactStored<Option<Config>>, Memory>> = RefCell::new(StableCell::init(memory(0), CompactStored(None)));
    static EXECUTIONS: RefCell<StableBTreeMap<Vec<u8>, CompactStored<model::Execution>, Memory>> = RefCell::new(StableBTreeMap::init(memory(2)));
    pub(crate) static HOMES: RefCell<StableBTreeMap<Vec<u8>, CompactStored<model::Home>, Memory>> = RefCell::new(StableBTreeMap::init(memory(1)));
}

pub(crate) fn cfg() -> Config {
    CONFIG.with_borrow(|t| t.get().0.clone().expect("initialized"))
}

pub(crate) fn save_cfg(c: &Config) {
    CONFIG.with_borrow_mut(|t| t.set(CompactStored(Some(c.clone()))));
}

pub(crate) fn home(account_id: &AccountId) -> Result<model::Home> {
    let mut home = HOMES.with_borrow(|t| t.load(account_id.as_slice()).ok_or(Error::NotFound))?;
    home.executions = records(account_id)
        .into_iter()
        .map(|(_, e)| (e.grant.execution_sequence, e))
        .collect();
    Ok(home)
}

pub(crate) fn save_home(account_id: &AccountId, h: &model::Home) {
    for (key, old) in records(account_id) {
        if !h.executions.contains_key(&old.grant.execution_sequence) {
            EXECUTIONS.with_borrow_mut(|t| t.delete(&key));
        }
    }
    for (seq, execution) in &h.executions {
        let key = [account_id.as_slice(), &seq.to_be_bytes()].concat();
        EXECUTIONS.with_borrow_mut(|t| {
            if t.load(&key).as_ref() != Some(execution) {
                t.put(&key, execution);
            }
        });
    }
    HOMES.with_borrow_mut(|t| t.put(account_id.as_slice(), h));
}

pub(crate) const STABLE_SCHEMA: u16 = 3;

fn records(account_id: &AccountId) -> Vec<(Vec<u8>, model::Execution)> {
    EXECUTIONS
        .with_borrow(|t| t.page(account_id.to_vec(), dmsg_runtime::WINDOW))
        .into_iter()
        .take_while(|(key, _)| key.starts_with(account_id.as_slice()))
        .collect()
}
