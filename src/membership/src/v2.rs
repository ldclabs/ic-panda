//! Shared full-waiver service. Applied commitments have no early-release endpoint.
use crate::{sns, store, v2_model::Claim};
use candid::Principal;
use dmsg_protocol::{commerce_v2::*, integration::*, *};
use dmsg_runtime::{
    call,
    storage::{MapExt, Stored},
};
use dmsg_types::{
    integration::*, integration_billing::*, integration_membership::*, membership::Eligibility, *,
};
use ic_stable_structures::{
    memory_manager::VirtualMemory, DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
type Memory = VirtualMemory<DefaultMemoryImpl>;
#[derive(Clone, Default, Serialize, Deserialize)]
struct Counters {
    hour: u64,
    applications: u64,
}

thread_local! {
    static CONFIG:RefCell<StableCell<Stored<Option<PandaServiceConfig>>,Memory>>=RefCell::new(StableCell::init(store::memory(7),Stored(None)));
    static CLAIMS:RefCell<StableBTreeMap<Vec<u8>,Stored<Claim>,Memory>>=RefCell::new(StableBTreeMap::init(store::memory(8)));
    static BUDGETS:RefCell<StableBTreeMap<Vec<u8>,Stored<PandaSubsidyBudget>,Memory>>=RefCell::new(StableBTreeMap::init(store::memory(9)));
    static POLICIES:RefCell<StableBTreeMap<Vec<u8>,Stored<PandaRatePolicy>,Memory>>=RefCell::new(StableBTreeMap::init(store::memory(10)));
    static NEURONS:RefCell<StableBTreeMap<Vec<u8>,Stored<Vec<Hash>>,Memory>>=RefCell::new(StableBTreeMap::init(store::memory(11)));
    static EXPIRATIONS:RefCell<StableBTreeMap<Vec<u8>,Stored<Hash>,Memory>>=RefCell::new(StableBTreeMap::init(store::memory(12)));
    static COUNTERS:RefCell<StableCell<Stored<Counters>,Memory>>=RefCell::new(StableCell::init(store::memory(13),Stored(Counters::default())));
}

fn config() -> Result<PandaServiceConfig> {
    CONFIG
        .with_borrow(|c| c.get().0.clone())
        .ok_or(Error::Unavailable("PANDA service unconfigured".into()))
}

fn load(id: Hash) -> Result<Claim> {
    CLAIMS
        .with_borrow(|t| t.load(id.as_slice()))
        .ok_or(Error::NotFound)
}

fn key(id: Hash) -> Vec<u8> {
    digest("dmsg/panda/claim-certificate/v2", &id).to_vec()
}

fn neuron(terms: &PandaApplicationTerms) -> Hash {
    digest(
        "dmsg/panda/occupancy/v2",
        &(terms.sns_governance, terms.neuron_id),
    )
}

fn expiry_key(at: u64, id: Hash) -> Vec<u8> {
    [&at.to_be_bytes(), id.as_slice()].concat()
}

fn actor(c: &Claim, caller: Principal) -> Result<()> {
    ensure(
        caller == c.view.terms.actor || caller == c.view.terms.offer.adapter,
        Error::Forbidden,
    )
}

fn budget(id: Hash) -> Result<PandaSubsidyBudget> {
    BUDGETS
        .with_borrow(|t| t.load(id.as_slice()))
        .ok_or(Error::QuotaExceeded)
}

fn save(c: &mut Claim) {
    let old = load(c.view.claim_id).ok();
    c.view.lease_revision = old.as_ref().map_or(c.view.lease_revision, |v| {
        v.view.lease_revision.checked_add(1).expect("view revision")
    });
    if let Some(at) = old.as_ref().and_then(Claim::release_at) {
        EXPIRATIONS.with_borrow_mut(|t| t.delete(&expiry_key(at, c.view.claim_id)));
    }
    if let Some(at) = c.release_at() {
        EXPIRATIONS.with_borrow_mut(|t| t.put(&expiry_key(at, c.view.claim_id), &c.view.claim_id));
    }
    let n = neuron(&c.view.terms);
    let mut refs = NEURONS
        .with_borrow(|t| t.load(n.as_slice()))
        .unwrap_or_default();
    if c.holds() {
        if !refs.contains(&c.view.claim_id) {
            refs.push(c.view.claim_id);
        }
    } else {
        refs.retain(|id| *id != c.view.claim_id);
    }
    NEURONS.with_borrow_mut(|t| {
        if refs.is_empty() {
            t.delete(n.as_slice());
        } else {
            t.put(n.as_slice(), &refs);
        }
    });
    CLAIMS.with_borrow_mut(|t| t.put(c.view.claim_id.as_slice(), c));
    store::CERT.with_borrow_mut(|t| t.put(key(c.view.claim_id), &c.view));
}

fn release_budget(c: &mut Claim) -> Result<()> {
    if c.budget_reserved && !c.holds() {
        let mut b = budget(c.view.terms.quote.policy.subsidy_budget_id)?;
        b.reserved_usd_micros = b
            .reserved_usd_micros
            .checked_sub(c.view.terms.quote.subsidy_usd_micros)
            .ok_or(Error::IntegrityFailed)?;
        c.budget_reserved = false;
        BUDGETS.with_borrow_mut(|t| t.put(b.budget_id.as_slice(), &b));
    }
    Ok(())
}

#[ic_cdk::update]
fn configure_panda_service(next: PandaServiceConfig) -> Result<()> {
    ensure(
        ic_cdk::api::msg_caller() == store::config().init.governance,
        Error::Forbidden,
    )?;
    authenticated(next.commerce_canister)?;
    ensure_valid(
        next.max_claims > 0
            && next.hourly_applications > 0
            && next.hourly_applications <= 10_000
            && next.cooling_ms >= PANDA_COOLING_MS
            && next.cooling_ms < APPLICATION_TTL_MS,
        "PANDA limits",
    )?;
    if let Ok(old) = config() {
        ensure(
            old.commerce_canister == next.commerce_canister
                && next.max_claims >= CLAIMS.with_borrow(|t| t.len()),
            Error::IntegrityFailed,
        )?;
    }
    CONFIG.with_borrow_mut(|c| c.set(Stored(Some(next))));
    Ok(())
}

#[ic_cdk::update]
fn set_panda_subsidy_budget(id: Hash, total_usd_micros: u128) -> Result<PandaSubsidyBudget> {
    ensure(
        ic_cdk::api::msg_caller() == store::config().init.governance,
        Error::Forbidden,
    )?;
    nonzero(id.as_slice())?;
    let old = budget(id).ok();
    ensure(
        old.as_ref()
            .is_none_or(|b| total_usd_micros >= b.total_usd_micros),
        Error::Forbidden,
    )?;
    ensure(
        old.is_some() || BUDGETS.with_borrow(|t| t.len()) < 16,
        Error::QuotaExceeded,
    )?;
    let b = PandaSubsidyBudget {
        budget_id: id,
        total_usd_micros,
        reserved_usd_micros: old.map_or(0, |b| b.reserved_usd_micros),
    };
    BUDGETS.with_borrow_mut(|t| t.put(id.as_slice(), &b));
    Ok(b)
}

#[ic_cdk::update]
fn schedule_panda_rate(policy: PandaRatePolicy) -> Result<PandaRatePolicy> {
    ensure(
        ic_cdk::api::msg_caller() == store::config().init.governance,
        Error::Forbidden,
    )?;
    let at = nanos_to_millis(ic_cdk::api::time());
    let mut policy = policy;
    // Local fixtures may seed a previously announced policy; production publication is consensus time.
    if store::config().init.environment != Environment::Local {
        policy.published_at_ms = at;
    }
    validate_rate_policy(&policy)?;
    ensure(
        policy.environment == store::config().init.environment && policy.published_at_ms <= at,
        Error::Forbidden,
    )?;
    budget(policy.subsidy_budget_id)?;
    if let Some(old) = POLICIES.with_borrow(|t| t.load(&policy.policy_version.to_be_bytes())) {
        ensure(old == policy, Error::IdempotencyConflict)?;
        return Ok(old);
    }
    ensure(
        POLICIES.with_borrow(|t| {
            t.iter().all(|v| {
                let p = v.value().0;
                p.effective_at_ms != policy.effective_at_ms
                    || !p
                        .product_ids
                        .iter()
                        .any(|id| policy.product_ids.contains(id))
            })
        }),
        Error::VersionConflict,
    )?;
    POLICIES.with_borrow_mut(|t| t.put(&policy.policy_version.to_be_bytes(), &policy));
    Ok(policy)
}

fn policy(product: &str, at: u64) -> Result<PandaRatePolicy> {
    POLICIES
        .with_borrow(|t| {
            t.iter()
                .map(|v| v.value().0)
                .filter(|p| p.effective_at_ms <= at && p.product_ids.iter().any(|id| id == product))
                .max_by_key(|p| p.effective_at_ms)
        })
        .ok_or(Error::NotFound)
}

async fn registration(offer: &BillingOffer) -> Result<(AppRegistration, ProductRegistration)> {
    let result: Result<(AppRegistration, Option<ProductRegistration>)> = call(
        config()?.commerce_canister,
        "read_integration_configuration",
        (offer.app_id.clone(), Some(offer.product_id.clone())),
    )
    .await?;
    let (a, p) = result?;
    Ok((a, p.ok_or(Error::NotFound)?))
}

fn admission() -> Result<()> {
    let c = store::config();
    ensure(!c.paused && c.sns_verified, Error::Locked)
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
    admission()?;
    let at = nanos_to_millis(ic_cdk::api::time());
    store::reserve_call(at, store::CallBudget::Authorization(caller))?;
    let (app, product) = registration(&offer).await?;
    let valid: Result<()> = call(
        product.quote_authority,
        "verify_billing_offer",
        (offer.clone(),),
    )
    .await?;
    valid?;
    let at = nanos_to_millis(ic_cdk::api::time());
    admission()?;
    let rate = policy(&offer.product_id, at)?;
    let quote = quote_panda(&offer, &app, &product, &rate, at)?;
    let terms = PandaApplicationTerms {
        home_membership: home,
        user_home,
        approving_account,
        actor: caller,
        sns_governance: store::config().init.governance,
        neuron_id,
        offer,
        quote,
    };
    validate_panda_terms(
        &terms,
        &app,
        &product,
        home,
        store::config().init.governance,
        at,
        true,
    )?;
    Ok(terms)
}

async fn authorize(request: &PandaClaimRequest, initial: bool) -> Result<()> {
    let home = ic_cdk::api::canister_self();
    let t = &request.terms;
    let a = &request.authorization;
    let (app, product) = registration(&t.offer).await?;
    let at = nanos_to_millis(ic_cdk::api::time());
    admission()?;
    validate_panda_terms(
        t,
        &app,
        &product,
        home,
        store::config().init.governance,
        at,
        initial,
    )?;
    if initial {
        ensure(
            policy(&t.offer.product_id, at)? == t.quote.policy,
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
    let product_auth: Result<ProductAuthorization> = call(
        product.beneficiary_authority,
        "authorize_product_billing",
        (a.clone(),),
    )
    .await?;
    let product_auth = product_auth?;
    let approved: Result<ApplicationAuthorization> = call(
        t.user_home,
        "verify_application_authorization",
        (a.approval_id, a.account_approval.clone()),
    )
    .await?;
    let approved = approved?;
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
            at < t.offer.accept_by_ms && policy(&t.offer.product_id, at)? == t.quote.policy,
            Error::PolicyStale,
        )?;
    }
    Ok(())
}

fn check_occupancy(terms: &PandaApplicationTerms, at: u64) -> Result<()> {
    let stale = NEURONS
        .with_borrow(|t| t.load(neuron(terms).as_slice()))
        .unwrap_or_default();
    for id in stale {
        let mut c = load(id)?;
        if c.release_at().is_some_and(|end| at >= end) {
            c.expire(at)?;
            release_budget(&mut c)?;
            save(&mut c);
        }
    }
    let refs = NEURONS
        .with_borrow(|t| t.load(neuron(terms).as_slice()))
        .unwrap_or_default();
    ensure(refs.len() < 2, Error::NeuronOccupied)?;
    for id in refs {
        let old = load(id)?;
        ensure(
            old.view.committed_until_ms > 0
                && old.view.terms.offer.beneficiary == terms.offer.beneficiary
                && old.view.terms.offer.expires_at_ms == terms.offer.starts_at_ms,
            Error::NeuronOccupied,
        )?;
    }
    Ok(())
}

#[ic_cdk::update]
async fn request_panda_claim(request: PandaClaimRequest) -> Result<PandaClaimView> {
    let caller = ic_cdk::api::msg_caller();
    ensure(caller == request.terms.actor, Error::Forbidden)?;
    let id = panda_claim_id(&request.terms);
    if let Ok(c) = load(id) {
        ensure(c.view.terms == request.terms, Error::IdempotencyConflict)?;
        return Ok(c.view);
    }
    let at = nanos_to_millis(ic_cdk::api::time());
    store::reserve_call(at, store::CallBudget::Authorization(caller))?;
    ensure(
        canonical(&request).len() <= MAX_PAYLOAD,
        Error::QuotaExceeded,
    )?;
    authorize(&request, true).await?;
    let at = nanos_to_millis(ic_cdk::api::time());
    admission()?;
    if let Ok(c) = load(id) {
        ensure(c.view.terms == request.terms, Error::IdempotencyConflict)?;
        return Ok(c.view);
    }
    sweep(at)?;
    check_occupancy(&request.terms, at)?;
    let cfg = config()?;
    ensure(
        CLAIMS.with_borrow(|t| t.len()) < cfg.max_claims,
        Error::QuotaExceeded,
    )?;
    let mut count = COUNTERS.with_borrow(|c| c.get().0.clone());
    if count.hour != at / (60 * MINUTE) {
        count.hour = at / (60 * MINUTE);
        count.applications = 0;
    }
    ensure(
        count.applications < cfg.hourly_applications,
        Error::QuotaExceeded,
    )?;
    let mut b = budget(request.terms.quote.policy.subsidy_budget_id)?;
    b.reserved_usd_micros = b
        .reserved_usd_micros
        .checked_add(request.terms.quote.subsidy_usd_micros)
        .ok_or(Error::QuotaExceeded)?;
    ensure(
        b.reserved_usd_micros <= b.total_usd_micros,
        Error::QuotaExceeded,
    )?;
    let mut claim = Claim::new(request, cfg.cooling_ms);
    count.applications += 1;
    BUDGETS.with_borrow_mut(|t| t.put(b.budget_id.as_slice(), &b));
    COUNTERS.with_borrow_mut(|c| c.set(Stored(count)));
    save(&mut claim);
    reserve_product(id).await?;
    qualify(id).await?;
    Ok(load(id)?.view)
}

async fn reserve_product(id: Hash) -> Result<()> {
    let old = load(id)?;
    if old.product_reserved || !old.holds() {
        return Ok(());
    }
    let answer: Result<Result<()>> = call(
        old.view.terms.offer.adapter,
        "reserve_product_billing",
        (
            old.authorization.clone(),
            old.view.terms.quote.application_deadline_ms,
        ),
    )
    .await;
    let mut current = load(id)?;
    if !current.holds() {
        return Ok(());
    }
    match answer {
        Ok(Ok(())) => current.product_reserved = true,
        Ok(Err(_)) => {
            current.view.status = PandaClaimStatus::Rejected;
            release_budget(&mut current)?;
        }
        Err(_) => return Err(Error::ExecutionUnknown),
    }
    save(&mut current);
    Ok(())
}

async fn qualify(id: Hash) -> Result<()> {
    let at = nanos_to_millis(ic_cdk::api::time());
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
        release_budget(&mut c)?;
        save(&mut c);
        return Ok(());
    }
    ensure(at >= c.busy_until_ms, Error::Pending)?;
    store::reserve_call(at, store::CallBudget::Qualification)?;
    c.generation = c.generation.checked_add(1).ok_or(Error::QuotaExceeded)?;
    c.busy_until_ms = at + MINUTE;
    let generation = c.generation;
    save(&mut c);
    let verification_pin = store::config().init;
    let verified = crate::api::fresh_sns(at).await;
    let observed = nanos_to_millis(ic_cdk::api::time());
    ensure(load(id)?.generation == generation, Error::VersionConflict)?;
    let result: Result<sns::GetNeuronResponse> = if verified {
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
    } else {
        Err(Error::MembershipStale)
    };
    let now = nanos_to_millis(ic_cdk::api::time());
    let mut current = load(id)?;
    ensure(current.generation == generation, Error::VersionConflict)?;
    current.busy_until_ms = 0;
    let eligibility = match result {
        Ok(sns::GetNeuronResponse {
            result: Some(sns::NeuronResult::Neuron(n)),
        }) if store::config().init == verification_pin => sns::assess(
            &n,
            current.view.terms.neuron_id,
            current.view.terms.actor,
            current.view.terms.quote.required_stake_e8s,
            current.view.terms.offer.expires_at_ms,
            observed,
        ),
        _ => Eligibility::Unverifiable,
    };
    if current.release_at().is_some_and(|end| now >= end) {
        current.expire(now)?;
    } else {
        current.observe(eligibility, observed, now)?;
    }
    release_budget(&mut current)?;
    save(&mut current);
    if !current.holds() {
        release_product(id).await?;
    }
    Ok(())
}

