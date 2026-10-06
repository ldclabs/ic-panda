//! Shared full-waiver application authorization, qualification and product delivery.
use crate::{
    claim::Claim,
    rate, sns, store,
    store::{actor, check_occupancy, key, live_claims, load, save, sweep},
};
use candid::Principal;
use dmsg_protocol::{commerce_v2::*, integration::*, *};
use dmsg_runtime::call;
use dmsg_types::{
    integration::*, integration_billing::*, integration_membership::*, membership::Eligibility, *,
};

const OBSERVATION_REUSE_MS: u64 = MINUTE;

async fn registration(
    commerce: Principal,
    offer: &BillingOffer,
) -> Result<(AppRegistration, ProductRegistration)> {
    let result: Result<(AppRegistration, Option<ProductRegistration>)> = call(
        commerce,
        "read_integration_configuration",
        (offer.app_id.clone(), Some(offer.product_id.clone())),
    )
    .await?;
    let (a, p) = result?;
    Ok((a, p.ok_or(Error::NotFound)?))
}

/// New admissions require an unpaused service with a verified SNS configuration.
fn admission() -> Result<store::Config> {
    let c = store::config();
    ensure(!c.paused && c.sns_verified, Error::Locked)?;
    Ok(c)
}

#[ic_cdk::update]
async fn quote_panda_subscription(
    offer: BillingOffer,
    user_home: Principal,
    approving_account: AccountId,
    neuron_id: Hash,
) -> Result<PandaApplicationTerms> {
    let caller = ic_cdk::api::msg_caller();
    let home = ic_cdk::api::canister_self();
    authenticated(caller)?;
    nonzero(approving_account.as_slice())?;
    nonzero(neuron_id.as_slice())?;
    let commerce = admission()?.service()?.commerce_canister;
    let at = nanos_to_millis(ic_cdk::api::time());
    store::reserve_call(at, store::CallBudget::Authorization(caller))?;
    let (app, product) = registration(commerce, &offer).await?;
    let valid: Result<()> = call(
        product.quote_authority,
        "verify_billing_offer",
        (offer.clone(),),
    )
    .await?;
    valid?;
    let at = nanos_to_millis(ic_cdk::api::time());
    let c = admission()?;
    ensure(app.user_homes.contains(&user_home), Error::Forbidden)?;
    // quote_panda validates the offer; the remaining terms are constructed from trusted values.
    let quote = quote_panda(
        &offer,
        &app,
        &product,
        &rate::current(&offer.product_id, at)?,
        at,
    )?;
    Ok(PandaApplicationTerms {
        home_membership: home,
        user_home,
        approving_account,
        actor: caller,
        sns_governance: c.init.governance,
        neuron_id,
        offer,
        quote,
    })
}

/// Check both approvals of the exact terms; returns the time read after the last await.
async fn authorize(request: &PandaClaimRequest, home: Principal, initial: bool) -> Result<u64> {
    let t = &request.terms;
    let a = &request.authorization;
    let commerce = store::config().service()?.commerce_canister;
    let (app, product) = registration(commerce, &t.offer).await?;
    let at = nanos_to_millis(ic_cdk::api::time());
    let c = admission()?;
    validate_panda_terms(t, &app, &product, home, c.init.governance, at, initial)?;
    if initial {
        ensure(
            rate::current(&t.offer.product_id, at)? == t.quote.policy,
            Error::PolicyStale,
        )?;
    }
    ensure(
        a.offer == t.offer
            && a.user_home == t.user_home
            && a.account_approval.approving_account == t.approving_account
            && a.account_approval.actor == t.actor
            && a.account_approval.action_digest == panda_application_hash(t),
        Error::Forbidden,
    )?;
    validate_product_request(a, home, SettlementMethod::Panda, at)?;
    validate_application_approval(&a.account_approval, &app, &product, home, at)?;
    // Product and account authorities are independent; ask both at once.
    let (product_auth, approved): (
        Result<Result<ProductAuthorization>>,
        Result<Result<ApplicationAuthorization>>,
    ) = futures::future::join(
        call(
            product.beneficiary_authority,
            "authorize_product_billing",
            (a.clone(),),
        ),
        call(
            t.user_home,
            "verify_application_authorization",
            (a.approval_id, a.account_approval.clone()),
        ),
    )
    .await;
    let product_auth = product_auth??;
    let approved = approved??;
    let at = nanos_to_millis(ic_cdk::api::time());
    admission()?;
    check_product_authorization(a, &product_auth, at)?;
    ensure(
        approved.approval_id == a.approval_id
            && approved.approval_hash == application_approval_hash(&a.account_approval)
            && approved.verified_at_ms <= at
            && at - approved.verified_at_ms <= MINUTE
            && at < approved.valid_until_ms
            && at < t.quote.application_deadline_ms,
        Error::PolicyStale,
    )?;
    if initial {
        ensure(
            at < t.offer.accept_by_ms && rate::current(&t.offer.product_id, at)? == t.quote.policy,
            Error::PolicyStale,
        )?;
    }
    Ok(at)
}

