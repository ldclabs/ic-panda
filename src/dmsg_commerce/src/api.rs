//! dMsg resource catalog and bounded leases. Money and PANDA delivery use the v2 adapters.
use crate::{
    model::{self, Subject},
    store::*,
};
use candid::Principal;
use dmsg_protocol::{billing::*, *};
use dmsg_runtime::storage::MapExt;
use dmsg_types::{billing::*, membership::*, *};

pub(crate) fn now() -> u64 {
    nanos_to_millis(ic_cdk::api::time())
}

pub(crate) fn valid_subject(b: &Beneficiary) -> Result<()> {
    beneficiary_account(b)?;
    ensure(
        config(|c| c.user_homes.contains(&b.authority_canister)),
        Error::Forbidden,
    )
}

pub(crate) fn get_subject(b: &Beneficiary) -> Result<Subject> {
    valid_subject(b)?;
    match load(b) {
        Ok(s) => Ok(s),
        Err(Error::NotFound) => {
            ensure(
                SUBJECTS.with_borrow(|t| t.len()) < config(|c| c.max_subjects),
                Error::QuotaExceeded,
            )?;
            Ok(Subject::new(b.clone()))
        }
        Err(e) => Err(e),
    }
}

fn publish(canister_id: Principal, s: &mut Subject, at: u64) -> Result<EntitlementView> {
    let v = model::project(canister_id, s, &catalog(at), at)?;
    save(s);
    Ok(v)
}

#[ic_cdk::init]
fn init(args: CommerceInit) {
    let at = now();
    for p in [args.governance, args.membership_canister] {
        authenticated(p).expect("configuration");
    }
    assert!(
        !args.user_homes.is_empty()
            && args.user_homes.len() <= 16
            && args.max_subjects > 0
            && args.max_subjects <= 1_000_000
            && args.daily_orders > 0
            && args.daily_orders <= 100_000
    );
    model::validate_catalog(&args.catalog).expect("catalog");
    assert!(args.catalog.effective_at_ms <= at);
    save_catalog(&args.catalog);
    set_config(Config::new(args));
    persist_config();
    rebuild(at);
}

#[ic_cdk::pre_upgrade]
fn pre_upgrade() {
    persist_config();
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    load_catalogs();
    rebuild(now());
}

#[ic_cdk::update]
fn schedule_policy(c: Catalog) -> Result<()> {
    let at = now();
    check_governance(ic_cdk::api::msg_caller())?;
    model::validate_catalog(&c)?;
    let old = latest_catalog();
    ensure_valid(
        c.effective_at_ms >= at.saturating_add(30 * DAY)
            && c.effective_at_ms > old.effective_at_ms
            && c.version > old.version,
        "catalog notice",
    )?;
    ensure(
        CATALOGS.with_borrow(|t| t.len()) < 256,
        Error::QuotaExceeded,
    )?;
    // A single month must never mix algorithm weight denominations.
    ensure_valid(
        c.plans[0].weights == old.plans[0].weights
            || month_bounds(month_utc(c.effective_at_ms)?)?.0 == c.effective_at_ms,
        "weight change at UTC month boundary",
    )?;
    save_catalog(&c);
    Ok(())
}

#[ic_cdk::update]
fn refresh_catalog() -> Catalog {
    let c = catalog(now());
    certify(catalog_key().to_vec(), &c);
    c
}

#[ic_cdk::query]
fn get_catalog() -> Result<CertifiedBatch> {
    CERT.with_borrow(|c| c.batch(ic_cdk::api::canister_self(), vec![catalog_key().to_vec()]))
}

#[ic_cdk::query]
fn list_catalogs(after_version: Option<u64>) -> Vec<Catalog> {
    CATALOGS
        .with_borrow(|t| {
            t.page(
                after_version.map_or_else(Vec::new, |v| v.to_be_bytes().to_vec()),
                64,
            )
        })
        .into_iter()
        .map(|(_, c)| c)
        .collect()
}

