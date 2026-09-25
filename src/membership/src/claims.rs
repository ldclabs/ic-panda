//! Shared full-waiver service. Applied commitments have no early-release endpoint.
use crate::{claim::Claim, sns, store};
use candid::Principal;
use dmsg_protocol::{commerce_v2::*, integration::*, *};
use dmsg_runtime::{
    call,
    storage::{MapExt, Stored},
    Certification,
};
use dmsg_types::{
    integration::*, integration_billing::*, integration_membership::*, membership::Eligibility, *,
};
use ic_stable_structures::{memory_manager::VirtualMemory, DefaultMemoryImpl, StableBTreeMap};
use std::cell::RefCell;
type Memory = VirtualMemory<DefaultMemoryImpl>;

/// Current and announced rate policies; superseded ones are pruned when scheduling.
const MAX_POLICIES: u64 = 64;
/// A claim reuses its last observation this long; the lease then still covers ~59 minutes.
const OBSERVATION_REUSE_MS: u64 = MINUTE;

thread_local! {
    static CLAIMS: RefCell<StableBTreeMap<Vec<u8>, Stored<Claim>, Memory>> =
        RefCell::new(StableBTreeMap::init(store::memory(1)));
    static POLICIES: RefCell<StableBTreeMap<Vec<u8>, Stored<PandaRatePolicy>, Memory>> =
        RefCell::new(StableBTreeMap::init(store::memory(2)));
    static NEURONS: RefCell<StableBTreeMap<Vec<u8>, Stored<Vec<Hash>>, Memory>> =
        RefCell::new(StableBTreeMap::init(store::memory(3)));
    static EXPIRATIONS: RefCell<StableBTreeMap<Vec<u8>, Stored<Hash>, Memory>> =
        RefCell::new(StableBTreeMap::init(store::memory(4)));
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

/// Persist a claim, touching only the indexes and certified view that actually changed.
fn save(c: &mut Claim) {
    let id = c.view.claim_id;
    let old = load(id).ok();
    let old_release = old.as_ref().and_then(Claim::release_at);
    let release = c.release_at();
    if old_release != release {
        EXPIRATIONS.with_borrow_mut(|t| {
            if let Some(at) = old_release {
                t.delete(&expiry_key(at, id));
            }
            if let Some(at) = release {
                t.put(&expiry_key(at, id), &id);
            }
        });
    }
    if old.as_ref().is_some_and(Claim::holds) != c.holds() {
        let n = neuron(&c.view.terms);
        NEURONS.with_borrow_mut(|t| {
            let mut refs: Vec<Hash> = t.load(n.as_slice()).unwrap_or_default();
            if c.holds() {
                refs.push(id);
            } else {
                refs.retain(|v| *v != id);
            }
            if refs.is_empty() {
                t.delete(n.as_slice());
            } else {
                t.put(n.as_slice(), &refs);
            }
        });
    }
    if old.as_ref().is_none_or(|o| o.view != c.view) {
        if let Some(o) = &old {
            c.view.lease_revision = o.view.lease_revision.checked_add(1).expect("view revision");
        }
        store::CERT.with_borrow_mut(|t| t.put(key(id), &c.view));
    }
    CLAIMS.with_borrow_mut(|t| t.put(id.as_slice(), c));
}

/// Claims occupying a neuron with a release time; short Apply windows are not counted.
pub(crate) fn live_claims() -> u64 {
    EXPIRATIONS.with_borrow(|t| t.len())
}

#[ic_cdk::update]
fn schedule_panda_rate(policy: PandaRatePolicy) -> Result<PandaRatePolicy> {
    let c = store::governance(ic_cdk::api::msg_caller())?;
    let at = nanos_to_millis(ic_cdk::api::time());
    let mut policy = policy;
    // Local fixtures may seed a previously announced policy; production publication is consensus time.
    if c.init.environment != Environment::Local {
        policy.published_at_ms = at;
    }
    validate_rate_policy(&policy)?;
    ensure(
        policy.environment == c.init.environment && policy.published_at_ms <= at,
        Error::Forbidden,
    )?;
    let id = policy.policy_version.to_be_bytes();
    if let Some(old) = POLICIES.with_borrow(|t| t.load(&id)) {
        ensure(old == policy, Error::IdempotencyConflict)?;
        return Ok(old);
    }
    POLICIES.with_borrow_mut(|t| -> Result<()> {
        let current: Vec<PandaRatePolicy> = t.iter().map(|v| v.value().0).collect();
        for p in &current {
            if superseded(p, &current, at) {
                t.delete(&p.policy_version.to_be_bytes());
            }
        }
        ensure(t.len() < MAX_POLICIES, Error::QuotaExceeded)?;
        ensure(
            t.iter().all(|v| {
                let p = v.value().0;
                p.effective_at_ms != policy.effective_at_ms
                    || !p
                        .product_ids
                        .iter()
                        .any(|id| policy.product_ids.contains(id))
            }),
            Error::VersionConflict,
        )?;
        t.put(&id, &policy);
        Ok(())
    })?;
    Ok(policy)
}

/// A policy is never selected again once each of its products has a later effective one.
fn superseded(p: &PandaRatePolicy, all: &[PandaRatePolicy], at: u64) -> bool {
    p.product_ids.iter().all(|product| {
        all.iter().any(|q| {
            q.effective_at_ms > p.effective_at_ms
                && q.effective_at_ms <= at
                && q.product_ids.contains(product)
        })
    })
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
    let quote = quote_panda(&offer, &app, &product, &policy(&offer.product_id, at)?, at)?;
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
            at < t.offer.accept_by_ms && policy(&t.offer.product_id, at)? == t.quote.policy,
            Error::PolicyStale,
        )?;
    }
    Ok(at)
}

