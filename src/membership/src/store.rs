use crate::model::Claim;
use dmsg_runtime::{
    storage::{CompactStored, MapExt, Stored},
    Certification,
};
use dmsg_types::{membership::*, *};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeMap};

type Memory = VirtualMemory<DefaultMemoryImpl>;

fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub init: MembershipInit,
    pub sns_verified: bool,
    pub paused: bool,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Limits {
    pub reserved: u64,
    pub hour: u64,
    pub applications: u32,
    pub minute: u64,
    pub qualifications: u32,
    pub refreshes: u32,
    pub authorizations: BTreeMap<candid::Principal, u32>,
}

type PolicyIndex = BTreeMap<(String, Hash), BTreeMap<u64, u64>>;

thread_local! {
    static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> =
        // 16 pages per bucket: small indexes/counters do not each reserve 8 MiB.
        RefCell::new(MemoryManager::init_with_bucket_size(DefaultMemoryImpl::default(), 16));
    static STABLE_CONFIG: RefCell<StableCell<CompactStored<Option<Config>>, Memory>> =
        RefCell::new(StableCell::init(memory(0), CompactStored::new(&None)));
    static CONFIG: RefCell<Option<Config>> = RefCell::new(STABLE_CONFIG.with_borrow(|t| t.get().value()));
    pub static CLAIMS: RefCell<StableBTreeMap<Vec<u8>, CompactStored<Claim>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(1)));
    pub static NEURONS: RefCell<StableBTreeMap<Vec<u8>, Stored<Vec<Hash>>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(2)));
    pub static POLICIES: RefCell<StableBTreeMap<Vec<u8>, Stored<MembershipPolicy>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(3)));
    pub static PENDING: RefCell<StableBTreeMap<Vec<u8>, Stored<Hash>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(4)));
    static STABLE_LIMITS: RefCell<StableCell<Stored<Limits>, Memory>> =
        RefCell::new(StableCell::init(memory(5), Stored(Limits::default())));
    static LIMITS: RefCell<Limits> = RefCell::new(STABLE_LIMITS.with_borrow(|t| t.get().0.clone()));
    static EXPIRATIONS: RefCell<StableBTreeMap<Vec<u8>, Stored<Hash>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(6)));
    static POLICY_INDEX: RefCell<PolicyIndex> = const { RefCell::new(BTreeMap::new()) };
    pub static CERT: RefCell<Certification> = RefCell::new(Certification::default());
}

pub fn config() -> Config {
    with_config(Clone::clone)
}

pub fn with_config<T>(f: impl FnOnce(&Config) -> T) -> T {
    CONFIG.with_borrow(|c| f(c.as_ref().expect("initialized")))
}

pub fn save_config(c: &Config) {
    STABLE_CONFIG.with_borrow_mut(|t| t.set(CompactStored::new(&Some(c.clone()))));
    CONFIG.with_borrow_mut(|value| *value = Some(c.clone()));
}

/// Counters commit in heap at message boundaries, then persist before upgrade.
pub fn persist_limits() {
    LIMITS.with_borrow(|c| STABLE_LIMITS.with_borrow_mut(|t| t.set(Stored(c.clone()))));
}

pub enum CallBudget {
    Authorization(candid::Principal),
    Qualification,
    Refresh,
}

pub fn reserve_call(at: u64, kind: CallBudget) -> Result<()> {
    LIMITS.with_borrow_mut(|c| {
        if c.minute != at / MINUTE {
            c.minute = at / MINUTE;
            c.qualifications = 0;
            c.refreshes = 0;
            c.authorizations.clear();
        }
        let used = match kind {
            CallBudget::Authorization(actor) => {
                ensure(
                    c.authorizations.values().sum::<u32>() < 200,
                    Error::QuotaExceeded,
                )?;
                let used = c.authorizations.entry(actor).or_default();
                ensure(*used < 10, Error::QuotaExceeded)?;
                used
            }
            CallBudget::Qualification => &mut c.qualifications,
            CallBudget::Refresh => &mut c.refreshes,
        };
        ensure(*used < 200, Error::QuotaExceeded)?;
        *used += 1;
        Ok(())
    })
}

