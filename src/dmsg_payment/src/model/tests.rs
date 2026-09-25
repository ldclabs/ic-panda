use super::*;
use ed25519_dalek::{Signer, SigningKey};

fn account(n: u8) -> Account {
    Account {
        owner: Principal::from_slice(&[n]),
        subaccount: None,
    }
}

fn input() -> OpenEscrow {
    let o = PaymentOffer {
        account_id: AccountId([1; 12]),
        device_id: Hash::new([1; 32]),
        security_epoch: 0,
        home_payment: account(5).owner,
        offer_id: Hash::new([3; 32]),
        ledger: account(6).owner,
        recipient: account(2),
        recipient_net: 1000,
        quote_scope: Hash::new([2; 32]),
        version: 1,
        issued_at: 1,
        expires_at: 15 * MINUTE,
    };
    let q = Quote {
        fee_policy_version: 1,
        quote_id: Hash::new([1; 32]),
        home_payment: o.home_payment,
        payer: account(1),
        offer_digest: digest("dmsg/payment-offer/v1", &o),
        quote_scope: o.quote_scope,
        ledger: o.ledger,
        recipient: o.recipient,
        recipient_net: o.recipient_net,
        platform: account(3),
        service_fee: 100,
        fee_reserve: 30,
        amount: 1130,
        max_network_fee: 20,
        max_bytes: 8192,
        retain_ms: DAY,
        envelope_digest: Hash::new([5; 32]),
        signer_epoch: 1,
        created_at: 1,
        fund_by: 1 + 15 * MINUTE,
        accept_by: 1 + 45 * MINUTE,
    };
    let key = SigningKey::from_bytes(&[7; 32]);
    let quote_signature = key
        .sign(digest("dmsg/quote/v2", &q).as_slice())
        .to_bytes()
        .into();
    OpenEscrow {
        op_id: Hash::new([1; 32]),
        quote: q,
        quote_signature,
        offer: SignedOffer {
            offer: o,
            signature: Default::default(),
        },
    }
}

fn setup() -> (Escrow, Principal) {
    let i = input();
    let me = i.quote.home_payment;
    let payer = i.quote.payer.owner;
    let escrow_id = digest("dmsg/escrow-id/v1", &(me, payer, i.op_id));
    (
        escrow(me, escrow_id, payer, &i, digest("dmsg/quote/v2", &i.quote)),
        me,
    )
}

fn transfer(
    e: &Escrow,
    me: Principal,
    block: u64,
    amount: u128,
    time: u64,
    from: Account,
) -> VerifiedTransfer {
    VerifiedTransfer {
        block,
        from,
        to: Account {
            owner: me,
            subaccount: Some(e.subaccount.into_array()),
        },
        amount,
        fee: Some(10),
        committed_at: time,
        memo: None,
        created_at_time: Some(1),
        spender: None,
    }
}

#[test]
fn expiry_is_half_open_and_terminal_decisions_are_exclusive() {
    let (mut e, me) = setup();
    let tx = transfer(&e, me, 1, 1130, 2, e.quote.payer);
    accept_deposit(&mut e, me, &tx).unwrap();
    let at = e.quote.accept_by;
    let mut a = e.clone();
    assert_eq!(settlement_open(&a, at), Err(Error::Expired));
    refund(&mut a, at).unwrap();
    assert_eq!(settlement_open(&a, at - 1), Err(Error::VersionConflict));
    settlement_open(&e, at - 1).unwrap();
    settle(&mut e, Hash::new([1; 32]));
    assert_eq!(refund(&mut e, at), Err(Error::VersionConflict));
    assert!(e.conserved());
    let (unfunded, _) = setup();
    assert_eq!(settlement_open(&unfunded, at - 1), Err(Error::Pending));
}

#[test]
fn late_underpaid_extra_and_wrong_source_deposits_keep_their_owner() {
    let (mut e, me) = setup();
    for (n, amount, source) in [
        (1, 5, account(1)),
        (2, 5000, account(4)),
        (3, 1135, account(1)),
        (4, 1130, account(1)),
    ] {
        let tx = transfer(&e, me, n, amount, 2, source);
        let d = accept_deposit(&mut e, me, &tx).unwrap();
        assert_eq!(d.from, source);
        assert_eq!(d.refundable, if n == 3 { 5 } else { amount });
        assert!(e.conserved());
    }
    assert_eq!(e.funding_ref, Some(3));
    let (mut late, me) = setup();
    let tx = transfer(&late, me, 1, 1130, late.quote.fund_by, late.quote.payer);
    let d = accept_deposit(&mut late, me, &tx).unwrap();
    assert!(late.funding_ref.is_none());
    assert_eq!(d.refundable, 1130);
}

