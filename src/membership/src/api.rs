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
        ic_cdk::api::msg_caller() == config().init.governance,
        Error::Forbidden,
    )
}

fn product(b: &Beneficiary) -> Result<ProductConfig> {
    validate_beneficiary(b)?;
    config()
        .init
        .products
        .into_iter()
        .find(|p| {
            p.product_id == b.product_id
                && p.authorities.contains(&b.authority_canister)
                && p.subject_schema == b.subject_schema
                && usize::from(p.subject_size) == b.subject_bytes.len()
        })
        .ok_or(Error::Forbidden)
}

fn validate_policy(p: &MembershipPolicy) -> Result<()> {
    ensure(
        p.version > 0 && p.product_id.len() <= 32 && p.subsidy_units > 0,
        invalid("policy"),
    )?;
    required_panda(&p.threshold, 8)?;
    Ok(())
}

#[ic_cdk::init]
fn init(args: MembershipInit) {
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
    for p in &args.policies {
        validate_policy(p).expect("policy");
        assert!(!POLICIES.with_borrow(|t| t.contains(&p.version.to_be_bytes())));
        POLICIES.with_borrow_mut(|t| t.put(&p.version.to_be_bytes(), p));
    }
    save_config(&Config {
        init: args,
        sns_verified: false,
        paused: false,
        reserved: 0,
        hour: 0,
        applications: 0,
        minute: 0,
        reads: 0,
    });
    rebuild();
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
        !POLICIES.with_borrow(|t| t.contains(&policy.version.to_be_bytes())),
        Error::IdempotencyConflict,
    )?;
    ensure(
        POLICIES.with_borrow(|t| t.len()) < 4096,
        Error::QuotaExceeded,
    )?;
    POLICIES.with_borrow_mut(|t| t.put(&policy.version.to_be_bytes(), &policy));
    CERT.with_borrow_mut(|c| c.put(policy_key(policy.version).to_vec(), &policy));
    Ok(())
}

