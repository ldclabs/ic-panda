use candid::{Nat, Principal};
use dmsg_types::{
    ledger,
    payment::*,
    stable::{self, Certification, Table},
    *,
};
use icrc_ledger_types::icrc1::{
    account::Account,
    transfer::{TransferArg, TransferError},
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
mod model;

#[derive(Serialize, Deserialize)]
struct Config {
    schema: u16,
    init: PaymentInit,
    day: u64,
    orders_today: u32,
    ledger_minute: u64,
    ledger_reads: u32,
}
thread_local! {
    static CONFIG:RefCell<Table>=RefCell::new(Table::new(0));
    static ESCROWS:RefCell<Table>=RefCell::new(Table::new(1));
    static QUOTES:RefCell<Table>=RefCell::new(Table::new(2));
    static FUNDING:RefCell<Table>=RefCell::new(Table::new(3));
    static DEPOSITS:RefCell<Table>=RefCell::new(Table::new(4));
    static LEGS:RefCell<Table>=RefCell::new(Table::new(5));
    static SIGNERS:RefCell<Table>=RefCell::new(Table::new(6));
    static PAYER_OPEN:RefCell<Table>=RefCell::new(Table::new(7));
    static OUTGOING:RefCell<Table>=RefCell::new(Table::new(8));
    static PAYER_INDEX:RefCell<Table>=RefCell::new(Table::new(9));
    static CERT:RefCell<Certification>=RefCell::new(Certification::default());
}
fn cfg() -> Config {
    CONFIG.with_borrow(|t| t.get(b"config").expect("initialized"))
}
fn save_cfg(c: &Config) {
    CONFIG.with_borrow_mut(|t| t.put(b"config", c));
}
fn now() -> u64 {
    ic_cdk::api::time()
}
fn me() -> Principal {
    ic_cdk::api::canister_self()
}
fn caller() -> Principal {
    ic_cdk::api::msg_caller()
}
fn controller() -> Result<()> {
    ensure(ic_cdk::api::is_controller(&caller()), Error::Forbidden)
}
fn load(id: &Hash) -> Result<Escrow> {
    ESCROWS.with_borrow(|t| t.get(id).ok_or(Error::NotFound))
}
fn save(e: &Escrow) {
    assert!(e.conserved(), "funds conservation");
    ESCROWS.with_borrow_mut(|t| t.put(&e.escrow_id, e));
    CERT.with_borrow_mut(|c| c.put(e.escrow_id.to_vec(), e));
}
fn key(id: Hash, n: u64) -> Vec<u8> {
    [id.as_slice(), n.to_be_bytes().as_slice()].concat()
}
fn signer(epoch: u64) -> Result<ReceiptSigner> {
    SIGNERS.with_borrow(|t| t.get(&epoch.to_be_bytes()).ok_or(Error::NotFound))
}
fn open_count(p: Principal) -> u32 {
    PAYER_OPEN.with_borrow(|t| t.get(p.as_slice()).unwrap_or(0))
}
fn release_payer(e: &Escrow) {
    let n = open_count(e.payer_principal)
        .checked_sub(1)
        .expect("open accounting");
    PAYER_OPEN.with_borrow_mut(|t| t.put(e.payer_principal.as_slice(), &n));
}
fn put_leg(l: &TransferLeg) {
    LEGS.with_borrow_mut(|t| t.put(&key(l.escrow_id, l.leg_id), l));
}
fn get_leg(id: Hash, n: u64) -> Result<TransferLeg> {
    LEGS.with_borrow(|t| t.get(&key(id, n)).ok_or(Error::NotFound))
}
#[ic_cdk::init]
fn init(args: PaymentInit) {
    for p in [args.home_user, args.ledger, args.platform.owner] {
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
    nonzero(&args.signer.public_key).expect("receipt key");
    SIGNERS.with_borrow_mut(|t| t.put(&args.signer.epoch.to_be_bytes(), &args.signer));
    save_cfg(&Config {
        schema: 1,
        init: args,
        day: 0,
        orders_today: 0,
        ledger_minute: 0,
        ledger_reads: 0,
    });
    CERT.with_borrow(|c| c.publish());
}
#[ic_cdk::post_upgrade]
fn post_upgrade() {
    CERT.with_borrow(|c| c.publish());
    assert_eq!(cfg().schema, 1);
    ESCROWS.with_borrow(|t| t.for_each::<Escrow>(|k, e| CERT.with_borrow_mut(|c| c.put(k, &e))));
}
#[ic_cdk::update]
fn set_orders_enabled(enabled: bool) -> Result<()> {
    controller()?;
    let mut c = cfg();
    c.init.enabled = enabled;
    save_cfg(&c);
    Ok(())
}
#[ic_cdk::update]
fn rotate_receipt_signer(new: ReceiptSigner) -> Result<()> {
    controller()?;
    nonzero(&new.public_key)?;
    ensure(
        new.epoch > cfg().init.signer.epoch && new.valid_from < new.valid_until && !new.revoked,
        invalid("signer epoch/interval"),
    )?;
    SIGNERS.with_borrow_mut(|t| t.put(&new.epoch.to_be_bytes(), &new));
    let mut c = cfg();
    c.init.signer = new;
    save_cfg(&c);
    Ok(())
}
#[ic_cdk::update]
fn revoke_receipt_signer(epoch: u64) -> Result<()> {
    controller()?;
    let mut s = signer(epoch)?;
    s.revoked = true;
    SIGNERS.with_borrow_mut(|t| t.put(&epoch.to_be_bytes(), &s));
    let mut c = cfg();
    c.init.enabled = false;
    save_cfg(&c);
    Ok(())
}

#[ic_cdk::update]
async fn open_escrow(input: OpenEscrow) -> Result<Escrow> {
    let who = caller();
    let mut c = cfg();
    let id = digest("dmsg/escrow-id/v1", &(me(), who, input.op_id));
    let quote_digest = digest("dmsg/quote/v1", &input.quote);
    if let Ok(e) = load(&id) {
        ensure(e.quote_digest == quote_digest, Error::IdempotencyConflict)?;
        return Ok(e);
    }
    model::validate_quote(
        &c.init,
        me(),
        who,
        &input,
        &signer(input.quote.signer_epoch)?,
        now(),
    )?;
    ensure(
        !QUOTES.with_borrow(|t| t.contains(&input.quote.quote_id)),
        Error::IdempotencyConflict,
    )?;
    ensure(
        open_count(who) < c.init.max_open_per_payer,
        Error::QuotaExceeded,
    )?;
    if now() / DAY > c.day {
        c.day = now() / DAY;
        c.orders_today = 0;
    }
    ensure(c.orders_today < c.init.daily_orders, Error::QuotaExceeded)?;
    c.orders_today += 1;
    save_cfg(&c);
    let verified: Result<u64> = stable::call(
        c.init.home_user,
        "verify_payment_offer",
        (input.offer.clone(),),
    )
    .await?;
    let observed_at = verified?;
    ensure(
        observed_at <= now() && now() - observed_at <= MINUTE,
        Error::PolicyStale,
    )?;
    // Recheck all local contract, signer and quota state after the await.
    if let Ok(e) = load(&id) {
        ensure(e.quote_digest == quote_digest, Error::IdempotencyConflict)?;
        return Ok(e);
    }
    c = cfg();
    model::validate_quote(
        &c.init,
        me(),
        who,
        &input,
        &signer(input.quote.signer_epoch)?,
        now(),
    )?;
    ensure(
        !QUOTES.with_borrow(|t| t.contains(&input.quote.quote_id))
            && open_count(who) < c.init.max_open_per_payer,
        Error::QuotaExceeded,
    )?;
    let e = model::escrow(me(), who, &input, quote_digest);
    let count = open_count(who) + 1;
    QUOTES.with_borrow_mut(|t| t.put(&input.quote.quote_id, &id));
    PAYER_OPEN.with_borrow_mut(|t| t.put(who.as_slice(), &count));
    PAYER_INDEX.with_borrow_mut(|t| {
        t.put(
            &[
                digest("dmsg/payer-index/v1", &who).as_slice(),
                id.as_slice(),
            ]
            .concat(),
            &id,
        )
    });
    save(&e);
    Ok(e)
}
fn reserve_ledger_call() -> Result<()> {
    let mut c = cfg();
    let minute = now() / MINUTE;
    if c.ledger_minute < minute {
        c.ledger_minute = minute;
        c.ledger_reads = 0;
    }
    ensure(c.ledger_reads < 400, Error::QuotaExceeded)?;
    c.ledger_reads += 1;
    save_cfg(&c);
    Ok(())
}
#[ic_cdk::update]
async fn check_funding(escrow_id: Hash, block: u64) -> Result<Escrow> {
    let e = load(&escrow_id)?;
    if let Some(id) = FUNDING.with_borrow(|t| t.get::<Hash>(&block.to_be_bytes())) {
        ensure(id == escrow_id, Error::IdempotencyConflict)?;
        return Ok(e);
    }
    reserve_ledger_call()?;
    let tx = ledger::read_transfer(e.quote.ledger, block).await?;
    ensure(tx.committed_at <= now(), Error::IntegrityFailed)?;
    // Concurrent check/finalize/refund may have changed both ownership and the
    // terminal direction while the trusted ledger was being read.
    if let Some(id) = FUNDING.with_borrow(|t| t.get::<Hash>(&block.to_be_bytes())) {
        ensure(id == escrow_id, Error::IdempotencyConflict)?;
        return load(&escrow_id);
    }
    let mut current = load(&escrow_id)?;
    let d = model::accept_deposit(&mut current, me(), &tx)?;
    FUNDING.with_borrow_mut(|t| t.put(&block.to_be_bytes(), &escrow_id));
    DEPOSITS.with_borrow_mut(|t| t.put(&key(escrow_id, block), &d));
    save(&current);
    Ok(current)
}

fn prepare_settlement(e: &mut Escrow, fee: u128) -> Result<()> {
    let count = if e.quote.service_fee > 0 { 2 } else { 1 };
    let fees = fee.checked_mul(count).ok_or(Error::FeeBlocked)?;
    ensure(e.quote.fee_reserve >= fees, Error::FeeBlocked)?;
    let recipient = model::leg(
        e,
        LegKind::Recipient,
        e.quote.recipient,
        e.quote.recipient_net,
        fee,
        now(),
    );
    put_leg(&recipient);
    if e.quote.service_fee > 0 {
        let platform = model::leg(
            e,
            LegKind::Platform,
            e.quote.platform,
            e.quote.service_fee,
            fee,
            now(),
        );
        put_leg(&platform);
    }
    e.primary_remaining = e.quote.fee_reserve - fees;
    e.pending_payouts = count as u32;
    Ok(())
}
#[ic_cdk::update]
fn finalize_receipt(signed: SignedReceipt) -> Result<Escrow> {
    let mut e = load(&signed.receipt.escrow_id)?;
    if e.decision == FundsDecision::SettlementCommitted {
        ensure(
            e.receipt_digest == Some(digest("dmsg/admission-receipt/v1", &signed.receipt)),
            Error::IdempotencyConflict,
        )?;
        return Ok(e);
    }
    ensure(e.decision == FundsDecision::Pending, Error::VersionConflict)?;
    let hash = model::receipt_valid(
        &e,
        &signed,
        &signer(signed.receipt.signer_epoch)?,
        me(),
        now(),
    )?;
    if model::settle(&mut e, hash, now())? {
        // No external calls are required here: funding has already been
        // verified and durably claimed by check_funding.
        prepare_settlement(&mut e, cfg().init.ledger_fee)?;
        release_payer(&e);
        save(&e);
    }
    Ok(e)
}
#[ic_cdk::update]
fn expiry_refund(escrow_id: Hash) -> Result<Escrow> {
    let mut e = load(&escrow_id)?;
    if model::refund(&mut e, now())? {
        release_payer(&e);
        save(&e);
    }
    Ok(e)
}
#[ic_cdk::update]
fn claim_deposit_refund(escrow_id: Hash, block: u64) -> Result<TransferLeg> {
    let mut e = load(&escrow_id)?;
    let mut d = DEPOSITS
        .with_borrow(|t| t.get::<Deposit>(&key(escrow_id, block)))
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
    let fee = cfg().init.ledger_fee;
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
    save(&e);
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
    let fee = cfg().init.ledger_fee;
    ensure(e.primary_remaining > fee, Error::FeeBlocked)?;
    let amount = e.primary_remaining - fee;
    let to = e.quote.payer;
    let leg = model::leg(&mut e, LegKind::ReserveRefund, to, amount, fee, now());
    e.primary_remaining = 0;
    put_leg(&leg);
    save(&e);
    Ok(leg)
}

fn complete(id: Hash, n: u64, block: u64) -> Result<TransferLeg> {
    let mut e = load(&id)?;
    let mut leg = get_leg(id, n)?;
    if let Some(old) = OUTGOING.with_borrow(|t| t.get::<Vec<u8>>(&block.to_be_bytes())) {
        ensure(old == key(id, n), Error::IdempotencyConflict)?;
    }
    model::complete_leg(&mut e, &mut leg, block)?;
    OUTGOING.with_borrow_mut(|t| t.put(&block.to_be_bytes(), &key(id, n)));
    put_leg(&leg);
    save(&e);
    Ok(leg)
}
#[ic_cdk::update]
async fn process_transfer(escrow_id: Hash, leg_id: u64) -> Result<TransferLeg> {
    let e = load(&escrow_id)?;
    let mut leg = get_leg(escrow_id, leg_id)?;
    if leg.status == LegStatus::Succeeded {
        return Ok(leg);
    }
    ensure(
        matches!(
            leg.status,
            LegStatus::Pending | LegStatus::Unknown | LegStatus::FeeBlocked | LegStatus::Rejected
        ),
        if leg.status == LegStatus::InFlight {
            Error::Pending
        } else {
            Error::FeeBlocked
        },
    )?;
    let was_unknown = leg.status == LegStatus::Unknown;
    reserve_ledger_call()?;
    // The outbox parameters are frozen before await; public retries never
    // replace timestamps or recreate a transfer after an ambiguous response.
    leg.status = LegStatus::InFlight;
    put_leg(&leg);
    let args = TransferArg {
        from_subaccount: Some(e.subaccount),
        to: leg.to,
        fee: Some(Nat::from(leg.fee)),
        created_at_time: Some(leg.created_at_time),
        memo: Some(leg.memo.to_vec().into()),
        amount: Nat::from(leg.amount),
    };
    let response: Result<std::result::Result<Nat, TransferError>> =
        stable::call(e.quote.ledger, "icrc1_transfer", (args,)).await;
    let current = get_leg(escrow_id, leg_id)?;
    if current.status == LegStatus::Succeeded {
        return Ok(current);
    }
    match response {
        Ok(Ok(block)) => complete(escrow_id, leg_id, ledger::block_index(block)?),
        Ok(Err(TransferError::Duplicate { duplicate_of })) => {
            complete(escrow_id, leg_id, ledger::block_index(duplicate_of)?)
        }
        Ok(Err(TransferError::BadFee { expected_fee })) => {
            leg.status = if was_unknown {
                LegStatus::Unknown
            } else {
                LegStatus::FeeBlocked
            };
            if !was_unknown {
                leg.expected_fee = ledger::token_amount(expected_fee).ok();
            }
            put_leg(&leg);
            Ok(leg)
        }
        Ok(Err(_)) => {
            leg.status = if was_unknown {
                LegStatus::Unknown
            } else {
                LegStatus::Rejected
            };
            put_leg(&leg);
            Ok(leg)
        }
        Err(_) => {
            leg.status = LegStatus::Unknown;
            put_leg(&leg);
            Err(Error::ExecutionUnknown)
        }
    }
}
/// Only an unambiguously rejected transfer may receive new parameters. Keep
/// its immutable old leg for audit, consume approved reserve for beneficiary
/// fees, and return all refund remainders to the original source.
#[ic_cdk::update]
fn revise_rejected_transfer(escrow_id: Hash, leg_id: u64, fee: u128) -> Result<TransferLeg> {
    let mut e = load(&escrow_id)?;
    let mut old = get_leg(escrow_id, leg_id)?;
    let new = model::revise_leg(&mut e, &mut old, caller(), fee, now())?;
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
            LEGS.with_borrow_mut(|t| t.remove(&key(escrow_id, parent)));
        }
    }
    save(&e);
    Ok(new)
}
#[ic_cdk::update]
async fn reconcile_transfer(escrow_id: Hash, leg_id: u64, block: u64) -> Result<TransferLeg> {
    let e = load(&escrow_id)?;
    let l = get_leg(escrow_id, leg_id)?;
    if l.status == LegStatus::Succeeded {
        return Ok(l);
    }
    ensure(
        matches!(l.status, LegStatus::InFlight | LegStatus::Unknown),
        Error::VersionConflict,
    )?;
    reserve_ledger_call()?;
    let tx = ledger::read_transfer(e.quote.ledger, block).await?;
    ensure(
        tx.from
            == Account {
                owner: me(),
                subaccount: Some(e.subaccount),
            }
            && tx.to == l.to
            && tx.amount == l.amount
            && tx.fee == Some(l.fee)
            && tx.memo == Some(l.memo.to_vec())
            && tx.created_at_time == Some(l.created_at_time),
        Error::IntegrityFailed,
    )?;
    complete(escrow_id, leg_id, block)
}
#[ic_cdk::query]
fn get_escrow(escrow_id: Hash) -> Result<Escrow> {
    load(&escrow_id)
}
#[ic_cdk::query]
fn get_escrow_by_operation(payer: Principal, op_id: Hash) -> Result<Escrow> {
    load(&digest("dmsg/escrow-id/v1", &(me(), payer, op_id)))
}
#[ic_cdk::query]
fn get_escrow_certified(ids: Vec<Hash>) -> Result<CertifiedBatch> {
    CERT.with_borrow(|c| c.batch(me(), ids.into_iter().map(|v| v.to_vec()).collect()))
}
#[ic_cdk::query]
fn get_transfer(escrow_id: Hash, leg_id: u64) -> Result<TransferLeg> {
    get_leg(escrow_id, leg_id)
}
#[ic_cdk::query]
fn get_deposit(escrow_id: Hash, block: u64) -> Option<Deposit> {
    DEPOSITS.with_borrow(|t| t.get(&key(escrow_id, block)))
}
#[ic_cdk::query]
fn get_receipt_signer(epoch: u64) -> Result<ReceiptSigner> {
    signer(epoch)
}
#[ic_cdk::query]
fn list_my_escrows(after: Option<Hash>) -> Result<Vec<Escrow>> {
    authenticated(caller())?;
    let prefix = digest("dmsg/payer-index/v1", &caller());
    let cursor = after.map_or_else(
        || prefix.to_vec(),
        |id| [prefix.as_slice(), id.as_slice()].concat(),
    );
    let entries = PAYER_INDEX.with_borrow(|t| t.page::<Hash>(cursor, 32));
    entries
        .into_iter()
        .take_while(|(key, _)| key.starts_with(&prefix))
        .map(|(_, id)| load(&id))
        .collect()
}
#[ic_cdk::query]
fn list_transfers(escrow_id: Hash, after: Option<u64>) -> Result<Vec<TransferLeg>> {
    load(&escrow_id)?;
    let cursor = after.map_or_else(|| escrow_id.to_vec(), |n| key(escrow_id, n));
    Ok(LEGS
        .with_borrow(|t| t.page::<TransferLeg>(cursor, 32))
        .into_iter()
        .take_while(|(key, _)| key.starts_with(&escrow_id))
        .map(|(_, leg)| leg)
        .collect())
}
ic_cdk::export_candid!();
