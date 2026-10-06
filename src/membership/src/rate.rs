//! Immutable announced conversion rates; version identities survive pruning.
use crate::store;
use dmsg_protocol::integration::*;
use dmsg_runtime::admin::{self, validation, Validation};
use dmsg_runtime::storage::{MapExt, Stored};
use dmsg_types::{integration::*, *};
use ic_stable_structures::{
    memory_manager::VirtualMemory, DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use std::cell::RefCell;
type Memory = VirtualMemory<DefaultMemoryImpl>;
const MAX_POLICIES: u64 = 64;
thread_local! {
    static POLICIES: RefCell<StableBTreeMap<Vec<u8>, Stored<PandaRatePolicy>, Memory>> = RefCell::new(StableBTreeMap::init(store::memory(2)));
    static LAST_VERSION: RefCell<StableCell<u64, Memory>> = RefCell::new(StableCell::init(store::memory(5), 0));
}

/// Announce an immutable PANDA conversion rate for its products.
#[ic_cdk::update]
fn schedule_panda_rate(policy: PandaRatePolicy) -> Result<PandaRatePolicy> {
    let c = store::admin(ic_cdk::api::msg_caller())?;
    let at = nanos_to_millis(ic_cdk::api::time());
    schedule(policy, c.init.environment, at)
}

#[ic_cdk::query]
fn validate_schedule_panda_rate(policy: PandaRatePolicy) -> Validation {
    let environment = store::config().init.environment;
    let at = nanos_to_millis(ic_cdk::api::time());
    validation(check(policy, environment, at).map(|scheduled| {
        let (p, fresh) = match scheduled {
            Scheduled::Existing(p) => (p, false),
            Scheduled::New(p, _) => (p, true),
        };
        format!(
            "Schedule PANDA rate policy {} for products [{}]: {}/{} effective at {} ms.{}",
            p.policy_version,
            p.product_ids.join(", "),
            p.r_num,
            p.r_den,
            p.effective_at_ms,
            admin::unchanged(fresh, "Already announced"),
        )
    }))
}

enum Scheduled {
    /// The same policy version is already announced.
    Existing(PandaRatePolicy),
    /// A new policy, and the stored versions it lets the schedule prune.
    New(PandaRatePolicy, Vec<u64>),
}

fn check(policy: PandaRatePolicy, environment: Environment, at: u64) -> Result<Scheduled> {
    let mut policy = policy;
    let id = policy.policy_version.to_be_bytes();
    if let Some(old) = POLICIES.with_borrow(|t| t.load(&id)) {
        if environment != Environment::Local {
            policy.published_at_ms = old.published_at_ms;
        }
        ensure(
            old == policy && policy.environment == environment,
            Error::IdempotencyConflict,
        )?;
        return Ok(Scheduled::Existing(old));
    }
    ensure(
        policy.policy_version > LAST_VERSION.with_borrow(|v| *v.get()),
        Error::VersionConflict,
    )?;
    // Local fixtures may seed a previously announced policy; production publication is consensus time.
    if environment != Environment::Local {
        policy.published_at_ms = at;
    }
    validate_rate_policy(&policy)?;
    ensure(
        policy.environment == environment && policy.published_at_ms <= at,
        Error::Forbidden,
    )?;
    let current: Vec<PandaRatePolicy> =
        POLICIES.with_borrow(|t| t.iter().map(|v| v.value().0).collect());
    let (pruned, live): (Vec<_>, Vec<_>) =
        current.iter().partition(|p| superseded(p, &current, at));
    ensure((live.len() as u64) < MAX_POLICIES, Error::QuotaExceeded)?;
    ensure(
        live.iter().all(|p| {
            p.effective_at_ms != policy.effective_at_ms
                || !p
                    .product_ids
                    .iter()
                    .any(|id| policy.product_ids.contains(id))
        }),
        Error::VersionConflict,
    )?;
    let pruned = pruned.iter().map(|p| p.policy_version).collect();
    Ok(Scheduled::New(policy, pruned))
}

fn schedule(policy: PandaRatePolicy, environment: Environment, at: u64) -> Result<PandaRatePolicy> {
    let (policy, pruned) = match check(policy, environment, at)? {
        Scheduled::Existing(old) => return Ok(old),
        Scheduled::New(policy, pruned) => (policy, pruned),
    };
    POLICIES.with_borrow_mut(|t| {
        for version in pruned {
            t.delete(&version.to_be_bytes());
        }
        t.put(&policy.policy_version.to_be_bytes(), &policy);
    });
    LAST_VERSION.with_borrow_mut(|v| v.set(policy.policy_version));
    Ok(policy)
}

/// A policy is never selected again once each of its products has a later effective one.
fn superseded(p: &PandaRatePolicy, all: &[PandaRatePolicy], at: u64) -> bool {
    p.product_ids.iter().all(|product| {
        all.iter().any(|q| {
            q.effective_at_ms > p.effective_at_ms
                && q.effective_at_ms <= at
                && q.product_ids.contains(product)
        })
    })
}

pub fn current(product: &str, at: u64) -> Result<PandaRatePolicy> {
    POLICIES
        .with_borrow(|t| {
            t.iter()
                .map(|v| v.value().0)
                .filter(|p| p.effective_at_ms <= at && p.product_ids.iter().any(|id| id == product))
                .max_by_key(|p| p.effective_at_ms)
        })
        .ok_or(Error::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonlocal_retries_keep_the_original_publication_even_after_notice_elapsed() {
        let at = crate::fixture::base::NOW;
        let mut p = crate::fixture::base::rate();
        p.environment = Environment::Staging;
        p.published_at_ms = at - MINUTE;
        p.effective_at_ms = at + POLICY_NOTICE_MS;
        let stored = schedule(p.clone(), Environment::Staging, at).unwrap();
        assert_eq!(stored.published_at_ms, at);
        assert_eq!(
            schedule(p.clone(), Environment::Staging, at + DAY).unwrap(),
            stored
        );
        assert_eq!(
            schedule(stored.clone(), Environment::Staging, at + POLICY_NOTICE_MS).unwrap(),
            stored
        );
        p.r_num += 1;
        assert_eq!(
            schedule(p, Environment::Staging, at + DAY),
            Err(Error::IdempotencyConflict)
        );
    }

    #[test]
    fn pruned_policy_versions_cannot_be_reassigned() {
        let at = crate::fixture::base::NOW;
        let p = crate::fixture::base::rate();
        schedule(p.clone(), Environment::Local, at).unwrap();
        let mut next = p.clone();
        next.policy_version = 2;
        next.effective_at_ms = at + MINUTE;
        schedule(next.clone(), Environment::Local, at).unwrap();
        next.policy_version = 3;
        next.effective_at_ms = at + DAY;
        schedule(next, Environment::Local, at + MINUTE).unwrap();
        assert!(!POLICIES.with_borrow(|t| t.contains(&1u64.to_be_bytes())));
        LAST_VERSION.with_borrow_mut(|v| *v = StableCell::init(store::memory(5), 0));
        let mut reused = p;
        reused.r_num += 1;
        reused.effective_at_ms = at + 2 * DAY;
        assert_eq!(
            schedule(reused, Environment::Local, at + MINUTE),
            Err(Error::VersionConflict)
        );
        assert_eq!(LAST_VERSION.with_borrow(|v| *v.get()), 3);
    }

    #[test]
    fn only_policies_replaced_for_every_product_are_superseded() {
        let policy = |version, effective_at_ms, products: &[&str]| PandaRatePolicy {
            version: COMMERCE_VERSION,
            policy_version: version,
            environment: Environment::Local,
            product_ids: products.iter().map(|p| p.to_string()).collect(),
            r_num: 1,
            r_den: 1,
            published_at_ms: 0,
            effective_at_ms,
        };
        let old = policy(1, 10, &["a", "b"]);
        let a = policy(2, 20, &["a"]);
        let b = policy(3, 30, &["b"]);
        let all = [old.clone(), a.clone(), b.clone()];
        assert!(!superseded(&old, &all, 29));
        assert!(superseded(&old, &all, 30));
        assert!(!superseded(&a, &all, u64::MAX));
        assert!(!superseded(&b, &all, u64::MAX));
    }
}
