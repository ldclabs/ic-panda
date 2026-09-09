use candid::Principal;
use dmsg_types::{ledger::VerifiedTransfer, payment::*, *};
use icrc_ledger_types::icrc1::account::Account;

pub fn validate_quote(
    config: &PaymentInit,
    id: Principal,
    payer: Principal,
    input: &OpenEscrow,
    signer: &ReceiptSigner,
    now: u64,
) -> Result<Hash> {
    authenticated(payer)?;
    nonzero(&input.op_id)?;
    let q = &input.quote;
    let o = &input.offer.offer;
    ensure(config.enabled, Error::Locked)?;
    ensure(q.max_network_fee == config.max_fee, Error::IntegrityFailed)?;
    ensure(
        q.home_payment == id
            && q.payer.owner == payer
            && q.ledger == config.ledger
            && q.platform == config.platform
            && q.service_fee == config.service_fee,
        Error::IntegrityFailed,
    )?;
    ensure(
        o.home_payment == id
            && o.ledger == q.ledger
            && o.recipient == q.recipient
            && o.recipient_net == q.recipient_net
            && o.quote_scope == q.quote_scope
            && digest("dmsg/payment-offer/v1", o) == q.offer_digest,
        Error::IntegrityFailed,
    )?;
    authenticated(q.recipient.owner)?;
    ensure(
        q.recipient.owner != id && q.platform.owner != id && q.payer.owner != id,
        invalid("escrow cannot pay itself"),
    )?;
    ensure(
        q.recipient_net > 0
            && q.fee_reserve
                >= config
                    .ledger_fee
                    .checked_mul(3)
                    .ok_or(Error::QuotaExceeded)?
            && q.fee_reserve <= config.max_fee.checked_mul(3).ok_or(Error::QuotaExceeded)?,
        Error::FeeBlocked,
    )?;
    ensure(
        q.recipient_net
            .checked_add(q.service_fee)
            .and_then(|v| v.checked_add(q.fee_reserve))
            == Some(q.amount),
        Error::IntegrityFailed,
    )?;
    ensure(
        q.created_at <= now
            && q.fund_by
                == q.created_at
                    .checked_add(15 * MINUTE)
                    .ok_or(Error::Expired)?
            && q.accept_by == q.fund_by.checked_add(30 * MINUTE).ok_or(Error::Expired)?
            && now < q.fund_by,
        Error::Expired,
    )?;
    ensure(
        q.max_bytes > 0 && q.max_bytes <= 8192 && q.retain_ms >= DAY && q.retain_ms <= 365 * DAY,
        invalid("storage terms"),
    )?;
    ensure(o.issued_at <= now && now < o.expires_at, Error::Expired)?;
    signer_valid(signer, q.signer_epoch, q.created_at, now)?;
    nonzero(&q.quote_id)?;
    nonzero(&q.envelope_digest)?;
    let hash = digest("dmsg/quote/v1", q);
    verify(&signer.public_key, hash.as_slice(), &input.quote_signature)?;
    Ok(hash)
}
pub fn signer_valid(s: &ReceiptSigner, epoch: u64, signed_at: u64, now: u64) -> Result<()> {
    ensure(
        !s.revoked
            && s.epoch == epoch
            && s.valid_from <= signed_at
            && signed_at < s.valid_until
            && signed_at <= now,
        Error::Forbidden,
    )
}
pub fn escrow(id: Principal, payer: Principal, input: &OpenEscrow, quote_digest: Hash) -> Escrow {
    let escrow_id = digest("dmsg/escrow-id/v1", &(id, payer, input.op_id));
    Escrow {
        escrow_id,
        payer_principal: payer,
        op_id: input.op_id,
        quote: input.quote.clone(),
        quote_digest,
        subaccount: digest("dmsg/escrow-subaccount/v1", &(id, escrow_id)),
        funding_ref: None,
        funded_at: None,
        decision: FundsDecision::Pending,
        receipt_digest: None,
        version: 0,
        confirmed_in: 0,
        liabilities: 0,
        transferred: 0,
        network_fees: 0,
        primary_remaining: 0,
        next_leg: 0,
        pending_payouts: 0,
    }
}
pub fn accept_deposit(e: &mut Escrow, id: Principal, tx: &VerifiedTransfer) -> Result<Deposit> {
    ensure(
        tx.to
            == Account {
                owner: id,
                subaccount: Some(e.subaccount.into_array()),
            }
            && tx.amount > 0
            && tx.from.owner != id,
        Error::IntegrityFailed,
    )?;
    let total = e
        .confirmed_in
        .checked_add(tx.amount)
        .ok_or(Error::QuotaExceeded)?;
    let liabilities = e
        .liabilities
        .checked_add(tx.amount)
        .ok_or(Error::QuotaExceeded)?;
    let primary = e.decision == FundsDecision::Pending
        && e.funding_ref.is_none()
        && tx.committed_at < e.quote.fund_by
        && tx.from == e.quote.payer
        && tx.amount >= e.quote.amount;
    let refundable = if primary {
        e.funding_ref = Some(tx.block);
        e.funded_at = Some(tx.committed_at);
        e.primary_remaining = e.quote.amount;
        tx.amount - e.quote.amount
    } else {
        tx.amount
    };
    e.confirmed_in = total;
    e.liabilities = liabilities;
    e.version += 1;
    debug_assert!(e.conserved());
    Ok(Deposit {
        block: tx.block,
        from: tx.from,
        amount: tx.amount,
        committed_at: tx.committed_at,
        refundable,
    })
}
pub fn receipt_valid(
    e: &Escrow,
    r: &SignedReceipt,
    s: &ReceiptSigner,
    id: Principal,
    now: u64,
) -> Result<Hash> {
    let a = &r.receipt;
    ensure(
        a.protocol == 1
            && a.escrow_id == e.escrow_id
            && a.home_payment == id
            && a.quote_digest == e.quote_digest
            && a.envelope_digest == e.quote.envelope_digest
            && a.signer_epoch == e.quote.signer_epoch
            && a.fund_by == e.quote.fund_by
            && a.accept_by == e.quote.accept_by,
        Error::IntegrityFailed,
    )?;
    ensure(
        a.size <= e.quote.max_bytes
            && a.size > 0
            && a.stored_at >= e.quote.created_at
            && a.stored_at <= now
            && a.retain_until
                >= a.stored_at
                    .checked_add(e.quote.retain_ms)
                    .ok_or(Error::IntegrityFailed)?,
        Error::IntegrityFailed,
    )?;
    signer_valid(s, a.signer_epoch, a.stored_at, now)?;
    let hash = digest("dmsg/admission-receipt/v1", a);
    verify(&s.public_key, hash.as_slice(), &r.signature)?;
    Ok(hash)
}
pub fn settle(e: &mut Escrow, receipt_digest: Hash, now: u64) -> Result<bool> {
    if e.decision == FundsDecision::SettlementCommitted {
        ensure(
            e.receipt_digest == Some(receipt_digest),
            Error::IdempotencyConflict,
        )?;
        return Ok(false);
    }
    ensure(e.decision == FundsDecision::Pending, Error::VersionConflict)?;
    ensure(now < e.quote.accept_by, Error::Expired)?;
    ensure(e.funding_ref.is_some(), Error::Pending)?;
    e.decision = FundsDecision::SettlementCommitted;
    e.receipt_digest = Some(receipt_digest);
    e.version += 1;
    Ok(true)
}
pub fn refund(e: &mut Escrow, now: u64) -> Result<bool> {
    if e.decision == FundsDecision::RefundCommitted {
        return Ok(false);
    }
    ensure(e.decision == FundsDecision::Pending, Error::VersionConflict)?;
    ensure(now >= e.quote.accept_by, Error::Expired)?;
    e.decision = FundsDecision::RefundCommitted;
    e.version += 1;
    Ok(true)
}
pub fn leg(
    e: &mut Escrow,
    kind: LegKind,
    to: Account,
    amount: u128,
    fee: u128,
    now: u64,
) -> TransferLeg {
    let leg_id = e.next_leg;
    e.next_leg = e.next_leg.checked_add(1).expect("leg limit");
    TransferLeg {
        escrow_id: e.escrow_id,
        leg_id,
        kind,
        to,
        amount,
        fee,
        memo: digest("dmsg/payment-leg/v1", &(e.escrow_id, leg_id)),
        created_at_time: millis_to_nanos(now).expect("consensus time fits ledger timestamp"),
        status: LegStatus::Pending,
        block: None,
        expected_fee: None,
        revision: 0,
        replaces: None,
        history_digest: Hash::new([0; 32]),
    }
}
pub fn revise_leg(
    e: &mut Escrow,
    old: &mut TransferLeg,
    caller: Principal,
    fee: u128,
    now: u64,
) -> Result<TransferLeg> {
    authenticated(caller)?;
    ensure(
        caller == e.payer_principal || caller == old.to.owner,
        Error::Forbidden,
    )?;
    ensure(
        matches!(old.status, LegStatus::FeeBlocked | LegStatus::Rejected),
        Error::ExecutionUnknown,
    )?;
    ensure(fee <= e.quote.max_network_fee, Error::FeeBlocked)?;
    // Only a verified BadFee reply may change the fee. Other clean rejections
    // can refresh the timestamp while preserving the approved amount and fee.
    let expected = if old.status == LegStatus::FeeBlocked {
        old.expected_fee.ok_or(Error::FeeBlocked)?
    } else {
        old.fee
    };
    ensure(fee == expected, Error::FeeBlocked)?;
    let revision = old.revision.checked_add(1).ok_or(Error::QuotaExceeded)?;
    let mut next = e.clone();
    let amount = match old.kind {
        LegKind::Recipient | LegKind::Platform => {
            let available = next
                .primary_remaining
                .checked_add(old.fee)
                .ok_or(Error::FeeBlocked)?;
            ensure(available >= fee, Error::FeeBlocked)?;
            next.primary_remaining = available - fee;
            old.amount
        }
        _ => old
            .amount
            .checked_add(old.fee)
            .and_then(|total| total.checked_sub(fee))
            .filter(|amount| *amount > 0)
            .ok_or(Error::FeeBlocked)?,
    };
    let mut new = leg(&mut next, old.kind.clone(), old.to, amount, fee, now);
    old.status = LegStatus::Superseded;
    new.revision = revision;
    new.replaces = Some(old.leg_id);
    new.history_digest = digest("dmsg/transfer-history/v1", old);
    *e = next;
    Ok(new)
}
pub fn complete_leg(e: &mut Escrow, leg: &mut TransferLeg, block: u64) -> Result<()> {
    if leg.status == LegStatus::Succeeded {
        ensure(leg.block == Some(block), Error::IntegrityFailed)?;
        return Ok(());
    }
    let debit = leg
        .amount
        .checked_add(leg.fee)
        .ok_or(Error::IntegrityFailed)?;
    ensure(e.liabilities >= debit, Error::IntegrityFailed)?;
    e.liabilities -= debit;
    e.transferred = e
        .transferred
        .checked_add(leg.amount)
        .ok_or(Error::IntegrityFailed)?;
    e.network_fees = e
        .network_fees
        .checked_add(leg.fee)
        .ok_or(Error::IntegrityFailed)?;
    leg.status = LegStatus::Succeeded;
    leg.block = Some(block);
    e.version += 1;
    if matches!(leg.kind, LegKind::Recipient | LegKind::Platform) {
        e.pending_payouts = e
            .pending_payouts
            .checked_sub(1)
            .ok_or(Error::IntegrityFailed)?;
    }
    ensure(e.conserved(), Error::IntegrityFailed)
}

#[cfg(test)]
mod tests;