/// Expire due references, then allow only the same beneficiary's committed contiguous term.
fn check_occupancy(terms: &PandaApplicationTerms, at: u64) -> Result<()> {
    let refs = NEURONS
        .with_borrow(|t| t.load(neuron(terms).as_slice()))
        .unwrap_or_default();
    let mut held = Vec::with_capacity(refs.len());
    for id in refs {
        let mut c = load(id)?;
        if c.release_at().is_some_and(|end| at >= end) {
            c.expire(at)?;
            save(&mut c);
        } else {
            held.push(c);
        }
    }
    ensure(
        held.len() < 2
            && held.iter().all(|old| {
                old.view.committed_until_ms > 0
                    && old.view.terms.offer.beneficiary == terms.offer.beneficiary
                    && old.view.terms.offer.expires_at_ms == terms.offer.starts_at_ms
            }),
        Error::NeuronOccupied,
    )
}

#[ic_cdk::update]
async fn request_panda_claim(request: PandaClaimRequest) -> Result<PandaClaimView> {
    let caller = ic_cdk::api::msg_caller();
    let home = ic_cdk::api::canister_self();
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
    let at = authorize(&request, home, true).await?;
    if let Ok(c) = load(id) {
        ensure(c.view.terms == request.terms, Error::IdempotencyConflict)?;
        return Ok(c.view);
    }
    sweep(at)?;
    check_occupancy(&request.terms, at)?;
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
    save(&mut Claim::new(id, request, service.cooling_ms));
    let at = reserve_product(id, at).await?;
    qualify(id, at).await?;
    Ok(load(id)?.view)
}