#[ic_cdk::update]
fn register_product(p: ProductConfig) -> Result<()> {
    governance()?;
    authenticated(p.adapter)?;
    ensure(
        !p.product_id.is_empty()
            && p.product_id.len() <= 32
            && !p.authorities.is_empty()
            && p.authorities.len() <= 16
            && p.subject_size > 0
            && p.subject_size <= 64
            && p.subject_schema.len() <= 64,
        invalid("product"),
    )?;
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

fn reserve_read(at: u64) -> Result<()> {
    let mut c = config();
    let minute = at / MINUTE;
    if c.minute != minute {
        c.minute = minute;
        c.reads = 0;
    }
    ensure(c.reads < 200, Error::QuotaExceeded)?;
    c.reads += 1;
    save_config(&c);
    Ok(())
}

fn acquire(id: Hash, at: u64) -> Result<Claim> {
    let mut c = load(id)?;
    ensure(at >= c.busy_until_ms, Error::Pending)?;
    c.generation = c.generation.checked_add(1).ok_or(Error::QuotaExceeded)?;
    c.busy_until_ms = at + MINUTE;
    save(&c);
    Ok(c)
}

fn current(id: Hash, generation: u64) -> Result<Claim> {
    let mut c = load(id)?;
    ensure(c.generation == generation, Error::VersionConflict)?;
    c.busy_until_ms = 0;
    Ok(c)
}

/// Returns the verified authorization and the local callback timestamp.
async fn authorize(c: &Claim) -> Result<(MembershipAuthorization, u64)> {
    let p = product(&c.request.authorization.beneficiary)?;
    let result: Result<MembershipAuthorization> = dmsg_runtime::call(
        p.adapter,
        "authorize_membership_intent",
        (c.request.clone(),),
    )
    .await?;
    let a = result?;
    let at = now();
    ensure(
        a.intent_digest == membership_intent_digest(&c.request.authorization)
            && at < a.valid_until_ms
            && a.verified_at_ms <= at
            && at - a.verified_at_ms <= MINUTE,
        Error::PolicyStale,
    )?;
    Ok((a, at))
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
            && auth.environment == config().init.environment,
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
    let mut cfg = config();
    ensure(
        cfg.sns_verified && !cfg.paused,
        Error::Unavailable("SNS admission is paused or unverified".into()),
    )?;
    let policy = POLICIES
        .with_borrow(|t| t.load(&request.policy_version.to_be_bytes()))
        .ok_or(Error::NotFound)?;
    ensure(
        policy.product_id == auth.beneficiary.product_id
            && policy.benefit_id == request.benefit_id
            && at >= policy.effective_at_ms,
        Error::PolicyStale,
    )?;
    // Only the newest effective policy for this benefit can create new commitments.
    let superseded = POLICIES.with_borrow(|t| {
        let mut yes = false;
        t.for_each(|_, v| {
            if v.product_id == policy.product_id
                && v.benefit_id == policy.benefit_id
                && v.effective_at_ms <= at
                && v.effective_at_ms > policy.effective_at_ms
            {
                yes = true;
            }
        });
        yes
    });
    ensure(!superseded, Error::PolicyStale)?;
    let mut required = required_panda(&policy.threshold, 8)?;
    let previous = match request.change {
        ClaimChange::Start => None,
        ClaimChange::Renew { previous_claim }
        | ClaimChange::Upgrade { previous_claim }
        | ClaimChange::Replace { previous_claim } => Some(load(previous_claim)?),
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
    let key = neuron_key(cfg.init.governance, request.neuron_id);
    let mut bound = NEURONS
        .with_borrow(|t| t.load(key.as_slice()))
        .unwrap_or_default();
    for id in &bound {
        expire_claim(*id, at)?;
    }
    cfg = config();
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
            cfg = config();
        } else {
            return Err(Error::Pending);
        }
    }
    if cfg.hour != at / (60 * MINUTE) {
        cfg.hour = at / (60 * MINUTE);
        cfg.applications = 0;
    }
    ensure(
        cfg.applications < cfg.init.hourly_applications
            && CLAIMS.with_borrow(|t| t.len()) < cfg.init.max_claims,
        Error::QuotaExceeded,
    )?;
    cfg.reserved = cfg
        .reserved
        .checked_add(policy.subsidy_units)
        .ok_or(Error::QuotaExceeded)?;
    ensure(
        cfg.reserved <= cfg.init.subsidy_budget,
        Error::QuotaExceeded,
    )?;
    cfg.applications += 1;
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
    let c = Claim {
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
    save_config(&cfg);
    save(&c);
    advance(id, caller, at).await
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
        return deliver(id, at).await;
    }
    if !matches!(
        old.view.status,
        ClaimStatus::Checking | ClaimStatus::CoolingDown
    ) {
        return Ok(old.view);
    }
    if at >= old.request.authorization.valid_until_ms {
        release_pending(id)?;
        return Err(Error::Expired);
    }
    if old
        .cooling_since_ms
        .is_some_and(|s| at < s + config().init.cooling_ms)
    {
        return Ok(old.view);
    }
    reserve_read(at)?;
    let c = acquire(id, at)?;
    let auth = authorize(&c).await;
    let (authorization, observed) = match auth {
        Ok(authorized) => authorized,
        Err(e) => {
            let c = current(id, c.generation)?;
            save(&c);
            if c.cooling_since_ms.is_none()
                && !matches!(
                    e,
                    Error::ExecutionUnknown | Error::MembershipStale | Error::Unavailable(_)
                )
            {
                release_pending(id)?;
            }
            return Err(e);
        }
    };
    let auth_until = authorization.valid_until_ms;
    let neuron = sns::read(config().init.governance, c.request.neuron_id).await;
    let at = now();
    let mut c = current(id, c.generation)?;
    ensure(
        at < c.request.authorization.valid_until_ms && at < auth_until,
        Error::Expired,
    )?;
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
    if eligibility != Eligibility::Eligible {
        save(&c);
        if eligibility == Eligibility::Ineligible && c.cooling_since_ms.is_none() {
            release_pending(id)?;
        }
        return Err(if eligibility == Eligibility::Ineligible {
            Error::MembershipIneligible
        } else {
            Error::MembershipStale
        });
    }
    ensure(
        at < observed.saturating_add(MAX_LEASE_MS),
        Error::MembershipStale,
    )?;
    if c.cooling_since_ms.is_none() {
        c.cooling_since_ms = Some(observed);
        c.view.status = ClaimStatus::CoolingDown;
        save(&c);
        return Ok(load(id)?.view);
    }
    c.applying(at, observed)?;
    if let Some(d) = &mut c.decision {
        d.apply_by_ms = d.apply_by_ms.min(auth_until);
    }
    save(&c);
    deliver(id, at).await
}

