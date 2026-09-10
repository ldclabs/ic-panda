use crate::state::*;
use dmsg_protocol::{canonical, execution_receipt_key};
use dmsg_runtime::storage::{CompactStored, MapExt, Stored};
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
    pub(crate) static CONFIG: RefCell<StableCell<CompactStored<Option<Config>>, Memory>> = RefCell::new(StableCell::init(memory(0), CompactStored(None)));
    pub(crate) static ACCOUNTS: RefCell<StableBTreeMap<Vec<u8>, CompactStored<AccountState>, Memory>> = RefCell::new(StableBTreeMap::init(memory(1)));
    pub(crate) static AUTH: RefCell<StableBTreeMap<Vec<u8>, Stored<AccountId>, Memory>> = RefCell::new(StableBTreeMap::init(memory(2)));
    pub(crate) static BINDINGS: RefCell<StableBTreeMap<Vec<u8>, Stored<PendingBinding>, Memory>> = RefCell::new(StableBTreeMap::init(memory(3)));
    pub(crate) static EXECUTIONS: RefCell<StableBTreeMap<Vec<u8>, CompactStored<AuthorizedExecution>, Memory>> = RefCell::new(StableBTreeMap::init(memory(5)));
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
    ACCOUNTS.with_borrow(|t| t.load(id.as_slice()).ok_or(Error::NotFound))
}

/// Persist internal bookkeeping without changing the public security snapshot.
pub(crate) fn save_account(s: &AccountState) {
    ACCOUNTS.with_borrow_mut(|t| t.put(s.account_id.as_slice(), s));
}

pub(crate) fn save(s: &AccountState) {
    save_account(s);
    CERT.with_borrow_mut(|c| {
        c.put(
            s.account_id.to_vec(),
            &s.snapshot(&config().init.issuer_namespace),
        )
    });
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

pub(crate) fn save_execution(execution: &AuthorizedExecution) {
    EXECUTIONS.with_borrow_mut(|t| {
        t.put(
            &execution_key(&execution.grant.account_id, &execution.grant.request_id),
            execution,
        )
    });
    if let Ok(receipt) = crate::execution::receipt(execution, &config().init.issuer_namespace) {
        CERT.with_borrow_mut(|c| {
            c.put(
                execution_receipt_key(&receipt.account_id, receipt.request_id),
                &receipt,
            )
        });
    }
}

pub(crate) fn remove_execution(account_id: &AccountId, request_id: &OpId) {
    EXECUTIONS.with_borrow_mut(|t| t.delete(&execution_key(account_id, request_id)));
    CERT.with_borrow_mut(|c| c.remove(&execution_receipt_key(account_id, *request_id)));
}

pub(crate) const STABLE_SCHEMA: u16 = 4;

pub(crate) fn rebuild_certification() {
    let namespace = config().init.issuer_namespace;
    CERT.with_borrow_mut(|c| {
        ACCOUNTS.with_borrow(|t| {
            t.for_each(|key, s| {
                c.0.insert(key, canonical(&s.snapshot(&namespace)));
            });
        });
        EXECUTIONS.with_borrow(|t| {
            t.for_each(|_, execution| {
                if let Ok(receipt) = crate::execution::receipt(&execution, &namespace) {
                    c.0.insert(
                        execution_receipt_key(&receipt.account_id, receipt.request_id),
                        canonical(&receipt),
                    );
                }
            });
        });
        // Upgrades are atomic; publish once after rebuilding both views.
        c.publish();
    });
}
