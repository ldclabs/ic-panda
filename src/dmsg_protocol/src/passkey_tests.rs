use super::*;
use p256::ecdsa::{SigningKey, signature::Signer};

fn key() -> SigningKey {
    SigningKey::from_slice(&[7; 32]).unwrap()
}

fn client(challenge: &Hash, register: bool) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "type": if register {"webauthn.create"} else {"webauthn.get"},
        "challenge": URL_SAFE_NO_PAD.encode(challenge.as_slice()),
        "origin": PASSKEY_ORIGIN,
        "crossOrigin": false
    }))
    .unwrap()
}

fn auth_data(flags: u8) -> Vec<u8> {
    [sha256(PASSKEY_RP.as_bytes()).as_slice(), &[flags], &[0; 4]].concat()
}

fn registration(challenge: &Hash) -> PasskeyResponse {
    let public = key().verifying_key().to_sec1_point(false);
    let id = vec![1, 2, 3];
    let cose = Value::Map(vec![
        (integer(1), integer(2)),
        (integer(3), integer(-7)),
        (integer(-1), integer(1)),
        (integer(-2), Value::Bytes(public.as_bytes()[1..33].to_vec())),
        (integer(-3), Value::Bytes(public.as_bytes()[33..].to_vec())),
    ]);
    let data = [
        auth_data(0x5d),
        vec![0; 16],
        vec![0, 3],
        id.clone(),
        cbor2::to_vec(&cose).unwrap(),
    ]
    .concat();
    let attestation = Value::Map(vec![
        (Value::Text("fmt".into()), Value::Text("none".into())),
        (Value::Text("attStmt".into()), Value::Map(vec![])),
        (Value::Text("authData".into()), Value::Bytes(data)),
    ]);
    PasskeyResponse {
        credential_id: id.into(),
        client_data_json: client(challenge, true).into(),
        authenticator_data: cbor2::to_vec(&attestation).unwrap().into(),
        signature: vec![].into(),
    }
}

fn assertion(challenge: &Hash) -> PasskeyResponse {
    let client_data_json = client(challenge, false);
    let data = auth_data(0x1d);
    let message = [data.as_slice(), sha256(&client_data_json).as_slice()].concat();
    let signature: Signature = key().sign(&message);
    PasskeyResponse {
        credential_id: vec![1, 2, 3].into(),
        client_data_json: client_data_json.into(),
        authenticator_data: data.into(),
        signature: signature.to_der().as_bytes().to_vec().into(),
    }
}

fn with_origin(mut response: PasskeyResponse, origin: &str) -> PasskeyResponse {
    let mut data: serde_json::Value = serde_json::from_slice(&response.client_data_json).unwrap();
    data["origin"] = serde_json::json!(origin);
    response.client_data_json = serde_json::to_vec(&data).unwrap().into();
    if !response.signature.is_empty() {
        let message = [
            response.authenticator_data.as_ref(),
            sha256(&response.client_data_json).as_slice(),
        ]
        .concat();
        let signature: Signature = key().sign(&message);
        response.signature = signature.to_der().as_bytes().to_vec().into();
    }
    response
}

#[test]
fn android_certificates_are_exact_and_do_not_relax_webauthn() {
    let challenge = Hash::new([9; 32]);
    let certificate = Hash::new([0x35; 32]);
    let policy = [certificate];
    let origin = android_origin(&certificate);
    let registration = with_origin(registration(&challenge), &origin);
    assert!(verify_registration(&registration, &challenge).is_err());
    let public = verify_registration_with_android(&registration, &challenge, &policy).unwrap();
    let proof = with_origin(assertion(&challenge), &origin);
    verify_assertion_with_android(&proof, &challenge, &public, &policy).unwrap();
    assert!(verify_assertion(&proof, &challenge, &public).is_err());
    assert!(verify_assertion_with_android(&proof, &Hash::new([8; 32]), &public, &policy).is_err());
    for invalid in [
        android_origin(&Hash::new([0x36; 32])),
        format!("{origin}="),
        format!("{origin}/"),
        "android:apk-key-hash:*".into(),
        "android:com.example.dmsg".into(),
        "https://dmsg.net.evil.example".into(),
    ] {
        let changed = with_origin(proof.clone(), &invalid);
        assert!(verify_assertion_with_android(&changed, &challenge, &public, &policy).is_err());
    }
    for position in [0, 32] {
        let mut changed = proof.clone();
        changed.authenticator_data[position] ^= 4;
        let changed = with_origin(changed, &origin); // A valid signature still cannot bypass RP/UV.
        assert!(verify_assertion_with_android(&changed, &challenge, &public, &policy).is_err());
    }
    let mut rewritten = proof.clone();
    rewritten.client_data_json = client(&challenge, false).into();
    assert!(verify_assertion_with_android(&rewritten, &challenge, &public, &policy).is_err());
    verify_assertion_with_android(&assertion(&challenge), &challenge, &public, &policy).unwrap();

    let vector = serde_json::json!({
        "protocol": "dmsg-passkey/1", "generator": "src/dmsg_protocol/src/passkey_tests.rs",
        "certificate_sha256_hex": hex::encode(certificate.as_slice()), "origin": origin,
        "challenge_hex": hex::encode(challenge.as_slice()), "public_key_hex": hex::encode(public),
        "registration_cbor": URL_SAFE_NO_PAD.encode(crate::canonical(&registration)),
        "assertion_cbor": URL_SAFE_NO_PAD.encode(crate::canonical(&proof)),
        "note": "Synthetic certificate. Never admit this hash in production."
    });
    let text = serde_json::to_string_pretty(&vector).unwrap() + "\n";
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/vectors/client/passkey-android-v1.json");
    if std::env::var_os("DMSG_WRITE_CLIENT_VECTORS").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), text);
}

