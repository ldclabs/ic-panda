//! Stable claim state, indexes, certification and service admission budgets.
use crate::claim::Claim;
use candid::Principal;
use dmsg_protocol::{commerce_v2::*, *};
use dmsg_runtime::{
    cert_map::CertMap,
    storage::{MapExt, Stored},
};
use dmsg_types::{
    integration_membership::*,
    membership::{MembershipInit, MembershipStats},
    *,
};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};
type Memory = VirtualMemory<DefaultMemoryImpl>;

/// Stable layout with certification nodes and the live-claim count in stable memory.
pub const STABLE_SCHEMA: u16 = 3;
/// A successful SNS verification is reused for one hour.
const SNS_FRESH_MS: u64 = 60 * MINUTE;
/// Hard bounds include historical operations, independently of live admission limits.
pub const MAX_FULL_CLAIMS: u64 = 1_000_000;
const MAX_OPERATIONS: u64 = 10_000_000;
const HISTORY_RETENTION_MS: u64 = 30 * DAY;

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    /// Development stable layout; other layouts are not migrated.
    #[serde(default)]
    pub schema: u16,
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

    /// The commerce canister serving the accounts of `home`.
    pub fn commerce(&self, home: Principal) -> Result<Principal> {
        self.service()?
            .commerce_homes
            .iter()
            .find(|h| h.user_home == home)
            .map(|h| h.commerce_canister)
            .ok_or(Error::Forbidden)
    }
}

/// Calls one actor may make per minute from each shared budget.
const ACTOR_CALLS_PER_MINUTE: u32 = 10;

/// Heap-only window; an upgrade simply starts a new minute.
#[derive(Default)]
struct Limits {
    minute: u64,
    qualifications: u32,
    refreshes: u32,
    authorizations: Shared,
    activations: Shared,
    products: Shared,
}

/// One minute of a global budget, of which each actor has a fixed share.
#[derive(Default)]
struct Shared {
    used: u32,
    actors: BTreeMap<Principal, u32>,
}

impl Shared {
    fn take(&mut self, actor: Principal, limit: u32) -> Result<()> {
        ensure(self.used < limit, Error::QuotaExceeded)?;
        take(
            self.actors.entry(actor).or_default(),
            ACTOR_CALLS_PER_MINUTE,
        )?;
        self.used += 1;
        Ok(())
    }
}

/// Per-minute budgets of `PandaServiceConfig`; each is 200 until the service is configured.
struct Budgets {
    sns_reads: u32,
    authorizations: u32,
    product_calls: u32,
}

thread_local! {
    static CLAIMS: RefCell<StableBTreeMap<Vec<u8>, Stored<Claim>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(1)));
    static NEURONS: RefCell<StableBTreeMap<Vec<u8>, Stored<Vec<Hash>>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(3)));
    static EXPIRATIONS: RefCell<StableBTreeMap<Vec<u8>, Stored<Hash>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(4)));
    // Claims occupying a neuron, kept with the claims so upgrades need no scan.
    static LIVE_CLAIMS: RefCell<StableCell<u64, Memory>> =
        RefCell::new(StableCell::init(memory(11), 0));
    static RETENTION: RefCell<StableBTreeMap<Vec<u8>, Stored<Hash>, Memory>> = RefCell::new(StableBTreeMap::init(memory(6)));
    static TOMBSTONES: RefCell<StableBTreeMap<Vec<u8>, Stored<Hash>, Memory>> = RefCell::new(StableBTreeMap::init(memory(7)));
    static READERS: RefCell<StableBTreeMap<Vec<u8>, (), Memory>> = RefCell::new(StableBTreeMap::init(memory(8)));
    static PRODUCT_CALLS: RefCell<BTreeSet<Hash>> = const { RefCell::new(BTreeSet::new()) };
    // Stable memory itself; on the host a shared vector the capacity image exports.
    pub(crate) static RAW: DefaultMemoryImpl = DefaultMemoryImpl::default();
    static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> =
        // 8 MiB buckets; 32,768 buckets address up to 256 GiB of stable data.
        RefCell::new(MemoryManager::init(RAW.with(Clone::clone)));
    static CONFIG: RefCell<StableCell<Stored<Option<Config>>, Memory>> =
        RefCell::new(StableCell::init(memory(0), Stored(None)));
    static LIMITS: RefCell<Limits> = RefCell::new(Limits::default());
    // Hashes only: each certified claim view is rebuilt from its record for a witness.
    pub static CERT: RefCell<CertMap<Memory>> = RefCell::new(CertMap::new(memory(9), memory(10)));
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

