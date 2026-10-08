use crate::{calls::CallGuard, model, store::*};
use candid::{Nat, Principal};
use dmsg_protocol::{agent::*, *};
use dmsg_runtime::admin::{self, hex, validation, Validation};
use dmsg_runtime::storage::MapExt;
use dmsg_types::{payment::*, profiles::delivery::*, *};
use icrc_ledger_types::icrc1::{
    account::Account,
    transfer::{TransferArg, TransferError},
};

fn now() -> u64 {
    nanos_to_millis(ic_cdk::api::time())
}

fn check_admin(caller: Principal) -> Result<()> {
    admin::check_admin(caller, with_cfg(|c| c.init.governance))
}

/// Refuse administrative ingress from anyone but a controller or governance,
/// and payer-only ingress from the anonymous principal, before it is paid for.
#[ic_cdk::inspect_message]
fn inspect_message() {
    let caller = ic_cdk::api::msg_caller();
    let allowed = match ic_cdk::api::msg_method_name().as_str() {
        "admin_add_user_home"
        | "admin_set_limits"
        | "set_orders_enabled"
        | "set_ledger_fee"
        | "rotate_receipt_signer"
        | "revoke_receipt_signer"
        | "schedule_fee_policy" => check_admin(caller).is_ok(),
        "open_escrow" | "claim_refund" | "revise_rejected_transfer" => {
            authenticated(caller).is_ok()
        }
        _ => true,
    };
    if allowed {
        ic_cdk::api::accept_message();
    }
}

#[ic_cdk::init]
fn init(args: PaymentInit) {
    validate_namespace(&args.issuer_namespace).expect("issuer namespace");
    validate_user_homes(&args.environment, &args.issuer_namespace, &args.user_homes)
        .expect("user homes");
    for p in [args.ledger, args.platform.owner, args.governance] {
        authenticated(p).expect("canister/account");
    }
    check_limits(&args.limits).expect("payment limits");
    assert!(
        args.ledger_fee <= args.max_fee && args.max_fee <= 1_000_000_000,
        "network fee ceiling"
    );
    assert!(args.signer.valid_from < args.signer.valid_until);
    assert!(
        args.fee_policy.effective_at_ms <= now(),
        "initial fee policy must be effective"
    );
    nonzero(args.signer.public_key.as_slice()).expect("receipt key");
    SIGNERS.with_borrow_mut(|t| t.put(&args.signer.epoch.to_be_bytes(), &args.signer));
    certify_signer(&args.signer);
    dmsg_protocol::billing::delivery_service_fee(1, &args.fee_policy).expect("fee policy");
    FEE_POLICIES
        .with_borrow_mut(|t| t.put(&args.fee_policy.version.to_be_bytes(), &args.fee_policy));
    certify_fee_policy(&args.fee_policy);
    save_cfg(Config::new(args));
    with_cfg(certify_config);
}

/// Save this minute's call budgets and today's admissions; administrative
/// changes were persisted when they were made.
#[ic_cdk::pre_upgrade]
fn pre_upgrade() {
    persist_config();
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    let started = ic_cdk::api::performance_counter(0);
    assert_eq!(
        with_cfg(|c| c.schema),
        STABLE_SCHEMA,
        "explicit stable-state migration required"
    );
    // Certification nodes persist in stable memory. The configuration leaf is
    // recertified from the restored configuration, then the root republished.
    with_cfg(certify_config);
    CERT.with_borrow(|c| c.publish());
    ic_cdk::println!(
        "payment_upgrade escrows={} instructions={}",
        ESCROWS.with_borrow(|t| t.len()),
        ic_cdk::api::performance_counter(0) - started,
    );
}

/// Replace the admission limits. Accepted escrows and their recovery are unaffected.
#[ic_cdk::update]
fn admin_set_limits(limits: PaymentLimits) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    check_limits(&limits)?;
    configure(|c| c.limits = limits);
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_set_limits(limits: PaymentLimits) -> Validation {
    validation(check_limits(&limits).map(|()| {
        let current = with_cfg(|c| c.init.limits.clone());
        format!(
            "Set payment limits: {} escrows, {} per day, {} open per payer; {} offer verifications, {} ledger reads and {} ledger transfers per minute, {} ledger calls per caller (currently {:?}).{}",
            limits.max_escrows,
            limits.daily_orders,
            limits.max_open_per_payer,
            limits.authorizations_per_minute,
            limits.ledger_reads_per_minute,
            limits.ledger_writes_per_minute,
            limits.ledger_calls_per_caller,
            current,
            admin::unchanged(current != limits, "Already set"),
        )
    }))
}

