use crate::{state::AuthorizedExecution, store};
use dmsg_protocol::{billing::*, canonical, digest};
use dmsg_runtime::{
    storage::{CompactStored, MapExt},
    Certification,
};
use dmsg_types::{billing::*, cose::*, *};
use ic_stable_structures::{memory_manager::VirtualMemory, DefaultMemoryImpl, StableBTreeMap};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Month {
    pub(crate) usage: ExecutionUsage,
    pub(crate) weights: ExecutionWeights,
    pub(crate) entitlement_digest: Hash,
}

type Memory = VirtualMemory<DefaultMemoryImpl>;
thread_local! {
    static MONTHS: RefCell<StableBTreeMap<Vec<u8>, CompactStored<Month>, Memory>> =
        RefCell::new(StableBTreeMap::init(store::memory(6)));
}

fn load(id: &AccountId, month: u32) -> Option<Month> {
    MONTHS.with_borrow(|t| t.load(usage_key(id, month).as_slice()))
}

fn save(m: &Month) {
    let key = usage_key(&m.usage.account_id, m.usage.month_utc);
    MONTHS.with_borrow_mut(|t| t.put(key.as_slice(), m));
    store::CERT.with_borrow_mut(|c| c.0.insert(key.to_vec(), canonical(&m.usage)));
}

pub fn usage(id: &AccountId, month: u32) -> Result<ExecutionUsage> {
    load(id, month).map(|m| m.usage).ok_or(Error::NotFound)
}

pub fn rebuild(c: &mut Certification) {
    MONTHS.with_borrow(|t| {
        t.for_each(|key, m| {
            c.0.insert(key, canonical(&m.usage));
        })
    });
}

pub fn is_current(id: &AccountId, at: u64) -> Result<bool> {
    Ok(load(id, month_utc(at)?).is_some_and(|m| at < m.usage.valid_until_ms))
}

/// Fetch current business terms even if a previous lease is still valid.
/// Returns the refreshed time for authorization after the remote call.
pub async fn refresh(id: &AccountId, at: u64) -> Result<u64> {
    let month = month_utc(at)?;
    let s = store::load(id)?;
    let home = store::config().init.commerce_canister;
    let b = beneficiary(s.home_user, id);
    let result: Result<ExecutionEntitlement> = dmsg_runtime::call(
        home,
        "get_execution_entitlement",
        (b.clone(), month, s.created_at_ms),
    )
    .await?;
    let e = result?;
    let now = nanos_to_millis(ic_cdk::api::time());
    ensure(
        month_utc(now)? == month
            && e.view.home_commerce == home
            && e.view.beneficiary == b
            && e.month.beneficiary == b
            && e.month.month_utc == month
            && e.month.calculation_version == 1
            && e.month.business_revision == e.view.business_revision
            && e.view.issued_at_ms <= now
            && now < e.view.valid_until_ms
            && e.view.valid_until_ms <= e.view.issued_at_ms.saturating_add(60 * MINUTE),
        Error::MembershipStale,
    )?;
    ensure(
        monthly_allowance(month, s.created_at_ms, &e.month.segments)? == e.month.allowed_units,
        Error::IntegrityFailed,
    )?;
    let old = load(id, month);
    let entitlement_digest = digest("dmsg/stored-execution-entitlement/v1", &e);
    if let Some(m) = &old {
        ensure(
            e.view.business_revision >= m.usage.business_revision
                && e.view.lease_revision >= m.usage.lease_revision
                && e.month.month_revision >= m.usage.month_revision,
            Error::PolicyStale,
        )?;
        if e.view.lease_revision == m.usage.lease_revision {
            ensure(
                entitlement_digest == m.entitlement_digest,
                Error::IntegrityFailed,
            )?;
        }
    }
    let usage = ExecutionUsage {
        account_id: id.clone(),
        month_utc: month,
        month_revision: e.month.month_revision,
        business_revision: e.view.business_revision,
        lease_revision: e.view.lease_revision,
        weight_policy_version: e.month.weights.version,
        allowed_units: e.month.allowed_units,
        held_units: old.as_ref().map_or(0, |m| m.usage.held_units),
        charged_units: old.as_ref().map_or(0, |m| m.usage.charged_units),
        valid_until_ms: e.view.valid_until_ms,
    };
    save(&Month {
        usage,
        weights: e.month.weights,
        entitlement_digest,
    });
    store::publish();
    Ok(now)
}

pub fn reserve(e: &mut AuthorizedExecution, now: u64) -> Result<()> {
    let ExecutionKind::Sign { key, .. } = &e.grant.kind else {
        return Ok(());
    };
    let month = month_utc(now)?;
    let mut m = load(&e.grant.account_id, month).ok_or(Error::MembershipStale)?;
    ensure(now < m.usage.valid_until_ms, Error::MembershipStale)?;
    let units = m
        .weights
        .units(&key.algorithm)
        .ok_or(Error::UnsupportedProtocol)?;
    let held = m
        .usage
        .held_units
        .checked_add(units)
        .ok_or(Error::QuotaExceeded)?;
    ensure(
        held.checked_add(m.usage.charged_units)
            .is_some_and(|n| n <= m.usage.allowed_units),
        Error::QuotaExceeded,
    )?;
    e.grant.expires_at = e.grant.expires_at.min(m.usage.valid_until_ms);
    e.grant.commerce = Some(CommercialReservation {
        reservation_id: e.grant.request_id,
        month_utc: month,
        units,
        weight_policy_version: m.usage.weight_policy_version,
        business_revision: m.usage.business_revision,
        lease_revision: m.usage.lease_revision,
        valid_until_ms: m.usage.valid_until_ms,
    });
    m.usage.held_units = held;
    save(&m);
    Ok(())
}

pub fn settle(e: &AuthorizedExecution) -> Result<()> {
    let Some(r) = &e.grant.commerce else {
        return Ok(());
    };
    let mut m = load(&e.grant.account_id, r.month_utc).ok_or(Error::IntegrityFailed)?;
    ensure(e.result.is_terminal(), Error::Pending)?;
    m.usage.held_units = m
        .usage
        .held_units
        .checked_sub(r.units)
        .ok_or(Error::IntegrityFailed)?;
    if !matches!(e.result.outcome, ExecutionOutcome::Failed(_)) {
        m.usage.charged_units = m
            .usage
            .charged_units
            .checked_add(r.units)
            .ok_or(Error::QuotaExceeded)?;
    }
    save(&m);
    Ok(())
}
