use super::*;
use dmsg_types::google_play::*;

fn configure(f: &Fixture) -> GooglePlayConfig {
    let config = GooglePlayConfig {
        verifiers: vec![person(90)],
        package_name: "net.dmsg.android".into(),
        environment: PlayEnvironment::Production,
        accept_test: true,
        products: vec![
            PlayProduct {
                product_id: "example.plus".into(),
                offer_id: None,
                kind: PlayProductKind::Plan {
                    plan: PlanId::Plus,
                    base_plan_id: "annual".into(),
                },
                enabled: true,
            },
            PlayProduct {
                product_id: "example.max".into(),
                offer_id: None,
                kind: PlayProductKind::Plan {
                    plan: PlanId::Max,
                    base_plan_id: "annual".into(),
                },
                enabled: true,
            },
            PlayProduct {
                product_id: "example.storage".into(),
                offer_id: None,
                kind: PlayProductKind::Storage {
                    product: Hash::new([120; 32]),
                    purchase_option_id: "buy".into(),
                },
                enabled: true,
            },
        ],
    };
    let result: Result<()> = update(
        &f.ic,
        f.commerce,
        f.sns,
        "set_google_play_config",
        (&config,),
    );
    result.unwrap();
    config
}

fn transaction(f: &Fixture, account: &AccountId) -> PlayTransaction {
    let at = time(&f.ic);
    PlayTransaction {
        beneficiary: beneficiary(f.user, account),
        token_hash: Hash::new([31; 32]),
        order_id: Some("GPA.example..0".into()),
        product_id: "example.plus".into(),
        option_id: "annual".into(),
        offer_id: None,
        account_binding: dmsg_protocol::google_play::account_token(f.user, account),
        package_name: "net.dmsg.android".into(),
        environment: PlayEnvironment::Production,
        state: PlayState::Active,
        purchased_at_ms: at,
        term_starts_at_ms: Some(at),
        expires_at_ms: Some(at + 365 * DAY),
        linked_token_hash: None,
        base_contract_id: None,
        replaced: false,
        revision: 1,
        checked_at_ms: at,
    }
}

fn deliver(f: &Fixture, t: &PlayTransaction) -> Result<EntitlementView> {
    update(
        &f.ic,
        f.commerce,
        person(90),
        "apply_google_play_transaction",
        (t,),
    )
}

fn receipt(f: &Fixture, t: &PlayTransaction) -> PlayReceipt {
    let result: Result<Option<PlayReceipt>> = query(
        &f.ic,
        f.commerce,
        person(90),
        "get_google_play_receipt",
        (&t.environment, t.token_hash),
    );
    result.unwrap().unwrap()
}

