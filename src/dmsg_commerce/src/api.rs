//! dMsg resource catalog and bounded leases. Money and PANDA delivery use the v2 adapters.
use crate::{
    model::{self, Subject},
    store::*,
};
use candid::Principal;
use dmsg_protocol::{billing::*, *};
use dmsg_runtime::admin::{self, Validation, hex, validation};
use dmsg_runtime::storage::MapExt;
use dmsg_types::{billing::*, integration::LEASE_RENEW_WINDOW_MS, membership::*, *};

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
                SUBJECTS.with_borrow(|t| t.len()) < config(|c| c.limits.max_subjects),
                Error::QuotaExceeded,
            )?;
            Ok(Subject::new(b.clone()))
        }
        Err(e) => Err(e),
    }
}

fn publish(canister_id: Principal, s: &mut Subject, at: u64) -> Result<EntitlementView> {
    let v = model::project(canister_id, s, &catalog(at), at, next_catalog_at(at))?;
    save(s);
    Ok(v)
}

#[ic_cdk::init]
fn init(args: CommerceInit) {
    let at = now();
    for p in [args.governance, args.membership_canister] {
        authenticated(p).expect("configuration");
    }
    assert!(!args.user_homes.is_empty(), "user homes");
    for (index, home) in args.user_homes.iter().enumerate() {
        assert!(
            check_user_home(&args.user_homes[..index], *home).expect("user home"),
            "duplicate user home"
        );
    }
    check_limits(&args.limits).expect("limits");
    model::validate_catalog(&args.catalog).expect("catalog");
    assert!(args.catalog.effective_at_ms <= at);
    save_catalog(&args.catalog);
    set_config(Config::new(args));
    persist_config();
    publish_certification(at);
    crate::settlement::start();
}

#[ic_cdk::pre_upgrade]
fn pre_upgrade() {
    persist_config();
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    let started = ic_cdk::api::performance_counter(0);
    // Decoding the configuration rejects another stable schema before any call runs.
    config(|_| ());
    load_catalogs();
    publish_certification(now());
    crate::settlement::start();
    ic_cdk::println!(
        "commerce_upgrade subjects={} instructions={}",
        SUBJECTS.with_borrow(|t| t.len()),
        ic_cdk::api::performance_counter(0) - started,
    );
}

/// Replace the admission limits. Accepted subjects, orders and recovery are unaffected.
#[ic_cdk::update]
fn admin_set_limits(limits: CommerceLimits) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    check_limits(&limits)?;
    set_limits(limits);
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_set_limits(limits: CommerceLimits) -> Validation {
    validation(check_limits(&limits).map(|()| {
        format!(
            "Set commerce limits: {} subjects, {} hot and {} total orders, {} orders per day, {} funding/product/transfer calls ({} per caller), {} authorizations and {} PANDA refreshes per minute (currently {:?}).{}",
            limits.max_subjects,
            limits.max_hot_orders,
            limits.max_orders,
            limits.daily_orders,
            limits.calls_per_minute,
            limits.calls_per_caller,
            limits.authorizations_per_minute,
            limits.refreshes_per_minute,
            config(|c| c.limits.clone()),
            admin::unchanged(config(|c| c.limits != limits), "Already set"),
        )
    }))
}

#[ic_cdk::query]
fn get_commerce_limits() -> CommerceLimits {
    config(|c| c.limits.clone())
}

/// Record counts against the limits, with the stable size and cycle balance.
#[ic_cdk::query]
fn commerce_stats() -> CommerceStats {
    crate::checkout_store::stats(
        SUBJECTS.with_borrow(|t| t.len()),
        CERT.with_borrow(|c| c.len()),
    )
}

