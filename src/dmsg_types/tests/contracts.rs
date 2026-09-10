use candid::Principal;
use dmsg_protocol::*;
use dmsg_types::{cose::*, *};

#[test]
fn fixed_bytes_keep_their_encoding_in_every_container_and_candid() {
    use cbor2::Value;
    use std::collections::BTreeMap;
    let id = Hash::new([0x42; 32]);
    let bytes = Value::Bytes(vec![0x42; 32]);
    let value: Value = cbor2::from_slice(&canonical(&(
        id,
        Some(id),
        vec![id],
        BTreeMap::from([(id, id)]),
    )))
    .unwrap();
    assert_eq!(
        value,
        Value::Array(vec![
            bytes.clone(),
            bytes.clone(),
            Value::Array(vec![bytes.clone()]),
            Value::Map(vec![(bytes.clone(), bytes)]),
        ])
    );
    assert_eq!(&canonical(&id)[..2], &[0x58, 0x20]);
    assert_eq!(decode_canonical::<Hash>(&canonical(&id)), Ok(id));
    // Serde's default array representation is not the protocol representation.
    assert!(decode_canonical::<Hash>(&canonical(&id.into_array())).is_err());
    for size in [0, 31, 33] {
        let bytes = serde_bytes::ByteBuf::from(vec![0x42; size]);
        assert!(decode_canonical::<Hash>(&canonical(&bytes)).is_err());
        assert!(candid::decode_one::<Hash>(&candid::encode_one(bytes).unwrap()).is_err());
    }
    let encoded = candid::encode_one(id).unwrap();
    assert_eq!(candid::decode_one::<Hash>(&encoded).unwrap(), id);
    assert_eq!(
        candid::decode_one::<Vec<u8>>(&encoded).unwrap(),
        id.to_vec()
    );
}

#[test]
fn protocol_accounts_encode_subaccounts_as_bytes() {
    use icrc_ledger_types::icrc1::account::Account;
    let payer = Account {
        owner: Principal::from_slice(&[1]),
        subaccount: Some([2; 32]),
    };
    let encoded = canonical(&account::account_cbor::value(&payer));
    let value: cbor2::Value = cbor2::from_slice(&encoded).unwrap();
    let cbor2::Value::Map(fields) = value else {
        panic!("account map")
    };
    assert!(fields.contains(&(
        cbor2::Value::Text("subaccount".into()),
        cbor2::Value::Bytes(vec![2; 32])
    )));
    #[derive(serde::Serialize, serde::Deserialize, candid::CandidType)]
    struct AccountField {
        #[serde(with = "account::account_cbor")]
        payer: Account,
    }
    let field = AccountField { payer };
    assert_eq!(
        decode_canonical::<AccountField>(&canonical(&field))
            .unwrap()
            .payer,
        payer
    );
    assert_eq!(
        candid::decode_one::<AccountField>(&candid::encode_one(field).unwrap())
            .unwrap()
            .payer,
        payer
    );
    assert_ne!(
        charge_terms_digest(payer.owner, &payer, 100, 10),
        digest(
            "dmsg/handle-charge/v1",
            &(payer.owner, payer, 100u128, 10u128)
        )
    );
}

#[test]
fn millisecond_deadlines_and_ledger_conversion_have_explicit_boundaries() {
    assert_eq!(SECOND, 1_000);
    assert_eq!(MINUTE, 60_000);
    assert_eq!(DAY, 86_400_000);
    let deadline = 1_800_000_000_000;
    let nanos = millis_to_nanos(deadline).unwrap();
    assert_eq!(nanos, 1_800_000_000_000_000_000);
    assert_eq!(nanos_to_millis(nanos - 1), deadline - 1);
    assert_eq!(nanos_to_millis(nanos), deadline);
    assert_eq!(nanos_to_millis(nanos + 999_999), deadline);
    assert!(expiry(deadline - 1, deadline, MINUTE).is_ok());
    assert_eq!(expiry(deadline, deadline, MINUTE), Err(Error::Expired));
    assert!(millis_to_nanos(u64::MAX).is_err());
}

