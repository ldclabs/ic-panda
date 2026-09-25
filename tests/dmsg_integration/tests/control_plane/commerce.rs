use super::*;
use dmsg_protocol::{billing::*, commerce_v2::*, integration::*};
use dmsg_types::{
    billing::*, integration::*, integration_billing::*, integration_membership::*, membership::*,
};
use serde::Serialize;
#[path = "external_integration.rs"]
mod external_integration;
fn offer(f: &Fixture, id: &AccountId, op: u8) -> BillingOffer {
    let r: Result<ExecutionUsage> = update(
        &f.ic,
        f.user,
        person(1),
        "refresh_execution_entitlement",
        (id,),
    );
    r.unwrap();
    let r: Result<BillingOffer> = update(
        &f.ic,
        f.commerce,
        person(1),
        "prepare_account_subscription",
        (
            "dmsg".to_string(),
            beneficiary(f.user, id),
            "plus".to_string(),
            Hash::new([op; 32]),
        ),
    );
    r.unwrap()
}
fn approve(
    f: &Fixture,
    id: &AccountId,
    offer: &BillingOffer,
    service: Principal,
    purpose: ApprovalPurpose,
    action_digest: Hash,
    op: u8,
) -> ProductAuthorizationRequest {
    let a = ApplicationApproval {
        version: 1,
        environment: Environment::Local,
        app_id: offer.app_id.clone(),
        app_config_version: 1,
        origin: "https://dmsg.test".into(),
        approving_account: *id,
        service,
        beneficiary: offer.beneficiary.clone(),
        actor: person(1),
        purpose,
        action_digest,
        operation_id: offer.operation_id,
        nonce: Hash::new([op; 32]),
        expires_at_ms: time(&f.ic) + AUTH_TTL_MS,
    };
    let state = f.account_id(1, id);
    let mut proof = Approval {
        device_id: Hash::new([1; 32]),
        security_epoch: state.security_epoch,
        sequence: state.devices[&Hash::new([1; 32])].next_sequence,
        request_id: Hash::new([op; 32]),
        expires_at: a.expires_at_ms,
        signature: Default::default(),
    };
    proof.signature = key(1)
        .sign(approval_message(f.user, id, "dmsg/application/approve/v1", &a, &proof).as_slice())
        .to_bytes()
        .into();
    let r: Result<Hash> = update(
        &f.ic,
        f.user,
        person(1),
        "approve_application",
        (a.clone(), proof),
    );
    let approval_id = r.unwrap();
    ProductAuthorizationRequest {
        offer: offer.clone(),
        account_approval: a,
        user_home: f.user,
        approval_id,
        product_approval: None,
    }
}
fn open(f: &Fixture, id: &AccountId, ledger: Principal, op: u8) -> CheckoutView {
    let bill = offer(f, id, op);
    let r: Result<CheckoutQuote> = update(
        &f.ic,
        f.commerce,
        person(1),
        "quote_checkout",
        (bill.clone(), ledger, account(person(1))),
    );
    let q = r.unwrap();
    let a = approve(
        f,
        id,
        &bill,
        f.commerce,
        ApprovalPurpose::CashCheckout,
        checkout_quote_hash(&q),
        op + 1,
    );
    let r: Result<CheckoutView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "open_checkout",
        (OpenCheckout {
            quote: q,
            authorization: a,
        },),
    );
    let o = r.unwrap();
    assert_eq!(o.progress.status, CheckoutStatus::AwaitingFunding);
    o
}
fn deposit(f: &Fixture, o: &CheckoutView, ledger: Principal, who: u8, amount: u128) -> CashBlock {
    void(
        &f.ic,
        ledger,
        person(who),
        "mint_test",
        (account(person(who)), amount + 10),
    );
    let r: std::result::Result<Nat, TransferError> = update(
        &f.ic,
        ledger,
        person(who),
        "icrc1_transfer",
        (TransferArg {
            from_subaccount: None,
            to: o.quote.cash.deposit,
            amount: amount.into(),
            fee: Some(10u64.into()),
            memo: None,
            created_at_time: Some(millis_to_nanos(time(&f.ic)).unwrap()),
        },),
    );
    CashBlock {
        ledger,
        block_index: u128::try_from(r.unwrap().0).unwrap(),
    }
}
fn status(f: &Fixture, id: Hash) -> CheckoutView {
    let r: Result<CheckoutView> = query(&f.ic, f.commerce, person(1), "get_checkout", (id,));
    r.unwrap()
}
fn funding(f: &Fixture, id: Hash, b: CashBlock) -> CheckoutProgress {
    let r: Result<CheckoutProgress> = update(
        &f.ic,
        f.commerce,
        person(1),
        "check_checkout_funding",
        (id, b),
    );
    r.unwrap()
}
#[test]
fn checkout_two_ledgers_same_block_isolation_delivery_replay_and_revenue() {
    for second in [false, true] {
        let f = Fixture::commercial();
        let id = f.create(1);
        let ledger = if second { f.ledger2 } else { f.ledger };
        let other = if second { f.ledger } else { f.ledger2 };
        let o = open(&f, &id, ledger, 60);
        let total = o.quote.cash.amount_atomic + o.quote.cash.fee_reserve_atomic;
        let wrong = deposit(&f, &o, other, 1, total);
        assert_eq!(
            funding(&f, o.progress.order_id, wrong.clone()).status,
            CheckoutStatus::AwaitingFunding
        );
        let good = deposit(&f, &o, ledger, 1, total + 50);
        assert_eq!(good.block_index, wrong.block_index);
        assert_eq!(
            funding(&f, o.progress.order_id, good.clone()).status,
            CheckoutStatus::Applied
        );
        assert_eq!(
            funding(&f, o.progress.order_id, good).status,
            CheckoutStatus::Applied
        );
        let audit: Result<CheckoutOperationsPage> = query(
            &f.ic,
            f.commerce,
            person(1),
            "checkout_operations",
            (None::<Hash>, 16u16),
        );
        let audit = audit.unwrap();
        assert_eq!(audit.orders.len(), 1);
        for b in &audit.orders[0].balances {
            assert_eq!(
                b.incoming_atomic,
                b.refundable_atomic
                    + b.service_reserve_atomic
                    + b.fee_reserve_atomic
                    + b.outgoing_atomic
            );
        }
        let outsider: Result<CheckoutOperationsPage> = query(
            &f.ic,
            f.commerce,
            person(99),
            "checkout_operations",
            (None::<Hash>, 16u16),
        );
        assert!(outsider.unwrap().orders.is_empty());
        let details = status(&f, o.progress.order_id);
        assert!(details.receipt.is_some());
        let r: Result<EntitlementView> = update(
            &f.ic,
            f.commerce,
            person(1),
            "refresh_entitlement",
            (beneficiary(f.user, &id),),
        );
        assert_eq!(r.unwrap().plan_snapshot.plan_id, PlanId::Plus);
        let refund: Result<CashTransfer> = update(
            &f.ic,
            f.commerce,
            person(1),
            "claim_checkout_refund",
            (
                o.progress.order_id,
                other,
                vec![wrong.block_index],
                Hash::new([75; 32]),
            ),
        );
        let refund = refund.unwrap();
        assert_eq!(refund.ledger, other);
        assert_eq!(refund.to, account(person(1)));
        assert_eq!(refund.amount_atomic + refund.fee_atomic, total);
        let moved: Result<CashTransferProgress> = update(
            &f.ic,
            f.commerce,
            person(1),
            "process_checkout_transfer",
            (refund.transfer_id,),
        );
        assert_eq!(moved.unwrap().status, CashTransferStatus::Succeeded);
        let denied: Result<CashTransfer> = update(
            &f.ic,
            f.commerce,
            person(1),
            "collect_checkout_revenue",
            (o.progress.order_id,),
        );
        assert_eq!(denied, Err(Error::Forbidden));
        f.ic.advance_time(Duration::from_millis(200 * DAY));
        let earned: Result<CashTransfer> = update(
            &f.ic,
            f.commerce,
            person(60),
            "collect_checkout_revenue",
            (o.progress.order_id,),
        );
        let earned = earned.unwrap();
        assert_eq!(earned.ledger, ledger);
        assert!(earned.amount_atomic < o.quote.cash.amount_atomic);
        f.ic.upgrade_canister(
            f.commerce,
            wasm("dmsg_commerce"),
            candid::encode_args(()).unwrap(),
            None,
        )
        .unwrap();
        assert_eq!(status(&f, o.progress.order_id).receipt, details.receipt);
    }
}
#[test]
fn checkout_unknown_transfer_preserves_arguments_and_reconciles_actual_block() {
    let f = Fixture::commercial();
    let id = f.create(1);
    let o = open(&f, &id, f.ledger, 80);
    let wrong = deposit(&f, &o, f.ledger, 2, 1000);
    funding(&f, o.progress.order_id, wrong.clone());
    let r: Result<CashTransfer> = update(
        &f.ic,
        f.commerce,
        person(2),
        "claim_checkout_refund",
        (
            o.progress.order_id,
            f.ledger,
            vec![wrong.block_index],
            Hash::new([82; 32]),
        ),
    );
    let t = r.unwrap();
    void(&f.ic, f.ledger, person(1), "lose_next_response", ());
    let r: Result<CashTransferProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "process_checkout_transfer",
        (t.transfer_id,),
    );
    assert!(r.is_err() || r.unwrap().status == CashTransferStatus::Unknown);
    let changed: Result<CashTransfer> = update(
        &f.ic,
        f.commerce,
        person(2),
        "revise_checkout_transfer_fee",
        (t.transfer_id, 11u128),
    );
    assert!(changed.is_err());
    let r: Result<CashTransferProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "reconcile_checkout_transfer",
        (
            t.transfer_id,
            CashBlock {
                ledger: f.ledger,
                block_index: wrong.block_index + 1,
            },
        ),
    );
    assert_eq!(r.unwrap().status, CashTransferStatus::Succeeded);
    let r: Result<CashTransfer> = query(
        &f.ic,
        f.commerce,
        person(2),
        "get_checkout_transfer",
        (t.transfer_id,),
    );
    let current = r.unwrap();
    assert_eq!(current.memo, t.memo);
    assert_eq!(current.created_at_time_ns, t.created_at_time_ns);
    assert_eq!(current.fee_atomic, t.fee_atomic);
}
#[derive(CandidType, Serialize, Deserialize, Clone)]
struct TestNeuronId {
    id: Vec<u8>,
}
#[derive(CandidType, Serialize, Deserialize, Clone)]
struct TestPermission {
    principal: Option<Principal>,
    permission_type: Vec<i32>,
}
#[derive(CandidType, Serialize, Deserialize, Clone)]
enum TestDissolve {
    DissolveDelaySeconds(u64),
    WhenDissolvedTimestampSeconds(u64),
}
#[derive(CandidType, Serialize, Deserialize, Clone)]
struct TestNeuron {
    id: Option<TestNeuronId>,
    permissions: Vec<TestPermission>,
    cached_neuron_stake_e8s: u64,
    neuron_fees_e8s: u64,
    dissolve_state: Option<TestDissolve>,
}
fn neuron(f: &Fixture, stake: u64, other: bool) {
    let mut permissions = vec![TestPermission {
        principal: Some(person(1)),
        permission_type: vec![1, 2, 4, 5, 6],
    }];
    if other {
        permissions.push(TestPermission {
            principal: Some(person(2)),
            permission_type: vec![2],
        });
    }
    void(
        &f.ic,
        f.sns,
        person(1),
        "set_neuron",
        (Some(TestNeuron {
            id: Some(TestNeuronId { id: vec![44; 32] }),
            permissions,
            cached_neuron_stake_e8s: stake,
            neuron_fees_e8s: 100,
            dissolve_state: Some(TestDissolve::DissolveDelaySeconds(400 * 24 * 3600)),
        }),),
    );
}
#[test]
fn panda_full_waiver_requires_fresh_post_cooling_approval_and_never_exits_early() {
    let f = Fixture::commercial();
    let id = f.create(1);
    neuron(&f, 100_000_000_000_100, false);
    let bill = offer(&f, &id, 90);
    let r: Result<PandaApplicationTerms> = update(
        &f.ic,
        f.membership,
        person(1),
        "quote_panda_subscription",
        (bill.clone(), f.user, id, Hash::new([44; 32])),
    );
    let terms = r.unwrap();
    assert_eq!(
        terms.quote.required_stake_e8s,
        required_panda_stake(bill.amount_usd_micros, 5000, 1).unwrap()
    );
    let a = approve(
        &f,
        &id,
        &bill,
        f.membership,
        ApprovalPurpose::PandaSubscription,
        panda_application_hash(&terms),
        91,
    );
    let r: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_panda_claim",
        (PandaClaimRequest {
            terms: terms.clone(),
            authorization: a.clone(),
        },),
    );
    let cooling = r.unwrap();
    assert_eq!(cooling.status, PandaClaimStatus::CoolingDown, "{cooling:?}");
    let early: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "advance_panda_claim",
        (cooling.claim_id, a.clone()),
    );
    assert!(early.is_err());
    f.ic.advance_time(Duration::from_millis(PANDA_COOLING_MS + 1));
    let stale: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "advance_panda_claim",
        (cooling.claim_id, a),
    );
    assert!(stale.is_err());
    let fresh = approve(
        &f,
        &id,
        &bill,
        f.membership,
        ApprovalPurpose::PandaSubscription,
        panda_application_hash(&terms),
        92,
    );
    let r: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "advance_panda_claim",
        (cooling.claim_id, fresh),
    );
    let active = r.unwrap();
    assert_eq!(active.status, PandaClaimStatus::Active);
    let audit: Result<PandaOperationsPage> = query(
        &f.ic,
        f.membership,
        person(1),
        "panda_operations",
        (None::<Hash>, 16u16),
    );
    assert_eq!(audit.unwrap().claims.len(), 1);
    let outsider: Result<PandaOperationsPage> = query(
        &f.ic,
        f.membership,
        person(99),
        "panda_operations",
        (None::<Hash>, 16u16),
    );
    assert!(outsider.unwrap().claims.is_empty());

    assert_eq!(active.committed_until_ms, bill.expires_at_ms);
    let exit: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "cancel_panda_application",
        (active.claim_id,),
    );
    assert_eq!(exit, Err(Error::Forbidden));
    neuron(&f, 100, false);
    f.ic.advance_time(Duration::from_millis(PANDA_LEASE_MS + 1));
    let r: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "refresh_panda_claim",
        (active.claim_id,),
    );
    let repairing = r.unwrap();
    assert_eq!(repairing.eligibility, Eligibility::Ineligible);
    // A repeated refresh within a minute reuses the observation and leaves the view unchanged.
    let r: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "refresh_panda_claim",
        (active.claim_id,),
    );
    assert_eq!(r.unwrap(), repairing);
    f.ic.advance_time(Duration::from_millis(8 * DAY));
    let r: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "refresh_panda_claim",
        (active.claim_id,),
    );
    let ended = r.unwrap();
    assert_eq!(ended.status, PandaClaimStatus::Terminated);
    assert_eq!(ended.committed_until_ms, bill.expires_at_ms);
    f.ic.upgrade_canister(
        f.membership,
        wasm("membership"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    f.ic.advance_time(Duration::from_millis(bill.expires_at_ms - time(&f.ic) + 1));
    let r: Result<u32> = update(
        &f.ic,
        f.membership,
        person(1),
        "sweep_panda_commitments",
        (),
    );
    r.unwrap();
    let r: Result<u32> = update(
        &f.ic,
        f.membership,
        person(1),
        "sweep_panda_commitments",
        (),
    );
    assert_eq!(r.unwrap(), 0);
    let r: Result<PandaClaimView> = query(
        &f.ic,
        f.membership,
        person(1),
        "get_panda_claim",
        (active.claim_id,),
    );
    assert_eq!(r.unwrap().status, PandaClaimStatus::Released);
}
fn sample(f: &Fixture) -> (Principal, AccountId) {
    let canister = f.ic.create_canister();
    f.ic.add_cycles(canister, 10_000_000_000_000_000);
    f.ic.install_canister(
        canister,
        wasm("dmsg_account_product"),
        candid::encode_args((dmsg_account_product::Config {
            admin: person(1),
            commerce: f.commerce,
            membership: f.membership,
            environment: Environment::Local,
            app_id: "sample".into(),
            product_id: "sample".into(),
            terms_hash: Hash::new([101; 32]),
            annual_usd_micros: 10_000_000,
        },))
        .unwrap(),
        None,
    );
    let id = AccountId([55; 12]);
    let r: Result<()> = update(
        &f.ic,
        canister,
        person(1),
        "assign_account",
        (id, person(1)),
    );
    r.unwrap();
    let p = ProductRegistration {
        version: 2,
        environment: Environment::Local,
        product_id: "sample".into(),
        config_version: 1,
        quote_authority: canister,
        beneficiary_authority: canister,
        adapter: canister,
        subject_schema: "sample-account-v1".into(),
        subject_size: 12,
        merchant: account(person(60)),
        ledgers: vec![f.ledger, f.ledger2],
        terms_hash: Hash::new([101; 32]),
        paused: false,
    };
    let r: Result<()> = update(
        &f.ic,
        f.commerce,
        f.sns,
        "register_integration_product",
        (p,),
    );
    r.unwrap();
    let a = AppRegistration {
        version: 1,
        environment: Environment::Local,
        app_id: "sample".into(),
        config_version: 1,
        origins: vec!["https://dmsg.test".into()],
        user_homes: vec![f.user],
        cose_homes: vec![f.cose],
        product_ids: vec!["sample".into()],
        capabilities: vec![AppCapability::Checkout],
        profiles: vec![],
        authentication_receiver: canister,
        action_authority: canister,
        paused: false,
    };
    let r: Result<()> = update(&f.ic, f.commerce, f.sns, "register_integration_app", (a,));
    r.unwrap();
    (canister, id)
}
fn sample_offer(
    f: &Fixture,
    home: Principal,
    id: &AccountId,
    op: u8,
    method: SettlementMethod,
) -> dmsg_account_product::Prepared {
    let r: Result<dmsg_account_product::Prepared> = update(
        &f.ic,
        home,
        person(1),
        "prepare_billing_offer",
        (*id, Hash::new([op; 32]), method),
    );
    r.unwrap()
}
#[test]
fn independent_account_adapter_lost_apply_ack_and_cash_panda_race_share_one_contract_book() {
    let f = Fixture::commercial();
    let account = f.create(1);
    let (home, id) = sample(&f);
    let cash = sample_offer(&f, home, &id, 110, SettlementMethod::Cash);
    let panda = sample_offer(&f, home, &id, 111, SettlementMethod::Panda);
    let q: Result<CheckoutQuote> = update(
        &f.ic,
        f.commerce,
        person(1),
        "quote_checkout",
        (cash.offer.clone(), f.ledger, super::account(person(1))),
    );
    let q = q.unwrap();
    let mut a = approve(
        &f,
        &account,
        &cash.offer,
        f.commerce,
        ApprovalPurpose::CashCheckout,
        checkout_quote_hash(&q),
        112,
    );
    a.product_approval = Some(cash.approval);
    let opened: Result<CheckoutView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "open_checkout",
        (OpenCheckout {
            quote: q,
            authorization: a,
        },),
    );
    let opened = opened.unwrap();
    assert_eq!(opened.progress.status, CheckoutStatus::AwaitingFunding);
    neuron(&f, 100_000_000_000_100, false);
    let terms: Result<PandaApplicationTerms> = update(
        &f.ic,
        f.membership,
        person(1),
        "quote_panda_subscription",
        (panda.offer.clone(), f.user, account, Hash::new([44; 32])),
    );
    let terms = terms.unwrap();
    let mut a = approve(
        &f,
        &account,
        &panda.offer,
        f.membership,
        ApprovalPurpose::PandaSubscription,
        panda_application_hash(&terms),
        113,
    );
    a.product_approval = Some(panda.approval);
    let r: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_panda_claim",
        (PandaClaimRequest {
            terms,
            authorization: a,
        },),
    );
    assert_eq!(r.unwrap().status, PandaClaimStatus::Rejected);
    let r: Result<()> = update(&f.ic, home, person(1), "lose_next_apply_ack", ());
    r.unwrap();
    let block = deposit(
        &f,
        &opened,
        f.ledger,
        1,
        opened.quote.cash.amount_atomic + opened.quote.cash.fee_reserve_atomic,
    );
    let r: Result<CheckoutProgress> = update(
        &f.ic,
        f.commerce,
        person(1),
        "check_checkout_funding",
        (opened.progress.order_id, block),
    );
    assert!(r.is_err() || r.unwrap().status == CheckoutStatus::Applying);
    let history: Result<Vec<SubscriptionContract>> =
        query(&f.ic, home, person(1), "contracts", (id,));
    assert_eq!(history.unwrap().len(), 1);
    let r: Result<CheckoutProgress> = update(
        &f.ic,
        f.commerce,
        person(1),
        "reconcile_checkout",
        (opened.progress.order_id,),
    );
    assert_eq!(r.unwrap().status, CheckoutStatus::Applied);
    f.ic.upgrade_canister(
        home,
        wasm("dmsg_account_product"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let history: Result<Vec<SubscriptionContract>> =
        query(&f.ic, home, person(1), "contracts", (id,));
    assert_eq!(history.unwrap().len(), 1);
}
#[test]
fn one_neuron_cannot_serve_two_products_and_contiguous_commitments_survive_first_expiry() {
    let f = Fixture::commercial();
    let account = f.create(1);
    let (home, id) = sample(&f);
    neuron(&f, 100_000_000_000_100, false);
    let prepared = sample_offer(&f, home, &id, 120, SettlementMethod::Panda);
    let terms: Result<PandaApplicationTerms> = update(
        &f.ic,
        f.membership,
        person(1),
        "quote_panda_subscription",
        (prepared.offer.clone(), f.user, account, Hash::new([44; 32])),
    );
    let terms = terms.unwrap();
    let mut a = approve(
        &f,
        &account,
        &prepared.offer,
        f.membership,
        ApprovalPurpose::PandaSubscription,
        panda_application_hash(&terms),
        121,
    );
    a.product_approval = Some(prepared.approval.clone());
    let r: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_panda_claim",
        (PandaClaimRequest {
            terms: terms.clone(),
            authorization: a,
        },),
    );
    let pending = r.unwrap();
    assert_eq!(pending.status, PandaClaimStatus::CoolingDown);
    let bill = offer(&f, &account, 122);
    let other: Result<PandaApplicationTerms> = update(
        &f.ic,
        f.membership,
        person(1),
        "quote_panda_subscription",
        (bill.clone(), f.user, account, Hash::new([44; 32])),
    );
    let other = other.unwrap();
    let a = approve(
        &f,
        &account,
        &bill,
        f.membership,
        ApprovalPurpose::PandaSubscription,
        panda_application_hash(&other),
        123,
    );
    let r: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_panda_claim",
        (PandaClaimRequest {
            terms: other,
            authorization: a,
        },),
    );
    assert_eq!(r, Err(Error::NeuronOccupied));
    f.ic.advance_time(Duration::from_millis(PANDA_COOLING_MS + 1));
    let mut a = approve(
        &f,
        &account,
        &prepared.offer,
        f.membership,
        ApprovalPurpose::PandaSubscription,
        panda_application_hash(&terms),
        124,
    );
    a.product_approval = Some(prepared.approval);
    let r: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "advance_panda_claim",
        (pending.claim_id, a),
    );
    let first = r.unwrap();
    assert_eq!(first.status, PandaClaimStatus::Active);
    // The reference product deliberately permits early next-period commitments.
    let prepared = sample_offer(&f, home, &id, 125, SettlementMethod::Panda);
    assert_eq!(prepared.offer.starts_at_ms, first.committed_until_ms);
    // Keep the physical lock sufficient for both complete terms.
    void(
        &f.ic,
        f.sns,
        person(1),
        "set_neuron",
        (Some(TestNeuron {
            id: Some(TestNeuronId { id: vec![44; 32] }),
            permissions: vec![TestPermission {
                principal: Some(person(1)),
                permission_type: vec![1, 2, 4, 5, 6],
            }],
            cached_neuron_stake_e8s: 100_000_000_000_100,
            neuron_fees_e8s: 100,
            dissolve_state: Some(TestDissolve::DissolveDelaySeconds(800 * 86400)),
        }),),
    );
    let terms: Result<PandaApplicationTerms> = update(
        &f.ic,
        f.membership,
        person(1),
        "quote_panda_subscription",
        (prepared.offer.clone(), f.user, account, Hash::new([44; 32])),
    );
    let terms = terms.unwrap();
    let mut a = approve(
        &f,
        &account,
        &prepared.offer,
        f.membership,
        ApprovalPurpose::PandaSubscription,
        panda_application_hash(&terms),
        126,
    );
    a.product_approval = Some(prepared.approval.clone());
    let r: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_panda_claim",
        (PandaClaimRequest {
            terms: terms.clone(),
            authorization: a,
        },),
    );
    let next = r.unwrap();
    assert_eq!(next.status, PandaClaimStatus::CoolingDown);
    f.ic.advance_time(Duration::from_millis(PANDA_COOLING_MS + 1));
    let mut a = approve(
        &f,
        &account,
        &prepared.offer,
        f.membership,
        ApprovalPurpose::PandaSubscription,
        panda_application_hash(&terms),
        127,
    );
    a.product_approval = Some(prepared.approval);
    let r: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "advance_panda_claim",
        (next.claim_id, a),
    );
    let next = r.unwrap();
    assert_eq!(next.status, PandaClaimStatus::Active);
    let r: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "cancel_panda_application",
        (next.claim_id,),
    );
    assert_eq!(r, Err(Error::Forbidden));
    f.ic.advance_time(Duration::from_millis(
        first.committed_until_ms - time(&f.ic) + 1,
    ));
    let r: Result<u32> = update(
        &f.ic,
        f.membership,
        person(1),
        "sweep_panda_commitments",
        (),
    );
    assert_eq!(r.unwrap(), 1);
    let r: Result<PandaClaimView> = query(
        &f.ic,
        f.membership,
        person(1),
        "get_panda_claim",
        (first.claim_id,),
    );
    assert_eq!(r.unwrap().status, PandaClaimStatus::Released);
    let r: Result<PandaClaimView> = query(
        &f.ic,
        f.membership,
        person(1),
        "get_panda_claim",
        (next.claim_id,),
    );
    assert_eq!(r.unwrap().committed_until_ms, next.committed_until_ms);
}

