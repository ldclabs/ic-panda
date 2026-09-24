//! Ledger-isolated merchant book. All methods mutate candidates; storage commits occur in the API.
use candid::Principal;
use dmsg_protocol::{commerce_v2::*, integration::*, *};
use dmsg_types::{integration::*, integration_billing::*, *};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Balance {
    pub incoming: u128,
    pub refundable: u128,
    pub service: u128,
    pub fees: u128,
    pub outgoing: u128,
}

impl Balance {
    pub fn conserved(&self) -> bool {
        self.refundable
            .checked_add(self.service)
            .and_then(|v| v.checked_add(self.fees))
            .and_then(|v| v.checked_add(self.outgoing))
            == Some(self.incoming)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    pub id: Hash,
    pub input: OpenCheckout,
    pub status: CheckoutStatus,
    pub balances: BTreeMap<Principal, Balance>,
    pub funding: Option<CashBlock>,
    pub decision: Option<ProductDecision>,
    pub receipt: Option<ProductReceipt>,
    pub cancellation: Option<CashCancellationReceipt>,
    pub cancellation_pending: bool,
    pub reservation_released: bool,
    pub next_transfer: u64,
    pub earned_allocated: u128,
    pub generation: u64,
    pub busy_until_ms: u64,
}

impl Order {
    pub fn new(home: Principal, input: OpenCheckout) -> Self {
        Self {
            id: checkout_id(home, &input.quote.offer),
            input,
            status: CheckoutStatus::Reserving,
            balances: BTreeMap::new(),
            funding: None,
            decision: None,
            receipt: None,
            cancellation: None,
            cancellation_pending: false,
            reservation_released: false,
            next_transfer: 0,
            earned_allocated: 0,
            generation: 0,
            busy_until_ms: 0,
        }
    }

    pub fn progress(&self) -> CheckoutProgress {
        CheckoutProgress {
            order_id: self.id,
            status: self.status.clone(),
            decision_id: self.decision.as_ref().map(|d| d.decision_id),
        }
    }

    pub fn view(&self) -> CheckoutView {
        let balance = self
            .balances
            .get(&self.input.quote.cash.ledger)
            .cloned()
            .unwrap_or_default();
        CheckoutView {
            progress: self.progress(),
            quote: self.input.quote.clone(),
            receipt: self.receipt.clone(),
            outgoing_atomic: balance.outgoing,
            service_reserve_atomic: balance.service,
            fee_reserve_atomic: balance.fees,
        }
    }

    pub fn conserved(&self) -> bool {
        self.balances.values().all(Balance::conserved)
    }

    /// Record actual money first. A valid one-shot payment freezes the decision before any await.
    pub fn deposit(
        &mut self,
        ledger: Principal,
        tx: &dmsg_runtime::ledger::VerifiedTransfer,
        at: u64,
    ) -> Result<CheckoutDeposit> {
        let quote = &self.input.quote;
        ensure(
            tx.to == quote.cash.deposit && tx.committed_at <= at,
            Error::IntegrityFailed,
        )?;
        let mut balance = self.balances.get(&ledger).cloned().unwrap_or_default();
        balance.incoming = balance
            .incoming
            .checked_add(tx.amount)
            .ok_or(Error::QuotaExceeded)?;
        let mut deposit = CheckoutDeposit {
            order_id: self.id,
            block: CashBlock {
                ledger,
                block_index: u128::from(tx.block),
            },
            from: tx.from,
            amount_atomic: tx.amount,
            refundable_atomic: tx.amount,
        };
        let total = quote
            .cash
            .amount_atomic
            .checked_add(quote.cash.fee_reserve_atomic)
            .ok_or(Error::QuotaExceeded)?;
        let apply = self.status == CheckoutStatus::AwaitingFunding
            && ledger == quote.cash.ledger
            && tx.from == quote.cash.payer
            && tx.amount >= total
            && tx.committed_at >= quote.quoted_at_ms
            && tx.committed_at < quote.cash.funding_deadline_ms
            && at < quote.cash.activation_deadline_ms;
        if apply {
            balance.service = quote.cash.amount_atomic;
            balance.fees = quote.cash.fee_reserve_atomic;
            deposit.refundable_atomic -= total;
        }
        balance.refundable = balance
            .refundable
            .checked_add(deposit.refundable_atomic)
            .ok_or(Error::QuotaExceeded)?;
        ensure(balance.conserved(), Error::IntegrityFailed)?;
        if apply {
            self.funding = Some(deposit.block.clone());
            self.decision = Some(ProductDecision {
                version: 2,
                offer: quote.offer.clone(),
                decision_id: digest("dmsg/checkout/apply/v2", &self.id),
                source: SettlementSource::Cash {
                    order_id: self.id,
                    ledger,
                    block_index: u128::from(tx.block),
                    amount_atomic: quote.cash.amount_atomic,
                },
                decided_at_ms: at,
                apply_by_ms: quote.cash.activation_deadline_ms,
            });
            self.status = CheckoutStatus::Applying;
        } else if self.status == CheckoutStatus::AwaitingFunding
            && at >= quote.cash.activation_deadline_ms
        {
            self.status = CheckoutStatus::RefundCommitted;
        }
        self.balances.insert(ledger, balance);
        Ok(deposit)
    }