/// Append a user home. Its `dmsg_user` must name this canister as
/// `commerce_canister`; product registrations list it separately.
#[ic_cdk::update]
fn admin_add_user_home(home: Principal) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    if config(|c| check_user_home(&c.user_homes, home))? {
        add_user_home(home);
    }
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_add_user_home(home: Principal) -> Validation {
    validation(
        config(|c| check_user_home(&c.user_homes, home)).map(|fresh| {
            format!(
                "Add user home {home} as an account beneficiary authority.{}",
                admin::unchanged(fresh, "Already listed"),
            )
        }),
    )
}

fn check_policy(mut c: Catalog, at: u64) -> Result<Catalog> {
    model::validate_catalog(&c)?;
    let old = latest_catalog();
    if c.effective_at_ms == 0 {
        ensure_valid(old.effective_at_ms < at, "pending catalog")?;
        ensure_valid(
            c.terms_digest == old.terms_digest && c.storage_products == old.storage_products,
            "immediate increase preserves terms and storage products",
        )?;
        let mut increased = false;
        for previous in &old.plans {
            let next = model::plan(&c, &previous.plan_id)?;
            ensure_valid(
                next.price_cents == previous.price_cents
                    && next.terms_version == previous.terms_version
                    && next.weights == previous.weights
                    && next.limits.storage_bytes >= previous.limits.storage_bytes
                    && next.limits.active_channels >= previous.limits.active_channels
                    && next.limits.monthly_execution_units
                        >= previous.limits.monthly_execution_units,
                "immediate policy must only increase limits",
            )?;
            increased |= next.limits != previous.limits;
        }
        ensure_valid(increased, "unchanged limits")?;
        c.effective_at_ms = at;
    } else {
        ensure_valid(
            c.effective_at_ms >= at.saturating_add(7 * DAY),
            "catalog notice",
        )?;
    }
    ensure_valid(
        c.effective_at_ms > old.effective_at_ms && c.version > old.version,
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
    Ok(c)
}

/// Announce a catalog at least 7 days ahead. With effective_at_ms = 0,
/// apply a limits-only increase now, preserving prices, terms and weights.
#[ic_cdk::update]
fn schedule_policy(c: Catalog) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    let at = now();
    let c = check_policy(c, at)?;
    save_catalog(&c);
    publish_certification(at);
    Ok(())
}

#[ic_cdk::query]
fn validate_schedule_policy(c: Catalog) -> Validation {
    validation(check_policy(c, now()).map(|c| {
        let plans: Vec<String> = c
            .plans
            .iter()
            .map(|p| format!("{:?} {} cents", p.plan_id, p.price_cents))
            .collect();
        format!(
            "Schedule catalog version {} effective at {} ms: plans [{}], {} storage products, terms digest {}.",
            c.version,
            c.effective_at_ms,
            plans.join(", "),
            c.storage_products.len(),
            hex(c.terms_digest.as_slice()),
        )
    }))
}

#[ic_cdk::update]
fn refresh_catalog() -> Catalog {
    let c = catalog(now());
    certify(catalog_key().to_vec(), &c);
    c
}

#[ic_cdk::query]
fn get_catalog() -> Result<CertifiedBatch> {
    CERT.with_borrow(|c| {
        c.batch(
            ic_cdk::api::canister_self(),
            vec![catalog_key().to_vec()],
            |_| certified_catalog(),
        )
    })
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

/// Pause or resume new cash checkouts; reconciliation, refunds, lease
/// refresh and expiry continue.
#[ic_cdk::update]
fn set_admission_pause(paused: bool) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    set_paused(paused);
    Ok(())
}

#[ic_cdk::query]
fn validate_set_admission_pause(paused: bool) -> Validation {
    let action = if paused { "Pause" } else { "Resume" };
    Ok(format!(
        "{action} new cash checkouts.{}",
        admin::unchanged(config(|c| c.paused != paused), "Already set"),
    ))
}

