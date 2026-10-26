use super::*;
use dmsg_types::app_store::*;

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
        environment: StoreEnvironment::Sandbox,
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
        environment: StoreEnvironment::Sandbox,
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
    wrong_environment.environment = StoreEnvironment::Production;
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
