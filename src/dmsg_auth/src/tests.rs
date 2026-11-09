use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use cbor2::Value;
use p256::ecdsa::{Signature, SigningKey, signature::Signer};

fn setup() {
    IDENTITIES.with_borrow_mut(|m| m.clear_new());
    SESSIONS.with_borrow_mut(|m| m.clear_new());
    REGISTRATIONS.with_borrow_mut(|m| m.clear_new());
    TRANSIENT.with_borrow_mut(|t| *t = Transient::default());
    save_config(Config {
        schema: 1,
        governance: Principal::from_slice(&[1]),
        salt: Some(Hash::new([8; 32])),
        nonce: 0,
        admission_key: None,
        limits: AuthLimits {
            max_identities: 100,
            daily_registrations: 100,
            requests_per_minute: 1000,
            renewals_per_minute: 1000,
            max_challenges: 32,
        }
        .into(),
        day: 0,
        registered: 0,
    });
}

fn device(n: u8) -> (Principal, Vec<u8>) {
    let key = SigningKey::from_slice(&[n; 32]).unwrap();
    let public = hardware_der(key.verifying_key().to_sec1_point(false).as_bytes()).unwrap();
    (Principal::self_authenticating(&public), public)
}

fn challenge(n: u8, action: AuthAction, now: u64) -> AuthChallenge {
    let (caller, public) = device(n);
    prepare(
        caller,
        Principal::from_slice(&[9]),
        ChallengeRequest {
            public_key: public.into(),
            action,
        },
        now,
    )
    .unwrap()
}

fn response(challenge: &Hash, register: bool) -> PasskeyResponse {
    let key = SigningKey::from_slice(&[42; 32]).unwrap();
    let client_data = serde_json::to_vec(
        &serde_json::json!({"type": if register {"webauthn.create"} else {"webauthn.get"},
        "challenge": URL_SAFE_NO_PAD.encode(challenge.as_slice()), "origin": PASSKEY_ORIGIN}),
    )
    .unwrap();
    let mut data = [
        dmsg_protocol::sha256(PASSKEY_RP.as_bytes()).as_slice(),
        &[if register { 0x45 } else { 5 }],
        &[0; 4],
    ]
    .concat();
    let signature = if register {
        let public = key.verifying_key().to_sec1_point(false);
        let int = |n: i64| Value::Integer(n.into());
        let cose = Value::Map(vec![
            (int(1), int(2)),
            (int(3), int(-7)),
            (int(-1), int(1)),
            (int(-2), Value::Bytes(public.as_bytes()[1..33].to_vec())),
            (int(-3), Value::Bytes(public.as_bytes()[33..].to_vec())),
        ]);
        data.extend([0; 16]);
        data.extend([0, 3, 1, 2, 3]);
        data.extend(cbor2::to_vec(&cose).unwrap());
        data = cbor2::to_vec(&Value::Map(vec![
            (Value::Text("fmt".into()), Value::Text("none".into())),
            (Value::Text("authData".into()), Value::Bytes(data)),
            (Value::Text("attStmt".into()), Value::Map(vec![])),
        ]))
        .unwrap();
        vec![]
    } else {
        let signature: Signature = key.sign(
            &[
                data.as_slice(),
                dmsg_protocol::sha256(&client_data).as_slice(),
            ]
            .concat(),
        );
        signature.to_der().as_bytes().to_vec()
    };
    PasskeyResponse {
        credential_id: vec![1, 2, 3].into(),
        client_data_json: client_data.into(),
        authenticator_data: data.into(),
        signature: signature.into(),
    }
}

fn enroll() -> Session {
    let c = challenge(1, AuthAction::Register, 1000);
    register(
        device(1).0,
        Principal::from_slice(&[9]),
        RegisterPasskey {
            challenge: c.challenge,
            response: response(&c.challenge, true),
            admission: None,
        },
        1001,
    )
    .unwrap()
}

#[test]
fn logout_and_logout_all_cannot_be_undone_by_replaying_authentication() {
    setup();
    let first = enroll();
    let stable_seed = seed(&config(), &first.identity).unwrap();
    let c = challenge(2, AuthAction::Login, 2000);
    let proof = AuthenticatePasskey {
        challenge: c.challenge,
        response: response(&c.challenge, false),
    };
    login(device(2).0, proof.clone(), 2001).unwrap();
    logout(device(2).0).unwrap();
    assert!(matches!(
        login(device(2).0, proof, 2002),
        Err(Error::AuthRequired)
    ));
    let stale = challenge(20, AuthAction::Login, 2500);
    let c = challenge(3, AuthAction::LogoutAll, 3000);
    let proof = AuthenticatePasskey {
        challenge: c.challenge,
        response: response(&c.challenge, false),
    };
    logout_all(device(3).0, proof.clone(), 3001).unwrap();
    assert!(matches!(renew(device(1).0, DAY), Err(Error::AuthRequired)));
    assert!(matches!(
        logout_all(device(3).0, proof, 3002),
        Err(Error::AuthRequired)
    ));
    assert!(matches!(
        login(
            device(20).0,
            AuthenticatePasskey {
                challenge: stale.challenge,
                response: response(&stale.challenge, false)
            },
            3003
        ),
        Err(Error::AuthRequired)
    ));
    assert_eq!(seed(&config(), &first.identity).unwrap(), stable_seed);
    let c = challenge(1, AuthAction::Login, DAY);
    let session = login(
        device(1).0,
        AuthenticatePasskey {
            challenge: c.challenge,
            response: response(&c.challenge, false),
        },
        DAY + 1,
    )
    .unwrap();
    assert_eq!(session.identity, first.identity);
    assert!(renew(device(1).0, 180 * DAY).is_ok());
}

