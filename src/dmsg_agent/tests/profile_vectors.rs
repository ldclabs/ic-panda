// Adapted from agent-protocols v0.12.6 (MIT, LDC Labs); vectors are vendored unchanged.
//! Agent Profile conformance vectors shared by every SDK.

use agent_protocols::identity::Envelope;
use agent_protocols::profile::*;
use serde_json::Value;

fn vectors() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/vectors/agent-protocols/agent-profile-1.0.vectors.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Parses a payload into the closed payload type, checks the actor binding,
/// and applies the Section 4.1 field rules.
fn check(case: &Value) -> Result<(), String> {
    let payload: ProfileUpdatePayload =
        serde_json::from_value(case["payload"].clone()).map_err(|e| e.to_string())?;
    if payload.id.as_str() != case["actor"].as_str().unwrap() {
        return Err("payload id differs from actor".to_owned());
    }
    validate_profile_payload(&payload).map_err(|e| e.to_string())
}

#[test]
fn payloads_are_accepted_and_rejected_as_listed() {
    let vectors = vectors();
    for case in vectors["payloads"]["valid"].as_array().unwrap() {
        check(case).unwrap_or_else(|e| panic!("{}: {e}", case["name"]));
    }
    for case in vectors["payloads"]["invalid"].as_array().unwrap() {
        assert!(check(case).is_err(), "{}", case["name"]);
    }
}

#[test]
fn history_materializes_the_greatest_nonce() {
    let vectors = vectors();
    let envelopes: Vec<Envelope<ProfileUpdatePayload>> =
        serde_json::from_value(vectors["history"]["envelopes"].clone()).unwrap();
    for envelope in &envelopes {
        validate_profile_update(envelope).unwrap();
    }
    let latest = latest_profile_update(&envelopes).unwrap();
    let document = serde_json::to_value(materialize_profile(latest).unwrap()).unwrap();
    for (field, expected) in vectors["history"]["latest"].as_object().unwrap() {
        assert_eq!(&document[field], expected, "{field}");
    }
}

#[test]
fn explicit_empty_arrays_and_objects_verify_and_materialize_as_signed() {
    let vectors = vectors();
    let envelope: Envelope<ProfileUpdatePayload> =
        serde_json::from_value(vectors["explicit_empty"]["envelope"].clone()).unwrap();
    validate_profile_update(&envelope).unwrap();
    let document = serde_json::to_value(materialize_profile(&envelope).unwrap()).unwrap();
    assert_eq!(document, vectors["explicit_empty"]["document"]);
}
