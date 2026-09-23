use crate::{model::Claim, sns, store::*};
use candid::Principal;
use dmsg_protocol::{billing::close_claim_digest, membership::*, *};
use dmsg_runtime::storage::MapExt;
use dmsg_types::{membership::*, *};

fn now() -> u64 {
    nanos_to_millis(ic_cdk::api::time())
}

fn governance() -> Result<()> {
    ensure(
        ic_cdk::api::msg_caller() == with_config(|c| c.init.governance),
        Error::Forbidden,
    )
}

fn product(b: &Beneficiary) -> Result<ProductConfig> {
    validate_beneficiary(b)?;
    with_config(|c| {
        c.init
            .products
            .iter()
            .find(|p| {
                p.product_id == b.product_id
                    && p.authorities.contains(&b.authority_canister)
                    && p.subject_schema == b.subject_schema
                    && usize::from(p.subject_size) == b.subject_bytes.len()
            })
            .cloned()
    })
    .ok_or(Error::Forbidden)
}

fn validate_product(p: &ProductConfig) -> Result<()> {
    authenticated(p.adapter)?;
    ensure(
        !p.product_id.is_empty()
            && p.product_id.len() <= 32
            && !p.authorities.is_empty()
            && p.authorities.len() <= 16
            && (1..=64).contains(&p.subject_size)
            && !p.subject_schema.is_empty()
            && p.subject_schema.len() <= 64,
        invalid("product"),
    )?;
    let mut authorities = std::collections::BTreeSet::new();
    for authority in &p.authorities {
        authenticated(*authority)?;
        ensure(
            authorities.insert(*authority),
            invalid("duplicate authority"),
        )?;
    }
    Ok(())
}

fn admission_open() -> Result<()> {
    ensure(
        with_config(|c| c.sns_verified && !c.paused),
        Error::Unavailable("SNS admission is paused or unverified".into()),
    )
}

fn validate_policy(p: &MembershipPolicy) -> Result<()> {
    ensure(
        p.version > 0
            && !p.product_id.is_empty()
            && p.product_id.len() <= 32
            && p.subsidy_units > 0,
        invalid("policy"),
    )?;
    required_panda(&p.threshold, 8)?;
    Ok(())
}

#[ic_cdk::init]
fn init(args: MembershipInit) {
    let mut args = args;
    for p in [args.governance, args.sns_root, args.panda_ledger] {
        authenticated(p).expect("SNS configuration");
    }
    assert!(!args.products.is_empty() && args.products.len() <= 16 && args.policies.len() <= 64);
    assert!(
        (MIN_COOLING_MS..=DAY).contains(&args.cooling_ms)
            && args.hourly_applications > 0
            && args.hourly_applications <= 1000
            && args.max_claims > 0
            && args.max_claims <= 1_000_000
    );
    let mut products = std::collections::BTreeSet::new();
    for p in &args.products {
        validate_product(p).expect("product");
        assert!(products.insert(p.product_id.clone()), "duplicate product");
    }
    for p in args.policies.drain(..) {
        validate_policy(&p).expect("policy");
        assert!(products.contains(&p.product_id), "unknown policy product");
        validate_policy_slot(&p).expect("policy slot");
        save_policy(&p);
    }
    save_config(&Config {
        init: args,
        sns_verified: false,
        paused: false,
    });
    persist_limits();
    rebuild();
}

#[ic_cdk::pre_upgrade]
fn pre_upgrade() {
    persist_limits();
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    rebuild();
}

#[ic_cdk::update]
async fn verify_sns_configuration() -> Result<()> {
    governance()?;
    let c = config();
    let reply: sns::SnsCanisters = dmsg_runtime::call(
        c.init.sns_root,
        "list_sns_canisters",
        (sns::ListRequest {},),
    )
    .await?;
    ensure(
        reply.root == Some(c.init.sns_root)
            && reply.governance == Some(c.init.governance)
            && reply.ledger == Some(c.init.panda_ledger),
        Error::IntegrityFailed,
    )?;
    let decimals: u8 = dmsg_runtime::call(c.init.panda_ledger, "icrc1_decimals", ()).await?;
    ensure(decimals == 8, Error::UnsupportedProtocol)?;
    let mut c = config();
    c.sns_verified = true;
    save_config(&c);
    Ok(())
}

