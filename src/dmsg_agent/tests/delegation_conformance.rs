use agent_protocols::delegation::*;
use agent_protocols::identity::{
    is_controller_challenge, verify_controller_challenge, AgentId, AgentSigner, Envelope,
};
use serde_json::{json, Value};

const ID: &str = "https://example.com/p";
const ORIGIN: &str = "https://dmsg.net";

fn vectors() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/vectors/agent-protocols/agent-delegation-1.0.vectors.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn registration_signer(registration: &Value) -> AgentSigner {
    let hex = registration["seed"].as_str().unwrap();
    let seed: [u8; 32] = (0..32)
        .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    AgentSigner::from_seed(seed)
}

#[test]
fn controller_registration_challenges_have_the_fixed_shape() {
    let registration = &vectors()["controller_registration"];
    let signer = registration_signer(registration);
    for fixture in registration["challenges"].as_array().unwrap() {
        let challenge = fixture["challenge"].as_str().unwrap();
        let valid = fixture["valid"].as_bool().unwrap();
        assert_eq!(
            is_controller_challenge(challenge),
            valid,
            "{}",
            fixture["name"]
        );
        assert_eq!(
            signer.sign_controller_challenge(challenge).is_ok(),
            valid,
            "{}",
            fixture["name"]
        );
    }
}

#[test]
fn controller_registration_proofs_sign_the_complete_challenge_string() {
    let registration = &vectors()["controller_registration"];
    let signer = registration_signer(registration);
    for fixture in registration["proofs"].as_array().unwrap() {
        let id: AgentId = fixture["id"].as_str().unwrap().parse().unwrap();
        let challenge = fixture["challenge"].as_str().unwrap();
        let signature = fixture["signature"].as_str().unwrap();
        let valid = fixture["valid"].as_bool().unwrap();
        assert_eq!(
            verify_controller_challenge(&id, challenge, signature).is_ok(),
            valid,
            "{}",
            fixture["name"]
        );
        if valid {
            assert_eq!(
                signer.sign_controller_challenge(challenge).unwrap(),
                signature
            );
        }
    }
}

fn key(byte: u8) -> AgentSigner {
    AgentSigner::from_seed([byte; 32])
}
fn signer() -> AgentSigner {
    key(61)
}
fn root() -> AgentSigner {
    key(62)
}
fn successor() -> AgentSigner {
    key(63)
}
fn other() -> AgentSigner {
    key(64)
}
fn document() -> PrincipalDocument {
    serde_json::from_value(json!({"protocol":PROTOCOL,"id":ID,"updated_at":2000,"delegation_service":"https://example.com","controllers":[
        {"id":signer().agent_id(),"source":ORIGIN,"valid_from":100,"delegation":{"scopes":["draft"],"audiences":[ORIGIN]}},
        {"id":root().agent_id(),"source":"local","valid_from":100,"delegation":"*"}
    ]})).unwrap()
}
fn grant_with(
    actor: &AgentSigner,
    nonce: u64,
    edit: impl FnOnce(&mut DelegationGrantPayload),
) -> Envelope<DelegationPayload> {
    let mut payload = DelegationGrantPayload::new(
        "del",
        ID,
        root().agent_id(),
        vec!["draft".into()],
        vec![ORIGIN.into()],
    );
    payload.expires_at = Some(1000);
    edit(&mut payload);
    let event = delegation_grant_event(actor.agent_id(), 200, nonce, payload);
    let envelope = actor.sign_event(event).unwrap();
    serde_json::from_value(serde_json::to_value(envelope).unwrap()).unwrap()
}
fn grant(actor: &AgentSigner, nonce: u64) -> Envelope<DelegationPayload> {
    grant_with(actor, nonce, |_| {})
}
fn revoke(actor: &AgentSigner, created_at: i64, nonce: u64) -> Envelope<DelegationPayload> {
    let event = delegation_revoke_event(
        actor.agent_id(),
        created_at,
        nonce,
        DelegationRevokePayload {
            id: "del".into(),
            principal_id: ID.into(),
            reason: None,
        },
    );
    let envelope = actor.sign_event(event).unwrap();
    serde_json::from_value(serde_json::to_value(envelope).unwrap()).unwrap()
}
fn materialize(
    envelope: &Envelope<DelegationPayload>,
    accepted_at: i64,
    previous: Option<&DelegationCredential>,
) -> DelegationCredential {
    materialize_delegation_credential(envelope, DelegationStatus::Active, accepted_at, previous)
        .unwrap()
}
fn record(envelope: &Envelope<DelegationPayload>, accepted_at: i64) -> DelegationRecord {
    DelegationRecord {
        envelope: envelope.clone(),
        accepted_at,
    }
}

