use candid::Principal;
use dmsg_types::{
    cose::*,
    handle::*,
    payment::SignedOffer,
    stable::{self, Certification, Table},
    user::*,
    *,
};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::cell::RefCell;

mod model;
#[cfg(test)]
mod tests;

// Stable schema v1: config=0, subjects=1, permanent auth routes=2,
// pending auth bindings=3, initialization intents=4. Never reuse ids.
thread_local! {
    static CONFIG: RefCell<Table> = RefCell::new(Table::new(0));
    static SUBJECTS: RefCell<Table> = RefCell::new(Table::new(1));
    static AUTH: RefCell<Table> = RefCell::new(Table::new(2));
    static BINDINGS: RefCell<Table> = RefCell::new(Table::new(3));
    static CREATIONS: RefCell<Table> = RefCell::new(Table::new(4));
    static CERT: RefCell<Certification> = RefCell::new(Certification::default());
}
#[derive(Serialize, Deserialize)]
struct Config {
    schema: u16,
    init: UserInit,
    day: u64,
    created_today: u32,
}
fn config() -> Config {
    CONFIG.with_borrow(|t| t.get(b"config").expect("initialized"))
}
fn load(id: &Hash) -> Result<Subject> {
    SUBJECTS.with_borrow(|t| t.get(id).ok_or(Error::NotFound))
}
fn save(s: &Subject) {
    SUBJECTS.with_borrow_mut(|t| t.put(&s.subject_id, s));
    CERT.with_borrow_mut(|c| c.put(s.subject_id.to_vec(), &s.snapshot()));
}
fn now() -> u64 {
    ic_cdk::api::time()
}
fn caller() -> Principal {
    ic_cdk::api::msg_caller()
}
fn me() -> Principal {
    ic_cdk::api::canister_self()
}
fn own(id: &Hash) -> Result<Subject> {
    let s = load(id)?;
    ensure(s.auth_bindings.contains(&caller()), Error::AuthRequired)?;
    Ok(s)
}

#[ic_cdk::init]
fn init(args: UserInit) {
    for p in [args.home_cose, args.handle_canister, args.payment_canister] {
        authenticated(p).expect("configured canister");
    }
    assert!(
        args.max_subjects > 0
            && args.max_subjects <= 1_000_000
            && args.daily_new_subjects > 0
            && args.daily_new_subjects <= 10_000,
        "hard limits"
    );
    CONFIG.with_borrow_mut(|t| {
        t.put(
            b"config",
            &Config {
                schema: 1,
                init: args,
                day: 0,
                created_today: 0,
            },
        )
    });
    CERT.with_borrow(|c| c.publish());
}
#[ic_cdk::post_upgrade]
fn post_upgrade() {
    CERT.with_borrow(|c| c.publish());
    assert_eq!(config().schema, 1, "unsupported stable schema");
    SUBJECTS.with_borrow(|t| {
        t.for_each::<Subject>(|key, s| CERT.with_borrow_mut(|c| c.put(key, &s.snapshot())))
    });
}

#[ic_cdk::update]
async fn create_subject(input: CreateSubject) -> Result<SubjectId> {
    let who = caller();
    let mut cfg = config();
    if let Some(id) = AUTH.with_borrow(|t| t.get::<Hash>(who.as_slice())) {
        let s = load(&id)?;
        ensure(s.auth_bindings.contains(&who), Error::DeviceNotApproved)?;
        return Ok(id);
    }
    // Validate the PoP before spending randomness cycles.
    model::create(me(), cfg.init.home_cose, [1; 32], who, &input, now())?;
    ensure(
        SUBJECTS.with_borrow(|t| t.len()) < cfg.init.max_subjects,
        Error::QuotaExceeded,
    )?;
    if now() / DAY > cfg.day {
        cfg.day = now() / DAY;
        cfg.created_today = 0;
    }
    ensure(
        cfg.created_today < cfg.init.daily_new_subjects,
        Error::QuotaExceeded,
    )?;
    if let Some((op, expires)) = CREATIONS.with_borrow(|t| t.get::<(Hash, u64)>(who.as_slice())) {
        ensure(
            now() >= expires,
            if op == input.op_id {
                Error::Pending
            } else {
                Error::IdempotencyConflict
            },
        )?;
    }
    CREATIONS.with_borrow_mut(|t| t.put(who.as_slice(), &(input.op_id, input.expires_at)));
    cfg.created_today += 1;
    CONFIG.with_borrow_mut(|t| t.put(b"config", &cfg));
    let random = ic_cdk_management_canister::raw_rand()
        .await
        .map_err(|_| Error::Unavailable("randomness".into()))?;
    if let Some(id) = AUTH.with_borrow(|t| t.get::<Hash>(who.as_slice())) {
        return Ok(id);
    }
    let current = CREATIONS
        .with_borrow(|t| t.get::<(Hash, u64)>(who.as_slice()))
        .ok_or(Error::VersionConflict)?;
    ensure(
        current == (input.op_id, input.expires_at),
        Error::VersionConflict,
    )?;
    ensure(
        SUBJECTS.with_borrow(|t| t.len()) < cfg.init.max_subjects,
        Error::QuotaExceeded,
    )?;
    let id = digest("dmsg/subject-id/v1", &(me(), random));
    ensure(
        !SUBJECTS.with_borrow(|t| t.contains(&id)),
        Error::VersionConflict,
    )?;
    let subject = model::create(me(), cfg.init.home_cose, id, who, &input, now())?;
    save(&subject);
    AUTH.with_borrow_mut(|t| t.put(who.as_slice(), &id));
    CREATIONS.with_borrow_mut(|t| t.remove(who.as_slice()));
    Ok(id)
}