#[test]
fn unknown_funding_can_refund_before_ledger_returns() {
    let (mut e, me) = setup();
    let at = e.quote.accept_by;
    refund(&mut e, at).unwrap();
    let tx = transfer(&e, me, 1, 1130, 2, e.quote.payer);
    let d = accept_deposit(&mut e, me, &tx).unwrap();
    assert_eq!(d.refundable, 1130);
    assert!(e.funding_ref.is_none());
}

#[test]
fn generated_deposit_and_refund_sequences_conserve_every_atomic_unit() {
    for seed in 1..128u64 {
        let (mut e, me) = setup();
        let at = e.quote.accept_by;
        refund(&mut e, at).unwrap();
        for n in 1..20u64 {
            let amount = u128::from((seed * 7919 + n * 104729) % 100_000 + 11);
            let tx = transfer(&e, me, n, amount, at, account((n % 4 + 1) as u8));
            let d = accept_deposit(&mut e, me, &tx).unwrap();
            let mut l = leg(
                &mut e,
                LegKind::Refund { funding_block: n },
                d.from,
                d.refundable - 10,
                10,
                at,
            );
            complete_leg(&mut e, &mut l, n + 100).unwrap();
            let before = e.clone();
            complete_leg(&mut e, &mut l, n + 100).unwrap();
            assert_eq!(e, before);
            assert!(e.conserved());
        }
        assert_eq!(e.liabilities, 0);
    }
}

#[test]
fn overflow_and_invalid_deposit_are_atomic() {
    let (mut e, me) = setup();
    e.confirmed_in = u128::MAX;
    e.liabilities = u128::MAX;
    let before = e.clone();
    let tx = transfer(&e, me, 1, 1, 1, account(1));
    assert_eq!(accept_deposit(&mut e, me, &tx), Err(Error::QuotaExceeded));
    assert_eq!(e, before);
}

#[test]
fn quote_signature_binds_beneficiary_fee_and_payment_home() {
    let i = input();
    let key = SigningKey::from_bytes(&[7; 32]);
    let s = ReceiptSigner {
        epoch: 1,
        public_key: key.verifying_key().to_bytes().into(),
        valid_from: 0,
        valid_until: DAY,
        revoked: false,
    };
    let c = PaymentInit {
        home_user: account(8).owner,
        ledger: i.quote.ledger,
        platform: i.quote.platform,
        governance: candid::Principal::from_slice(&[90]),
        fee_policy: dmsg_types::payment::DeliveryFeePolicy {
            version: 1,
            effective_at_ms: 0,
            rate_bps: 500,
            minimum_atomic: 100,
        },
        ledger_fee: 10,
        max_fee: 20,
        signer: s.clone(),
        max_open_per_payer: 4,
        daily_orders: 100,
        enabled: true,
    };
    assert!(validate_quote(
        &c,
        &c.fee_policy,
        i.quote.home_payment,
        i.quote.payer.owner,
        &i,
        &s,
        2
    )
    .is_ok());
    let mut disabled = c.clone();
    disabled.enabled = false;
    assert_eq!(
        quote_current(&disabled, &c.fee_policy, &i, &s, 2),
        Err(Error::Locked)
    );
    let mut revoked = s.clone();
    revoked.revoked = true;
    assert_eq!(
        quote_current(&c, &c.fee_policy, &i, &revoked, 2),
        Err(Error::Forbidden)
    );
    assert_eq!(
        quote_current(&c, &c.fee_policy, &i, &s, i.quote.fund_by),
        Err(Error::Expired)
    );
    assert_eq!(
        quote_current(&c, &c.fee_policy, &i, &s, i.offer.offer.expires_at),
        Err(Error::Expired)
    );
    let mut malformed = i.clone();
    malformed.quote_signature = Default::default();
    assert!(validate_quote(
        &c,
        &c.fee_policy,
        i.quote.home_payment,
        i.quote.payer.owner,
        &malformed,
        &s,
        2
    )
    .is_err());
    assert!(validate_quote(
        &c,
        &c.fee_policy,
        i.quote.home_payment,
        i.quote.payer.owner,
        &i,
        &s,
        MINUTE + 2
    )
    .is_ok());
    assert_eq!(
        validate_quote(
            &c,
            &c.fee_policy,
            i.quote.home_payment,
            i.quote.payer.owner,
            &i,
            &s,
            i.offer.offer.expires_at
        ),
        Err(Error::Expired)
    );
    let mut modified = i.clone();
    modified.quote.recipient = account(9);
    assert!(validate_quote(
        &c,
        &c.fee_policy,
        i.quote.home_payment,
        i.quote.payer.owner,
        &modified,
        &s,
        2
    )
    .is_err());
    modified = i.clone();
    modified.quote.fee_reserve += 1;
    assert!(validate_quote(
        &c,
        &c.fee_policy,
        i.quote.home_payment,
        i.quote.payer.owner,
        &modified,
        &s,
        2
    )
    .is_err());
}

