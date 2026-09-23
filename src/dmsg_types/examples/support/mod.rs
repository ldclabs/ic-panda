//! Shared encoding of deterministic interoperability fixtures.
use dmsg_protocol::sha256;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::{json, Value as Json};

fn tree(v: cbor2::Value) -> Json {
    use cbor2::Value::*;
    match v {
        Integer(n) => match u128::try_from(n) {
            Ok(n) => json!({"uint":n.to_string()}),
            Err(_) => json!({"int":i128::from(n).to_string()}),
        },
        Text(s) => json!({"text":s}),
        Bytes(b) => json!({"bytes":hex::encode(b)}),
        Array(a) => json!({"array":a.into_iter().map(tree).collect::<Vec<_>>()}),
        Map(m) => {
            json!({"map":m.into_iter().map(|(k,v)|vec![tree(k),tree(v)]).collect::<Vec<_>>()})
        }
        Null => Json::Null,
        Bool(b) => json!(b),
        Tag(tag, v) => json!({"tag":tag,"value":tree(*v)}),
        _ => panic!("unsupported protocol fixture value"),
    }
}

pub fn vector(name: &str, bytes: Vec<u8>) -> Json {
    let hash = sha256(&bytes);
    let key = SigningKey::from_bytes(&[7; 32]);
    json!({"name":name,"value":tree(cbor2::from_slice(&bytes).unwrap()),"cbor_hex":hex::encode(bytes),"sha256_hex":hex::encode(hash.as_slice()),"ed25519_public_hex":hex::encode(key.verifying_key().to_bytes()),"signature_over_sha256_hex":hex::encode(key.sign(hash.as_slice()).to_bytes())})
}
