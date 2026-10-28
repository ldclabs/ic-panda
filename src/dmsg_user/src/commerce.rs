use crate::store;
use dmsg_protocol::{billing::*, digest};
use dmsg_runtime::storage::{CompactStored, MapExt};
use dmsg_types::{
    billing::*,
    user::{EXECUTION_STATS_MONTHS, ExecutionMonthStats},
    *,
};
use ic_stable_structures::{DefaultMemoryImpl, StableBTreeMap, memory_manager::VirtualMemory};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

/// Device admission uses the existing commerce projection without adding
/// fields to signed plan snapshots or persisting a second membership cache.
pub async fn device_plan(home_user: candid::Principal, id: &AccountId) -> Result<(PlanId, u64)> {
    let home = store::config().init.commerce_canister;
    let b = beneficiary(home_user, id);
    let reply: Result<EntitlementView> =
        dmsg_runtime::call(home, "refresh_entitlement", (b.clone(),)).await?;
    let at = nanos_to_millis(ic_cdk::api::time());
    ensure(
        store::config().init.commerce_canister == home,
        Error::PolicyStale,
    )?;
    let plan = match reply {
        Ok(view) => device_plan_from_view(&view, home, &b, at)?,
        // Commerce has no subject row for an account that has never paid.
        Err(Error::NotFound) => PlanId::Free,
        Err(error) => return Err(error),
    };
    Ok((plan, at))
}

fn device_plan_from_view(
    view: &EntitlementView,
    home: candid::Principal,
    b: &dmsg_types::membership::Beneficiary,
    at: u64,
) -> Result<PlanId> {
    ensure(
        view.schema == 1
            && view.home_commerce == home
            && view.beneficiary == *b
            && view.issued_at_ms <= at
            && at < view.valid_until_ms
            && view.next_limit_change_at_ms.is_none_or(|end| at < end)
            && view.effective_stop_at_ms.is_none_or(|end| at < end)
            && view.eligibility_status == dmsg_types::membership::Eligibility::Eligible
            && (view.source_status == SourceStatus::Active
                || (view.plan_snapshot.plan_id == PlanId::Free
                    && matches!(
                        view.source_status,
                        SourceStatus::Free | SourceStatus::Expired
                    ))),
        Error::MembershipStale,
    )?;
    Ok(view.plan_snapshot.plan_id.clone())
}

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

// Rows of one account are contiguous: account ID, then the YYYYMM month.
fn key(id: &AccountId, month: u32) -> Vec<u8> {
    [id.as_slice(), &month.to_be_bytes()].concat()
}

fn load(id: &AccountId, month: u32) -> Option<Month> {
    MONTHS.with_borrow(|t| t.load(&key(id, month)))
}

/// Months are not certified data; the owner gets a certified copy as the
/// `refresh_execution_entitlement` update reply.
pub(crate) fn save(m: &Month) {
    MONTHS.with_borrow_mut(|t| t.put(&key(&m.usage.account_id, m.usage.month_utc), m));
}

/// Keep `month` and the month before it. Attestations are charged when they
/// commit, so no older month can still receive a settlement.
fn retain_recent(id: &AccountId, month: u32) {
    let previous = if month % 100 == 1 {
        month - 89
    } else {
        month - 1
    };
    MONTHS.with_borrow_mut(|t| {
        let old: Vec<Vec<u8>> = t.keys_range(key(id, 0)..key(id, previous)).collect();
        for k in old {
            t.remove(&k);
        }
    });
}

pub fn usage(id: &AccountId, month: u32) -> Result<ExecutionUsage> {
    load(id, month).map(|m| m.usage).ok_or(Error::NotFound)
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
            && now < e.view.valid_until_ms,
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
        // Revision 0 is the stateless Free projection of an account without a
        // commerce record; it follows the catalogs, not a stored lease.
        if e.view.lease_revision == m.usage.lease_revision && e.view.lease_revision > 0 {
            ensure(
                entitlement_digest == m.entitlement_digest,
                Error::IntegrityFailed,
            )?;
        }
    }
    let usage = ExecutionUsage {
        account_id: *id,
        month_utc: month,
        month_revision: e.month.month_revision,
        business_revision: e.view.business_revision,
        lease_revision: e.view.lease_revision,
        weight_policy_version: e.month.weights.version,
        allowed_units: e.month.allowed_units,
        charged_units: old.as_ref().map_or(0, |m| m.usage.charged_units),
        // Recheck business terms hourly even when the resource lease runs longer,
        // so an upgrade reaches the execution allowance without a manual refresh.
        valid_until_ms: e.view.valid_until_ms.min(now.saturating_add(60 * MINUTE)),
    };
    if old.is_none() {
        retain_recent(id, month);
    }
    save(&Month {
        usage,
        weights: e.month.weights,
        entitlement_digest,
    });
    Ok(now)
}

