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
        .sign(digest("dmsg/quote/v1", &q).as_slice())
        .to_bytes()
        .to_vec()
        .into();
    OpenEscrow {
        op_id: Hash::new([1; 32]),
        quote: q,
        quote_signature,
        offer: SignedOffer {
            offer: o,
            signature: vec![0; 64].into(),
        },
    }
}
fn setup() -> (Escrow, Principal) {
    let i = input();
    let me = i.quote.home_payment;
    (
        escrow(
            me,
            i.quote.payer.owner,
            &i,
            digest("dmsg/quote/v1", &i.quote),
        ),
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
    assert_eq!(settle(&mut a, Hash::new([1; 32]), at), Err(Error::Expired));
    refund(&mut a, at).unwrap();
    assert_eq!(
        settle(&mut a, Hash::new([1; 32]), at - 1),
        Err(Error::VersionConflict)
    );
    settle(&mut e, Hash::new([1; 32]), at - 1).unwrap();
    assert_eq!(refund(&mut e, at), Err(Error::VersionConflict));
    assert!(e.conserved());
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
        service_fee: 100,
        ledger_fee: 10,
        max_fee: 20,
        signer: s.clone(),
        max_open_per_payer: 4,
        daily_orders: 100,
        enabled: true,
    };
    assert!(validate_quote(&c, i.quote.home_payment, i.quote.payer.owner, &i, &s, 2).is_ok());
    let mut disabled = c.clone();
    disabled.enabled = false;
    assert_eq!(quote_current(&disabled, &i, &s, 2), Err(Error::Locked));
    let mut revoked = s.clone();
    revoked.revoked = true;
    assert_eq!(quote_current(&c, &i, &revoked, 2), Err(Error::Forbidden));
    assert_eq!(
        quote_current(&c, &i, &s, i.quote.fund_by),
        Err(Error::Expired)
    );
    assert_eq!(
        quote_current(&c, &i, &s, i.offer.offer.expires_at),
        Err(Error::Expired)
    );
    let mut malformed = i.clone();
    malformed.offer.signature = vec![0; 65].into();
    assert!(validate_quote(
        &c,
        i.quote.home_payment,
        i.quote.payer.owner,
        &malformed,
        &s,
        2
    )
    .is_err());
    malformed = i.clone();
    malformed.quote_signature = vec![0; 64].into();
    assert!(validate_quote(
        &c,
        i.quote.home_payment,
        i.quote.payer.owner,
        &malformed,
        &s,
        2
    )
    .is_err());
    assert!(validate_quote(
        &c,
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
    settle(&mut e, Hash::new([1; 32]), 3).unwrap();
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
