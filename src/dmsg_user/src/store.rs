use crate::state::*;
use dmsg_protocol::execution_receipt_key;
use dmsg_runtime::storage::{MapExt, Stored};
use dmsg_runtime::Certification;
use dmsg_types::{user::*, *};
use ic_auth_types::XidGenerator;
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

// Stable layout: config=0, accounts=1, permanent auth routes=2,
// pending bindings=3, retired memory=4, execution records=5.
type PendingBinding = (AccountId, Hash, u64); // account_id, nonce, expiry
type Memory = VirtualMemory<DefaultMemoryImpl>;
fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}
thread_local! {
    pub(crate) static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    pub(crate) static CONFIG: RefCell<StableCell<Stored<Option<Config>>, Memory>> = RefCell::new(StableCell::init(memory(0), Stored(None)));
    pub(crate) static ACCOUNTS: RefCell<StableBTreeMap<Vec<u8>, Stored<AccountState>, Memory>> = RefCell::new(StableBTreeMap::init(memory(1)));
    pub(crate) static AUTH: RefCell<StableBTreeMap<Vec<u8>, Stored<AccountId>, Memory>> = RefCell::new(StableBTreeMap::init(memory(2)));
    pub(crate) static BINDINGS: RefCell<StableBTreeMap<Vec<u8>, Stored<PendingBinding>, Memory>> = RefCell::new(StableBTreeMap::init(memory(3)));
    pub(crate) static EXECUTIONS: RefCell<StableBTreeMap<Vec<u8>, Stored<AuthorizedExecution>, Memory>> = RefCell::new(StableBTreeMap::init(memory(5)));
    pub(crate) static CERT: RefCell<Certification> = RefCell::new(Certification::default());
}
#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct Config {
    pub(crate) schema: u16,
    pub(crate) init: UserInit,
    pub(crate) allocator: XidGenerator,
    pub(crate) allocator_namespace_digest: Hash,
    pub(crate) day: u64,
    pub(crate) created_today: u32,
}
pub(crate) fn config() -> Config {
    CONFIG.with_borrow(|t| t.get().0.clone().expect("initialized"))
}
pub(crate) fn load(id: &AccountId) -> Result<AccountState> {
    let mut account = ACCOUNTS.with_borrow(|t| t.load(id.as_slice()).ok_or(Error::NotFound))?;
    account.executions = execution_records(id)
        .into_iter()
        .map(|(_, e)| (e.grant.request_id, e))
        .collect();
    Ok(account)
}
pub(crate) fn save(s: &AccountState) {
    for (key, previous) in execution_records(&s.account_id) {
        if !s.executions.contains_key(&previous.grant.request_id) {
            EXECUTIONS.with_borrow_mut(|t| t.delete(&key));
            CERT.with_borrow_mut(|c| {
                c.remove(&execution_receipt_key(
                    &s.account_id,
                    previous.grant.request_id,
                ))
            });
        }
    }
    for (id, execution) in &s.executions {
        let key = [s.account_id.as_slice(), id.as_slice()].concat();
        EXECUTIONS.with_borrow_mut(|t| {
            if t.load(&key).as_ref() != Some(execution) {
                t.put(&key, execution);
            }
        });
    }
    for execution in s.executions.values() {
        certify_execution(execution);
    }
    ACCOUNTS.with_borrow_mut(|t| t.put(s.account_id.as_slice(), s));
    CERT.with_borrow_mut(|c| {
        c.put(
            s.account_id.to_vec(),
            &s.snapshot(&config().init.issuer_namespace),
        )
    });
}

fn execution_records(account_id: &AccountId) -> Vec<(Vec<u8>, AuthorizedExecution)> {
    EXECUTIONS
        .with_borrow(|t| t.page(account_id.to_vec(), dmsg_runtime::WINDOW))
        .into_iter()
        .take_while(|(key, _)| key.starts_with(account_id.as_slice()))
        .collect()
}

pub(crate) const STABLE_SCHEMA: u16 = 2;

pub(crate) fn certify_execution(execution: &AuthorizedExecution) {
    if let Ok(receipt) = crate::execution::receipt(execution, &config().init.issuer_namespace) {
        CERT.with_borrow_mut(|c| {
            c.put(
                execution_receipt_key(&receipt.account_id, receipt.request_id),
                &receipt,
            )
        });
    }
}
