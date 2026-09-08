use candid::Principal;
use dmsg_types::{cose::*, handle::*, *};

#[test]
fn production_requires_a_fixed_home_key_and_fingerprint() {
    let id = Principal::from_slice(&[1, 1]);
    let mut c = CoseInit {
        environment: Environment::Production,
        executing_canister: id,
        initial_home_user: Principal::from_slice(&[2, 1]),
        derivation_version: 1,
        masters: vec![MasterKey {
            algorithm: Algorithm::Ed25519,
            key_name: "test_key_1".into(),
            expected_fingerprint: [1; 32],
        }],
        daily_executions: 10,
        daily_cycles: 100,
    };
    assert!(c.validate(id).is_err());
    c.masters[0].key_name = "dfx_test_key".into();
    assert!(c.validate(id).is_err());
    c.masters[0].key_name = "key_1".into();
    c.masters[0].expected_fingerprint = [0; 32];
    assert!(c.validate(id).is_err());
    c.masters[0].expected_fingerprint = [1; 32];
    assert!(c.validate(id).is_ok());
    assert!(c.validate(c.initial_home_user).is_err());
    c.environment = Environment::Local;
    c.masters[0].key_name = "dfx_test_key".into();
    c.masters[0].expected_fingerprint = [0; 32];
    assert!(c.validate(id).is_ok());
}
#[test]
fn unknown_schema_assets_provider_and_noncanonical_payloads_fail_closed() {
    let request = KeyRequest {
        purpose: KeyPurpose::Statement,
        algorithm: Algorithm::Ed25519,
        generation: 1,
        provider: None,
    };
    let mut p = FormalPayload {
        schema: 1,
        subject: [1; 32],
        request_id: [2; 32],
        origin: "https://example.com".into(),
        audience: "app".into(),
        expires_at: 100,
        body: FormalBody::Statement {
            text: "release statement".into(),
        },
    };
    let check =
        |p: &FormalPayload| validate_payload(&canonical(p), [1; 32], [2; 32], &request, 100);
    assert!(check(&p).is_ok());
    p.schema = 2;
    assert_eq!(check(&p), Err(Error::IntegrityFailed));
    p.schema = 1;
    p.origin = "https://example.com@evil.com".into();
    assert!(check(&p).is_err());
    p.origin = format!("chrome-extension://{}", "a".repeat(32));
    assert!(check(&p).is_ok());
    let mut value: cbor2::Value = cbor2::from_slice(&canonical(&p)).unwrap();
    if let cbor2::Value::Map(ref mut entries) = value {
        entries.push((
            cbor2::Value::Text("asset_permission".into()),
            cbor2::Value::Bool(true),
        ));
    }
    assert!(validate_payload(&canonical(&value), [1; 32], [2; 32], &request, 100).is_err());
    let mut external = request.clone();
    external.purpose = KeyPurpose::ProviderController;
    assert_eq!(external.validate(), Err(Error::UnsupportedProtocol));
    external = request;
    external.provider = Some("arbitrary-app".into());
    assert_eq!(external.validate(), Err(Error::UnsupportedProtocol));
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
    let id = execution_request_id([1; 32], 2, [3; 32], 4);
    assert_ne!(id, execution_request_id([1; 32], 2, [3; 32], 5));
    assert_ne!(id, execution_request_id([1; 32], 3, [3; 32], 4));
    assert_ne!(id, execution_request_id([1; 32], 2, [4; 32], 4));
    assert_ne!(id, execution_request_id([2; 32], 2, [3; 32], 4));
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
