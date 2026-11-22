//! Verifier-authorized fixed grants. An application and its receipt are one IC message.
use crate::{api, model, product, store};
use candid::Principal;
use dmsg_protocol::{authenticated, digest};
use dmsg_runtime::{
    admin::{Validation, validation},
    storage::{MapExt, Stored},
};
use dmsg_types::{
    billing::*,
    membership::{Beneficiary, Eligibility},
    promo::*,
    *,
};
use ic_stable_structures::{
    DefaultMemoryImpl, StableBTreeMap, StableCell, memory_manager::VirtualMemory,
};
use std::{cell::RefCell, collections::BTreeSet};

type Memory = VirtualMemory<DefaultMemoryImpl>;

thread_local! {
    static CONFIG: RefCell<StableCell<Stored<PromoConfig>, Memory>> = RefCell::new(StableCell::init(store::memory(41), Stored(PromoConfig::default())));
    static RECEIPTS: RefCell<StableBTreeMap<Vec<u8>, Stored<PromoReceipt>, Memory>> = RefCell::new(StableBTreeMap::init(store::memory(42)));
    static COUNTS: RefCell<StableCell<Stored<Vec<PromoCount>>, Memory>> = RefCell::new(StableCell::init(store::memory(43), Stored(vec![])));
}

fn validate(config: &PromoConfig) -> Result<()> {
    ensure_valid(
        config.verifiers.len() <= 8 && (!config.enabled || !config.verifiers.is_empty()),
        "promo verifiers",
    )?;
    let mut unique = BTreeSet::new();
    for p in &config.verifiers {
        authenticated(*p)?;
        ensure_valid(unique.insert(*p), "duplicate verifier")?;
    }
    Ok(())
}

#[ic_cdk::update]
fn set_promo_config(config: PromoConfig) -> Result<()> {
    store::check_admin(ic_cdk::api::msg_caller())?;
    validate(&config)?;
    CONFIG.with_borrow_mut(|c| c.set(Stored(config)));
    Ok(())
}

#[ic_cdk::query]
fn validate_set_promo_config(config: PromoConfig) -> Validation {
    validation(validate(&config).map(|()| {
        format!(
            "Set promotional grants enabled={} with {} verifiers",
            config.enabled,
            config.verifiers.len()
        )
    }))
}

#[ic_cdk::query]
fn get_promo_config() -> PromoConfig {
    CONFIG.with_borrow(|c| c.get().0.clone())
}

fn grants_enabled() -> bool {
    CONFIG.with_borrow(|c| c.get().0.enabled) && !store::config(|c| c.paused)
}

#[ic_cdk::query]
fn promo_stats() -> Vec<PromoCount> {
    COUNTS.with_borrow(|c| c.get().0.clone())
}

fn authorized(caller: Principal, allow_admin: bool) -> Result<()> {
    ensure(
        CONFIG.with_borrow(|c| c.get().0.verifiers.contains(&caller))
            || (allow_admin && store::check_admin(caller).is_ok()),
        Error::Forbidden,
    )
}

fn count(grant: &PromoGrant, revoked: bool) {
    COUNTS.with_borrow_mut(|c| {
        let mut rows = c.get().0.clone();
        let days = ((grant.expires_at_ms - grant.starts_at_ms) / DAY) as u16;
        let index = rows.iter().position(|r| {
            r.plan == grant.plan && r.days == days && r.compensation == grant.compensation
        });
        let row = match index {
            Some(i) => &mut rows[i],
            None => {
                rows.push(PromoCount {
                    plan: grant.plan.clone(),
                    days,
                    compensation: grant.compensation,
                    issued: 0,
                    revoked: 0,
                });
                rows.last_mut().unwrap()
            }
        };
        if revoked {
            row.revoked += 1;
        } else {
            row.issued += 1;
        }
        c.set(Stored(rows));
    });
}

fn receipt(id: &Hash) -> Option<PromoReceipt> {
    RECEIPTS.with_borrow(|t| t.load(id.as_slice()))
}

fn record(r: PromoReceipt) -> PromoReceipt {
    RECEIPTS.with_borrow_mut(|t| t.put(r.grant.grant_id.as_slice(), &r));
    r
}

fn validate_grant(g: &PromoGrant) -> Result<()> {
    api::valid_subject(&g.beneficiary)?;
    ensure_valid(
        g.grant_id.iter().any(|b| *b != 0)
            && g.plan != PlanId::Free
            && g.expires_at_ms
                .checked_sub(g.starts_at_ms)
                .is_some_and(|d| d == 30 * DAY || d == 365 * DAY),
        "grant terms",
    )
}