#[test]
fn known_fee_rejection_can_revise_only_within_original_cap_and_pause_keeps_refunds_open() {
    let f = Fixture::commercial();
    let id = f.create(1);
    let opened = open(&f, &id, f.ledger, 140);
    let incoming = deposit(&f, &opened, f.ledger, 2, 1000);
    funding(&f, opened.progress.order_id, incoming.clone());
    let assets: Vec<SettlementAssetView> =
        query(&f.ic, f.commerce, person(1), "settlement_assets", ());
    let mut policy = assets
        .into_iter()
        .find(|a| a.policy.ledger == f.ledger)
        .unwrap()
        .policy;
    policy.policy_version += 1;
    policy.enabled = false;
    let paused: Result<()> = update(
        &f.ic,
        f.commerce,
        f.sns,
        "register_settlement_asset",
        (policy,),
    );
    paused.unwrap();
    let transfer: Result<CashTransfer> = update(
        &f.ic,
        f.commerce,
        person(2),
        "claim_checkout_refund",
        (
            opened.progress.order_id,
            f.ledger,
            vec![incoming.block_index],
            Hash::new([142; 32]),
        ),
    );
    let transfer = transfer.unwrap();
    let audit: Result<CashTransfersPage> = query(
        &f.ic,
        f.commerce,
        person(2),
        "checkout_transfers",
        (None::<Hash>, 16u16),
    );
    assert_eq!(audit.unwrap().transfers.len(), 1);
    let outsider: Result<CashTransfersPage> = query(
        &f.ic,
        f.commerce,
        person(99),
        "checkout_transfers",
        (None::<Hash>, 16u16),
    );
    assert!(outsider.unwrap().transfers.is_empty());

    void(&f.ic, f.ledger, person(1), "set_fee", (21u128,));
    let reply: Result<CashTransferProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "process_checkout_transfer",
        (transfer.transfer_id,),
    );
    assert_eq!(reply.unwrap().status, CashTransferStatus::Rejected);
    let denied: Result<CashTransfer> = update(
        &f.ic,
        f.commerce,
        person(2),
        "revise_checkout_transfer_fee",
        (transfer.transfer_id, 21u128),
    );
    assert_eq!(denied, Err(Error::FeeBlocked));
    // Retrying the same definitely rejected args records the current ledger fee, without changing them.
    void(&f.ic, f.ledger, person(1), "set_fee", (15u128,));
    let reply: Result<CashTransferProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "process_checkout_transfer",
        (transfer.transfer_id,),
    );
    assert_eq!(reply.unwrap().status, CashTransferStatus::Rejected);
    let denied: Result<CashTransfer> = update(
        &f.ic,
        f.commerce,
        person(1),
        "revise_checkout_transfer_fee",
        (transfer.transfer_id, 15u128),
    );
    assert_eq!(denied, Err(Error::Forbidden));
    let revised: Result<CashTransfer> = update(
        &f.ic,
        f.commerce,
        person(2),
        "revise_checkout_transfer_fee",
        (transfer.transfer_id, 15u128),
    );
    let revised = revised.unwrap();
    assert_eq!(revised.to, transfer.to);
    assert_eq!(revised.ledger, transfer.ledger);
    assert_eq!(revised.max_fee_atomic, transfer.max_fee_atomic);
    assert_eq!(
        revised.amount_atomic + revised.fee_atomic,
        transfer.amount_atomic + transfer.fee_atomic
    );
    let reply: Result<CashTransferProgress> = update(
        &f.ic,
        f.commerce,
        person(2),
        "process_checkout_transfer",
        (revised.transfer_id,),
    );
    assert_eq!(reply.unwrap().status, CashTransferStatus::Succeeded);
}

