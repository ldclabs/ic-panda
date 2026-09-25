use crate::{calls::CallGuard, model, state::Escrow, store::*};
use candid::{Nat, Principal};
use dmsg_protocol::*;
use dmsg_runtime::storage::MapExt;
use dmsg_types::{payment::*, profiles::delivery::*, *};
use icrc_ledger_types::icrc1::{
    account::Account,
    transfer::{TransferArg, TransferError},
};

fn now() -> u64 {
    nanos_to_millis(ic_cdk::api::time())
}

fn controller() -> Result<()> {
    ensure(
        ic_cdk::api::is_controller(&ic_cdk::api::msg_caller()),
        Error::Forbidden,
    )
}

#[ic_cdk::init]
fn init(args: PaymentInit) {
    for p in [
        args.home_user,
        args.ledger,
        args.platform.owner,
        args.governance,
    ] {
        authenticated(p).expect("canister/account");
    }
    assert!(
        args.max_open_per_payer > 0
            && args.max_open_per_payer <= 16
            && args.daily_orders > 0
            && args.daily_orders <= 100_000
            && args.ledger_fee <= args.max_fee
            && args.max_fee <= 1_000_000_000,
        "hard limits"
    );
    assert!(args.signer.valid_from < args.signer.valid_until);
    assert!(
        args.fee_policy.effective_at_ms <= now(),
        "initial fee policy must be effective"
    );
    nonzero(args.signer.public_key.as_slice()).expect("receipt key");
    SIGNERS.with_borrow_mut(|t| t.put(&args.signer.epoch.to_be_bytes(), &args.signer));
    dmsg_protocol::billing::delivery_service_fee(1, &args.fee_policy).expect("fee policy");
    FEE_POLICIES
        .with_borrow_mut(|t| t.put(&args.fee_policy.version.to_be_bytes(), &args.fee_policy));
    save_cfg(&Config {
        schema: STABLE_SCHEMA,
        init: args,
        day: 0,
        orders_today: 0,
        ledger_minute: 0,
        ledger_reads: 0,
        authorizations: Default::default(),
    });
    persist_config();
    rebuild_certification();
}

#[ic_cdk::pre_upgrade]
fn pre_upgrade() {
    persist_config();
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    assert_eq!(
        with_cfg(|c| c.schema),
        STABLE_SCHEMA,
        "explicit stable-state migration required"
    );
    rebuild_certification();
}

#[ic_cdk::update]
fn set_orders_enabled(enabled: bool) -> Result<()> {
    controller()?;
    let mut c = cfg();
    c.init.enabled = enabled;
    save_cfg(&c);
    certify_config(&c);
    Ok(())
}

/// Update the expected ledger fee within the deployment's approved ceiling.
/// Existing quotes and prepared transfers retain their approved terms.
#[ic_cdk::update]
fn set_ledger_fee(fee: u128) -> Result<()> {
    controller()?;
    let mut c = cfg();
    ensure(fee <= c.init.max_fee, Error::FeeBlocked)?;
    if c.init.ledger_fee != fee {
        c.init.ledger_fee = fee;
        save_cfg(&c);
        certify_config(&c);
    }
    Ok(())
}

#[ic_cdk::update]
fn rotate_receipt_signer(new: ReceiptSigner) -> Result<()> {
    controller()?;
    nonzero(new.public_key.as_slice())?;
    let mut c = cfg();
    ensure_valid(
        new.epoch > c.init.signer.epoch && new.valid_from < new.valid_until && !new.revoked,
        "signer epoch/interval",
    )?;
    SIGNERS.with_borrow_mut(|t| t.put(&new.epoch.to_be_bytes(), &new));
    CERT.with_borrow_mut(|c| {
        c.put(
            [b"signer/".as_slice(), new.epoch.to_be_bytes().as_slice()].concat(),
            &new,
        )
    });
    c.init.signer = new;
    save_cfg(&c);
    certify_config(&c);
    Ok(())
}

