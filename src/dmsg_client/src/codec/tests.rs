use super::*;
use crate::vectors::{bytes, load, text, value};
use dmsg_types::AccountId;
use std::str::FromStr;

#[test]
fn canonical_cbor_matches_vectors() {
    let vectors = load("primitives-v1.json");
    for case in vectors["cbor"].as_array().unwrap() {
        let encoded = bytes(&case["hex"]);
        assert_eq!(
            canonical(&value(&case["value"])),
            encoded,
            "{}",
            case["name"]
        );
        let decoded = decode_value(&encoded, MAX_RECORD).unwrap();
        assert_eq!(canonical(&decoded), encoded, "{}", case["name"]);
    }
}

#[test]
fn rejects_non_canonical_and_out_of_model_cbor() {
    let vectors = load("primitives-v1.json");
    for case in vectors["cbor_reject"].as_array().unwrap() {
        let input = hex::decode(text(&case["hex"])).unwrap();
        assert!(
            decode_value(&input, MAX_RECORD).is_err(),
            "{}",
            case["name"]
        );
    }
    let mut deepest = vec![0x81; 24];
    deepest.push(0x01);
    assert!(decode_value(&deepest, MAX_RECORD).is_ok());
    assert_eq!(decode_value(&[0x01], 0).unwrap_err(), Error::QuotaExceeded);
}

#[test]
fn bounds_items_and_containers() {
    // 25,000 entries of [0] are 50,001 nodes; 25,001 entries exceed one container.
    let mut nodes = vec![0x99, 0x61, 0xa8];
    nodes.extend([0x81, 0x00].repeat(25_000));
    assert!(decode_value(&nodes, usize::MAX).is_err());
    let mut items = vec![0x99, 0x61, 0xa9];
    items.extend([0x00].repeat(25_001));
    assert_eq!(
        decode_value(&items, usize::MAX).unwrap_err(),
        Error::QuotaExceeded
    );
    let mut within = vec![0x99, 0x61, 0xa8];
    within.extend([0x00].repeat(25_000));
    assert!(decode_value(&within, usize::MAX).is_ok());
}

#[test]
fn typed_decoding_rejects_unknown_fields() {
    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
    struct Pair {
        a: u64,
    }
    let pair = canonical(&std::collections::BTreeMap::from([("a", 1u64)]));
    assert_eq!(decode::<Pair>(&pair, MAX_RECORD).unwrap(), Pair { a: 1 });
    let extra = canonical(&std::collections::BTreeMap::from([("a", 1u64), ("b", 2)]));
    assert_eq!(
        decode::<Pair>(&extra, MAX_RECORD).unwrap_err(),
        Error::IntegrityFailed
    );
}

#[test]
fn typed_decoding_fills_fields_an_older_record_lacks() {
    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
    struct Record {
        a: u64,
        #[serde(default)]
        b: bool,
        c: Option<u64>,
    }
    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
    struct Records {
        items: Vec<Record>,
    }
    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
    struct Trimmed {
        a: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        c: Option<u64>,
    }
    let a = |n: u64| std::collections::BTreeMap::from([("a", n)]);
    let older = Record {
        a: 1,
        b: false,
        c: None,
    };
    assert_eq!(
        decode::<Record>(&canonical(&a(1)), MAX_RECORD).unwrap(),
        older
    );
    let nested = canonical(&std::collections::BTreeMap::from([("items", vec![a(1)])]));
    assert_eq!(
        decode::<Records>(&nested, MAX_RECORD).unwrap(),
        Records { items: vec![older] }
    );
    // Still canonical: `{"a": 1}` with a two-byte 1.
    assert_eq!(
        decode::<Record>(&[0xa1, 0x61, 0x61, 0x18, 0x01], MAX_RECORD).unwrap_err(),
        Error::IntegrityFailed
    );
    // An entry the type would drop is not kept.
    let null = canonical(&std::collections::BTreeMap::from([
        ("a", Some(1u64)),
        ("c", None),
    ]));
    assert_eq!(
        decode::<Trimmed>(&null, MAX_RECORD).unwrap_err(),
        Error::IntegrityFailed
    );
}

#[test]
fn base64url_is_canonical() {
    let vectors = load("primitives-v1.json");
    for case in vectors["base64url"]["accept"].as_array().unwrap() {
        let expected = hex::decode(text(&case["hex"])).unwrap();
        assert_eq!(unb64(text(&case["text"])).unwrap(), expected);
        assert_eq!(b64(&expected), text(&case["text"]));
    }
    for case in vectors["base64url"]["reject"].as_array().unwrap() {
        assert!(unb64(text(case)).is_err(), "{case}");
    }
}

#[test]
fn xid_text_round_trips() {
    let vectors = load("primitives-v1.json");
    for case in vectors["xid"].as_array().unwrap() {
        let id = AccountId::from_str(text(&case["text"])).unwrap();
        assert_eq!(id.to_string(), text(&case["text"]));
        assert_eq!(
            canonical(&id),
            canonical(&serde_bytes::Bytes::new(&bytes(&case["hex"])))
        );
    }
}

#[test]
fn encodes_public_rust_protocol_vectors() {
    let path = format!(
        "{}/../dmsg_types/tests/protocol_vectors.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let vectors: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for case in vectors.as_array().unwrap() {
        let encoded = canonical(&value(&case["value"]));
        assert_eq!(
            hex::encode(&encoded),
            text(&case["cbor_hex"]),
            "{}",
            case["name"]
        );
        assert_eq!(sha256(&encoded).to_vec(), bytes(&case["sha256_hex"]));
        let key = bytes(&case["ed25519_public_hex"]).try_into().unwrap();
        dmsg_protocol::verify(
            &dmsg_types::Hash::new(key),
            &bytes(&case["sha256_hex"]),
            &bytes(&case["signature_over_sha256_hex"]),
        )
        .unwrap();
    }
}