#[test]
fn repricing_requires_an_owner_and_the_ledgers_expected_fee() {
    let (mut e, me) = setup();
    let tx = transfer(&e, me, 1, 1130, 2, e.quote.payer);
    accept_deposit(&mut e, me, &tx).unwrap();
    settle(&mut e, Hash::new([1; 32]));
    e.primary_remaining = 10;
    e.pending_payouts = 2;
    let to = e.quote.recipient;
    let mut old = leg(&mut e, LegKind::Recipient, to, 1000, 10, 3);
    assert_eq!(old.created_at_time, 3_000_000);
    old.status = LegStatus::FeeBlocked;
    old.expected_fee = Some(20);
    let before = (e.clone(), old.clone());
    assert_eq!(
        revise_leg(&mut e, &mut old, account(9).owner, 0, 4),
        Err(Error::Forbidden)
    );
    assert_eq!((e.clone(), old.clone()), before);
    assert_eq!(
        revise_leg(&mut e, &mut old, account(1).owner, 0, 4),
        Err(Error::FeeBlocked)
    );
    assert_eq!((e.clone(), old.clone()), before);
    let next = revise_leg(&mut e, &mut old, account(1).owner, 20, 4).unwrap();
    assert_eq!(old.created_at_time, 3_000_000);
    assert_eq!(next.created_at_time, 4_000_000);
    assert_eq!(next.amount, 1000);
    assert_eq!(next.fee, 20);
    assert_eq!(next.revision, 1);
    assert_eq!(next.replaces, Some(old.leg_id));
    assert_eq!(
        next.history_digest,
        digest("dmsg/transfer-history/v1", &old)
    );
    assert!(e.conserved());
    assert_eq!(e.primary_remaining, 0);
    assert_eq!(
        revise_leg(&mut e, &mut old, account(1).owner, 20, 5),
        Err(Error::ExecutionUnknown)
    );
}

#[test]
fn a_refund_source_can_fix_fees_but_unknown_transfers_cannot_be_repriced() {
    let (mut e, me) = setup();
    let at = e.quote.accept_by;
    refund(&mut e, at).unwrap();
    let tx = transfer(&e, me, 1, 100, at, account(2));
    accept_deposit(&mut e, me, &tx).unwrap();
    let mut old = leg(
        &mut e,
        LegKind::Refund { funding_block: 1 },
        account(2),
        90,
        10,
        at,
    );
    old.status = LegStatus::Unknown;
    assert_eq!(
        revise_leg(&mut e, &mut old, account(2).owner, 10, at),
        Err(Error::ExecutionUnknown)
    );
    old.status = LegStatus::FeeBlocked;
    old.expected_fee = Some(20);
    let next = revise_leg(&mut e, &mut old, account(2).owner, 20, at).unwrap();
    assert_eq!(next.to, account(2));
    assert_eq!(next.amount + next.fee, 100);
    assert!(e.conserved());
}

#[test]
fn receipt_checks_signed_bindings_storage_terms_and_signer_window() {
    let (e, id) = setup();
    let key = SigningKey::from_bytes(&[7; 32]);
    let signer = ReceiptSigner {
        epoch: 1,
        public_key: key.verifying_key().to_bytes().into(),
        valid_from: 1,
        valid_until: DAY,
        revoked: false,
    };
    let receipt = AdmissionReceipt {
        protocol: 2,
        relay_id: Hash::new([9; 32]),
        signer_epoch: 1,
        home_payment: id,
        escrow_id: e.escrow_id,
        quote_digest: e.quote_digest,
        envelope_digest: e.quote.envelope_digest,
        size: 128,
        policy_version: 1,
        inbox_key_version: 1,
        admission_seq: 1,
        stored_at: 2,
        retain_until: 2 + DAY,
        fund_by: e.quote.fund_by,
        accept_by: e.quote.accept_by,
    };
    let sign = |receipt: AdmissionReceipt| SignedReceipt {
        signature: key
            .sign(digest("dmsg/admission-receipt/v2", &receipt).as_slice())
            .to_bytes()
            .into(),
        receipt,
    };
    assert!(receipt_valid(&e, &sign(receipt.clone()), &signer, id, 3).is_ok());
    let invalid: &[fn(&mut AdmissionReceipt)] = &[
        |r| r.protocol = 1,
        |r| r.home_payment = account(9).owner,
        |r| r.escrow_id = Hash::new([99; 32]),
        |r| r.quote_digest = Hash::new([99; 32]),
        |r| r.envelope_digest = Hash::new([99; 32]),
        |r| r.signer_epoch += 1,
        |r| r.fund_by += 1,
        |r| r.accept_by += 1,
        |r| r.size = 0,
        |r| r.size = 8193,
        |r| r.stored_at = 0,
        |r| r.stored_at = 4,
        |r| r.retain_until -= 1,
    ];
    for mutate in invalid {
        let mut bad = receipt.clone();
        mutate(&mut bad);
        assert_eq!(
            receipt_valid(&e, &sign(bad), &signer, id, 3),
            Err(Error::IntegrityFailed)
        );
    }
    let mut signed = sign(receipt);
    for signature in [[0; 64], [1; 64]] {
        signed.signature = signature.into();
        assert_eq!(
            receipt_valid(&e, &signed, &signer, id, 3),
            Err(Error::IntegrityFailed)
        );
    }
    let signed = sign(signed.receipt);
    for (from, until, revoked) in [(3, DAY, false), (1, 2, false), (1, DAY, true)] {
        let mut invalid = signer.clone();
        invalid.valid_from = from;
        invalid.valid_until = until;
        invalid.revoked = revoked;
        assert_eq!(
            receipt_valid(&e, &signed, &invalid, id, 3),
            Err(Error::Forbidden)
        );
    }
}