#[ic_cdk::update]
async fn advance_panda_claim(
    id: Hash,
    authorization: ProductAuthorizationRequest,
) -> Result<PandaClaimView> {
    let caller = ic_cdk::api::msg_caller();
    let c = load(id)?;
    ensure(caller == c.view.terms.actor, Error::Forbidden)?;
    if c.view.status == PandaClaimStatus::Applying {
        deliver(id, true).await?;
        return Ok(load(id)?.view);
    }
    if matches!(
        c.view.status,
        PandaClaimStatus::Active | PandaClaimStatus::Terminated | PandaClaimStatus::Released
    ) {
        return Ok(c.view);
    }
    let at = nanos_to_millis(ic_cdk::api::time());
    admission()?;
    ensure(
        c.view.cooling_until_ms.is_some_and(|ready| at >= ready)
            && at < c.view.terms.quote.application_deadline_ms,
        Error::Pending,
    )?;
    reserve_product(id).await?;
    qualify(id).await?;
    let checked = load(id)?;
    ensure(
        checked.view.status == PandaClaimStatus::CoolingDown,
        Error::MembershipIneligible,
    )?;
    let request = PandaClaimRequest {
        terms: checked.view.terms.clone(),
        authorization,
    };
    authorize(&request, false).await?;
    let at = nanos_to_millis(ic_cdk::api::time());
    let mut current = load(id)?;
    admission()?;
    ensure(
        current.generation == checked.generation,
        Error::VersionConflict,
    )?;
    current.preparing_apply(at)?;
    current.authorization = request.authorization;
    save(&mut current);
    deliver(id, false).await?;
    Ok(load(id)?.view)
}

