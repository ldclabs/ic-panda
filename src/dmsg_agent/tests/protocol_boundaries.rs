#![allow(clippy::result_large_err)]
// Adapted from agent-protocols v0.12.6 (MIT, LDC Labs); vectors are vendored unchanged.
use agent_protocols::{delegation, discourse, identity, profile};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf};

fn vectors(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../tests/vectors/agent-protocols/agent-{name}-1.0.vectors.json"
    ));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn shared_replay_boundary_covers_the_last_live_millisecond() {
    let v = vectors("identity");
    let c = &v["replay_boundary"];
    let e: identity::Envelope<Value> = serde_json::from_value(c["envelope"].clone()).unwrap();
    let mut store = identity::MemoryNonceStore::new();
    let mut options = identity::LiveWriteOptions {
        now_ms: c["accepted_at"].as_i64().unwrap(),
        window_ms: c["window_ms"].as_i64().unwrap(),
        nonce_ttl_ms: c["nonce_ttl_ms"].as_i64().unwrap(),
    };
    identity::verify_submission(&e, &options, &mut store, |_| false).unwrap();
    for step in c["attempts"].as_array().unwrap() {
        options.now_ms = step["now"].as_i64().unwrap();
        let error = identity::verify_submission(&e, &options, &mut store, |_| false).unwrap_err();
        assert_eq!(error.code(), step["error"].as_str());
    }
}

#[test]
fn shared_profile_pins_survive_restore() {
    let v = vectors("profile");
    let c = &v["latest_pins"];
    let owner = c["owner"].as_str().unwrap().parse().unwrap();
    let mut cache = profile::ProfileCache::new();
    let mut expected = String::new();
    for step in c["steps"].as_array().unwrap() {
        let e =
            serde_json::from_value(c["updates"][step["update"].as_str().unwrap()].clone()).unwrap();
        let result = cache.observe(&e, &owner);
        if let Some(code) = step["error"].as_str() {
            assert_eq!(result.unwrap_err().code(), Some(code));
        } else {
            expected = result.unwrap().hash;
        }
        cache = profile::ProfileCache::from_snapshot(&cache.snapshot().unwrap()).unwrap();
        assert_eq!(cache.get(&owner).unwrap().hash, expected);
    }
}

#[test]
fn shared_delegation_status_requires_fresh_consistent_evidence() {
    let v = vectors("delegation");
    let scope = &v["status_checks"];
    let document: delegation::PrincipalDocument =
        serde_json::from_value(scope["document"].clone()).unwrap();
    for c in scope["cases"].as_array().unwrap() {
        let credential = serde_json::from_value(c["credential"].clone()).unwrap();
        let records: Vec<_> = serde_json::from_value(c["records"].clone()).unwrap();
        let result = delegation::verify_delegation_credential(
            &credential,
            &records,
            &document,
            &document.id,
            scope["audience"].as_str().unwrap(),
            c["now"].as_i64().unwrap(),
        );
        assert_eq!(
            result.verified,
            c["verified"].as_bool().unwrap(),
            "{}",
            c["name"]
        );
        assert_eq!(
            result.usable,
            c["usable"].as_bool().unwrap(),
            "{}: {:?}",
            c["name"],
            result.reasons
        );
    }
}

#[test]
fn shared_discourse_payload_schemas_are_portable() {
    for c in vectors("discourse")["payload_validation"]
        .as_array()
        .unwrap()
    {
        let mut registry = discourse::TypeRegistry::new();
        let def = serde_json::from_value(
            json!({"type":"review.test","kind":"message","title":"Test","schema":c["schema"]}),
        )
        .unwrap();
        registry.define(def).unwrap();
        assert_eq!(
            registry
                .validate_payload("review.test", &c["value"])
                .is_ok(),
            c["valid"].as_bool().unwrap(),
            "{}",
            c["name"]
        );
        assert_eq!(registry.get("review.test").unwrap().schema, c["schema"]);
    }
}

