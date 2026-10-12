use super::*;
use crate::integration::validate_identifier;

fn key() -> String {
    let mut der = P256_SPKI_PREFIX.to_vec();
    der.push(4);
    der.extend([7u8; 64]);
    STANDARD.encode(der)
}

fn command(method: &str) -> ConnectCommand {
    let create = CREATE_METHODS.contains(&method);
    ConnectCommand {
        method: method.into(),
        app_id: "token-list".into(),
        operation_id: "01".repeat(32),
        public_key: key(),
        payload: create.then(|| STANDARD.encode(b"payload")),
        result_digest: (method == "acknowledge").then(|| "ab".repeat(32)),
    }
}

#[test]
fn commands_have_a_closed_shape() {
    for method in METHODS {
        validate_command(&command(method)).unwrap();
    }
    validate_identifier("token-list").unwrap();
    let mut c = command("signDocument");
    c.method = "sign".into();
    assert_eq!(validate_command(&c), Err(Error::UnsupportedProtocol));
    for change in [
        |c: &mut ConnectCommand| c.operation_id = "00".repeat(32),
        |c: &mut ConnectCommand| c.operation_id = "AB".repeat(32),
        |c: &mut ConnectCommand| c.operation_id = "ab".repeat(31),
        |c: &mut ConnectCommand| c.app_id = "Token".into(),
        |c: &mut ConnectCommand| c.public_key = STANDARD.encode([4u8; 91]),
        |c: &mut ConnectCommand| c.public_key = c.public_key.trim_end_matches('=').into(),
        |c: &mut ConnectCommand| c.payload = None,
        |c: &mut ConnectCommand| {
            c.payload = Some(STANDARD.encode(vec![0u8; MAX_COMMAND_PAYLOAD + 1]))
        },
        |c: &mut ConnectCommand| c.payload = Some("cGF5bG9hZA".into()),
        |c: &mut ConnectCommand| c.result_digest = Some("ab".repeat(32)),
    ] {
        let mut c = command("signDocument");
        change(&mut c);
        assert!(
            matches!(validate_command(&c), Err(Error::InvalidInput(_))),
            "{c:?}"
        );
    }
    let mut c = command("getOperation");
    c.payload = Some(STANDARD.encode(b"x"));
    assert!(validate_command(&c).is_err());
    let mut c = command("acknowledge");
    c.result_digest = None;
    assert!(validate_command(&c).is_err());
}

#[test]
fn digests_bind_every_field() {
    let c = command("signDocument");
    let a = operation_digest("https://app.example", &c).unwrap();
    assert_ne!(a, operation_digest("https://other.example", &c).unwrap());
    let mut d = c.clone();
    d.payload = Some(STANDARD.encode(b"other"));
    assert_ne!(a, operation_digest("https://app.example", &d).unwrap());
    assert!(operation_digest("https://app.example", &command("getOperation")).is_err());
    let nonce = Hash::new([1; 32]);
    let connection = Hash::new([2; 32]);
    let p = proof_message(&nonce, "https://app.example", &connection, &c);
    assert_ne!(
        p,
        proof_message(&connection, "https://app.example", &nonce, &c)
    );
    let account = AccountId([3; 12]);
    let invite = Hash::new([4; 32]);
    let secret = Hash::new([5; 32]);
    let pub_key = STANDARD.decode(key()).unwrap();
    let mac = pairing_mac(
        &secret,
        &Environment::Local,
        &account,
        &invite,
        &pub_key,
        "Chrome · macOS",
    );
    assert_ne!(
        mac,
        pairing_mac(
            &secret,
            &Environment::Local,
            &account,
            &invite,
            &pub_key,
            "Firefox · macOS"
        )
    );
    assert_ne!(
        mac,
        pairing_mac(
            &secret,
            &Environment::Staging,
            &account,
            &invite,
            &pub_key,
            "Chrome · macOS"
        )
    );
}

#[test]
fn requests_are_bound_to_the_deployment_and_clock() {
    let now = 1_800_000_000_000;
    let mut request = ConnectRequest {
        protocol: CONNECT_PROTOCOL.into(),
        environment: Environment::Local,
        account_id: AccountId([3; 12]),
        pairing_id: Hash::new([1; 32]),
        request_id: Hash::new([2; 32]),
        kind: RequestKind::Create,
        origin: "http://localhost:5173".into(),
        connection_id: Hash::new([4; 32]),
        nonce: Hash::new([5; 32]),
        command: command("signDocument"),
        page_signature: serde_bytes::ByteArray::new([6; 64]),
        result_key: Hash::new([7; 32]),
        created_at: now,
        expires_at: now + CREATE_TTL_MS,
    };
    validate_request(&request, &Environment::Local, now).unwrap();
    assert_eq!(
        validate_request(&request, &Environment::Staging, now),
        Err(Error::UnsupportedProtocol)
    );
    request.expires_at += 1;
    assert_eq!(
        validate_request(&request, &Environment::Local, now),
        Err(Error::Expired)
    );
    request.expires_at = now + MINUTE;
    request.created_at = now + 2 * MINUTE;
    assert_eq!(
        validate_request(&request, &Environment::Local, now),
        Err(Error::Expired)
    );
    request.created_at = now;
    request.kind = RequestKind::Read;
    assert!(validate_request(&request, &Environment::Local, now).is_err());
    request.command = command("getOperation");
    validate_request(&request, &Environment::Local, now).unwrap();
    request.expires_at = now + READ_TTL_MS + 1;
    assert_eq!(
        validate_request(&request, &Environment::Local, now),
        Err(Error::Expired)
    );
    request.expires_at = now + READ_TTL_MS;
    request.origin = "chrome-extension://abcdefghijklmnopabcdefghijklmnop".into();
    assert!(validate_request(&request, &Environment::Local, now).is_err());
}

#[test]
fn grants_bind_their_key_and_lifetime() {
    let pairing_pub = STANDARD.decode(key()).unwrap();
    let mut grant = PairingGrant {
        invite_id: Hash::new([1; 32]),
        pairing_id: pairing_id(&pairing_pub),
        pairing_pub: pairing_pub.into(),
        label: "Chrome · macOS".into(),
        home: candid::Principal::from_slice(&[1; 10]),
        granted_at: 1_000,
        expires_at: 1_000 + PAIRING_TTL_MS,
    };
    validate_grant(&grant, 2_000).unwrap();
    assert_eq!(
        validate_grant(&grant, grant.expires_at),
        Err(Error::Expired)
    );
    grant.expires_at += 1;
    assert!(validate_grant(&grant, 2_000).is_err());
    grant.expires_at -= 1;
    grant.pairing_id = Hash::new([9; 32]);
    assert!(validate_grant(&grant, 2_000).is_err());
}
