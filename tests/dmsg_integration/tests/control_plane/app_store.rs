use super::*;
use dmsg_types::app_store::*;

#[test]
fn expired_apple_subscription_can_be_purchased_by_another_account_without_moving_receipts() {
    for environment in [StoreEnvironment::Production, StoreEnvironment::Sandbox] {
        assert_expired_subscription_repurchase(environment);
    }
}

fn assert_expired_subscription_repurchase(environment: StoreEnvironment) {
    let f = Fixture::commercial();
    let a = f.create(1);
    let b = f.create(2);
    let verifier = person(90);
    let mut config = AppStoreConfig {
        verifiers: vec![verifier],
        bundle_id: "net.dmsg.ios".into(),
        app_apple_id: 6827050680,
        environment: StoreEnvironment::Production,
        accept_sandbox: Some(true),
        products: vec![StoreProduct {
            product_id: "test.max.annual".into(),
            kind: StoreProductKind::Plan(PlanId::Max),
        }],
    };
    let configured: Result<()> = update(
        &f.ic,
        f.commerce,
        f.sns,
        "set_app_store_config",
        (config.clone(),),
    );
    configured.unwrap();
    let deliver = |t: &StoreTransaction| -> Result<EntitlementView> {
        update(
            &f.ic,
            f.commerce,
            verifier,
            "apply_app_store_transaction",
            (t.clone(),),
        )
    };
    let now = time(&f.ic);
    let original = StoreTransaction {
        beneficiary: beneficiary(f.user, &a),
        transaction_id: "3001".into(),
        original_transaction_id: "3000".into(),
        product_id: "test.max.annual".into(),
        app_account_token: dmsg_protocol::app_store::account_token(f.user, &a),
        bundle_id: "net.dmsg.ios".into(),
        environment,
        purchased_at_ms: now,
        expires_at_ms: Some(now + DAY),
        revoked_at_ms: None,
        signed_at_ms: now,
        checked_at_ms: now,
    };
    assert_eq!(
        deliver(&original).unwrap().plan_snapshot.plan_id,
        PlanId::Max
    );
    let mut second = original.clone();
    second.beneficiary = beneficiary(f.user, &b);
    second.app_account_token = dmsg_protocol::app_store::account_token(f.user, &b);
    // The same paid transaction can never move, even with a changed token.
    assert_eq!(deliver(&second), Err(Error::Forbidden));
    second.transaction_id = "3002".into();
    assert_eq!(deliver(&second), Err(Error::Forbidden));
    f.ic.advance_time(Duration::from_secs(2 * 86400));
    second.purchased_at_ms = time(&f.ic);
    second.expires_at_ms = Some(second.purchased_at_ms + DAY);
    second.signed_at_ms = second.purchased_at_ms;
    second.checked_at_ms = second.purchased_at_ms;
    let accepted = deliver(&second).unwrap();
    assert_eq!(accepted.plan_snapshot.plan_id, PlanId::Max);
    assert_eq!(
        deliver(&second).unwrap().business_revision,
        accepted.business_revision
    );

    // Closing Sandbox admission must still admit old-account refunds after B
    // takes over the original subscription, but cannot admit a new account.
    config.accept_sandbox = Some(false);
    let configured: Result<()> = update(
        &f.ic,
        f.commerce,
        f.sns,
        "set_app_store_config",
        (config.clone(),),
    );
    configured.unwrap();

    let mut old = original.clone();
    old.signed_at_ms = time(&f.ic);
    old.checked_at_ms = time(&f.ic);
    // Old-account restore/refund stays with A and cannot take B's current term.
    assert_eq!(deliver(&old).unwrap().plan_snapshot.plan_id, PlanId::Free);
    old.revoked_at_ms = Some(original.purchased_at_ms + DAY / 2);
    old.signed_at_ms += 1;
    assert_eq!(deliver(&old).unwrap().plan_snapshot.plan_id, PlanId::Free);
    old.expires_at_ms = second.expires_at_ms;
    old.signed_at_ms += 1;
    assert_eq!(deliver(&old), Err(Error::Forbidden));
    let mut third = second.clone();
    third.beneficiary = original.beneficiary.clone();
    third.app_account_token = original.app_account_token.clone();
    third.transaction_id = "3003".into();
    assert_eq!(deliver(&third), Err(Error::Forbidden));

    // Monthly pruning and upgrade must not make an overlapping term eligible.
    f.ic.advance_time(Duration::from_secs(40 * 86400));
    let refreshed: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        person(2),
        "refresh_entitlement",
        (&second.beneficiary,),
    );
    assert_eq!(refreshed.unwrap().plan_snapshot.plan_id, PlanId::Free);
    f.ic.upgrade_canister(
        f.commerce,
        wasm("dmsg_commerce"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    third.signed_at_ms = time(&f.ic);
    third.checked_at_ms = time(&f.ic);
    third.expires_at_ms = Some(time(&f.ic) + DAY);
    assert_eq!(deliver(&third), Err(Error::Forbidden));
    third.purchased_at_ms = time(&f.ic);
    if third.environment == StoreEnvironment::Sandbox {
        assert_eq!(deliver(&third), Err(Error::Forbidden));
    }
    config.accept_sandbox = Some(true);
    let configured: Result<()> =
        update(&f.ic, f.commerce, f.sns, "set_app_store_config", (config,));
    configured.unwrap();
    assert_eq!(deliver(&third).unwrap().plan_snapshot.plan_id, PlanId::Max);
    let mut moved_receipt = second.clone();
    moved_receipt.beneficiary = original.beneficiary;
    moved_receipt.app_account_token = original.app_account_token;
    moved_receipt.signed_at_ms = time(&f.ic);
    moved_receipt.checked_at_ms = time(&f.ic);
    assert_eq!(deliver(&moved_receipt), Err(Error::Forbidden));
}

#[test]
fn app_store_verifier_binding_idempotence_refunds_and_upgrade() {
    let f = Fixture::commercial();
    let account = f.create(1);
    let other = f.create(2);
    let verifier = person(90);
    let config = AppStoreConfig {
        verifiers: vec![verifier],
        bundle_id: "net.dmsg.ios".into(),
        app_apple_id: 6827050680,
        environment: StoreEnvironment::Production,
        accept_sandbox: None,
        products: vec![
            StoreProduct {
                product_id: "test.plus.annual".into(),
                kind: StoreProductKind::Plan(PlanId::Plus),
            },
            StoreProduct {
                product_id: "test.storage".into(),
                kind: StoreProductKind::Storage(Hash::new([120; 32])),
            },
        ],
    };
    let preview = |config: &AppStoreConfig| -> std::result::Result<String, String> {
        query(
            &f.ic,
            f.commerce,
            person(9),
            "validate_set_app_store_config",
            (config.clone(),),
        )
    };
    let mut invalid = config.clone();
    invalid.verifiers.clear();
    assert_eq!(preview(&invalid), Err("InvalidInput(\"verifiers\")".into()));
    assert!(
        preview(&config)
            .unwrap()
            .starts_with("Set App Store config for net.dmsg.ios")
    );
    let denied: Result<()> = update(
        &f.ic,
        f.commerce,
        person(1),
        "set_app_store_config",
        (config.clone(),),
    );
    assert_eq!(denied, Err(Error::Forbidden));
    let applied: Result<()> = update(
        &f.ic,
        f.commerce,
        f.sns,
        "set_app_store_config",
        (config.clone(),),
    );
    applied.unwrap();
    assert!(preview(&config).unwrap().ends_with("; no change."));
    let now = time(&f.ic);
    let mut transaction = StoreTransaction {
        beneficiary: beneficiary(f.user, &account),
        transaction_id: "1001".into(),
        original_transaction_id: "1000".into(),
        product_id: "test.plus.annual".into(),
        app_account_token: dmsg_protocol::app_store::account_token(f.user, &account),
        bundle_id: config.bundle_id.clone(),
        environment: StoreEnvironment::Production,
        purchased_at_ms: now,
        expires_at_ms: Some(now + 365 * DAY),
        revoked_at_ms: None,
        signed_at_ms: now,
        checked_at_ms: now,
    };
    let denied: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "apply_app_store_transaction",
        (transaction.clone(),),
    );
    assert_eq!(denied, Err(Error::Forbidden));
    let view: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        verifier,
        "apply_app_store_transaction",
        (transaction.clone(),),
    );
    let view = view.unwrap();
    assert_eq!(view.plan_snapshot.plan_id, PlanId::Plus);
    let repeated: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        verifier,
        "apply_app_store_transaction",
        (transaction.clone(),),
    );
    assert_eq!(view.business_revision, repeated.unwrap().business_revision);
    // Apple renews the term itself: no cash or PANDA base term while it lasts.
    let renewal: Result<BillingOffer> = update(
        &f.ic,
        f.commerce,
        person(1),
        "prepare_account_subscription",
        (
            "dmsg".to_string(),
            beneficiary(f.user, &account),
            "plus".to_string(),
            Hash::new([7; 32]),
        ),
    );
    assert_eq!(renewal, Err(Error::Forbidden));
    let mut forged = transaction.clone();
    forged.beneficiary = beneficiary(f.user, &other);
    forged.app_account_token = dmsg_protocol::app_store::account_token(f.user, &other);
    let denied: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        verifier,
        "apply_app_store_transaction",
        (forged,),
    );
    assert_eq!(denied, Err(Error::Forbidden));
    let mut wrong_environment = transaction.clone();
    wrong_environment.environment = StoreEnvironment::Sandbox;
    let denied: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        verifier,
        "apply_app_store_transaction",
        (wrong_environment,),
    );
    assert_eq!(denied, Err(Error::Forbidden));

    // Exercise the actual stable cells/maps, not just a pure projection.
    f.ic.upgrade_canister(
        f.commerce,
        wasm("dmsg_commerce"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let retained: Option<AppStoreConfig> = query(
        &f.ic,
        f.commerce,
        Principal::anonymous(),
        "get_app_store_config",
        (),
    );
    assert_eq!(retained, Some(config));
    let terms: Result<MembershipTerms> = query(
        &f.ic,
        f.commerce,
        Principal::anonymous(),
        "get_membership_terms",
        (&transaction.beneficiary,),
    );
    assert_eq!(terms.unwrap().contracts.len(), 1);
    let repeated: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        verifier,
        "apply_app_store_transaction",
        (transaction.clone(),),
    );
    assert_eq!(repeated.unwrap().business_revision, view.business_revision);

    let mut storage = transaction.clone();
    storage.transaction_id = "2001".into();
    storage.original_transaction_id = "2000".into();
    storage.product_id = "test.storage".into();
    storage.expires_at_ms = None;
    storage.checked_at_ms = time(&f.ic);
    let addon: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        verifier,
        "apply_app_store_transaction",
        (storage.clone(),),
    );
    assert_eq!(addon.unwrap().addons.len(), 1);
    let repeated: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        verifier,
        "apply_app_store_transaction",
        (storage.clone(),),
    );
    assert_eq!(repeated.unwrap().addons.len(), 1);

    f.ic.advance_time(Duration::from_secs(61 * 60));
    let stale: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "refresh_entitlement",
        (&transaction.beneficiary,),
    );
    assert_eq!(stale.unwrap().source_status, SourceStatus::Unverifiable);
    transaction.checked_at_ms = time(&f.ic);
    let fresh: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        verifier,
        "apply_app_store_transaction",
        (transaction.clone(),),
    );
    assert_eq!(fresh.unwrap().source_status, SourceStatus::Active);
    let old = transaction.clone();
    transaction.signed_at_ms = time(&f.ic);
    transaction.revoked_at_ms = Some(transaction.signed_at_ms);
    let revoked: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        verifier,
        "apply_app_store_transaction",
        (transaction.clone(),),
    );
    assert_eq!(revoked.unwrap().plan_snapshot.plan_id, PlanId::Free);
    let rollback: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        verifier,
        "apply_app_store_transaction",
        (old,),
    );
    assert_eq!(rollback, Err(Error::VersionConflict));
    // Once the App Store term ended, other payment opens again.
    assert_eq!(offer_sku(&f, &account, "plus", 8).sku, "plus".to_string());
}