#[test]
fn controller_conformance_vectors() {
    let vectors = vectors();
    for case in vectors["principal_documents"].as_array().unwrap() {
        let result = serde_json::from_value::<PrincipalDocument>(case["document"].clone())
            .map_err(|e| e.to_string())
            .and_then(|d| validate_principal_document(&d).map_err(|e| e.to_string()));
        assert_eq!(
            result.is_ok(),
            case["valid"].as_bool().unwrap(),
            "{}: {:?}",
            case["name"],
            result
        );
    }
}

#[test]
fn delegation_id_grammar_vectors() {
    let vectors = vectors();
    for id in vectors["delegation_ids"]["valid"].as_array().unwrap() {
        validate_delegation_id(id.as_str().unwrap()).unwrap();
    }
    for id in vectors["delegation_ids"]["invalid"].as_array().unwrap() {
        assert!(
            validate_delegation_id(id.as_str().unwrap()).is_err(),
            "{id}"
        );
    }
}

#[test]
fn audience_vectors() {
    let vectors = vectors();
    for audience in vectors["audiences"]["valid"].as_array().unwrap() {
        validate_audience(audience.as_str().unwrap()).unwrap();
    }
    for audience in vectors["audiences"]["invalid"].as_array().unwrap() {
        assert!(
            validate_audience(audience.as_str().unwrap()).is_err(),
            "{audience}"
        );
    }
}

#[test]
fn acceptance_vectors() {
    let vectors = vectors();
    let acceptance = &vectors["acceptance"];
    let document: PrincipalDocument =
        serde_json::from_value(acceptance["document"].clone()).unwrap();
    validate_principal_document(&document).unwrap();
    for case in acceptance["cases"].as_array().unwrap() {
        let envelope: Envelope<DelegationPayload> =
            serde_json::from_value(case["envelope"].clone()).unwrap();
        let accepted_at = case["accepted_at"].as_i64().unwrap();
        let previous: Option<DelegationCredential> =
            serde_json::from_value(case["previous"].clone()).unwrap();
        let result = match case["mode"].as_str().unwrap() {
            "live" => validate_delegation_acceptance(
                &envelope,
                &document,
                &document.id,
                accepted_at,
                previous.as_ref(),
            ),
            _ => validate_historical_delegation(
                &DelegationRecord {
                    envelope,
                    accepted_at,
                },
                &document,
                &document.id,
                previous.as_ref(),
            ),
        };
        let outcome = match &result {
            Ok(()) => "ok",
            Err(error) => error.code().unwrap_or("other"),
        };
        assert_eq!(
            outcome,
            case["expected"].as_str().unwrap(),
            "{}: {result:?}",
            case["name"]
        );
    }
}

#[test]
fn authority_ownership_and_materialization() {
    let doc = document();
    let envelope = grant(&signer(), 1);
    validate_delegation_acceptance(&envelope, &doc, ID, 250, None).unwrap();
    let credential = materialize(&envelope, 250, None);
    assert_eq!(credential.accepted_at, 250);
    assert_eq!(credential.checked_at, 250);
    assert_eq!(credential.principal_id, ID);
    assert_eq!(credential.grant_event_id, envelope.hash);
    validate_delegation_use(&credential, ORIGIN, 300).unwrap();
    assert!(validate_delegation_use(&credential, "https://tokenlist.ing", 300).is_err());
    assert!(validate_delegation_use(&credential, ORIGIN, 1000).is_err());
    for bad in [
        grant_with(&signer(), 2, |p| p.scopes = vec!["admin".into()]),
        grant_with(&signer(), 2, |p| {
            p.audiences = vec!["https://tokenlist.ing".into()]
        }),
    ] {
        assert!(validate_delegation_acceptance(&bad, &doc, ID, 250, None).is_err());
    }
    assert!(
        validate_delegation_acceptance(&envelope, &doc, "https://impostor.example", 250, None)
            .is_err()
    );
    assert!(validate_delegation_acceptance(&envelope, &doc, ID, 1000, None).is_err());
    let mut only = doc.clone();
    only.controllers[0].delegation = None;
    // Mathematically valid, but not authorized.
    validate_delegation_envelope(&envelope).unwrap();
    assert!(validate_delegation_acceptance(&envelope, &only, ID, 250, None).is_err());
    // A restricted key manages another key's credential only within its ceiling.
    let root_credential = materialize(&grant(&root(), 1), 250, None);
    validate_delegation_acceptance(&envelope, &doc, ID, 300, Some(&root_credential)).unwrap();
    let wide = materialize(
        &grant_with(&root(), 2, |p| {
            p.scopes = vec!["draft".into(), "admin".into()]
        }),
        250,
        None,
    );
    assert_eq!(
        validate_delegation_acceptance(&envelope, &doc, ID, 300, Some(&wide))
            .unwrap_err()
            .code(),
        Some("delegation_ceiling_exceeded")
    );
    assert!(validate_controller_enumeration(&doc, &signer().agent_id(), 300, &wide).is_err());
    validate_controller_enumeration(&doc, &signer().agent_id(), 300, &credential).unwrap();
    validate_controller_enumeration(&doc, &root().agent_id(), 300, &wide).unwrap();
    let revocation = revoke(&root(), 400, 2);
    validate_delegation_acceptance(&revocation, &doc, ID, 450, Some(&credential)).unwrap();
    let revoked = materialize(&revocation, 450, Some(&credential));
    assert_eq!(revoked.status, DelegationStatus::Revoked);
    assert_eq!(revoked.controller, root().agent_id());
    assert_eq!(revoked.grant_event_id, envelope.hash);
    assert_eq!(revoked.audiences, credential.audiences);
    assert!(validate_delegation_acceptance(&revocation, &doc, ID, 450, None).is_err());
    let replacement = grant(&root(), 3);
    validate_delegation_acceptance(&replacement, &doc, ID, 500, Some(&revoked)).unwrap();
    assert_eq!(
        materialize(&replacement, 500, Some(&revoked)).controller,
        root().agent_id()
    );
}

