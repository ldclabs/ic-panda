use candid::Principal;
use dmsg_protocol::billing::*;
use dmsg_types::{billing::*, integration::PANDA_LEASE_MS, membership::*, *};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Subject {
    pub beneficiary: Beneficiary,
    pub created_at_ms: Option<u64>,
    pub business_revision: u64,
    pub lease_revision: u64,
    pub contracts: Vec<MembershipContract>,
    pub addons: Vec<StorageAddon>,
    pub view: Option<EntitlementView>,
    pub busy_until_ms: u64,
    pub generation: u64,
    pub retry_after_ms: u64,
}

impl Subject {
    pub fn new(b: Beneficiary) -> Self {
        Self {
            beneficiary: b,
            created_at_ms: None,
            business_revision: 0,
            lease_revision: 0,
            contracts: vec![],
            addons: vec![],
            view: None,
            busy_until_ms: 0,
            generation: 0,
            retry_after_ms: 0,
        }
    }

    pub fn active(&self, at: u64) -> Option<&MembershipContract> {
        self.contracts
            .iter()
            .rev()
            .find(|c| c.starts_at_ms <= at && at < end(c))
    }
}

pub fn end(c: &MembershipContract) -> u64 {
    c.expires_at_ms.min(c.terminated_at_ms.unwrap_or(u64::MAX))
}

/// Retained storage add-ons per subject; expired add-ons do not count.
pub const MAX_ADDONS: usize = 64;
const MAX_PAUSES: usize = 128;

/// Record a qualification result. Known and unverifiable losses both pause paid execution time.
pub fn set_eligibility(
    c: &mut MembershipContract,
    eligibility: Eligibility,
    at: u64,
) -> Result<()> {
    let open = c
        .resource_pauses
        .last()
        .is_some_and(|(_, end)| end.is_none());
    if eligibility == Eligibility::Eligible {
        if open {
            c.resource_pauses.last_mut().expect("open pause").1 = Some(at);
        }
    } else if !open {
        ensure(c.resource_pauses.len() < MAX_PAUSES, Error::QuotaExceeded)?;
        c.resource_pauses.push((at, None));
    }
    if eligibility != Eligibility::Ineligible {
        c.repair_deadline_ms = None;
    }
    c.eligibility = eligibility;
    Ok(())
}

pub fn add_storage(s: &mut Subject, addon: StorageAddon, at: u64) -> Result<()> {
    s.addons.retain(|a| a.expires_at_ms > at);
    ensure(s.addons.len() < MAX_ADDONS, Error::QuotaExceeded)?;
    s.addons.push(addon);
    Ok(())
}

pub fn plan(catalog: &Catalog, id: &PlanId) -> Result<PlanVersion> {
    catalog
        .plans
        .iter()
        .find(|p| p.plan_id == *id)
        .cloned()
        .ok_or(Error::NotFound)
}

pub fn validate_catalog(c: &Catalog) -> Result<()> {
    ensure_valid(
        c.schema == 1 && c.version > 0 && c.plans.len() == 4 && c.storage_products.len() <= 16,
        "resource catalog",
    )?;
    for id in [PlanId::Free, PlanId::Plus, PlanId::Pro, PlanId::Max] {
        let p = plan(c, &id)?;
        ensure_valid(
            p.catalog_version == c.version
                && p.weights.ed25519 > 0
                && p.weights.ecdsa_secp256k1 > 0
                && p.weights.version > 0
                && p.limits.storage_bytes <= 1_099_511_627_776
                && p.limits.active_channels <= 10_000
                && p.limits.monthly_execution_units <= 100_000,
            "plan",
        )?;
    }
    ensure_valid(
        c.plans.iter().all(|p| p.weights == c.plans[0].weights),
        "uniform execution weights",
    )?;
    ensure_valid(plan(c, &PlanId::Free)?.price_cents == 0, "Free price")?;
    for p in &c.storage_products {
        ensure_valid(
            p.price_cents > 0 && p.storage_bytes > 0 && p.storage_bytes <= 1_099_511_627_776,
            "storage product",
        )?;
    }
    Ok(())
}