/// Reserve the product interval once; returns the time after any product call.
async fn reserve_product(id: Hash, at: u64) -> Result<u64> {
    let old = load(id)?;
    if old.product_reserved || !old.holds() {
        return Ok(at);
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
    if current.holds() {
        match answer {
            Ok(Ok(())) => current.product_reserved = true,
            Ok(Err(_)) => current.reject(),
            Err(_) => return Err(Error::ExecutionUnknown),
        }
        save(&mut current);
    }
    Ok(nanos_to_millis(ic_cdk::api::time()))
}

/// Observe the neuron at `at`, reusing an observation younger than one minute.
async fn qualify(id: Hash, at: u64) -> Result<()> {
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
        save(&mut c);
        return Ok(());
    }
    if at < c.view.observed_at_ms.saturating_add(OBSERVATION_REUSE_MS) {
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
    ensure(load(id)?.generation == generation, Error::VersionConflict)?;
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
    let eligibility = match result {
        Ok(sns::GetNeuronResponse {
            result: Some(sns::NeuronResult::Neuron(n)),
        }) if store::config().init == verification_pin => sns::assess(
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
    let home = ic_cdk::api::canister_self();
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
    ensure(c.holds(), Error::MembershipIneligible)?;
    let at = nanos_to_millis(ic_cdk::api::time());
    admission()?;
    ensure(
        c.view.cooling_until_ms.is_some_and(|ready| at >= ready)
            && at < c.view.terms.quote.application_deadline_ms,
        Error::Pending,
    )?;
    store::reserve_call(at, store::CallBudget::Authorization(caller))?;
    let at = reserve_product(id, at).await?;
    qualify(id, at).await?;
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
        let at = reserve_product(id, nanos_to_millis(ic_cdk::api::time())).await?;
        qualify(id, at).await?;
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
        save(&mut c);
    } else if c.view.status == PandaClaimStatus::Active
        && (c.view.eligibility != Eligibility::Eligible
            || c.view.valid_until_ms <= at.saturating_add(MINUTE))
    {
        qualify(id, at).await?;
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
        save(&mut c);
        released += 1;
    }
    Ok(released)
}

#[ic_cdk::update]
fn sweep_panda_commitments() -> Result<u32> {
    sweep(nanos_to_millis(ic_cdk::api::time()))
}

/// Stage every claim view; the caller publishes the root once.
pub(crate) fn certify_all(cert: &mut Certification) {
    CLAIMS.with_borrow(|t| {
        t.for_each(|_, c: Claim| {
            cert.set(key(c.view.claim_id), canonical(&c.view));
        })
    });
}

#[ic_cdk::query]
fn panda_operations(after: Option<Hash>, take: u16) -> Result<PandaOperationsPage> {
    use std::ops::Bound::{Excluded, Unbounded};
    let caller = ic_cdk::api::msg_caller();
    authenticated(caller)?;
    ensure_valid((1..=32).contains(&take), "page size")?;
    let governance = caller == store::config().init.governance;
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
            if governance || actor(&c, caller).is_ok() {
                claims.push(c.view);
            }
            if claims.len() >= usize::from(take) {
                break;
            }
        }
    });
    Ok(PandaOperationsPage { claims, next })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture;

    #[test]
    fn save_touches_indexes_and_certified_view_only_when_they_change() {
        let request = fixture::claim();
        let id = panda_claim_id(&request.terms);
        let n = neuron(&request.terms);
        let mut c = Claim::new(id, request, PANDA_COOLING_MS);
        save(&mut c);
        let leaf = |id| store::CERT.with_borrow(|t| t.get(&key(id)).map(<[u8]>::to_vec));
        let certified = leaf(id);
        assert_eq!(live_claims(), 1);
        assert_eq!(
            NEURONS.with_borrow(|t| t.load(n.as_slice())),
            Some(vec![id])
        );
        // A private busy marker keeps the public revision and certified leaf.
        c.busy_until_ms = 99;
        save(&mut c);
        assert_eq!(load(id).unwrap().view.lease_revision, 1);
        assert_eq!(leaf(id), certified);
        c.cancel().unwrap();
        save(&mut c);
        let saved = load(id).unwrap();
        assert_eq!(saved.view.lease_revision, 2);
        assert_ne!(leaf(id), certified);
        assert_eq!(live_claims(), 0);
        assert!(NEURONS.with_borrow(|t| t.load(n.as_slice())).is_none());
    }

    #[test]
    fn only_policies_replaced_for_every_product_are_superseded() {
        let policy = |version, effective_at_ms, products: &[&str]| PandaRatePolicy {
            version: COMMERCE_VERSION,
            policy_version: version,
            environment: Environment::Local,
            product_ids: products.iter().map(|p| p.to_string()).collect(),
            r_num: 1,
            r_den: 1,
            published_at_ms: 0,
            effective_at_ms,
        };
        let old = policy(1, 10, &["a", "b"]);
        let a = policy(2, 20, &["a"]);
        let b = policy(3, 30, &["b"]);
        let all = [old.clone(), a.clone(), b.clone()];
        assert!(!superseded(&old, &all, 29));
        assert!(superseded(&old, &all, 30));
        assert!(!superseded(&a, &all, u64::MAX));
        assert!(!superseded(&b, &all, u64::MAX));
    }
}