#[test]
fn a_replacement_cannot_move_a_credential_to_another_subject() {
    let doc = document();
    let credential = materialize(&grant(&signer(), 1), 250, None);
    let moved = grant_with(&root(), 2, |p| p.subject = other().agent_id());
    let error =
        validate_delegation_acceptance(&moved, &doc, ID, 300, Some(&credential)).unwrap_err();
    assert!(error.to_string().contains("immutable"), "{error}");
}

#[test]
fn retired_history_needs_accepted_records_and_the_original_ceiling() {
    let mut doc = document();
    let envelope = grant(&signer(), 1);
    let mut retired = doc.controllers.remove(0);
    retired.retired_at = Some(500);
    doc.retired_controllers.push(retired);
    assert!(validate_delegation_acceptance(&envelope, &doc, ID, 600, None).is_err());
    validate_historical_delegation(&record(&envelope, 250), &doc, ID, None).unwrap();
    assert!(validate_historical_delegation(&record(&envelope, 500), &doc, ID, None).is_err());
    doc.retired_controllers[0].invalid_from = Some(300);
    validate_historical_delegation(&record(&envelope, 250), &doc, ID, None).unwrap();
    doc.retired_controllers[0].invalid_from = Some(250);
    assert!(validate_historical_delegation(&record(&envelope, 250), &doc, ID, None).is_err());
    doc.retired_controllers[0].invalid_from = None;
    let admin = grant_with(&signer(), 2, |p| p.scopes = vec!["admin".into()]);
    assert!(validate_historical_delegation(&record(&admin, 250), &doc, ID, None).is_err());
}

#[test]
fn a_restricted_controller_manages_credentials_within_its_ceiling() {
    let mut doc = document();
    let mut retired = doc.controllers.remove(0);
    retired.retired_at = Some(600);
    doc.retired_controllers.push(retired);
    doc.controllers.push(
        serde_json::from_value(json!({
            "id": successor().agent_id(), "source": ORIGIN, "valid_from": 600,
            "delegation": {"scopes": ["draft", "inbox"], "audiences": [ORIGIN]}
        }))
        .unwrap(),
    );
    validate_principal_document(&doc).unwrap();
    let credential = materialize(&grant(&signer(), 1), 250, None);
    assert!(controller_covers_credential(
        &doc.controllers[1],
        &credential
    ));
    let revocation = revoke(&successor(), 700, 1);
    validate_delegation_acceptance(&revocation, &doc, ID, 700, Some(&credential)).unwrap();
    validate_controller_enumeration(&doc, &successor().agent_id(), 700, &credential).unwrap();
    let widened = {
        let mut payload = DelegationGrantPayload::new(
            "del",
            ID,
            root().agent_id(),
            vec!["draft".into(), "inbox".into()],
            vec![ORIGIN.into()],
        );
        payload.expires_at = Some(1900);
        let event = delegation_grant_event(successor().agent_id(), 700, 2, payload);
        let envelope = successor().sign_event(event).unwrap();
        serde_json::from_value::<Envelope<DelegationPayload>>(
            serde_json::to_value(envelope).unwrap(),
        )
        .unwrap()
    };
    validate_delegation_acceptance(&widened, &doc, ID, 700, Some(&credential)).unwrap();
    assert_eq!(
        materialize(&widened, 700, Some(&credential)).controller,
        successor().agent_id()
    );
    // A credential outside the ceiling cannot be revoked, replaced, or
    // enumerated by the restricted key.
    let wide = materialize(
        &grant_with(&root(), 3, |p| {
            p.scopes = vec!["draft".into(), "admin".into()]
        }),
        250,
        None,
    );
    assert!(!controller_covers_credential(&doc.controllers[1], &wide));
    assert_eq!(
        validate_delegation_acceptance(&revocation, &doc, ID, 700, Some(&wide))
            .unwrap_err()
            .code(),
        Some("delegation_ceiling_exceeded")
    );
    assert!(validate_controller_enumeration(&doc, &successor().agent_id(), 700, &wide).is_err());
}

