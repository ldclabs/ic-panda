//! Deterministic BLS-signed test certificate. This is not a production authority.
use candid::Principal;
use dmsg_protocol::{canonical, integration::*};
use dmsg_types::{integration::*, *};
use ic_certification::{fork, labeled, leaf, Certificate};
fn fixture() -> (CertifiedBatch, AuthenticationRequest, Vec<u8>) {
    let home = Principal::from_slice(&[1, 1]);
    let at = 1_800_000_000_000u64;
    let request = AuthenticationRequest {
        version: 1,
        environment: Environment::Local,
        app_id: "sample".into(),
        app_config_version: 1,
        origin: "https://sample.test".into(),
        receiver: Principal::from_slice(&[2, 1]),
        challenge_hash: Hash::new([3; 32]),
        session_key_hash: Hash::new([4; 32]),
        purpose: AuthenticationPurpose::Login,
        nonce: Hash::new([5; 32]),
        operation_id: Hash::new([6; 32]),
        issued_at_ms: at,
        expires_at_ms: at + AUTH_TTL_MS,
    };
    let result = AuthenticationResult {
        version: 1,
        request: request.clone(),
        account_id: AccountId([1; 12]),
        home_user: home,
        security_epoch: 2,
        device_id: Hash::new([7; 32]),
        approved_at_ms: at,
        expires_at_ms: request.expires_at_ms,
    };
    let key = authentication_key(&result.account_id, &request.operation_id);
    let value = canonical(&result);
    let witness = labeled(key.clone(), leaf(value.clone()));
    let mut n = at * 1_000_000;
    let mut time = vec![];
    loop {
        let mut b = (n & 127) as u8;
        n >>= 7;
        if n > 0 {
            b |= 128;
        }
        time.push(b);
        if n == 0 {
            break;
        }
    }
    let tree = fork(
        labeled(
            b"canister".to_vec(),
            labeled(
                home.as_slice().to_vec(),
                labeled(b"certified_data".to_vec(), leaf(witness.digest().to_vec())),
            ),
        ),
        labeled(b"time".to_vec(), leaf(time)),
    );
    let keypair = ic_verify_bls_signature::PrivateKey::deserialize(&[1; 32]).unwrap();
    let mut message = b"\x0Dic-state-root".to_vec();
    message.extend(tree.digest());
    let certificate = Certificate {
        tree,
        signature: keypair.sign(&message).serialize().to_vec(),
        delegation: None,
    };
    let mut bytes = vec![0xd9, 0xd9, 0xf7];
    bytes.extend(canonical(&certificate));
    let mut root = ic_canister_sig_creation::IC_ROOT_PK_DER_PREFIX.to_vec();
    root.extend(keypair.public_key().serialize());
    (
        CertifiedBatch {
            schema: 1,
            canister: home,
            certificate: bytes.into(),
            entries: vec![CertifiedEntry {
                key: key.into(),
                value: Some(value.into()),
                witness: canonical(&witness).into(),
            }],
        },
        request,
        root,
    )
}

fn main() {
    let (batch, request, root) = fixture();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "batch_cbor_hex": hex::encode(canonical(&batch)),
            "request_cbor_hex": hex::encode(canonical(&request)),
            "root_der_hex": hex::encode(root),
            "now_ms": request.issued_at_ms.to_string()
        }))
        .unwrap()
    );
}
