use super::*;

fn upgrade(f: &Fixture) {
    f.ic.upgrade_canister(
        f.payment,
        wasm("dmsg_payment"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
}

fn open(f: &Fixture, nonce: u8) -> EscrowInfo {
    let recipient = f.create(2);
    let result: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "open_escrow",
        (f.order(&recipient, 2, nonce),),
    );
    result.unwrap()
}

fn deposit(f: &Fixture, e: &EscrowInfo, from: Account, amount: u128, fee: u128) -> u64 {
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "mint_test",
        (from, amount + fee),
    );
    let sent: std::result::Result<Nat, TransferError> = update(
        &f.ic,
        f.ledger,
        from.owner,
        "icrc1_transfer",
        (TransferArg {
            from_subaccount: from.subaccount,
            to: Account {
                owner: f.payment,
                subaccount: Some(e.subaccount.into_array()),
            },
            amount: amount.into(),
            fee: Some(fee.into()),
            memo: None,
            created_at_time: Some(millis_to_nanos(time(&f.ic)).unwrap()),
        },),
    );
    let block = dmsg_runtime::ledger::block_index(sent.unwrap()).unwrap();
    let result: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "check_funding",
        (e.escrow_id, block),
    );
    result.unwrap();
    block
}

fn settle(f: &Fixture, e: &EscrowInfo) {
    let result: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(99),
        "finalize_receipt",
        (f.receipt(e),),
    );
    result.unwrap();
}

fn leg(f: &Fixture, id: Hash, n: u64, caller: Principal) -> Result<TransferLeg> {
    update(&f.ic, f.payment, caller, "process_transfer", (id, n))
}

#[test]
fn failed_reads_and_writes_have_independent_bounded_caller_budgets() {
    let f = Fixture::new();
    let e = open(&f, 1);
    deposit(&f, &e, e.quote.payer, e.quote.amount, 10);
    settle(&f, &e);
    f.ic.advance_time(Duration::from_millis(MINUTE));
    for who in std::iter::once(Principal::anonymous()).chain((70..74).map(person)) {
        for _ in 0..40 {
            let missing: Result<EscrowInfo> = update(
                &f.ic,
                f.payment,
                who,
                "check_funding",
                (e.escrow_id, 9999u64),
            );
            assert_eq!(missing, Err(Error::NotFound));
        }
        let limited: Result<EscrowInfo> = update(
            &f.ic,
            f.payment,
            who,
            "check_funding",
            (e.escrow_id, 9999u64),
        );
        assert_eq!(limited, Err(Error::QuotaExceeded));
    }
    upgrade(&f);
    let exhausted: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(90),
        "check_funding",
        (e.escrow_id, 9999u64),
    );
    assert_eq!(exhausted, Err(Error::QuotaExceeded));
    // Failed discovery cannot consume the funds-transfer allowance.
    assert_eq!(
        leg(&f, e.escrow_id, 0, person(40)).unwrap().status,
        LegStatus::Succeeded
    );
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "reject_next_transfers",
        (40u32,),
    );
    for _ in 0..40 {
        assert_eq!(
            leg(&f, e.escrow_id, 1, Principal::anonymous())
                .unwrap()
                .status,
            LegStatus::Rejected
        );
    }
    upgrade(&f);
    assert_eq!(
        leg(&f, e.escrow_id, 1, Principal::anonymous()),
        Err(Error::QuotaExceeded)
    );
    assert_eq!(
        leg(&f, e.escrow_id, 1, person(40)).unwrap().status,
        LegStatus::Succeeded
    );
    f.ic.advance_time(Duration::from_millis(MINUTE));
    let missing: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        Principal::anonymous(),
        "check_funding",
        (e.escrow_id, 9999u64),
    );
    assert_eq!(missing, Err(Error::NotFound));
}

#[test]
fn both_payouts_reach_the_ceiling_and_dust_combines_with_same_source_deposits() {
    let f = Fixture::new();
    let e = open(&f, 1);
    assert_eq!(e.quote.fee_reserve, 40);
    let primary = deposit(&f, &e, e.quote.payer, e.quote.amount + 17, 10);
    settle(&f, &e);
    void(
        &f.ic,
        f.ledger,
        Principal::anonymous(),
        "set_fee",
        (20u128,),
    );
    let updated: Result<()> = update(
        &f.ic,
        f.payment,
        Principal::anonymous(),
        "set_ledger_fee",
        (20u128,),
    );
    updated.unwrap();
    for n in 0..2u64 {
        assert_eq!(
            leg(&f, e.escrow_id, n, person(40)).unwrap().status,
            LegStatus::FeeBlocked
        );
        let revised: Result<TransferLeg> = update(
            &f.ic,
            f.payment,
            person(40),
            "revise_rejected_transfer",
            (e.escrow_id, n, 20u128),
        );
        let revised = revised.unwrap();
        upgrade(&f);
        assert_eq!(
            leg(&f, e.escrow_id, revised.leg_id, person(40))
                .unwrap()
                .status,
            LegStatus::Succeeded
        );
    }
    let dust: Result<RefundQuote> = query(
        &f.ic,
        f.payment,
        person(40),
        "quote_refund",
        (e.escrow_id, vec![primary], true),
    );
    let dust = dust.unwrap();
    assert_eq!((dust.available, dust.fee, dust.amount), (17, 20, 0));
    let extra = deposit(&f, &e, e.quote.payer, 8, 20);
    let preview: Result<RefundQuote> = query(
        &f.ic,
        f.payment,
        person(40),
        "quote_refund",
        (e.escrow_id, vec![extra, primary], true),
    );
    assert_eq!(preview.unwrap().amount, 5);
    let refund: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(40),
        "claim_refund",
        (e.escrow_id, vec![extra, primary], true),
    );
    let refund = refund.unwrap();
    assert_eq!(refund.to, e.quote.payer);
    assert_eq!(
        refund.kind,
        LegKind::Refund {
            funding_blocks: vec![primary, extra],
            includes_reserve: true
        }
    );
    upgrade(&f);
    let duplicate: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(40),
        "claim_refund",
        (e.escrow_id, vec![extra, primary], true),
    );
    assert_eq!(duplicate, Err(Error::FeeBlocked));
    assert_eq!(
        leg(&f, e.escrow_id, refund.leg_id, person(40))
            .unwrap()
            .status,
        LegStatus::Succeeded
    );
    let final_state: Result<EscrowInfo> =
        query(&f.ic, f.payment, person(40), "get_escrow", (e.escrow_id,));
    assert_eq!(final_state.unwrap().liabilities, 0);
}

