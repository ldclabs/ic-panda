use super::*;
use dmsg_protocol::authentication::verify_authentication;

fn registrations(f: &Fixture) -> (AppRegistration, ProductRegistration) {
    let product = ProductRegistration {
        version: 2,
        environment: Environment::Local,
        product_id: "sample".into(),
        config_version: 1,
        quote_authority: f.commerce,
        beneficiary_authorities: vec![f.commerce],
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
        product_ids: vec![product.product_id.clone()],
        capabilities: vec![AppCapability::Authenticate, AppCapability::Checkout],
        profiles: vec![],
        authentication_receiver: f.commerce,
        action_authority: f.commerce,
        action_schema: None,
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

fn action_schema() -> dmsg_types::app_action::ActionSchema {
    use dmsg_types::app_action::*;
    let label = |text: &str| {
        vec![ActionLabel {
            locale: "en".into(),
            text: text.into(),
        }]
    };
    ActionSchema {
        version: 1,
        commands: vec![CommandSchema {
            name: "CertifyDisclosure".into(),
            title: label("Certify disclosure draft"),
            fields: SchemaFields(vec![FieldSchema {
                name: "revision".into(),
                label: label("Draft revision"),
                ty: FieldType::Nat {
                    min: 1,
                    max: u64::MAX,
                },
            }]),
        }],
    }
}

fn sample_action(
    app_id: &str,
    origin: &str,
    receiver: Principal,
    account: AccountId,
    operation_id: Hash,
    at: u64,
) -> dmsg_types::app_action::AppAction {
    use dmsg_types::app_action::*;
    AppAction {
        version: 1,
        environment: Environment::Local,
        app_id: app_id.into(),
        app_config_version: 1,
        origin: origin.into(),
        receiver,
        actor: vec![8; 12].into(),
        signing_account: account,
        operation_id,
        intent_hash: Hash::new([1; 32]),
        schema_hash: dmsg_protocol::app_action::action_schema_hash(&action_schema()),
        issued_at_ms: at,
        expires_at_ms: at + AUTH_TTL_MS,
        command: ActionCommand {
            name: "CertifyDisclosure".into(),
            args: ActionArgs(vec![ActionArg {
                name: "revision".into(),
                value: ActionValue::Nat(1),
            }]),
        },
        files: vec![],
    }
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
            authority_canister: product.beneficiary_authorities[0],
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
    assert_refused(
        &f.ic,
        f.user,
        person(2),
        "verify_application_authorization",
        (id, application.clone()),
    );
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
fn external_app_action_cannot_use_document_attestation_or_consume_a_sequence() {
    let f = Fixture::new();
    let account = f.create(1);
    let state = f.account_id(1, &account);
    let hash = Hash::new([1; 32]);
    let at = time(&f.ic);
    let action = sample_action(
        "sample-actions",
        "https://sample.test",
        f.commerce,
        account,
        hash,
        at,
    );
    let statement = Statement {
        issuer: state.issuer.clone(),
        subject: None,
        issued_at: None,
        content: StatementContent::AppAction(Box::new(action)),
    };
    let mut request = f.attest_request(1, &account, statement);
    request.origin = "https://sample.test".into();
    request.approval.signature = key(1)
        .sign(attest_approval(f.user, &request).as_slice())
        .to_bytes()
        .into();
    let result: Result<SignedArtifact> = update(&f.ic, f.user, person(1), "attest", (request,));
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
    let (mut app, _) = registrations(&f);
    app.app_id = "sample-actions".into();
    app.action_authority = f.sns;
    app.capabilities.push(AppCapability::SignAction);
    app.profiles.push(SigningProfile::AppActionV1);
    app.action_schema = Some(action_schema());
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
    let mut action = sample_action(
        &app.app_id,
        &app.origins[0],
        f.commerce,
        account,
        Hash::new([79; 32]),
        at,
    );
    let request = |body: AppAction| {
        let state = f.account_id(1, &account);
        let device = &state.devices[&hash];
        let mut request = AppActionAttestRequest {
            account_id: account,
            issuer: state.issuer,
            action: body,
            signature: Default::default(),
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
        let statement = app_action_statement(&request).unwrap();
        let prepared =
            prepare_attestation(&statement, &key(1).verifying_key().to_bytes().into()).unwrap();
        request.signature = key(1).sign(&prepared.to_be_signed).to_bytes().into();
        request.approval.signature = key(1)
            .sign(
                approval_message(
                    f.user,
                    &account,
                    ATTEST_APPROVAL_DOMAIN,
                    &attest_approval_command(
                        &statement,
                        &request.action.origin,
                        &request.signature,
                    ),
                    &request.approval,
                )
                .as_slice(),
            )
            .to_bytes()
            .into();
        request
    };
    let req = request(action.clone());
    let before = f.account_id(1, &account).devices[&hash].next_sequence;
    let refused: Result<SignedArtifact> =
        update(&f.ic, f.user, person(1), "attest_app_action", (req.clone(),));
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
            action.clone(),
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
    let before_cycles = f.ic.cycle_balance(f.user);
    let signed: Result<SignedArtifact> =
        update(&f.ic, f.user, person(1), "attest_app_action", (req.clone(),));
    println!(
        "user_cycles method=attest_app_action cycles={}",
        before_cycles - f.ic.cycle_balance(f.user)
    );
    let artifact = signed.unwrap();
    assert!(matches!(
        verify_artifact(&artifact).unwrap().content,
        StatementContent::AppAction(_)
    ));
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
    match_execution_receipt(&artifact, &receipt).unwrap();
    assert_eq!(receipt.origin, "https://sample.test");
    f.ic.upgrade_canister(
        f.user,
        wasm("dmsg_user"),
        candid::encode_args(()).unwrap(),
        None,
    )
    .unwrap();
    let before_cycles = f.ic.cycle_balance(f.user);
    let repeated: Result<SignedArtifact> =
        update(&f.ic, f.user, person(1), "attest_app_action", (req,));
    println!(
        "user_cycles method=attest_app_action_retry cycles={}",
        before_cycles - f.ic.cycle_balance(f.user)
    );
    assert_eq!(repeated, Ok(artifact));
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
        (f.user, account, action.clone(), Some((f.commerce, app))),
    );
    let paused: Result<SignedArtifact> =
        update(&f.ic, f.user, person(1), "attest_app_action", (next,));
    assert_eq!(paused, Err(Error::PolicyStale));
    assert_eq!(f.account_id(1, &account).devices[&hash].next_sequence, seq);
}

#[test]
fn external_quota_rejections_do_not_call_commerce() {
    let f = Fixture::new();
    let id = f.create(1);
    let (app, _) = registrations(&f);
    let mut last = None;
    for n in 1..=60u8 {
        if n == 33 {
            f.ic.stop_canister(f.commerce, None).unwrap();
            let (request, approval) = last.clone().unwrap();
            let replay: Result<AuthenticationResult> = update(
                &f.ic,
                f.user,
                person(1),
                "approve_authentication",
                (id, request, approval),
            );
            replay.unwrap();
            let request = auth(&f, &app, n);
            let approval = approve(
                &f,
                &id,
                "dmsg/authentication/approve/v1",
                &request,
                request.operation_id,
            );
            let denied: Result<AuthenticationResult> = update(
                &f.ic,
                f.user,
                person(1),
                "approve_authentication",
                (id, request, approval),
            );
            assert_eq!(denied, Err(Error::QuotaExceeded));
            f.ic.start_canister(f.commerce, None).unwrap();
            f.ic.advance_time(Duration::from_millis(6 * MINUTE));
        }
        let request = auth(&f, &app, n);
        let approval = approve(
            &f,
            &id,
            "dmsg/authentication/approve/v1",
            &request,
            request.operation_id,
        );
        let result: Result<AuthenticationResult> = update(
            &f.ic,
            f.user,
            person(1),
            "approve_authentication",
            (id, request.clone(), approval.clone()),
        );
        result.unwrap();
        last = Some((request, approval));
    }
    f.ic.stop_canister(f.commerce, None).unwrap();
    let before = f.account_id(1, &id);
    let request = auth(&f, &app, 61);
    let approval = approve(
        &f,
        &id,
        "dmsg/authentication/approve/v1",
        &request,
        request.operation_id,
    );
    let denied: Result<AuthenticationResult> = update(
        &f.ic,
        f.user,
        person(1),
        "approve_authentication",
        (id, request, approval),
    );
    assert_eq!(denied, Err(Error::QuotaExceeded));
    assert_eq!(f.account_id(1, &id), before);
}

#[test]
#[ignore = "mixed account/month/authentication upgrade capacity sample"]
fn user_mixed_upgrade_profile() {
    let f = Fixture::new();
    let (app, _) = registrations(&f);
    let accounts: Vec<_> = (1..=64).map(|n| (n, f.create(n))).collect();
    for month in 0..12 {
        if month > 0 {
            f.ic.advance_time(Duration::from_millis(32 * DAY));
        }
        for (owner, id) in &accounts {
            let usage: Result<dmsg_types::billing::ExecutionUsage> = update(
                &f.ic,
                f.user,
                person(*owner),
                "refresh_execution_entitlement",
                (id,),
            );
            usage.unwrap();
        }
    }
    for count in 1..=32u8 {
        for (owner, id) in &accounts {
            let state = f.account_id(*owner, id);
            let request = auth(&f, &app, count);
            let mut approval = Approval {
                device_id: Hash::new([*owner; 32]),
                security_epoch: state.security_epoch,
                sequence: state.devices[&Hash::new([*owner; 32])].next_sequence,
                request_id: request.operation_id,
                expires_at: request.expires_at_ms,
                signature: Default::default(),
            };
            approval.signature = key(*owner)
                .sign(
                    approval_message(
                        f.user,
                        id,
                        "dmsg/authentication/approve/v1",
                        &request,
                        &approval,
                    )
                    .as_slice(),
                )
                .to_bytes()
                .into();
            let result: Result<AuthenticationResult> = update(
                &f.ic,
                f.user,
                person(*owner),
                "approve_authentication",
                (id, request, approval),
            );
            result.unwrap();
        }
        if [8, 32].contains(&count) {
            println!(
                "user_mixed_upgrade authentication_rows={}",
                accounts.len() * usize::from(count)
            );
            user_tests::measure_user_upgrade(&f, &accounts, 12);
            let (owner, id) = accounts.last().unwrap();
            let batch: Result<CertifiedBatch> = query(
                &f.ic,
                f.user,
                person(*owner),
                "authentication_certificate",
                (id, Hash::new([count; 32])),
            );
            let leaf: AuthenticationResult = decode_canonical(&certified_value(
                &f,
                batch.unwrap(),
                &authentication_key(id, &Hash::new([count; 32])),
            ))
            .unwrap();
            assert_eq!(leaf.account_id, *id);
        }
    }
}