/// Installed configuration with the current homes, switch, network fee,
/// signer and limits.
#[ic_cdk::query]
fn payment_config() -> PaymentInit {
    with_cfg(|c| c.init.clone())
}

/// Record counts against the limits, with the stable size and cycle balance.
#[ic_cdk::query]
fn payment_stats() -> PaymentStats {
    PaymentStats {
        escrows: ESCROWS.with_borrow(|t| t.len()),
        orders_today: orders_today(now()),
        open_escrows: OPEN.with_borrow(|t| t.len()),
        deposits: DEPOSITS.with_borrow(|t| t.len()),
        pending_transfers: PENDING.with_borrow(|t| t.len()),
        certified_leaves: CERT.with_borrow(|c| c.len()),
        stable_pages: ic_cdk::api::stable_size(),
        cycles: ic_cdk::api::canister_cycle_balance(),
    }
}

fn check_home(init: &PaymentInit, home: Principal) -> Result<bool> {
    check_user_home(
        &init.environment,
        &init.issuer_namespace,
        &init.user_homes,
        home,
    )
}

/// Append a user home. Its `dmsg_user` must name this canister as
/// `payment_canister` and share the environment and issuer namespace.
#[ic_cdk::update]
fn admin_add_user_home(home: Principal) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    if with_cfg(|c| check_home(&c.init, home))? {
        configure(|c| c.user_homes.push(home));
    }
    Ok(())
}

#[ic_cdk::query]
fn validate_admin_add_user_home(home: Principal) -> Validation {
    validation(with_cfg(|c| {
        Ok(admin::user_home_payload(
            &c.init.environment,
            &c.init.issuer_namespace,
            home,
            check_home(&c.init, home)?,
        ))
    }))
}

#[ic_cdk::update]
fn set_orders_enabled(enabled: bool) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    configure(|c| c.enabled = enabled);
    Ok(())
}

#[ic_cdk::query]
fn validate_set_orders_enabled(enabled: bool) -> Validation {
    let fresh = with_cfg(|c| c.init.enabled != enabled);
    let action = if enabled { "Enable" } else { "Disable" };
    Ok(format!(
        "{action} new payment escrows.{}",
        admin::unchanged(fresh, "Already set"),
    ))
}

fn check_ledger_fee(fee: u128) -> Result<()> {
    ensure(fee <= with_cfg(|c| c.init.max_fee), Error::FeeBlocked)
}

/// Update the expected ledger fee within the deployment's approved ceiling.
/// Existing quotes and prepared transfers retain their approved terms.
#[ic_cdk::update]
fn set_ledger_fee(fee: u128) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    check_ledger_fee(fee)?;
    if with_cfg(|c| c.init.ledger_fee) != fee {
        configure(|c| c.ledger_fee = fee);
    }
    Ok(())
}

#[ic_cdk::query]
fn validate_set_ledger_fee(fee: u128) -> Validation {
    validation(check_ledger_fee(fee).map(|()| {
        format!(
            "Set the payment ledger fee to {fee} base units for new quotes and transfers (ceiling {}).",
            with_cfg(|c| c.init.max_fee),
        )
    }))
}

fn check_signer(new: &ReceiptSigner) -> Result<()> {
    nonzero(new.public_key.as_slice())?;
    ensure_valid(
        new.epoch > with_cfg(|c| c.init.signer.epoch)
            && new.valid_from < new.valid_until
            && !new.revoked,
        "signer epoch/interval",
    )
}

/// Make a new receipt signer current. Earlier epochs stay valid for their
/// quotes until they expire or are revoked.
#[ic_cdk::update]
fn rotate_receipt_signer(new: ReceiptSigner) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    check_signer(&new)?;
    SIGNERS.with_borrow_mut(|t| t.put(&new.epoch.to_be_bytes(), &new));
    certify_signer(&new);
    configure(|c| c.signer = new);
    Ok(())
}

