use super::*;
use crate::*;
use ed25519_dalek::{Signer, SigningKey};
#[path = "../../dmsg_types/tests/support/app_action.rs"]
mod fixture;

fn finish(tbs: &[u8], public: &[u8], signature: Vec<u8>) -> Result<SignedArtifact> {
    parse_signing_input(tbs)?
        .into_signature(public)?
        .finish(signature)
}

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
    let kid = key_thumbprint(&public_cose_key(&[], &public).unwrap()).unwrap();
    for command in fixture::commands() {
        let mut action = fixture::action();
        action.command = command;
        action.input_hash = dmsg_protocol::app_action::action_input_hash(&action.command);
        validate_app_action(&action).unwrap();
        let original = statement(action.clone());
        let (_, tbs) = prepare_cose(&original, &kid[..]).unwrap();
        let sig = signer.sign(&tbs).to_bytes().to_vec();
        let artifact = finish(&tbs, &public, sig.clone()).unwrap();
        assert_eq!(verify_artifact(&artifact).unwrap(), original);
        assert_eq!(statement_purpose(&original), KeyPurpose::AppAction);
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
            let (_, changed_tbs) =
                prepare_cose(&statement(changed), &kid[..]).unwrap();
            let changed_artifact = finish(&changed_tbs, &public, sig.clone()).unwrap();
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
    assert!(prepare_cose(&s, &[1]).is_err());
    s.subject = None;
    s.issued_at = Some(1);
    assert!(prepare_cose(&s, &[1]).is_err());
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
}

#[test]
fn app_action_attestations_require_the_signing_account_and_a_bounded_approval() {
    let action = fixture::action();
    let request = AppActionAttestRequest {
        account_id: action.signing_account,
        issuer: "https://example.test/u/00000000000000000000".into(),
        action: action.clone(),
        signature: [6; 64].into(),
        approval: Approval {
            device_id: Hash::new([1; 32]),
            security_epoch: 1,
            sequence: 1,
            request_id: Hash::new([2; 32]),
            expires_at: action.expires_at_ms,
            signature: Default::default(),
        },
    };
    let statement = app_action_statement(&request).unwrap();
    assert_eq!(statement.issuer, request.issuer);
    assert_eq!(statement_purpose(&statement), KeyPurpose::AppAction);
    let mut foreign = request.clone();
    foreign.account_id = AccountId([4; 12]);
    assert_eq!(app_action_statement(&foreign), Err(Error::Forbidden));
    let mut late = request;
    late.approval.expires_at = action.expires_at_ms + 1;
    assert_eq!(app_action_statement(&late), Err(Error::Expired));
}
