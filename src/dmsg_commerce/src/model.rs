use candid::Principal;
use dmsg_protocol::{billing::*, membership::*, *};
use dmsg_types::{billing::*, membership::*, *};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Subject {
    pub beneficiary: Beneficiary,
    pub created_at_ms: Option<u64>,
    pub business_revision: u64,
    pub lease_revision: u64,
    pub contracts: Vec<MembershipContract>,
    pub addons: Vec<StorageAddon>,
    pub first_cash_order: Option<Hash>,
    pub self_refund_used: bool,
    pub view: Option<EntitlementView>,
    pub busy_until_ms: u64,
    pub generation: u64,
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
            first_cash_order: None,
            self_refund_used: false,
            view: None,
            busy_until_ms: 0,
            generation: 0,
        }
    }

    pub fn active(&self, at: u64) -> Option<&MembershipContract> {
        self.contracts
            .iter()
            .rev()
            .find(|c| c.starts_at_ms <= at && at < end(c))
    }

    pub fn bump(&mut self) {
        self.business_revision = self
            .business_revision
            .checked_add(1)
            .expect("business revision");
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
    authenticated(c.ledger)?;
    ensure(
        c.schema == 1
            && c.version > 0
            && c.decimals == 6
            && c.plans.len() == 4
            && c.storage_products.len() <= 16
            && c.ledger_fee > 0,
        invalid("ckUSDC catalog"),
    )?;
    for id in [PlanId::Free, PlanId::Plus, PlanId::Pro, PlanId::Max] {
        let p = plan(c, &id)?;
        ensure(
            p.catalog_version == c.version
                && p.weights.ed25519 > 0
                && p.weights.ecdsa_secp256k1 > 0
                && p.weights.version > 0
                && p.limits.storage_bytes <= 1_099_511_627_776
                && p.limits.active_channels <= 10_000
                && p.limits.monthly_execution_units <= 100_000,
            invalid("plan"),
        )?;
    }
    ensure(
        plan(c, &PlanId::Free)?.price_cents == 0,
        invalid("Free price"),
    )?;
    for p in &c.storage_products {
        ensure(
            p.price_cents > 0
                && p.storage_bytes > 0
                && p.duration_ms > 0
                && p.duration_ms <= 366 * DAY,
            invalid("storage product"),
        )?;
    }
    Ok(())
}

