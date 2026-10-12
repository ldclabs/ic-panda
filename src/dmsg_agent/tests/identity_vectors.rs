//! Agent Identity conformance vectors shared by every SDK.

use agent_protocols::identity::*;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde_json::Value;

fn vectors() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/vectors/agent-protocols/agent-identity-1.0.vectors.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn seed(hex: &str) -> [u8; 32] {
    let mut seed = [0_u8; 32];
    for (index, byte) in seed.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).unwrap();
    }
    seed
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap()
}

#[test]
fn keys_jcs_bytes_hashes_and_signatures_are_reproduced() {
    let vectors = vectors();
    for key in vectors["keys"].as_array().unwrap() {
        let signer = AgentSigner::from_seed(seed(text(&key["seed"])));
        assert_eq!(signer.agent_id().to_string(), text(&key["agent_id"]));
        assert_eq!(
            URL_SAFE_NO_PAD.encode(signer.agent_id().public_key_bytes().unwrap()),
            text(&key["public_key"])
        );
    }
    for vector in vectors["events"].as_array().unwrap() {
        let name = text(&vector["name"]);
        let signer = AgentSigner::from_seed(seed(text(&vector["seed"])));
        let event: Event<Value> = serde_json::from_value(vector["event"].clone()).unwrap();
        let jcs = String::from_utf8(canonical_event_bytes(&event).unwrap()).unwrap();
        assert_eq!(jcs, text(&vector["jcs"]), "{name}");
        let envelope = signer.sign_event(event).unwrap();
        assert_eq!(envelope.hash, text(&vector["hash"]), "{name}");
        assert_eq!(envelope.signature, text(&vector["signature"]), "{name}");
        verify_envelope(&envelope).unwrap();
        // The vector text parses strictly and round-trips.
        assert_eq!(
            parse_strict_json(text(&vector["jcs"])).unwrap(),
            vector["event"]
        );
    }
}

#[test]
fn only_strictly_valid_ed25519_signatures_verify() {
    let vectors = vectors();
    for vector in vectors["signatures"].as_array().unwrap() {
        let name = text(&vector["name"]);
        let public_key: [u8; 32] = URL_SAFE_NO_PAD
            .decode(text(&vector["public_key"]))
            .unwrap()
            .try_into()
            .unwrap();
        let message = URL_SAFE_NO_PAD.decode(text(&vector["message"])).unwrap();
        let signature: [u8; 64] = URL_SAFE_NO_PAD
            .decode(text(&vector["signature"]))
            .unwrap()
            .try_into()
            .unwrap();
        let result = verify_ed25519_strict_bytes(&public_key, &message, &signature);
        assert_eq!(result.is_ok(), vector["valid"].as_bool().unwrap(), "{name}");
        if let Err(error) = result {
            assert_eq!(error.code(), Some("invalid_signature"), "{name}");
        }
    }
}

#[test]
fn malformed_agent_ids_are_rejected() {
    let vectors = vectors();
    for value in vectors["agent_ids"]["valid"].as_array().unwrap() {
        text(value).parse::<AgentId>().unwrap();
    }
    for vector in vectors["agent_ids"]["invalid"].as_array().unwrap() {
        assert!(
            text(&vector["value"]).parse::<AgentId>().is_err(),
            "{}",
            vector["name"]
        );
    }
}

#[test]
fn agent_urls_parse_into_components_and_format_back_exactly() {
    let vectors = vectors();
    for vector in vectors["agent_urls"]["valid"].as_array().unwrap() {
        let parsed = parse_agent_url(text(&vector["value"])).unwrap();
        assert_eq!(serde_json::to_value(&parsed).unwrap(), vector["parsed"]);
        assert_eq!(format_agent_url(&parsed).unwrap(), text(&vector["value"]));
    }
    for vector in vectors["agent_urls"]["invalid"].as_array().unwrap() {
        assert!(
            parse_agent_url(text(&vector["value"])).is_err(),
            "{}",
            vector["name"]
        );
    }
    let mut half = parse_agent_url(text(&vectors["agent_ids"]["valid"][0])).unwrap();
    half.protocol = Some("mail".into());
    assert!(format_agent_url(&half).is_err());
}

#[test]
fn signed_json_is_parsed_strictly() {
    let vectors = vectors();
    for vector in vectors["json"]["valid"].as_array().unwrap() {
        parse_strict_json(text(&vector["text"])).unwrap();
    }
    for vector in vectors["json"]["invalid"].as_array().unwrap() {
        let error = parse_strict_json(text(&vector["text"])).unwrap_err();
        assert!(
            error.to_string().contains("invalid JSON"),
            "{}",
            vector["name"]
        );
    }
    let signer = AgentSigner::from_seed([7; 32]);
    let event: Event<Value> =
        serde_json::from_value(vectors["events"][0]["event"].clone()).unwrap();
    let envelope = signer.sign_event(event).unwrap();
    let json = serde_json::to_string(&envelope).unwrap();
    let parsed: Envelope<Value> = parse_envelope_json(&json).unwrap();
    assert_eq!(parsed.hash, envelope.hash);
    let mut extra = serde_json::to_value(&envelope).unwrap();
    extra["extra"] = Value::from(1);
    assert!(parse_envelope_json::<Value>(&extra.to_string())
        .unwrap_err()
        .to_string()
        .contains("unknown envelope field"));
    assert!(parse_envelope_json::<Value>("[]").is_err());
}