#[ic_cdk::query]
fn validate_rotate_receipt_signer(new: ReceiptSigner) -> Validation {
    validation(check_signer(&new).map(|()| {
        format!(
            "Rotate the receipt signer to epoch {} with Ed25519 key {}, valid from {} to {} ms.",
            new.epoch,
            hex(new.public_key.as_slice()),
            new.valid_from,
            new.valid_until,
        )
    }))
}

/// Revoke a signer epoch and stop new escrows until orders are re-enabled.
#[ic_cdk::update]
fn revoke_receipt_signer(epoch: u64) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    let mut s = signer(epoch)?;
    s.revoked = true;
    SIGNERS.with_borrow_mut(|t| t.put(&epoch.to_be_bytes(), &s));
    certify_signer(&s);
    configure(|c| c.enabled = false);
    Ok(())
}

#[ic_cdk::query]
fn validate_revoke_receipt_signer(epoch: u64) -> Validation {
    // Revoking again still disables escrows that were re-enabled since.
    let enabled = with_cfg(|c| c.init.enabled);
    validation(signer(epoch).map(|s| {
        format!(
            "Revoke receipt signer epoch {epoch} with key {} and disable new payment escrows.{}",
            hex(s.public_key.as_slice()),
            admin::unchanged(
                !s.revoked || enabled,
                "Already revoked and escrows disabled"
            ),
        )
    }))
}

#[ic_cdk::update]
async fn open_escrow(input: OpenEscrow) -> Result<EscrowInfo> {
    let at = now();
    let canister_id = ic_cdk::api::canister_self();
    let who = ic_cdk::api::msg_caller();
    let id = digest("dmsg/escrow-id/v1", &(canister_id, who, input.op_id));
    if let Ok(e) = load(&id) {
        ensure(
            e.quote_digest == digest("dmsg/quote/v2", &input.quote),
            Error::IdempotencyConflict,
        )?;
        return Ok(e.info());
    }
    let mut config = with_cfg(|c| c.init.clone());
    let _call = CallGuard::open(who, input.quote.quote_id)?;
    ensure(
        !QUOTES.with_borrow(|t| t.contains(input.quote.quote_id.as_slice())),
        Error::IdempotencyConflict,
    )?;
    ensure(
        open_slots(who, at) < config.limits.max_open_per_payer,
        Error::QuotaExceeded,
    )?;
    check_order_capacity(at)?;
    let quote_digest = model::validate_quote(
        &config,
        &current_fee_policy(at),
        canister_id,
        who,
        &input,
        &signer(input.quote.signer_epoch)?,
        at,
    )?;
    // The offer's account ID names the user home that allocated it.
    let home = account_home(
        &config.environment,
        &config.issuer_namespace,
        &config.user_homes,
        &input.offer.offer.account_id,
    )
    .ok_or(Error::NotFound)?;
    reserve_call(at, CallBudget::Authorization(who))?;
    let verified: Result<u64> =
        dmsg_runtime::call(home, "verify_payment_offer", (&input.offer,)).await?;
    let observed_at = verified?;
    let at = now();
    // The user home may sit on another subnet; only bound the freshness gap.
    ensure(at.abs_diff(observed_at) <= MINUTE, Error::PolicyStale)?;
    // The guard holds this payer and quote exclusively until commit. Other
    // messages can release the payer's slots, but cannot open another order.
    // Recheck mutable fees, enablement, deadlines and revocation. Other payers
    // can consume the daily admission budget while verification is pending.
    config = with_cfg(|c| c.init.clone());
    model::quote_current(
        &config,
        &current_fee_policy(at),
        &input,
        &signer(input.quote.signer_epoch)?,
        at,
    )?;
    let e = model::escrow(canister_id, id, who, &input, quote_digest);
    reserve_order(at)?;
    QUOTES.with_borrow_mut(|t| t.put(input.quote.quote_id.as_slice(), &()));
    index_payer(&e);
    save(&e);
    Ok(e.info())
}