pub fn reserve_application(at: u64, units: u64) -> Result<()> {
    let (hourly, budget) = with_config(|c| (c.init.hourly_applications, c.init.subsidy_budget));
    LIMITS.with_borrow_mut(|c| {
        if c.hour != at / (60 * MINUTE) {
            c.hour = at / (60 * MINUTE);
            c.applications = 0;
        }
        let reserved = c.reserved.checked_add(units).ok_or(Error::QuotaExceeded)?;
        ensure(
            c.applications < hourly && reserved <= budget,
            Error::QuotaExceeded,
        )?;
        c.reserved = reserved;
        c.applications += 1;
        Ok(())
    })
}

pub fn release_budget(c: &mut Claim) {
    if c.budget_reserved {
        LIMITS.with_borrow_mut(|limits| {
            limits.reserved = limits
                .reserved
                .checked_sub(c.policy.subsidy_units)
                .expect("reserved subsidy");
        });
        c.budget_reserved = false;
    }
}

pub fn load(id: Hash) -> Result<Claim> {
    CLAIMS.with_borrow(|t| t.load(id.as_slice()).ok_or(Error::NotFound))
}

pub fn save(c: &mut Claim) {
    let previous = load(c.view.claim_id).ok();
    let view_changed = previous.as_ref().is_none_or(|old| old.view != c.view);
    if let Some(previous) = &previous {
        if previous.view != c.view {
            c.view.lease_revision = c.view.lease_revision.max(
                previous
                    .view
                    .lease_revision
                    .checked_add(1)
                    .expect("claim revision exhausted"),
            );
        }
    } else {
        c.view.lease_revision = c.view.lease_revision.max(1);
    }
    let old_pending = previous.as_ref().is_some_and(Claim::pending);
    if old_pending != c.pending() {
        let b = dmsg_protocol::digest("membership/pending-subject/v1", &c.view.beneficiary);
        if c.pending() {
            PENDING.with_borrow_mut(|t| t.put(b.as_slice(), &c.view.claim_id));
        } else if PENDING.with_borrow(|t| t.load(b.as_slice())) == Some(c.view.claim_id) {
            PENDING.with_borrow_mut(|t| t.delete(b.as_slice()));
        }
    }
    let old_deadline = previous.as_ref().and_then(Claim::release_deadline);
    let deadline = c.release_deadline();
    if old_deadline != deadline {
        EXPIRATIONS.with_borrow_mut(|t| {
            if let Some(at) = old_deadline {
                t.delete(&expiration_key(at, c.view.claim_id));
            }
            if let Some(at) = deadline {
                t.put(&expiration_key(at, c.view.claim_id), &c.view.claim_id);
            }
        });
    }
    if previous.as_ref().is_some_and(|old| !old.terminal()) && c.terminal() {
        let key = dmsg_protocol::membership::neuron_key(
            with_config(|c| c.init.governance),
            c.request.neuron_id,
        );
        NEURONS.with_borrow_mut(|t| {
            let mut bound = t.load(key.as_slice()).unwrap_or_default();
            bound.retain(|id| *id != c.view.claim_id);
            if bound.is_empty() {
                t.delete(key.as_slice());
            } else {
                t.put(key.as_slice(), &bound);
            }
        });
    }
    CLAIMS.with_borrow_mut(|t| t.put(c.view.claim_id.as_slice(), c));
    if view_changed {
        CERT.with_borrow_mut(|t| {
            t.put(
                dmsg_protocol::membership::claim_key(c.view.claim_id).to_vec(),
                &c.view,
            )
        });
    }
}

fn expiration_key(at: u64, id: Hash) -> Vec<u8> {
    [&at.to_be_bytes(), id.as_slice()].concat()
}

pub fn expired_ids(at: u64) -> Vec<Hash> {
    EXPIRATIONS.with_borrow(|t| {
        t.range(..=expiration_key(at, Hash::new([255; 32])))
            .take(32)
            .map(|entry| entry.value().0)
            .collect()
    })
}

