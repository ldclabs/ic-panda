use super::*;
use crate::crypto::tests::Replay;
use crate::crypto::{aes_gcm_seal, hpke_public, p256_public, p256_sign};
use hpke::OpModeS;
use serde_bytes::ByteArray;
use serde_json::json;

const VECTORS: &str = "../../tests/vectors/client/connect-v1.json";
const NOW: u64 = 1_800_000_000_000;

fn seed(label: &str) -> Key {
    *crate::codec::sha256(format!("connect-v1:{label}").as_bytes())
}

fn h(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

struct Fixture {
    account: AccountId,
    page: Key,
    pairing: Key,
    device: Hash,
    device_hpke: Key,
    other_hpke: Key,
    result: Key,
    command: ConnectCommand,
    request: ConnectRequest,
}

fn fixture() -> Fixture {
    let account = AccountId([0x31; 12]);
    let page = seed("page");
    let pairing = seed("pairing");
    let result = seed("result");
    let statement = json!({
        "protocol": "dmsg-connect/1",
        "method": "signature.request",
        "requestId": "a1".repeat(32),
        "accountId": account.to_string(),
        "nonce": "b2".repeat(32),
        "expiresAt": (NOW + 300_000).to_string(),
        "statement": {
            "issuer": format!("https://dmsg.local/{account}"),
            "content": {"kind": "text", "text": "I approve release 1.2.0."}
        }
    });
    let command = ConnectCommand {
        method: "signDocument".into(),
        app_id: "token-list".into(),
        operation_id: "a1".repeat(32),
        public_key: STANDARD.encode(p256_public(&page).unwrap()),
        payload: Some(STANDARD.encode(serde_json::to_vec(&statement).unwrap())),
        result_digest: None,
    };
    let pairing_id = pairing_id(&p256_public(&pairing).unwrap());
    let nonce = Hash::new([0x44; 32]);
    let connection = Hash::new([0x55; 32]);
    let origin = "http://localhost:5173".to_string();
    let message = proof_message(&nonce, &origin, &connection, &command);
    let request = ConnectRequest {
        protocol: CONNECT_PROTOCOL.into(),
        environment: dmsg_types::Environment::Local,
        account_id: account,
        pairing_id,
        request_id: Hash::new([0x66; 32]),
        kind: RequestKind::Create,
        origin,
        connection_id: connection,
        nonce,
        command: command.clone(),
        page_signature: ByteArray::new(p256_sign(&page, message.as_slice()).unwrap()),
        result_key: Hash::new(hpke_public(&result)),
        created_at: NOW,
        expires_at: NOW + 300_000,
    };
    Fixture {
        account,
        page,
        pairing,
        device: Hash::new([0x77; 32]),
        device_hpke: seed("device-hpke"),
        other_hpke: seed("other-hpke"),
        result,
        command,
        request,
    }
}

fn build() -> Json {
    let f = fixture();
    let pairing_pub = p256_public(&f.pairing).unwrap();
    let pairing = pairing_id(&pairing_pub);
    let request_cbor = canonical(&f.request);
    let cose = es256_sign1(&f.pairing, &pairing, REQUEST_TYP, &request_cbor).unwrap();
    let cek = seed("cek");
    let nonce = [0x88u8; 12];
    let ekm = seed("wrap-ekm");
    let wrap = wrap_info(&f.account, &pairing, &f.request.request_id, &f.device);
    let wrapped = crate::crypto::hpke_seal_with(
        &OpModeS::Base,
        &hpke_public(&f.device_hpke),
        &cek,
        &wrap,
        &mut Replay(ekm.to_vec()),
    )
    .unwrap();
    let aad = request_aad(&f.account, &pairing, &f.request.request_id);
    let envelope = Envelope {
        v: 1,
        recipients: vec![Recipient {
            device_id: f.device,
            enc: Hash::new(wrapped.enc.clone().try_into().unwrap()),
            wrapped: wrapped.ciphertext.clone().into(),
        }],
        nonce: ByteArray::new(nonce),
        ciphertext: aes_gcm_seal(&cek, &nonce, &cose, &aad).into(),
    };
    let value = json!({"operationId": f.command.operation_id, "kind": "document", "state": "awaiting_user"});
    let response = ConnectResponse {
        protocol: CONNECT_PROTOCOL.into(),
        request_id: f.request.request_id,
        version: 1,
        operation_digest: operation_digest(&f.request.origin, &f.command).unwrap(),
        value: serde_json::to_vec(&value).unwrap().into(),
    };
    let info = response_info(&f.account, &pairing, &f.request.request_id, 1, &f.device);
    let sealed = crate::crypto::hpke_seal_auth_with(
        &f.device_hpke,
        &f.request.result_key,
        &canonical(&response),
        &info,
        &mut Replay(seed("response-ekm").to_vec()),
    )
    .unwrap();
    let sealed = Sealed {
        enc: Hash::new(sealed.enc.try_into().unwrap()),
        ciphertext: sealed.ciphertext.into(),
    };
    let invite = PairingInvite {
        v: 1,
        environment: dmsg_types::Environment::Local,
        account_id: f.account,
        invite_id: Hash::new([0x99; 32]),
        secret: Hash::new(seed("invite-secret")),
    };
    let label = "Chrome · macOS";
    let mac = pairing_mac(
        &invite.secret,
        &invite.environment,
        &f.account,
        &invite.invite_id,
        &pairing_pub,
        label,
    );
    let pop = ConnectPop {
        protocol: CONNECT_PROTOCOL.into(),
        account_id: f.account,
        pairing_id: pairing,
        request_id: Hash::new([0xaa; 32]),
        deadline: NOW + 45_000,
        audience: "http://127.0.0.1:8787".into(),
        method: "POST".into(),
        target: "/v1/connect/requests".into(),
        body_digest: sha256(&canonical(&CreateRequest {
            request_id: f.request.request_id,
            kind: RequestKind::Create,
            expires_at: f.request.expires_at,
            envelope: envelope.clone(),
        })),
    };
    let pop_cose = es256_sign1(&f.pairing, &pairing, POP_TYP, &canonical(&pop)).unwrap();
    let proof = proof_message(
        &f.request.nonce,
        &f.request.origin,
        &f.request.connection_id,
        &f.command,
    );
    json!({
        "schema": 1,
        "test_only": true,
        "generator": "src/dmsg_client/src/connect_tests.rs",
        "description": "dmsg-connect/1 byte rules. P-256 signatures here are deterministic (RFC 6979); WebCrypto's are randomized and verify the same way. HPKE ephemeral keys come from the given ekm. Readers must reproduce every digest and encoding, verify every signature and open every sealed item with the given keys.",
        "keys": {
            "page_secret_hex": h(&f.page),
            "page_spki_hex": h(&p256_public(&f.page).unwrap()),
            "pairing_secret_hex": h(&f.pairing),
            "pairing_spki_hex": h(&pairing_pub),
            "device_id_hex": h(f.device.as_slice()),
            "device_hpke_seed_hex": h(&f.device_hpke),
            "device_hpke_public_hex": h(&hpke_public(&f.device_hpke)),
            "result_seed_hex": h(&f.result),
            "result_public_hex": h(&hpke_public(&f.result)),
        },
        "account": f.account.to_string(),
        "account_hex": h(f.account.as_slice()),
        "environment": "Local",
        "command": serde_json::to_value(&f.command).unwrap(),
        "command_cbor_hex": h(&canonical(&f.command)),
        "proof": {
            "nonce_hex": h(f.request.nonce.as_slice()),
            "origin": f.request.origin,
            "connection_id_hex": h(f.request.connection_id.as_slice()),
            "message_hex": h(proof.as_slice()),
            "signature_hex": h(f.request.page_signature.as_slice()),
        },
        "operation_digest_hex": h(response.operation_digest.as_slice()),
        "pairing_id_hex": h(pairing.as_slice()),
        "invite": {
            "cbor_hex": h(&canonical(&invite)),
            "fragment": b64(&canonical(&invite)),
            "label": label,
            "mac_hex": h(mac.as_slice()),
        },
        "request": {
            "cbor_hex": h(&request_cbor),
            "protected_hex": h(&es256_header(&pairing, REQUEST_TYP)),
            "cose_hex": h(&cose),
        },
        "envelope": {
            "aad_hex": h(&aad),
            "wrap_info_hex": h(&wrap),
            "cek_hex": h(&cek),
            "ekm_hex": h(&ekm),
            "cbor_hex": h(&canonical(&envelope)),
        },
        "response": {
            "value": value,
            "cbor_hex": h(&canonical(&response)),
            "info_hex": h(&info),
            "ekm_hex": h(&seed("response-ekm")),
            "sealed_cbor_hex": h(&canonical(&sealed)),
        },
        "pop": {
            "cbor_hex": h(&canonical(&pop)),
            "cose_hex": h(&pop_cose),
            "header": b64(&pop_cose),
        },
    })
}

#[test]
fn connect_vectors_match() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(VECTORS);
    let mut text = serde_json::to_string_pretty(&build()).unwrap();
    text.push('\n');
    if std::env::var_os("DMSG_WRITE_CLIENT_VECTORS").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        text,
        "rerun with DMSG_WRITE_CLIENT_VECTORS=1"
    );
}