async fn refresh(
    b: Beneficiary,
    canister_id: Principal,
    caller: Principal,
    mut at: u64,
) -> Result<(EntitlementView, u64)> {
    valid_subject(&b)?;
    let mut s = load(&b)?;
    if let Some(v) = &s.view
        && at.saturating_add(LEASE_RENEW_WINDOW_MS) < v.valid_until_ms
        && v.business_revision == s.business_revision
        && same_catalog(v.issued_at_ms, at)
    {
        return Ok((v.clone(), at));
    }
    let source = s.active(at).and_then(|c| match c.source {
        ContractSource::Sns { claim_id } => Some((c.contract_id, claim_id)),
        _ => None,
    });
    if let Some((contract, id)) = source {
        let _call = crate::calls::CallGuard::entitlement(entitlement_key(&b))?;
        if at < s.retry_after_ms {
            return s
                .view
                .clone()
                .map(|v| (v, at))
                .ok_or(Error::MembershipStale);
        }
        reserve_call(at, CallBudget::Refresh(caller))?;
        let response: Result<Result<dmsg_types::integration_membership::PandaClaimView>> =
            dmsg_runtime::call(
                config(|c| c.membership_canister),
                "refresh_panda_claim",
                (id,),
            )
            .await;
        at = now();
        s = load(&b)?;
        match response {
            Ok(Ok(view))
                if view.claim_id == id
                    && view.terms.offer.beneficiary == b
                    && view.terms.home_membership == config(|c| c.membership_canister) =>
            {
                crate::product::observe(&view, at)?;
            }
            // Membership coalesced or rate-limited the check, or the call did
            // not complete: nothing was learned, so the previous qualification
            // lease stands. It ends within the hour in any case.
            Ok(Err(
                Error::Pending
                | Error::QuotaExceeded
                | Error::Unavailable(_)
                | Error::ExecutionUnknown,
            ))
            | Err(_) => {
                s.retry_after_ms = at.saturating_add(MINUTE);
                save_subject(&s);
                return s
                    .view
                    .clone()
                    .map(|v| (v, at))
                    .ok_or(Error::MembershipStale);
            }
            Ok(_) => {
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
    refresh(
        b,
        ic_cdk::api::canister_self(),
        ic_cdk::api::msg_caller(),
        now(),
    )
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
            |key| {
                let view = SUBJECTS.with_borrow(|t| t.load(key))?.view?;
                Some(dmsg_protocol::canonical(&view))
            },
        )
    })
}

/// A beneficiary's base contracts and storage add-ons for display, with the
/// refund window of its first cash purchase. An account that never bought
/// anything has none; its rights are the certified Free entitlement.
#[ic_cdk::query]
fn get_membership_terms(b: Beneficiary) -> Result<MembershipTerms> {
    valid_subject(&b)?;
    let at = now();
    let s = match load(&b) {
        Ok(s) => s,
        Err(Error::NotFound) => {
            return Ok(MembershipTerms {
                contracts: vec![],
                addons: vec![],
                refund: None,
            });
        }
        Err(e) => return Err(e),
    };
    Ok(MembershipTerms {
        refund: model::refund_window(&s, at),
        contracts: s.contracts,
        addons: s
            .addons
            .into_iter()
            .filter(|a| a.expires_at_ms > at)
            .collect(),
    })
}

