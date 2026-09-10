use super::*;
use ed25519_dalek::{Signer, SigningKey};
use serde_bytes::ByteBuf;

#[test]
fn canonical_decode_returns_an_error_when_reencoding_fails() {
    #[derive(Debug, serde::Deserialize)]
    struct DecodeOnly;
    impl Serialize for DecodeOnly {
        fn serialize<S: serde::Serializer>(&self, _: S) -> std::result::Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("not serializable"))
        }
    }
    assert_eq!(
        decode_canonical::<DecodeOnly>(&[0xf6]).unwrap_err(),
        Error::IntegrityFailed
    );
}

#[test]
fn canonical_decode_rejects_noncanonical_and_oversized_inputs() {
    assert_eq!(decode_canonical::<u64>(&canonical(&u64::MAX)), Ok(u64::MAX));
    assert_eq!(
        decode_canonical::<u128>(&canonical(&u128::MAX)),
        Ok(u128::MAX)
    );
    for bytes in [vec![], vec![0x18, 1], vec![1, 1], vec![0x1b, 0], vec![0xff]] {
        assert_eq!(decode_canonical::<u64>(&bytes), Err(Error::IntegrityFailed));
    }
    assert_eq!(
        decode_canonical::<Vec<u8>>(&[0x9f, 1, 0xff]),
        Err(Error::IntegrityFailed)
    );
    // A deserializer may collapse duplicate map keys; reencoding must detect that.
    assert_eq!(
        decode_canonical::<std::collections::BTreeMap<u8, u8>>(&[0xa2, 1, 2, 1, 3]),
        Err(Error::IntegrityFailed)
    );
    let exact = canonical(&ByteBuf::from(vec![0; MAX_PAYLOAD - 3]));
    assert_eq!(exact.len(), MAX_PAYLOAD);
    assert!(decode_canonical::<ByteBuf>(&exact).is_ok());
    assert_eq!(
        decode_canonical::<ByteBuf>(&vec![0; MAX_PAYLOAD + 1]),
        Err(Error::QuotaExceeded)
    );
}

#[test]
fn authentication_expiry_and_sequence_boundaries() {
    assert_eq!(
        authenticated(Principal::anonymous()),
        Err(Error::AuthRequired)
    );
    assert_eq!(
        authenticated(Principal::management_canister()),
        Err(Error::AuthRequired)
    );
    assert_eq!(authenticated(Principal::from_slice(&[1])), Ok(()));
    for (now, end, maximum, expected) in [
        (10, 11, 1, Ok(())),
        (10, 20, 10, Ok(())),
        (10, 20, 9, Err(Error::Expired)),
        (10, 10, 10, Err(Error::Expired)),
        (11, 10, 10, Err(Error::Expired)),
        (10, 11, 0, Err(Error::Expired)),
        (u64::MAX - 1, u64::MAX, 1, Ok(())),
        (u64::MAX, 0, u64::MAX, Err(Error::Expired)),
    ] {
        assert_eq!(expiry(now, end, maximum), expected);
    }
    assert_eq!(check_sequence(0, 0), Ok(()));
    assert_eq!(check_sequence(1, 0), Err(Error::ResultExpired));
    assert_eq!(check_sequence(1, 2), Err(Error::VersionConflict));
    assert_eq!(check_sequence(u64::MAX - 1, u64::MAX - 1), Ok(()));
    assert_eq!(
        check_sequence(u64::MAX, u64::MAX),
        Err(Error::VersionConflict)
    );
    assert_eq!(check_sequence(u64::MAX, 0), Err(Error::ResultExpired));
}

#[test]
fn nonzero_and_transport_keys_reject_invalid_points() {
    assert!(nonzero(&[]).is_err());
    assert!(nonzero(&[0; 32]).is_err());
    assert_eq!(nonzero(&[0, 1, 0]), Ok(()));
    assert_eq!(
        validate_transport_key(&ic_bls12_381::G1Affine::generator().to_compressed()),
        Ok(())
    );
    for bytes in [
        vec![],
        vec![0; 47],
        vec![0; 49],
        vec![0; 48],
        vec![255; 48],
        ic_bls12_381::G1Affine::identity().to_compressed().to_vec(),
    ] {
        assert!(validate_transport_key(&bytes).is_err());
    }
}

#[test]
fn ed25519_rejects_tampering_bad_lengths_and_weak_keys() {
    let signer = SigningKey::from_bytes(&[7; 32]);
    let public = signer.verifying_key().to_bytes().into();
    let signature = signer.sign(b"message").to_bytes();
    assert_eq!(verify(&public, b"message", &signature), Ok(()));
    assert_eq!(
        verify(&public, b"changed", &signature),
        Err(Error::IntegrityFailed)
    );
    for size in [0, 63, 65] {
        assert_eq!(
            verify(&public, b"message", &vec![0; size]),
            Err(Error::IntegrityFailed)
        );
    }
    let mut identity = [0; 32];
    identity[0] = 1;
    assert_eq!(
        verify(&identity.into(), b"message", &[0; 64]),
        Err(Error::IntegrityFailed)
    );
}

#[test]
fn execution_ids_bind_every_component() {
    let account = AccountId([1; 12]);
    let device = Hash::new([2; 32]);
    let expected = execution_request_id(&account, 3, device, 4);
    assert_ne!(
        execution_request_id(&AccountId([2; 12]), 3, device, 4),
        expected
    );
    assert_ne!(execution_request_id(&account, 4, device, 4), expected);
    assert_ne!(
        execution_request_id(&account, 3, Hash::new([3; 32]), 4),
        expected
    );
    assert_ne!(execution_request_id(&account, 3, device, 5), expected);
    assert_ne!(digest("other", &(&account, 3u64, device, 4u64)), expected);
    assert_eq!(
        hex::encode(sha256(b"abc").as_slice()),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
#[test]
fn signature_domains_and_canonical_encoding_are_bound() {
    let sk = SigningKey::from_bytes(&[7; 32]);
    let p = Principal::from_slice(&[1]);
    let mut a = Approval {
        device_id: Hash::new([2; 32]),
        security_epoch: 1,
        sequence: 0,
        request_id: Hash::new([3; 32]),
        expires_at: 100,
        signature: ByteBuf::new(),
    };
    let msg = approval_message(p, &AccountId([4; 12]), "root", &42u64, &a);
    a.signature = sk.sign(msg.as_slice()).to_bytes().to_vec().into();
    assert!(verify(
        &sk.verifying_key().to_bytes().into(),
        msg.as_slice(),
        &a.signature
    )
    .is_ok());
    assert!(verify(
        &sk.verifying_key().to_bytes().into(),
        approval_message(p, &AccountId([4; 12]), "sign", &42u64, &a).as_slice(),
        &a.signature
    )
    .is_err());
    assert_eq!(
        hex::encode(canonical(&(1u8, "dmsg", 42u64))),
        "830164646d7367182a"
    );
    assert!(decode_canonical::<u64>(&[0x18, 0x01]).is_err());
    assert!(decode_canonical::<u64>(&[0x01, 0x01]).is_err());
}