#[test]
fn devices_open_requests_and_pages_open_replies() {
    let v = build();
    let f = fixture();
    let unhex = |p: &str| hex::decode(v.pointer(p).unwrap().as_str().unwrap()).unwrap();
    let pairing = Hash::new(unhex("/pairing_id_hex").try_into().unwrap());
    let envelope: Envelope = decode(&unhex("/envelope/cbor_hex"), MAX_ENVELOPE).unwrap();
    let cose = open_request(
        &envelope,
        &f.account,
        &pairing,
        &f.request.request_id,
        &f.device,
        &f.device_hpke,
    )
    .unwrap();
    assert_eq!(cose, unhex("/request/cose_hex"));
    let payload = es256_open(
        &cose,
        &pairing,
        REQUEST_TYP,
        &unhex("/keys/pairing_spki_hex"),
    )
    .unwrap();
    let request: ConnectRequest = decode(&payload, MAX_ENVELOPE).unwrap();
    validate_request(&request, &dmsg_types::Environment::Local, NOW).unwrap();
    p256_verify(
        &command_key(&request.command).unwrap(),
        &unhex("/proof/message_hex"),
        request.page_signature.as_slice(),
    )
    .unwrap();
    // The relay swaps the request ID or the recipient: nothing opens.
    assert!(
        open_request(
            &envelope,
            &f.account,
            &pairing,
            &Hash::new([1; 32]),
            &f.device,
            &f.device_hpke
        )
        .is_err()
    );
    assert_eq!(
        open_request(
            &envelope,
            &f.account,
            &pairing,
            &f.request.request_id,
            &Hash::new([2; 32]),
            &f.device_hpke
        ),
        Err(Error::NotFound)
    );
    // Another pairing key cannot pass for this one (DT19).
    let other = p256_public(&seed("other-pairing")).unwrap();
    assert!(es256_open(&cose, &pairing, REQUEST_TYP, &other).is_err());
    assert!(es256_open(&cose, &pairing, POP_TYP, &unhex("/keys/pairing_spki_hex")).is_err());

    let sealed: Sealed = decode(&unhex("/response/sealed_cbor_hex"), 640 * 1024).unwrap();
    let response = open_response(
        &sealed,
        &f.account,
        &pairing,
        &f.request.request_id,
        1,
        &f.device,
        &hpke_public(&f.device_hpke),
        &f.result,
    )
    .unwrap();
    assert_eq!(canonical(&response), unhex("/response/cbor_hex"));
    // A reply from a key outside the certified device set fails (DT32),
    // and so does a replayed version.
    assert!(
        open_response(
            &sealed,
            &f.account,
            &pairing,
            &f.request.request_id,
            1,
            &f.device,
            &hpke_public(&f.other_hpke),
            &f.result
        )
        .is_err()
    );
    assert!(
        open_response(
            &sealed,
            &f.account,
            &pairing,
            &f.request.request_id,
            2,
            &f.device,
            &hpke_public(&f.device_hpke),
            &f.result
        )
        .is_err()
    );
    let forged = seal_response(
        &response,
        &f.account,
        &pairing,
        &f.device,
        &f.other_hpke,
        &f.request.result_key,
    )
    .unwrap();
    assert!(
        open_response(
            &forged,
            &f.account,
            &pairing,
            &f.request.request_id,
            1,
            &f.device,
            &hpke_public(&f.device_hpke),
            &f.result
        )
        .is_err()
    );

    // The pairing MAC binds the label the user confirms (DT18).
    let invite: PairingInvite = decode(&unhex("/invite/cbor_hex"), 4096).unwrap();
    let pairing_pub = unhex("/keys/pairing_spki_hex");
    let mac = pairing_mac(
        &invite.secret,
        &invite.environment,
        &invite.account_id,
        &invite.invite_id,
        &pairing_pub,
        "Chrome · macOS",
    );
    assert_eq!(mac.to_vec(), unhex("/invite/mac_hex"));
    assert_ne!(
        mac,
        pairing_mac(
            &invite.secret,
            &invite.environment,
            &invite.account_id,
            &invite.invite_id,
            &pairing_pub,
            "Evil · macOS"
        )
    );
    assert_ne!(
        mac,
        pairing_mac(
            &invite.secret,
            &invite.environment,
            &invite.account_id,
            &invite.invite_id,
            &other,
            "Chrome · macOS"
        )
    );

    let pop: ConnectPop = decode(&unhex("/pop/cbor_hex"), 4096).unwrap();
    assert_eq!(
        es256_open(
            &unhex("/pop/cose_hex"),
            &pop.pairing_id,
            POP_TYP,
            &pairing_pub
        )
        .unwrap(),
        unhex("/pop/cbor_hex")
    );
}