pub fn quote(
    home: Principal,
    s: &Subject,
    catalog: Catalog,
    request: QuoteOrder,
    at: u64,
) -> Result<OrderQuote> {
    ensure(
        request.expected_business_revision == s.business_revision,
        Error::VersionConflict,
    )?;
    authenticated(request.payer.owner)?;
    nonzero(request.op_id.as_slice())?;
    let current = s.active(at);
    let mut term = TermRule::CalendarYear;
    let amount = match &request.action {
        OrderAction::Subscribe { plan: id } => {
            ensure(
                current.is_none()
                    && !s
                        .contracts
                        .iter()
                        .any(|c| c.starts_at_ms > at && end(c) > c.starts_at_ms),
                Error::VersionConflict,
            )?;
            let p = plan(&catalog, id)?;
            ensure(p.price_cents > 0, invalid("paid plan"))?;
            cents_atomic(p.price_cents, catalog.decimals)?
        }
        OrderAction::Renew { plan: id } => {
            let old = current.ok_or(Error::NotFound)?;
            ensure(
                at.saturating_add(30 * DAY) >= old.expires_at_ms
                    && old.closing_at_ms.is_none()
                    && !s
                        .contracts
                        .iter()
                        .any(|c| c.starts_at_ms >= old.expires_at_ms && end(c) > c.starts_at_ms),
                Error::VersionConflict,
            )?;
            term = TermRule::Fixed {
                starts_at_ms: old.expires_at_ms,
                expires_at_ms: next_year(old.expires_at_ms)?,
            };
            let p = plan(&catalog, id)?;
            ensure(p.price_cents > 0, invalid("paid plan"))?;
            cents_atomic(p.price_cents, catalog.decimals)?
        }
        OrderAction::Upgrade { plan: id } => {
            let old = current.ok_or(Error::NotFound)?;
            ensure(
                matches!(old.source, ContractSource::Cash { .. })
                    && old.closing_at_ms.is_none()
                    && !s
                        .contracts
                        .iter()
                        .any(|c| c.starts_at_ms > at && end(c) > c.starts_at_ms),
                Error::VersionConflict,
            )?;
            let p = plan(&catalog, id)?;
            ensure(
                p.price_cents > old.plan.price_cents
                    && p.limits.storage_bytes >= old.plan.limits.storage_bytes
                    && p.limits.monthly_execution_units >= old.plan.limits.monthly_execution_units,
                invalid("upgrade"),
            )?;
            term = TermRule::Fixed {
                starts_at_ms: at,
                expires_at_ms: old.expires_at_ms,
            };
            let diff = cents_atomic(p.price_cents - old.plan.price_cents, catalog.decimals)?;
            mul_div(
                diff,
                (old.expires_at_ms - at).into(),
                (old.expires_at_ms - old.term_starts_at_ms).into(),
                true,
            )?
        }
        OrderAction::Storage { product_id } => {
            let p = catalog
                .storage_products
                .iter()
                .find(|p| p.product_id == *product_id)
                .ok_or(Error::NotFound)?;
            cents_atomic(p.price_cents, catalog.decimals)?
        }
        OrderAction::Buyout { contract_id } => {
            let old = current.ok_or(Error::NotFound)?;
            ensure(
                old.contract_id == *contract_id
                    && matches!(old.source, ContractSource::Sns { .. })
                    && old.closing_at_ms.is_none(),
                Error::VersionConflict,
            )?;
            term = TermRule::Fixed {
                starts_at_ms: at,
                expires_at_ms: old.expires_at_ms,
            };
            mul_div(
                cents_atomic(old.plan.price_cents, catalog.decimals)?,
                (old.expires_at_ms - at).into(),
                (old.expires_at_ms - old.term_starts_at_ms).into(),
                true,
            )?
        }
    };
    ensure(amount > 0, invalid("zero order"))?;
    let fee_reserve = catalog
        .ledger_fee
        .checked_mul(3)
        .ok_or(Error::QuotaExceeded)?;
    Ok(OrderQuote {
        home_commerce: home,
        request,
        catalog,
        amount_atomic: amount,
        fee_reserve,
        created_at_ms: at,
        fund_by_ms: at.checked_add(15 * MINUTE).ok_or(Error::QuotaExceeded)?,
        activate_by_ms: at.checked_add(45 * MINUTE).ok_or(Error::QuotaExceeded)?,
        term,
    })
}

pub fn new_order(home: Principal, input: OpenOrder) -> BillingOrder {
    let id = digest(
        "dmsg/commerce/order-id/v1",
        &(
            home,
            input.quote.request.payer.owner,
            input.quote.request.op_id,
        ),
    );
    BillingOrder {
        order_id: id,
        receive_subaccount: digest("dmsg/commerce/subaccount/v1", &(home, id)),
        input,
        status: OrderStatus::Authorizing,
        activated_contract_id: None,
        funding_block: None,
        close_effective_at_ms: None,
        confirmed_in: 0,
        service_reserve: 0,
        earned: 0,
        refundable: 0,
        fee_reserve: 0,
        outgoing: 0,
        transferred: 0,
        network_fees: 0,
        refunded_principal: 0,
        next_transfer: 0,
        busy_until_ms: 0,
        generation: 0,
    }
}

pub fn conserved(o: &BillingOrder) -> bool {
    [
        o.service_reserve,
        o.earned,
        o.refundable,
        o.fee_reserve,
        o.outgoing,
        o.transferred,
        o.network_fees,
    ]
    .into_iter()
    .try_fold(0u128, |a, b| a.checked_add(b))
        == Some(o.confirmed_in)
}