/// Only the new authentication principal may reserve its own binding. The
/// existing subject's administrator must separately approve the exact nonce.
#[ic_cdk::update]
fn begin_auth_binding(subject: Hash, nonce: Hash, expires_at: u64) -> Result<()> {
    authenticated(caller())?;
    nonzero(&nonce)?;
    expiry(now(), expires_at, 5 * MINUTE)?;
    load(&subject)?;
    if let Some(id) = AUTH.with_borrow(|t| t.get::<Hash>(caller().as_slice())) {
        ensure(id == subject, Error::IdempotencyConflict)?;
    }
    ensure(
        BINDINGS.with_borrow(|t| t.contains(caller().as_slice()) || t.len() < 1024),
        Error::QuotaExceeded,
    )?;
    BINDINGS.with_borrow_mut(|t| t.put(caller().as_slice(), &(subject, nonce, expires_at)));
    Ok(())
}
#[ic_cdk::update]
fn prune_auth_bindings(after: ByteBuf) -> Option<ByteBuf> {
    let entries = BINDINGS.with_borrow(|t| t.page::<(Hash, Hash, u64)>(after.to_vec(), 64));
    let next = entries.last().map(|(key, _)| ByteBuf::from(key.clone()));
    for (key, (_, _, expires_at)) in entries {
        if now() >= expires_at {
            BINDINGS.with_borrow_mut(|t| t.remove(&key));
        }
    }
    next
}

#[ic_cdk::update]
fn mutate_account(input: AccountMutation) -> Result<OperationReceipt> {
    let mut s = load(&input.subject)?;
    let replay = s
        .operations
        .iter()
        .any(|r| r.id == input.approval.request_id);
    if let AccountCommand::BindAuth { principal, nonce } = &input.command {
        if !replay {
            let (subject, n, e) = BINDINGS
                .with_borrow(|t| t.get::<(Hash, Hash, u64)>(principal.as_slice()))
                .ok_or(Error::AuthRequired)?;
            ensure(
                subject == s.subject_id && n == *nonce && now() < e,
                Error::AuthRequired,
            )?;
            if let Some(id) = AUTH.with_borrow(|t| t.get::<Hash>(principal.as_slice())) {
                ensure(id == subject, Error::IdempotencyConflict)?;
            }
        }
    }
    let r = model::apply(
        &mut s,
        caller(),
        &input,
        now(),
        config().init.handle_canister,
    )?;
    if let AccountCommand::BindAuth { principal, .. } = input.command {
        AUTH.with_borrow_mut(|t| t.put(principal.as_slice(), &s.subject_id));
        BINDINGS.with_borrow_mut(|t| t.remove(principal.as_slice()));
    }
    save(&s);
    Ok(r)
}

#[ic_cdk::update]
fn consume_handle_authorization(intent: HandleIntent) -> Result<()> {
    ensure(
        caller() == config().init.handle_canister && intent.handle_canister == caller(),
        Error::Forbidden,
    )?;
    let mut s = load(&intent.subject)?;
    let a = s
        .handle_authorizations
        .get_mut(&intent.op_id)
        .ok_or(Error::NotFound)?;
    ensure(a.intent == intent, Error::IdempotencyConflict)?;
    ensure(now() < a.expires_at, Error::Expired)?;
    // Permission was linearized when the device authorized this exact intent.
    a.consumed = true;
    save(&s);
    Ok(())
}

#[ic_cdk::update]
async fn authorize_and_execute(input: ExecuteRequest) -> Result<ExecutionResult> {
    let mut s = load(&input.subject)?;
    let e = model::authorize(&mut s, caller(), &input, now())?;
    save(&s);
    if matches!(
        e.result.status,
        ExecutionStatus::Completed | ExecutionStatus::Failed | ExecutionStatus::ResultExpired
    ) {
        return Ok(e.result);
    }
    dispatch(e.grant).await
}
async fn dispatch(grant: ExecutionGrant) -> Result<ExecutionResult> {
    let registered: Result<()> =
        stable::call(grant.home_cose, "register_subject", (grant.subject,)).await?;
    registered?;
    let response: Result<ExecutionResult> =
        match stable::call(grant.home_cose, "execute", (grant.clone(),)).await {
            Ok(response) => response,
            Err(e) => Err(e),
        };
    let mut s = load(&grant.subject)?;
    let result = model::record_execution_response(&mut s, grant.request_id, response);
    save(&s);
    result
}
#[ic_cdk::update]
async fn reconcile_execution(subject: Hash, request_id: Hash) -> Result<ExecutionResult> {
    let mut s = own(&subject)?;
    let e = s
        .executions
        .get(&request_id)
        .ok_or(Error::NotFound)?
        .clone();
    let response: Result<ExecutionResult> =
        stable::call(s.home_cose, "get_execution", (subject, request_id)).await?;
    if response == Err(Error::NotFound) {
        return dispatch(e.grant).await;
    }
    s = load(&subject)?;
    let result = model::record_execution_response(&mut s, request_id, response);
    save(&s);
    result
}