#[test]
fn production_requires_a_fixed_home_key_and_fingerprint() {
    let id = Principal::from_slice(&[1, 1]);
    let mut c = CoseInit {
        issuer_namespace: "https://dmsg.test/u/".into(),
        environment: Environment::Production,
        executing_canister: id,
        initial_home_user: Principal::from_slice(&[2, 1]),
        derivation_version: 2,
        masters: vec![MasterKey {
            algorithm: Algorithm::Ed25519,
            key_name: "test_key_1".into(),
            expected_fingerprint: Hash::new([1; 32]),
        }],
        daily_executions: 10,
        daily_cycles: 100,
    };
    assert!(c.validate(id).is_err());
    c.masters[0].key_name = "dfx_test_key".into();
    assert!(c.validate(id).is_err());
    c.masters[0].key_name = "key_1".into();
    c.masters[0].expected_fingerprint = Hash::new([0; 32]);
    assert!(c.validate(id).is_err());
    c.masters[0].expected_fingerprint = Hash::new([1; 32]);
    assert!(c.validate(id).is_ok());
    assert!(c.validate(c.initial_home_user).is_err());
    c.environment = Environment::Local;
    c.masters[0].key_name = "dfx_test_key".into();
    c.masters[0].expected_fingerprint = Hash::new([0; 32]);
    assert!(c.validate(id).is_ok());
}

#[test]
fn legacy_normalization_and_panda_prices_are_preserved() {
    assert_eq!(normalize_handle("Alice_01").unwrap(), "alice_01");
    for name in [
        "",
        "_hidden",
        "a-b",
        "a.b",
        "名字",
        " alice",
        "a/b",
        "123456789012345678901",
    ] {
        assert!(normalize_handle(name).is_err(), "{name}");
    }
    assert_eq!(price("a"), 100_000_000_000_000);
    assert_eq!(price("ab"), 20_000_000_000_000);
    assert_eq!(price("abc"), 5_000_000_000_000);
    assert_eq!(price("abcde"), 2_000_000_000_000);
    assert_eq!(price("abcdefg"), 500_000_000_000);
}

#[test]
fn execution_ids_bind_the_device_epoch_and_sequence() {
    let id = execution_request_id(&AccountId([1; 12]), 2, Hash::new([3; 32]), 4);
    assert_ne!(
        id,
        execution_request_id(&AccountId([1; 12]), 2, Hash::new([3; 32]), 5)
    );
    assert_ne!(
        id,
        execution_request_id(&AccountId([1; 12]), 3, Hash::new([3; 32]), 4)
    );
    assert_ne!(
        id,
        execution_request_id(&AccountId([1; 12]), 2, Hash::new([4; 32]), 4)
    );
    assert_ne!(
        id,
        execution_request_id(&AccountId([2; 12]), 2, Hash::new([3; 32]), 4)
    );
}

#[test]
fn transport_keys_must_be_nonidentity_subgroup_points() {
    assert!(validate_transport_key(&[1; 48]).is_err());
    assert!(validate_transport_key(&[0; 48]).is_err());
    let identity = ic_bls12_381::G1Affine::identity().to_compressed();
    assert!(validate_transport_key(&identity).is_err());
    let valid = ic_bls12_381::G1Affine::generator().to_compressed();
    assert!(validate_transport_key(&valid).is_ok());
    assert!(validate_transport_key(&valid[..47]).is_err());
}

#[test]
fn xid_binary_and_canonical_text_have_distinct_wire_representations() {
    let id = AccountId([1; 12]);
    let text = id.to_string();
    assert_eq!(text.len(), 20);
    assert_eq!(text.parse::<AccountId>().unwrap(), id);
    assert_eq!(serde_json::to_string(&id).unwrap(), format!("\"{text}\""));
    assert_eq!(
        serde_json::from_str::<AccountId>(&format!("\"{text}\"")).unwrap(),
        id
    );
    assert_eq!(canonical(&id), [vec![0x4c], vec![1; 12]].concat());
    assert_eq!(decode_canonical::<AccountId>(&canonical(&id)).unwrap(), id);
    assert_eq!(
        candid::decode_one::<AccountId>(&candid::encode_one(&id).unwrap()).unwrap(),
        id
    );
    for len in [0, 11, 13, 29, 32] {
        let value = serde_bytes::ByteBuf::from(vec![1; len]);
        assert!(candid::decode_one::<AccountId>(&candid::encode_one(&value).unwrap()).is_err());
        assert!(decode_canonical::<AccountId>(&canonical(&value)).is_err());
    }
    assert!("040g2081040g2081040h".parse::<AccountId>().is_err());
}