pub fn project(
    home: Principal,
    s: &mut Subject,
    catalog: &Catalog,
    at: u64,
) -> Result<EntitlementView> {
    let free = plan(catalog, &PlanId::Free)?;
    let active = s.active(at).cloned();
    let mut p = free;
    let mut status = if s.contracts.is_empty() {
        SourceStatus::Free
    } else {
        SourceStatus::Expired
    };
    let mut eligibility = Eligibility::Eligible;
    let mut sources = vec![];
    let mut until = at.saturating_add(PANDA_LEASE_MS);
    let mut observed = at;
    let mut repair = None;
    let mut terminated = s
        .contracts
        .iter()
        .filter_map(|c| {
            c.terminated_at_ms
                .or(Some(c.expires_at_ms))
                .filter(|t| *t <= at)
        })
        .max();
    if let Some(c) = &active {
        eligibility = c.eligibility.clone();
        observed = c.observed_at_ms;
        repair = c.repair_deadline_ms;
        if c.eligibility == Eligibility::Unverifiable
            || (c.eligibility == Eligibility::Eligible
                && matches!(c.source, ContractSource::Sns { .. })
                && at >= c.qualified_until_ms)
        {
            status = SourceStatus::Unverifiable;
            eligibility = Eligibility::Unverifiable;
        } else if c.eligibility == Eligibility::Ineligible {
            match repair {
                Some(d) if at >= d => {
                    status = SourceStatus::Suspended;
                    terminated = repair;
                }
                _ => {
                    status = SourceStatus::RepairRequired;
                    until = until.min(repair.unwrap_or(u64::MAX));
                }
            }
        } else {
            p = c.plan.clone();
            sources.push(c.contract_id);
            status = SourceStatus::Active;
            until = until.min(end(c)).min(c.qualified_until_ms);
        }
    }
    let mut limits = p.limits.clone();
    let mut addons = vec![];
    for a in &s.addons {
        if a.starts_at_ms <= at && at < a.expires_at_ms {
            limits.storage_bytes = limits
                .storage_bytes
                .checked_add(a.storage_bytes)
                .ok_or(Error::QuotaExceeded)?;
            sources.push(a.contract_id);
            until = until.min(a.expires_at_ms);
            addons.push(a.clone());
        }
    }
    let next = s
        .contracts
        .iter()
        .flat_map(|c| [c.starts_at_ms, end(c)])
        .chain(
            s.addons
                .iter()
                .flat_map(|a| [a.starts_at_ms, a.expires_at_ms]),
        )
        .filter(|t| *t > at)
        .min();
    if let Some(t) = next {
        until = until.min(t);
    }
    // An unknown paid qualification never silently becomes a reusable Free lease.
    if status == SourceStatus::Unverifiable {
        until = at;
    }
    if status == SourceStatus::Active {
        terminated = None;
    }
    s.lease_revision = s
        .lease_revision
        .checked_add(1)
        .ok_or(Error::QuotaExceeded)?;
    for a in &mut s.addons {
        if sources.contains(&a.contract_id) {
            a.last_issued_until_ms = a.last_issued_until_ms.max(until);
        }
    }
    let v = EntitlementView {
        schema: 1,
        home_commerce: home,
        beneficiary: s.beneficiary.clone(),
        business_revision: s.business_revision,
        lease_revision: s.lease_revision,
        active_contract_id: active.as_ref().map(|c| c.contract_id),
        plan_snapshot: p,
        addons,
        effective_limits: limits,
        next_limit_change_at_ms: next,
        lease_source_contract_ids: sources,
        source_status: status,
        eligibility_status: eligibility,
        observed_at_ms: observed,
        issued_at_ms: at,
        valid_until_ms: until,
        effective_stop_at_ms: None,
        repair_deadline_ms: repair,
        service_terminated_at_ms: terminated,
    };
    s.view = Some(v.clone());
    Ok(v)
}