#[test]
fn operation_key_expiry_and_session_limits_are_enforced() {
    setup();
    enroll();
    let c = challenge(2, AuthAction::Login, 2000);
    let proof = AuthenticatePasskey {
        challenge: c.challenge,
        response: response(&c.challenge, false),
    };
    assert!(logout_all(device(2).0, proof.clone(), 2001).is_err());
    assert!(login(device(3).0, proof.clone(), 2001).is_err());
    assert!(matches!(
        login(device(2).0, proof, c.expires_at),
        Err(Error::Expired)
    ));
    for n in 2..=16 {
        let c = challenge(n, AuthAction::Login, 3000);
        login(
            device(n).0,
            AuthenticatePasskey {
                challenge: c.challenge,
                response: response(&c.challenge, false),
            },
            3001,
        )
        .unwrap();
    }
    let c = challenge(17, AuthAction::Login, 4000);
    assert!(matches!(
        login(
            device(17).0,
            AuthenticatePasskey {
                challenge: c.challenge,
                response: response(&c.challenge, false)
            },
            4001
        ),
        Err(Error::SessionLimitExceeded)
    ));
    assert!(session(device(1).0).is_some());
    assert_eq!(SESSIONS.with_borrow(|m| m.len()), 16);
}

#[test]
fn stable_records_survive_transient_reset_and_budgets_bound_renewals() {
    setup();
    let first = enroll();
    let cfg = config();
    let bytes = cbor2::to_vec(&cfg).unwrap();
    let restored: Config = cbor2::from_slice(&bytes).unwrap();
    assert_eq!(
        seed(&cfg, &first.identity),
        seed(&restored, &first.identity)
    );
    TRANSIENT.with_borrow_mut(|t| *t = Transient::default());
    assert_eq!(session(device(1).0).unwrap().identity, first.identity);
    assert!(matches!(
        renew(device(1).0, 1002),
        Err(Error::QuotaExceeded)
    ));
    let renewed = renew(device(1).0, MINUTE + 1001).unwrap();
    assert_eq!(
        renewed.expiration,
        (MINUTE + 1001 + AUTH_TTL_MS) * NANOS_PER_MILLISECOND
    );
    let mut cfg = config();
    let mut limits: AuthLimits = cfg.limits.into();
    limits.requests_per_minute = 1;
    cfg.limits = limits.into();
    save_config(cfg);
    let c = challenge(3, AuthAction::Login, 2 * MINUTE);
    assert!(matches!(
        login(
            device(3).0,
            AuthenticatePasskey {
                challenge: c.challenge,
                response: response(&c.challenge, false)
            },
            2 * MINUTE + 1
        ),
        Err(Error::QuotaExceeded)
    ));
    assert!(session(device(3).0).is_none());
    assert_eq!(SESSIONS.with_borrow(|m| m.len()), 1);
    let (caller, public) = device(4);
    assert!(matches!(
        prepare(
            caller,
            Principal::from_slice(&[9]),
            ChallengeRequest {
                public_key: public.into(),
                action: AuthAction::Login
            },
            2 * MINUTE
        ),
        Err(Error::QuotaExceeded)
    ));
}

#[test]
fn logout_does_not_allow_reusing_a_registration_caller() {
    setup();
    enroll();
    logout(device(1).0).unwrap();
    let c = challenge(1, AuthAction::Register, 2000);
    let mut response = response(&c.challenge, true);
    response.credential_id[0] = 4;
    let Value::Map(mut fields) = cbor2::from_slice(&response.authenticator_data).unwrap() else {
        panic!()
    };
    let (_, Value::Bytes(data)) = fields
        .iter_mut()
        .find(|(k, _)| *k == Value::Text("authData".into()))
        .unwrap()
    else {
        panic!()
    };
    data[55] = 4;
    response.authenticator_data = cbor2::to_vec(&Value::Map(fields)).unwrap().into();
    verify_registration(&response, &c.challenge).unwrap();
    assert_eq!(
        register(
            device(1).0,
            Principal::from_slice(&[9]),
            RegisterPasskey {
                challenge: c.challenge,
                response,
                admission: None,
            },
            2001
        )
        .err(),
        Some(Error::IdempotencyConflict)
    );
    assert_eq!(IDENTITIES.with_borrow(|m| m.len()), 1);
}