/// Escrows of `payer` still waiting for a funds decision. Expired ones get
/// their refund decision first, as anyone may give with `expiry_refund`, so
/// abandoned unpaid orders do not hold the payer's slots.
fn open_slots(payer: Principal, at: u64) -> u32 {
    let mut open = 0;
    for id in open_escrows(payer) {
        let mut e = load(&id).expect("open escrow");
        if model::refund(&mut e, at) == Ok(true) {
            release_payer(&e);
            save(&e);
        } else {
            open += 1;
        }
    }
    open
}

#[ic_cdk::update]
async fn check_funding(escrow_id: Hash, block: u64) -> Result<EscrowInfo> {
    let e = load(&escrow_id)?;
    if let Some(id) = FUNDING.with_borrow(|t| t.load(&block.to_be_bytes())) {
        ensure(id == escrow_id, Error::IdempotencyConflict)?;
        return Ok(e.info());
    }
    let _call = CallGuard::funding(block)?;
    reserve_call(now(), CallBudget::LedgerRead(ic_cdk::api::msg_caller()))?;
    let tx = dmsg_runtime::ledger::read_transfer(e.quote.ledger, block).await?;
    // The guard excludes another claim of this block. A concurrent settlement
    // or refund can still change the order's direction, so reload the escrow.
    let mut current = load(&escrow_id)?;
    let d = model::accept_deposit(&mut current, ic_cdk::api::canister_self(), &tx)?;
    FUNDING.with_borrow_mut(|t| t.put(&block.to_be_bytes(), &escrow_id));
    DEPOSITS.with_borrow_mut(|t| t.put(&key(escrow_id, block), &d));
    save(&current);
    Ok(current.info())
}

#[ic_cdk::update]
fn finalize_receipt(signed: SignedReceipt) -> Result<EscrowInfo> {
    let at = now();
    let mut e = load(&signed.receipt.escrow_id)?;
    if e.decision == FundsDecision::SettlementCommitted {
        ensure(
            e.receipt_digest == Some(digest("dmsg/admission-receipt/v2", &signed.receipt)),
            Error::IdempotencyConflict,
        )?;
        return Ok(e.info());
    }
    // Reject impossible settlements before doing public-key verification.
    model::settlement_open(&e, at)?;
    let hash = model::receipt_valid(
        &e,
        &signed,
        &signer(signed.receipt.signer_epoch)?,
        ic_cdk::api::canister_self(),
        at,
    )?;
    // No external calls are required here: funding has already been
    // verified and durably claimed by check_funding.
    model::settle(&mut e, hash);
    for leg in model::prepare_settlement(&mut e, with_cfg(|c| c.init.ledger_fee), at)? {
        put_leg(&leg);
    }
    release_payer(&e);
    save(&e);
    Ok(e.info())
}

#[ic_cdk::update]
fn expiry_refund(escrow_id: Hash) -> Result<EscrowInfo> {
    let mut e = load(&escrow_id)?;
    if model::refund(&mut e, now())? {
        release_payer(&e);
        save(&e);
    }
    Ok(e.info())
}

fn refund_deposits(escrow_id: Hash, mut blocks: Vec<u64>) -> Result<Vec<Deposit>> {
    ensure(blocks.len() <= MAX_REFUND_DEPOSITS, Error::QuotaExceeded)?;
    blocks.sort_unstable();
    ensure_valid(
        blocks.windows(2).all(|pair| pair[0] != pair[1]),
        "duplicate deposits",
    )?;
    DEPOSITS.with_borrow(|t| {
        blocks
            .into_iter()
            .map(|block| t.load(&key(escrow_id, block)).ok_or(Error::NotFound))
            .collect()
    })
}

/// Preview the current fee and any balance too small to refund. Reserve is
/// available after a refund decision or after all settlement payouts succeed.
#[ic_cdk::query]
fn quote_refund(escrow_id: Hash, blocks: Vec<u64>, include_reserve: bool) -> Result<RefundQuote> {
    let e = load(&escrow_id)?;
    let deposits = refund_deposits(escrow_id, blocks)?;
    model::refund_quote(
        &e,
        &deposits,
        include_reserve,
        with_cfg(|c| c.init.ledger_fee),
    )
}