#[ic_cdk::update]
fn set_admission_pause(paused: bool) -> Result<()> {
    check_governance(ic_cdk::api::msg_caller())?;
    set_paused(paused);
    Ok(())
}

async fn refresh(
    b: Beneficiary,
    canister_id: Principal,
    mut at: u64,
) -> Result<(EntitlementView, u64)> {
    valid_subject(&b)?;
    let mut s = load(&b)?;
    if let Some(v) = &s.view {
        if at.saturating_add(MINUTE) < v.valid_until_ms
            && v.business_revision == s.business_revision
        {
            return Ok((v.clone(), at));
        }
    }
    let source = s.active(at).and_then(|c| match c.source {
        ContractSource::Sns { claim_id } => Some((c.contract_id, claim_id)),
        _ => None,
    });
    if let Some((contract, id)) = source {
        ensure(at >= s.busy_until_ms, Error::Pending)?;
        if at < s.retry_after_ms {
            return s
                .view
                .clone()
                .map(|v| (v, at))
                .ok_or(Error::MembershipStale);
        }
        reserve_call(at, CallBudget::Refresh)?;
        s.generation += 1;
        let generation = s.generation;
        s.busy_until_ms = at + MINUTE;
        save_subject(&s);
        let response: Result<Result<dmsg_types::integration_membership::PandaClaimView>> =
            dmsg_runtime::call(
                config(|c| c.membership_canister),
                "refresh_panda_claim",
                (id,),
            )
            .await;
        at = now();
        s = load(&b)?;
        ensure(s.generation == generation, Error::VersionConflict)?;
        s.busy_until_ms = 0;
        save_subject(&s);
        match response {
            Ok(Ok(view))
                if view.claim_id == id
                    && view.terms.offer.beneficiary == b
                    && view.terms.home_membership == config(|c| c.membership_canister) =>
            {
                crate::product::observe(&view, at)?;
            }
            _ => {
                s.retry_after_ms = at.saturating_add(MINUTE);
                let marked = s
                    .contracts
                    .iter_mut()
                    .find(|c| c.contract_id == contract)
                    .map_or(Ok(()), |c| {
                        model::set_eligibility(c, Eligibility::Unverifiable, at)
                    });
                save_subject(&s);
                marked?;
            }
        }
        s = load(&b)?;
    }
    Ok((publish(canister_id, &mut s, at)?, at))
}

#[ic_cdk::update]
async fn refresh_entitlement(b: Beneficiary) -> Result<EntitlementView> {
    refresh(b, ic_cdk::api::canister_self(), now())
        .await
        .map(|(view, _)| view)
}

#[ic_cdk::query]
fn get_entitlement_batch(subjects: Vec<Beneficiary>) -> Result<CertifiedBatch> {
    for b in &subjects {
        valid_subject(b)?;
    }
    CERT.with_borrow(|c| {
        c.batch(
            ic_cdk::api::canister_self(),
            subjects
                .iter()
                .map(|b| entitlement_key(b).to_vec())
                .collect(),
        )
    })
}

#[ic_cdk::update]
async fn get_execution_entitlement(
    b: Beneficiary,
    month: u32,
    account_created_at_ms: u64,
) -> Result<ExecutionEntitlement> {
    let at = now();
    ensure(
        ic_cdk::api::msg_caller() == b.authority_canister
            && account_created_at_ms <= at
            && month == month_utc(at)?,
        Error::Forbidden,
    )?;
    let mut s = get_subject(&b)?;
    if let Some(created) = s.created_at_ms {
        ensure(created == account_created_at_ms, Error::IntegrityFailed)?;
    } else {
        s.created_at_ms = Some(account_created_at_ms);
        save(&s);
    }
    let (view, at) = refresh(b.clone(), ic_cdk::api::canister_self(), at).await?;
    let s = load(&b)?;
    let month = model::month(&s, &catalog(at), month)?;
    Ok(ExecutionEntitlement { view, month })
}
