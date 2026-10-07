use crate::{account, execution, principal, recovery, state::*, store::*, xid};
use candid::Principal;
use dmsg_protocol::*;
use dmsg_runtime::admin::{self, validation, Validation};
use dmsg_runtime::storage::{CompactStored, MapExt};
use dmsg_runtime::{self as stable};
use dmsg_types::{agent::*, billing::*, cose::*, handle::*, payment::SignedOffer, user::*, *};
use ic_auth_types::XidGenerator;
use serde_bytes::ByteBuf;
use std::time::Duration;

fn now() -> u64 {
    nanos_to_millis(ic_cdk::api::time())
}

fn own(id: &AccountId, caller: Principal) -> Result<AccountState> {
    let s = load(id)?;
    ensure(s.auth_bindings.contains(&caller), Error::AuthRequired)?;
    Ok(s)
}

/// Generate the master secret behind device unlock secrets once, in a timer
/// after installation. Upgrades keep it; a failed draw retries.
fn ensure_master_secret() {
    if config().master_secret.is_some() {
        return;
    }
    ic_cdk_timers::set_timer(Duration::ZERO, draw_master_secret());
}

async fn draw_master_secret() {
    let drawn = ic_cdk_management_canister::raw_rand().await;
    let mut cfg = config();
    if cfg.master_secret.is_some() {
        return;
    }
    match drawn.ok().and_then(|bytes| <[u8; 32]>::try_from(bytes).ok()) {
        Some(bytes) => {
            cfg.master_secret = Some(Hash::new(bytes));
            save_config(&cfg);
        }
        None => {
            ic_cdk_timers::set_timer(Duration::from_secs(10), draw_master_secret());
        }
    }
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
        args.directory_canister,
        args.governance,
    ] {
        authenticated(p).expect("configured canister");
    }
    dmsg_protocol::agent::validate_principal_origin(&args.principal_origin)
        .expect("principal origin");
    check_limits(args.max_accounts, args.daily_new_accounts).expect("hard limits");
    CONFIG.with_borrow_mut(|t| {
        t.set(CompactStored::new(&Some(Config {
            schema: STABLE_SCHEMA,
            init: args,
            allocator,
            allocator_namespace_digest,
            day: 0,
            created_today: 0,
            master_secret: None,
        })))
    });
    CERT.with_borrow(|c| c.publish());
    ensure_master_secret();
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
    // Certification nodes persist in stable memory; only the root is republished.
    CERT.with_borrow(|c| c.publish());
    ensure_master_secret();
    #[cfg(target_arch = "wasm32")]
    ic_cdk::println!(
        "dmsg_user upgrade: instructions={} wasm_memory_bytes={}",
        ic_cdk::api::performance_counter(0),
        core::arch::wasm32::memory_size(0) * 65_536,
    );
}

fn check_limits(max_accounts: u64, daily_new_accounts: u32) -> Result<()> {
    ensure_valid(
        (1..=1_000_000).contains(&max_accounts) && (1..=10_000).contains(&daily_new_accounts),
        "account limits",
    )
}

