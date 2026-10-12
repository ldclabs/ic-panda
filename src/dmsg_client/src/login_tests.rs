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
    let accepted = s
        .accept(
            &body(
                &s,
                chain(&key, NOW + 600_000, Some(targets()[..1].to_vec())),
            ),
            NOW,
        )
        .unwrap();
    let user = BasicIdentity::from_raw_key(&[3; 32]);
    assert_eq!(accepted.principal, user.sender().unwrap());
    assert_eq!(accepted.identity.sender().unwrap(), accepted.principal);
    assert_eq!(accepted.expires_at, NOW + 600_000);
    assert!(s
        .url(&test_config())
        .starts_with("https://dmsg.net/_/login#"));
}

#[test]
fn refuses_foreign_or_broader_callbacks() {
    let s = session();
    let key = s.request.session_pubkey.to_vec();
    let ok = |at| chain(&key, at, Some(targets()));
    // Another key, a target outside the request, an unrestricted delegation.
    let mut other = ED25519_SPKI_PREFIX.to_vec();
    other.extend([9; 32]);
    let wider = vec![Principal::from_text("ryjl3-tyaaa-aaaaa-aaaba-cai").unwrap()];
    for (delegation, error) in [
        (
            chain(&other, NOW + 1, Some(targets())),
            Error::IntegrityFailed,
        ),
        (chain(&key, NOW + 1, Some(wider)), Error::Forbidden),
        (chain(&key, NOW + 1, None), Error::Forbidden),
        (ok(NOW), Error::Expired),
        (ok(NOW + DEFAULT_TTL_MS + 1), Error::Expired),
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
        agent_service: crate::config::AGENT_SERVICE.into(),
        background_online: false,
    }
}