/// Attribute a real purchase to the latest marketing grant. The receipt survives pruning.
pub(crate) fn observe_purchase(s: &model::Subject, at: u64) {
    let Some(id) = s.attribution_grant_id else {
        return;
    };
    let Some(mut r) = receipt(&id) else { return };
    if r.outcome != PromoOutcome::Applied {
        return;
    }
    let limit = r.grant.expires_at_ms.saturating_add(30 * DAY);
    let first = s
        .contracts
        .iter()
        .filter(|c| {
            c.source.priority() == 2
                && c.starts_at_ms >= r.grant.starts_at_ms
                && c.starts_at_ms <= at.min(limit)
                && model::end(c) > c.starts_at_ms
        })
        .min_by_key(|c| c.starts_at_ms);
    if let Some(c) = first
        && r.conversion
            .as_ref()
            .is_none_or(|(_, old)| c.starts_at_ms < *old)
    {
        r.conversion = Some((display(c).source_kind, c.starts_at_ms));
        record(r);
    }
}

fn contract_id(g: &PromoGrant) -> Hash {
    digest("dmsg/promo/contract/v1", &g.grant_id)
}

fn admission(s: &model::Subject, grant: &PromoGrant, at: u64) -> Result<Option<&'static str>> {
    if at >= grant.expires_at_ms || grant.starts_at_ms > at.saturating_add(MINUTE) {
        return Ok(Some("PROMO_EXPIRED"));
    }
    for c in &s.contracts {
        if c.source.is_sandbox() || model::end(c) <= grant.starts_at_ms {
            continue;
        }
        if !c.source.is_grant()
            && c.starts_at_ms <= at
            && model::end(c) > at
            && (c.eligibility == Eligibility::Unverifiable
                || (c.eligibility == Eligibility::Eligible && c.qualified_until_ms <= at))
        {
            return Err(Error::MembershipStale);
        }
        return Ok(Some(if c.source.is_grant() {
            "PROMO_ACTIVE_GRANT"
        } else {
            "PROMO_ACTIVE_PAID_PLAN"
        }));
    }
    Ok(None)
}

fn save_projection(home: Principal, s: &mut model::Subject, at: u64) -> Result<()> {
    s.business_revision = s
        .business_revision
        .checked_add(1)
        .ok_or(Error::QuotaExceeded)?;
    model::project(home, s, &store::catalog(at), at, store::next_catalog_at(at))?;
    store::save(s);
    Ok(())
}

#[ic_cdk::update]
fn apply_promo_grant(grant: PromoGrant) -> Result<PromoReceipt> {
    authorized(ic_cdk::api::msg_caller(), false)?;
    if let Some(r) = receipt(&grant.grant_id) {
        ensure(r.grant == grant, Error::IdempotencyConflict)?;
        return Ok(r);
    }
    validate_grant(&grant)?;
    let at = api::now();
    let mut s = match api::get_subject(&grant.beneficiary) {
        Ok(s) => s,
        Err(Error::QuotaExceeded) => {
            return Ok(record(PromoReceipt {
                grant,
                outcome: PromoOutcome::Rejected("PROMO_CAPACITY".into()),
                business_revision: 0,
                conversion: None,
            }));
        }
        Err(error) => return Err(error),
    };
    let rejection = if !grants_enabled() {
        Some("PROMO_PAUSED")
    } else if product::base_reserved(&grant.beneficiary, at) {
        Some("PROMO_PURCHASE_PENDING")
    } else {
        admission(&s, &grant, at)?
    };
    let outcome = if let Some(reason) = rejection {
        PromoOutcome::Rejected(reason.to_owned())
    } else {
        let plan = model::plan(&store::catalog(at), &grant.plan)?;
        s.contracts.push(MembershipContract {
            term_starts_at_ms: grant.starts_at_ms,
            resource_pauses: vec![],
            contract_id: contract_id(&grant),
            plan,
            source: if grant.compensation {
                ContractSource::Compensation
            } else {
                ContractSource::Promo {
                    grant_id: grant.grant_id,
                }
            },
            starts_at_ms: grant.starts_at_ms,
            expires_at_ms: grant.expires_at_ms,
            terminated_at_ms: None,
            eligibility: Eligibility::Eligible,
            observed_at_ms: at,
            qualified_until_ms: grant.expires_at_ms,
        });
        s.contracts.sort_by_key(|c| c.starts_at_ms);
        if !grant.compensation {
            s.attribution_grant_id = Some(grant.grant_id);
        }
        save_projection(ic_cdk::api::canister_self(), &mut s, at)?;
        count(&grant, false);
        PromoOutcome::Applied
    };
    Ok(record(PromoReceipt {
        grant,
        conversion: None,
        business_revision: s.business_revision,
        outcome,
    }))
}

#[ic_cdk::update]
fn get_promo_receipt(id: Hash) -> Result<Option<PromoReceipt>> {
    authorized(ic_cdk::api::msg_caller(), true)?;
    Ok(receipt(&id))
}