/// Configuration for a call made by a controller or the pinned governance canister.
pub fn admin(caller: Principal) -> Result<Config> {
    let c = config();
    dmsg_runtime::admin::check_admin(caller, c.init.governance)?;
    Ok(c)
}

pub enum CallBudget {
    /// Quotes and new applications.
    Authorization(Principal),
    /// Activation of an accepted application, apart from quotes so they cannot crowd it out.
    Activation(Principal),
    Qualification,
    Refresh,
    Product(Principal),
}

fn take(used: &mut u32, limit: u32) -> Result<()> {
    ensure(*used < limit, Error::QuotaExceeded)?;
    *used += 1;
    Ok(())
}

fn budgets() -> Budgets {
    let limit = |v: u64| u32::try_from(v).unwrap_or(u32::MAX);
    CONFIG.with_borrow(|c| {
        c.get().0.as_ref().and_then(|c| c.service.as_ref()).map_or(
            Budgets {
                sns_reads: 200,
                authorizations: 200,
                product_calls: 200,
            },
            |s| Budgets {
                sns_reads: limit(s.qualifications_per_minute),
                authorizations: limit(s.authorizations_per_minute),
                product_calls: limit(s.product_calls_per_minute),
            },
        )
    })
}

pub fn reserve_call(at: u64, kind: CallBudget) -> Result<()> {
    let b = budgets();
    LIMITS.with_borrow_mut(|c| {
        if c.minute != at / MINUTE {
            *c = Limits {
                minute: at / MINUTE,
                ..Default::default()
            };
        }
        match kind {
            CallBudget::Authorization(actor) => c.authorizations.take(actor, b.authorizations),
            CallBudget::Activation(actor) => c.activations.take(actor, b.authorizations),
            CallBudget::Qualification => take(&mut c.qualifications, b.sns_reads),
            CallBudget::Refresh => take(&mut c.refreshes, b.sns_reads),
            CallBudget::Product(actor) => c.products.take(actor, b.product_calls),
        }
    })
}

pub fn load(id: Hash) -> Result<Claim> {
    CLAIMS
        .with_borrow(|t| t.load(id.as_slice()))
        .ok_or_else(|| {
            if TOMBSTONES.with_borrow(|t| t.contains(id.as_slice())) {
                Error::ResultExpired
            } else {
                Error::NotFound
            }
        })
}

/// Archived identities cannot be reused with either the original or different terms.
pub fn replay(id: Hash, terms: &PandaApplicationTerms) -> Result<Option<Claim>> {
    if let Some(hash) = TOMBSTONES.with_borrow(|t| t.load(id.as_slice())) {
        ensure(
            hash == panda_application_hash(terms),
            Error::IdempotencyConflict,
        )?;
        return Err(Error::ResultExpired);
    }
    let old = CLAIMS.with_borrow(|t| t.load(id.as_slice()));
    if let Some(c) = &old {
        ensure(c.view.terms == *terms, Error::IdempotencyConflict)?;
    }
    Ok(old)
}

pub fn check_capacity() -> Result<()> {
    let full = CLAIMS.with_borrow(|t| t.len());
    ensure(
        full < MAX_FULL_CLAIMS && full + TOMBSTONES.with_borrow(|t| t.len()) < MAX_OPERATIONS,
        Error::QuotaExceeded,
    )
}

/// Heap guard: coalesce retries until the original callback completes; upgrades drop it.
pub struct ProductCall(Hash);

impl Drop for ProductCall {
    fn drop(&mut self) {
        PRODUCT_CALLS.with_borrow_mut(|calls| calls.remove(&self.0));
    }
}

pub fn product_call(id: Hash, actor: Principal, at: u64) -> Result<ProductCall> {
    PRODUCT_CALLS.with_borrow_mut(|calls| {
        ensure(!calls.contains(&id), Error::Pending)?;
        reserve_call(at, CallBudget::Product(actor))?;
        calls.insert(id);
        Ok(ProductCall(id))
    })
}

pub fn key(id: Hash) -> Vec<u8> {
    digest("dmsg/panda/claim-certificate/v2", &id).to_vec()
}

fn neuron(terms: &PandaApplicationTerms) -> Hash {
    digest(
        "dmsg/panda/occupancy/v2",
        &(terms.sns_governance, terms.neuron_id),
    )
}