/// Combine only allocations belonging to the same exact original account.
/// The payer or the owner of the refunded funds may claim.
#[ic_cdk::update]
fn claim_refund(escrow_id: Hash, blocks: Vec<u64>, include_reserve: bool) -> Result<TransferLeg> {
    let at = now();
    let mut e = load(&escrow_id)?;
    let mut deposits = refund_deposits(escrow_id, blocks)?;
    let leg = model::claim_refund(
        &mut e,
        &mut deposits,
        include_reserve,
        with_cfg(|c| c.init.ledger_fee),
        ic_cdk::api::msg_caller(),
        at,
    )?;
    DEPOSITS.with_borrow_mut(|t| {
        for deposit in &deposits {
            t.put(&key(escrow_id, deposit.block), deposit);
        }
    });
    put_leg(&leg);
    save_escrow(&e);
    Ok(leg)
}

fn complete(mut leg: TransferLeg, block: u64) -> Result<TransferLeg> {
    let id = leg.escrow_id;
    let n = leg.leg_id;
    if leg.status == LegStatus::Succeeded {
        ensure(leg.block == Some(block), Error::IntegrityFailed)?;
        return Ok(leg);
    }
    let mut e = load(&id)?;
    if let Some(old) = OUTGOING.with_borrow(|t| t.load(&block.to_be_bytes())) {
        ensure(old == (id, n), Error::IdempotencyConflict)?;
    }
    model::complete_leg(&mut e, &mut leg, block)?;
    OUTGOING.with_borrow_mut(|t| t.put(&block.to_be_bytes(), &(id, n)));
    put_leg(&leg);
    save(&e);
    Ok(leg)
}

#[ic_cdk::update]
async fn process_transfer(escrow_id: Hash, leg_id: u64) -> Result<TransferLeg> {
    let mut leg = get_leg(escrow_id, leg_id)?;
    if leg.status == LegStatus::Succeeded {
        return Ok(leg);
    }
    ensure(leg.status != LegStatus::Superseded, Error::VersionConflict)?;
    // The guard excludes a concurrent transfer or reconciliation of this leg.
    // A leg left InFlight by a lost callback is resent with its frozen
    // parameters; the ledger deduplicates by memo and created_at_time.
    let _call = CallGuard::leg(escrow_id, leg_id)?;
    let e = load(&escrow_id)?;
    let was_unknown = matches!(leg.status, LegStatus::Unknown | LegStatus::InFlight);
    reserve_call(now(), CallBudget::LedgerWrite(ic_cdk::api::msg_caller()))?;
    // The outbox parameters are frozen before await; public retries never
    // replace timestamps or recreate a transfer after an ambiguous response.
    leg.status = LegStatus::InFlight;
    put_leg(&leg);
    let args = TransferArg {
        from_subaccount: Some(e.subaccount.into_array()),
        to: leg.to,
        fee: Some(Nat::from(leg.fee)),
        created_at_time: Some(leg.created_at_time),
        memo: Some(leg.memo.to_vec().into()),
        amount: Nat::from(leg.amount),
    };
    let response: std::result::Result<
        std::result::Result<Nat, TransferError>,
        dmsg_runtime::CallFailure,
    > = dmsg_runtime::call_classified(e.quote.ledger, "icrc1_transfer", (args,)).await;
    let mut current = get_leg(escrow_id, leg_id)?;
    if current.status == LegStatus::Succeeded {
        return Ok(current);
    }
    let outcome = model::transfer_result(&mut current, was_unknown, response);
    if let Ok(Some(block)) = outcome {
        return complete(current, block);
    }
    put_leg(&current);
    outcome.map(|_| current)
}

/// Only an unambiguously rejected transfer may receive new parameters. Keep
/// its immutable old leg for audit, consume approved reserve for beneficiary
/// fees, and return all refund remainders to the original source.
#[ic_cdk::update]
fn revise_rejected_transfer(escrow_id: Hash, leg_id: u64, fee: u128) -> Result<TransferLeg> {
    let mut e = load(&escrow_id)?;
    let mut old = get_leg(escrow_id, leg_id)?;
    let new = model::revise_leg(&mut e, &mut old, ic_cdk::api::msg_caller(), fee, now())?;
    put_leg(&old);
    put_leg(&new);
    // Keep only a bounded chain of known-unsent versions. Pending, ambiguous
    // and successful transfers are never pruned; IDs are never reused.
    let mut cursor = new.clone();
    for _ in 1..TRANSFER_HISTORY_LIMIT {
        let Some(parent) = cursor.replaces else { break };
        let Ok(previous) = get_leg(escrow_id, parent) else {
            break;
        };
        cursor = previous;
    }
    if let Some(parent) = cursor.replaces {
        if get_leg(escrow_id, parent).is_ok_and(|leg| leg.status == LegStatus::Superseded) {
            LEGS.with_borrow_mut(|t| t.delete(&key(escrow_id, parent)));
        }
    }
    save_escrow(&e);
    Ok(new)
}