#[ic_cdk::update]
async fn request_panda_claim(request: PandaClaimRequest) -> Result<PandaClaimView> {
    let caller = ic_cdk::api::msg_caller();
    let home = ic_cdk::api::canister_self();
    ensure(caller == request.terms.actor, Error::Forbidden)?;
    let id = panda_claim_id(&request.terms);
    if let Some(c) = store::replay(id, &request.terms)? {
        return Ok(c.view);
    }
    let at = nanos_to_millis(ic_cdk::api::time());
    store::reserve_call(at, store::CallBudget::Authorization(caller))?;
    ensure(
        canonical(&request).len() <= MAX_PAYLOAD,
        Error::QuotaExceeded,
    )?;
    let at = authorize(&request, home, true).await?;
    if let Some(c) = store::replay(id, &request.terms)? {
        return Ok(c.view);
    }
    sweep(at)?;
    check_occupancy(&request.terms, at)?;
    store::check_capacity()?;
    let mut c = store::config();
    let service = c.service()?.clone();
    ensure(live_claims() < service.max_claims, Error::QuotaExceeded)?;
    let hour = at / (60 * MINUTE);
    if c.application_hour != hour {
        c.application_hour = hour;
        c.applications = 0;
    }
    ensure(
        c.applications < service.hourly_applications,
        Error::QuotaExceeded,
    )?;
    c.applications += 1;
    store::save_config(&c);
    save(&mut Claim::new(id, request, service.cooling_ms), at);
    let at = reserve_product(id, at).await?;
    qualify(id, at, false).await?;
    Ok(load(id)?.view)
}

/// Reserve the product interval once; returns the time after any product call.
async fn reserve_product(id: Hash, at: u64) -> Result<u64> {
    let old = load(id)?;
    if old.product_reserved || !old.holds() {
        return Ok(at);
    }
    let _guard = store::product_call(id, old.view.terms.actor, at)?;
    let answer: Result<Result<()>> = call(
        old.view.terms.offer.adapter,
        "reserve_product_billing",
        (
            old.authorization.clone(),
            old.view.terms.quote.application_deadline_ms,
        ),
    )
    .await;
    let at = nanos_to_millis(ic_cdk::api::time());
    let mut current = load(id)?;
    if current.holds() && !current.product_reserved {
        match answer {
            Ok(Ok(())) => current.product_reserved = true,
            Ok(Err(
                e @ (Error::Unavailable(_)
                | Error::ExecutionUnknown
                | Error::Pending
                | Error::QuotaExceeded),
            )) => return Err(e),
            Ok(Err(_)) => current.reject(),
            Err(_) => return Err(Error::ExecutionUnknown),
        }
        save(&mut current, at);
    }
    Ok(at)
}