#[test]
fn a_governance_module_pin_changed_during_neuron_read_never_issues_a_lease() {
    let f = Fixture::commercial();
    let id = f.create(1);
    neuron(&f, 100_000_000_000_100, false);
    let bill = offer(&f, &id, 160);
    let terms: Result<PandaApplicationTerms> = update(
        &f.ic,
        f.membership,
        person(1),
        "quote_panda_subscription",
        (bill.clone(), f.user, id, Hash::new([44; 32])),
    );
    let terms = terms.unwrap();
    let authorization = approve(
        &f,
        &id,
        &bill,
        f.membership,
        ApprovalPurpose::PandaSubscription,
        panda_application_hash(&terms),
        161,
    );
    void(
        &f.ic,
        f.sns,
        person(1),
        "change_pin_on_neuron_read",
        (f.membership, Hash::new([162; 32])),
    );
    let r: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_panda_claim",
        (PandaClaimRequest {
            terms,
            authorization,
        },),
    );
    let claim = r.unwrap();
    assert_eq!(claim.status, PandaClaimStatus::Checking);
    assert_eq!(claim.eligibility, Eligibility::Unverifiable);
    assert!(claim.cooling_until_ms.is_none());
    assert!(claim.valid_until_ms <= time(&f.ic));
    let r: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "cancel_panda_application",
        (claim.claim_id,),
    );
    assert_eq!(r.unwrap().status, PandaClaimStatus::Cancelled);
    // A pinned module is checked through canister_info, including its sole root controller.
    let verified: Result<()> = update(
        &f.ic,
        f.membership,
        person(1),
        "verify_sns_configuration",
        (),
    );
    assert_eq!(verified, Err(Error::UnsupportedProtocol));
    let module = f.ic.canister_status(f.sns, None).unwrap().module_hash.unwrap();
    let module = Hash::new(module.as_slice().try_into().unwrap());
    let r: Result<()> = update(
        &f.ic,
        f.membership,
        f.sns,
        "set_sns_governance_module_hash",
        (module,),
    );
    r.unwrap();
    let verified: Result<()> = update(
        &f.ic,
        f.membership,
        person(1),
        "verify_sns_configuration",
        (),
    );
    assert_eq!(verified, Err(Error::UnsupportedProtocol));
    f.ic.set_controllers(f.sns, None, vec![f.sns]).unwrap();
    let verified: Result<()> = update(
        &f.ic,
        f.membership,
        person(1),
        "verify_sns_configuration",
        (),
    );
    verified.unwrap();
}