#[ic_cdk::update]
async fn reconcile_transfer(escrow_id: Hash, leg_id: u64, block: u64) -> Result<TransferLeg> {
    let l = get_leg(escrow_id, leg_id)?;
    if l.status == LegStatus::Succeeded {
        return Ok(l);
    }
    ensure(
        matches!(l.status, LegStatus::InFlight | LegStatus::Unknown),
        Error::VersionConflict,
    )?;
    let _call = CallGuard::leg(escrow_id, leg_id)?;
    let e = load(&escrow_id)?;
    reserve_call(now(), CallBudget::LedgerRead(ic_cdk::api::msg_caller()))?;
    let tx = dmsg_runtime::ledger::read_transfer(e.quote.ledger, block).await?;
    ensure(
        tx.from
            == Account {
                owner: ic_cdk::api::canister_self(),
                subaccount: Some(e.subaccount.into_array()),
            }
            && tx.to == l.to
            && tx.amount == l.amount
            && tx.fee == Some(l.fee)
            && tx.memo == Some(l.memo.to_vec())
            && tx.created_at_time == Some(l.created_at_time),
        Error::IntegrityFailed,
    )?;
    complete(get_leg(escrow_id, leg_id)?, block)
}

#[ic_cdk::query]
fn get_escrow(escrow_id: Hash) -> Result<EscrowInfo> {
    Ok(load(&escrow_id)?.info())
}

#[ic_cdk::query]
fn get_escrow_by_operation(payer: Principal, op_id: Hash) -> Result<EscrowInfo> {
    Ok(load(&digest(
        "dmsg/escrow-id/v1",
        &(ic_cdk::api::canister_self(), payer, op_id),
    ))?
    .info())
}

#[ic_cdk::query]
fn get_escrow_certified(ids: Vec<Hash>) -> Result<CertifiedBatch> {
    CERT.with_borrow(|c| {
        c.batch(
            ic_cdk::api::canister_self(),
            ids.into_iter().map(|v| v.to_vec()).collect(),
            |key| {
                let id = Hash::new(key.try_into().ok()?);
                load(&id).ok().map(|e| canonical(&e.info()))
            },
        )
    })
}

#[ic_cdk::query]
fn get_transfer(escrow_id: Hash, leg_id: u64) -> Result<TransferLeg> {
    get_leg(escrow_id, leg_id)
}

#[ic_cdk::query]
fn get_deposit(escrow_id: Hash, block: u64) -> Option<Deposit> {
    DEPOSITS.with_borrow(|t| t.load(&key(escrow_id, block)))
}

/// Enumerate verified incoming records, including non-primary deposits.
#[ic_cdk::query]
fn list_deposits(escrow_id: Hash, after: Option<u64>) -> Result<Vec<Deposit>> {
    load(&escrow_id)?;
    let cursor = after.map_or_else(|| escrow_id.to_vec(), |block| key(escrow_id, block));
    Ok(DEPOSITS
        .with_borrow(|t| t.page(cursor, MAX_REFUND_DEPOSITS))
        .into_iter()
        .take_while(|(key, _)| key.starts_with(escrow_id.as_slice()))
        .map(|(_, deposit)| deposit)
        .collect())
}

#[ic_cdk::query]
fn get_receipt_signer(epoch: u64) -> Result<ReceiptSigner> {
    signer(epoch)
}

