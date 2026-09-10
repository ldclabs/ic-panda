use super::*;

fn device() -> DeviceInput {
    DeviceInput {
        device_id: Hash::new([1; 32]),
        signing_pub: ed25519_dalek::SigningKey::from_bytes(&[7; 32])
            .verifying_key()
            .to_bytes()
            .into(),
        hpke_pub: Hash::new([2; 32]),
        role: ControllerRole::Administrator,
        capabilities: vec![Capability::ContentSign, Capability::FormalApprove],
    }
}

fn approval() -> Approval {
    Approval {
        device_id: Hash::new([1; 32]),
        security_epoch: 2,
        sequence: 3,
        request_id: Hash::new([4; 32]),
        expires_at: 100,
        signature: vec![5; 64].into(),
    }
}

fn sign_request() -> SignRequest {
    SignRequest {
        account_id: AccountId([1; 12]),
        key: SigningKeyRef {
            algorithm: SigningAlgorithm::Ed25519,
            kid: b"kid".to_vec().into(),
            public_key_fingerprint: Hash::new([8; 32]),
        },
        statement: Statement {
            issuer: "urn:test:author".into(),
            subject: None,
            issued_at: None,
            content: StatementContent::Text("hello".into()),
        },
        origin: "https://app.test".into(),
        max_cycles: 1_000,
        approval: approval(),
    }
}

#[test]
fn device_capabilities_are_nonempty_bounded_and_unique() {
    let mut input = device();
    input.capabilities = vec![
        Capability::ContentSign,
        Capability::VaultUnlock,
        Capability::RootManage,
        Capability::FormalApprove,
        Capability::PaymentOffer,
    ];
    assert_eq!(input.validate(), Ok(()));
    for i in 0..5 {
        let mut changed = input.clone();
        changed.capabilities[(i + 1) % 5] = changed.capabilities[i].clone();
        assert!(changed.validate().is_err());
    }
    input.capabilities.push(Capability::ContentSign);
    assert!(input.validate().is_err());
    input.capabilities.clear();
    assert!(input.validate().is_err());
    input.capabilities.push(Capability::VaultUnlock);
    input.role = ControllerRole::Member;
    assert_eq!(input.validate(), Ok(()));
}

#[test]
fn devices_reject_zero_identifiers_and_weak_signing_keys() {
    let mut input = device();
    input.device_id = Hash::new([0; 32]);
    assert!(input.validate().is_err());
    input = device();
    input.hpke_pub = Hash::new([0; 32]);
    assert!(input.validate().is_err());
    for mut bytes in [[0; 32], [1; 32]] {
        bytes[1..].fill(0);
        input = device();
        input.signing_pub = bytes.into();
        assert_eq!(input.validate(), Err(Error::IntegrityFailed));
    }
    // Exercise a non-decompressible point independently of small-order points.
    let bytes = (0..=255)
        .map(|b| [b; 32])
        .find(|b| ed25519_dalek::VerifyingKey::from_bytes(b).is_err())
        .unwrap();
    input.signing_pub = bytes.into();
    assert_eq!(input.validate(), Err(Error::IntegrityFailed));
}

#[test]
fn key_requests_enforce_purpose_algorithm_and_generation() {
    for purpose in [
        KeyPurpose::Statement,
        KeyPurpose::FileAttestation,
        KeyPurpose::ContentRoot,
    ] {
        for algorithm in [
            Algorithm::Ed25519,
            Algorithm::EcdsaSecp256k1,
            Algorithm::VetKdBls12381,
        ] {
            for generation in [0, 1, 2, u64::MAX] {
                let expected = generation > 0
                    && match purpose {
                        KeyPurpose::ContentRoot => algorithm == Algorithm::VetKdBls12381,
                        _ => generation == 1 && algorithm != Algorithm::VetKdBls12381,
                    };
                let key = KeyRequest {
                    purpose: purpose.clone(),
                    algorithm: algorithm.clone(),
                    generation,
                };
                assert_eq!(key.validate().is_ok(), expected, "{key:?}");
            }
        }
    }
}

