//! Run with `cargo bench -p dmsg_protocol --bench validation --locked`.
use std::{hint::black_box, time::Instant};

use dmsg_protocol::*;
use dmsg_types::{cose::*, user::*, *};
use ed25519_dalek::{Signer, SigningKey};

fn measure(name: &str, iterations: u32, mut run: impl FnMut()) {
    for _ in 0..100 {
        run();
    }
    let mut samples = [0; 7];
    for sample in &mut samples {
        let start = Instant::now();
        for _ in 0..iterations {
            run();
        }
        *sample = start.elapsed().as_nanos() / u128::from(iterations);
    }
    samples.sort_unstable();
    println!("{name}: {} ns/op (median of 7 samples)", samples[3]);
}

fn main() {
    let namespace = "https://dmsg.test/u/";
    let account = AccountId([1; 12]);
    let issuer = account_issuer(namespace, &account);
    let signer = SigningKey::from_bytes(&[7; 32]);
    let statement = Statement {
        issuer: issuer.clone(),
        subject: Some("release/spec".into()),
        issued_at: Some(1_800_000_000),
        content: StatementContent::Text("document".repeat(512)),
    };
    let (_, tbs) = prepare_cose(&statement, &Algorithm::Ed25519, b"kid").unwrap();
    let signature = signer.sign(&tbs).to_bytes().to_vec();
    let artifact = finish_cose(&tbs, &signer.verifying_key().to_bytes(), signature).unwrap();
    let receipt = ExecutionReceipt {
        schema: 1,
        account_id: account,
        issuer: issuer.clone(),
        request_id: Hash::new([2; 32]),
        device_id: Hash::new([3; 32]),
        security_epoch: 1,
        approved_at: 10,
        expires_at: 20,
        origin: "https://app.test".into(),
        max_cycles: 100,
        to_be_signed_digest: sha256(&tbs),
        public_key_fingerprint: key_thumbprint(&artifact.cose_key).unwrap(),
        status: ExecutionStatus::Completed,
        signature_digest: Some(signature_digest(&artifact.cose_sign1).unwrap()),
    };
    let device = DeviceInput {
        device_id: Hash::new([3; 32]),
        signing_pub: signer.verifying_key().to_bytes().into(),
        hpke_pub: Hash::new([4; 32]),
        role: ControllerRole::Administrator,
        capabilities: vec![Capability::ContentSign, Capability::FormalApprove],
    };
    measure("mul_div_fee", 100_000, || {
        black_box(
            dmsg_protocol::billing::delivery_service_fee(
                black_box(1_000_001),
                black_box(&dmsg_types::payment::DeliveryFeePolicy {
                    version: 1,
                    effective_at_ms: 0,
                    rate_bps: 500,
                    minimum_atomic: 20_000,
                }),
            )
            .unwrap(),
        );
    });
    measure("account_issuer", 10_000, || {
        black_box(account_issuer(black_box(namespace), black_box(&account)));
    });
    measure("parse_account_issuer", 10_000, || {
        black_box(parse_account_issuer(black_box(namespace), black_box(&issuer)).unwrap());
    });
    measure("parse_signing_input", 1_000, || {
        black_box(parse_signing_input(black_box(&tbs)).unwrap());
    });
    measure("match_execution_receipt", 1_000, || {
        match_execution_receipt(black_box(&artifact), black_box(&receipt)).unwrap();
    });
    measure("verification_report", 1_000, || {
        black_box(verification_report(black_box(&artifact), None).unwrap());
    });
    measure("device_validation", 10_000, || {
        black_box(&device).validate().unwrap();
    });
}