#[ic_cdk::query]
fn list_my_escrows(after: Option<Hash>) -> Result<Vec<EscrowInfo>> {
    let caller = ic_cdk::api::msg_caller();
    authenticated(caller)?;
    let prefix = payer_prefix(caller);
    let cursor = after.map_or_else(|| prefix.to_vec(), |id| payer_index_key(caller, &id));
    let entries = PAYER_INDEX.with_borrow(|t| t.page(cursor, 32));
    entries
        .into_iter()
        .take_while(|(key, _)| key.starts_with(prefix.as_slice()))
        .map(|(key, _)| {
            let id = Hash::new(key[prefix.len()..].try_into().expect("payer index key"));
            load(&id).map(|e| e.info())
        })
        .collect()
}

/// Legs of every escrow that have not succeeded or been superseded, oldest
/// `created_at_time` first, 32 per page. Pass the last leg's
/// `(created_at_time, escrow_id, leg_id)` for the next page. A payout
/// dispatcher sends them with `process_transfer` inside the ledger's
/// deduplication window.
#[ic_cdk::query]
fn list_pending_transfers(after: Option<(u64, Hash, u64)>) -> Vec<TransferLeg> {
    pending_legs(after)
}

#[ic_cdk::query]
fn list_transfers(escrow_id: Hash, after: Option<u64>) -> Result<Vec<TransferLeg>> {
    load(&escrow_id)?;
    let cursor = after.map_or_else(|| escrow_id.to_vec(), |n| key(escrow_id, n));
    Ok(LEGS
        .with_borrow(|t| t.page(cursor, 32))
        .into_iter()
        .take_while(|(key, _)| key.starts_with(escrow_id.as_slice()))
        .map(|(_, leg)| leg)
        .collect())
}

fn current_fee_policy(at: u64) -> DeliveryFeePolicy {
    // Versions and effective times increase together; future schedules are skipped.
    FEE_POLICIES.with_borrow(|t| {
        t.iter()
            .rev()
            .map(|entry| entry.value().into_inner())
            .find(|policy| policy.effective_at_ms <= at)
            .expect("initial fee policy is effective at installation")
    })
}

#[ic_cdk::query]
fn get_fee_policy() -> DeliveryFeePolicy {
    current_fee_policy(now())
}

fn check_fee_policy(p: &DeliveryFeePolicy, at: u64) -> Result<()> {
    dmsg_protocol::billing::delivery_service_fee(1, p)?;
    let latest =
        FEE_POLICIES.with_borrow(|t| t.last_key_value().expect("initial policy").1.into_inner());
    // A newer version than the last key cannot already be present.
    ensure(
        p.version > latest.version
            && p.effective_at_ms > latest.effective_at_ms
            && p.effective_at_ms >= at.saturating_add(30 * DAY),
        Error::PolicyStale,
    )?;
    ensure(
        FEE_POLICIES.with_borrow(|t| t.len()) < 256,
        Error::QuotaExceeded,
    )
}

/// Announce a delivery fee policy at least 30 days before it takes effect.
#[ic_cdk::update]
fn schedule_fee_policy(p: DeliveryFeePolicy) -> Result<()> {
    check_admin(ic_cdk::api::msg_caller())?;
    check_fee_policy(&p, now())?;
    FEE_POLICIES.with_borrow_mut(|t| t.put(&p.version.to_be_bytes(), &p));
    certify_fee_policy(&p);
    Ok(())
}

#[ic_cdk::query]
fn validate_schedule_fee_policy(p: DeliveryFeePolicy) -> Validation {
    validation(check_fee_policy(&p, now()).map(|()| {
        format!(
            "Schedule delivery fee policy version {} effective at {} ms: {} bps with a minimum of {} base units.",
            p.version, p.effective_at_ms, p.rate_bps, p.minimum_atomic,
        )
    }))
}

#[ic_cdk::query]
fn get_configuration_certified(
    signer_epoch: Option<u64>,
    fee_version: Option<u64>,
) -> Result<CertifiedBatch> {
    let signer_epoch = signer_epoch.unwrap_or_else(|| with_cfg(|c| c.init.signer.epoch));
    let fee_version = fee_version.unwrap_or_else(|| current_fee_policy(now()).version);
    CERT.with_borrow(|c| {
        c.batch(
            ic_cdk::api::canister_self(),
            vec![
                b"configuration".to_vec(),
                signer_key(signer_epoch),
                fee_key(fee_version),
            ],
            configuration_leaf,
        )
    })
}