async fn deliver(id: Hash, reconcile: bool) -> Result<()> {
    let c = load(id)?;
    if c.view.receipt.is_some() {
        return Ok(());
    }
    let decision = c.decision.clone().ok_or(Error::NotFound)?;
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
    current.accept(receipt)?;
    release_budget(&mut current)?;
    save(&mut current);
    Ok(())
}

async fn release_product(id: Hash) -> Result<()> {
    let c = load(id)?;
    if c.reservation_released || c.view.committed_until_ms > 0 || c.decision.is_some() {
        return Ok(());
    }
    ensure(!c.holds(), Error::Forbidden)?;
    let result: Result<()> = call(
        c.view.terms.offer.adapter,
        "release_product_billing",
        (c.authorization.clone(),),
    )
    .await?;
    result?;
    let mut current = load(id)?;
    current.reservation_released = true;
    save(&mut current);
    Ok(())
}

#[ic_cdk::update]
async fn cancel_panda_application(id: Hash) -> Result<PandaClaimView> {
    let mut c = load(id)?;
    actor(&c, ic_cdk::api::msg_caller())?;
    c.cancel()?;
    release_budget(&mut c)?;
    save(&mut c);
    release_product(id).await?;
    Ok(load(id)?.view)
}

#[ic_cdk::update]
async fn reconcile_panda_claim(id: Hash) -> Result<PandaClaimView> {
    let c = load(id)?;
    actor(&c, ic_cdk::api::msg_caller())?;
    if c.view.status == PandaClaimStatus::Applying {
        deliver(id, true).await?;
    } else if !c.holds() {
        release_product(id).await?;
    } else if matches!(
        c.view.status,
        PandaClaimStatus::Checking | PandaClaimStatus::CoolingDown
    ) {
        reserve_product(id).await?;
        qualify(id).await?;
    }
    Ok(load(id)?.view)
}

