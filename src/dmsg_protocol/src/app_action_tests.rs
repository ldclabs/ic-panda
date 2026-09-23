use super::*;
use crate::*;
use dmsg_types::cose::*;
use ed25519_dalek::{Signer, SigningKey};
#[path = "../../dmsg_types/tests/support/app_action.rs"]
mod fixture;

fn statement(action: AppAction) -> Statement {
    Statement {
        issuer: "https://example.test/u/00000000000000000000".into(),
        subject: None,
        issued_at: None,
        content: StatementContent::AppAction(Box::new(action)),
    }
}

#[test]
fn closed_actions_roundtrip_and_every_signed_field_is_bound() {
    let signer = SigningKey::from_bytes(&[71; 32]);
    let public = signer.verifying_key().to_bytes();
    let kid = key_thumbprint(&public_cose_key(&Algorithm::Ed25519, &[], &public).unwrap()).unwrap();
    for command in fixture::commands() {
        let mut action = fixture::action();
        action.command = command;
        action.input_hash = dmsg_protocol::app_action::action_input_hash(&action.command);
        validate_app_action(&action).unwrap();
        let original = statement(action.clone());
        let (_, tbs) = prepare_cose(&original, &Algorithm::Ed25519, &kid[..]).unwrap();
        let sig = signer.sign(&tbs).to_bytes().to_vec();
        let artifact = finish_cose(&tbs, &public, sig.clone()).unwrap();
        assert_eq!(verify_artifact(&artifact).unwrap(), original);
        assert_eq!(statement_purpose(&original), KeyPurpose::AppAction);
        assert_eq!(
            verification_report(&artifact, None).unwrap().content,
            VerificationStatus::NotChecked
        );
        for mutate in [
            |a: &mut AppAction| a.origin = "https://other.test".into(),
            |a: &mut AppAction| a.receiver = candid::Principal::from_slice(&[99, 1]),
            |a: &mut AppAction| a.files[0].display_name = Some("changed.cbor".into()),
            |a: &mut AppAction| a.files[0].sha256 = Hash::new([42; 32]),
            |a: &mut AppAction| a.files[0].revision += 1,
            |a: &mut AppAction| a.files[0].representation = ActionFileRepresentation::Encrypted,
            |a: &mut AppAction| a.expires_at_ms -= 1,
            |a: &mut AppAction| a.actor_id = AccountId([42; 12]),
            |a: &mut AppAction| {
                a.command = AppActionCommand::TokenListApproveTransition {
                    project_id: 42,
                    transition_id: 1,
                    approve: true,
                    statement_hash: Hash::new([1; 32]),
                    rationale: "A different complete decision".into(),
                }
            },
        ] {
            let mut changed = action.clone();
            mutate(&mut changed);
            changed.input_hash = action_input_hash(&changed.command);
            assert_ne!(app_action_digest(&changed), app_action_digest(&action));
            let (_, changed_tbs) =
                prepare_cose(&statement(changed), &Algorithm::Ed25519, &kid[..]).unwrap();
            let changed_artifact = finish_cose(&changed_tbs, &public, sig.clone()).unwrap();
            assert!(verify_artifact(&changed_artifact).is_err());
        }
    }
}

#[test]
fn unknown_fields_profiles_and_ambiguous_claims_are_rejected() {
    let a = fixture::action();
    let mut value: cbor2::Value = cbor2::from_slice(&canonical(&a)).unwrap();
    if let cbor2::Value::Map(ref mut entries) = value {
        entries.push(("display_summary".into(), "approve everything".into()));
    }
    assert!(decode_canonical::<AppAction>(&canonical(&value)).is_err());
    let mut s = statement(a.clone());
    s.subject = Some("a contradictory subject".into());
    assert!(prepare_cose(&s, &Algorithm::Ed25519, &[1]).is_err());
    s.subject = None;
    s.issued_at = Some(1);
    assert!(prepare_cose(&s, &Algorithm::Ed25519, &[1]).is_err());
    let mut wrong = a.clone();
    wrong.version = 2;
    assert!(validate_app_action(&wrong).is_err());
    wrong = a.clone();
    wrong.files.push(wrong.files[0].clone());
    assert!(validate_app_action(&wrong).is_err());
    wrong = a;
    wrong.expires_at_ms += 1;
    assert!(validate_app_action(&wrong).is_err());
}

#[test]
fn admission_and_document_endpoint_fail_closed() {
    let action = fixture::action();
    let mut app = AppRegistration {
        version: 1,
        environment: action.environment.clone(),
        app_id: action.app_id.clone(),
        config_version: 1,
        origins: vec![action.origin.clone()],
        user_homes: vec![candid::Principal::from_slice(&[1, 1])],
        cose_homes: vec![candid::Principal::from_slice(&[2, 1])],
        product_ids: vec![],
        capabilities: vec![AppCapability::Authenticate],
        profiles: vec![],
        authentication_receiver: candid::Principal::from_slice(&[3, 1]),
        action_authority: candid::Principal::from_slice(&[3, 1]),
        paused: false,
    };
    app.capabilities.push(AppCapability::SignAction);
    app.profiles.push(SigningProfile::AppActionV1);
    validate_action_admission(&action, &app, action.issued_at_ms).unwrap();
    assert_eq!(
        validate_action_admission(&action, &app, action.expires_at_ms),
        Err(Error::Expired)
    );
    app.paused = true;
    assert_eq!(
        validate_action_admission(&action, &app, action.issued_at_ms),
        Err(Error::Locked)
    );
    let request = SignRequest {
        account_id: AccountId([3; 12]),
        statement: statement(action),
        origin: "https://example.test".into(),
        key: SigningKeyRef {
            algorithm: SigningAlgorithm::Ed25519,
            kid: vec![1].into(),
            public_key_fingerprint: Hash::new([1; 32]),
        },
        max_cycles: 1,
        approval: Approval {
            device_id: Hash::new([1; 32]),
            security_epoch: 1,
            sequence: 1,
            request_id: Hash::new([2; 32]),
            expires_at: 1,
            signature: vec![0; 64].into(),
        },
    };
    assert_eq!(request.into_execution(), Err(Error::UnsupportedProtocol));
}