fn release_pending(id: Hash) -> Result<()> {
    let mut c = load(id)?;
    ensure(
        matches!(
            c.view.status,
            ClaimStatus::Checking | ClaimStatus::CoolingDown | ClaimStatus::Rejected
        ),
        Error::Pending,
    )?;
    c.view.status = ClaimStatus::Released;
    c.generation += 1;
    c.busy_until_ms = 0;
    release_budget(&mut c);
    save(&c);
    Ok(())
}

fn release_budget(c: &mut Claim) {
    if c.budget_reserved {
        let mut cfg = config();
        cfg.reserved = cfg
            .reserved
            .checked_sub(c.policy.subsidy_units)
            .expect("reserved subsidy");
        save_config(&cfg);
        c.budget_reserved = false;
    }
}

async fn deliver(id: Hash, at: u64) -> Result<ClaimView> {
    let c = acquire(id, at)?;
    let d = c.decision.clone().ok_or(Error::IntegrityFailed)?;
    let adapter = product(&c.view.beneficiary)?.adapter;
    let known: Result<Option<MembershipDecisionReceipt>> =
        dmsg_runtime::call(adapter, "get_membership_decision", (d.decision_id,)).await?;
    let result = match known? {
        Some(r) => Ok(r),
        None => {
            let reply: Result<Result<MembershipDecisionReceipt>> =
                dmsg_runtime::call(adapter, "apply_membership_decision", (d,)).await;
            reply.and_then(|r| r)
        }
    };
    let mut c = current(id, c.generation)?;
    match result {
        Ok(r) => {
            c.accept_receipt(r)?;
            if c.view.status == ClaimStatus::Rejected {
                release_budget(&mut c);
            }
            save(&c);
            if c.view.status == ClaimStatus::Active
                && matches!(
                    c.request.change,
                    ClaimChange::Upgrade { .. } | ClaimChange::Replace { .. }
                )
            {
                let previous = match c.request.change {
                    ClaimChange::Upgrade { previous_claim }
                    | ClaimChange::Replace { previous_claim } => previous_claim,
                    _ => unreachable!(),
                };
                prepare_close(previous, now())?;
            }
            Ok(load(id)?.view)
        }
        Err(e) => {
            save(&c);
            Err(e)
        }
    }
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
    if c.view.status == ClaimStatus::Applying
        || (c.view.status == ClaimStatus::Closing && c.receipt.is_none())
    {
        return deliver(id, at).await;
    }
    if c.view.status == ClaimStatus::Closing
        && at >= c.view.release_after_ms
        && c.receipt
            .as_ref()
            .is_some_and(|r| r.outcome == DecisionOutcome::Applied)
    {
        let mut c = c;
        c.view.status = ClaimStatus::Released;
        release_budget(&mut c);
        save(&c);
        return Ok(load(id)?.view);
    }
    if c.view.status != ClaimStatus::Active {
        return Ok(load(id)?.view);
    }
    if at >= c.view.expires_at_ms && at >= c.view.release_after_ms {
        let mut c = c;
        c.view.status = ClaimStatus::Released;
        release_budget(&mut c);
        save(&c);
        return Ok(load(id)?.view);
    }
    if at < c.view.valid_until_ms && c.view.eligibility == Eligibility::Eligible {
        return Ok(load(id)?.view);
    }
    ensure(
        at >= c.view.observed_at_ms.saturating_add(MINUTE),
        Error::Pending,
    )?;
    reserve_read(at)?;
    let c = acquire(id, at)?;
    let observed = at;
    let result = sns::read(config().init.governance, c.request.neuron_id).await;
    let at = now();
    let mut c = current(id, c.generation)?;
    ensure(c.view.status == ClaimStatus::Active, Error::VersionConflict)?;
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
    c.observe(eligible, observed, at)?;
    save(&c);
    Ok(load(id)?.view)
}