#[test]
fn verify_delegation_credential_checks_the_latest_grant() {
    let doc = document();
    let envelope = grant(&signer(), 1);
    let credential = materialize(&envelope, 250, None);
    let records = vec![record(&envelope, 250)];
    let ok = verify_delegation_credential(&credential, &records, &doc, ID, ORIGIN, 300);
    assert!(
        ok.verified && ok.usable && ok.reasons.is_empty(),
        "{:?}",
        ok.reasons
    );
    let wrong = verify_delegation_credential(
        &credential,
        &records,
        &doc,
        ID,
        "https://tokenlist.ing",
        300,
    );
    assert!(wrong.verified && !wrong.usable);
    let expired = verify_delegation_credential(&credential, &records, &doc, ID, ORIGIN, 1000);
    assert_eq!(expired.reasons, vec!["expired".to_owned()]);
    let mut forged = credential.clone();
    forged.subject = other().agent_id();
    assert!(!verify_delegation_credential(&forged, &records, &doc, ID, ORIGIN, 300).verified);
    assert!(!verify_delegation_credential(&credential, &[], &doc, ID, ORIGIN, 300).verified);
    let revocation = revoke(&root(), 400, 2);
    let revoked = materialize(&revocation, 450, Some(&credential));
    let history = vec![record(&envelope, 250), record(&revocation, 450)];
    let verdict = verify_delegation_credential(&revoked, &history, &doc, ID, ORIGIN, 500);
    assert!(verdict.verified && !verdict.usable);
    assert_eq!(verdict.reasons, vec!["status is revoked".to_owned()]);
    // A service that hides the revocation does not match its own history.
    let mut hidden = revoked.clone();
    hidden.status = DelegationStatus::Active;
    assert!(!verify_delegation_credential(&hidden, &history, &doc, ID, ORIGIN, 500).verified);
    // Relying parties need only the latest grant record; auditors replay all.
    let latest_only = vec![record(&envelope, 250)];
    assert!(verify_delegation_credential(&revoked, &latest_only, &doc, ID, ORIGIN, 500).verified);
    audit_delegation_history(&revoked, &history, &doc, ID).unwrap();
    assert!(audit_delegation_history(&hidden, &history, &doc, ID).is_err());
    assert!(audit_delegation_history(&revoked, &latest_only, &doc, ID).is_err());
}

#[test]
fn credential_verification_binds_grant_fields_and_preserves_service_metadata() {
    let doc = document();
    let envelope = grant_with(&signer(), 1, |payload| {
        payload.relationship = Some("assistant".into());
        payload.not_before = Some(210);
        payload.constraints =
            Some(serde_json::from_value(json!({"limit": 1, "project": "alpha"})).unwrap());
    });
    let credential = materialize(&envelope, 250, None);
    let records = vec![record(&envelope, 250)];
    for (field, value) in [
        ("protocol", json!("other/1.0")),
        ("relationship", json!("owner")),
        ("scopes", json!(["admin"])),
        ("audiences", json!(["https://other.test"])),
        ("constraints", json!({"limit": true, "project": "alpha"})),
        ("not_before", json!(0)),
        ("expires_at", json!(2000)),
        ("accepted_at", json!(251)),
    ] {
        let mut forged = serde_json::to_value(&credential).unwrap();
        forged[field] = value;
        let forged = serde_json::from_value(forged).unwrap();
        let verdict = verify_delegation_credential(&forged, &records, &doc, ID, ORIGIN, 300);
        assert!(!verdict.verified && !verdict.usable, "{field}");
    }
    for field in ["constraints", "not_before", "expires_at"] {
        let mut forged = serde_json::to_value(&credential).unwrap();
        forged.as_object_mut().unwrap().remove(field);
        let forged = serde_json::from_value(forged).unwrap();
        assert!(
            !verify_delegation_credential(&forged, &records, &doc, ID, ORIGIN, 300).verified,
            "{field}"
        );
    }
    let mut control = credential;
    control.constraints =
        Some(serde_json::from_value(json!({"project": "alpha", "limit": 1.0})).unwrap());
    control.checked_at = 400;
    assert!(verify_delegation_credential(&control, &records, &doc, ID, ORIGIN, 400).usable);
    control.status = DelegationStatus::Suspended;
    let suspended = verify_delegation_credential(&control, &records, &doc, ID, ORIGIN, 400);
    assert!(suspended.verified && !suspended.usable);
}