#[ic_cdk::update]
fn request_recovery(
    subject: Hash,
    request: RecoveryRequest,
    signature: ByteBuf,
    device_proof: ByteBuf,
) -> Result<()> {
    ensure(caller() == request.new_auth, Error::AuthRequired)?;
    if let Some(id) = AUTH.with_borrow(|t| t.get::<Hash>(caller().as_slice())) {
        ensure(id == subject, Error::IdempotencyConflict)?;
    }
    let mut s = load(&subject)?;
    model::begin_recovery(&mut s, &request, &signature, &device_proof, now())?;
    save(&s);
    Ok(())
}
#[ic_cdk::update]
fn reconfirm_recovery(
    subject: Hash,
    confirmation: RecoveryConfirmation,
    signature: ByteBuf,
) -> Result<()> {
    let mut s = load(&subject)?;
    model::reconfirm_recovery(&mut s, &confirmation, &signature, now())?;
    save(&s);
    Ok(())
}
#[ic_cdk::update]
fn complete_recovery(subject: Hash) -> Result<()> {
    if let Some(id) = AUTH.with_borrow(|t| t.get::<Hash>(caller().as_slice())) {
        ensure(id == subject, Error::IdempotencyConflict)?;
    }
    let mut s = load(&subject)?;
    model::complete_recovery(&mut s, caller(), now())?;
    AUTH.with_borrow_mut(|t| t.put(caller().as_slice(), &subject));
    save(&s);
    Ok(())
}

#[ic_cdk::update]
fn verify_payment_offer(signed: SignedOffer) -> Result<u64> {
    let o = &signed.offer;
    ensure(
        caller() == config().init.payment_canister && o.home_payment == caller(),
        Error::Forbidden,
    )?;
    let s = load(&o.subject)?;
    let d = s
        .devices
        .get(&o.device_id)
        .ok_or(Error::DeviceNotApproved)?;
    ensure(
        s.status == AccountStatus::Active && !s.sensitive_policy.frozen,
        Error::Locked,
    )?;
    ensure(
        d.revoked_at.is_none() && d.input.capabilities.contains(&Capability::PaymentOffer),
        Error::Forbidden,
    )?;
    ensure(o.security_epoch == s.security_epoch, Error::PolicyStale)?;
    ensure(o.issued_at <= now() && now() < o.expires_at, Error::Expired)?;
    verify(
        &d.input.signing_pub,
        &digest("dmsg/payment-offer/v1", o),
        &signed.signature,
    )?;
    Ok(now())
}

#[ic_cdk::query]
fn get_subject(subject: Hash) -> Result<Subject> {
    own(&subject)
}
#[ic_cdk::query]
fn get_recovery_request(subject: Hash) -> Result<Option<PendingRecovery>> {
    model::recovery_request(&load(&subject)?, caller())
}
#[ic_cdk::query]
fn my_subject() -> Option<Hash> {
    AUTH.with_borrow(|t| t.get(caller().as_slice()))
}
#[ic_cdk::query]
fn get_root_ref(subject: Hash) -> Result<Option<ContentRootRef>> {
    Ok(own(&subject)?.current_root)
}
#[ic_cdk::query]
fn get_operation(subject: Hash, op_id: Hash) -> Result<OperationReceipt> {
    own(&subject)?
        .operations
        .into_iter()
        .find(|r| r.id == op_id)
        .ok_or(Error::ResultExpired)
}
#[ic_cdk::query]
fn security_snapshot_batch(subjects: Vec<Hash>) -> Result<CertifiedBatch> {
    CERT.with_borrow(|c| c.batch(me(), subjects.into_iter().map(|s| s.to_vec()).collect()))
}
#[ic_cdk::query]
fn get_device_bundle(
    subject: Hash,
) -> Result<(SecuritySnapshot, std::collections::BTreeMap<Hash, Device>)> {
    let s = load(&subject)?;
    Ok((s.snapshot(), s.devices))
}
#[ic_cdk::query]
fn get_execution(subject: Hash, request_id: Hash) -> Result<AuthorizedExecution> {
    own(&subject)?
        .executions
        .get(&request_id)
        .cloned()
        .ok_or(Error::ResultExpired)
}
ic_cdk::export_candid!();