#[ic_cdk::update]
async fn request_change(id: Hash, intent: MembershipIntent) -> Result<ClaimView> {
    let at = now();
    let c = load(id)?;
    let p = product(&c.view.beneficiary)?;
    ensure(
        ic_cdk::api::msg_caller() == intent.actor
            && intent.actor == c.request.authorization.actor
            && intent.beneficiary == c.view.beneficiary
            && intent.service_canister == ic_cdk::api::canister_self()
            && intent.environment == config().init.environment
            && intent.action_digest == close_claim_digest(id),
        Error::Forbidden,
    )?;
    if c.view.status == ClaimStatus::Closing {
        return deliver(id, at).await;
    }
    ensure(c.view.status == ClaimStatus::Active, Error::VersionConflict)?;
    let c = acquire(id, at)?;
    let result: Result<MembershipAuthorization> = dmsg_runtime::call(
        p.adapter,
        "authorize_membership_close",
        (id, intent.clone()),
    )
    .await?;
    let a = result?;
    let at = now();
    ensure(
        a.intent_digest == membership_intent_digest(&intent) && at < a.valid_until_ms,
        Error::PolicyStale,
    )?;
    let mut c = current(id, c.generation)?;
    ensure(c.view.status == ClaimStatus::Active, Error::VersionConflict)?;
    let decision_id = digest("membership/decision-id/v1", &(id, "close"));
    let mut d = c.decision.clone().ok_or(Error::IntegrityFailed)?;
    d.decision_id = decision_id;
    d.kind = DecisionKind::Close;
    d.apply_by_ms = intent.valid_until_ms.min(a.valid_until_ms);
    c.decision = Some(d);
    c.receipt = None;
    c.view.status = ClaimStatus::Closing;
    c.view.decision_id = Some(decision_id);
    c.view.valid_until_ms = at;
    save(&c);
    deliver(id, at).await
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
    Ok(load(id)?.view)
}

#[ic_cdk::query]
fn get_policy(version: u64) -> Result<MembershipPolicy> {
    POLICIES.with_borrow(|t| t.load(&version.to_be_bytes()).ok_or(Error::NotFound))
}

fn prepare_close(id: Hash, at: u64) -> Result<()> {
    let mut c = load(id)?;
    if c.view.status == ClaimStatus::Closing || c.view.status == ClaimStatus::Released {
        return Ok(());
    }
    ensure(c.view.status == ClaimStatus::Active, Error::VersionConflict)?;
    let decision_id = digest("membership/decision-id/v1", &(id, "close"));
    let mut d = c.decision.clone().ok_or(Error::IntegrityFailed)?;
    d.decision_id = decision_id;
    d.kind = DecisionKind::Close;
    c.generation += 1;
    c.busy_until_ms = 0;
    c.decision = Some(d);
    c.receipt = None;
    c.view.status = ClaimStatus::Closing;
    c.view.decision_id = Some(decision_id);
    c.view.valid_until_ms = at;
    save(&c);
    Ok(())
}

#[ic_cdk::update]
async fn close_for_consumer(id: Hash) -> Result<ClaimView> {
    ensure(
        ic_cdk::api::msg_caller() == product(&load(id)?.view.beneficiary)?.adapter,
        Error::Forbidden,
    )?;
    let at = now();
    prepare_close(id, at)?;
    deliver(id, at).await
}

#[ic_cdk::update]
async fn reconcile_claim(id: Hash) -> Result<ClaimView> {
    let at = now();
    expire_claim(id, at)?;
    let c = load(id)?;
    if matches!(c.view.status, ClaimStatus::Applying | ClaimStatus::Closing) {
        return deliver(id, at).await;
    }
    if matches!(
        c.view.status,
        ClaimStatus::Checking | ClaimStatus::CoolingDown
    ) && at >= c.request.authorization.valid_until_ms
    {
        release_pending(id)?;
    }
    Ok(load(id)?.view)
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

fn expire_claim(id: Hash, at: u64) -> Result<()> {
    let mut c = load(id)?;
    let terminal_commitment = c.view.status == ClaimStatus::Active
        || (c.view.status == ClaimStatus::Closing
            && c.receipt
                .as_ref()
                .is_some_and(|r| r.outcome == DecisionOutcome::Applied));
    let deadline = if c.view.status == ClaimStatus::Closing {
        c.view.release_after_ms
    } else {
        c.view.expires_at_ms.max(c.view.release_after_ms)
    };
    if terminal_commitment && at >= deadline {
        c.view.status = ClaimStatus::Released;
        c.generation += 1;
        c.busy_until_ms = 0;
        release_budget(&mut c);
        save(&c);
    } else if matches!(
        c.view.status,
        ClaimStatus::Checking | ClaimStatus::CoolingDown
    ) && at >= c.request.authorization.valid_until_ms
    {
        release_pending(id)?;
    }
    Ok(())
}

#[ic_cdk::update]
fn cancel_application(id: Hash) -> Result<ClaimView> {
    ensure(
        ic_cdk::api::msg_caller() == load(id)?.request.authorization.actor,
        Error::Forbidden,
    )?;
    release_pending(id)?;
    Ok(load(id)?.view)
}
