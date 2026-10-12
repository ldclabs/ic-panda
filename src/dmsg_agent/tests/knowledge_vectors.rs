// Copied from agent-protocols v0.12.0; its SdkError is large by design.
#![allow(clippy::result_large_err)]

use agent_protocols::{
    identity::{self, AgentSigner, Envelope},
    knowledge::*,
    Result,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
const SERVICE: &str = "https://knowledge.example.com";
fn vectors() -> Value {
    // The container deliberately holds one lone UTF-16 surrogate. Rust strings
    // cannot represent it; preserve its exact JSON lexeme and exercise the
    // public strict JSON boundary instead of attempting to construct a &str.
    let source =
        include_str!("../../../tests/vectors/agent-protocols/agent-knowledge-1.0.vectors.json");
    let source = source.replace(r#""text": "\ud800""#, r#""raw_text_json": "\"\\ud800\"""#);
    serde_json::from_str(&source).unwrap()
}
fn array(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn envelope(v: &Value, name: &Value) -> Value {
    v["fixtures"][text(name)]["envelope"].clone()
}
fn id(v: &Value, name: &Value) -> String {
    text(&v["fixtures"][text(name)]["envelope"]["hash"]).to_owned()
}
fn names(v: &Value, list: &Value) -> Vec<String> {
    array(list).iter().map(|n| id(v, n)).collect()
}
fn parameters(v: &Value) -> Vec<(String, String)> {
    array(v)
        .iter()
        .map(|p| (text(&p[0]).into(), text(&p[1]).into()))
        .collect()
}
fn modes(case: &Value) -> Vec<String> {
    case.get("modes")
        .map(|v| array(v).iter().map(|v| text(v).into()).collect())
        .unwrap_or_else(|| vec!["lexical".into(), "semantic".into(), "hybrid".into()])
}
fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}
fn mutate(mut value: Value, changes: &Value) -> Value {
    for change in array(changes) {
        let path = text(&change["path"]);
        let (parent_path, key) = path.rsplit_once('/').unwrap();
        let parent = value.pointer_mut(parent_path).unwrap();
        match text(&change["op"]) {
            "set" => {
                if parent.is_array() {
                    parent[key.parse::<usize>().unwrap()] = change["value"].clone();
                } else {
                    parent[key] = change["value"].clone();
                }
            }
            "remove" => {
                if let Some(a) = parent.as_array_mut() {
                    a.remove(key.parse::<usize>().unwrap());
                } else {
                    parent.as_object_mut().unwrap().remove(key);
                }
            }
            _ => panic!("unexpected mutation"),
        }
    }
    value
}
fn assert_outcome<T>(result: &Result<T>, expected: &Value, case: &Value) {
    let actual = match result {
        Ok(_) => "valid",
        Err(e) => e.code().unwrap_or("unmapped_error"),
    };
    assert_eq!(
        actual,
        text(expected),
        "{}: {:?}",
        text(&case["name"]),
        result.as_ref().err()
    );
}
fn model(v: &Value, case: &Value) -> KnowledgeStore {
    let mut store = KnowledgeStore::new(SERVICE).unwrap();
    for name in array(&case["accepted"]) {
        store
            .submit(&envelope(v, name), v["now"].as_i64().unwrap())
            .unwrap();
    }
    for name in array(&case["hidden"]) {
        store.hide(&id(v, name));
    }
    store
}
fn selection(v: &Value, case: &Value, request: &Value) -> KnowledgeSearchSelection {
    KnowledgeSearchSelection {
        candidates: names(v, &case["candidates"]),
        ranking: case
            .get("ranking")
            .cloned()
            .unwrap_or(json!({"mode":request["mode"],"id":"fixture-v1"})),
        coverage: case
            .get("coverage")
            .cloned()
            .unwrap_or(json!({"exhaustive":true,"reasons":[]})),
        ..Default::default()
    }
}

#[test]
fn bundled_schema_matches_normative_source() {
    assert_eq!(
        SCHEMA_JSON,
        include_str!("../../../tests/vectors/agent-protocols/agent-knowledge-1.0.schema.json")
    );
}

#[test]
fn all_signature_fixtures() {
    let v = vectors();
    assert_eq!(v["fixtures"].as_object().unwrap().len(), 63);
    for (name, fixture) in v["fixtures"].as_object().unwrap() {
        let raw = &fixture["envelope"];
        let e: Envelope = serde_json::from_value(raw.clone()).unwrap();
        identity::verify_envelope(&e).unwrap();
        assert_eq!(
            String::from_utf8(identity::canonical_event_bytes(&e.event).unwrap()).unwrap(),
            text(&fixture["canonical_event_utf8"]),
            "{name}"
        );
        let signer = AgentSigner::from_seed(
            bytes(text(&v["seeds"][text(&fixture["signer"])]))
                .try_into()
                .unwrap(),
        );
        assert_eq!(signer.agent_id(), e.event.actor);
        assert_eq!(
            serde_json::to_value(signer.sign_event(e.event).unwrap()).unwrap(),
            *raw,
            "{name}"
        );
    }
}

#[test]
fn identity_and_schema_cases() {
    let v = vectors();
    for case in array(&v["identity_cases"]) {
        let result = if let Some(raw) = case["raw_json"].as_str() {
            identity::parse_strict_json(raw).map(|_| ())
        } else {
            validate_knowledge_envelope(&mutate(envelope(&v, &case["fixture"]), &case["changes"]))
                .map(|_| ())
        };
        assert_outcome(&result, &case["expected"], case);
    }
    for case in array(&v["schema_cases"]) {
        let value = case
            .get("value")
            .cloned()
            .unwrap_or_else(|| mutate(envelope(&v, &case["fixture"]), &case["changes"]));
        let result = validate_knowledge_schema(&value, text(&case["definition"]));
        assert_eq!(
            result.is_ok(),
            case["valid"].as_bool().unwrap(),
            "{}: {:?}",
            text(&case["name"]),
            result
        );
    }
}

#[test]
fn object_and_known_set_cases() {
    let v = vectors();
    for case in array(&v["object_cases"]) {
        let store = model(&v, case);
        let retained = store.known_envelopes();
        let item = envelope(&v, &case["fixture"]);
        let result = validate_knowledge_envelope(&item)
            .and_then(|_| validate_knowledge_dependencies(&item, &retained));
        assert_outcome(&result, &case["expected"], case);
        if text(&case["expected"]) == "missing_dependency" {
            assert_eq!(
                result.unwrap_err().data(),
                Some(&json!({"missing":case["missing"]}))
            );
        }
    }
    for case in array(&v["view_cases"]) {
        for order in array(&case["arrival_orders"]) {
            let mut store = KnowledgeStore::new(SERVICE).unwrap();
            let mut pending: Vec<_> = array(order).to_vec();
            while !pending.is_empty() {
                let old = pending.len();
                let mut unresolved = Vec::new();
                for name in pending {
                    let before = format!("{store:?}");
                    match store.submit(&envelope(&v, &name), v["now"].as_i64().unwrap()) {
                        Ok(_) => (),
                        Err(e) => {
                            assert_eq!(e.code(), Some("missing_dependency"));
                            assert_eq!(format!("{store:?}"), before);
                            unresolved.push(name);
                        }
                    }
                }
                pending = unresolved;
                assert!(pending.len() < old, "dependency deadlock");
            }
            assert_eq!(
                materialize_knowledge(&store.known_envelopes()).unwrap(),
                case["expected"],
                "{}",
                text(&case["name"])
            );
            if let Some(links) = case["expected_relations"].as_object() {
                for (name, expected) in links {
                    assert_eq!(
                        store.event(&id(&v, &json!(name))).unwrap()["envelope"]["event"]["payload"]
                            ["relations"],
                        *expected
                    );
                }
            }
        }
    }
}

#[test]
fn acceptance_and_evidence_cases() {
    let v = vectors();
    for case in array(&v["acceptance_cases"]) {
        let mut store = KnowledgeStore::new(SERVICE).unwrap();
        let mut receipts = BTreeMap::new();
        for step in array(&case["steps"]) {
            if let Some(name) = step.get("withhold") {
                store.hide(&id(&v, name));
                continue;
            }
            let item = envelope(&v, &step["fixture"]);
            let now = step["now"].as_i64().unwrap();
            let before = format!("{store:?}");
            let result = store.submit(&item, now);
            let outcome = match &result {
                Ok(record) => match receipts.get(text(&item["hash"])) {
                    Some(original) => {
                        assert_eq!(record, original, "retry must return the original record");
                        "resubmission"
                    }
                    None => {
                        assert_eq!(record["accepted_at"], step["now"]);
                        receipts.insert(text(&item["hash"]).to_owned(), record.clone());
                        "accepted"
                    }
                },
                Err(e) => e.code().unwrap_or("unmapped_error"),
            };
            assert_eq!(
                outcome,
                text(&step["expected"]),
                "{} / {}: {:?}",
                text(&case["name"]),
                text(&step["fixture"]),
                result
            );
            assert_eq!(store.checkpoint(), step["seq"].as_u64().unwrap());
            if outcome != "accepted" {
                assert_eq!(
                    format!("{store:?}"),
                    before,
                    "rejection/retry changed state"
                );
            }
        }
    }
    for case in array(&v["evidence_cases"]) {
        let raw = case["representation_hex"].as_str().map(bytes);
        let result = verify_knowledge_evidence(case["digest"].as_str(), raw.as_deref()).unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            case["expected"],
            "{}",
            text(&case["name"])
        );
    }
}

#[test]
fn query_and_text_cases() {
    let v = vectors();
    for case in array(&v["query_cases"]) {
        let result = parse_knowledge_query(&parameters(&case["parameters"])).map(|request| {
            let matches: Vec<_> = array(&case["accepted"])
                .iter()
                .filter(|name| knowledge_query_matches(&envelope(&v, name), &request).unwrap())
                .cloned()
                .collect();
            assert_eq!(json!(matches), case["matches"], "{}", text(&case["name"]));
        });
        assert_outcome(&result, &case["expected"], case);
    }
    for case in array(&v["text_cases"]) {
        let result = case["raw_text_json"]
            .as_str()
            .map_or_else(
                || {
                    knowledge_text_terms(
                        text(&case["text"]),
                        case["lexical"].as_bool().unwrap_or(true),
                    )
                },
                |raw| parse_knowledge_read_json(raw).map(|_| Vec::new()),
            )
            .map(|terms| {
                assert_eq!(json!(terms), case["terms"], "{}", text(&case["name"]));
                if case.get("fixture").is_some() {
                    assert_eq!(
                        knowledge_text_matches(
                            &envelope(&v, &case["fixture"]),
                            text(&case["text"])
                        )
                        .unwrap(),
                        case["matches"].as_bool().unwrap()
                    );
                }
            });
        assert_outcome(&result, &case["expected"], case);
    }
}

#[test]
fn batch_and_search_cases() {
    let v = vectors();
    let now = v["now"].as_i64().unwrap();
    for case in array(&v["batch_cases"]) {
        let store = model(&v, case);
        let before = format!("{store:?}");
        let result = (|| -> Result<()> {
            let request = if let Some(raw) = case["raw_json"].as_str() {
                parse_knowledge_read_json(raw)?
            } else {
                case["request"].clone()
            };
            let response = mutate(store.batch(&request, now)?, &case["response_changes"]);
            validate_knowledge_batch_response(
                &response,
                &validate_knowledge_batch_request(&request)?,
                SERVICE,
            )?;
            assert_eq!(
                json!(array(&response["result"])
                    .iter()
                    .map(|r| r["envelope"]["hash"].clone())
                    .collect::<Vec<_>>()),
                case["result"]
            );
            assert_eq!(response["missing"], case["missing"]);
            Ok(())
        })();
        assert_outcome(&result, &case["expected"], case);
        assert_eq!(format!("{store:?}"), before);
    }
    for case in array(&v["search_cases"]) {
        let store = model(&v, case);
        let before = format!("{store:?}");
        let result = (|| -> Result<()> {
            let request = if let Some(raw) = case["raw_json"].as_str() {
                parse_knowledge_read_json(raw)?
            } else {
                case["request"].clone()
            };
            validate_knowledge_search_request(&request, &modes(case))?;
            let response = mutate(
                store.search(&request, &selection(&v, case, &request), &modes(case), now)?,
                &case["response_changes"],
            );
            validate_knowledge_search_response(&response, &request, SERVICE)?;
            let hashes: Vec<_> = array(&response["result"])
                .iter()
                .map(|r| r["record"]["envelope"]["hash"].clone())
                .collect();
            assert_eq!(
                json!(hashes),
                case.get("result").cloned().unwrap_or(json!([])),
                "{}",
                text(&case["name"])
            );
            Ok(())
        })();
        assert_outcome(&result, &case["expected"], case);
        assert_eq!(format!("{store:?}"), before);
    }
}

#[test]
fn pagination_and_discovery_cases() {
    let v = vectors();
    for case in array(&v["pagination_cases"]) {
        let mut store = model(&v, case);
        let mut now = v["now"].as_i64().unwrap();
        let mut previous = Value::Null;
        let mut tracker = KnowledgePageTracker::new(SERVICE).unwrap();
        for step in array(&case["steps"]) {
            for name in array(&step["add"]) {
                store
                    .submit(&envelope(&v, name), v["now"].as_i64().unwrap())
                    .unwrap();
            }
            for name in array(&step["hide"]) {
                store.hide(&id(&v, name));
            }
            now += step["advance_ms"].as_i64().unwrap_or(0);
            let result = (|| -> Result<()> {
                let mut request = parse_knowledge_query(&parameters(&step["parameters"]))?;
                if step["continue"] == true {
                    request["cursor"] = previous["next_cursor"].clone();
                } else {
                    tracker = KnowledgePageTracker::new(SERVICE)?;
                }
                let response = mutate(store.query(&request, now)?, &step["response_changes"]);
                tracker.accept(&request, &response)?;
                let hashes: Vec<_> = array(&response["result"])
                    .iter()
                    .map(|r| text(&r["envelope"]["hash"]).to_owned())
                    .collect();
                assert_eq!(
                    hashes,
                    names(&v, &step["matches"]),
                    "{}",
                    text(&case["name"])
                );
                assert_eq!(
                    response.get("next_cursor").is_some(),
                    step["more"].as_bool().unwrap(),
                    "{}",
                    text(&case["name"])
                );
                previous = response;
                Ok(())
            })();
            assert_outcome(&result, &step["expected"], case);
        }
    }
    for case in array(&v["discovery_cases"]) {
        assert_outcome(
            &validate_knowledge_discovery(
                &case["document"],
                case["origin"].as_str().unwrap_or(SERVICE),
            ),
            &case["expected"],
            case,
        );
    }
}

#[test]
fn typed_roundtrip_and_builders_preserve_signed_data() {
    let v = vectors();
    let mut count = 0;
    for fixture in v["fixtures"].as_object().unwrap().values() {
        let raw = &fixture["envelope"];
        if validate_knowledge_envelope(raw).is_err() {
            continue;
        }
        let roundtrip = match text(&raw["event"]["type"]) {
            "knowledge.publish" => serde_json::to_value(
                serde_json::from_value::<Envelope<KnowledgePublishPayload>>(raw.clone()).unwrap(),
            )
            .unwrap(),
            "knowledge.assess" => serde_json::to_value(
                serde_json::from_value::<Envelope<KnowledgeAssessPayload>>(raw.clone()).unwrap(),
            )
            .unwrap(),
            _ => serde_json::to_value(
                serde_json::from_value::<Envelope<KnowledgeRetractPayload>>(raw.clone()).unwrap(),
            )
            .unwrap(),
        };
        assert_eq!(roundtrip, *raw);
        validate_knowledge_envelope(&roundtrip).unwrap();
        count += 1;
    }
    assert!(count > 30);
    let signer = AgentSigner::from_seed([7; 32]);
    let mut payload = envelope(&v, &json!("original"))["event"]["payload"].clone();
    payload["evidence"] = json!([]);
    payload["relations"] = json!([]);
    payload["tags"] = json!([]);
    payload["profiles"] = json!([]);
    payload["extra"] = json!({"arbitrary":{"nested":[{},[],null]},"duplicate_values":[1,1]});
    let payload: KnowledgePublishPayload = serde_json::from_value(payload).unwrap();
    let signed = signer
        .sign_event(knowledge_publish_event(
            signer.agent_id(),
            v["now"].as_i64().unwrap(),
            1,
            payload,
        ))
        .unwrap();
    validate_typed_knowledge_envelope(&signed).unwrap();
    let raw = serde_json::to_value(&signed).unwrap();
    let back: Envelope<KnowledgePublishPayload> = serde_json::from_value(raw.clone()).unwrap();
    assert_eq!(serde_json::to_value(back).unwrap(), raw);
    let mut unknown = raw.clone();
    unknown["unknown"] = json!(1);
    assert!(validate_knowledge_envelope(&unknown).is_err());
    let mut unknown = raw;
    unknown["event"]["unknown"] = json!(1);
    assert!(validate_knowledge_envelope(&unknown).is_err());
}

#[test]
fn pruning_cursors_and_page_guards() {
    let v = vectors();
    let now = v["now"].as_i64().unwrap();
    let mut store = model(
        &v,
        &json!({"accepted":["original","branch_left","branch_right"]}),
    );
    let request = json!({"limit":1});
    let first = store.query(&request, now).unwrap();
    let mut tracker = KnowledgePageTracker::new(SERVICE).unwrap();
    tracker.accept(&request, &first).unwrap();
    assert_eq!(tracker.checkpoint(), None);
    let original = envelope(&v, &json!("original"));
    let hash = text(&original["hash"]).to_owned();
    store.hide(&hash);
    assert!(store.event(&hash).is_err());
    assert_eq!(store.submit(&original, now).unwrap()["seq"], 1);
    store.prune(&hash);
    assert_eq!(store.submit(&original, now).unwrap()["seq"], 4);
    let next = json!({"limit":1,"cursor":first["next_cursor"]});
    let mut second = store.query(&next, now + 1).unwrap();
    let mut replay = second.clone();
    replay["result"] = first["result"].clone();
    assert!(tracker.clone().accept(&next, &replay).is_err());
    tracker.accept(&next, &second).unwrap();
    let last = json!({"limit":1,"cursor":second["next_cursor"]});
    second = store.query(&last, now + 2).unwrap();
    tracker.accept(&last, &second).unwrap();
    assert!(tracker.is_complete());
    assert_eq!(tracker.checkpoint(), Some(3));
    let poll = store.query(&json!({"after_seq":3}), now).unwrap();
    assert_eq!(poll["result"][0]["seq"], 4);
    assert_eq!(
        store
            .query(&json!({"after_seq":5}), now)
            .unwrap_err()
            .code(),
        Some("invalid_request")
    );
    for cursor in ["x", "1.2.3", &format!("{}x", text(&first["next_cursor"]))] {
        assert_eq!(
            store
                .query(&json!({"limit":1,"cursor":cursor}), now)
                .unwrap_err()
                .code(),
            Some("invalid_cursor")
        );
    }
    assert_eq!(
        KnowledgeStore::new(SERVICE)
            .unwrap()
            .query(&next, now)
            .unwrap_err()
            .code(),
        Some("invalid_cursor")
    );
    assert!(materialize_knowledge(&BTreeMap::from([(hash, original)])).is_ok());
}

#[test]
fn integral_json_numbers_and_store_limits() {
    let v = vectors();
    let now = v["now"].as_i64().unwrap();
    let mut store = KnowledgeStore::new(SERVICE).unwrap();
    let mut original = envelope(&v, &json!("original"));
    original["event"]["nonce"] = json!(original["event"]["nonce"].as_u64().unwrap() as f64);
    original["event"]["created_at"] = json!(now as f64);
    // JCS hashes 1 and 1.0 identically and protocol JSON integers accept both.
    validate_knowledge_envelope(&original).unwrap();
    store.submit(&original, now).unwrap();
    let page = store
        .query(&json!({"limit":1.0,"created_from":0.0}), now)
        .unwrap();
    let mut floated = page.clone();
    floated["checkpoint"] = json!(1.0);
    floated["result"][0]["seq"] = json!(1.0);
    floated["result"][0]["accepted_at"] = json!(now as f64);
    validate_knowledge_query_response(&floated, &json!({"limit":1.0}), SERVICE).unwrap();
    let mut tracker = KnowledgePageTracker::new(SERVICE).unwrap();
    tracker.accept(&json!({"limit":1.0}), &floated).unwrap();
    assert_eq!(tracker.checkpoint(), Some(1));
    let mut small = KnowledgeStore::new(SERVICE).unwrap();
    small.max_envelope_bytes = 100;
    assert_eq!(
        small.submit(&original, now).unwrap_err().code(),
        Some("payload_too_large")
    );
    let mut closed = KnowledgeStore::new(SERVICE).unwrap();
    closed.set_admission(|_| Err(agent_protocols::SdkError::PermissionDenied));
    assert_eq!(
        closed.submit(&original, now).unwrap_err().code(),
        Some("permission_denied")
    );
    assert_eq!(closed.checkpoint(), 0);
}

#[test]
fn relationships_may_cite_or_dispute_assessments() {
    let v = vectors();
    let known: BTreeMap<String, Value> = ["original", "assessment", "retract_original"]
        .iter()
        .map(|name| (id(&v, &json!(name)), envelope(&v, &json!(name))))
        .collect();
    for (name, expected) in [
        ("derived_from_assessment", None),
        ("contradicts_assessment", None),
        ("supports_retraction", Some("invalid_target")),
    ] {
        let result = validate_knowledge_dependencies(&envelope(&v, &json!(name)), &known);
        assert_eq!(
            result
                .err()
                .and_then(|e| e.code().map(str::to_owned))
                .as_deref(),
            expected
        );
    }
}

#[test]
fn duplicate_events_are_rejected_in_batch_and_search() {
    let v = vectors();
    let now = v["now"].as_i64().unwrap();
    let store = model(&v, &json!({"accepted":["original","branch_left"]}));
    let ids = names(&v, &json!(["original", "branch_left"]));
    let mut batch = store.batch(&json!({"hashes":ids}), now).unwrap();
    batch["result"][1] = batch["result"][0].clone();
    assert_eq!(
        validate_knowledge_batch_response(&batch, &ids, SERVICE)
            .unwrap_err()
            .code(),
        Some("invalid_response")
    );
    let request = json!({"mode":"semantic","text":"research"});
    let selection = KnowledgeSearchSelection {
        candidates: ids.clone(),
        ranking: json!({"mode":"semantic","id":"test"}),
        coverage: json!({"exhaustive":false,"reasons":["approximate"]}),
        explanations: BTreeMap::from([(ids[0].clone(), "top hit".to_owned())]),
    };
    let mut search = store
        .search(&request, &selection, &["semantic".into()], now)
        .unwrap();
    assert!(search.get("next_cursor").is_none());
    assert_eq!(search["result"][0]["explanation"], "top hit");
    assert!(search["result"][1].get("explanation").is_none());
    search["result"][1] = search["result"][0].clone();
    assert_eq!(
        validate_knowledge_search_response(&search, &request, SERVICE)
            .unwrap_err()
            .code(),
        Some("invalid_response")
    );
}
