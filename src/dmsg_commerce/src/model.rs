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
    c.expires_at_ms
        .min(c.terminated_at_ms.unwrap_or(u64::MAX))
        .min(c.closing_at_ms.unwrap_or(u64::MAX))
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
    let mut stop = None;
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
        if let Some(t) = c.closing_at_ms {
            status = SourceStatus::Closing;
            stop = Some(t);
        } else if c.eligibility == Eligibility::Unverifiable
            || (c.eligibility == Eligibility::Eligible
                && matches!(c.source, ContractSource::Sns { .. })
                && at >= c.qualified_until_ms)
        {
            status = SourceStatus::Unverifiable;
            eligibility = Eligibility::Unverifiable;
        } else if c.eligibility == Eligibility::Ineligible {
            status = if c.repair_deadline_ms.is_some_and(|d| at >= d) {
                SourceStatus::Suspended
            } else {
                SourceStatus::RepairRequired
            };
            if status == SourceStatus::Suspended {
                terminated = c.repair_deadline_ms;
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
    for c in &mut s.contracts {
        if sources.contains(&c.contract_id) {
            c.last_issued_until_ms = c.last_issued_until_ms.max(until);
        }
    }
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
        effective_stop_at_ms: stop,
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
