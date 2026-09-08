//! Deterministic public interoperability fixtures; every private key is a
//! fixed test seed. Run through scripts/verify-dmsg-vectors.mjs independently.
use candid::Principal;
use dmsg_types::{cose::*, *};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{json, Value as Json};

fn tree(v: cbor2::Value) -> Json {
    use cbor2::Value::*;
    match v {
        Integer(n) => json!({"uint":u128::try_from(n).unwrap().to_string()}),
        Text(s) => json!({"text":s}),
        Bytes(b) => json!({"bytes":hex::encode(b)}),
        Array(a) => json!({"array":a.into_iter().map(tree).collect::<Vec<_>>()}),
        Map(m) => {
            json!({"map":m.into_iter().map(|(k,v)|vec![tree(k),tree(v)]).collect::<Vec<_>>()})
        }
        Null => Json::Null,
        Bool(b) => json!(b),
        Tag(tag, v) => json!({"tag":tag,"value":tree(*v)}),
        _ => panic!("protocol forbids floats, negative and simple values"),
    }
}
fn vector(name: &str, bytes: Vec<u8>) -> Json {
    let hash = sha256(&bytes);
    let key = SigningKey::from_bytes(&[7; 32]);
    json!({"name":name,"value":tree(cbor2::from_slice(&bytes).unwrap()),"cbor_hex":hex::encode(bytes),"sha256_hex":hex::encode(hash),"ed25519_public_hex":hex::encode(key.verifying_key().to_bytes()),"signature_over_sha256_hex":hex::encode(key.sign(&hash).to_bytes())})
}
fn main() {
    let payload = FormalPayload {
        schema: 1,
        subject: [1; 32],
        request_id: execution_request_id([1; 32], 0, [2; 32], 0),
        origin: "https://example.com".into(),
        audience: "release".into(),
        expires_at: 1_800_000_000_000_000_000,
        body: FormalBody::FileAttestation {
            sha256: [3; 32],
            size: 12345,
            version: [4; 32],
            project: "dmsg".into(),
        },
    };
    let values = vec![
        vector("formal_file_v1", canonical(&payload)),
        vector(
            "principal_and_generation",
            canonical(&(Principal::from_slice(&[0, 1, 1]), [5u8; 32], 42u64)),
        ),
        vector("amount_over_u64", canonical(&("dmsg/amount/v1", u128::MAX))),
        vector(
            "map_key_order",
            canonical(&std::collections::BTreeMap::from([
                ("aa", 1u64),
                ("b", 2u64),
            ])),
        ),
        vector("root_input_v1", canonical(&([1u8; 32], 2u64))),
        vector(
            "execution_request_id_v1",
            canonical(&(
                1u8,
                "dmsg/execution-request/v1",
                ([1u8; 32], 0u64, [2u8; 32], 0u64),
            )),
        ),
    ];
    println!("{}", serde_json::to_string_pretty(&values).unwrap());
}