#[ic_cdk::update]
fn revoke_promo_grant(id: Hash) -> Result<PromoReceipt> {
    authorized(ic_cdk::api::msg_caller(), true)?;
    let mut r = receipt(&id).ok_or(Error::NotFound)?;
    if r.outcome != PromoOutcome::Applied {
        return Ok(r);
    }
    let at = api::now();
    // Changing the business revision cannot invalidate a purchase already reserved for delivery.
    ensure(
        !product::base_reserved(&r.grant.beneficiary, at),
        Error::Pending,
    )?;
    let mut s = store::load(&r.grant.beneficiary)?;
    if let Some(c) = s
        .contracts
        .iter_mut()
        .find(|c| c.contract_id == contract_id(&r.grant))
    {
        c.terminated_at_ms = Some(at.max(c.starts_at_ms).min(c.expires_at_ms));
    }
    save_projection(ic_cdk::api::canister_self(), &mut s, at)?;
    count(&r.grant, true);
    r.business_revision = s.business_revision;
    r.outcome = PromoOutcome::Revoked(at);
    Ok(record(r))
}

/// Authority observation for issuing one guest pass. The caller also enforces its annual limit.
#[ic_cdk::update]
fn check_guest_pass_issuer(b: Beneficiary) -> Result<u64> {
    authorized(ic_cdk::api::msg_caller(), false)?;
    ensure(grants_enabled(), Error::Forbidden)?;
    api::valid_subject(&b)?;
    let at = api::now();
    let s = store::load(&b)?;
    let c = s
        .active(at)
        .filter(|c| {
            c.source.priority() == 2
                && c.terminated_at_ms.is_none()
                && c.eligibility == Eligibility::Eligible
        })
        .ok_or(Error::MembershipIneligible)?;
    let until = model::end(c)
        .min(c.qualified_until_ms)
        .min(at.saturating_add(MINUTE));
    ensure(until > at, Error::MembershipStale)?;
    Ok(until)
}

fn display(c: &MembershipContract) -> TermDisplayV3 {
    let (kind, reference, environment, token) = match &c.source {
        ContractSource::Cash { order_id } => ("cash", Some(*order_id), None, None),
        ContractSource::Sns { claim_id } => ("sns", Some(*claim_id), None, None),
        ContractSource::AppStore {
            transaction_id,
            environment,
        } => (
            "app_store",
            Some(*transaction_id),
            environment.as_ref().map(|v| format!("{v:?}")),
            None,
        ),
        ContractSource::GooglePlay {
            transaction_id,
            token_hash,
            environment,
        } => (
            "google_play",
            Some(*transaction_id),
            Some(format!("{environment:?}")),
            Some(*token_hash),
        ),
        ContractSource::Promo { grant_id } => ("promo", Some(*grant_id), None, None),
        ContractSource::Compensation => ("compensation", None, None, None),
    };
    TermDisplayV3 {
        contract_id: c.contract_id,
        plan_id: format!("{:?}", c.plan.plan_id),
        source_kind: kind.into(),
        source_ref: reference,
        is_sandbox: c.source.is_sandbox(),
        store_environment: environment,
        store_token_hash: token,
        term_starts_at_ms: c.term_starts_at_ms,
        starts_at_ms: c.starts_at_ms,
        expires_at_ms: c.expires_at_ms,
        terminated_at_ms: c.terminated_at_ms,
    }
}

fn display_terms(b: Beneficiary, at: u64) -> Result<TermsDisplayV3> {
    api::valid_subject(&b)?;
    let s = match store::load(&b) {
        Ok(s) => s,
        Err(Error::NotFound) => model::Subject::new(b),
        Err(e) => return Err(e),
    };
    let formal = s.active(at).filter(|c| c.source.priority() == 2);
    let mut allowed_actions = vec![];
    if formal.is_none_or(|c| c.eligibility == Eligibility::Eligible && c.qualified_until_ms > at) {
        match formal.map(|c| &c.source) {
            Some(ContractSource::AppStore { .. }) => allowed_actions.push("app_store"),
            Some(ContractSource::GooglePlay { .. }) => allowed_actions.push("google_play"),
            Some(_) => allowed_actions.extend(["cash", "sns"]),
            None => allowed_actions.extend(["cash", "sns", "app_store", "google_play"]),
        }
        if formal.is_some_and(|c| c.terminated_at_ms.is_none()) {
            allowed_actions.push("storage");
        }
    }
    let refund = model::refund_window(&s, at);
    if refund.is_some() {
        allowed_actions.push("refund");
    }
    Ok(TermsDisplayV3 {
        business_revision: s.business_revision,
        contracts: s.contracts.iter().map(display).collect(),
        addons: s
            .addons
            .into_iter()
            .filter(|a| a.expires_at_ms > at)
            .collect(),
        refund,
        allowed_actions: allowed_actions.into_iter().map(str::to_owned).collect(),
    })
}

#[ic_cdk::query]
fn get_membership_terms_v3(b: Beneficiary) -> std::result::Result<TermsDisplayV3, TermsErrorV3> {
    display_terms(b, api::now()).map_err(|e| TermsErrorV3 {
        code: format!("{e:?}"),
    })
}