pub fn month(s: &Subject, catalog: &Catalog, month: u32) -> Result<MonthEntitlement> {
    let created = s.created_at_ms.ok_or(Error::NotFound)?;
    let (start, end_at) = month_bounds(month)?;
    let start = start.max(created).min(end_at);
    let free = plan(catalog, &PlanId::Free)?;
    let mut points = vec![start, end_at];
    for c in &s.contracts {
        for p in [c.starts_at_ms, end(c)].into_iter().chain(
            c.resource_pauses
                .iter()
                .flat_map(|(a, b)| [*a, b.unwrap_or(end_at)]),
        ) {
            if p > start && p < end_at {
                points.push(p);
            }
        }
    }
    points.sort_unstable();
    points.dedup();
    let mut segments: Vec<MonthSegment> = vec![];
    for w in points.windows(2) {
        let c = s.active(w[0]).filter(|c| {
            !c.resource_pauses
                .iter()
                .any(|(a, b)| *a <= w[0] && w[0] < b.unwrap_or(u64::MAX))
        });
        let units = c.map_or(free.limits.monthly_execution_units, |c| {
            c.plan.limits.monthly_execution_units
        });
        let source = c.map(|c| c.contract_id);
        if let Some(last) = segments
            .last_mut()
            .filter(|s| s.monthly_units == units && s.source_contract_id == source)
        {
            last.end_ms = w[1];
        } else {
            segments.push(MonthSegment {
                start_ms: w[0],
                end_ms: w[1],
                monthly_units: units,
                source_contract_id: source,
            });
        }
    }
    let allowed = monthly_allowance(month, created, &segments)?;
    Ok(MonthEntitlement {
        beneficiary: s.beneficiary.clone(),
        month_utc: month,
        month_revision: s.lease_revision,
        business_revision: s.business_revision,
        weights: free.weights,
        segments,
        allowed_units: allowed,
        calculation_version: 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dmsg_protocol::commerce_v2::REPAIR_WINDOW_MS;

    const HOME: Principal = Principal::from_slice(&[1]);

    fn catalog() -> Catalog {
        Catalog {
            schema: 1,
            version: 1,
            effective_at_ms: 0,
            plans: default_plans(1),
            storage_products: vec![],
            terms_digest: Hash::new([9; 32]),
        }
    }

    fn subject(start: u64, end: u64) -> Subject {
        let mut s = Subject::new(beneficiary(HOME, &AccountId([2; 12])));
        s.created_at_ms = Some(0);
        s.contracts.push(MembershipContract {
            term_starts_at_ms: start,
            resource_pauses: vec![],
            contract_id: Hash::new([3; 32]),
            plan: plan(&catalog(), &PlanId::Max).unwrap(),
            source: ContractSource::Sns {
                claim_id: Hash::new([4; 32]),
            },
            starts_at_ms: start,
            expires_at_ms: end,
            terminated_at_ms: None,
            eligibility: Eligibility::Eligible,
            observed_at_ms: start,
            qualified_until_ms: end,
            repair_deadline_ms: None,
        });
        s
    }

    #[test]
    fn unverifiable_time_is_paused_like_known_ineligibility() {
        let (start, end) = month_bounds(202609).unwrap();
        let mid = start + (end - start) / 2;
        let mut s = subject(start, end + DAY);
        set_eligibility(&mut s.contracts[0], Eligibility::Unverifiable, mid).unwrap();
        set_eligibility(&mut s.contracts[0], Eligibility::Ineligible, mid + 1).unwrap();
        assert_eq!(s.contracts[0].resource_pauses, vec![(mid, None)]);
        set_eligibility(&mut s.contracts[0], Eligibility::Eligible, end - 1).unwrap();
        assert_eq!(s.contracts[0].resource_pauses, vec![(mid, Some(end - 1))]);
        let m = month(&s, &catalog(), 202609).unwrap();
        assert_eq!(m.segments.len(), 3);
        assert_eq!(m.segments[1].source_contract_id, None);
        assert_eq!(m.segments[1].monthly_units, 3);
        assert!(m.allowed_units < 200);
    }

    #[test]
    fn repair_deadline_moves_the_view_to_suspended() {
        let mut s = subject(1_000, 1_000 + 365 * DAY);
        let at = 2_000;
        set_eligibility(&mut s.contracts[0], Eligibility::Ineligible, at).unwrap();
        s.contracts[0].repair_deadline_ms = Some(at + REPAIR_WINDOW_MS);
        let v = project(HOME, &mut s, &catalog(), at).unwrap();
        assert_eq!(v.source_status, SourceStatus::RepairRequired);
        assert_eq!(v.repair_deadline_ms, Some(at + REPAIR_WINDOW_MS));
        assert!(v.valid_until_ms <= at + REPAIR_WINDOW_MS);
        let v = project(HOME, &mut s, &catalog(), at + REPAIR_WINDOW_MS).unwrap();
        assert_eq!(v.source_status, SourceStatus::Suspended);
        assert_eq!(v.service_terminated_at_ms, Some(at + REPAIR_WINDOW_MS));
        set_eligibility(&mut s.contracts[0], Eligibility::Eligible, at + 1).unwrap();
        assert_eq!(s.contracts[0].repair_deadline_ms, None);
    }

    #[test]
    fn expired_storage_does_not_hold_add_on_capacity() {
        let mut s = subject(0, 365 * DAY);
        let addon = |i: u8, expires_at_ms| StorageAddon {
            contract_id: Hash::new([i; 32]),
            order_id: Hash::new([i; 32]),
            storage_bytes: 1,
            starts_at_ms: 0,
            expires_at_ms,
            last_issued_until_ms: 0,
        };
        for i in 0..MAX_ADDONS as u8 {
            add_storage(&mut s, addon(i, 10), 0).unwrap();
        }
        assert_eq!(
            add_storage(&mut s, addon(100, 20), 5),
            Err(Error::QuotaExceeded)
        );
        add_storage(&mut s, addon(100, 20), 10).unwrap();
        assert_eq!(s.addons.len(), 1);
    }
}