#[test]
fn random_seals_open() {
    let f = fixture();
    let pairing = f.request.pairing_id;
    let cose = es256_sign1(&f.pairing, &pairing, REQUEST_TYP, &canonical(&f.request)).unwrap();
    let envelope = seal_request(
        &cose,
        &f.account,
        &pairing,
        &f.request.request_id,
        &[(f.device, hpke_public(&f.device_hpke))],
        &random_key(),
        &[1; 12],
    )
    .unwrap();
    assert_eq!(
        open_request(
            &envelope,
            &f.account,
            &pairing,
            &f.request.request_id,
            &f.device,
            &f.device_hpke
        )
        .unwrap(),
        cose
    );
    let _ = f.page;
}

#[test]
fn accepts_high_s_signatures_as_webcrypto_makes_them() {
    let secret = seed("high-s");
    let der = p256_public(&secret).unwrap();
    let message = b"proof message";
    let low = p256_sign(&secret, message).unwrap();
    let signature = p256::ecdsa::Signature::from_slice(&low).unwrap();
    let s = signature.s();
    let high = p256::ecdsa::Signature::from_scalars(signature.r(), -*s.as_ref()).unwrap();
    p256_verify(&der, message, &low).unwrap();
    p256_verify(&der, message, &high.to_bytes()).unwrap();
}

#[test]
fn a_mailbox_page_read_before_an_account_switch_is_refused() {
    let dir = crate::testing::TempDir::new();
    let mut client = crate::testing::unlocked(&dir.0, false, None);
    crate::testing::warm(&mut client);
    let (_, from) = client.connect_fetch_request().unwrap();
    let page = json!({"requests": [{
        "seq": 7,
        "request_id": "a1".repeat(32),
        "pairing_id": "b2".repeat(32),
        "kind": "read",
        "created_at": NOW,
        "expires_at": NOW + 300_000,
        "envelope": b64(&[1, 2, 3]),
        "state": "pending",
        "version": 1,
        "pinned_device": null,
        "acked_digest": null,
    }]});
    // Signed for another account, the page neither lands here nor moves
    // this account's cursor.
    let other = MailboxCursor {
        account: AccountId([0x31; 12]),
        after: from.after,
    };
    assert!(matches!(
        client.connect_fetch_store(other, &page),
        Err(Error::Unavailable(_))
    ));
    assert_eq!(client.connect_cursor(), Ok(0));
    assert_eq!(client.connect_fetch_store(from, &page), Ok(1));
    assert_eq!(client.connect_cursor(), Ok(7));
}