#[ic_cdk::update]
fn schedule_policy(policy: MembershipPolicy) -> Result<()> {
    governance()?;
    validate_policy(&policy)?;
    ensure(
        policy.effective_at_ms >= now().saturating_add(30 * DAY),
        invalid("30 day notice"),
    )?;
    ensure(
        with_config(|c| {
            c.init
                .products
                .iter()
                .any(|p| p.product_id == policy.product_id)
        }),
        invalid("unknown policy product"),
    )?;
    validate_policy_slot(&policy)?;
    ensure(
        POLICIES.with_borrow(|t| t.len()) < 4096,
        Error::QuotaExceeded,
    )?;
    save_policy(&policy);
    Ok(())
}

#[ic_cdk::update]
fn register_product(p: ProductConfig) -> Result<()> {
    governance()?;
    validate_product(&p)?;
    let mut c = config();
    ensure(
        c.init.products.len() < 16 && !c.init.products.iter().any(|v| v.product_id == p.product_id),
        Error::VersionConflict,
    )?;
    c.init.products.push(p);
    save_config(&c);
    Ok(())
}

#[ic_cdk::update]
fn set_admission_pause(paused: bool) -> Result<()> {
    governance()?;
    let mut c = config();
    c.paused = paused;
    save_config(&c);
    Ok(())
}

#[ic_cdk::update]
fn increase_subsidy_budget(total: u64) -> Result<()> {
    governance()?;
    let mut c = config();
    ensure(
        total >= c.init.subsidy_budget,
        invalid("budget cannot revoke commitments"),
    )?;
    c.init.subsidy_budget = total;
    save_config(&c);
    Ok(())
}

fn acquire(mut c: Claim, at: u64) -> Result<Claim> {
    ensure(at >= c.busy_until_ms, Error::Pending)?;
    c.generation = c.generation.checked_add(1).ok_or(Error::QuotaExceeded)?;
    c.busy_until_ms = at + MINUTE;
    save(&mut c);
    Ok(c)
}

fn current(id: Hash, generation: u64) -> Result<Claim> {
    let mut c = load(id)?;
    ensure(c.generation == generation, Error::VersionConflict)?;
    c.busy_until_ms = 0;
    Ok(c)
}

fn validate_authorization(
    intent: &MembershipIntent,
    a: &MembershipAuthorization,
    at: u64,
) -> Result<()> {
    ensure(
        a.intent_digest == membership_intent_digest(intent)
            && at < a.valid_until_ms
            && at < intent.valid_until_ms
            && a.verified_at_ms <= at
            && at - a.verified_at_ms <= MINUTE,
        Error::PolicyStale,
    )
}

/// Authorization is read-only; unapproved initial requests never allocate claims.
async fn authorize(request: &ClaimRequest) -> Result<(MembershipAuthorization, u64)> {
    let p = product(&request.authorization.beneficiary)?;
    let result: Result<MembershipAuthorization> =
        dmsg_runtime::call(p.adapter, "authorize_membership_intent", (request.clone(),)).await?;
    let a = result?;
    let at = now();
    validate_authorization(&request.authorization, &a, at)?;
    Ok((a, at))
}

fn admission_policy(request: &ClaimRequest, at: u64) -> Result<MembershipPolicy> {
    admission_open()?;
    let policy = POLICIES
        .with_borrow(|t| t.load(&request.policy_version.to_be_bytes()))
        .ok_or(Error::NotFound)?;
    ensure(
        policy.product_id == request.authorization.beneficiary.product_id
            && policy.benefit_id == request.benefit_id
            && effective_policy(&policy.product_id, policy.benefit_id, at) == Some(policy.version),
        Error::PolicyStale,
    )?;
    Ok(policy)
}

