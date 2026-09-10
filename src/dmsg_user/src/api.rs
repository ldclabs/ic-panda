use crate::{account, execution, recovery, state::*, store::*, xid};
use candid::Principal;
use dmsg_protocol::*;
use dmsg_runtime::storage::{CompactStored, MapExt};
use dmsg_runtime::{self as stable};
use dmsg_types::{cose::*, handle::*, payment::SignedOffer, user::*, *};
use ic_auth_types::XidGenerator;
use serde_bytes::ByteBuf;

fn now() -> u64 {
    nanos_to_millis(ic_cdk::api::time())
}
fn caller() -> Principal {
    ic_cdk::api::msg_caller()
}
fn me() -> Principal {
    ic_cdk::api::canister_self()
}
fn own(id: &AccountId) -> Result<AccountState> {
    let s = load(id)?;
    ensure(s.auth_bindings.contains(&caller()), Error::AuthRequired)?;
    Ok(s)
}

#[ic_cdk::init]
fn init(args: UserInit) {
    validate_namespace(&args.issuer_namespace).expect("identity namespace");
    let allocator_namespace_digest =
        xid::namespace_digest(&args.environment, &args.issuer_namespace, me());
    let allocator = XidGenerator::new(
        allocator_namespace_digest[..5]
            .try_into()
            .expect("five bytes"),
    );
    for p in [args.home_cose, args.handle_canister, args.payment_canister] {
        authenticated(p).expect("configured canister");
    }
    assert!(
        args.max_accounts > 0
            && args.max_accounts <= 1_000_000
            && args.daily_new_accounts > 0
            && args.daily_new_accounts <= 10_000,
        "hard limits"
    );
    CONFIG.with_borrow_mut(|t| {
        t.set(CompactStored(Some(Config {
            schema: STABLE_SCHEMA,
            init: args,
            allocator,
            allocator_namespace_digest,
            day: 0,
            created_today: 0,
        })))
    });
    CERT.with_borrow(|c| c.publish());
}
#[ic_cdk::post_upgrade]
fn post_upgrade() {
    assert_eq!(
        config().schema,
        STABLE_SCHEMA,
        "explicit stable-state migration required"
    );
    let cfg = config();
    xid::validate(
        &cfg.allocator,
        cfg.allocator_namespace_digest,
        &cfg.init.environment,
        &cfg.init.issuer_namespace,
        me(),
    )
    .expect("immutable allocator");
    CERT.with_borrow(|c| c.publish());
    ACCOUNTS.with_borrow(|t| {
        t.for_each(|key, s| {
            CERT.with_borrow_mut(|c| c.put(key, &s.snapshot(&config().init.issuer_namespace)))
        })
    });
    EXECUTIONS.with_borrow(|t| t.for_each(|_, execution| certify_execution(&execution)));
}

#[ic_cdk::update]
fn create_account(input: CreateAccount) -> Result<AccountId> {
    let who = caller();
    authenticated(who)?;
    let mut cfg = config();
    if let Some(id) = AUTH.with_borrow(|t| t.load(who.as_slice())) {
        let account = load(&id)?;
        ensure(
            account.auth_bindings.contains(&who),
            Error::DeviceNotApproved,
        )?;
        return Ok(id);
    }
    let now = now();
    ensure(
        ACCOUNTS.with_borrow(|t| t.len()) < cfg.init.max_accounts,
        Error::QuotaExceeded,
    )?;
    if now / DAY > cfg.day {
        cfg.day = now / DAY;
        cfg.created_today = 0;
    }
    ensure(
        cfg.created_today < cfg.init.daily_new_accounts,
        Error::QuotaExceeded,
    )?;
    xid::validate(
        &cfg.allocator,
        cfg.allocator_namespace_digest,
        &cfg.init.environment,
        &cfg.init.issuer_namespace,
        me(),
    )?;
    let (id, allocator) = cfg
        .allocator
        .allocate(now / SECOND)
        .map_err(xid::allocation_error)?;
    ensure(
        !ACCOUNTS.with_borrow(|t| t.contains(id.as_slice())),
        Error::IdGeneratorStateConflict,
    )?;
    let account = account::create(me(), cfg.init.home_cose, id.clone(), who, &input, now)?;
    cfg.created_today = cfg
        .created_today
        .checked_add(1)
        .ok_or(Error::QuotaExceeded)?;
    cfg.allocator = allocator;
    // All fallible validation precedes these writes; one IC message commits all three.
    save(&account);
    AUTH.with_borrow_mut(|t| t.put(who.as_slice(), &id));
    CONFIG.with_borrow_mut(|t| t.set(CompactStored(Some(cfg))));
    Ok(id)
}

