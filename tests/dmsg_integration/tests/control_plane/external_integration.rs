use super::*;
use dmsg_protocol::{authentication::verify_authentication, integration::*};
use dmsg_types::integration::*;

fn registrations(f: &Fixture) -> (AppRegistration, ProductRegistration) {
    let product = ProductRegistration {
        version: 2,
        environment: Environment::Local,
        product_id: "sample".into(),
        config_version: 1,
        quote_authority: f.commerce,
        beneficiary_authority: f.commerce,
        adapter: f.commerce,
        subject_schema: "sample-project-v1".into(),
        subject_size: 8,
        merchant: account(person(60)),
        ledgers: vec![f.ledger],
        terms_hash: Hash::new([11; 32]),
        subsidy_budget_id: Hash::new([12; 32]),
        paused: false,
    };
    let app = AppRegistration {
        version: 1,
        environment: Environment::Local,
        app_id: "sample".into(),
        config_version: 1,
        origins: vec!["https://sample.test".into()],
        user_homes: vec![f.user],
        cose_homes: vec![f.cose],
        product_ids: vec![product.product_id.clone()],
        capabilities: vec![AppCapability::Authenticate, AppCapability::Checkout],
        profiles: vec![],
        authentication_receiver: f.commerce,
        paused: false,
    };
    let unauthorized: Result<()> = update(
        &f.ic,
        f.commerce,
        person(1),
        "register_integration_product",
        (product.clone(),),
    );
    assert_eq!(unauthorized, Err(Error::Forbidden));
    let registered: Result<()> = update(
        &f.ic,
        f.commerce,
        f.sns,
        "register_integration_product",
        (product.clone(),),
    );
    registered.unwrap();
    let registered: Result<()> = update(
        &f.ic,
        f.commerce,
        f.sns,
        "register_integration_app",
        (app.clone(),),
    );
    registered.unwrap();
    (app, product)
}

fn auth(f: &Fixture, app: &AppRegistration, op: u8) -> AuthenticationRequest {
    let at = time(&f.ic);
    AuthenticationRequest {
        version: 1,
        environment: Environment::Local,
        app_id: app.app_id.clone(),
        app_config_version: app.config_version,
        origin: app.origins[0].clone(),
        receiver: app.authentication_receiver,
        challenge_hash: Hash::new([21; 32]),
        session_key_hash: Hash::new([22; 32]),
        purpose: AuthenticationPurpose::Login,
        nonce: Hash::new([23; 32]),
        operation_id: Hash::new([op; 32]),
        issued_at_ms: at,
        expires_at_ms: at + AUTH_TTL_MS,
    }
}

fn approve<T: Serialize>(
    f: &Fixture,
    account: &AccountId,
    domain: &str,
    command: &T,
    id: Hash,
) -> Approval {
    let state = f.account_id(1, account);
    let mut approval = Approval {
        device_id: Hash::new([1; 32]),
        security_epoch: state.security_epoch,
        sequence: state.devices[&Hash::new([1; 32])].next_sequence,
        request_id: id,
        expires_at: time(&f.ic) + AUTH_TTL_MS,
        signature: ByteBuf::new(),
    };
    approval.signature = key(1)
        .sign(approval_message(f.user, account, domain, command, &approval).as_slice())
        .to_bytes()
        .to_vec()
        .into();
    approval
}