fn init() -> CoseInit {
    CoseInit {
        issuer_namespace: "urn:dmsg:".into(),
        environment: Environment::Production,
        executing_canister: Principal::from_slice(&[1]),
        initial_home_user: Principal::from_slice(&[2]),
        derivation_version: 2,
        masters: vec![MasterKey {
            algorithm: Algorithm::Ed25519,
            key_name: "key_1".into(),
            expected_fingerprint: Hash::new([1; 32]),
        }],
        daily_executions: 1,
        daily_cycles: 1,
    }
}

#[test]
fn initialization_enforces_key_names_for_each_environment() {
    for environment in [
        Environment::Local,
        Environment::Staging,
        Environment::Production,
    ] {
        for name in ["key_1", "test_key_1", "dfx_test_key", "", "other"] {
            let mut config = init();
            config.environment = environment.clone();
            config.masters[0].key_name = name.into();
            let expected = name == "key_1"
                || (environment != Environment::Production
                    && matches!(name, "test_key_1" | "dfx_test_key"));
            assert_eq!(config.validate(config.executing_canister).is_ok(), expected);
        }
        let mut config = init();
        config.environment = environment.clone();
        config.masters[0].expected_fingerprint = Hash::new([0; 32]);
        assert_eq!(
            config.validate(config.executing_canister).is_ok(),
            environment != Environment::Production
        );
    }
}

#[test]
fn initialization_checks_home_version_masters_and_budgets() {
    let config = init();
    assert_eq!(config.validate(config.executing_canister), Ok(()));
    assert!(config.validate(Principal::from_slice(&[3])).is_err());
    let changes: &[fn(&mut CoseInit)] = &[
        |c| c.derivation_version = 1,
        |c| c.initial_home_user = Principal::anonymous(),
        |c| c.initial_home_user = Principal::management_canister(),
        |c| c.issuer_namespace = "relative/".into(),
        |c| c.masters.clear(),
        |c| c.masters.push(c.masters[0].clone()),
        |c| c.masters = vec![c.masters[0].clone(); 4],
        |c| c.daily_executions = 0,
        |c| c.daily_cycles = 0,
    ];
    for change in changes {
        let mut changed = config.clone();
        change(&mut changed);
        assert!(changed.validate(config.executing_canister).is_err());
    }
    let mut config = config;
    for algorithm in [Algorithm::EcdsaSecp256k1, Algorithm::VetKdBls12381] {
        config.masters.push(MasterKey {
            algorithm,
            ..config.masters[0].clone()
        });
    }
    assert_eq!(config.validate(config.executing_canister), Ok(()));
}

#[test]
fn sign_conversion_preserves_context_and_derives_the_correct_key_purpose() {
    for algorithm in [SigningAlgorithm::Ed25519, SigningAlgorithm::EcdsaSecp256k1] {
        for content in [
            StatementContent::Text("hello".into()),
            StatementContent::Digest {
                sha256: sha256(b"file"),
                content_type: None,
                location: None,
            },
        ] {
            let mut request = sign_request();
            request.key.algorithm = algorithm.clone();
            request.statement.content = content;
            let execution = request.clone().into_execution().unwrap();
            assert_eq!(execution.account_id, request.account_id);
            assert_eq!(execution.approval, request.approval);
            assert_eq!(execution.max_cycles, request.max_cycles);
            let ExecutionKind::Sign {
                key,
                to_be_signed,
                public_key_fingerprint,
                origin,
            } = execution.kind
            else {
                panic!()
            };
            assert_eq!(key.algorithm, algorithm.clone().into());
            assert_eq!(key.purpose, statement_purpose(&request.statement));
            assert_eq!(key.generation, 1);
            assert_eq!(public_key_fingerprint, request.key.public_key_fingerprint);
            assert_eq!(origin, request.origin);
            let parsed = parse_signing_input(&to_be_signed).unwrap();
            assert_eq!(parsed.statement, request.statement);
            assert_eq!(parsed.kid, request.key.kid.into_vec());
        }
    }
    let changes: &[fn(&mut SignRequest)] = &[
        |r| r.origin = "http://app.test".into(),
        |r| r.statement.issuer = "relative".into(),
        |r| r.statement.content = StatementContent::Text(String::new()),
        |r| r.key.kid = Vec::new().into(),
        |r| r.key.kid = vec![1; MAX_KID_BYTES + 1].into(),
    ];
    for change in changes {
        let mut request = sign_request();
        change(&mut request);
        assert!(request.into_execution().is_err());
    }
}