fn expiry_key(at: u64, id: Hash) -> Vec<u8> {
    [&at.to_be_bytes(), id.as_slice()].concat()
}

pub fn actor(c: &Claim, caller: Principal) -> Result<()> {
    ensure(
        caller == c.view.terms.actor || caller == c.view.terms.offer.adapter,
        Error::Forbidden,
    )
}

/// Persist a claim, touching only the indexes and certified view that actually changed.
pub fn save(c: &mut Claim, at: u64) {
    let id = c.view.claim_id;
    let old = load(id).ok();
    if !c.holds() && c.retain_until_ms.is_none() {
        // By this time an unused product reservation has expired even if its release ACK was lost.
        c.retain_until_ms = Some(
            at.max(c.view.terms.quote.application_deadline_ms)
                .saturating_add(HISTORY_RETENTION_MS),
        );
    }
    if old.as_ref() == Some(c) {
        return;
    }
    if old.is_none() {
        READERS.with_borrow_mut(|t| {
            for reader in [c.view.terms.actor, c.view.terms.offer.adapter] {
                t.insert(reader_key(reader, id), ());
            }
        });
    }
    if old.as_ref().and_then(|v| v.retain_until_ms) != c.retain_until_ms {
        if let Some(until) = c.retain_until_ms {
            RETENTION.with_borrow_mut(|t| t.put(&expiry_key(until, id), &id));
        }
    }
    let old_release = old.as_ref().and_then(Claim::release_at);
    let release = c.release_at();
    if old_release != release {
        EXPIRATIONS.with_borrow_mut(|t| {
            if let Some(at) = old_release {
                t.delete(&expiry_key(at, id));
            }
            if let Some(at) = release {
                t.put(&expiry_key(at, id), &id);
            }
        });
    }
    if old.as_ref().is_some_and(Claim::holds) != c.holds() {
        LIVE_CLAIMS.with_borrow_mut(|count| {
            let next = if c.holds() {
                count.get().checked_add(1)
            } else {
                count.get().checked_sub(1)
            };
            count.set(next.expect("live claim count"));
        });
        let n = neuron(&c.view.terms);
        NEURONS.with_borrow_mut(|t| {
            let mut refs: Vec<Hash> = t.load(n.as_slice()).unwrap_or_default();
            if c.holds() {
                refs.push(id);
            } else {
                refs.retain(|v| *v != id);
            }
            if refs.is_empty() {
                t.delete(n.as_slice());
            } else {
                t.put(n.as_slice(), &refs);
            }
        });
    }
    if old.as_ref().is_none_or(|o| o.view != c.view) {
        if let Some(o) = &old {
            c.view.lease_revision = o.view.lease_revision.checked_add(1).expect("view revision");
        }
        CERT.with_borrow_mut(|t| t.insert(key(id), &canonical(&c.view)));
    }
    CLAIMS.with_borrow_mut(|t| t.put(id.as_slice(), c));
}

/// All claims occupying a neuron, including unresolved Apply decisions.
pub(crate) fn live_claims() -> u64 {
    LIVE_CLAIMS.with_borrow(|count| *count.get())
}

/// Expire due references, then allow only the same beneficiary's committed contiguous term.
pub fn check_occupancy(terms: &PandaApplicationTerms, at: u64) -> Result<()> {
    let refs = NEURONS
        .with_borrow(|t| t.load(neuron(terms).as_slice()))
        .unwrap_or_default();
    let mut held = Vec::with_capacity(refs.len());
    for id in refs {
        let mut c = load(id)?;
        if c.release_at().is_some_and(|end| at >= end) {
            c.expire(at)?;
            save(&mut c, at);
        } else {
            held.push(c);
        }
    }
    ensure(
        held.len() < 2
            && held.iter().all(|old| {
                old.view.committed_until_ms > 0
                    && old.view.terms.offer.beneficiary == terms.offer.beneficiary
                    && old.view.terms.offer.expires_at_ms == terms.offer.starts_at_ms
            }),
        Error::NeuronOccupied,
    )
}

/// Release up to 32 due commitments and compact up to 32 due terminal records;
/// returns both counts together, so a caller repeats until it is zero.
pub fn sweep(at: u64) -> Result<u32> {
    let ids: Vec<Hash> = EXPIRATIONS.with_borrow(|t| {
        t.range(..=expiry_key(at, Hash::new([255; 32])))
            .take(32)
            .map(|v| v.value().0)
            .collect()
    });
    let released = ids.len() as u32;
    for id in ids {
        let mut c = load(id)?;
        c.expire(at)?;
        save(&mut c, at);
    }
    Ok(released + prune_history(at))
}