#[ic_cdk::update]
async fn get_execution_entitlement(
    b: Beneficiary,
    month: u32,
    account_created_at_ms: u64,
) -> Result<ExecutionEntitlement> {
    let at = now();
    let caller = ic_cdk::api::msg_caller();
    ensure(
        caller == b.authority_canister && account_created_at_ms <= at && month == month_utc(at)?,
        Error::Forbidden,
    )?;
    valid_subject(&b)?;
    let home = ic_cdk::api::canister_self();
    let mut s = match load(&b) {
        Ok(s) => s,
        // An account that never bought anything keeps no commerce state.
        Err(Error::NotFound) => {
            return model::free(
                home,
                b,
                account_created_at_ms,
                &month_catalogs(month, at)?,
                at,
                next_catalog_at(at),
            );
        }
        Err(e) => return Err(e),
    };
    if let Some(created) = s.created_at_ms {
        ensure(created == account_created_at_ms, Error::IntegrityFailed)?;
    } else {
        s.created_at_ms = Some(account_created_at_ms);
        save(&s);
    }
    let (view, at) = refresh(b.clone(), home, caller, at).await?;
    let s = load(&b)?;
    ensure(month_utc(at)? == month, Error::MembershipStale)?;
    let month = model::month(&s, &month_catalogs(month, at)?, month)?;
    Ok(ExecutionEntitlement { view, month })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkout_model::fixture;

    #[test]
    fn catalog_notice_requires_seven_days() {
        let at = fixture::base::NOW;
        let mut next = fixture::init().catalog;
        save_catalog(&next);
        next.version += 1;
        for plan in &mut next.plans {
            plan.catalog_version = next.version;
        }

        next.effective_at_ms = at + 7 * DAY - 1;
        assert_eq!(
            check_policy(next.clone(), at),
            Err(Error::InvalidInput("catalog notice".into()))
        );
        next.effective_at_ms += 1;
        assert_eq!(check_policy(next.clone(), at), Ok(next.clone()));
        next.effective_at_ms = at + 8 * DAY;
        assert_eq!(check_policy(next.clone(), at), Ok(next));
    }

    #[test]
    fn an_immediate_catalog_does_not_reuse_a_view_issued_at_the_same_timestamp() {
        let at = fixture::base::NOW;
        let mut c = fixture::init().catalog;
        save_catalog(&c);
        assert!(same_catalog(at, at));
        c.version += 1;
        c.effective_at_ms = at;
        save_catalog(&c);
        assert!(!same_catalog(at, at));
        assert!(!same_catalog(at - 1, at + 1));
        assert!(same_catalog(at + 1, at + 2));
    }

    #[test]
    fn immediate_policy_only_accepts_increases_without_repricing_or_pending_catalogs() {
        let at = fixture::base::NOW;
        let old = fixture::init().catalog;
        save_catalog(&old);
        let mut next = old.clone();
        next.version += 1;
        for plan in &mut next.plans {
            plan.catalog_version = next.version;
        }
        assert!(check_policy(next.clone(), at).is_err());
        next.plans[1].limits.active_channels = 20;
        next.plans[1].limits.monthly_execution_units = 20;
        let accepted = check_policy(next.clone(), at).unwrap();
        assert_eq!(accepted.effective_at_ms, at);

        let mut invalid = next.clone();
        invalid.plans[2].limits.active_channels -= 1;
        assert!(check_policy(invalid, at).is_err());
        let mut invalid = next.clone();
        invalid.plans[1].price_cents += 1;
        assert!(check_policy(invalid, at).is_err());
        let mut invalid = next.clone();
        invalid.plans[1].terms_version += 1;
        assert!(check_policy(invalid, at).is_err());
        let mut invalid = next.clone();
        invalid.terms_digest = Hash::new([7; 32]);
        assert!(check_policy(invalid, at).is_err());
        let mut invalid = next.clone();
        for plan in &mut invalid.plans {
            plan.weights.version += 1;
        }
        assert!(check_policy(invalid, at).is_err());
        let mut invalid = next.clone();
        invalid.storage_products.push(StorageProduct {
            product_id: Hash::new([1; 32]),
            storage_bytes: 100,
            price_cents: 100,
        });
        assert!(check_policy(invalid, at).is_err());

        let mut scheduled = next.clone();
        scheduled.effective_at_ms = at + 8 * DAY;
        save_catalog(&scheduled);
        next.version += 1;
        for plan in &mut next.plans {
            plan.catalog_version = next.version;
        }
        assert_eq!(
            check_policy(next, at),
            Err(Error::InvalidInput("pending catalog".into()))
        );
    }
}
