use crate::state::*;
use dmsg_protocol::{canonical, digest, execution_receipt_key};
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

// Stable layout: config=0, accounts=1, permanent auth routes=2, hourly remote
// calls=3, retired memory=4, execution records=5, monthly usage=6, external
// approvals=7, principals=8, certified leaf hashes=9, certification nodes=10.
type Memory = VirtualMemory<DefaultMemoryImpl>;
type HourlyCalls = (u64, u32); // UTC hour, remote calls started in it
// Instructions after which a public cleanup page stops at an account boundary.
const PRUNE_INSTRUCTIONS: u64 = 20_000_000_000;

pub(crate) fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

thread_local! {
    // Stable memory itself; on the host a shared vector the capacity image exports.
    pub(crate) static RAW: DefaultMemoryImpl = DefaultMemoryImpl::default();
    pub(crate) static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(RAW.with(Clone::clone)));
    pub(crate) static CONFIG: RefCell<StableCell<CompactStored<Option<Config>>, Memory>> =
        RefCell::new(StableCell::init(memory(0), CompactStored::new(&None)));
    pub(crate) static ACCOUNTS: RefCell<
        StableBTreeMap<Vec<u8>, CompactStored<AccountState>, Memory>,
    > = RefCell::new(StableBTreeMap::init(memory(1)));
    pub(crate) static AUTH: RefCell<StableBTreeMap<Vec<u8>, Stored<AccountId>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(2)));
    static CALLS: RefCell<StableBTreeMap<Vec<u8>, Stored<HourlyCalls>, Memory>> =
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
    // Random secret behind every device unlock secret; generated once from
    // `raw_rand` after installation and never rotated by upgrades.
    pub(crate) master_secret: Option<Hash>,
    // Public totals of the latest charged months, newest first.
    pub(crate) execution_months: Vec<ExecutionMonthStats>,
}

pub(crate) fn config() -> Config {
    CONFIG.with_borrow(|t| t.get().value().expect("initialized"))
}

pub(crate) fn save_config(cfg: &Config) {
    CONFIG.with_borrow_mut(|t| t.set(CompactStored::new(&Some(cfg.clone()))));
}

/// The 32-byte secret a bound login Principal fetches to unlock one device's
/// local store. It is a function of the home's master secret, so nothing is
/// stored per device, and a revoked device is refused at the query.
pub(crate) fn unlock_secret(cfg: &Config, account_id: &AccountId, device_id: &Hash) -> Result<Hash> {
    let master = cfg
        .master_secret
        .ok_or_else(|| Error::Unavailable("unlock secret is not ready".into()))?;
    Ok(digest(
        "dmsg/unlock-secret/v1",
        &(master, account_id, device_id),
    ))
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

/// Persist an execution and, for an attestation, its certified receipt leaf.
pub(crate) fn save_execution(execution: &AuthorizedExecution) {
    EXECUTIONS.with_borrow_mut(|t| {
        t.put(
            &execution_key(&execution.account_id, &execution.request_id),
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

/// Count a remote call an account's login starts, at most
/// `MAX_HOURLY_ACCOUNT_CALLS` per UTC hour. The count commits before the call,
/// so concurrent and failed calls are counted too.
pub(crate) fn admit_call(id: &AccountId, at: u64) -> Result<()> {
    let hour = at / (60 * MINUTE);
    CALLS.with_borrow_mut(|t| {
        let used = t
            .load(id.as_slice())
            .filter(|(h, _)| *h == hour)
            .map_or(0, |(_, used)| used);
        ensure(used < MAX_HOURLY_ACCOUNT_CALLS, Error::QuotaExceeded)?;
        t.put(id.as_slice(), &(hour, used + 1));
        Ok(())
    })
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

/// Prune the expired results of the accounts holding the 64 execution records
/// after `after`, stopping at an account boundary once the page has used
/// `PRUNE_INSTRUCTIONS`. Returns the last key visited; None when none remain.
pub(crate) fn prune_executions(after: Vec<u8>, now: u64) -> Option<Vec<u8>> {
    use std::ops::Bound::{Excluded, Unbounded};
    let keys: Vec<Vec<u8>> = EXECUTIONS.with_borrow(|t| {
        t.keys_range((Excluded(after), Unbounded))
            .take(64)
            .collect()
    });
    let mut cursor = None;
    let mut visited: Option<AccountId> = None;
    for key in keys {
        let id = AccountId(key[..12].try_into().expect("execution key"));
        if visited != Some(id) {
            if ic_cdk::api::instruction_counter() > PRUNE_INSTRUCTIONS {
                break;
            }
            if let Ok(mut s) = load(&id) {
                if prune_account_executions(&mut s, now) > 0 {
                    save_account(&s);
                }
            }
            visited = Some(id);
        }
        cursor = Some(key);
    }
    cursor
}

pub(crate) const STABLE_SCHEMA: u16 = 14;
