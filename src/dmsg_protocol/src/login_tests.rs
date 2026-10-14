use super::*;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use cbor2::Value;
use serde_json::{json, Value as Json};

const VECTORS: &str = "../../tests/vectors/client/login-v1.json";

type Map = Vec<(Value, Value)>;

fn session_pubkey() -> Vec<u8> {
    let mut key = ED25519_SPKI_PREFIX.to_vec();
    key.extend([0x11; 32]);
    key
}

fn request() -> LoginRequest {
    LoginRequest {
        v: 1,
        state: Hash::new([0x5a; 32]),
        session_pubkey: session_pubkey().into(),
        ttl_ms: 15 * MINUTE,
        derivation_origin: "https://dmsg.net".into(),
        next_url: "http://127.0.0.1:53817/auth/callback".into(),
    }
}

/// The request as an editable CBOR map, in source order.
fn request_map(request: &LoginRequest) -> Map {
    let Value::Map(entries) = cbor2::from_slice(&canonical(request)).unwrap() else {
        unreachable!()
    };
    entries
}

fn set(map: &mut Map, key: &str, value: Option<Value>) {
    map.retain(|(k, _)| k.as_text() != Some(key));
    if let Some(value) = value {
        map.push((Value::Text(key.into()), value));
    }
}

fn text(value: &str) -> Value {
    Value::Text(value.into())
}

fn fragment(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn request_json(request: &LoginRequest) -> Json {
    json!({
        "v": request.v,
        "state": hex::encode(*request.state),
        "session_pubkey": hex::encode(&request.session_pubkey),
        "ttl_ms": request.ttl_ms,
        "derivation_origin": request.derivation_origin,
        "next_url": request.next_url,
    })
}

fn vector(name: &str, fields: Json, bytes: &[u8]) -> Json {
    json!({
        "name": name,
        "fields": fields,
        "cbor_hex": hex::encode(bytes),
        "fragment": fragment(bytes),
    })
}

fn rejected(name: &str, bytes: Vec<u8>, reason: &str) -> Json {
    json!({
        "name": name,
        "cbor_hex": hex::encode(&bytes),
        "fragment": fragment(&bytes),
        "reason": reason,
    })
}

/// A structurally valid `DelegationChain.toJSON()` sample without targets,
/// as Internet Identity signs it. The signature is a placeholder: the vectors
/// fix the encoding, not II's signature.
fn delegation_json() -> String {
    let mut user = ED25519_SPKI_PREFIX.to_vec();
    user.extend([0x22; 32]);
    json!({
        "delegations": [{
            "delegation": {
                "expiration": format!("{:x}", 1_760_000_900_000u64 * 1_000_000),
                "pubkey": hex::encode(session_pubkey()),
            },
            "signature": hex::encode([0x33; 64]),
        }],
        "publicKey": hex::encode(user),
    })
    .to_string()
}

fn build() -> Json {
    let base = request();
    let mut requests = vec![vector(
        "dmsg.net",
        request_json(&base),
        &encode_login(&base),
    )];
    let legacy = LoginRequest {
        derivation_origin: "https://panda.fans".into(),
        next_url: "http://[::1]:8/auth/callback".into(),
        ttl_ms: MAX_TTL_MS,
        ..base.clone()
    };
    requests.push(vector(
        "panda.fans over IPv6",
        request_json(&legacy),
        &encode_login(&legacy),
    ));

    let result = LoginResult {
        v: 1,
        state: base.state,
        delegation: delegation_json().into_bytes().into(),
    };
    let results = vec![vector(
        "delegation",
        json!({
            "v": 1,
            "state": hex::encode(*result.state),
            "delegation": delegation_json(),
        }),
        &encode_login(&result),
    )];
    let errors: Vec<Json> = [
        ("user_cancelled", LoginErrorCode::UserCancelled),
        ("ii_error", LoginErrorCode::IiError),
        (
            "derivation_origin_not_allowed",
            LoginErrorCode::DerivationOriginNotAllowed,
        ),
        ("timeout", LoginErrorCode::Timeout),
    ]
    .into_iter()
    .map(|(name, code)| {
        let error = LoginError {
            v: 1,
            state: base.state,
            error: code,
        };
        vector(
            name,
            json!({"v": 1, "state": hex::encode(*error.state), "error": name}),
            &encode_login(&error),
        )
    })
    .collect();

    let edit = |change: &dyn Fn(&mut Map)| {
        let mut map = request_map(&base);
        change(&mut map);
        cbor2::to_canonical_vec(&Value::Map(map)).unwrap()
    };
    let with = |field: &str, value: Value| edit(&|m| set(m, field, Some(value.clone())));
    let mut zero_key = ED25519_SPKI_PREFIX.to_vec();
    zero_key[1] = 0x2b;
    zero_key.extend([0x11; 32]);
    let mut unsorted = request_map(&base);
    unsorted.reverse();
    let rejected_requests = vec![
        rejected(
            "unknown key",
            with("extra", Value::Bool(true)),
            "unknown field",
        ),
        rejected(
            "missing next_url",
            edit(&|m| set(m, "next_url", None)),
            "missing field",
        ),
        rejected(
            "non-canonical map order",
            cbor2::to_vec(&Value::Map(unsorted)).unwrap(),
            "not canonical",
        ),
        rejected("version 2", with("v", Value::from(2u8)), "v"),
        rejected(
            "zero state",
            with("state", Value::Bytes(vec![0; 32])),
            "state",
        ),
        rejected(
            "raw session key",
            with("session_pubkey", Value::Bytes(vec![0x11; 32])),
            "session_pubkey",
        ),
        rejected(
            "malformed SPKI",
            with("session_pubkey", Value::Bytes(zero_key)),
            "session_pubkey",
        ),
        rejected(
            "ttl below one minute",
            with("ttl_ms", Value::from(MIN_TTL_MS - 1)),
            "ttl_ms",
        ),
        rejected(
            "ttl above eight hours",
            with("ttl_ms", Value::from(MAX_TTL_MS + 1)),
            "ttl_ms",
        ),
        rejected(
            "other derivation origin",
            with("derivation_origin", text("https://id.ai")),
            "derivation_origin",
        ),
    ];
    let mut rejected_requests = rejected_requests;
    for (name, url) in [
        ("localhost", "http://localhost:53817/auth/callback"),
        ("https", "https://127.0.0.1:53817/auth/callback"),
        ("remote host", "http://203.0.113.7:53817/auth/callback"),
        ("other path", "http://127.0.0.1:53817/auth"),
        ("query", "http://127.0.0.1:53817/auth/callback?x=1"),
        ("fragment", "http://127.0.0.1:53817/auth/callback#x"),
        ("no port", "http://127.0.0.1/auth/callback"),
        ("port zero", "http://127.0.0.1:0/auth/callback"),
        ("leading zero port", "http://127.0.0.1:080/auth/callback"),
        ("port above 65535", "http://127.0.0.1:65536/auth/callback"),
        ("userinfo", "http://user@127.0.0.1:53817/auth/callback"),
    ] {
        rejected_requests.push(rejected(
            &format!("next_url {name}"),
            with("next_url", text(url)),
            "next_url",
        ));
    }

    let mut unknown = request_map(&base);
    unknown.clear();
    unknown.extend([
        (text("v"), Value::from(1u8)),
        (text("state"), Value::Bytes(vec![0x5a; 32])),
        (text("error"), text("denied")),
    ]);
    let rejected_callbacks = vec![
        rejected(
            "unknown error code",
            cbor2::to_canonical_vec(&Value::Map(unknown)).unwrap(),
            "unknown error",
        ),
        rejected(
            "result version 2",
            encode_login(&LoginResult {
                v: 2,
                ..result.clone()
            }),
            "v",
        ),
        rejected(
            "result and error",
            {
                let Value::Map(mut map) = cbor2::from_slice(&encode_login(&result)).unwrap() else {
                    unreachable!()
                };
                map.push((text("error"), text("timeout")));
                cbor2::to_canonical_vec(&Value::Map(map)).unwrap()
            },
            "unknown field",
        ),
    ];

    json!({
        "schema": 1,
        "test_only": true,
        "generator": "src/dmsg_protocol/src/login_tests.rs",
        "description": "dmsg-login/1 messages: canonical CBOR and the base64url fragment. `fields` restates each message in JSON (bytes as hex). Readers must decode `fragment`, require canonical bytes and check the fields; every rejected item must be refused before contacting Internet Identity or accepting a callback. The delegation signature is a placeholder.",
        "requests": requests,
        "results": results,
        "errors": errors,
        "rejected_requests": rejected_requests,
        "rejected_callbacks": rejected_callbacks,
    })
}

#[test]
fn login_vectors_match() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(VECTORS);
    let mut text = serde_json::to_string_pretty(&build()).unwrap();
    text.push('\n');
    if std::env::var_os("DMSG_WRITE_CLIENT_VECTORS").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        text,
        "rerun with DMSG_WRITE_CLIENT_VECTORS=1"
    );
}

