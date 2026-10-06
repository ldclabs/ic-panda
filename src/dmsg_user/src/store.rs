use crate::state::*;
use dmsg_protocol::{canonical, execution_receipt_key};
use dmsg_runtime::cert_map::CertMap;
use dmsg_runtime::storage::{CompactStored, MapExt, Stored};
use dmsg_types::{user::*, *};
use ic_auth_types::XidGenerator;
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

// Stable layout: config=0, accounts=1, permanent auth routes=2, pending bindings=3,
// retired memory=4, execution records=5, monthly usage=6, external approvals=7,
// principals=8, certified leaf hashes=9, certification nodes=10.
type PendingBinding = (AccountId, Hash, u64); // account_id, nonce, expiry
type Memory = VirtualMemory<DefaultMemoryImpl>;

pub(crate) fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

thread_local! {
    pub(crate) static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    pub(crate) static CONFIG: RefCell<StableCell<CompactStored<Option<Config>>, Memory>> =
        RefCell::new(StableCell::init(memory(0), CompactStored::new(&None)));
    pub(crate) static ACCOUNTS: RefCell<
        StableBTreeMap<Vec<u8>, CompactStored<AccountState>, Memory>,
    > = RefCell::new(StableBTreeMap::init(memory(1)));
    pub(crate) static AUTH: RefCell<StableBTreeMap<Vec<u8>, Stored<AccountId>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(2)));
    pub(crate) static BINDINGS: RefCell<StableBTreeMap<Vec<u8>, Stored<PendingBinding>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(3)));
    pub(crate) static EXECUTIONS: RefCell<
        StableBTreeMap<Vec<u8>, CompactStored<AuthorizedExecution>, Memory>,
    > = RefCell::new(StableBTreeMap::init(memory(5)));
    // Hashes only: each certified value is rebuilt from its record for a witness.
    pub(crate) static CERT: RefCell<CertMap<Memory>> =
        RefCell::new(CertMap::new(memory(9), memory(10)));
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
    CONFIG.with_borrow(|t| t.get().value().expect("initialized"))
}

pub(crate) fn load(id: &AccountId) -> Result<AccountState> {
    ACCOUNTS.with_borrow(|t| t.load(id.as_slice()).ok_or(Error::NotFound))
}

/// Persist internal bookkeeping without changing the public security snapshot.
pub(crate) fn save_account(s: &AccountState) {
    ACCOUNTS.with_borrow_mut(|t| t.put(s.account_id.as_slice(), s));
}

pub(crate) fn save(s: &AccountState) {
    save_account(s);
    let snapshot = canonical(&s.snapshot(&config().init.issuer_namespace));
    CERT.with_borrow_mut(|c| c.insert(s.account_id.to_vec(), &snapshot));
}

fn execution_key(account_id: &AccountId, request_id: &OpId) -> Vec<u8> {
    [account_id.as_slice(), request_id.as_slice()].concat()
}

pub(crate) fn load_execution(
    account_id: &AccountId,
    request_id: &OpId,
) -> Option<AuthorizedExecution> {
    EXECUTIONS.with_borrow(|t| t.load(&execution_key(account_id, request_id)))
}

/// Persist an execution and update its certified receipt leaf.
pub(crate) fn save_execution(execution: &AuthorizedExecution) {
    EXECUTIONS.with_borrow_mut(|t| {
        t.put(
            &execution_key(&execution.grant.account_id, &execution.grant.request_id),
            execution,
        )
    });
    if let Ok(receipt) = crate::execution::receipt(execution, &config().init.issuer_namespace) {
        CERT.with_borrow_mut(|c| {
            c.insert(
                execution_receipt_key(&receipt.account_id, receipt.request_id),
                &canonical(&receipt),
            )
        });
    }
}

pub(crate) fn remove_execution(account_id: &AccountId, request_id: &OpId) {
    EXECUTIONS.with_borrow_mut(|t| t.delete(&execution_key(account_id, request_id)));
    CERT.with_borrow_mut(|c| c.remove(&execution_receipt_key(account_id, *request_id)));
}

/// Only terminal results whose retention deadline passed may be evicted.
/// Sequences, receipts and monthly settlement counters remain unchanged.
pub(crate) fn prune_account_executions(s: &mut AccountState, now: u64) -> u32 {
    let expired: Vec<_> = s
        .execution_expirations
        .iter()
        .filter_map(|(id, deadline)| deadline.filter(|at| *at <= now).map(|_| *id))
        .collect();
    for id in &expired {
        remove_execution(&s.account_id, id);
        s.execution_expirations.remove(id);
    }
    expired.len() as u32
}

pub(crate) const STABLE_SCHEMA: u16 = 11;