/// Compact a bounded batch of terminal claims, retaining immutable operation digests.
fn prune_history(at: u64) -> u32 {
    let due: Vec<(Vec<u8>, Hash)> = RETENTION.with_borrow(|t| {
        t.range(..=expiry_key(at, Hash::new([255; 32])))
            .take(32)
            .map(|r| (r.key().clone(), r.value().0))
            .collect()
    });
    for (retention_key, id) in &due {
        let c = load(*id).expect("retained claim");
        assert!(!c.holds() && c.retain_until_ms.is_some_and(|until| until <= at));
        TOMBSTONES
            .with_borrow_mut(|t| t.put(id.as_slice(), &panda_application_hash(&c.view.terms)));
        READERS.with_borrow_mut(|t| {
            for reader in [c.view.terms.actor, c.view.terms.offer.adapter] {
                t.remove(&reader_key(reader, *id));
            }
        });
        CLAIMS.with_borrow_mut(|t| t.delete(id.as_slice()));
        RETENTION.with_borrow_mut(|t| t.delete(retention_key));
        CERT.with_borrow_mut(|cert| cert.remove(&key(*id)));
    }
    due.len() as u32
}

/// Release due commitments and compact due history, at most 32 of each per call;
/// returns how many were processed, so a maintenance job repeats until zero.
#[ic_cdk::update]
fn sweep_panda_commitments() -> Result<u32> {
    sweep(nanos_to_millis(ic_cdk::api::time()))
}

/// Record counts against the service limits, with the stable size and cycle balance.
#[ic_cdk::query]
fn membership_stats() -> MembershipStats {
    MembershipStats {
        live_claims: live_claims(),
        full_claims: CLAIMS.with_borrow(|t| t.len()),
        tombstones: TOMBSTONES.with_borrow(|t| t.len()),
        stable_pages: ic_cdk::api::stable_size(),
        cycles: ic_cdk::api::canister_cycle_balance(),
    }
}

#[ic_cdk::query]
fn panda_operations(after: Option<Hash>, take: u16) -> Result<PandaOperationsPage> {
    let caller = ic_cdk::api::msg_caller();
    authenticated(caller)?;
    ensure_valid((1..=32).contains(&take), "page size")?;
    operations(caller, caller == config().init.governance, after, take)
}

fn reader_key(reader: Principal, id: Hash) -> Vec<u8> {
    [
        &[reader.as_slice().len() as u8],
        reader.as_slice(),
        id.as_slice(),
    ]
    .concat()
}

/// Read only this reader's indexed claims; governance can page all retained records.
fn operations(
    caller: Principal,
    governance: bool,
    after: Option<Hash>,
    take: u16,
) -> Result<PandaOperationsPage> {
    use std::ops::Bound::{Excluded, Included, Unbounded};
    let limit = usize::from(take) + 1;
    let mut claims: Vec<PandaClaimView> = if governance {
        CLAIMS.with_borrow(|t| {
            let start = after.map_or(Unbounded, |id| Excluded(id.to_vec()));
            t.range((start, Unbounded))
                .take(limit)
                .map(|r| r.value().0.view)
                .collect()
        })
    } else {
        let ids: Vec<Hash> = READERS.with_borrow(|t| {
            let start = after.map_or_else(
                || Included(reader_key(caller, Hash::new([0; 32]))),
                |id| Excluded(reader_key(caller, id)),
            );
            t.range((start, Included(reader_key(caller, Hash::new([255; 32])))))
                .take(limit)
                .map(|r| {
                    let key = r.key();
                    Hash::new(key[key.len() - 32..].try_into().expect("reader claim key"))
                })
                .collect()
        });
        ids.into_iter()
            .map(|id| load(id).map(|c| c.view))
            .collect::<Result<_>>()?
    };
    let next = if claims.len() > usize::from(take) {
        claims.pop();
        claims.last().map(|c| c.claim_id)
    } else {
        None
    };
    Ok(PandaOperationsPage { claims, next })
}

/// Certification nodes persist in stable memory; only the root is republished.
pub fn publish() {
    CERT.with_borrow(|c| c.publish());
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