#[test]
fn play_receipts_authorization_replacement_refund_and_upgrade_are_durable() {
    let f = Fixture::commercial();
    let owner = f.create(1);
    let other = f.create(2);
    let configured = configure(&f);
    let mut t = transaction(&f, &owner);
    let denied: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "apply_google_play_transaction",
        (&t,),
    );
    assert_eq!(denied, Err(Error::Forbidden));
    let first = deliver(&f, &t).unwrap();
    assert_eq!(first.plan_snapshot.plan_id, PlanId::Plus);
    assert_eq!(
        deliver(&f, &t).unwrap().business_revision,
        first.business_revision
    );
    let base_id = receipt(&f, &t).transaction_id;
    let denied: Result<Option<PlayReceipt>> = query(
        &f.ic,
        f.commerce,
        person(1),
        "get_google_play_receipt",
        (&t.environment, t.token_hash),
    );
    assert_eq!(denied, Err(Error::Forbidden));
    let mut stolen = t.clone();
    stolen.beneficiary = beneficiary(f.user, &other);
    stolen.account_binding = dmsg_protocol::google_play::account_token(f.user, &other);
    stolen.revision += 1;
    assert_eq!(deliver(&f, &stolen), Err(Error::Forbidden));
    let cash: Result<BillingOffer> = update(
        &f.ic,
        f.commerce,
        person(1),
        "prepare_account_subscription",
        (
            "dmsg".to_string(),
            t.beneficiary.clone(),
            "plus".to_string(),
            Hash::new([90; 32]),
        ),
    );
    assert_eq!(cash, Err(Error::Forbidden));

    f.ic.advance_time(Duration::from_secs(60));
    let mut storage = PlayTransaction {
        term_starts_at_ms: None,
        token_hash: Hash::new([32; 32]),
        order_id: None,
        product_id: "example.storage".into(),
        option_id: "buy".into(),
        purchased_at_ms: time(&f.ic),
        checked_at_ms: time(&f.ic),
        expires_at_ms: None,
        base_contract_id: Some(base_id),
        ..t.clone()
    };
    let addon = deliver(&f, &storage).unwrap();
    assert_eq!(addon.addons.len(), 1);
    assert_eq!(addon.addons[0].expires_at_ms, t.expires_at_ms.unwrap());
    storage.order_id = Some("GPA.late-storage-id".into());
    storage.revision += 1;
    assert_eq!(deliver(&f, &storage).unwrap().addons.len(), 1);

    f.ic.upgrade_canister(
        f.commerce,
        wasm("dmsg_commerce"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let retained: Option<GooglePlayConfig> = query(
        &f.ic,
        f.commerce,
        Principal::anonymous(),
        "get_google_play_config",
        (),
    );
    assert_eq!(retained, Some(configured));
    assert!(receipt(&f, &storage).delivered);
    assert_eq!(deliver(&f, &storage).unwrap().addons.len(), 1);

    // A deferred change immediately transfers the old period to a new token.
    // Its order/anchor remain old; the new interval starts at the actual handoff.
    let transferred = PlayTransaction {
        token_hash: Hash::new([34; 32]),
        linked_token_hash: Some(t.token_hash),
        purchased_at_ms: time(&f.ic),
        checked_at_ms: time(&f.ic),
        ..t.clone()
    };
    assert_eq!(
        deliver(&f, &transferred).unwrap().plan_snapshot.plan_id,
        PlanId::Plus
    );
    let terms: Result<MembershipTerms> = query(
        &f.ic,
        f.commerce,
        Principal::anonymous(),
        "get_membership_terms",
        (&t.beneficiary,),
    );
    let terms = terms.unwrap();
    let current = terms
        .contracts
        .iter()
        .find(|c| c.contract_id == receipt(&f, &transferred).transaction_id)
        .unwrap();
    assert_eq!(current.term_starts_at_ms, t.purchased_at_ms);
    assert_eq!(current.starts_at_ms, transferred.purchased_at_ms);
    // Pending replacement never closes the original term.
    let mut replacement = PlayTransaction {
        term_starts_at_ms: Some(time(&f.ic)),
        token_hash: Hash::new([33; 32]),
        order_id: Some("GPA.replacement..0".into()),
        product_id: "example.max".into(),
        linked_token_hash: Some(transferred.token_hash),
        purchased_at_ms: time(&f.ic),
        checked_at_ms: time(&f.ic),
        state: PlayState::Pending,
        ..t.clone()
    };
    assert_eq!(deliver(&f, &replacement), Err(Error::Forbidden));
    replacement.state = PlayState::Active;
    let upgraded = deliver(&f, &replacement).unwrap();
    assert_eq!(upgraded.plan_snapshot.plan_id, PlanId::Max);
    assert_eq!(upgraded.addons.len(), 1);
    t.revision += 1;
    t.checked_at_ms = time(&f.ic);
    assert_eq!(
        deliver(&f, &t),
        Err(Error::VersionConflict),
        "linked predecessor cannot revive after replacement"
    );
    t.replaced = true;
    t.state = PlayState::Expired;
    deliver(&f, &t).unwrap();
    storage.state = PlayState::Refunded;
    storage.revision += 1;
    storage.checked_at_ms = time(&f.ic);
    let refunded = deliver(&f, &storage).unwrap();
    assert!(refunded.addons.is_empty());
    assert_eq!(refunded.plan_snapshot.plan_id, PlanId::Max);
    storage.state = PlayState::Active;
    storage.revision += 1;
    assert_eq!(deliver(&f, &storage), Err(Error::VersionConflict));
    let mut reversed = replacement.clone();
    reversed.state = PlayState::OnHold;
    reversed.revision += 1;
    reversed.checked_at_ms = time(&f.ic);
    assert_eq!(
        deliver(&f, &reversed).unwrap().source_status,
        SourceStatus::Suspended
    );
    f.ic.advance_time(Duration::from_secs(60));
    reversed.state = PlayState::Grace;
    reversed.revision += 1;
    reversed.checked_at_ms = time(&f.ic);
    assert_eq!(
        deliver(&f, &reversed).unwrap().plan_snapshot.plan_id,
        PlanId::Max
    );
}

#[test]
#[ignore = "requires DMSG_PRE_GOOGLE_PLAY_WASM built from the previous public commit"]
fn play_adapter_upgrade_preserves_existing_cash_subject_and_new_receipts() {
    let old = std::fs::read(
        std::env::var_os("DMSG_PRE_GOOGLE_PLAY_WASM").expect("previous commerce Wasm"),
    )
    .unwrap();
    let f = Fixture::with_modules(
        false,
        100,
        MAX_PAYMENT_ESCROWS,
        |_| {},
        [
            wasm("dmsg_user"),
            wasm("dmsg_directory"),
            wasm("dmsg_handle"),
            old,
        ],
    );
    let paid = f.create(1);
    let future_play = f.create(2);
    pay(&f, &paid, "pro", 160);
    let b = beneficiary(f.user, &paid);
    let before: Result<MembershipTerms> = query(
        &f.ic,
        f.commerce,
        Principal::anonymous(),
        "get_membership_terms",
        (&b,),
    );
    let before = before.unwrap();
    assert_eq!(before.contracts.len(), 1);
    f.ic.upgrade_canister(
        f.commerce,
        wasm("dmsg_commerce"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let after: Result<MembershipTerms> = query(
        &f.ic,
        f.commerce,
        Principal::anonymous(),
        "get_membership_terms",
        (&b,),
    );
    assert_eq!(after.unwrap(), before);
    configure(&f);
    let t = transaction(&f, &future_play);
    assert_eq!(deliver(&f, &t).unwrap().plan_snapshot.plan_id, PlanId::Plus);
    assert!(receipt(&f, &t).delivered);
    f.ic.upgrade_canister(
        f.commerce,
        wasm("dmsg_commerce"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    assert!(receipt(&f, &t).delivered);
    let preserved: Result<MembershipTerms> = query(
        &f.ic,
        f.commerce,
        Principal::anonymous(),
        "get_membership_terms",
        (&b,),
    );
    assert_eq!(preserved.unwrap(), before);
}