/// Observe the neuron at `at`, reusing an observation younger than one minute.
async fn qualify(id: Hash, at: u64, activating: bool) -> Result<()> {
    let mut c = load(id)?;
    if !c.holds()
        || matches!(
            c.view.status,
            PandaClaimStatus::Applying | PandaClaimStatus::Terminated
        )
    {
        return Ok(());
    }
    if c.release_at().is_some_and(|end| at >= end) {
        c.expire(at)?;
        save(&mut c, at);
        return Ok(());
    }
    let after_cooling = !activating
        || c.view
            .cooling_until_ms
            .is_some_and(|ready| c.view.observed_at_ms >= ready);
    if after_cooling && at < c.view.observed_at_ms.saturating_add(OBSERVATION_REUSE_MS) {
        return Ok(());
    }
    ensure(at >= c.busy_until_ms, Error::Pending)?;
    store::reserve_call(at, store::CallBudget::Qualification)?;
    c.generation = c.generation.checked_add(1).ok_or(Error::QuotaExceeded)?;
    c.busy_until_ms = at + MINUTE;
    let generation = c.generation;
    save(&mut c, at);
    let verification_pin = store::config().init;
    let verified = crate::api::fresh_sns(at).await;
    ensure(load(id)?.generation == generation, Error::VersionConflict)?;
    if let Err(e @ (Error::Pending | Error::QuotaExceeded)) = verified {
        // A coalesced/rate-limited check observed no SNS failure. Keep the previous lease.
        let mut current = load(id)?;
        current.busy_until_ms = 0;
        save(&mut current, at);
        return Err(e);
    }
    let result: Result<sns::GetNeuronResponse> = match verified {
        Ok(()) => {
            call(
                c.view.terms.sns_governance,
                "get_neuron",
                (sns::GetNeuronRequest {
                    neuron_id: Some(sns::NeuronId {
                        id: c.view.terms.neuron_id.to_vec(),
                    }),
                },),
            )
            .await
        }
        Err(e) => Err(e),
    };
    let now = nanos_to_millis(ic_cdk::api::time());
    let mut current = load(id)?;
    ensure(current.generation == generation, Error::VersionConflict)?;
    current.busy_until_ms = 0;
    // The observation starts at `at`, never at the delayed callback time.
    let config = store::config();
    let eligibility = match result {
        Ok(sns::GetNeuronResponse {
            result: Some(sns::NeuronResult::Neuron(n)),
        }) if config.init == verification_pin && config.sns_fresh(now) => sns::assess(
            &n,
            current.view.terms.neuron_id,
            current.view.terms.actor,
            current.view.terms.quote.required_stake_e8s,
            current.view.terms.offer.expires_at_ms,
            at,
        ),
        _ => Eligibility::Unverifiable,
    };
    if current.release_at().is_some_and(|end| now >= end) {
        current.expire(now)?;
    } else {
        current.observe(eligibility, at, now)?;
    }
    save(&mut current, now);
    if !current.holds() {
        release_product(id, now).await?;
    }
    Ok(())
}

#[ic_cdk::update]
async fn advance_panda_claim(
    id: Hash,
    authorization: ProductAuthorizationRequest,
) -> Result<PandaClaimView> {
    let caller = ic_cdk::api::msg_caller();
    let home = ic_cdk::api::canister_self();
    let at = nanos_to_millis(ic_cdk::api::time());
    let c = load(id)?;
    ensure(caller == c.view.terms.actor, Error::Forbidden)?;
    if c.view.status == PandaClaimStatus::Applying {
        deliver(id, true, at).await?;
        return Ok(load(id)?.view);
    }
    if matches!(
        c.view.status,
        PandaClaimStatus::Active | PandaClaimStatus::Terminated | PandaClaimStatus::Released
    ) {
        return Ok(c.view);
    }
    ensure(c.holds(), Error::MembershipIneligible)?;
    admission()?;
    ensure(
        c.view.cooling_until_ms.is_some_and(|ready| at >= ready)
            && at < c.view.terms.quote.application_deadline_ms,
        Error::Pending,
    )?;
    store::reserve_call(at, store::CallBudget::Authorization(caller))?;
    let at = reserve_product(id, at).await?;
    qualify(id, at, true).await?;
    let checked = load(id)?;
    ensure(
        checked.view.status == PandaClaimStatus::CoolingDown,
        Error::MembershipIneligible,
    )?;
    let request = PandaClaimRequest {
        terms: checked.view.terms.clone(),
        authorization,
    };
    let at = authorize(&request, home, false).await?;
    let mut current = load(id)?;
    ensure(
        current.generation == checked.generation,
        Error::VersionConflict,
    )?;
    current.preparing_apply(at)?;
    current.authorization = request.authorization;
    save(&mut current, at);
    deliver(id, false, at).await?;
    Ok(load(id)?.view)
}