    pub fn accept(&mut self, receipt: ProductReceipt) -> Result<()> {
        if let Some(old) = &self.receipt {
            return ensure(*old == receipt, Error::IdempotencyConflict);
        }
        let decision = self.decision.as_ref().ok_or(Error::IntegrityFailed)?;
        match_product_receipt(&receipt, decision)?;
        ensure(
            self.status == CheckoutStatus::Applying,
            Error::VersionConflict,
        )?;
        self.status = if matches!(receipt.outcome, ProductOutcome::Applied { .. }) {
            CheckoutStatus::Applied
        } else {
            CheckoutStatus::RefundCommitted
        };
        self.receipt = Some(receipt);
        Ok(())
    }

    /// Returns the amount added back to the original funding deposit's refundable obligation.
    pub fn refund_price(&mut self) -> Result<u128> {
        ensure(
            self.status == CheckoutStatus::RefundCommitted,
            Error::VersionConflict,
        )?;
        let Some(balance) = self.balances.get_mut(&self.input.quote.cash.ledger) else {
            return Ok(0);
        };
        let total = balance
            .service
            .checked_add(balance.fees)
            .ok_or(Error::QuotaExceeded)?;
        let refundable = balance
            .refundable
            .checked_add(total)
            .ok_or(Error::QuotaExceeded)?;
        balance.service = 0;
        balance.fees = 0;
        balance.refundable = refundable;
        Ok(total)
    }