pub fn activate(s: &mut Subject, o: &mut BillingOrder, at: u64) -> Result<()> {
    let q = &o.input.quote;
    ensure(
        s.business_revision == q.request.expected_business_revision
            && at < q.activate_by_ms
            && o.activated_contract_id.is_none(),
        Error::VersionConflict,
    )?;
    ensure(
        s.contracts.len() < 256 && s.addons.len() < 64,
        Error::QuotaExceeded,
    )?;
    let id = digest("dmsg/commerce/contract-id/v1", &o.order_id);
    let prior_term_start = s.active(at).map(|c| c.term_starts_at_ms);
    let (mut start, end_at) = match q.term {
        TermRule::CalendarYear => (at, next_year(at)?),
        TermRule::Fixed {
            starts_at_ms,
            expires_at_ms,
        } => (starts_at_ms, expires_at_ms),
    };
    if let OrderAction::Storage { product_id } = &q.request.action {
        let p = q
            .catalog
            .storage_products
            .iter()
            .find(|p| p.product_id == *product_id)
            .ok_or(Error::NotFound)?;
        s.addons.push(StorageAddon {
            contract_id: id,
            order_id: o.order_id,
            storage_bytes: p.storage_bytes,
            starts_at_ms: at,
            expires_at_ms: at.checked_add(p.duration_ms).ok_or(Error::QuotaExceeded)?,
            last_issued_until_ms: 0,
        });
    } else {
        let p = match &q.request.action {
            OrderAction::Subscribe { plan: id }
            | OrderAction::Renew { plan: id }
            | OrderAction::Upgrade { plan: id } => plan(&q.catalog, id)?,
            OrderAction::Buyout { contract_id } => s
                .contracts
                .iter()
                .find(|c| c.contract_id == *contract_id)
                .ok_or(Error::NotFound)?
                .plan
                .clone(),
            _ => return Err(Error::UnsupportedProtocol),
        };
        if matches!(
            q.request.action,
            OrderAction::Upgrade { .. } | OrderAction::Buyout { .. }
        ) {
            start = at;
            let old = s
                .contracts
                .iter_mut()
                .find(|c| c.starts_at_ms <= at && at < end(c))
                .ok_or(Error::VersionConflict)?;
            old.terminated_at_ms = Some(at);
        }
        ensure(
            start < end_at
                && !s
                    .contracts
                    .iter()
                    .any(|c| c.starts_at_ms < end_at && start < end(c)),
            Error::VersionConflict,
        )?;
        s.contracts.push(MembershipContract {
            term_starts_at_ms: if matches!(
                q.request.action,
                OrderAction::Upgrade { .. } | OrderAction::Buyout { .. }
            ) {
                prior_term_start.unwrap_or(start)
            } else {
                start
            },
            resource_pauses: vec![],
            contract_id: id,
            plan: p,
            source: ContractSource::Cash {
                order_id: o.order_id,
            },
            starts_at_ms: start,
            expires_at_ms: end_at,
            terminated_at_ms: None,
            closing_at_ms: None,
            last_issued_until_ms: 0,
            eligibility: Eligibility::Eligible,
            observed_at_ms: at,
            qualified_until_ms: end_at,
            repair_deadline_ms: None,
            unverifiable_since_ms: None,
        });
        if s.first_cash_order.is_none() {
            s.first_cash_order = Some(o.order_id);
        }
    }
    s.bump();
    o.activated_contract_id = Some(id);
    o.status = OrderStatus::Active;
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
    let mut until = at.saturating_add(MAX_LEASE_MS);
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
        month_revision: s.business_revision,
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
    use dmsg_runtime::storage::{compact_bytes, compact_from_bytes};

    fn principal(n: u8) -> Principal {
        Principal::self_authenticating([n])
    }

    fn subject() -> Subject {
        let mut s = Subject::new(beneficiary(principal(1), &AccountId([1; 12])));
        s.created_at_ms = Some(month_bounds(202609).unwrap().0);
        s
    }

    fn catalog() -> Catalog {
        Catalog {
            schema: 1,
            version: 1,
            effective_at_ms: 0,
            plans: default_plans(1),
            storage_products: vec![],
            ledger: principal(2),
            decimals: 6,
            ledger_fee: 10,
            terms_digest: Hash::new([7; 32]),
        }
    }

    fn order(s: &Subject, action: OrderAction, at: u64) -> BillingOrder {
        let q = quote(
            principal(3),
            s,
            catalog(),
            QuoteOrder {
                op_id: Hash::new([8; 32]),
                beneficiary: s.beneficiary.clone(),
                action,
                expected_business_revision: s.business_revision,
                payer: icrc_ledger_types::icrc1::account::Account {
                    owner: principal(4),
                    subaccount: None,
                },
            },
            at,
        )
        .unwrap();
        new_order(
            principal(3),
            OpenOrder {
                authorization: MembershipIntent {
                    application_id: q.request.op_id,
                    environment: Environment::Local,
                    service_canister: principal(3),
                    beneficiary: s.beneficiary.clone(),
                    actor: principal(4),
                    action_digest: order_digest(&q),
                    nonce: Hash::new([9; 32]),
                    valid_until_ms: at + DAY,
                },
                quote: q,
            },
        )
    }

    #[test]
    fn activation_prorates_month_and_lease_refresh_does_not_change_business_revision() {
        let (a, b) = month_bounds(202609).unwrap();
        let mut s = subject();
        let mut o = order(
            &s,
            OrderAction::Subscribe { plan: PlanId::Plus },
            a + (b - a) / 2,
        );
        activate(&mut s, &mut o, a + (b - a) / 2).unwrap();
        assert_eq!(month(&s, &catalog(), 202609).unwrap().allowed_units, 6);
        let v = project(principal(3), &mut s, &catalog(), a + (b - a) / 2).unwrap();
        let v2 = project(principal(3), &mut s, &catalog(), a + (b - a) / 2 + 1).unwrap();
        assert_eq!(v.business_revision, v2.business_revision);
        assert!(v2.lease_revision > v.lease_revision);
        assert!(activate(&mut s, &mut o, b - 1).is_err());
        assert_eq!(compact_from_bytes::<Subject>(&compact_bytes(&s)), s);
    }

    #[test]
    fn creation_before_month_end_is_not_a_whole_free_month() {
        let (a, b) = month_bounds(202609).unwrap();
        let mut s = subject();
        s.created_at_ms = Some(b - 1);
        assert_eq!(month(&s, &catalog(), 202609).unwrap().allowed_units, 0);
        s.created_at_ms = Some(a);
        assert_eq!(month(&s, &catalog(), 202609).unwrap().allowed_units, 3);
    }

    #[test]
    fn storage_survives_base_contract_close() {
        let (a, _) = month_bounds(202609).unwrap();
        let mut s = subject();
        let mut o = order(&s, OrderAction::Subscribe { plan: PlanId::Plus }, a);
        activate(&mut s, &mut o, a).unwrap();
        s.addons.push(StorageAddon {
            contract_id: Hash::new([2; 32]),
            order_id: Hash::new([3; 32]),
            storage_bytes: 107_374_182_400,
            starts_at_ms: a,
            expires_at_ms: a + DAY,
            last_issued_until_ms: 0,
        });
        s.contracts[0].closing_at_ms = Some(a + 1000);
        let v = project(principal(3), &mut s, &catalog(), a + 1001).unwrap();
        assert_eq!(v.plan_snapshot.plan_id, PlanId::Free);
        assert_eq!(
            v.effective_limits.storage_bytes,
            107_374_182_400 + 104_857_600
        );
    }

    #[test]
    fn unknown_sns_cannot_issue_a_new_paid_or_free_lease() {
        let (a, _) = month_bounds(202609).unwrap();
        let mut s = subject();
        let mut o = order(&s, OrderAction::Subscribe { plan: PlanId::Plus }, a);
        activate(&mut s, &mut o, a).unwrap();
        s.contracts[0].source = ContractSource::Sns {
            claim_id: Hash::new([5; 32]),
        };
        s.contracts[0].eligibility = Eligibility::Unverifiable;
        let v = project(principal(3), &mut s, &catalog(), a + 1).unwrap();
        assert_eq!(v.valid_until_ms, a + 1);
        assert_eq!(v.source_status, SourceStatus::Unverifiable);
        assert!(v.service_terminated_at_ms.is_none());
    }

    #[test]
    fn totals_do_not_treat_a_panda_claim_as_cash() {
        let s = subject();
        let o = order(
            &s,
            OrderAction::Subscribe { plan: PlanId::Max },
            month_bounds(202609).unwrap().0,
        );
        assert!(conserved(&o));
        let mut wrong = o;
        wrong.service_reserve = 1;
        assert!(!conserved(&wrong));
    }
}