#[test]
fn execution_approvals_bind_all_context_but_not_the_signature_itself() {
    let request = sign_request().into_execution().unwrap();
    let home = Principal::from_slice(&[1]);
    let expected = request.approval_message(home);
    assert_ne!(
        request.approval_message(Principal::from_slice(&[2])),
        expected
    );
    let changes: &[fn(&mut ExecuteRequest)] = &[
        |r| r.account_id = AccountId([2; 12]),
        |r| r.max_cycles += 1,
        |r| r.approval.device_id = Hash::new([2; 32]),
        |r| r.approval.security_epoch += 1,
        |r| r.approval.sequence += 1,
        |r| r.approval.request_id = Hash::new([6; 32]),
        |r| r.approval.expires_at += 1,
        |r| {
            if let ExecutionKind::Sign { origin, .. } = &mut r.kind {
                *origin = "https://other.test".into()
            }
        },
        |r| {
            if let ExecutionKind::Sign { to_be_signed, .. } = &mut r.kind {
                to_be_signed[0] ^= 1
            }
        },
        |r| {
            if let ExecutionKind::Sign {
                public_key_fingerprint,
                ..
            } = &mut r.kind
            {
                *public_key_fingerprint = Hash::new([9; 32])
            }
        },
        |r| {
            if let ExecutionKind::Sign { key, .. } = &mut r.kind {
                key.generation += 1
            }
        },
    ];
    for change in changes {
        let mut changed = request.clone();
        change(&mut changed);
        assert_ne!(changed.approval_message(home), expected);
    }
    let mut changed = request;
    changed.approval.signature = Vec::new().into();
    assert_eq!(changed.approval_message(home), expected);
}

#[test]
fn recovery_confirmations_bind_home_account_nonce_request_and_confirmation() {
    let home = Principal::from_slice(&[1]);
    let account = AccountId([1; 12]);
    let request = RecoveryRequest {
        op_id: Hash::new([1; 32]),
        new_auth: home,
        device: device(),
        generation: 1,
        expires_at: 100,
    };
    let confirmation = RecoveryConfirmation {
        request_id: Hash::new([2; 32]),
        dispute: Hash::new([3; 32]),
        expires_at: 200,
    };
    let expected = recovery_confirmation_message(home, &account, 1, &request, &confirmation);
    assert_ne!(
        recovery_confirmation_message(
            Principal::from_slice(&[2]),
            &account,
            1,
            &request,
            &confirmation
        ),
        expected
    );
    assert_ne!(
        recovery_confirmation_message(home, &AccountId([2; 12]), 1, &request, &confirmation),
        expected
    );
    assert_ne!(
        recovery_confirmation_message(home, &account, 2, &request, &confirmation),
        expected
    );
    let mut changed = request.clone();
    changed.new_auth = Principal::from_slice(&[2]);
    assert_ne!(
        recovery_confirmation_message(home, &account, 1, &changed, &confirmation),
        expected
    );
    let mut changed = confirmation.clone();
    changed.dispute = Hash::new([4; 32]);
    assert_ne!(
        recovery_confirmation_message(home, &account, 1, &request, &changed),
        expected
    );
}