#[ic_cdk::update]
async fn request_claim(request: ClaimRequest) -> Result<ClaimView> {
    let at = now();
    let canister_id = ic_cdk::api::canister_self();
    let caller = ic_cdk::api::msg_caller();
    authenticated(caller)?;
    let auth = &request.authorization;
    ensure(
        caller == auth.actor
            && auth.service_canister == canister_id
            && with_config(|c| auth.environment == c.init.environment),
        Error::Forbidden,
    )?;
    ensure(
        auth.action_digest == claim_action_digest(&request),
        Error::IntegrityFailed,
    )?;
    expiry(at, auth.valid_until_ms, DAY)?;
    nonzero(auth.application_id.as_slice())?;
    nonzero(request.neuron_id.as_slice())?;
    product(&auth.beneficiary)?;
    let id = claim_id(canister_id, auth);
    if let Ok(c) = load(id) {
        ensure(c.request == request, Error::IdempotencyConflict)?;
        return advance(id, caller, at).await;
    }
    admission_policy(&request, at)?;
    reserve_call(at, CallBudget::Authorization(caller))?;
    let (authorization, observed) = authorize(&request).await?;
    // A concurrent authorized request may already have reserved this exact ID.
    if let Ok(c) = load(id) {
        ensure(c.request == request, Error::IdempotencyConflict)?;
        return Ok(c.view);
    }
    let at = observed;
    expiry(at, auth.valid_until_ms, DAY)?;
    product(&auth.beneficiary)?;
    let policy = admission_policy(&request, at)?;
    sweep_expired(at)?;
    let mut required = required_panda(&policy.threshold, 8)?;
    let previous = match request.change {
        ClaimChange::Start => None,
        ClaimChange::Renew { previous_claim }
        | ClaimChange::Upgrade { previous_claim }
        | ClaimChange::Replace { previous_claim } => Some(expire_claim(previous_claim, at)?),
    };
    if let Some(p) = &previous {
        ensure(
            p.view.status == ClaimStatus::Active
                && p.request.authorization.beneficiary == auth.beneficiary,
            Error::Forbidden,
        )?;
        if matches!(
            request.change,
            ClaimChange::Upgrade { .. } | ClaimChange::Replace { .. }
        ) {
            required = required.max(p.required_atomic);
        }
    }
    let key = neuron_key(with_config(|c| c.init.governance), request.neuron_id);
    let mut bound = NEURONS
        .with_borrow(|t| t.load(key.as_slice()))
        .unwrap_or_default();
    for id in &bound {
        expire_claim(*id, at)?;
    }
    bound.retain(|v| {
        load(*v)
            .is_ok_and(|c| !matches!(c.view.status, ClaimStatus::Released | ClaimStatus::Rejected))
    });
    ensure(
        bound.len() < 2
            && bound
                .iter()
                .all(|id| previous.as_ref().is_some_and(|p| p.view.claim_id == *id)),
        Error::VersionConflict,
    )?;
    // One pending application per beneficiary, including different neurons.
    let pending_key = digest("membership/pending-subject/v1", &auth.beneficiary);
    if let Some(pending) = PENDING.with_borrow(|t| t.load(pending_key.as_slice())) {
        let p = load(pending)?;
        if at >= p.request.authorization.valid_until_ms
            && matches!(
                p.view.status,
                ClaimStatus::Checking | ClaimStatus::CoolingDown
            )
        {
            release_pending(pending)?;
        } else {
            return Err(Error::Pending);
        }
    }
    ensure(
        CLAIMS.with_borrow(|t| t.len()) < with_config(|c| c.init.max_claims),
        Error::QuotaExceeded,
    )?;
    reserve_application(at, policy.subsidy_units)?;
    let view = ClaimView {
        schema: 1,
        home_membership: canister_id,
        claim_id: id,
        beneficiary: auth.beneficiary.clone(),
        benefit_id: request.benefit_id,
        policy_version: policy.version,
        status: ClaimStatus::Checking,
        eligibility: Eligibility::Unverifiable,
        starts_at_ms: 0,
        expires_at_ms: 0,
        observed_at_ms: 0,
        valid_until_ms: 0,
        lease_revision: 0,
        decision_id: None,
        release_after_ms: 0,
    };
    let mut c = Claim {
        request,
        policy,
        required_atomic: required,
        view,
        cooling_since_ms: None,
        busy_until_ms: 0,
        generation: 0,
        decision: None,
        receipt: None,
        budget_reserved: true,
        last_issued_until_ms: 0,
    };
    bound.push(id);
    NEURONS.with_borrow_mut(|t| t.put(key.as_slice(), &bound));
    save(&mut c);
    let c = acquire(c, at)?;
    check_qualification(c, authorization, observed).await
}