#[test]
fn cancelled_applications_do_not_consume_claim_capacity() {
    let f = Fixture::commercial();
    let id = f.create(1);
    neuron(&f, 100_000_000_000_100, false);
    let r: Result<()> = update(
        &f.ic,
        f.membership,
        f.sns,
        "configure_panda_service",
        (PandaServiceConfig {
            commerce_canister: f.commerce,
            max_claims: 1,
            hourly_applications: 100,
            cooling_ms: PANDA_COOLING_MS,
        },),
    );
    r.unwrap();
    let apply = |op: u8, neuron_id: u8| -> Result<PandaClaimView> {
        let bill = offer(&f, &id, op);
        let terms: Result<PandaApplicationTerms> = update(
            &f.ic,
            f.membership,
            person(1),
            "quote_panda_subscription",
            (bill.clone(), f.user, id, Hash::new([neuron_id; 32])),
        );
        let terms = terms.unwrap();
        let authorization = approve(
            &f,
            &id,
            &bill,
            f.membership,
            ApprovalPurpose::PandaSubscription,
            panda_application_hash(&terms),
            op + 1,
        );
        update(
            &f.ic,
            f.membership,
            person(1),
            "request_panda_claim",
            (PandaClaimRequest {
                terms,
                authorization,
            },),
        )
    };
    let first = apply(170, 44).unwrap();
    assert_eq!(first.status, PandaClaimStatus::CoolingDown);
    assert_eq!(apply(172, 45), Err(Error::QuotaExceeded));
    let r: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "cancel_panda_application",
        (first.claim_id,),
    );
    assert_eq!(r.unwrap().status, PandaClaimStatus::Cancelled);
    assert_eq!(apply(174, 44).unwrap().status, PandaClaimStatus::CoolingDown);
}