#[test]
fn accepts_and_rejects_vectors() {
    let vectors = build();
    let unhex = |v: &Json| hex::decode(v["cbor_hex"].as_str().unwrap()).unwrap();
    for item in vectors["requests"].as_array().unwrap() {
        let request = decode_request(&unhex(item)).unwrap();
        assert_eq!(request_json(&request), item["fields"]);
        assert_eq!(encode_login(&request), unhex(item));
    }
    for item in vectors["rejected_requests"].as_array().unwrap() {
        assert!(decode_request(&unhex(item)).is_err(), "{}", item["name"]);
    }
    for item in vectors["results"]
        .as_array()
        .unwrap()
        .iter()
        .chain(vectors["errors"].as_array().unwrap())
    {
        let callback = decode_callback(&unhex(item)).unwrap();
        assert_eq!(encode_login(&callback), unhex(item));
    }
    assert!(matches!(
        decode_callback(&unhex(&vectors["results"][0])).unwrap(),
        LoginCallback::Result(_)
    ));
    for item in vectors["rejected_callbacks"].as_array().unwrap() {
        assert!(decode_callback(&unhex(item)).is_err(), "{}", item["name"]);
    }
}

#[test]
fn callback_ports() {
    assert_eq!(
        callback_port("http://127.0.0.1:1/auth/callback").unwrap(),
        1
    );
    assert_eq!(
        callback_port("http://[::1]:65535/auth/callback").unwrap(),
        65535
    );
}