#[ic_cdk::update]
fn revoke_receipt_signer(epoch: u64) -> Result<()> {
    controller()?;
    let mut s = signer(epoch)?;
    s.revoked = true;
    CERT.with_borrow_mut(|c| {
        c.put(
            [b"signer/".as_slice(), epoch.to_be_bytes().as_slice()].concat(),
            &s,
        )
    });
    SIGNERS.with_borrow_mut(|t| t.put(&epoch.to_be_bytes(), &s));
    let mut c = cfg();
    c.init.enabled = false;
    save_cfg(&c);
    certify_config(&c);
    Ok(())
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
    let mut c = cfg();
    let _call = CallGuard::open(who, input.quote.quote_id)?;
    ensure(
        !QUOTES.with_borrow(|t| t.contains(input.quote.quote_id.as_slice())),
        Error::IdempotencyConflict,
    )?;
    ensure(
        open_count(who) < c.init.max_open_per_payer,
        Error::QuotaExceeded,
    )?;
    check_order_capacity(at)?;
    let quote_digest = model::validate_quote(
        &c.init,
        &current_fee_policy(at),
        canister_id,
        who,
        &input,
        &signer(input.quote.signer_epoch)?,
        at,
    )?;
    reserve_call(at, CallBudget::Authorization(who))?;
    let verified: Result<u64> =
        dmsg_runtime::call(c.init.home_user, "verify_payment_offer", (&input.offer,)).await?;
    let observed_at = verified?;
    let at = now();
    // The user home may sit on another subnet; only bound the freshness gap.
    ensure(at.abs_diff(observed_at) <= MINUTE, Error::PolicyStale)?;
    // The guard holds this payer and quote exclusively until commit. Other
    // messages can release the payer's slots, but cannot open another order.
    // Recheck mutable fees, enablement, deadlines and revocation. Other payers
    // can consume the daily admission budget while verification is pending.
    c = cfg();
    model::quote_current(
        &c.init,
        &current_fee_policy(at),
        &input,
        &signer(input.quote.signer_epoch)?,
        at,
    )?;
    let e = model::escrow(canister_id, id, who, &input, quote_digest);
    let count = open_count(who) + 1;
    reserve_order(at)?;
    QUOTES.with_borrow_mut(|t| t.put(input.quote.quote_id.as_slice(), &id));
    PAYER_OPEN.with_borrow_mut(|t| t.put(who.as_slice(), &count));
    PAYER_INDEX.with_borrow_mut(|t| t.put(&payer_index_key(who, &id), &()));
    save(&e);
    Ok(e.info())
}

fn payer_index_key(payer: Principal, id: &Hash) -> Vec<u8> {
    [
        digest("dmsg/payer-index/v1", &payer).as_slice(),
        id.as_slice(),
    ]
    .concat()
}