    pub fn earned(&self, at: u64) -> Result<u128> {
        ensure(
            self.status == CheckoutStatus::Applied && !self.cancellation_pending,
            Error::Pending,
        )?;
        let receipt = self.receipt.as_ref().ok_or(Error::IntegrityFailed)?;
        earned_atomic(
            self.input.quote.cash.amount_atomic,
            &self.input.quote.offer,
            receipt.applied_at_ms,
            at,
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transfer {
    pub view: CashTransfer,
    pub dispatched_at_ms: u64,
}

/// Freeze one outgoing obligation. Revisions replace an unexecuted leg, never allocate twice.
pub fn transfer(
    order: &mut Order,
    ledger: Principal,
    to: icrc_ledger_types::icrc1::account::Account,
    debit: u128,
    fee: u128,
    max_fee: u128,
    at: u64,
) -> Result<Transfer> {
    ensure(fee > 0 && fee <= max_fee && debit > fee, Error::FeeBlocked)?;
    let next = order
        .next_transfer
        .checked_add(1)
        .ok_or(Error::QuotaExceeded)?;
    let id = digest(
        "dmsg/checkout/transfer/v2",
        &(order.id, order.next_transfer),
    );
    let source_subaccount = Hash::new(
        order
            .input
            .quote
            .cash
            .deposit
            .subaccount
            .ok_or(Error::IntegrityFailed)?,
    );
    let balance = order.balances.get_mut(&ledger).ok_or(Error::NotFound)?;
    let outgoing = balance
        .outgoing
        .checked_add(debit)
        .ok_or(Error::QuotaExceeded)?;
    let created_at_time_ns = millis_to_nanos(at)?;
    balance.outgoing = outgoing;
    order.next_transfer = next;
    Ok(Transfer {
        view: CashTransfer {
            transfer_id: id,
            order_id: order.id,
            ledger,
            source_subaccount,
            to,
            amount_atomic: debit - fee,
            fee_atomic: fee,
            max_fee_atomic: max_fee,
            memo: id,
            created_at_time_ns,
            status: CashTransferStatus::Pending,
            block_index: None,
            expected_fee_atomic: None,
            error_code: None,
            replaces: None,
            replaced_by: None,
        },
        dispatched_at_ms: 0,
    })
}

#[cfg(test)]
#[path = "../../dmsg_types/tests/support/commerce.rs"]
pub(crate) mod fixture;
#[cfg(test)]
mod tests {
    use super::*;
    use fixture::base::*;
    fn tx(
        order: &Order,
        from: u8,
        amount: u128,
        at: u64,
    ) -> dmsg_runtime::ledger::VerifiedTransfer {
        dmsg_runtime::ledger::VerifiedTransfer {
            block: 7,
            from: principal(from).into(),
            to: order.input.quote.cash.deposit,
            amount,
            fee: Some(10),
            committed_at: at,
            memo: None,
            created_at_time: Some(at * 1_000_000),
            spender: None,
        }
    }

    fn order() -> Order {
        let mut o = Order::new(principal(8), fixture::open());
        o.status = CheckoutStatus::AwaitingFunding;
        o
    }

    #[test]
    fn wrong_ledger_source_partial_and_excess_remain_isolated_refund_obligations() {
        let mut o = order();
        let total = o.input.quote.cash.amount_atomic + o.input.quote.cash.fee_reserve_atomic;
        for (ledger, from, amount) in [(7, 9, total), (6, 55, total), (6, 9, 100)] {
            let d = o
                .deposit(principal(ledger), &tx(&o, from, amount, NOW), NOW)
                .unwrap();
            assert_eq!(d.refundable_atomic, amount);
            assert_eq!(d.from, principal(from).into());
            assert_eq!(o.status, CheckoutStatus::AwaitingFunding);
            assert!(o.conserved());
        }
        let d = o
            .deposit(principal(6), &tx(&o, 9, total + 20, NOW), NOW)
            .unwrap();
        assert_eq!(d.refundable_atomic, 20);
        assert_eq!(o.status, CheckoutStatus::Applying);
        assert!(o.conserved());
        assert_eq!(o.balances[&principal(7)].service, 0);
        let d = o
            .deposit(principal(6), &tx(&o, 9, total, NOW), NOW)
            .unwrap();
        assert_eq!(d.refundable_atomic, total);
        assert!(o.conserved());
    }

    #[test]
    fn late_funding_and_definite_nondelivery_refund_without_allocating_revenue() {
        let mut o = order();
        let total = o.input.quote.cash.amount_atomic + o.input.quote.cash.fee_reserve_atomic;
        let d = o
            .deposit(principal(6), &tx(&o, 9, total, NOW + DAY), NOW + DAY)
            .unwrap();
        assert_eq!(d.refundable_atomic, total);
        assert_eq!(o.status, CheckoutStatus::RefundCommitted);
        assert!(o.earned(NOW + 2 * DAY).is_err());
        let mut o = order();
        o.deposit(principal(6), &tx(&o, 9, total, NOW), NOW)
            .unwrap();
        let d = o.decision.clone().unwrap();
        let r = dmsg_protocol::product_book::rejected(&d, ProductRejection::Unauthorized, NOW);
        o.accept(r).unwrap();
        assert_eq!(o.refund_price().unwrap(), total);
        assert_eq!(o.refund_price().unwrap(), 0);
        assert!(o.conserved());
    }

    #[test]
    fn applied_receipt_is_idempotent_and_revenue_starts_only_at_delivery_and_term_start() {
        let mut o = order();
        let total = o.input.quote.cash.amount_atomic + o.input.quote.cash.fee_reserve_atomic;
        o.deposit(principal(6), &tx(&o, 9, total, NOW), NOW)
            .unwrap();
        let d = o.decision.as_ref().unwrap();
        let r = ProductReceipt {
            version: 2,
            decision_id: d.decision_id,
            decision_hash: product_decision_hash(d),
            adapter: d.offer.adapter,
            outcome: ProductOutcome::Applied {
                business_revision: 10,
                contract_id: Hash::new([88; 32]),
                committed_until_ms: d.offer.expires_at_ms,
            },
            applied_at_ms: NOW,
        };
        o.accept(r.clone()).unwrap();
        o.accept(r).unwrap();
        assert_eq!(o.earned(NOW).unwrap(), 0);
        assert_eq!(
            o.earned(o.input.quote.offer.expires_at_ms).unwrap(),
            o.input.quote.cash.amount_atomic
        );
        assert!(o.refund_price().is_err());
    }
}
