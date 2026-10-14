use super::*;
use dmsg_types::login::{LoginError, LoginResult};
use serde_json::json;

const NOW: u64 = 1_760_000_000_000;

fn targets() -> Vec<Principal> {
    vec![
        Principal::from_text("rrkah-fqaaa-aaaaa-aaaaq-cai").unwrap(),
        Principal::from_text("aaaaa-aa").unwrap(),
    ]
}

fn session() -> LoginSession {
    LoginSession::new(&targets(), "https://dmsg.net", 53817, DEFAULT_TTL_MS, NOW).unwrap()
}

/// A chain from a test login key to `to`, as `DelegationChain.toJSON()` writes it.
fn chain(to: &[u8], expiration_ms: u64, targets: Option<Vec<Principal>>) -> Vec<u8> {
    let user = BasicIdentity::from_raw_key(&[3; 32]);
    let delegation = Delegation {
        pubkey: to.to_vec(),
        expiration: expiration_ms * 1_000_000,
        targets: targets.clone(),
        permissions: None,
    };
    let signature = user
        .sign_delegation(&delegation)
        .unwrap()
        .signature
        .unwrap();
    let mut link = json!({
        "expiration": format!("{:x}", delegation.expiration),
        "pubkey": hex::encode(to),
    });
    if let Some(targets) = targets {
        link["targets"] = json!(targets
            .iter()
            .map(|t| hex::encode(t.as_slice()))
            .collect::<Vec<_>>());
    }
    json!({
        "delegations": [{"delegation": link, "signature": hex::encode(signature)}],
        "publicKey": hex::encode(user.public_key().unwrap()),
    })
    .to_string()
    .into_bytes()
}

fn body(session: &LoginSession, delegation: Vec<u8>) -> Vec<u8> {
    canonical(&LoginCallback::Result(LoginResult {
        v: 1,
        state: session.request.state,
        delegation: delegation.into(),
    }))
}

#[test]
fn accepts_a_delegation_to_the_session_key() {
    let s = session();
    let key = s.request.session_pubkey.to_vec();
    // Internet Identity signs no targets; the session key adds them.
    let accepted = s
        .accept(&body(&s, chain(&key, NOW + 600_000, None)), NOW)
        .unwrap();
    let user = BasicIdentity::from_raw_key(&[3; 32]);
    assert_eq!(accepted.principal, user.sender().unwrap());
    assert_eq!(accepted.identity.sender().unwrap(), accepted.principal);
    assert_eq!(accepted.expires_at, NOW + 600_000);
    let links = accepted.identity.delegation_chain();
    assert_eq!(links.len(), 2);
    assert_eq!(links[0].delegation.targets, None);
    let scoped = &links[1];
    assert_eq!(scoped.delegation.targets, Some(targets()));
    assert_eq!(scoped.delegation.expiration, links[0].delegation.expiration);
    assert_ne!(scoped.delegation.pubkey, key);
    // Ed25519 signs deterministically.
    let session_key = BasicIdentity::from_raw_key(&s.seed);
    assert_eq!(
        session_key
            .sign_delegation(&scoped.delegation)
            .unwrap()
            .signature,
        Some(scoped.signature.clone())
    );
    // A link that names targets stays exactly as signed.
    let narrow = Some(targets()[..1].to_vec());
    let accepted = s
        .accept(&body(&s, chain(&key, NOW + 600_000, narrow.clone())), NOW)
        .unwrap();
    assert_eq!(
        accepted.identity.delegation_chain()[0].delegation.targets,
        narrow
    );
    // Internet Identity dates the expiration by the IC's clock, which may
    // run ahead of this one.
    let ahead = NOW + DEFAULT_TTL_MS + 5_000;
    let late = s.accept(&body(&s, chain(&key, ahead, Some(targets()))), NOW);
    assert_eq!(late.unwrap().expires_at, ahead);
    assert!(s
        .url(&test_config())
        .starts_with("https://dmsg.net/_/login#"));
}

#[test]
fn refuses_foreign_or_broader_callbacks() {
    let s = session();
    let key = s.request.session_pubkey.to_vec();
    let ok = |at| chain(&key, at, None);
    // Another key, a query-only delegation.
    let mut other = ED25519_SPKI_PREFIX.to_vec();
    other.extend([9; 32]);
    let mut queries: serde_json::Value = serde_json::from_slice(&ok(NOW + 1)).unwrap();
    queries["delegations"][0]["delegation"]["permissions"] = json!("queries");
    for (delegation, error) in [
        (chain(&other, NOW + 1, None), Error::IntegrityFailed),
        (queries.to_string().into_bytes(), Error::IntegrityFailed),
        (ok(NOW), Error::Expired),
        (ok(NOW + DEFAULT_TTL_MS + CLOCK_SKEW_MS + 1), Error::Expired),
        (b"{}".to_vec(), Error::IntegrityFailed),
    ] {
        assert_eq!(s.accept(&body(&s, delegation), NOW).err(), Some(error));
    }
    let foreign = canonical(&LoginCallback::Result(LoginResult {
        v: 1,
        state: Hash::new([1; 32]),
        delegation: ok(NOW + 1).into(),
    }));
    assert_eq!(s.accept(&foreign, NOW).err(), Some(Error::IntegrityFailed));
    let cancelled = canonical(&LoginCallback::Error(LoginError {
        v: 1,
        state: s.request.state,
        error: LoginErrorCode::UserCancelled,
    }));
    assert_eq!(
        s.accept(&cancelled, NOW).err(),
        Some(Error::Unavailable("login: user_cancelled".into()))
    );
    assert!(s.accept(&[], NOW).is_err());
}

#[test]
fn requests_follow_the_page_rules() {
    assert!(LoginSession::new(&[], "https://dmsg.net", 1, DEFAULT_TTL_MS, NOW).is_err());
    let many: Vec<Principal> = (0..=MAX_TARGETS as u64)
        .map(|i| Principal::from_slice(&i.to_be_bytes()))
        .collect();
    assert!(LoginSession::new(&many, "https://dmsg.net", 1, DEFAULT_TTL_MS, NOW).is_err());
    assert!(LoginSession::new(&targets(), "https://id.ai", 1, DEFAULT_TTL_MS, NOW).is_err());
    assert!(LoginSession::new(&targets(), "https://dmsg.net", 0, DEFAULT_TTL_MS, NOW).is_err());
    let s = session();
    assert_eq!(s.request.next_url, "http://127.0.0.1:53817/auth/callback");
    dmsg_protocol::login::decode_request(&canonical(&s.request)).unwrap();
    assert_eq!(s.deadline, NOW + LISTEN_MS);
}

fn test_config() -> Config {
    Config {
        environment: dmsg_types::Environment::Production,
        ic_host: "https://icp-api.io".into(),
        relay: "https://api.dmsg.test".into(),
        handle: Principal::anonymous(),
        user_homes: vec![],
        root_key: None,
        login_url: crate::config::LOGIN_URL.into(),
        connect_url: crate::config::CONNECT_URL.into(),
        background_online: false,
        legacy: None,
    }
}