#[ic_cdk::update]
async fn check_funding(escrow_id: Hash, block: u64) -> Result<EscrowInfo> {
    let e = load(&escrow_id)?;
    if let Some(id) = FUNDING.with_borrow(|t| t.load(&block.to_be_bytes())) {
        ensure(id == escrow_id, Error::IdempotencyConflict)?;
        return Ok(e.info());
    }
    let _call = CallGuard::funding(block)?;
    reserve_call(now(), CallBudget::Ledger)?;
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

fn prepare_settlement(e: &mut Escrow, fee: u128, at: u64) -> Result<()> {
    let count = if e.quote.service_fee > 0 { 2 } else { 1 };
    let fees = fee.checked_mul(count).ok_or(Error::FeeBlocked)?;
    ensure(e.quote.fee_reserve >= fees, Error::FeeBlocked)?;
    let recipient = model::leg(
        e,
        LegKind::Recipient,
        e.quote.recipient,
        e.quote.recipient_net,
        fee,
        at,
    );
    put_leg(&recipient);
    if e.quote.service_fee > 0 {
        let platform = model::leg(
            e,
            LegKind::Platform,
            e.quote.platform,
            e.quote.service_fee,
            fee,
            at,
        );
        put_leg(&platform);
    }
    e.primary_remaining = e.quote.fee_reserve - fees;
    e.pending_payouts = count as u32;
    Ok(())
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
    prepare_settlement(&mut e, with_cfg(|c| c.init.ledger_fee), at)?;
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

#[ic_cdk::update]
fn claim_deposit_refund(escrow_id: Hash, block: u64) -> Result<TransferLeg> {
    let mut e = load(&escrow_id)?;
    let mut d = DEPOSITS
        .with_borrow(|t| t.load(&key(escrow_id, block)))
        .ok_or(Error::NotFound)?;
    // Original funding remains escrowed until a refund decision. Excess,
    // underpayments and later transfers belong to their actual source.
    let primary = if e.funding_ref == Some(block) && e.decision == FundsDecision::RefundCommitted {
        e.primary_remaining
    } else {
        0
    };
    let refundable = d
        .refundable
        .checked_add(primary)
        .ok_or(Error::IntegrityFailed)?;
    let fee = with_cfg(|c| c.init.ledger_fee);
    ensure(refundable > fee, Error::FeeBlocked)?;
    let leg = model::leg(
        &mut e,
        LegKind::Refund {
            funding_block: block,
        },
        d.from,
        refundable - fee,
        fee,
        now(),
    );
    d.refundable = 0;
    if primary > 0 {
        e.primary_remaining = 0;
    }
    DEPOSITS.with_borrow_mut(|t| t.put(&key(escrow_id, block), &d));
    put_leg(&leg);
    save_escrow(&e);
    Ok(leg)
}

#[ic_cdk::update]
fn claim_fee_reserve(escrow_id: Hash) -> Result<TransferLeg> {
    let mut e = load(&escrow_id)?;
    ensure(
        e.decision == FundsDecision::SettlementCommitted,
        Error::VersionConflict,
    )?;
    // The original fee allocation of each payout is fixed. Additional fees
    // can use only this unallocated reserve, never recipient_net.
    ensure(e.pending_payouts == 0, Error::Pending)?;
    let fee = with_cfg(|c| c.init.ledger_fee);
    ensure(e.primary_remaining > fee, Error::FeeBlocked)?;
    let amount = e.primary_remaining - fee;
    let to = e.quote.payer;
    let leg = model::leg(&mut e, LegKind::ReserveRefund, to, amount, fee, now());
    e.primary_remaining = 0;
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
    reserve_call(now(), CallBudget::Ledger)?;
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
    reserve_call(now(), CallBudget::Ledger)?;
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

#[ic_cdk::query]
fn get_receipt_signer(epoch: u64) -> Result<ReceiptSigner> {
    signer(epoch)
}

#[ic_cdk::query]
fn list_my_escrows(after: Option<Hash>) -> Result<Vec<EscrowInfo>> {
    let caller = ic_cdk::api::msg_caller();
    authenticated(caller)?;
    let prefix = digest("dmsg/payer-index/v1", &caller);
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

#[ic_cdk::update]
fn schedule_fee_policy(p: DeliveryFeePolicy) -> Result<()> {
    ensure(
        ic_cdk::api::msg_caller() == with_cfg(|c| c.init.governance),
        Error::Forbidden,
    )?;
    dmsg_protocol::billing::delivery_service_fee(1, &p)?;
    let latest =
        FEE_POLICIES.with_borrow(|t| t.last_key_value().expect("initial policy").1.into_inner());
    // A newer version than the last key cannot already be present.
    ensure(
        p.version > latest.version
            && p.effective_at_ms > latest.effective_at_ms
            && p.effective_at_ms >= now().saturating_add(30 * DAY),
        Error::PolicyStale,
    )?;
    ensure(
        FEE_POLICIES.with_borrow(|t| t.len()) < 256,
        Error::QuotaExceeded,
    )?;
    FEE_POLICIES.with_borrow_mut(|t| t.put(&p.version.to_be_bytes(), &p));
    CERT.with_borrow_mut(|c| {
        c.put(
            [b"fee/".as_slice(), p.version.to_be_bytes().as_slice()].concat(),
            &p,
        )
    });
    Ok(())
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
                [b"signer/".as_slice(), signer_epoch.to_be_bytes().as_slice()].concat(),
                [b"fee/".as_slice(), fee_version.to_be_bytes().as_slice()].concat(),
            ],
        )
    })
}
