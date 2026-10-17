//! Readers for the language-independent vectors in `tests/vectors/client`.

use cbor2::{Value, value::Integer};
use serde_json::Value as Json;

/// Parse a vector file by name.
pub(crate) fn load(name: &str) -> Json {
    let path = format!(
        "{}/../../tests/vectors/client/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap()
}

/// The CBOR item of a typed JSON value (`{"uint": "1"}`, `{"map": [...]}`, ...).
pub(crate) fn value(json: &Json) -> Value {
    let (kind, inner) = json.as_object().unwrap().iter().next().unwrap();
    match kind.as_str() {
        "uint" => Value::Integer(inner.as_str().unwrap().parse::<u64>().unwrap().into()),
        "int" => Value::Integer(
            Integer::try_from(inner.as_str().unwrap().parse::<i128>().unwrap()).unwrap(),
        ),
        "text" => Value::Text(inner.as_str().unwrap().into()),
        "bytes" => Value::Bytes(bytes(inner)),
        "bool" => Value::Bool(inner.as_bool().unwrap()),
        "null" => Value::Null,
        "array" => Value::Array(inner.as_array().unwrap().iter().map(value).collect()),
        "map" => Value::Map(
            inner
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| (value(&pair[0]), value(&pair[1])))
                .collect(),
        ),
        "tag" => Value::Tag(inner.as_u64().unwrap(), Box::new(value(&json["value"]))),
        other => panic!("unknown typed value {other}"),
    }
}

/// Bytes of a hex string field.
pub(crate) fn bytes(json: &Json) -> Vec<u8> {
    hex::decode(json.as_str().unwrap()).unwrap()
}

/// A 32-byte key of a hex string field.
pub(crate) fn key(json: &Json) -> [u8; 32] {
    bytes(json).try_into().unwrap()
}

/// Text of a string field.
pub(crate) fn text(json: &Json) -> &str {
    json.as_str().unwrap()
}

/// A field of a CBOR map with text keys.
pub(crate) fn field<'a>(map: &'a Value, name: &str) -> &'a Value {
    map.as_map()
        .unwrap()
        .iter()
        .find(|(k, _)| k.as_text() == Some(name))
        .map(|(_, v)| v)
        .unwrap_or_else(|| panic!("missing field {name}"))
}