#[test]
fn android_policy_is_bounded_and_disabled_by_default() {
    assert!(validate_android_signers(&[]).is_ok());
    assert!(!allowed_origin(&android_origin(&Hash::new([1; 32])), &[]));
    assert!(validate_android_signers(&[Hash::new([0; 32])]).is_err());
    assert!(validate_android_signers(&[Hash::new([1; 32]); 2]).is_err());
    let hashes: Vec<_> = (1..=9).map(|n| Hash::new([n; 32])).collect();
    assert!(validate_android_signers(&hashes[..8]).is_ok());
    assert!(validate_android_signers(&hashes).is_err());
}

#[test]
fn registration_and_assertion_preserve_the_signed_bytes() {
    for action in [
        AuthAction::Register,
        AuthAction::Login,
        AuthAction::LogoutAll,
    ] {
        let bytes = candid::encode_one(action).unwrap();
        assert_eq!(candid::decode_one::<AuthAction>(&bytes).unwrap(), action);
    }
    let challenge = Hash::new([9; 32]);
    let public = verify_registration(&registration(&challenge), &challenge).unwrap();
    assert_eq!(
        public,
        key().verifying_key().to_sec1_point(false).as_bytes()
    );
    verify_assertion(&assertion(&challenge), &challenge, &public).unwrap();
    let der = hardware_der(&public).unwrap();
    assert_eq!(hardware_public(&der).unwrap(), public);
    let mut bad = der.clone();
    bad[20] ^= 1;
    assert!(hardware_public(&bad).is_err());
}

#[test]
fn another_challenge_origin_or_operation_is_rejected() {
    let challenge = Hash::new([9; 32]);
    let public = key().verifying_key().to_sec1_point(false);
    let response = assertion(&challenge);
    assert!(verify_assertion(&response, &Hash::new([8; 32]), public.as_bytes()).is_err());
    for (field, value) in [
        ("origin", serde_json::json!("https://connect.dmsg.net")),
        ("crossOrigin", serde_json::json!(true)),
        ("type", serde_json::json!("webauthn.create")),
        ("topOrigin", serde_json::json!(PASSKEY_ORIGIN)),
    ] {
        let mut changed = response.clone();
        let mut json: serde_json::Value =
            serde_json::from_slice(&changed.client_data_json).unwrap();
        json[field] = value;
        changed.client_data_json = serde_json::to_vec(&json).unwrap().into();
        assert!(verify_assertion(&changed, &challenge, public.as_bytes()).is_err());
    }
}

#[test]
fn uv_rp_signature_and_registration_id_are_checked() {
    let challenge = Hash::new([9; 32]);
    let public = key().verifying_key().to_sec1_point(false);
    for index in [0, 32] {
        let mut response = assertion(&challenge);
        response.authenticator_data[index] ^= 4;
        assert!(verify_assertion(&response, &challenge, public.as_bytes()).is_err());
    }
    let mut response = assertion(&challenge);
    response.signature[5] ^= 1;
    assert!(verify_assertion(&response, &challenge, public.as_bytes()).is_err());
    let mut response = registration(&challenge);
    response.credential_id[0] ^= 1;
    assert!(verify_registration(&response, &challenge).is_err());
    assert!(credential_hash(&vec![0; MAX_CREDENTIAL_ID + 1]).is_err());
}

#[test]
fn passkey_transport_vectors_match() {
    let request = PasskeyRequest {
        protocol: "dmsg-passkey/1".into(),
        state: Hash::new([1; 32]),
        challenge: AuthChallenge {
            challenge: Hash::new([9; 32]),
            expires_at: 1_800_000_300_000,
            action: AuthAction::Login,
        },
        next_url: "http://127.0.0.1:2345/auth/callback".into(),
        credential_id: None,
    };
    let callback = PasskeyCallback {
        protocol: request.protocol.clone(),
        state: request.state,
        response: Some(assertion(&request.challenge.challenge)),
        error: None,
    };
    let request_bytes = crate::canonical(&request);
    let callback_bytes = crate::canonical(&callback);
    let vector = serde_json::json!({ "protocol": "dmsg-passkey/1", "now_ms": 1_800_000_000_000u64,
        "generator": "src/dmsg_protocol/src/passkey_tests.rs",
        "request": URL_SAFE_NO_PAD.encode(&request_bytes), "callback": URL_SAFE_NO_PAD.encode(&callback_bytes),
        "public_key_hex": hex::encode(key().verifying_key().to_sec1_point(false).as_bytes()) });
    let text = serde_json::to_string_pretty(&vector).unwrap() + "\n";
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/vectors/client/passkey-v1.json");
    if std::env::var_os("DMSG_WRITE_CLIENT_VECTORS").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    assert_eq!(std::fs::read_to_string(path).unwrap(), text);
    let decoded: PasskeyCallback = cbor2::from_slice(&callback_bytes).unwrap();
    verify_assertion(
        &decoded.response.unwrap(),
        &request.challenge.challenge,
        key().verifying_key().to_sec1_point(false).as_bytes(),
    )
    .unwrap();
}
