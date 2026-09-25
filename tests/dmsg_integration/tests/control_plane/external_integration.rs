use super::*;
use dmsg_protocol::authentication::verify_authentication;

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
        action_authority: f.commerce,
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
        signature: Default::default(),
    };
    approval.signature = key(1)
        .sign(approval_message(f.user, account, domain, command, &approval).as_slice())
        .to_bytes()
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
        (account, request.clone(), approval.clone()),
    );
    let first = first.unwrap();
    let duplicate: Result<AuthenticationResult> = update(
        &f.ic,
        f.user,
        person(1),
        "approve_authentication",
        (account, request.clone(), approval),
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
        (account, request.operation_id),
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
        (account, request.operation_id),
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
        (account, next, approval),
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
        (account, request.operation_id),
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
        approving_account: account,
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

#[test]
fn external_app_action_cannot_use_document_signer_or_consume_a_sequence() {
    use dmsg_types::app_action::*;
    let f = Fixture::new();
    let account = f.create(1);
    let state = f.account_id(1, &account);
    let at = time(&f.ic);
    let hash = Hash::new([1; 32]);
    let mut action = AppAction {
        version: 1,
        environment: Environment::Local,
        app_id: "tokenlisting".into(),
        app_config_version: 1,
        origin: "https://sample.test".into(),
        receiver: f.commerce,
        actor_id: AccountId([8; 12]),
        signing_account: account,
        operation_id: hash,
        intent_hash: hash,
        input_hash: hash,
        subject_hash: hash,
        precondition_hash: hash,
        role_snapshot_hash: hash,
        signing_policy_hash: hash,
        rule_set_hash: hash,
        issued_at_ms: at,
        expires_at_ms: at + AUTH_TTL_MS,
        command: AppActionCommand::TokenListCertifyDisclosure {
            project_id: 1,
            contract_id: 1,
            revision: 1,
        },
        files: vec![],
    };
    action.input_hash = dmsg_protocol::app_action::action_input_hash(&action.command);
    let statement = Statement {
        issuer: state.issuer.clone(),
        subject: None,
        issued_at: None,
        content: StatementContent::AppAction(Box::new(action)),
    };
    let (_, tbs) = prepare_cose(&statement, &Algorithm::Ed25519, hash.as_slice()).unwrap();
    let kind = ExecutionKind::Sign {
        key: KeyRequest {
            purpose: KeyPurpose::AppAction,
            algorithm: Algorithm::Ed25519,
            generation: 1,
        },
        to_be_signed: tbs.into(),
        public_key_fingerprint: hash,
        origin: "https://sample.test".into(),
    };
    let max_cycles = 100_000_000_000u128;
    let request_id = execution_request_id(
        &account,
        state.security_epoch,
        hash,
        state.devices[&hash].next_sequence,
    );
    let approval = approve(
        &f,
        &account,
        "dmsg/execute/v3",
        &(&kind, max_cycles),
        request_id,
    );
    let request = SignRequest {
        account_id: account,
        key: SigningKeyRef {
            algorithm: SigningAlgorithm::Ed25519,
            kid: hash.to_vec().into(),
            public_key_fingerprint: hash,
        },
        statement,
        origin: "https://sample.test".into(),
        max_cycles,
        approval,
    };
    let result: Result<ExecutionResult> = update(&f.ic, f.user, person(1), "sign", (request,));
    assert_eq!(result, Err(Error::UnsupportedProtocol));
    assert_eq!(
        f.account_id(1, &account).devices[&hash].next_sequence,
        state.devices[&hash].next_sequence
    );
}

#[test]
fn external_action_authority_signature_receipt_replay_and_callback_pause() {
    use dmsg_types::app_action::*;
    let f = Fixture::commercial();
    let account = f.create(1);
    f.recoverable(1, &account);
    let ready: Result<KeyState> =
        update(&f.ic, f.cose, Principal::anonymous(), "initialize_keys", ());
    ready.unwrap();
    let (mut app, _) = registrations(&f);
    app.app_id = "sample-actions".into();
    app.action_authority = f.sns;
    app.capabilities.push(AppCapability::SignAction);
    app.profiles.push(SigningProfile::AppActionV1);
    let registered: Result<()> = update(
        &f.ic,
        f.commerce,
        f.sns,
        "register_integration_app",
        (app.clone(),),
    );
    registered.unwrap();
    let at = time(&f.ic);
    let hash = Hash::new([1; 32]);
    let mut action = AppAction {
        version: 1,
        environment: Environment::Local,
        app_id: app.app_id.clone(),
        app_config_version: 1,
        origin: app.origins[0].clone(),
        receiver: f.commerce,
        actor_id: AccountId([8; 12]),
        signing_account: account,
        operation_id: Hash::new([79; 32]),
        intent_hash: hash,
        input_hash: hash,
        subject_hash: hash,
        precondition_hash: hash,
        role_snapshot_hash: hash,
        signing_policy_hash: hash,
        rule_set_hash: hash,
        issued_at_ms: at,
        expires_at_ms: at + AUTH_TTL_MS,
        command: AppActionCommand::TokenListCertifyDisclosure {
            project_id: 1,
            contract_id: 1,
            revision: 1,
        },
        files: vec![],
    };
    action.input_hash = dmsg_protocol::app_action::action_input_hash(&action.command);
    let signing_key = f.key_ref(
        &account,
        SigningPurpose::AppAction,
        SigningAlgorithm::Ed25519,
    );
    let request = |body: AppAction| {
        let state = f.account_id(1, &account);
        let device = &state.devices[&hash];
        let mut request = AppActionSignRequest {
            account_id: account,
            key: signing_key.clone(),
            issuer: state.issuer,
            action: body,
            max_cycles: 100_000_000_000,
            approval: Approval {
                device_id: hash,
                security_epoch: state.security_epoch,
                sequence: device.next_sequence,
                request_id: execution_request_id(
                    &account,
                    state.security_epoch,
                    hash,
                    device.next_sequence,
                ),
                expires_at: at + AUTH_TTL_MS,
                signature: Default::default(),
            },
        };
        request.approval.signature = key(1)
            .sign(
                request
                    .clone()
                    .into_execution()
                    .unwrap()
                    .approval_message(f.user)
                    .as_slice(),
            )
            .to_bytes()
            .into();
        request
    };
    let req = request(action.clone());
    let before = f.account_id(1, &account).devices[&hash].next_sequence;
    let refused: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "sign_app_action", (req.clone(),));
    assert_eq!(refused, Err(Error::Forbidden));
    assert_eq!(
        f.account_id(1, &account).devices[&hash].next_sequence,
        before
    );
    update::<_, ()>(
        &f.ic,
        f.sns,
        person(1),
        "set_action_approval",
        (
            f.user,
            account,
            dmsg_protocol::app_action::app_action_digest(&action),
            None::<(Principal, AppRegistration)>,
        ),
    );
    let preview: Result<()> = update(
        &f.ic,
        f.user,
        person(1),
        "inspect_app_action",
        (account, action.clone()),
    );
    preview.unwrap();
    let signed: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "sign_app_action", (req.clone(),));
    let signed = signed.unwrap();
    let ExecutionOutput::Signature {
        artifact,
        key: descriptor,
    } = signed.output().unwrap()
    else {
        panic!()
    };
    assert_eq!(descriptor.purpose, KeyPurpose::AppAction);
    let proof: Result<CertifiedBatch> = query(
        &f.ic,
        f.user,
        person(1),
        "get_execution_receipt",
        (account, req.approval.request_id),
    );
    let (value, _) = dmsg_protocol::authentication::verify_certified_leaf(
        &proof.unwrap(),
        &execution_receipt_key(&account, req.approval.request_id),
        f.user,
        &f.ic.root_key().unwrap(),
        time(&f.ic),
    )
    .unwrap();
    let receipt: ExecutionReceipt = decode_canonical(&value).unwrap();
    match_execution_receipt(artifact, &receipt).unwrap();
    for (home, name) in [(f.user, "dmsg_user"), (f.cose, "dmsg_cose")] {
        f.ic.upgrade_canister(home, wasm(name), candid::encode_args(()).unwrap(), None)
            .unwrap();
    }
    let repeated: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "sign_app_action", (req,));
    assert_eq!(repeated, Ok(signed));
    let seq = f.account_id(1, &account).devices[&hash].next_sequence;
    action.operation_id = Hash::new([80; 32]);
    let next = request(action.clone());
    app.config_version = 2;
    app.paused = true;
    update::<_, ()>(
        &f.ic,
        f.sns,
        person(1),
        "set_action_approval",
        (
            f.user,
            account,
            dmsg_protocol::app_action::app_action_digest(&action),
            Some((f.commerce, app)),
        ),
    );
    let paused: Result<ExecutionResult> =
        update(&f.ic, f.user, person(1), "sign_app_action", (next,));
    assert_eq!(paused, Err(Error::PolicyStale));
    assert_eq!(f.account_id(1, &account).devices[&hash].next_sequence, seq);
}