#[ic_cdk::update]
async fn advance_application(id: Hash) -> Result<ClaimView> {
    advance(id, ic_cdk::api::msg_caller(), now()).await
}

async fn advance(id: Hash, caller: Principal, at: u64) -> Result<ClaimView> {
    let old = load(id)?;
    ensure(
        caller == old.request.authorization.actor
            || caller == product(&old.view.beneficiary)?.adapter,
        Error::Forbidden,
    )?;
    if matches!(
        old.view.status,
        ClaimStatus::Applying | ClaimStatus::Closing
    ) {
        return deliver(id, at, true).await;
    }
    if !old.pending() {
        return Ok(expire_claim(id, at)?.view);
    }
    if at >= old.request.authorization.valid_until_ms {
        release_pending(id)?;
        return Err(Error::Expired);
    }
    if old
        .cooling_since_ms
        .is_some_and(|s| at < s + with_config(|c| c.init.cooling_ms))
    {
        return Ok(old.view);
    }
    admission_open()?;
    ensure(at >= old.busy_until_ms, Error::Pending)?;
    reserve_call(
        at,
        CallBudget::Authorization(old.request.authorization.actor),
    )?;
    let c = acquire(old, at)?;
    match authorize(&c.request).await {
        Ok((authorization, observed)) => {
            let c = current(id, c.generation)?;
            check_qualification(c, authorization, observed).await
        }
        Err(e) => {
            let mut c = current(id, c.generation)?;
            if c.cooling_since_ms.is_none()
                && !matches!(
                    e,
                    Error::ExecutionUnknown | Error::MembershipStale | Error::Unavailable(_)
                )
            {
                c.release()?;
                release_budget(&mut c);
            }
            save(&mut c);
            Err(e)
        }
    }
}

