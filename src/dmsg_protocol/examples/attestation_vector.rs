//! A deterministic attestation bundle (`dmsg-attestation/1`) whose receipt a
//! test BLS key certifies. This is not a production authority. Verifiers in
//! Rust, the TypeScript SDK and the dmsg.net page check the same bytes.
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use candid::Principal;
use dmsg_protocol::{
    account_issuer, canonical, execution_receipt_key, prepare_attestation, sha256,
};
use dmsg_types::*;
use ed25519_dalek::{Signer, SigningKey};
use ic_certification_v3::{Certificate, fork, labeled, leaf};

fn main() {
    let home = Principal::from_slice(&[1, 1]);
    let account = AccountId([1; 12]);
    let at = 1_800_000_000_000u64;
    let file = b"release 1.0 artifact bytes".to_vec();
    let statement = Statement {
        issuer: account_issuer("https://dmsg.net/u/", &account),
        subject: None,
        issued_at: None,
        content: StatementContent::FileStatement {
            text: "I approve release 1.0".into(),
            sha256: sha256(&file),
            content_type: Some("text/plain".into()),
            location: None,
        },
    };
    let key = SigningKey::from_bytes(&[9; 32]);
    let public = Hash::new(key.verifying_key().to_bytes());
    let prepared = prepare_attestation(&statement, &public).unwrap();
    let signature = key.sign(&prepared.to_be_signed).to_bytes();
    let to_be_signed = sha256(&prepared.to_be_signed);
    let fingerprint = prepared.thumbprint;
    let artifact = prepared.finish(&signature).unwrap();
    let request = Hash::new([8; 32]);
    let receipt = ExecutionReceipt {
        schema: 2,
        account_id: account,
        issuer: statement.issuer.clone(),
        request_id: request,
        device_id: Hash::new([7; 32]),
        security_epoch: 3,
        approved_at: at,
        expires_at: at + 60_000,
        origin: "https://dmsg.net".into(),
        to_be_signed_digest: to_be_signed,
        public_key_fingerprint: fingerprint,
        signature_digest: sha256(&signature),
    };
    let path = execution_receipt_key(&account, request);
    let value = canonical(&receipt);
    let witness = labeled(path.clone(), leaf(value.clone()));
    let mut n = (at + 1_000) * 1_000_000;
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
    let bls = ic_verify_bls_signature::PrivateKey::deserialize(&[1; 32]).unwrap();
    let mut message = b"\x0Dic-state-root".to_vec();
    message.extend(tree.digest());
    let certificate = Certificate {
        tree,
        signature: bls.sign(&message).serialize().to_vec(),
        delegation: None,
    };
    let mut certificate_bytes = vec![0xd9, 0xd9, 0xf7];
    certificate_bytes.extend(canonical(&certificate));
    let batch = CertifiedBatch {
        schema: 1,
        canister: home,
        certificate: certificate_bytes.into(),
        entries: vec![CertifiedEntry {
            key: path.into(),
            value: Some(value.into()),
            witness: canonical(&witness).into(),
        }],
    };
    let mut root = ic_canister_sig_creation::IC_ROOT_PK_DER_PREFIX.to_vec();
    root.extend(bls.public_key().serialize());
    let b64 = |bytes: &[u8]| URL_SAFE_NO_PAD.encode(bytes);
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "bundle": {
                "format": "dmsg-attestation/1",
                "account": account.to_string(),
                "home": home.to_text(),
                "request_id": hex::encode(*request),
                "cose_sign1": b64(&artifact.cose_sign1),
                "cose_key": b64(&artifact.cose_key),
                "receipt": b64(&canonical(&batch)),
            },
            "file_hex": hex::encode(&file),
            "root_der_hex": hex::encode(root),
            "certified_at_ms": (at + 1_000).to_string(),
        }))
        .unwrap()
    );
}