/// Set the account capacity and the daily new-account quota. A capacity below
/// the current count only stops new accounts.
#[ic_cdk::update]
fn admin_set_account_limits(max_accounts: u64, daily_new_accounts: u32) -> Result<()> {
    let mut cfg = config();
    admin::check_admin(ic_cdk::api::msg_caller(), cfg.init.governance)?;
    check_limits(max_accounts, daily_new_accounts)?;
    cfg.init.max_accounts = max_accounts;
    cfg.init.daily_new_accounts = daily_new_accounts;
    save_config(&cfg);
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_set_account_limits(max_accounts: u64, daily_new_accounts: u32) -> Validation {
    let init = config().init;
    validation(check_limits(max_accounts, daily_new_accounts).map(|()| {
        format!(
            "Set account limits to {max_accounts} accounts and {daily_new_accounts} new accounts per day (currently {} and {}; existing accounts: {}).{}",
            init.max_accounts,
            init.daily_new_accounts,
            ACCOUNTS.with_borrow(|t| t.len()),
            admin::unchanged(
                (init.max_accounts, init.daily_new_accounts) != (max_accounts, daily_new_accounts),
                "Same limits",
            ),
        )
    }))
}

/// Account count, capacity, today's admissions, unlock readiness, stable
/// pages and the cycle balance.
#[ic_cdk::query]
fn user_stats() -> UserStats {
    let cfg = config();
    let day = now() / DAY;
    UserStats {
        accounts: ACCOUNTS.with_borrow(|t| t.len()),
        max_accounts: cfg.init.max_accounts,
        day,
        created_today: if cfg.day == day { cfg.created_today } else { 0 },
        daily_new_accounts: cfg.init.daily_new_accounts,
        unlock_ready: cfg.master_secret.is_some(),
        stable_pages: ic_cdk::api::stable_size(),
        cycles: ic_cdk::api::canister_cycle_balance(),
    }
}

#[ic_cdk::update]
fn create_account(input: CreateAccount) -> Result<AccountId> {
    let canister_id = ic_cdk::api::canister_self();
    let who = ic_cdk::api::msg_caller();
    authenticated(who)?;
    let mut cfg = config();
    // RemoveAuth and completed recovery delete their routes, so an existing
    // route is a current binding.
    if let Some(id) = AUTH.with_borrow(|t| t.load(who.as_slice())) {
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
    let account = account::create(canister_id, cfg.init.home_cose, id, who, &input, now)?;
    cfg.created_today = cfg
        .created_today
        .checked_add(1)
        .ok_or(Error::QuotaExceeded)?;
    cfg.allocator = allocator;
    // All fallible validation precedes these writes; one IC message commits all three.
    save(&account);
    AUTH.with_borrow_mut(|t| t.put(who.as_slice(), &id));
    save_config(&cfg);
    Ok(id)
}

/// The secret a bound login fetches to unlock one device's local store. It is
/// refused for revoked devices, so revocation also locks the device's copy.
#[ic_cdk::query]
fn unlock_secret(account_id: AccountId, device_id: Hash) -> Result<Hash> {
    let s = own(&account_id, ic_cdk::api::msg_caller())?;
    let device = s.devices.get(&device_id).ok_or(Error::DeviceNotApproved)?;
    ensure(device.revoked_at.is_none(), Error::DeviceNotApproved)?;
    crate::store::unlock_secret(&config(), &account_id, &device_id)
}

/// Only the new authentication principal may reserve its own binding. The
/// existing account_id's administrator must separately approve the exact nonce.
#[ic_cdk::update]
fn begin_auth_binding(account_id: AccountId, nonce: Hash, expires_at: u64) -> Result<()> {
    let caller = ic_cdk::api::msg_caller();
    authenticated(caller)?;
    nonzero(nonce.as_slice())?;
    let at = now();
    expiry(at, expires_at, 5 * MINUTE)?;
    ensure(
        ACCOUNTS.with_borrow(|t| t.contains(account_id.as_slice())),
        Error::NotFound,
    )?;
    if let Some(id) = AUTH.with_borrow(|t| t.load(caller.as_slice())) {
        ensure(id == account_id, Error::IdempotencyConflict)?;
    }
    BINDINGS.with_borrow_mut(|t| {
        if !t.contains(caller.as_slice()) && t.len() >= 1024 {
            // The whole table is bounded at 1024 small entries. Reclaim on
            // admission instead of depending on an external cleanup schedule.
            let expired: Vec<_> = t
                .iter()
                .filter_map(|e| (at >= e.value().0 .2).then(|| e.key().clone()))
                .collect();
            for key in expired {
                t.delete(&key);
            }
        }
        ensure(
            t.contains(caller.as_slice()) || t.len() < 1024,
            Error::QuotaExceeded,
        )
    })?;
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
async fn mutate_account(input: AccountMutation) -> Result<OperationReceipt> {
    if principal::is_command(&input.command) {
        let receipt = commit_principal(&input, ic_cdk::api::msg_caller(), now())?;
        // A failed or unknown publication is retried by publish_principal.
        let _ = principal::publish(input.account_id).await;
        return Ok(receipt);
    }
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
    match input.command {
        AccountCommand::BindAuth { principal, .. } => {
            AUTH.with_borrow_mut(|t| t.put(principal.as_slice(), &s.account_id));
            BINDINGS.with_borrow_mut(|t| t.delete(principal.as_slice()));
        }
        AccountCommand::RemoveAuth { principal } => {
            AUTH.with_borrow_mut(|t| t.delete(principal.as_slice()));
        }
        _ => {}
    }
    save(&s);
    Ok(r)
}

/// Commit one principal command and its account receipt in this message.
fn commit_principal(
    input: &AccountMutation,
    caller: Principal,
    at: u64,
) -> Result<OperationReceipt> {
    let mut s = load(&input.account_id)?;
    let mut p = principal::load(&input.account_id);
    let replay = s
        .operations
        .iter()
        .any(|r| r.id == input.approval.request_id);
    let receipt = principal::apply(&mut s, &mut p, caller, input, at)?;
    if !replay {
        principal::save(&input.account_id, p.as_ref().expect("applied principal"));
        save(&s);
    }
    Ok(receipt)
}

/// Push the account's current principal state to the directory. Anyone may
/// retry a publication; it is idempotent and never rolls the document back.
#[ic_cdk::update]
async fn publish_principal(account_id: AccountId) -> Result<u64> {
    principal::publish(account_id).await
}

/// Public principal state and publication progress.
#[ic_cdk::query]
fn get_principal(account_id: AccountId) -> Result<PrincipalInfo> {
    let p = principal::load(&account_id).ok_or(Error::NotFound)?;
    Ok(principal::info(&account_id, p))
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

/// Record a device-signed document statement and certify its receipt.
/// Application actions use `attest_app_action`.
#[ic_cdk::update]
async fn attest(input: AttestRequest) -> Result<SignedArtifact> {
    ensure(
        !matches!(input.statement.content, StatementContent::AppAction(_)),
        Error::UnsupportedProtocol,
    )?;
    attest_statement(
        input.account_id,
        input.statement,
        input.origin,
        input.signature,
        input.approval,
        None,
    )
    .await
}

#[ic_cdk::update]
async fn inspect_app_action(
    account_id: AccountId,
    action: dmsg_types::app_action::AppAction,
) -> Result<()> {
    let caller = ic_cdk::api::msg_caller();
    let account = own(&account_id, caller)?;
    ensure(!account.sensitive_policy.frozen, Error::Locked)?;
    crate::external::authorize_action(&account_id, &action).await?;
    let account = own(&account_id, caller)?;
    ensure(!account.sensitive_policy.frozen, Error::Locked)
}

/// Record a device-signed application action after the registered product
/// authority confirms it, and certify its receipt.
#[ic_cdk::update]
async fn attest_app_action(input: AppActionAttestRequest) -> Result<SignedArtifact> {
    let statement = app_action_statement(&input)?;
    attest_statement(
        input.account_id,
        statement,
        input.action.origin.clone(),
        input.signature,
        input.approval,
        Some(input.action),
    )
    .await
}

fn existing_artifact(e: AuthorizedExecution, fingerprint: Hash) -> Result<SignedArtifact> {
    ensure(e.command_digest == fingerprint, Error::IdempotencyConflict)?;
    match e.record {
        ExecutionRecord::Attestation(a) => Ok(a.artifact),
        ExecutionRecord::Derivation { .. } => Err(Error::IdempotencyConflict),
    }
}

async fn attest_statement(
    account_id: AccountId,
    statement: Statement,
    origin: String,
    signature: Ed25519Signature,
    approval: Approval,
    action: Option<dmsg_types::app_action::AppAction>,
) -> Result<SignedArtifact> {
    let caller = ic_cdk::api::msg_caller();
    let fingerprint = digest(
        "dmsg/attest-request/v1",
        &(&account_id, &statement, &origin, &signature, &approval),
    );
    let mut at = now();
    let mut s = own(&account_id, caller)?;
    if let Some(e) = load_execution(&account_id, &approval.request_id) {
        return existing_artifact(e, fingerprint);
    }
    // Each check rereads the configuration, as it does the account.
    let check = |s: &AccountState, at: u64| {
        execution::check_attestation(
            s,
            caller,
            &account_id,
            &statement,
            &origin,
            &signature,
            &approval,
            fingerprint,
            at,
            &config().init,
        )
    };
    let mut checked = check(&s, at)?;
    if !crate::commerce::is_current(&account_id, at)? {
        at = crate::commerce::refresh(&account_id, at).await?;
        s = own(&account_id, caller)?;
        if let Some(e) = load_execution(&account_id, &approval.request_id) {
            return existing_artifact(e, fingerprint);
        }
        checked = check(&s, at)?;
    }
    if let Some(action) = &action {
        crate::external::authorize_action(&account_id, action).await?;
        at = now();
        s = own(&account_id, caller)?;
        if let Some(e) = load_execution(&account_id, &approval.request_id) {
            return existing_artifact(e, fingerprint);
        }
        checked = check(&s, at)?;
    }
    let e = execution::commit_attestation(
        &mut s,
        &approval,
        origin,
        &signature,
        checked,
        fingerprint,
        at,
    )?;
    // All validation precedes writes: the month charge, the record, its
    // certified receipt and the account commit together in this message.
    crate::commerce::charge(&account_id, at)?;
    prune_account_executions(&mut s, at);
    save_execution(&e);
    save(&s);
    existing_artifact(e, fingerprint)
}

/// A recovered device's vetKD derivation of the committed root; see
/// `DeriveRootRequest`. Retries with the same request return the stored result;
/// a new approval derives again until the device commits a new root.
#[ic_cdk::update]
async fn derive_root(input: DeriveRootRequest) -> Result<ExecutionResult> {
    let caller = ic_cdk::api::msg_caller();
    let at = now();
    let mut s = own(&input.account_id, caller)?;
    let fingerprint = digest("dmsg/derive-request/v1", &input);
    if let Some(e) = load_execution(&input.account_id, &input.approval.request_id) {
        return execute_existing(e, fingerprint).await;
    }
    let budget = execution::check_derivation(&s, caller, &input, fingerprint, at)?;
    let e = execution::commit_derivation(&mut s, input, fingerprint, budget, at);
    // All validation precedes writes, with no await until budget, sequence,
    // execution and certification have committed together.
    prune_account_executions(&mut s, at);
    save_execution(&e);
    save(&s);
    drop(s);
    let ExecutionRecord::Derivation { grant, .. } = e.record else {
        unreachable!("derivation record")
    };
    dispatch(grant).await
}

async fn execute_existing(e: AuthorizedExecution, fingerprint: Hash) -> Result<ExecutionResult> {
    ensure(e.command_digest == fingerprint, Error::IdempotencyConflict)?;
    match e.record {
        ExecutionRecord::Derivation { grant, result } => {
            if result.is_terminal() {
                return Ok(result);
            }
            dispatch(grant).await
        }
        ExecutionRecord::Attestation(_) => Err(Error::IdempotencyConflict),
    }
}

async fn dispatch(grant: ExecutionGrant) -> Result<ExecutionResult> {
    let account_id = grant.account_id;
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
            // its original sequence so retry can close the COSE slot.
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
    let ExecutionRecord::Derivation { result, .. } = current.record else {
        return Err(Error::IdempotencyConflict);
    };
    execution::rejected_dispatch_result(result, error)
}

fn record_response(
    account_id: &AccountId,
    request_id: OpId,
    response: Result<ExecutionResult>,
) -> Result<ExecutionResult> {
    // A callback must read the latest record: a concurrent callback may already
    // have completed it, or a later authorization may have evicted it.
    let mut e = load_execution(account_id, &request_id).ok_or(Error::ResultExpired)?;
    let ExecutionRecord::Derivation { result, .. } = &e.record else {
        return Err(Error::IdempotencyConflict);
    };
    if result.is_terminal() {
        return Ok(result.clone());
    }
    let previous = result.clone();
    let result = execution::record_execution_response(&mut e, response);
    if result != previous {
        if result.is_terminal() {
            let ExecutionRecord::Derivation { grant, .. } = &e.record else {
                unreachable!("derivation record")
            };
            let mut s = load(account_id)?;
            execution::settle_budget(&mut s, grant, &result);
            s.execution_expirations
                .insert(request_id, Some(e.retention()));
            // Only the budget and retention index changed, not the security leaf.
            save_account(&s);
        }
        save_execution(&e);
    }
    Ok(result)
}

#[ic_cdk::update]
async fn reconcile_execution(account_id: AccountId, request_id: Hash) -> Result<ExecutionResult> {
    let home_cose = own(&account_id, ic_cdk::api::msg_caller())?.home_cose;
    let e = load_execution(&account_id, &request_id).ok_or(Error::NotFound)?;
    let ExecutionRecord::Derivation { result, .. } = e.record else {
        return Err(Error::UnsupportedProtocol);
    };
    if result.is_terminal() {
        return Ok(result);
    }
    // A failed query proves nothing about the execution; only COSE's answer is recorded.
    let response: Result<ExecutionResult> =
        stable::call(home_cose, "get_execution", (&account_id, request_id)).await?;
    if response == Err(Error::NotFound) {
        let e = load_execution(&account_id, &request_id).ok_or(Error::ResultExpired)?;
        let digest = e.command_digest;
        return execute_existing(e, digest).await;
    }
    record_response(&account_id, request_id, response)
}

/// Begin a delayed takeover from a bound login: after the account's recovery
/// delay, `complete_recovery` replaces every device and binding with these.
/// Any active device cancels it with `DisputeRecovery`.
#[ic_cdk::update]
fn request_recovery(
    account_id: AccountId,
    request: RecoveryRequest,
    device_proof: ByteBuf,
) -> Result<()> {
    let caller = ic_cdk::api::msg_caller();
    let mut s = load(&account_id)?;
    recovery::begin_recovery(&mut s, caller, &request, &device_proof, now())?;
    save(&s);
    Ok(())
}

#[ic_cdk::update]
fn complete_recovery(account_id: AccountId, request_id: OpId) -> Result<()> {
    let caller = ic_cdk::api::msg_caller();
    if let Some(id) = AUTH.with_borrow(|t| t.load(caller.as_slice())) {
        ensure(id == account_id, Error::IdempotencyConflict)?;
    }
    let mut s = load(&account_id)?;
    let Some(removed) = recovery::complete_recovery(&mut s, caller, request_id, now())? else {
        return Ok(());
    };
    AUTH.with_borrow_mut(|t| {
        for principal in removed.iter().filter(|p| **p != caller) {
            t.delete(principal.as_slice());
        }
        t.put(caller.as_slice(), &account_id);
    });
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
    ensure(!s.sensitive_policy.frozen, Error::Locked)?;
    ensure(
        d.revoked_at.is_none() && d.input.capabilities.contains(&Capability::PaymentOffer),
        Error::Forbidden,
    )?;
    ensure(o.security_epoch == s.security_epoch, Error::PolicyStale)?;
    ensure(o.issued_at <= at && at < o.expires_at, Error::Expired)?;
    verify(
        &d.input.signing_pub,
        digest("dmsg/payment-offer/v1", o).as_slice(),
        signed.signature.as_slice(),
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
    let namespace = config().init.issuer_namespace;
    CERT.with_borrow(|c| {
        c.batch(
            ic_cdk::api::canister_self(),
            accounts.into_iter().map(|s| s.to_vec()).collect(),
            |key| {
                let id = AccountId::try_from(key).ok()?;
                load(&id).ok().map(|s| canonical(&s.snapshot(&namespace)))
            },
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

/// The retained result of a derivation; attestations are read with `get_attestation`.
#[ic_cdk::query]
fn get_execution(account_id: AccountId, request_id: Hash) -> Result<ExecutionResult> {
    own(&account_id, ic_cdk::api::msg_caller())?;
    match load_execution(&account_id, &request_id)
        .ok_or(Error::ResultExpired)?
        .record
    {
        ExecutionRecord::Derivation { result, .. } => Ok(result),
        ExecutionRecord::Attestation(_) => Err(Error::UnsupportedProtocol),
    }
}

/// The retained artifact of an attestation, for a client resuming after a lost reply.
#[ic_cdk::query]
fn get_attestation(account_id: AccountId, request_id: Hash) -> Result<SignedArtifact> {
    own(&account_id, ic_cdk::api::msg_caller())?;
    match load_execution(&account_id, &request_id)
        .ok_or(Error::ResultExpired)?
        .record
    {
        ExecutionRecord::Attestation(a) => Ok(a.artifact),
        ExecutionRecord::Derivation { .. } => Err(Error::UnsupportedProtocol),
    }
}

/// Release expired terminal results for an account, without starting another execution.
/// Public maintenance is safe: at most 64 retention entries are examined.
#[ic_cdk::update]
fn prune_executions(account_id: AccountId) -> Result<u32> {
    let at = now();
    let mut s = load(&account_id)?;
    let removed = prune_account_executions(&mut s, at);
    if removed > 0 {
        save_account(&s);
    }
    Ok(removed)
}

#[ic_cdk::query]
fn get_execution_receipt(account_id: AccountId, request_id: OpId) -> Result<CertifiedBatch> {
    own(&account_id, ic_cdk::api::msg_caller())?;
    if let Some(execution) = load_execution(&account_id, &request_id) {
        ensure(
            matches!(execution.record, ExecutionRecord::Attestation(_)),
            Error::UnsupportedProtocol,
        )?;
    }
    let namespace = config().init.issuer_namespace;
    CERT.with_borrow(|c| {
        c.batch(
            ic_cdk::api::canister_self(),
            vec![execution_receipt_key(&account_id, request_id)],
            |_| {
                let execution = load_execution(&account_id, &request_id)?;
                execution::receipt(&execution, &namespace)
                    .ok()
                    .map(|r| canonical(&r))
            },
        )
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
            |_| {
                crate::commerce::usage(&account_id, month_utc)
                    .ok()
                    .map(|u| canonical(&u))
            },
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