#[test]
fn lost_panda_apply_ack_keeps_capacity_across_upgrade_and_reconciliation() {
    let f = Fixture::commercial();
    let account = f.create(1);
    let (home, subject) = sample(&f);
    neuron(&f, 100_000_000_000_100, false);
    let r: Result<()> = update(
        &f.ic,
        f.membership,
        f.sns,
        "configure_panda_service",
        (PandaServiceConfig {
            commerce_canister: f.commerce,
            max_claims: 1,
            hourly_applications: 100,
            cooling_ms: PANDA_COOLING_MS,
        },),
    );
    r.unwrap();
    let prepared = sample_offer(&f, home, &subject, 180, SettlementMethod::Panda);
    let terms: Result<PandaApplicationTerms> = update(
        &f.ic,
        f.membership,
        person(1),
        "quote_panda_subscription",
        (prepared.offer.clone(), f.user, account, Hash::new([44; 32])),
    );
    let terms = terms.unwrap();
    let authorize = |op| {
        let mut a = approve(
            &f,
            &account,
            &prepared.offer,
            f.membership,
            ApprovalPurpose::PandaSubscription,
            panda_application_hash(&terms),
            op,
        );
        a.product_approval = Some(prepared.approval.clone());
        a
    };
    let r: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "request_panda_claim",
        (PandaClaimRequest {
            terms: terms.clone(),
            authorization: authorize(181),
        },),
    );
    let claim = r.unwrap();
    assert_eq!(claim.status, PandaClaimStatus::CoolingDown);
    f.ic.advance_time(Duration::from_millis(PANDA_COOLING_MS + 1));
    let r: Result<()> = update(&f.ic, home, person(1), "lose_next_apply_ack", ());
    r.unwrap();
    let r: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "advance_panda_claim",
        (claim.claim_id, authorize(182)),
    );
    assert!(r.is_err());
    let r: Result<PandaClaimView> = query(
        &f.ic,
        f.membership,
        person(1),
        "get_panda_claim",
        (claim.claim_id,),
    );
    assert_eq!(r.unwrap().status, PandaClaimStatus::Applying);
    let contracts: Result<Vec<SubscriptionContract>> =
        query(&f.ic, home, person(1), "contracts", (subject,));
    assert_eq!(contracts.unwrap().len(), 1);

    // An independent product and neuron must still respect the occupied slot.
    let apply_other = |op| -> Result<PandaClaimView> {
        let bill = offer(&f, &account, op);
        let terms: Result<PandaApplicationTerms> = update(
            &f.ic,
            f.membership,
            person(1),
            "quote_panda_subscription",
            (bill.clone(), f.user, account, Hash::new([45; 32])),
        );
        let terms = terms.unwrap();
        let authorization = approve(
            &f,
            &account,
            &bill,
            f.membership,
            ApprovalPurpose::PandaSubscription,
            panda_application_hash(&terms),
            op + 1,
        );
        update(
            &f.ic,
            f.membership,
            person(1),
            "request_panda_claim",
            (PandaClaimRequest {
                terms,
                authorization,
            },),
        )
    };
    assert_eq!(apply_other(183), Err(Error::QuotaExceeded));
    f.ic.upgrade_canister(
        f.membership,
        wasm("membership"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(apply_other(185), Err(Error::QuotaExceeded));
    let r: Result<PandaClaimView> = update(
        &f.ic,
        f.membership,
        person(1),
        "reconcile_panda_claim",
        (claim.claim_id,),
    );
    assert_eq!(r.unwrap().status, PandaClaimStatus::Active);
    assert_eq!(apply_other(187), Err(Error::QuotaExceeded));
    f.ic.advance_time(Duration::from_millis(
        prepared.offer.expires_at_ms - time(&f.ic) + 1,
    ));
    // Admission sweeps the ended commitment and can reserve the freed slot.
    assert_eq!(apply_other(189).unwrap().status, PandaClaimStatus::Checking);
}