/// Only the new authentication principal may reserve its own binding. The
/// existing account_id's administrator must separately approve the exact nonce.
#[ic_cdk::update]
fn begin_auth_binding(account_id: AccountId, nonce: Hash, expires_at: u64) -> Result<()> {
    authenticated(caller())?;
    nonzero(nonce.as_slice())?;
    expiry(now(), expires_at, 5 * MINUTE)?;
    load(&account_id)?;
    if let Some(id) = AUTH.with_borrow(|t| t.load(caller().as_slice())) {
        ensure(id == account_id, Error::IdempotencyConflict)?;
    }
    ensure(
        BINDINGS.with_borrow(|t| t.contains(caller().as_slice()) || t.len() < 1024),
        Error::QuotaExceeded,
    )?;
    BINDINGS.with_borrow_mut(|t| t.put(caller().as_slice(), &(account_id, nonce, expires_at)));
    Ok(())
}
#[ic_cdk::update]
fn prune_auth_bindings(after: ByteBuf) -> Option<ByteBuf> {
    let entries = BINDINGS.with_borrow(|t| t.page(after.to_vec(), 64));
    let next = entries.last().map(|(key, _)| ByteBuf::from(key.clone()));
    for (key, (_, _, expires_at)) in entries {
        if now() >= expires_at {
            BINDINGS.with_borrow_mut(|t| t.delete(&key));
        }
    }
    next
}