#[ic_cdk::update]
async fn refresh_panda_claim(id: Hash) -> Result<PandaClaimView> {
    let c = load(id)?;
    actor(&c, ic_cdk::api::msg_caller())?;
    let at = nanos_to_millis(ic_cdk::api::time());
    if c.view.status == PandaClaimStatus::Applying {
        deliver(id, true).await?;
    } else if c.release_at().is_some_and(|end| at >= end) {
        let mut c = c;
        c.expire(at)?;
        release_budget(&mut c)?;
        save(&mut c);
    } else if c.view.status == PandaClaimStatus::Active
        && (c.view.eligibility != Eligibility::Eligible
            || c.view.valid_until_ms <= at.saturating_add(MINUTE))
    {
        qualify(id).await?;
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
    store::CERT.with_borrow(|t| t.batch(ic_cdk::api::canister_self(), vec![key(id)]))
}

#[ic_cdk::query]
fn panda_budgets() -> Vec<PandaSubsidyBudget> {
    BUDGETS.with_borrow(|t| t.iter().map(|v| v.value().0).collect())
}

fn sweep(at: u64) -> Result<u32> {
    let ids: Vec<Hash> = EXPIRATIONS.with_borrow(|t| {
        t.range(..=expiry_key(at, Hash::new([255; 32])))
            .take(32)
            .map(|v| v.value().0)
            .collect()
    });
    let mut released = 0;
    for id in ids {
        let mut c = load(id)?;
        c.expire(at)?;
        release_budget(&mut c)?;
        save(&mut c);
        released += 1;
    }
    Ok(released)
}

#[ic_cdk::update]
fn sweep_panda_commitments() -> Result<u32> {
    sweep(nanos_to_millis(ic_cdk::api::time()))
}

pub(crate) fn rebuild(cert: &mut dmsg_runtime::Certification) {
    CLAIMS.with_borrow(|t| {
        t.for_each(|_, c| {
            cert.insert(key(c.view.claim_id), canonical(&c.view));
        })
    });
}

#[ic_cdk::query]
fn panda_operations(after: Option<Hash>, take: u16) -> Result<PandaOperationsPage> {
    use std::ops::Bound::{Excluded, Unbounded};
    let caller = ic_cdk::api::msg_caller();
    authenticated(caller)?;
    ensure_valid((1..=32).contains(&take), "page size")?;
    let mut claims = vec![];
    let mut next = None;
    CLAIMS.with_borrow(|t| {
        let start = after.map_or(Unbounded, |id| Excluded(id.to_vec()));
        let mut iter = t.range((start, Unbounded));
        for _ in 0..512 {
            let Some(row) = iter.next() else {
                next = None;
                break;
            };
            let c = row.value().0;
            next = Some(c.view.claim_id);
            if caller == store::config().init.governance || actor(&c, caller).is_ok() {
                claims.push(c.view);
            }
            if claims.len() >= usize::from(take) {
                break;
            }
        }
    });
    Ok(PandaOperationsPage { claims, next })
}
