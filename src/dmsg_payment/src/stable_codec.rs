use crate::{state::Escrow, store::Config};
use cbor2::Cbor;
use dmsg_runtime::{stable_types::*, storage::StableCodec};
use dmsg_types::{payment::*, profiles::delivery::Quote, *};

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct ConfigRepr {
    #[cbor(key = 1)]
    pub schema: u16,
    #[cbor(key = 2)]
    pub init: PaymentInitRepr,
    #[cbor(key = 3)]
    pub day: u64,
    #[cbor(key = 4)]
    pub orders_today: u32,
    #[cbor(key = 5)]
    pub ledger_minute: u64,
    #[cbor(key = 6)]
    pub ledger_reads: u32,
}

impl StableCodec for Config {
    type Repr = ConfigRepr;

    fn to_repr(&self) -> Self::Repr {
        ConfigRepr {
            schema: self.schema,
            init: self.init.to_repr(),
            day: self.day,
            orders_today: self.orders_today,
            ledger_minute: self.ledger_minute,
            ledger_reads: self.ledger_reads,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            schema: repr.schema,
            init: PaymentInit::from_repr(repr.init),
            day: repr.day,
            orders_today: repr.orders_today,
            ledger_minute: repr.ledger_minute,
            ledger_reads: repr.ledger_reads,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub struct EscrowRepr {
    #[cbor(key = 1)]
    pub escrow_id: Hash,
    #[cbor(key = 2)]
    pub payer_principal: candid::Principal,
    #[cbor(key = 3)]
    pub op_id: OpId,
    #[cbor(key = 4)]
    pub quote: QuoteRepr,
    #[cbor(key = 5)]
    pub quote_digest: Hash,
    #[cbor(key = 6)]
    pub subaccount: Hash,
    #[cbor(key = 7)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub funding_ref: Option<u64>,
    #[cbor(key = 8)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub funded_at: Option<u64>,
    #[cbor(key = 9)]
    pub decision: FundsDecision,
    #[cbor(key = 10)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_digest: Option<Hash>,
    #[cbor(key = 11)]
    pub version: u64,
    #[cbor(key = 12)]
    pub confirmed_in: u128,
    #[cbor(key = 13)]
    pub liabilities: u128,
    #[cbor(key = 14)]
    pub transferred: u128,
    #[cbor(key = 15)]
    pub network_fees: u128,
    #[cbor(key = 16)]
    pub primary_remaining: u128,
    #[cbor(key = 17)]
    pub next_leg: u64,
    #[cbor(key = 18)]
    pub pending_payouts: u32,
}

impl StableCodec for Escrow {
    type Repr = EscrowRepr;

    fn to_repr(&self) -> Self::Repr {
        EscrowRepr {
            escrow_id: self.escrow_id,
            payer_principal: self.payer_principal,
            op_id: self.op_id,
            quote: self.quote.to_repr(),
            quote_digest: self.quote_digest,
            subaccount: self.subaccount,
            funding_ref: self.funding_ref,
            funded_at: self.funded_at,
            decision: self.decision.clone(),
            receipt_digest: self.receipt_digest,
            version: self.version,
            confirmed_in: self.confirmed_in,
            liabilities: self.liabilities,
            transferred: self.transferred,
            network_fees: self.network_fees,
            primary_remaining: self.primary_remaining,
            next_leg: self.next_leg,
            pending_payouts: self.pending_payouts,
        }
    }

    fn from_repr(repr: Self::Repr) -> Self {
        Self {
            escrow_id: repr.escrow_id,
            payer_principal: repr.payer_principal,
            op_id: repr.op_id,
            quote: Quote::from_repr(repr.quote),
            quote_digest: repr.quote_digest,
            subaccount: repr.subaccount,
            funding_ref: repr.funding_ref,
            funded_at: repr.funded_at,
            decision: repr.decision,
            receipt_digest: repr.receipt_digest,
            version: repr.version,
            confirmed_in: repr.confirmed_in,
            liabilities: repr.liabilities,
            transferred: repr.transferred,
            network_fees: repr.network_fees,
            primary_remaining: repr.primary_remaining,
            next_leg: repr.next_leg,
            pending_payouts: repr.pending_payouts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use candid::Principal;
    use dmsg_runtime::storage::{compact_bytes, compact_from_bytes};
    use icrc_ledger_types::icrc1::account::Account;

    fn p(n: u8) -> Principal {
        Principal::from_slice(&[n, 1])
    }

    fn account(n: u8) -> Account {
        Account {
            owner: p(n),
            subaccount: Some([n; 32]),
        }
    }

    fn quote() -> Quote {
        Quote {
            quote_id: Hash::new([1; 32]),
            home_payment: p(5),
            payer: account(1),
            offer_digest: Hash::new([2; 32]),
            quote_scope: Hash::new([3; 32]),
            ledger: p(6),
            recipient: account(2),
            recipient_net: 1_000,
            platform: account(3),
            service_fee: 100,
            fee_reserve: 30,
            amount: 1_130,
            max_network_fee: 20,
            max_bytes: 8_192,
            retain_ms: DAY,
            envelope_digest: Hash::new([5; 32]),
            signer_epoch: 1,
            created_at: 1_700_000_000_000,
            fund_by: 1_700_000_900_000,
            accept_by: 1_700_002_700_000,
        }
    }

    fn escrow() -> Escrow {
        Escrow {
            escrow_id: Hash::new([6; 32]),
            payer_principal: account(1).owner,
            op_id: Hash::new([7; 32]),
            quote: quote(),
            quote_digest: Hash::new([8; 32]),
            subaccount: Hash::new([9; 32]),
            funding_ref: Some(42),
            funded_at: Some(1_700_000_000_010),
            decision: FundsDecision::SettlementCommitted,
            receipt_digest: Some(Hash::new([10; 32])),
            version: 4,
            confirmed_in: 1_130,
            liabilities: 1_130,
            transferred: 0,
            network_fees: 0,
            primary_remaining: 1_130,
            next_leg: 2,
            pending_payouts: 2,
        }
    }

    fn assert_integer_top_keys(bytes: &[u8], count: usize) {
        let cbor2::Value::Map(entries) = cbor2::from_slice(bytes).unwrap() else {
            panic!("stable record must be a map")
        };
        assert_eq!(entries.len(), count);
        assert!(entries
            .iter()
            .all(|(key, _)| matches!(key, cbor2::Value::Integer(_))));
    }

    fn assert_public_text_keys<T: serde::Serialize>(value: &T) {
        let cbor2::Value::Map(entries) = cbor2::from_slice(&cbor2::to_vec(value).unwrap()).unwrap()
        else {
            panic!("public record must be a map")
        };
        assert!(entries
            .iter()
            .all(|(key, _)| matches!(key, cbor2::Value::Text(_))));
    }

    #[test]
    fn payment_records_round_trip_and_remain_conserved() {
        let escrow = escrow();
        assert_public_text_keys(&escrow.quote);
        let escrow_bytes = compact_bytes(&escrow);
        assert_eq!(escrow_bytes.len(), 567);
        assert_eq!(
            hex(&escrow_bytes),
            "9a9f7bb9bf59608337c26f18a8ac93e13a36cc8de9afbe1d5f04af1298f0388c"
        );
        assert_integer_top_keys(&escrow_bytes, 18);
        let decoded = compact_from_bytes::<Escrow>(&escrow_bytes);
        assert_eq!(decoded, escrow);
        assert!(decoded.conserved());
        assert!(escrow_bytes.len() * 100 <= cbor2::to_vec(&escrow).unwrap().len() * 60);

        let deposit = Deposit {
            block: 42,
            from: account(1),
            amount: 1_130,
            committed_at: 1_700_000_000_010,
            refundable: 0,
        };
        let deposit_bytes = compact_bytes(&deposit);
        assert_integer_top_keys(&deposit_bytes, 5);
        assert_eq!(compact_from_bytes::<Deposit>(&deposit_bytes), deposit);

        let leg = TransferLeg {
            escrow_id: escrow.escrow_id,
            leg_id: 1,
            kind: LegKind::Refund { funding_block: 42 },
            to: account(1),
            amount: 1_100,
            fee: 10,
            memo: Hash::new([11; 32]),
            created_at_time: 1_700_000_000_000_000_000,
            status: LegStatus::FeeBlocked,
            block: None,
            expected_fee: Some(11),
            revision: 1,
            replaces: Some(0),
            history_digest: Hash::new([12; 32]),
        };
        let leg_bytes = compact_bytes(&leg);
        assert_integer_top_keys(&leg_bytes, 13);
        assert_eq!(compact_from_bytes::<TransferLeg>(&leg_bytes), leg);
        assert!(leg_bytes.len() * 100 <= cbor2::to_vec(&leg).unwrap().len() * 60);

        for kind in [
            LegKind::Recipient,
            LegKind::Platform,
            LegKind::Refund { funding_block: 9 },
            LegKind::ReserveRefund,
        ] {
            let mut variant = leg.clone();
            variant.kind = kind;
            assert_eq!(
                compact_from_bytes::<TransferLeg>(&compact_bytes(&variant)),
                variant
            );
        }

        let mut sparse_leg = leg;
        sparse_leg.block = None;
        sparse_leg.expected_fee = None;
        sparse_leg.replaces = None;
        assert_eq!(
            compact_from_bytes::<TransferLeg>(&compact_bytes(&sparse_leg)),
            sparse_leg
        );

        let mut pending = escrow;
        pending.funding_ref = None;
        pending.funded_at = None;
        pending.decision = FundsDecision::Pending;
        pending.receipt_digest = None;
        assert_eq!(
            compact_from_bytes::<Escrow>(&compact_bytes(&pending)),
            pending
        );
    }

    fn hex(bytes: &[u8]) -> String {
        let digest = dmsg_protocol::sha256(bytes);
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn payment_config_and_signer_round_trip() {
        let signer = ReceiptSigner {
            epoch: 1,
            public_key: Hash::new([13; 32]),
            valid_from: 1,
            valid_until: DAY,
            revoked: false,
        };
        assert_eq!(
            compact_from_bytes::<ReceiptSigner>(&compact_bytes(&signer)),
            signer
        );

        let config = Config {
            schema: 3,
            init: PaymentInit {
                home_user: p(1),
                ledger: p(2),
                platform: account(3),
                service_fee: 100,
                ledger_fee: 10,
                max_fee: 20,
                signer,
                max_open_per_payer: 16,
                daily_orders: 100_000,
                enabled: true,
            },
            day: 42,
            orders_today: 7,
            ledger_minute: 123,
            ledger_reads: 4,
        };
        let decoded = compact_from_bytes::<Config>(&compact_bytes(&config));
        assert_eq!(decoded.schema, config.schema);
        assert_eq!(decoded.init, config.init);
        assert_eq!(decoded.day, config.day);
        assert_eq!(decoded.orders_today, config.orders_today);
        assert_eq!(decoded.ledger_minute, config.ledger_minute);
        assert_eq!(decoded.ledger_reads, config.ledger_reads);
    }
}
