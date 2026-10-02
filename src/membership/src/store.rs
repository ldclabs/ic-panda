//! Stable claim state, indexes, certification and service admission budgets.
use crate::claim::Claim;
use candid::Principal;
use dmsg_protocol::{commerce_v2::*, *};
use dmsg_runtime::{
    storage::{MapExt, Stored},
    Certification,
};
use dmsg_types::{integration_membership::*, membership::MembershipInit, *};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
};
type Memory = VirtualMemory<DefaultMemoryImpl>;

/// A successful SNS verification is reused for one hour.
const SNS_FRESH_MS: u64 = 60 * MINUTE;
/// Hard bounds include historical operations, independently of live admission limits.
pub const MAX_FULL_CLAIMS: u64 = 100_000;
const MAX_OPERATIONS: u64 = 1_000_000;
const HISTORY_RETENTION_MS: u64 = 30 * DAY;

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
    products: u32,
    product_actors: BTreeMap<Principal, u32>,
}

thread_local! {
    static CLAIMS: RefCell<StableBTreeMap<Vec<u8>, Stored<Claim>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(1)));
    static NEURONS: RefCell<StableBTreeMap<Vec<u8>, Stored<Vec<Hash>>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(3)));
    static EXPIRATIONS: RefCell<StableBTreeMap<Vec<u8>, Stored<Hash>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(4)));
    // Rebuilt from stable claims alongside the certified views on init and upgrade.
    static LIVE_CLAIMS: Cell<u64> = const { Cell::new(0) };
    static RETENTION: RefCell<StableBTreeMap<Vec<u8>, Stored<Hash>, Memory>> = RefCell::new(StableBTreeMap::init(memory(6)));
    static TOMBSTONES: RefCell<StableBTreeMap<Vec<u8>, Stored<Hash>, Memory>> = RefCell::new(StableBTreeMap::init(memory(7)));
    static READERS: RefCell<StableBTreeMap<Vec<u8>, (), Memory>> = RefCell::new(StableBTreeMap::init(memory(8)));
    static PRODUCT_CALLS: RefCell<BTreeSet<Hash>> = const { RefCell::new(BTreeSet::new()) };
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
    Product(Principal),
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
            CallBudget::Product(actor) => {
                ensure(c.products < 200, Error::QuotaExceeded)?;
                take(c.product_actors.entry(actor).or_default(), 10)?;
                c.products += 1;
                Ok(())
            }
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
        LIVE_CLAIMS.with(|count| {
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
        CERT.with_borrow_mut(|t| t.put(key(id), &c.view));
    }
    CLAIMS.with_borrow_mut(|t| t.put(id.as_slice(), c));
}

/// All claims occupying a neuron, including unresolved Apply decisions.
pub(crate) fn live_claims() -> u64 {
    LIVE_CLAIMS.with(Cell::get)
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

pub fn sweep(at: u64) -> Result<u32> {
    let ids: Vec<Hash> = EXPIRATIONS.with_borrow(|t| {
        t.range(..=expiry_key(at, Hash::new([255; 32])))
            .take(32)
            .map(|v| v.value().0)
            .collect()
    });
    let mut released = 0;
    for id in ids {
        let mut c = load(id)?;
        c.expire(at)?;
        save(&mut c, at);
        released += 1;
    }
    prune_history(at);
    Ok(released)
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

#[ic_cdk::update]
fn sweep_panda_commitments() -> Result<u32> {
    sweep(nanos_to_millis(ic_cdk::api::time()))
}

/// Rebuild the live count and stage every claim view; the caller publishes the root once.
fn rebuild_claims(cert: &mut Certification) {
    let mut live = 0;
    CLAIMS.with_borrow(|t| {
        t.for_each(|_, c: Claim| {
            live += u64::from(c.holds());
            cert.set(key(c.view.claim_id), canonical(&c.view));
        })
    });
    LIVE_CLAIMS.with(|count| count.set(live));
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

pub fn rebuild() {
    CERT.with_borrow_mut(|c| {
        *c = Certification::default();
        rebuild_claims(c);
        c.publish();
    });
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