async fn check_qualification(
    mut c: Claim,
    authorization: MembershipAuthorization,
    observed: u64,
) -> Result<ClaimView> {
    // No user-visible state is held across authorization or SNS callbacks without CAS.
    let ready = (|| {
        admission_open()?;
        ensure(
            matches!(
                c.view.status,
                ClaimStatus::Checking | ClaimStatus::CoolingDown
            ),
            Error::VersionConflict,
        )?;
        reserve_call(observed, CallBudget::Qualification)
    })();
    if let Err(e) = ready {
        c.busy_until_ms = 0;
        save(&mut c);
        return Err(e);
    }
    c.busy_until_ms = observed + MINUTE;
    save(&mut c);
    let neuron = sns::read(with_config(|c| c.init.governance), c.request.neuron_id).await;
    let at = now();
    let mut c = current(c.view.claim_id, c.generation)?;
    let result = (|| -> Result<()> {
        ensure(
            matches!(
                c.view.status,
                ClaimStatus::Checking | ClaimStatus::CoolingDown
            ),
            Error::VersionConflict,
        )?;
        if at >= c.request.authorization.valid_until_ms {
            c.release()?;
            release_budget(&mut c);
            return Err(Error::Expired);
        }
        ensure(at < authorization.valid_until_ms, Error::Expired)?;
        admission_open()?;
        let (_, end) = c.term(at)?;
        let eligibility = neuron.as_ref().map_or(Eligibility::Unverifiable, |n| {
            sns::assess(
                n,
                c.request.neuron_id,
                c.request.authorization.actor,
                c.required_atomic,
                end,
                observed,
            )
        });
        c.view.eligibility = eligibility.clone();
        c.view.observed_at_ms = observed;
        if eligibility == Eligibility::Ineligible {
            c.release()?;
            release_budget(&mut c);
            return Err(Error::MembershipIneligible);
        }
        ensure(
            eligibility == Eligibility::Eligible && at < observed.saturating_add(MAX_LEASE_MS),
            Error::MembershipStale,
        )?;
        if c.cooling_since_ms.is_none() {
            c.cooling_since_ms = Some(observed);
            c.view.status = ClaimStatus::CoolingDown;
        } else {
            c.applying(at, observed)?;
            if let Some(d) = &mut c.decision {
                d.apply_by_ms = d.apply_by_ms.min(authorization.valid_until_ms);
            }
        }
        Ok(())
    })();
    save(&mut c);
    result?;
    if c.view.status == ClaimStatus::Applying {
        deliver(c.view.claim_id, at, false).await
    } else {
        Ok(c.view)
    }
}

fn release_pending(id: Hash) -> Result<Claim> {
    let mut c = load(id)?;
    if c.view.status == ClaimStatus::Released {
        return Ok(c);
    }
    ensure(
        matches!(
            c.view.status,
            ClaimStatus::Checking | ClaimStatus::CoolingDown | ClaimStatus::Rejected
        ),
        Error::Pending,
    )?;
    c.release()?;
    release_budget(&mut c);
    save(&mut c);
    Ok(c)
}

/// New immutable decisions go straight to Apply. Recovery first asks for the original receipt.
async fn deliver(id: Hash, at: u64, reconcile: bool) -> Result<ClaimView> {
    let c = expire_claim(id, at)?;
    if !matches!(c.view.status, ClaimStatus::Applying | ClaimStatus::Closing) || c.receipt.is_some()
    {
        return Ok(c.view);
    }
    let c = acquire(c, at)?;
    let d = c.decision.clone().ok_or(Error::IntegrityFailed)?;
    let adapter = product(&c.view.beneficiary)?.adapter;
    let result: Result<MembershipDecisionReceipt> = async {
        if reconcile {
            let known: Result<Option<MembershipDecisionReceipt>> =
                dmsg_runtime::call(adapter, "get_membership_decision", (d.decision_id,)).await?;
            let latest = current(id, c.generation)?;
            ensure(
                matches!(
                    latest.view.status,
                    ClaimStatus::Applying | ClaimStatus::Closing
                ) && latest.receipt.is_none(),
                Error::VersionConflict,
            )?;
            if let Some(r) = known? {
                return Ok(r);
            }
        }
        dmsg_runtime::call(adapter, "apply_membership_decision", (d,)).await?
    }
    .await;
    let at = now();
    let mut c = current(id, c.generation)?;
    let result = result.and_then(|r| c.accept_receipt(r));
    if c.view.status == ClaimStatus::Rejected {
        release_budget(&mut c);
    }
    save(&mut c);
    result?;
    if c.view.status == ClaimStatus::Active {
        match c.request.change {
            ClaimChange::Upgrade { previous_claim } | ClaimChange::Replace { previous_claim } => {
                prepare_close(previous_claim, at, at.saturating_add(5 * MINUTE))?;
            }
            _ => {}
        }
    }
    Ok(c.view)
}

#[ic_cdk::update]
async fn refresh_claim(id: Hash) -> Result<ClaimView> {
    refresh(id, ic_cdk::api::msg_caller(), now()).await
}

