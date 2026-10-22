use super::*;
use crate::cloud::{Signer, sign_http};
use dmsg_types::Hash;
use serde_json::json;

#[test]
fn grants_must_match_the_verified_file_and_slot() {
    let object = format!("/v1/accounts/test/objects/{}", "a".repeat(64));
    let part = Slot {
        url: "https://example.test/unused".into(),
        digest: "b".repeat(64),
        size: 4,
    };
    let grant = Grant {
        upload_id: "a".repeat(64),
        expires_at: 1234,
        manifest: part.clone(),
        chunks: vec![part],
    };
    assert!(grant.slot(&object, "0", &"b".repeat(64)).is_ok());
    assert!(grant.slot(&object, "1", &"b".repeat(64)).is_err());
    assert!(grant.slot(&object, "0", &"c".repeat(64)).is_err());
    assert!(
        grant
            .slot(
                &object.replace(&"a".repeat(64), &"d".repeat(64)),
                "0",
                &"b".repeat(64)
            )
            .is_err()
    );
}

/// Independent public HTTP PoP and download response shapes for the relay interop suite.
#[test]
#[ignore = "fixture producer, run explicitly with DMSG_DOWNLOAD_VECTOR_OUT"]
fn print_download_vector() {
    let account = "040g2081040g2081040g";
    let upload = "ab".repeat(32);
    let seed = [7u8; 32];
    let deadline = 1_900_000_060_000u64;
    let signer = Signer {
        issuer: "https://dmsg.test/u/040g2081040g2081040g",
        device_id: Hash::new([7; 32]),
        security_epoch: 1,
        seed: &seed,
        expires_at: deadline,
    };
    let target = format!("/v1/accounts/{account}/objects/{upload}/download");
    let url = url::Url::parse(&format!("https://dmsg.test{target}")).unwrap();
    let pop = sign_http(&signer, &url, "GET", &[], &"cd".repeat(32), deadline).unwrap();
    let response = json!({"upload_id":upload,"expires_at":deadline,"manifest":{"size":3,"digest":"11".repeat(32),"url":"https://11111111111111111111111111111111.r2.cloudflarestorage.com/bucket/manifest?X-Amz-Signature=test"},"chunks":[{"size":4,"digest":"22".repeat(32),"url":"https://11111111111111111111111111111111.r2.cloudflarestorage.com/bucket/0?X-Amz-Signature=test"}]});
    let grant: Grant = serde_json::from_value(response.clone()).unwrap();
    assert_eq!(grant.chunks[0].size, 4);
    let output = json!({"test_only":true,"account":account,"target":target,"deadline":deadline,"pop":pop,"public_key":hex::encode(crate::crypto::signing_public(&seed)),"response":response});
    std::fs::write(
        std::env::var("DMSG_DOWNLOAD_VECTOR_OUT").unwrap(),
        serde_json::to_vec_pretty(&output).unwrap(),
    )
    .unwrap();
}