#[test]
fn the_event_object_is_closed() {
    let vectors = vectors();
    let profile: Event<Value> =
        serde_json::from_value(vectors["events"][0]["event"].clone()).unwrap();
    validate_event_fields(&profile, &[]).unwrap();
    let message: Event<Value> =
        serde_json::from_value(vectors["events"][1]["event"].clone()).unwrap();
    validate_event_fields(&message, &["room_id", "base_seq", "base_hash", "mentions"]).unwrap();
    assert!(validate_event_fields(&message, &[]).is_err());
    for vector in vectors["events_closed_shape"]["invalid"]
        .as_array()
        .unwrap()
    {
        let result = serde_json::from_value::<Event<Value>>(vector["event"].clone())
            .map_err(|error| error.to_string())
            .and_then(|event| {
                validate_event_fields(&event, &[]).map_err(|error| error.to_string())
            });
        assert!(result.is_err(), "{}", vector["name"]);
    }
}

#[test]
fn max_seen_nonce_jumps_are_bounded() {
    let vectors = vectors();
    for vector in vectors["max_seen_nonce"].as_array().unwrap() {
        let name = text(&vector["name"]);
        let next = vector["next_nonce"].as_u64().unwrap();
        let mut manager = ClientNonceManager::with_next(next).unwrap();
        let result = manager
            .observe_max_nonce_header(text(&vector["header"]), vector["now_ms"].as_i64().unwrap());
        if vector["accepted"].as_bool().unwrap() {
            result.unwrap();
            assert_eq!(
                manager.peek(),
                vector["next_after"].as_u64().unwrap(),
                "{name}"
            );
        } else {
            assert!(result.is_err(), "{name}");
            assert_eq!(manager.peek(), next, "{name}");
        }
    }
}

#[test]
fn clock_derived_nonces_stay_monotonic() {
    let mut manager = ClientNonceManager::new();
    assert_eq!(manager.next_nonce_at(1_000).unwrap(), 1_000);
    assert_eq!(manager.next_nonce_at(1_000).unwrap(), 1_001);
    assert_eq!(manager.next_nonce_at(900).unwrap(), 1_002);
    assert_eq!(manager.next_nonce().unwrap(), 1_003);
    assert_eq!(manager.next_nonce_at(5_000).unwrap(), 5_000);
}

#[test]
fn origins_are_serialized_https_origins() {
    let vectors = vectors();
    for origin in vectors["origins"]["valid"].as_array().unwrap() {
        validate_origin(text(origin)).unwrap();
    }
    for origin in vectors["origins"]["invalid"].as_array().unwrap() {
        assert!(validate_origin(text(origin)).is_err(), "{origin}");
    }
}

#[test]
fn request_jwts_verify_exactly_as_listed() {
    let vectors = vectors();
    let jwts = &vectors["request_jwts"];
    let context = RequestAuthContext {
        audience: text(&jwts["audience"]).to_owned(),
        now_secs: jwts["now_secs"].as_i64().unwrap(),
        max_ttl_secs: jwts["max_ttl_secs"].as_i64().unwrap(),
    };
    for case in jwts["valid"].as_array().unwrap() {
        let claims = verify_request_jwt(text(&case["token"]), &context).unwrap();
        assert_eq!(serde_json::to_value(claims).unwrap(), case["claims"]);
    }
    for case in jwts["invalid"].as_array().unwrap() {
        assert!(
            verify_request_jwt(text(&case["token"]), &context).is_err(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn submissions_resolve_resubmission_windows_and_nonce_replays() {
    let vectors = vectors();
    let submissions = &vectors["submissions"];
    let envelopes: std::collections::BTreeMap<String, Envelope<Value>> =
        serde_json::from_value(submissions["envelopes"].clone()).unwrap();
    let mut store = MemoryNonceStore::new();
    for step in submissions["steps"].as_array().unwrap() {
        let envelope = &envelopes[text(&step["envelope"])];
        let accepted: Vec<&str> = step["accepted"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| envelopes[text(name)].hash.as_str())
            .collect();
        let options = LiveWriteOptions {
            now_ms: step["now_ms"].as_i64().unwrap(),
            window_ms: submissions["window_ms"].as_i64().unwrap(),
            nonce_ttl_ms: DEFAULT_NONCE_TTL_MS,
        };
        let outcome = match verify_submission(envelope, &options, &mut store, |hash| {
            accepted.contains(&hash)
        }) {
            Ok(SubmissionResult::Accepted { .. }) => "accepted".to_owned(),
            Ok(SubmissionResult::Resubmission) => "resubmission".to_owned(),
            Err(error) => error.code().unwrap_or("other").to_owned(),
        };
        assert_eq!(outcome, text(&step["expected"]), "{}", step["name"]);
    }
}