async fn refresh(id: Hash, caller: Principal, at: u64) -> Result<ClaimView> {
    let c = load(id)?;
    ensure(
        caller == c.request.authorization.actor || caller == product(&c.view.beneficiary)?.adapter,
        Error::Forbidden,
    )?;
    let c = expire_claim(id, at)?;
    if c.view.status == ClaimStatus::Applying
        || (c.view.status == ClaimStatus::Closing && c.receipt.is_none())
    {
        return deliver(id, at, true).await;
    }
    if c.view.status != ClaimStatus::Active
        || (at < c.view.valid_until_ms && c.view.eligibility == Eligibility::Eligible)
    {
        return Ok(c.view);
    }
    ensure(
        at >= c.view.observed_at_ms.saturating_add(MINUTE) && at >= c.busy_until_ms,
        Error::Pending,
    )?;
    reserve_call(at, CallBudget::Refresh)?;
    let c = acquire(c, at)?;
    let observed = at;
    let result = sns::read(with_config(|c| c.init.governance), c.request.neuron_id).await;
    let at = now();
    let mut c = current(id, c.generation)?;
    ensure(c.view.status == ClaimStatus::Active, Error::VersionConflict)?;
    let result = if c.release_deadline().is_some_and(|deadline| at >= deadline) {
        c.release()?;
        release_budget(&mut c);
        Ok(())
    } else {
        let eligible = result.as_ref().map_or(Eligibility::Unverifiable, |n| {
            sns::assess(
                n,
                c.request.neuron_id,
                c.request.authorization.actor,
                c.required_atomic,
                c.view.expires_at_ms,
                observed,
            )
        });
        c.observe(eligible, observed, at)
    };
    save(&mut c);
    result?;
    Ok(c.view)
}

#[ic_cdk::update]
async fn request_change(id: Hash, intent: MembershipIntent) -> Result<ClaimView> {
    let at = now();
    let caller = ic_cdk::api::msg_caller();
    let c = load(id)?;
    let p = product(&c.view.beneficiary)?;
    ensure(
        caller == intent.actor
            && intent.actor == c.request.authorization.actor
            && intent.beneficiary == c.view.beneficiary
            && intent.service_canister == ic_cdk::api::canister_self()
            && with_config(|c| intent.environment == c.init.environment)
            && intent.action_digest == close_claim_digest(id),
        Error::Forbidden,
    )?;
    let c = expire_claim(id, at)?;
    if matches!(c.view.status, ClaimStatus::Closing | ClaimStatus::Released) {
        return deliver(id, at, true).await;
    }
    ensure(c.view.status == ClaimStatus::Active, Error::VersionConflict)?;
    expiry(at, intent.valid_until_ms, DAY)?;
    let c = acquire(c, at)?;
    let response: Result<Result<MembershipAuthorization>> = dmsg_runtime::call(
        p.adapter,
        "authorize_membership_close",
        (id, intent.clone()),
    )
    .await;
    let at = now();
    let mut c = current(id, c.generation)?;
    let result = response.and_then(|r| r).and_then(|a| {
        validate_authorization(&intent, &a, at)?;
        c.prepare_close(at, intent.valid_until_ms.min(a.valid_until_ms))
    });
    save(&mut c);
    result?;
    deliver(id, at, false).await
}

#[ic_cdk::update]
async fn get_claim_for_consumer(id: Hash) -> Result<ClaimView> {
    let caller = ic_cdk::api::msg_caller();
    ensure(
        caller == product(&load(id)?.view.beneficiary)?.adapter,
        Error::Forbidden,
    )?;
    refresh(id, caller, now()).await
}

#[ic_cdk::query]
fn get_claim_certified(ids: Vec<Hash>) -> Result<CertifiedBatch> {
    CERT.with_borrow(|c| {
        c.batch(
            ic_cdk::api::canister_self(),
            ids.into_iter().map(|id| claim_key(id).to_vec()).collect(),
        )
    })
}