async fn deliver(id: Hash, reconcile: bool, at: u64) -> Result<()> {
    let c = load(id)?;
    if c.view.receipt.is_some() {
        return Ok(());
    }
    let decision = c.decision.clone().ok_or(Error::NotFound)?;
    let _guard = store::product_call(id, c.view.terms.actor, at)?;
    let receipt = if reconcile {
        let r: Result<Option<ProductReceipt>> = call(
            c.view.terms.offer.adapter,
            "get_product_decision",
            (decision.decision_id,),
        )
        .await?;
        r?
    } else {
        None
    };
    let receipt = if let Some(r) = receipt {
        r
    } else {
        if reconcile {
            let at = nanos_to_millis(ic_cdk::api::time());
            store::reserve_call(at, store::CallBudget::Product(c.view.terms.actor))?;
        }
        let r: Result<ProductReceipt> = call(
            c.view.terms.offer.adapter,
            "apply_product_decision",
            (decision.clone(),),
        )
        .await?;
        r?
    };
    let mut current = load(id)?;
    if current.view.receipt.is_some() {
        return Ok(());
    }
    ensure(
        current.decision.as_ref() == Some(&decision),
        Error::IdempotencyConflict,
    )?;
    let at = nanos_to_millis(ic_cdk::api::time());
    current.accept(receipt)?;
    save(&mut current, at);
    Ok(())
}

async fn release_product(id: Hash, at: u64) -> Result<()> {
    let c = load(id)?;
    if c.reservation_released || c.view.committed_until_ms > 0 || c.decision.is_some() {
        return Ok(());
    }
    ensure(!c.holds(), Error::Forbidden)?;
    let _guard = store::product_call(id, c.view.terms.actor, at)?;
    let result: Result<()> = call(
        c.view.terms.offer.adapter,
        "release_product_billing",
        (c.authorization.clone(),),
    )
    .await?;
    result?;
    let mut current = load(id)?;
    let at = nanos_to_millis(ic_cdk::api::time());
    current.reservation_released = true;
    save(&mut current, at);
    Ok(())
}

#[ic_cdk::update]
async fn cancel_panda_application(id: Hash) -> Result<PandaClaimView> {
    let at = nanos_to_millis(ic_cdk::api::time());
    let mut c = load(id)?;
    actor(&c, ic_cdk::api::msg_caller())?;
    c.cancel()?;
    save(&mut c, at);
    release_product(id, at).await?;
    Ok(load(id)?.view)
}

#[ic_cdk::update]
async fn reconcile_panda_claim(id: Hash) -> Result<PandaClaimView> {
    let at = nanos_to_millis(ic_cdk::api::time());
    let c = load(id)?;
    actor(&c, ic_cdk::api::msg_caller())?;
    if c.view.status == PandaClaimStatus::Applying {
        deliver(id, true, at).await?;
    } else if !c.holds() {
        release_product(id, at).await?;
    } else if matches!(
        c.view.status,
        PandaClaimStatus::Checking | PandaClaimStatus::CoolingDown
    ) {
        let at = reserve_product(id, at).await?;
        qualify(id, at, false).await?;
    }
    Ok(load(id)?.view)
}

#[ic_cdk::update]
async fn refresh_panda_claim(id: Hash) -> Result<PandaClaimView> {
    let c = load(id)?;
    actor(&c, ic_cdk::api::msg_caller())?;
    let at = nanos_to_millis(ic_cdk::api::time());
    if c.view.status == PandaClaimStatus::Applying {
        deliver(id, true, at).await?;
    } else if c.release_at().is_some_and(|end| at >= end) {
        let mut c = c;
        c.expire(at)?;
        save(&mut c, at);
    } else if c.view.status == PandaClaimStatus::Active
        && (c.view.eligibility != Eligibility::Eligible
            || c.view.valid_until_ms <= at.saturating_add(MINUTE))
    {
        qualify(id, at, false).await?;
    }
    Ok(load(id)?.view)
}

#[ic_cdk::update]
fn get_panda_claim_for_product(id: Hash) -> Result<PandaClaimView> {
    let c = load(id)?;
    ensure(
        ic_cdk::api::msg_caller() == c.view.terms.offer.adapter,
        Error::Forbidden,
    )?;
    Ok(c.view)
}

#[ic_cdk::query]
fn get_panda_claim(id: Hash) -> Result<PandaClaimView> {
    let c = load(id)?;
    actor(&c, ic_cdk::api::msg_caller())?;
    Ok(c.view)
}

#[ic_cdk::query]
fn panda_claim_certificate(id: Hash) -> Result<CertifiedBatch> {
    let c = load(id)?;
    actor(&c, ic_cdk::api::msg_caller())?;
    store::CERT.with_borrow(|t| {
        t.batch(ic_cdk::api::canister_self(), vec![key(id)], |_| {
            Some(canonical(&c.view))
        })
    })
}
