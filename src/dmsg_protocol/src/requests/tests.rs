use super::*;
use ed25519_dalek::{Signer, SigningKey};

fn device() -> DeviceInput {
    DeviceInput {
        device_id: Hash::new([1; 32]),
        signing_pub: SigningKey::from_bytes(&[7; 32])
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
        signature: [5; 64].into(),
    }
}

fn statement() -> Statement {
    Statement {
        issuer: "urn:test:author".into(),
        subject: None,
        issued_at: None,
        content: StatementContent::Text("hello".into()),
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

fn init() -> CoseInit {
    CoseInit {
        issuer_namespace: "urn:dmsg:".into(),
        environment: Environment::Production,
        executing_canister: Principal::from_slice(&[1]),
        user_homes: vec![Principal::from_slice(&[2])],
        derivation_version: 2,
        master: MasterKey {
            key_name: "key_1".into(),
            expected_fingerprint: Hash::new([1; 32]),
        },
        daily_executions: 1,
        daily_cycles: 1,
        governance: Principal::from_slice(&[9]),
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
            config.master.key_name = name.into();
            let expected = name == "key_1"
                || (environment != Environment::Production
                    && matches!(name, "test_key_1" | "dfx_test_key"));
            assert_eq!(config.validate(config.executing_canister).is_ok(), expected);
        }
        let mut config = init();
        config.environment = environment.clone();
        config.master.expected_fingerprint = Hash::new([0; 32]);
        assert_eq!(
            config.validate(config.executing_canister).is_ok(),
            environment != Environment::Production
        );
    }
}

#[test]
fn initialization_checks_home_version_and_budgets() {
    let config = init();
    assert_eq!(config.validate(config.executing_canister), Ok(()));
    assert!(config.validate(Principal::from_slice(&[3])).is_err());
    let changes: &[fn(&mut CoseInit)] = &[
        |c| c.derivation_version = 1,
        |c| c.user_homes = vec![Principal::anonymous()],
        |c| c.user_homes = vec![Principal::management_canister()],
        |c| c.user_homes.clear(),
        |c| c.user_homes.push(c.user_homes[0]),
        |c| c.governance = Principal::anonymous(),
        |c| c.issuer_namespace = "relative/".into(),
        |c| c.daily_executions = 0,
        |c| c.daily_cycles = 0,
    ];
    for change in changes {
        let mut changed = config.clone();
        change(&mut changed);
        assert!(changed.validate(config.executing_canister).is_err());
    }
}

#[test]
fn attestations_name_the_device_key_and_derive_the_policy_purpose() {
    let signer = SigningKey::from_bytes(&[7; 32]);
    let public: Hash = signer.verifying_key().to_bytes().into();
    for (content, purpose) in [
        (StatementContent::Text("hello".into()), KeyPurpose::Statement),
        (
            StatementContent::FileStatement {
                text: "review the file".into(),
                sha256: sha256(b"file"),
                content_type: None,
                location: None,
            },
            KeyPurpose::Statement,
        ),
        (
            StatementContent::Digest {
                sha256: sha256(b"file"),
                content_type: None,
                location: None,
            },
            KeyPurpose::FileAttestation,
        ),
    ] {
        let mut statement = statement();
        statement.content = content;
        let prepared = prepare_attestation(&statement, &public).unwrap();
        assert_eq!(prepared.purpose, purpose);
        assert_eq!(
            prepared.thumbprint,
            key_thumbprint(&public_cose_key(&[], public.as_slice()).unwrap()).unwrap()
        );
        assert_eq!(key_thumbprint(&prepared.cose_key).unwrap(), prepared.thumbprint);
        let parsed = parse_signing_input(&prepared.to_be_signed).unwrap();
        assert_eq!(parsed.statement(), &statement);
        assert_eq!(parsed.kid(), prepared.thumbprint.as_slice());
        let artifact = parsed
            .into_signature(public.as_slice())
            .unwrap()
            .finish(signer.sign(&prepared.to_be_signed).to_bytes().to_vec())
            .unwrap();
        assert_eq!(artifact.cose_key.as_slice(), prepared.cose_key.as_slice());
        assert_eq!(verify_artifact(&artifact).unwrap(), statement);
        match_signing_result(&artifact, &prepared.to_be_signed, prepared.thumbprint).unwrap();
    }
    let mut bad = statement();
    bad.issuer = "relative".into();
    assert!(prepare_attestation(&bad, &public).is_err());
    let mut bad = statement();
    bad.content = StatementContent::Text(String::new());
    assert!(prepare_attestation(&bad, &public).is_err());
    assert!(prepare_attestation(&statement(), &Hash::new([0; 32])).is_err());
}

#[test]
fn attestation_approvals_bind_statement_origin_and_signature() {
    let home = Principal::from_slice(&[1]);
    let request = AttestRequest {
        account_id: AccountId([1; 12]),
        statement: statement(),
        origin: "https://app.test".into(),
        signature: [6; 64].into(),
        approval: approval(),
    };
    let message = |r: &AttestRequest| {
        approval_message(
            home,
            &r.account_id,
            ATTEST_APPROVAL_DOMAIN,
            &attest_approval_command(&r.statement, &r.origin, &r.signature),
            &r.approval,
        )
    };
    let expected = message(&request);
    let changes: &[fn(&mut AttestRequest)] = &[
        |r| r.account_id = AccountId([2; 12]),
        |r| r.origin = "https://other.test".into(),
        |r| r.signature = [7; 64].into(),
        |r| r.statement.content = StatementContent::Text("bye".into()),
        |r| r.approval.device_id = Hash::new([2; 32]),
        |r| r.approval.security_epoch += 1,
        |r| r.approval.sequence += 1,
        |r| r.approval.request_id = Hash::new([6; 32]),
        |r| r.approval.expires_at += 1,
    ];
    for change in changes {
        let mut changed = request.clone();
        change(&mut changed);
        assert_ne!(message(&changed), expected);
    }
    let mut changed = request;
    changed.approval.signature = Default::default();
    assert_eq!(message(&changed), expected);
}

#[test]
fn derivation_approvals_bind_generation_transport_and_cycles() {
    let home = Principal::from_slice(&[1]);
    let request = DeriveRootRequest {
        account_id: AccountId([1; 12]),
        generation: 3,
        transport_public_key: [8; 48].into(),
        max_cycles: 70_000_000_000,
        approval: approval(),
    };
    let message = |r: &DeriveRootRequest| {
        approval_message(
            home,
            &r.account_id,
            DERIVE_APPROVAL_DOMAIN,
            &derive_approval_command(r),
            &r.approval,
        )
    };
    let expected = message(&request);
    let changes: &[fn(&mut DeriveRootRequest)] = &[
        |r| r.generation += 1,
        |r| r.transport_public_key = [9; 48].into(),
        |r| r.max_cycles += 1,
        |r| r.approval.sequence += 1,
    ];
    for change in changes {
        let mut changed = request.clone();
        change(&mut changed);
        assert_ne!(message(&changed), expected);
    }
}

#[test]
fn controller_and_recovery_proofs_bind_every_component() {
    use dmsg_types::agent::DelegationAuthority;
    let home = Principal::from_slice(&[1]);
    let account = AccountId([1; 12]);
    let delegation = DelegationAuthority::Restricted {
        scopes: vec!["message.draft".into()],
        audiences: vec!["https://dmsg.net".into()],
    };
    let expected = controller_pop_message(home, &account, 2, &delegation, &[1], Hash::new([5; 32]));
    assert_ne!(
        controller_pop_message(
            Principal::from_slice(&[2]),
            &account,
            2,
            &delegation,
            &[1],
            Hash::new([5; 32])
        ),
        expected
    );
    assert_ne!(
        controller_pop_message(home, &account, 3, &delegation, &[1], Hash::new([5; 32])),
        expected
    );
    assert_ne!(
        controller_pop_message(
            home,
            &account,
            2,
            &DelegationAuthority::Unrestricted,
            &[1],
            Hash::new([5; 32])
        ),
        expected
    );
    assert_ne!(
        controller_pop_message(home, &account, 2, &delegation, &[], Hash::new([5; 32])),
        expected
    );
    assert_ne!(
        controller_pop_message(home, &account, 2, &delegation, &[1], Hash::new([6; 32])),
        expected
    );
    let request = RecoveryRequest {
        op_id: Hash::new([1; 32]),
        new_auth: home,
        device: device(),
        expires_at: 100,
    };
    let expected = recovery_device_message(home, &account, &request);
    let mut changed = request.clone();
    changed.new_auth = Principal::from_slice(&[2]);
    assert_ne!(recovery_device_message(home, &account, &changed), expected);
    assert_ne!(
        recovery_device_message(home, &AccountId([2; 12]), &request),
        expected
    );
}

#[test]
fn root_digests_sort_recipients_and_bind_generation_and_body() {
    let a = Hash::new([1; 32]);
    let b = Hash::new([2; 32]);
    assert_eq!(
        root_recipients_digest(&[b, a], 3),
        root_recipients_digest(&[a, b], 3)
    );
    assert_eq!(
        root_recipients_digest(&[a, b], 3),
        digest("dmsg/root-recipients/1", &(vec![a, b], 3u64))
    );
    assert_ne!(
        root_recipients_digest(&[a, b], 3),
        root_recipients_digest(&[a, b], 4)
    );
    assert_ne!(
        root_recipients_digest(&[a], 3),
        root_recipients_digest(&[a, b], 3)
    );
    let recipients = root_recipients_digest(&[a, b], 3);
    let body = sha256(b"body");
    assert_eq!(
        root_bundle_digest(recipients, body),
        digest("dmsg/root-bundle-digest/2", &(recipients, body))
    );
    assert_ne!(
        root_bundle_digest(recipients, body),
        root_bundle_digest(recipients, sha256(b"other"))
    );
    assert_ne!(
        root_bundle_digest(recipients, body),
        root_bundle_digest(root_recipients_digest(&[a], 3), body)
    );
}