#[test]
fn production_and_sandbox_subscriptions_are_isolated_and_survive_upgrade() {
    let f = Fixture::commercial();
    let account = f.create(1);
    let verifier = person(90);
    let mut config = AppStoreConfig {
        verifiers: vec![verifier],
        bundle_id: "net.dmsg.ios".into(),
        app_apple_id: 6827050680,
        environment: StoreEnvironment::Production,
        accept_sandbox: None,
        products: vec![
            StoreProduct {
                product_id: "test.plus".into(),
                kind: StoreProductKind::Plan(PlanId::Plus),
            },
            StoreProduct {
                product_id: "test.max".into(),
                kind: StoreProductKind::Plan(PlanId::Max),
            },
        ],
    };
    let set = |c: &AppStoreConfig| {
        let r: Result<()> = update(
            &f.ic,
            f.commerce,
            f.sns,
            "set_app_store_config",
            (c.clone(),),
        );
        r.unwrap();
    };
    let apply = |t: &StoreTransaction| -> Result<EntitlementView> {
        update(
            &f.ic,
            f.commerce,
            verifier,
            "apply_app_store_transaction",
            (t.clone(),),
        )
    };
    let now = time(&f.ic);
    let mut sandbox = StoreTransaction {
        beneficiary: beneficiary(f.user, &account),
        transaction_id: "101".into(),
        original_transaction_id: "100".into(),
        product_id: "test.plus".into(),
        app_account_token: dmsg_protocol::app_store::account_token(f.user, &account),
        bundle_id: config.bundle_id.clone(),
        environment: StoreEnvironment::Sandbox,
        purchased_at_ms: now,
        expires_at_ms: Some(now + 3 * MINUTE),
        revoked_at_ms: None,
        signed_at_ms: now,
        checked_at_ms: now,
    };
    set(&config);
    assert_eq!(apply(&sandbox), Err(Error::Forbidden));
    config.accept_sandbox = Some(true);
    set(&config);
    assert_eq!(apply(&sandbox).unwrap().plan_snapshot.plan_id, PlanId::Plus);
    let mut paid = sandbox.clone();
    paid.environment = StoreEnvironment::Production;
    paid.expires_at_ms = Some(now + MINUTE);
    let real = apply(&paid).unwrap();
    assert_ne!(
        real.active_contract_id,
        Some(dmsg_protocol::digest(
            "dmsg/app-store/transaction/v1",
            &(&sandbox.environment, &sandbox.transaction_id)
        ))
    );
    f.ic.advance_time(Duration::from_secs(10));
    sandbox.transaction_id = "102".into();
    sandbox.product_id = "test.max".into();
    sandbox.purchased_at_ms = time(&f.ic);
    sandbox.signed_at_ms = time(&f.ic);
    sandbox.checked_at_ms = time(&f.ic);
    let hidden = apply(&sandbox).unwrap();
    assert_eq!(hidden.plan_snapshot.plan_id, PlanId::Plus);
    assert_eq!(hidden.active_contract_id, real.active_contract_id);
    let repeated = apply(&sandbox).unwrap();
    assert_eq!(repeated.business_revision, hidden.business_revision);
    f.ic.upgrade_canister(
        f.commerce,
        wasm("dmsg_commerce"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let terms: Result<MembershipTerms> = query(
        &f.ic,
        f.commerce,
        Principal::anonymous(),
        "get_membership_terms",
        (&sandbox.beneficiary,),
    );
    let terms = terms.unwrap();
    assert_eq!(terms.contracts.len(), 3);
    assert_eq!(
        terms
            .contracts
            .iter()
            .filter(|c| c.source.is_sandbox())
            .count(),
        2
    );
    config.accept_sandbox = Some(false);
    set(&config);
    let mut unknown = sandbox.clone();
    unknown.transaction_id = "999".into();
    unknown.original_transaction_id = "999".into();
    assert_eq!(apply(&unknown), Err(Error::Forbidden));
    assert_eq!(apply(&sandbox).unwrap().plan_snapshot.plan_id, PlanId::Plus);
    f.ic.advance_time(Duration::from_secs(51));
    let fallback: Result<EntitlementView> = update(
        &f.ic,
        f.commerce,
        person(1),
        "refresh_entitlement",
        (&sandbox.beneficiary,),
    );
    assert_eq!(fallback.unwrap().plan_snapshot.plan_id, PlanId::Max);
    sandbox.revoked_at_ms = Some(time(&f.ic));
    sandbox.signed_at_ms = time(&f.ic);
    sandbox.checked_at_ms = time(&f.ic);
    assert_eq!(apply(&sandbox).unwrap().plan_snapshot.plan_id, PlanId::Free);
}

#[test]
#[ignore = "requires DMSG_PRE_IAP_COMMERCE_WASM built from the pre-change commit"]
fn legacy_production_subscription_survives_dual_environment_upgrade() {
    let legacy =
        std::fs::read(std::env::var("DMSG_PRE_IAP_COMMERCE_WASM").expect("pre-change Wasm path"))
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
            legacy,
        ],
    );
    let account = f.create(1);
    let verifier = person(90);
    let now = time(&f.ic);
    let mut config = AppStoreConfig {
        verifiers: vec![verifier],
        bundle_id: "net.dmsg.ios".into(),
        app_apple_id: 6827050680,
        environment: StoreEnvironment::Production,
        accept_sandbox: None,
        products: vec![StoreProduct {
            product_id: "test.plus".into(),
            kind: StoreProductKind::Plan(PlanId::Plus),
        }],
    };
    let set = |c: &AppStoreConfig| {
        let r: Result<()> = update(
            &f.ic,
            f.commerce,
            f.sns,
            "set_app_store_config",
            (c.clone(),),
        );
        r.unwrap();
    };
    let apply = |t: &StoreTransaction| -> Result<EntitlementView> {
        update(
            &f.ic,
            f.commerce,
            verifier,
            "apply_app_store_transaction",
            (t.clone(),),
        )
    };
    set(&config);
    let paid = StoreTransaction {
        beneficiary: beneficiary(f.user, &account),
        transaction_id: "101".into(),
        original_transaction_id: "100".into(),
        product_id: "test.plus".into(),
        app_account_token: dmsg_protocol::app_store::account_token(f.user, &account),
        bundle_id: config.bundle_id.clone(),
        environment: StoreEnvironment::Production,
        purchased_at_ms: now,
        expires_at_ms: Some(now + 365 * DAY),
        revoked_at_ms: None,
        signed_at_ms: now,
        checked_at_ms: now,
    };
    let before = apply(&paid).unwrap();
    let terms: Result<MembershipTerms> = query(
        &f.ic,
        f.commerce,
        Principal::anonymous(),
        "get_membership_terms",
        (&paid.beneficiary,),
    );
    assert!(matches!(
        &terms.unwrap().contracts[0].source,
        ContractSource::AppStore {
            environment: None,
            ..
        }
    ));
    f.ic.upgrade_canister(
        f.commerce,
        wasm("dmsg_commerce"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let restored = apply(&paid).unwrap();
    assert_eq!(restored.active_contract_id, before.active_contract_id);
    let terms: Result<MembershipTerms> = query(
        &f.ic,
        f.commerce,
        Principal::anonymous(),
        "get_membership_terms",
        (&paid.beneficiary,),
    );
    assert!(matches!(
        &terms.unwrap().contracts[0].source,
        ContractSource::AppStore {
            environment: Some(StoreEnvironment::Production),
            ..
        }
    ));
    let retained: Option<AppStoreConfig> = query(
        &f.ic,
        f.commerce,
        Principal::anonymous(),
        "get_app_store_config",
        (),
    );
    assert_eq!(retained.as_ref().unwrap().accept_sandbox, None);
    let mut sandbox = paid.clone();
    sandbox.environment = StoreEnvironment::Sandbox;
    assert_eq!(apply(&sandbox), Err(Error::Forbidden));
    config.accept_sandbox = Some(true);
    set(&config);
    assert_eq!(
        apply(&sandbox).unwrap().active_contract_id,
        before.active_contract_id
    );
    sandbox.revoked_at_ms = Some(now);
    sandbox.signed_at_ms = now + 1;
    assert_eq!(
        apply(&sandbox).unwrap().active_contract_id,
        before.active_contract_id
    );
}