/// Charge one attestation to the current month. An attestation completes in
/// the message that authorizes it, so nothing is held and settled later.
pub fn charge(id: &AccountId, now: u64) -> Result<()> {
    let month = month_utc(now)?;
    let mut m = load(id, month).ok_or(Error::MembershipStale)?;
    ensure(now < m.usage.valid_until_ms, Error::MembershipStale)?;
    let units = m.weights.ed25519;
    let charged = m
        .usage
        .charged_units
        .checked_add(units)
        .ok_or(Error::QuotaExceeded)?;
    ensure(charged <= m.usage.allowed_units, Error::QuotaExceeded)?;
    // Commerce weights are positive, so zero means the account's first charge.
    let first = m.usage.charged_units == 0;
    m.usage.charged_units = charged;
    save(&m);
    count(month, first, units);
    Ok(())
}

/// Add one attestation to the home's public monthly totals.
fn count(month: u32, first: bool, units: u64) {
    let mut cfg = store::config();
    let months = &mut cfg.execution_months;
    if months.first().is_none_or(|t| t.month_utc != month) {
        months.insert(
            0,
            ExecutionMonthStats {
                month_utc: month,
                ..Default::default()
            },
        );
        months.truncate(EXECUTION_STATS_MONTHS);
    }
    let t = &mut months[0];
    t.accounts += u64::from(first);
    t.attestations += 1;
    t.charged_units += units;
    store::save_config(&cfg);
}

#[cfg(test)]
mod tests {
    use super::*;
    use candid::Principal;
    use dmsg_types::membership::Eligibility;

    #[test]
    fn device_admission_requires_a_current_matching_eligible_projection() {
        let home = Principal::from_slice(&[1]);
        let b = beneficiary(Principal::from_slice(&[2]), &AccountId([3; 12]));
        let plan = default_plans(1).pop().unwrap();
        let view = EntitlementView {
            schema: 1,
            home_commerce: home,
            beneficiary: b.clone(),
            business_revision: 1,
            lease_revision: 1,
            active_contract_id: Some(Hash::new([4; 32])),
            effective_limits: plan.limits.clone(),
            plan_snapshot: plan,
            addons: vec![],
            next_limit_change_at_ms: None,
            lease_source_contract_ids: vec![Hash::new([4; 32])],
            source_status: SourceStatus::Active,
            eligibility_status: Eligibility::Eligible,
            observed_at_ms: 100,
            issued_at_ms: 100,
            valid_until_ms: 200,
            effective_stop_at_ms: None,
            repair_deadline_ms: None,
            service_terminated_at_ms: None,
        };
        assert_eq!(device_plan_from_view(&view, home, &b, 100), Ok(PlanId::Max));
        for at in [99, 200, 201] {
            assert_eq!(
                device_plan_from_view(&view, home, &b, at),
                Err(Error::MembershipStale)
            );
        }
        for change in [
            |v: &mut EntitlementView| v.schema = 2,
            |v: &mut EntitlementView| v.home_commerce = Principal::anonymous(),
            |v: &mut EntitlementView| v.beneficiary.authority_canister = Principal::anonymous(),
            |v: &mut EntitlementView| v.eligibility_status = Eligibility::Unverifiable,
            |v: &mut EntitlementView| v.eligibility_status = Eligibility::Ineligible,
            |v: &mut EntitlementView| v.source_status = SourceStatus::Suspended,
            |v: &mut EntitlementView| v.source_status = SourceStatus::Expired,
            |v: &mut EntitlementView| v.next_limit_change_at_ms = Some(150),
            |v: &mut EntitlementView| v.effective_stop_at_ms = Some(150),
        ] {
            let mut invalid = view.clone();
            change(&mut invalid);
            assert_eq!(
                device_plan_from_view(&invalid, home, &b, 150),
                Err(Error::MembershipStale)
            );
        }
        for status in [SourceStatus::Free, SourceStatus::Expired] {
            let mut free = view.clone();
            free.plan_snapshot = default_plans(1).remove(0);
            free.source_status = status;
            assert_eq!(
                device_plan_from_view(&free, home, &b, 150),
                Ok(PlanId::Free)
            );
        }
    }
}