#[test]
fn shared_discourse_blank_strings_and_atomic_import() {
    let v = vectors("discourse");
    for c in v["blank_strings"].as_array().unwrap() {
        let valid = c["valid"].as_bool().unwrap();
        let create=serde_json::from_value(json!({"host":"https://host.example","topic":c["value"],"visibility":"public","start_time":1000,"end_time":2000})).unwrap();
        let update = serde_json::from_value(json!({"topic":c["value"]})).unwrap();
        let def = serde_json::from_value(
            json!({"type":"review.blank","kind":"message","title":c["value"],"schema":{}}),
        )
        .unwrap();
        let message =
            serde_json::from_value(json!({"content_type":c["value"],"content":"test"})).unwrap();
        assert_eq!(
            discourse::validate_room_create_payload(&create).is_ok(),
            valid
        );
        assert_eq!(
            discourse::validate_room_update_payload(&update).is_ok(),
            valid
        );
        assert_eq!(discourse::validate_type_def(&def).is_ok(), valid);
        assert_eq!(
            discourse::validate_message_create_payload(&message).is_ok(),
            valid
        );
    }
    let c = &v["atomic_import"];
    let mut registry = discourse::TypeRegistry::new();
    registry
        .define(serde_json::from_value(c["initial"].clone()).unwrap())
        .unwrap();
    let before: Vec<_> = registry.definitions().cloned().collect();
    let pack: discourse::Pack = serde_json::from_value(c["pack"].clone()).unwrap();
    let declaration = serde_json::from_value(c["declaration"].clone()).unwrap();
    let error = registry
        .apply(&declaration, &BTreeMap::from([(pack.id.clone(), pack)]))
        .unwrap_err();
    assert_eq!(error.code(), c["error"].as_str());
    assert_eq!(registry.definitions().cloned().collect::<Vec<_>>(), before);
}

mod http {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    fn serve(bodies: Vec<Vec<u8>>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for body in bodies {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    if stream.read(&mut byte).unwrap() == 0 {
                        return;
                    }
                    request.push(byte[0]);
                }
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
                stream.write_all(&body).unwrap();
            }
        });
        url
    }
    fn client(url: String) -> agent_protocols::http_client::ProfileClient {
        agent_protocols::http_client::ProfileClient::with_client(
            url,
            reqwest::Client::builder().no_proxy().build().unwrap(),
        )
    }
    #[test]
    fn raw_http_signed_json_and_latest_profile_pins() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let v = vectors("identity");
                let owner = v["keys"][0]["agent_id"].as_str().unwrap().parse().unwrap();
                for c in v["http_json"].as_array().unwrap() {
                    let hex = c["body_hex"].as_str().unwrap();
                    let bytes = (0..hex.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                        .collect();
                    let result = client(serve(vec![bytes]))
                        .profile_events(&owner, Some(1))
                        .await;
                    if c["valid"] == true {
                        profile::validate_profile_update(&result.unwrap().result[0]).unwrap();
                    } else {
                        assert_eq!(
                            result.unwrap_err().code(),
                            Some("invalid_event"),
                            "{}",
                            c["name"]
                        );
                    }
                }
                let v = vectors("profile");
                let c = &v["latest_pins"];
                let bodies = ["new", "old", "old"]
                    .iter()
                    .map(|key| serde_json::to_vec(&json!({"result":[c["updates"][key]]})).unwrap())
                    .collect();
                let client = client(serve(bodies));
                let latest = client.latest_profile_update(&owner).await.unwrap();
                assert_eq!(latest.hash, c["updates"]["new"]["hash"]);
                assert_eq!(
                    client
                        .latest_profile_update(&owner)
                        .await
                        .unwrap_err()
                        .code(),
                    Some("stale_profile")
                );
                assert_eq!(
                    client
                        .profile_events(&owner, Some(20))
                        .await
                        .unwrap()
                        .result[0]
                        .hash,
                    c["updates"]["old"]["hash"]
                );
            });
    }
}