#[test]
fn ledger_diagnostics_preserve_unknown_and_frozen_parameters() {
    use candid::Nat;
    use dmsg_runtime::CallFailure;
    use icrc_ledger_types::icrc1::transfer::TransferError;
    let (mut e, _) = setup();
    let to = e.quote.recipient;
    let original = leg(&mut e, LegKind::Recipient, to, 1000, 10, 3);
    let errors = [
        (
            TransferError::BadFee {
                expected_fee: 20u64.into(),
            },
            TransferFailure::BadFee,
        ),
        (
            TransferError::BadBurn {
                min_burn_amount: 100u64.into(),
            },
            TransferFailure::BadBurn,
        ),
        (
            TransferError::InsufficientFunds {
                balance: 0u64.into(),
            },
            TransferFailure::InsufficientFunds,
        ),
        (TransferError::TooOld, TransferFailure::TooOld),
        (
            TransferError::CreatedInFuture { ledger_time: 1 },
            TransferFailure::CreatedInFuture,
        ),
        (
            TransferError::TemporarilyUnavailable,
            TransferFailure::TemporarilyUnavailable,
        ),
        (
            TransferError::GenericError {
                error_code: 42u64.into(),
                message: "not persisted".into(),
            },
            TransferFailure::GenericError,
        ),
    ];
    for was_unknown in [false, true] {
        for (error, reason) in &errors {
            let mut current = original.clone();
            assert_eq!(
                transfer_result(&mut current, was_unknown, Ok(Err(error.clone()))),
                Ok(None)
            );
            assert_eq!(current.last_failure.as_ref(), Some(reason));
            let bad_fee = *reason == TransferFailure::BadFee;
            assert_eq!(
                current.status,
                if was_unknown {
                    LegStatus::Unknown
                } else if bad_fee {
                    LegStatus::FeeBlocked
                } else {
                    LegStatus::Rejected
                }
            );
            assert_eq!(
                current.expected_fee,
                if bad_fee && !was_unknown {
                    Some(20)
                } else {
                    None
                }
            );
            assert_eq!(
                (
                    current.to,
                    current.amount,
                    current.fee,
                    current.memo,
                    current.created_at_time
                ),
                (
                    original.to,
                    original.amount,
                    original.fee,
                    original.memo,
                    original.created_at_time
                )
            );
        }
        for failure in [CallFailure::NotExecuted, CallFailure::Unknown] {
            let mut current = original.clone();
            let expected = if failure.preserves_unknown(was_unknown) {
                LegStatus::Unknown
            } else {
                LegStatus::Rejected
            };
            assert!(transfer_result(&mut current, was_unknown, Err(failure)).is_err());
            assert_eq!(current.status, expected);
        }
        for reply in [
            Ok(Nat::from(9u64)),
            Err(TransferError::Duplicate {
                duplicate_of: 9u64.into(),
            }),
        ] {
            let mut current = original.clone();
            assert_eq!(
                transfer_result(&mut current, was_unknown, Ok(reply)),
                Ok(Some(9))
            );
        }
    }
    let mut current = original;
    assert_eq!(
        transfer_result(&mut current, false, Ok(Ok(u128::MAX.into()))),
        Err(Error::ExecutionUnknown)
    );
    assert_eq!(current.status, LegStatus::Unknown);
    assert_eq!(current.last_failure, Some(TransferFailure::InvalidResponse));
}