#[test]
fn deposits_paginate_after_upgrade_and_mixed_accounts_cannot_share_a_refund() {
    let f = Fixture::new();
    let e = open(&f, 1);
    let other = Account {
        owner: e.payer_principal,
        subaccount: Some([7; 32]),
    };
    let mut blocks = vec![];
    for n in 0..35u128 {
        if n % 15 == 0 {
            f.ic.advance_time(Duration::from_millis(MINUTE));
        }
        blocks.push(deposit(
            &f,
            &e,
            if n % 2 == 0 { e.quote.payer } else { other },
            n + 11,
            10,
        ));
    }
    upgrade(&f);
    let first: Result<Vec<Deposit>> = query(
        &f.ic,
        f.payment,
        person(99),
        "list_deposits",
        (e.escrow_id, None::<u64>),
    );
    let first = first.unwrap();
    assert_eq!(first.len(), 32);
    assert_eq!(first[0].block, blocks[0]);
    let last: Result<Vec<Deposit>> = query(
        &f.ic,
        f.payment,
        person(99),
        "list_deposits",
        (e.escrow_id, Some(first[31].block)),
    );
    assert_eq!(
        last.unwrap().iter().map(|d| d.block).collect::<Vec<_>>(),
        blocks[32..]
    );
    let bad: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(40),
        "claim_refund",
        (e.escrow_id, vec![blocks[0], blocks[1]], false),
    );
    assert_eq!(bad, Err(Error::IntegrityFailed));
    let repeated: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(40),
        "claim_refund",
        (e.escrow_id, vec![blocks[0], blocks[0]], false),
    );
    assert!(matches!(repeated, Err(Error::InvalidInput(_))));
    let too_many: Result<RefundQuote> = query(
        &f.ic,
        f.payment,
        person(40),
        "quote_refund",
        (e.escrow_id, blocks[..33].to_vec(), false),
    );
    assert_eq!(too_many, Err(Error::QuotaExceeded));
    let unchanged: Result<Vec<Deposit>> = query(
        &f.ic,
        f.payment,
        person(99),
        "list_deposits",
        (e.escrow_id, None::<u64>),
    );
    assert_eq!(unchanged.unwrap(), first);
    // Pending underpayments can be refunded together without becoming funding.
    let refund: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        person(40),
        "claim_refund",
        (e.escrow_id, vec![blocks[0], blocks[2]], false),
    );
    let refund = refund.unwrap();
    assert_eq!(refund.amount, 14);
    assert_eq!(refund.to, e.quote.payer);
}

#[test]
fn retained_capacity_rechecks_after_await_and_keeps_old_funds_recoverable() {
    let f = Fixture::with_payment_capacity(true, 100, 1, |_| {});
    let recipient = f.create(2);
    let first = f.order(&recipient, 2, 1);
    let mut second = f.order(&recipient, 2, 2);
    second.quote.payer = account(person(41));
    second.quote_signature = key(50)
        .sign(digest("dmsg/quote/v2", &second.quote).as_slice())
        .to_bytes()
        .into();
    let calls = [first.clone(), second.clone()].map(|input| {
        f.ic.submit_call(
            f.payment,
            input.quote.payer.owner,
            "open_escrow",
            candid::encode_args((input,)).unwrap(),
        )
        .unwrap()
    });
    let results: Vec<Result<EscrowInfo>> = calls
        .into_iter()
        .map(|call| candid::decode_one(&f.ic.await_call(call).unwrap()).unwrap())
        .collect();
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Err(Error::QuotaExceeded))
            .count(),
        1
    );
    let e = results.into_iter().find_map(|r| r.ok()).unwrap();
    f.ic.advance_time(Duration::from_millis(46 * MINUTE));
    let refunded: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        e.payer_principal,
        "expiry_refund",
        (e.escrow_id,),
    );
    refunded.unwrap();
    upgrade(&f);
    let full: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        person(40),
        "open_escrow",
        (f.order(&recipient, 2, 3),),
    );
    assert_eq!(full, Err(Error::QuotaExceeded));
    let block = deposit(&f, &e, e.quote.payer, 100, 10);
    // Whichever payer won the last slot claims its own refund.
    let refund: Result<TransferLeg> = update(
        &f.ic,
        f.payment,
        e.payer_principal,
        "claim_refund",
        (e.escrow_id, vec![block], true),
    );
    let refund = refund.unwrap();
    assert_eq!(
        leg(&f, e.escrow_id, refund.leg_id, person(40))
            .unwrap()
            .status,
        LegStatus::Succeeded
    );
    let original = if e.payer_principal == person(40) {
        first
    } else {
        second
    };
    let replay: Result<EscrowInfo> = update(
        &f.ic,
        f.payment,
        e.payer_principal,
        "open_escrow",
        (original,),
    );
    assert_eq!(replay.unwrap().liabilities, 0);
}