#[ic_cdk::update]
fn mutate_account(input: AccountMutation) -> Result<OperationReceipt> {
    let mut s = load(&input.account_id)?;
    let replay = s
        .operations
        .iter()
        .any(|r| r.id == input.approval.request_id);
    if let AccountCommand::BindAuth { principal, nonce } = &input.command {
        if !replay {
            let (account_id, n, e) = BINDINGS
                .with_borrow(|t| t.load(principal.as_slice()))
                .ok_or(Error::AuthRequired)?;
            ensure(
                account_id == s.account_id && n == *nonce && now() < e,
                Error::AuthRequired,
            )?;
            if let Some(id) = AUTH.with_borrow(|t| t.load(principal.as_slice())) {
                ensure(id == account_id, Error::IdempotencyConflict)?;
            }
        }
    }
    let r = account::apply(
        &mut s,
        caller(),
        &input,
        now(),
        config().init.handle_canister,
    )?;
    if let AccountCommand::BindAuth { principal, .. } = input.command {
        AUTH.with_borrow_mut(|t| t.put(principal.as_slice(), &s.account_id));
        BINDINGS.with_borrow_mut(|t| t.delete(principal.as_slice()));
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
    let mut s = load(&intent.account_id)?;
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
async fn sign(input: SignRequest) -> Result<ExecutionResult> {
    authorize_and_execute(input.into_execution()?).await
}
#[ic_cdk::update]
async fn derive_root(input: DeriveRootRequest) -> Result<ExecutionResult> {
    authorize_and_execute(input.into_execution()).await
}
async fn authorize_and_execute(input: ExecuteRequest) -> Result<ExecutionResult> {
    let mut s = load(&input.account_id)?;
    let e = execution::authorize(
        &mut s,
        caller(),
        &input,
        now(),
        &config().init.issuer_namespace,
    )?;
    save(&s);
    if e.result.is_terminal() {
        return Ok(e.result);
    }
    dispatch(e.grant).await
}
async fn dispatch(grant: ExecutionGrant) -> Result<ExecutionResult> {
    let response: Result<ExecutionResult> =
        match stable::call(grant.home_cose, "execute", (grant.clone(),)).await {
            Ok(response) => response,
            Err(e) => Err(e),
        };
    let mut s = load(&grant.account_id)?;
    let result = execution::record_execution_response(&mut s, grant.request_id, response);
    save(&s);
    result
}
#[ic_cdk::update]
async fn reconcile_execution(account_id: AccountId, request_id: Hash) -> Result<ExecutionResult> {
    let mut s = own(&account_id)?;
    let e = s
        .executions
        .get(&request_id)
        .ok_or(Error::NotFound)?
        .clone();
    if e.result.is_terminal() {
        return Ok(e.result);
    }
    let response: Result<ExecutionResult> =
        match stable::call(s.home_cose, "get_execution", (&account_id, request_id)).await {
            Ok(response) => response,
            Err(error) => Err(error),
        };
    if response == Err(Error::NotFound) {
        return dispatch(e.grant).await;
    }
    s = load(&account_id)?;
    let result = execution::record_execution_response(&mut s, request_id, response);
    save(&s);
    result
}

#[ic_cdk::update]
fn request_recovery(
    account_id: AccountId,
    request: RecoveryRequest,
    signature: ByteBuf,
    device_proof: ByteBuf,
) -> Result<()> {
    ensure(caller() == request.new_auth, Error::AuthRequired)?;
    if let Some(id) = AUTH.with_borrow(|t| t.load(caller().as_slice())) {
        ensure(id == account_id, Error::IdempotencyConflict)?;
    }
    let mut s = load(&account_id)?;
    recovery::begin_recovery(&mut s, &request, &signature, &device_proof, now())?;
    save(&s);
    Ok(())
}
#[ic_cdk::update]
fn reconfirm_recovery(
    account_id: AccountId,
    confirmation: RecoveryConfirmation,
    signature: ByteBuf,
) -> Result<()> {
    let mut s = load(&account_id)?;
    recovery::reconfirm_recovery(&mut s, &confirmation, &signature, now())?;
    save(&s);
    Ok(())
}
#[ic_cdk::update]
fn complete_recovery(account_id: AccountId) -> Result<()> {
    if let Some(id) = AUTH.with_borrow(|t| t.load(caller().as_slice())) {
        ensure(id == account_id, Error::IdempotencyConflict)?;
    }
    let mut s = load(&account_id)?;
    recovery::complete_recovery(&mut s, caller(), now())?;
    AUTH.with_borrow_mut(|t| t.put(caller().as_slice(), &account_id));
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
    let s = load(&o.account_id)?;
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
        digest("dmsg/payment-offer/v1", o).as_slice(),
        &signed.signature,
    )?;
    Ok(now())
}

#[ic_cdk::query]
fn get_account(account_id: AccountId) -> Result<AccountInfo> {
    Ok(own(&account_id)?.info(&config().init.issuer_namespace))
}
#[ic_cdk::query]
fn get_recovery_request(account_id: AccountId) -> Result<Option<PendingRecovery>> {
    recovery::recovery_request(&load(&account_id)?, caller())
}
#[ic_cdk::query]
fn my_account() -> Option<AccountId> {
    AUTH.with_borrow(|t| t.load(caller().as_slice()))
}
#[ic_cdk::query]
fn get_root_ref(account_id: AccountId) -> Result<Option<ContentRootRef>> {
    Ok(own(&account_id)?.current_root)
}
#[ic_cdk::query]
fn get_operation(account_id: AccountId, op_id: Hash) -> Result<OperationReceipt> {
    own(&account_id)?
        .operations
        .into_iter()
        .find(|r| r.id == op_id)
        .ok_or(Error::ResultExpired)
}
#[ic_cdk::query]
fn security_snapshot_batch(accounts: Vec<AccountId>) -> Result<CertifiedBatch> {
    CERT.with_borrow(|c| c.batch(me(), accounts.into_iter().map(|s| s.to_vec()).collect()))
}
#[ic_cdk::query]
fn get_device_bundle(
    account_id: AccountId,
) -> Result<(SecuritySnapshot, std::collections::BTreeMap<Hash, Device>)> {
    let s = load(&account_id)?;
    Ok((s.snapshot(&config().init.issuer_namespace), s.devices))
}
#[ic_cdk::query]
fn get_execution(account_id: AccountId, request_id: Hash) -> Result<ExecutionResult> {
    own(&account_id)?
        .executions
        .get(&request_id)
        .map(|e| e.result.clone())
        .ok_or(Error::ResultExpired)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[ic_cdk::query]
fn get_execution_receipt(account_id: AccountId, request_id: OpId) -> Result<CertifiedBatch> {
    let account = own(&account_id)?;
    let execution = account
        .executions
        .get(&request_id)
        .ok_or(Error::ResultExpired)?;
    crate::execution::receipt(execution, &config().init.issuer_namespace)?;
    CERT.with_borrow(|c| c.batch(me(), vec![execution_receipt_key(&account_id, request_id)]))
}
