use crate::{account, execution, recovery, state::*, store::*, xid};
use candid::Principal;
use dmsg_protocol::*;
use dmsg_runtime::storage::{CompactStored, MapExt};
use dmsg_runtime::{self as stable};
use dmsg_types::{billing::*, cose::*, handle::*, membership::*, payment::SignedOffer, user::*, *};
use ic_auth_types::XidGenerator;
use serde_bytes::ByteBuf;

fn now() -> u64 {
    nanos_to_millis(ic_cdk::api::time())
}

fn own(id: &AccountId, caller: Principal) -> Result<AccountState> {
    let s = load(id)?;
    ensure(s.auth_bindings.contains(&caller), Error::AuthRequired)?;
    Ok(s)
}

#[ic_cdk::init]
fn init(args: UserInit) {
    validate_namespace(&args.issuer_namespace).expect("identity namespace");
    let allocator_namespace_digest = xid::namespace_digest(
        &args.environment,
        &args.issuer_namespace,
        ic_cdk::api::canister_self(),
    );
    let allocator = XidGenerator::new(
        allocator_namespace_digest[..5]
            .try_into()
            .expect("five bytes"),
    );
    for p in [
        args.home_cose,
        args.handle_canister,
        args.payment_canister,
        args.commerce_canister,
        args.membership_canister,
    ] {
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
        t.set(CompactStored::new(&Some(Config {
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
    let cfg = config();
    assert_eq!(
        cfg.schema, STABLE_SCHEMA,
        "explicit stable-state migration required"
    );
    xid::validate(
        &cfg.allocator,
        cfg.allocator_namespace_digest,
        &cfg.init.environment,
        &cfg.init.issuer_namespace,
        ic_cdk::api::canister_self(),
    )
    .expect("immutable allocator");
    rebuild_certification();
}

#[ic_cdk::update]
fn create_account(input: CreateAccount) -> Result<AccountId> {
    let canister_id = ic_cdk::api::canister_self();
    let who = ic_cdk::api::msg_caller();
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
        canister_id,
    )?;
    let (id, allocator) = cfg
        .allocator
        .allocate(now / SECOND)
        .map_err(xid::allocation_error)?;
    ensure(
        !ACCOUNTS.with_borrow(|t| t.contains(id.as_slice())),
        Error::IdGeneratorStateConflict,
    )?;
    let account = account::create(
        canister_id,
        cfg.init.home_cose,
        id.clone(),
        who,
        &input,
        now,
    )?;
    cfg.created_today = cfg
        .created_today
        .checked_add(1)
        .ok_or(Error::QuotaExceeded)?;
    cfg.allocator = allocator;
    // All fallible validation precedes these writes; one IC message commits all three.
    save(&account);
    AUTH.with_borrow_mut(|t| t.put(who.as_slice(), &id));
    CONFIG.with_borrow_mut(|t| t.set(CompactStored::new(&Some(cfg))));
    Ok(id)
}

/// Only the new authentication principal may reserve its own binding. The
/// existing account_id's administrator must separately approve the exact nonce.
#[ic_cdk::update]
fn begin_auth_binding(account_id: AccountId, nonce: Hash, expires_at: u64) -> Result<()> {
    let caller = ic_cdk::api::msg_caller();
    authenticated(caller)?;
    nonzero(nonce.as_slice())?;
    expiry(now(), expires_at, 5 * MINUTE)?;
    ensure(
        ACCOUNTS.with_borrow(|t| t.contains(account_id.as_slice())),
        Error::NotFound,
    )?;
    if let Some(id) = AUTH.with_borrow(|t| t.load(caller.as_slice())) {
        ensure(id == account_id, Error::IdempotencyConflict)?;
    }
    ensure(
        BINDINGS.with_borrow(|t| t.contains(caller.as_slice()) || t.len() < 1024),
        Error::QuotaExceeded,
    )?;
    BINDINGS.with_borrow_mut(|t| t.put(caller.as_slice(), &(account_id, nonce, expires_at)));
    Ok(())
}

#[ic_cdk::update]
fn prune_auth_bindings(after: ByteBuf) -> Option<ByteBuf> {
    let now = now();
    let entries = BINDINGS.with_borrow(|t| t.page(after.to_vec(), 64));
    let next = entries.last().map(|(key, _)| ByteBuf::from(key.clone()));
    for (key, (_, _, expires_at)) in entries {
        if now >= expires_at {
            BINDINGS.with_borrow_mut(|t| t.delete(&key));
        }
    }
    next
}

#[ic_cdk::update]
fn mutate_account(input: AccountMutation) -> Result<OperationReceipt> {
    let at = now();
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
                account_id == s.account_id && n == *nonce && at < e,
                Error::AuthRequired,
            )?;
            if let Some(id) = AUTH.with_borrow(|t| t.load(principal.as_slice())) {
                ensure(id == account_id, Error::IdempotencyConflict)?;
            }
        }
    }
    if let AccountCommand::AuthorizeMembership { intent } = &input.command {
        let c = config().init;
        ensure(
            intent.environment == c.environment
                && [c.commerce_canister, c.membership_canister].contains(&intent.service_canister),
            Error::Forbidden,
        )?;
    }
    let r = account::apply(
        &mut s,
        ic_cdk::api::msg_caller(),
        &input,
        at,
        config().init.handle_canister,
    )?;
    if replay {
        return Ok(r);
    }
    if let AccountCommand::BindAuth { principal, .. } = input.command {
        AUTH.with_borrow_mut(|t| t.put(principal.as_slice(), &s.account_id));
        BINDINGS.with_borrow_mut(|t| t.delete(principal.as_slice()));
    }
    save(&s);
    Ok(r)
}

#[ic_cdk::update]
fn consume_handle_authorization(intent: HandleIntent) -> Result<()> {
    let caller = ic_cdk::api::msg_caller();
    ensure(caller == config().init.handle_canister, Error::Forbidden)?;
    check_handle_authorization(&intent, caller, now())
}

#[ic_cdk::update]
fn consume_handle_transfer_authorizations(from: HandleIntent, accept: HandleIntent) -> Result<()> {
    let caller = ic_cdk::api::msg_caller();
    ensure(caller == config().init.handle_canister, Error::Forbidden)?;
    let at = now();
    check_handle_authorization(&from, caller, at)?;
    check_handle_authorization(&accept, caller, at)
}

fn check_handle_authorization(intent: &HandleIntent, caller: Principal, at: u64) -> Result<()> {
    ensure(intent.handle_canister == caller, Error::Forbidden)?;
    let s = load(&intent.account_id)?;
    let a = s
        .handle_authorizations
        .get(&intent.op_id)
        .ok_or(Error::NotFound)?;
    ensure(&a.intent == intent, Error::IdempotencyConflict)?;
    ensure(at < a.expires_at, Error::Expired)?;
    // Permission was linearized when the device authorized this exact intent.
    // The handle canister deduplicates the operation; revalidation is read-only.
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
    let caller = ic_cdk::api::msg_caller();
    let mut at = now();
    let mut s = own(&input.account_id, caller)?;
    let mut previous = load_execution(&input.account_id, &input.approval.request_id);
    if matches!(input.kind, ExecutionKind::Sign { .. })
        && previous.is_none()
        && !crate::commerce::is_current(&input.account_id, at)?
    {
        // Reject invalid caller/device/payload before doing commercial cross-canister work.
        execution::authorize(
            &mut s,
            caller,
            &input,
            at,
            &config().init.issuer_namespace,
            None,
        )?;
        at = crate::commerce::refresh(&input.account_id, at).await?;
        s = own(&input.account_id, caller)?;
        previous = load_execution(&input.account_id, &input.approval.request_id);
    }
    let expired: Vec<_> = s
        .execution_expirations
        .iter()
        .filter_map(|(id, expires_at)| expires_at.filter(|deadline| *deadline <= at).map(|_| *id))
        .collect();
    let mut e = execution::authorize(
        &mut s,
        caller,
        &input,
        at,
        &config().init.issuer_namespace,
        previous.as_ref(),
    )?;
    if previous.is_none() {
        crate::commerce::reserve(&mut e, at)?;
        // All validation precedes these writes, with no await until the account,
        // budget, sequence, execution and certification have committed together.
        for id in expired {
            remove_execution(&s.account_id, &id);
        }
        save_execution(&e);
        save(&s);
    }
    if e.result.is_terminal() {
        return Ok(e.result);
    }
    dispatch(e.grant).await
}

async fn dispatch(grant: ExecutionGrant) -> Result<ExecutionResult> {
    let account_id = grant.account_id.clone();
    let request_id = grant.request_id;
    let response: Result<ExecutionResult> = match stable::call_classified::<
        _,
        Result<ExecutionResult>,
    >(grant.home_cose, "execute", (grant,))
    .await
    {
        Ok(Ok(response)) => Ok(response),
        // A pruned result is historical execution evidence, not nonexecution.
        Ok(Err(Error::ResultExpired)) => Err(Error::ResultExpired),
        // An application error need not have consumed the COSE sequence. Keep
        // the original grant and hold until that service records a terminal result.
        Ok(Err(error)) => {
            return rejected_dispatch(&account_id, &request_id, error);
        }
        Err(stable::CallFailure::NotExecuted) => {
            // Preserve the current state: another dispatch may have completed
            // while this attempt was in flight. An unsent Authorized grant keeps
            // its original sequence/reservation so retry can close the COSE slot.
            return rejected_dispatch(
                &account_id,
                &request_id,
                stable::CallFailure::NotExecuted.into(),
            );
        }
        Err(stable::CallFailure::Unknown) => Err(Error::ExecutionUnknown),
    };
    record_response(&account_id, request_id, response)
}

fn rejected_dispatch(
    account_id: &AccountId,
    request_id: &OpId,
    error: Error,
) -> Result<ExecutionResult> {
    let current = load_execution(account_id, request_id).ok_or(Error::ResultExpired)?;
    execution::rejected_dispatch_result(current.result, error)
}

fn record_response(
    account_id: &AccountId,
    request_id: OpId,
    response: Result<ExecutionResult>,
) -> Result<ExecutionResult> {
    // A callback must read the latest record: a concurrent callback may already
    // have completed it, or a later authorization may have evicted it.
    let mut e = load_execution(account_id, &request_id).ok_or(Error::ResultExpired)?;
    if e.result.is_terminal() {
        return Ok(e.result);
    }
    let previous = e.result.clone();
    let result = execution::record_execution_response(&mut e, response);
    if result != previous {
        if result.is_terminal() {
            let mut s = load(account_id)?;
            crate::commerce::settle(&e)?;
            s.execution_expirations
                .insert(request_id, Some(e.grant.expires_at.saturating_add(DAY)));
            // Only the internal retention index changed, not the security leaf.
            save_account(&s);
        }
        save_execution(&e);
        publish();
    }
    Ok(result)
}

#[ic_cdk::update]
async fn reconcile_execution(account_id: AccountId, request_id: Hash) -> Result<ExecutionResult> {
    let home_cose = own(&account_id, ic_cdk::api::msg_caller())?.home_cose;
    let e = load_execution(&account_id, &request_id).ok_or(Error::NotFound)?;
    if e.result.is_terminal() {
        return Ok(e.result);
    }
    drop(e);
    let response: Result<ExecutionResult> =
        match stable::call(home_cose, "get_execution", (&account_id, request_id)).await {
            Ok(response) => response,
            Err(error) => Err(error),
        };
    if response == Err(Error::NotFound) {
        let e = load_execution(&account_id, &request_id).ok_or(Error::ResultExpired)?;
        if e.result.is_terminal() {
            return Ok(e.result);
        }
        return dispatch(e.grant).await;
    }
    record_response(&account_id, request_id, response)
}

#[ic_cdk::update]
fn request_recovery(
    account_id: AccountId,
    request: RecoveryRequest,
    signature: ByteBuf,
    device_proof: ByteBuf,
) -> Result<()> {
    let caller = ic_cdk::api::msg_caller();
    ensure(caller == request.new_auth, Error::AuthRequired)?;
    if let Some(id) = AUTH.with_borrow(|t| t.load(caller.as_slice())) {
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
    let caller = ic_cdk::api::msg_caller();
    if let Some(id) = AUTH.with_borrow(|t| t.load(caller.as_slice())) {
        ensure(id == account_id, Error::IdempotencyConflict)?;
    }
    let mut s = load(&account_id)?;
    recovery::complete_recovery(&mut s, caller, now())?;
    AUTH.with_borrow_mut(|t| t.put(caller.as_slice(), &account_id));
    save(&s);
    Ok(())
}

#[ic_cdk::update]
fn verify_payment_offer(signed: SignedOffer) -> Result<u64> {
    let at = now();
    let caller = ic_cdk::api::msg_caller();
    let o = &signed.offer;
    ensure(
        caller == config().init.payment_canister && o.home_payment == caller,
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
    ensure(o.issued_at <= at && at < o.expires_at, Error::Expired)?;
    verify(
        &d.input.signing_pub,
        digest("dmsg/payment-offer/v1", o).as_slice(),
        &signed.signature,
    )?;
    Ok(at)
}

#[ic_cdk::query]
fn get_account(account_id: AccountId) -> Result<AccountInfo> {
    Ok(own(&account_id, ic_cdk::api::msg_caller())?.info(&config().init.issuer_namespace))
}

#[ic_cdk::query]
fn get_recovery_request(account_id: AccountId) -> Result<Option<PendingRecovery>> {
    recovery::recovery_request(&load(&account_id)?, ic_cdk::api::msg_caller())
}

#[ic_cdk::query]
fn my_account() -> Option<AccountId> {
    AUTH.with_borrow(|t| t.load(ic_cdk::api::msg_caller().as_slice()))
}

#[ic_cdk::query]
fn get_root_ref(account_id: AccountId) -> Result<Option<ContentRootRef>> {
    Ok(own(&account_id, ic_cdk::api::msg_caller())?.current_root)
}

#[ic_cdk::query]
fn get_operation(account_id: AccountId, op_id: Hash) -> Result<OperationReceipt> {
    own(&account_id, ic_cdk::api::msg_caller())?
        .operations
        .into_iter()
        .find(|r| r.id == op_id)
        .ok_or(Error::ResultExpired)
}

#[ic_cdk::query]
fn security_snapshot_batch(accounts: Vec<AccountId>) -> Result<CertifiedBatch> {
    CERT.with_borrow(|c| {
        c.batch(
            ic_cdk::api::canister_self(),
            accounts.into_iter().map(|s| s.to_vec()).collect(),
        )
    })
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
    own(&account_id, ic_cdk::api::msg_caller())?;
    load_execution(&account_id, &request_id)
        .map(|e| e.result)
        .ok_or(Error::ResultExpired)
}

#[ic_cdk::query]
fn get_execution_receipt(account_id: AccountId, request_id: OpId) -> Result<CertifiedBatch> {
    own(&account_id, ic_cdk::api::msg_caller())?;
    let execution = load_execution(&account_id, &request_id).ok_or(Error::ResultExpired)?;
    ensure(
        matches!(execution.grant.kind, ExecutionKind::Sign { .. }),
        Error::UnsupportedProtocol,
    )?;
    CERT.with_borrow(|c| {
        c.batch(
            ic_cdk::api::canister_self(),
            vec![execution_receipt_key(&account_id, request_id)],
        )
    })
}

#[ic_cdk::update]
fn verify_membership_authorization(intent: MembershipIntent) -> Result<MembershipAuthorization> {
    let at = now();
    let cfg = config().init;
    ensure(
        [cfg.commerce_canister, cfg.membership_canister].contains(&ic_cdk::api::msg_caller())
            && [cfg.commerce_canister, cfg.membership_canister].contains(&intent.service_canister)
            && intent.environment == cfg.environment,
        Error::Forbidden,
    )?;
    let id = dmsg_protocol::billing::beneficiary_account(&intent.beneficiary)?;
    let s = load(&id)?;
    ensure(
        intent.beneficiary.authority_canister == ic_cdk::api::canister_self()
            && s.status == AccountStatus::Active,
        Error::Forbidden,
    )?;
    let a = s
        .membership_authorizations
        .get(&intent.application_id)
        .ok_or(Error::NotFound)?;
    ensure(
        a.intent == intent && a.security_epoch == s.security_epoch,
        Error::PolicyStale,
    )?;
    ensure(
        s.devices
            .get(&a.device_id)
            .is_some_and(|d| d.revoked_at.is_none()),
        Error::DeviceNotApproved,
    )?;
    ensure(
        at < a.expires_at && at < intent.valid_until_ms,
        Error::Expired,
    )?;
    Ok(MembershipAuthorization {
        intent_digest: dmsg_protocol::membership::membership_intent_digest(&intent),
        security_epoch: s.security_epoch,
        verified_at_ms: at,
        valid_until_ms: a.expires_at.min(intent.valid_until_ms),
    })
}

#[ic_cdk::query]
fn get_execution_usage(account_id: AccountId, month_utc: u32) -> Result<ExecutionUsage> {
    own(&account_id, ic_cdk::api::msg_caller())?;
    crate::commerce::usage(&account_id, month_utc)
}

#[ic_cdk::query]
fn get_execution_usage_certified(account_id: AccountId, month_utc: u32) -> Result<CertifiedBatch> {
    own(&account_id, ic_cdk::api::msg_caller())?;
    CERT.with_borrow(|c| {
        c.batch(
            ic_cdk::api::canister_self(),
            vec![dmsg_protocol::billing::usage_key(&account_id, month_utc).to_vec()],
        )
    })
}

#[ic_cdk::update]
async fn refresh_execution_entitlement(account_id: AccountId) -> Result<ExecutionUsage> {
    let caller = ic_cdk::api::msg_caller();
    own(&account_id, caller)?;
    let at = crate::commerce::refresh(&account_id, now()).await?;
    own(&account_id, caller)?;
    crate::commerce::usage(&account_id, dmsg_protocol::billing::month_utc(at)?)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