#[test]
fn external_authentication_certificate_retry_pause_and_upgrade() {
    let f = Fixture::new();
    let account = f.create(1);
    let (mut app, _) = registrations(&f);
    let request = auth(&f, &app, 40);
    let approval = approve(
        &f,
        &account,
        "dmsg/authentication/approve/v1",
        &request,
        request.operation_id,
    );
    let first: Result<AuthenticationResult> = update(
        &f.ic,
        f.user,
        person(1),
        "approve_authentication",
        (account.clone(), request.clone(), approval.clone()),
    );
    let first = first.unwrap();
    let duplicate: Result<AuthenticationResult> = update(
        &f.ic,
        f.user,
        person(1),
        "approve_authentication",
        (account.clone(), request.clone(), approval),
    );
    assert_eq!(duplicate, Ok(first));
    assert_eq!(
        f.account_id(1, &account).devices[&Hash::new([1; 32])].next_sequence,
        1
    );
    for canister in [f.user, f.commerce] {
        let name = if canister == f.user {
            "dmsg_user"
        } else {
            "dmsg_commerce"
        };
        f.ic.upgrade_canister(canister, wasm(name), candid::encode_args(()).unwrap(), None)
            .unwrap();
    }
    let proof: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(1),
        "authentication_certificate",
        (account.clone(), request.operation_id),
    );
    let proof = proof.unwrap();
    let verified = verify_authentication(
        &proof,
        &request,
        f.user,
        &f.ic.root_key().unwrap(),
        time(&f.ic),
    )
    .unwrap();
    assert_eq!(verified.account_id, account);
    if let Some(path) = std::env::var_os("DMSG_EXTERNAL_FIXTURE") {
        std::fs::write(
            path,
            canonical(&(
                1u16,
                "dmsg-authentication/1",
                request.clone(),
                proof.clone(),
                ByteBuf::from(f.ic.root_key().unwrap()),
                time(&f.ic),
            )),
        )
        .unwrap();
    }

    let hidden: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(2),
        "authentication_certificate",
        (account.clone(), request.operation_id),
    );
    assert_eq!(hidden, Err(Error::AuthRequired));
    app.paused = true;
    app.config_version += 1;
    let pause: Result<()> = update(
        &f.ic,
        f.commerce,
        f.sns,
        "register_integration_app",
        (app.clone(),),
    );
    pause.unwrap();
    let next = auth(&f, &app, 41);
    let approval = approve(
        &f,
        &account,
        "dmsg/authentication/approve/v1",
        &next,
        next.operation_id,
    );
    let denied: Result<AuthenticationResult> = update(
        &f.ic,
        f.user,
        person(1),
        "approve_authentication",
        (account.clone(), next, approval),
    );
    assert_eq!(denied, Err(Error::Locked));
    assert_eq!(
        f.account_id(1, &account).devices[&Hash::new([1; 32])].next_sequence,
        1
    );
    f.mutate(
        1,
        &account,
        AccountCommand::SetDeviceCapabilities {
            device_id: Hash::new([1; 32]),
            capabilities: vec![Capability::RootManage],
        },
    )
    .unwrap();
    let revoked: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(1),
        "authentication_certificate",
        (account.clone(), request.operation_id),
    );
    assert!(matches!(
        revoked,
        Err(Error::PolicyStale | Error::DeviceNotApproved)
    ));
}

#[test]
fn external_project_approval_never_infers_beneficiary_from_dmsg_account() {
    let f = Fixture::new();
    let account = f.create(1);
    let (app, product) = registrations(&f);
    let application = ApplicationApproval {
        version: 1,
        environment: Environment::Local,
        app_id: app.app_id,
        app_config_version: 1,
        origin: app.origins[0].clone(),
        approving_account: account.clone(),
        service: f.commerce,
        beneficiary: Beneficiary {
            authority_canister: product.beneficiary_authority,
            product_id: product.product_id,
            subject_schema: product.subject_schema,
            subject_bytes: 42u64.to_be_bytes().to_vec().into(),
        },
        actor: person(2),
        purpose: ApprovalPurpose::CashCheckout,
        action_digest: Hash::new([31; 32]),
        operation_id: Hash::new([32; 32]),
        nonce: Hash::new([33; 32]),
        expires_at_ms: time(&f.ic) + AUTH_TTL_MS,
    };
    let approval = approve(
        &f,
        &account,
        "dmsg/application/approve/v1",
        &application,
        Hash::new([34; 32]),
    );
    let approved: Result<Hash> = update(
        &f.ic,
        f.user,
        person(1),
        "approve_application",
        (application.clone(), approval),
    );
    let id = approved.unwrap();
    let verified: Result<ApplicationAuthorization> = update(
        &f.ic,
        f.user,
        f.commerce,
        "verify_application_authorization",
        (id, application.clone()),
    );
    assert_eq!(
        verified.unwrap().approval_hash,
        application_approval_hash(&application)
    );
    let wrong_caller: Result<ApplicationAuthorization> = update(
        &f.ic,
        f.user,
        person(2),
        "verify_application_authorization",
        (id, application.clone()),
    );
    assert_eq!(wrong_caller, Err(Error::Forbidden));
    let mut other = application;
    other.actor = person(3);
    let substituted: Result<ApplicationAuthorization> = update(
        &f.ic,
        f.user,
        f.commerce,
        "verify_application_authorization",
        (id, other),
    );
    assert_eq!(substituted, Err(Error::IdempotencyConflict));
}