pub fn validate_policy_slot(p: &MembershipPolicy) -> Result<()> {
    ensure(
        !POLICIES.with_borrow(|t| t.contains(&p.version.to_be_bytes())),
        Error::IdempotencyConflict,
    )?;
    ensure(
        POLICY_INDEX.with_borrow(|t| {
            t.get(&(p.product_id.clone(), p.benefit_id))
                .is_none_or(|versions| !versions.contains_key(&p.effective_at_ms))
        }),
        Error::VersionConflict,
    )
}

fn index_policy(p: &MembershipPolicy) {
    POLICY_INDEX.with_borrow_mut(|t| {
        t.entry((p.product_id.clone(), p.benefit_id))
            .or_default()
            .insert(p.effective_at_ms, p.version);
    });
}

pub fn save_policy(p: &MembershipPolicy) {
    POLICIES.with_borrow_mut(|t| t.put(&p.version.to_be_bytes(), p));
    index_policy(p);
    CERT.with_borrow_mut(|c| c.put(dmsg_protocol::membership::policy_key(p.version).to_vec(), p));
}

pub fn effective_policy(product: &str, benefit: Hash, at: u64) -> Option<u64> {
    POLICY_INDEX.with_borrow(|t| {
        t.get(&(product.to_owned(), benefit))?
            .range(..=at)
            .next_back()
            .map(|(_, v)| *v)
    })
}

pub fn rebuild() {
    POLICY_INDEX.with_borrow_mut(BTreeMap::clear);
    CERT.with_borrow_mut(|c| {
        *c = Certification::default();
        CLAIMS.with_borrow(|t| {
            t.for_each(|_, v| {
                c.0.insert(
                    dmsg_protocol::membership::claim_key(v.view.claim_id).to_vec(),
                    dmsg_protocol::canonical(&v.view),
                );
            })
        });
        POLICIES.with_borrow(|t| {
            t.for_each(|_, p| {
                index_policy(&p);
                c.0.insert(
                    dmsg_protocol::membership::policy_key(p.version).to_vec(),
                    dmsg_protocol::canonical(&p),
                );
            })
        });
        c.publish();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stable_codec::tests::claim;
    use dmsg_protocol::{canonical, membership::claim_key};

    #[test]
    fn saving_only_internal_state_preserves_the_public_revision_and_leaf() {
        let mut c = claim(false);
        save(&mut c);
        let before = c.view.clone();
        let key = claim_key(c.view.claim_id).to_vec();
        let leaf = CERT.with_borrow(|tree| tree.0.get(&key).cloned());
        c.generation += 1;
        c.busy_until_ms += 1;
        save(&mut c);
        assert_eq!(c.view, before);
        assert_eq!(CERT.with_borrow(|tree| tree.0.get(&key).cloned()), leaf);
        c.view.status = ClaimStatus::CoolingDown;
        save(&mut c);
        assert_eq!(c.view.lease_revision, before.lease_revision + 1);
        assert_eq!(load(c.view.claim_id).unwrap().view, c.view);
        assert_eq!(
            CERT.with_borrow(|tree| tree.0.get(&key).cloned()),
            Some(canonical(&c.view))
        );
    }

    #[test]
    fn expiration_index_is_bounded_and_moves_with_claim_state() {
        let at = 1_700_000_000_000;
        for i in 1..=33 {
            let mut c = claim(false);
            c.view.claim_id = Hash::new([i; 32]);
            c.request.authorization.valid_until_ms = at;
            save(&mut c);
        }
        assert!(expired_ids(at - 1).is_empty());
        let ids = expired_ids(at);
        assert_eq!(ids.len(), 32);
        for id in ids {
            let mut c = load(id).unwrap();
            c.applying(at - 1, at - 1).unwrap();
            save(&mut c);
        }
        assert_eq!(expired_ids(at), vec![Hash::new([33; 32])]);
        assert_eq!(expired_ids(u64::MAX), vec![Hash::new([33; 32])]);
    }
}