#[ic_cdk::query]
fn get_operation(id: Hash) -> Result<ClaimView> {
    let caller = ic_cdk::api::msg_caller();
    let c = load(id)?;
    ensure(
        caller == c.request.authorization.actor || caller == product(&c.view.beneficiary)?.adapter,
        Error::Forbidden,
    )?;
    Ok(c.view)
}

#[ic_cdk::query]
fn get_policy(version: u64) -> Result<MembershipPolicy> {
    POLICIES.with_borrow(|t| t.load(&version.to_be_bytes()).ok_or(Error::NotFound))
}

fn prepare_close(id: Hash, at: u64, apply_by: u64) -> Result<bool> {
    let mut c = expire_claim(id, at)?;
    let changed = c.prepare_close(at, apply_by)?;
    if changed {
        save(&mut c);
    }
    Ok(changed)
}

#[ic_cdk::update]
async fn close_for_consumer(id: Hash) -> Result<ClaimView> {
    ensure(
        ic_cdk::api::msg_caller() == product(&load(id)?.view.beneficiary)?.adapter,
        Error::Forbidden,
    )?;
    let at = now();
    let changed = prepare_close(id, at, at.saturating_add(5 * MINUTE))?;
    deliver(id, at, !changed).await
}

#[ic_cdk::update]
async fn reconcile_claim(id: Hash) -> Result<ClaimView> {
    deliver(id, now(), true).await
}

#[ic_cdk::query]
fn get_policy_certified(versions: Vec<u64>) -> Result<CertifiedBatch> {
    CERT.with_borrow(|c| {
        c.batch(
            ic_cdk::api::canister_self(),
            versions
                .into_iter()
                .map(|v| policy_key(v).to_vec())
                .collect(),
        )
    })
}

fn expire_claim(id: Hash, at: u64) -> Result<Claim> {
    let mut c = load(id)?;
    if c.release_deadline().is_some_and(|deadline| at >= deadline) {
        c.release()?;
        release_budget(&mut c);
        save(&mut c);
    }
    Ok(c)
}

fn sweep_expired(at: u64) -> Result<u32> {
    let ids = expired_ids(at);
    for id in &ids {
        expire_claim(*id, at)?;
    }
    Ok(ids.len() as u32)
}

/// Bounded local cleanup; unknown Apply/Close results are never swept by time.
#[ic_cdk::update]
fn sweep_expired_claims() -> Result<u32> {
    sweep_expired(now())
}

#[ic_cdk::update]
fn cancel_application(id: Hash) -> Result<ClaimView> {
    ensure(
        ic_cdk::api::msg_caller() == load(id)?.request.authorization.actor,
        Error::Forbidden,
    )?;
    Ok(release_pending(id)?.view)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn products_validate_identity_schema_bounds_and_unique_authorities() {
        let valid = ProductConfig {
            product_id: "p".into(),
            adapter: Principal::from_slice(&[1]),
            authorities: vec![Principal::from_slice(&[2])],
            subject_schema: "subject/1".into(),
            subject_size: 12,
        };
        validate_product(&valid).unwrap();
        let mutations: &[fn(&mut ProductConfig)] = &[
            |p| p.product_id.clear(),
            |p| p.product_id = "p".repeat(33),
            |p| p.adapter = Principal::anonymous(),
            |p| p.authorities.clear(),
            |p| p.authorities[0] = Principal::management_canister(),
            |p| p.authorities.push(p.authorities[0]),
            |p| p.subject_schema.clear(),
            |p| p.subject_schema = "s".repeat(65),
            |p| p.subject_size = 0,
            |p| p.subject_size = 65,
        ];
        for mutate in mutations {
            let mut p = valid.clone();
            mutate(&mut p);
            assert!(validate_product(&p).is_err());
        }
    }
}
