use cose2::{iana, Header, Key, Label, Sign1Message, Value, Verifier};
use dmsg_protocol::*;
use dmsg_types::{cose::Algorithm, *};
use ed25519_dalek::{Signer, SigningKey};
fn key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}
fn statement() -> Statement {
    Statement {
        issuer: account_issuer("https://dmsg.test/u/", AccountId::new([1; 12])).unwrap(),
        subject: Some("release/spec".into()),
        issued_at: Some(1_800_000_000),
        content: StatementContent::Digest {
            sha256: sha256(b"document"),
            content_type: Some("application/pdf".into()),
            location: None,
        },
    }
}
fn sign(statement: &Statement, kid: &[u8]) -> SignedArtifact {
    let (_, tbs) = prepare_cose(statement, &Algorithm::Ed25519, kid).unwrap();
    finish_cose(
        &tbs,
        &key().verifying_key().to_bytes(),
        key().sign(&tbs).to_bytes().to_vec(),
    )
    .unwrap()
}
struct Aware(cose2::ed25519::Ed25519Verifier);
impl Verifier for Aware {
    fn alg(&self) -> Option<Label> {
        self.0.alg()
    }
    fn understood_critical_headers(&self) -> &[Label] {
        &[Label::Int(15), Label::Int(16), Label::Int(258)]
    }
    fn verify(&self, data: &[u8], sig: &[u8]) -> std::result::Result<(), cose2::Error> {
        self.0.verify(data, sig)
    }
}
#[test]
fn standard_cose_verification_and_variable_identifiers() {
    for size in [12, 29, 32, 64] {
        let artifact = sign(&statement(), &vec![9; size]);
        let cose_key = Key::from_slice(&artifact.cose_key).unwrap();
        let verifier = Aware(cose2::ed25519::Ed25519Verifier::from_cose_key(&cose_key).unwrap());
        let message =
            Sign1Message::verify_and_decode(&verifier, &artifact.cose_sign1, None).unwrap();
        assert_eq!(message.payload.unwrap(), sha256(b"document").to_vec());
        assert_eq!(verify_artifact(&artifact).unwrap(), statement());
        assert_eq!(
            verification_report(&artifact, Some(b"document"))
                .unwrap()
                .content,
            VerificationStatus::Verified
        );
        assert_eq!(
            verification_report(&artifact, None).unwrap().content,
            VerificationStatus::NotProvided
        );
        assert!(verification_report(&artifact, Some(b"different")).is_err());
    }
    assert_eq!(
        principal_issuer(
            "https://id.test/ic/mainnet/",
            candid::Principal::management_canister()
        )
        .unwrap(),
        "https://id.test/ic/mainnet/aaaaa-aa"
    );
    let account = AccountId::new([1; 12]);
    assert_eq!(
        parse_account_issuer(
            "https://dmsg.test/u/",
            &account_issuer("https://dmsg.test/u/", account).unwrap()
        )
        .unwrap(),
        account
    );
    assert!(parse_account_issuer("https://other.test/u/", &statement().issuer).is_err());
}
fn resigned(
    original: &SignedArtifact,
    change: impl FnOnce(&mut Header, &mut Vec<u8>),
) -> SignedArtifact {
    let old = Sign1Message::from_slice(&original.cose_sign1).unwrap();
    let mut payload = old.payload.unwrap();
    let mut headers = old.protected;
    change(&mut headers, &mut payload);
    let mut message = Sign1Message::new(Some(payload));
    message.protected = headers;
    let tbs = message.prepare_signature(None, None, None).unwrap();
    message
        .set_signature(key().sign(&tbs).to_bytes().to_vec())
        .unwrap();
    SignedArtifact {
        cose_sign1: message.to_vec().unwrap().into(),
        cose_key: original.cose_key.clone(),
    }
}
#[test]
fn unknown_profiles_claims_and_hash_header_conflicts_are_rejected() {
    let artifact = sign(&statement(), &[8; 12]);
    for changed in [
        resigned(&artifact, |h, _| {
            h.insert(TYPE_HEADER, "application/unknown+cose");
        }),
        resigned(&artifact, |h, _| {
            h.set_content_type("application/pdf");
        }),
        resigned(&artifact, |h, _| {
            h.insert(HASH_ALGORITHM, -44);
        }),
        resigned(&artifact, |h, _| {
            h.set_crit([15, 16]);
        }),
        resigned(&artifact, |h, _| {
            h.insert(1000, "unknown");
        }),
        resigned(&artifact, |_, p| {
            p.pop();
        }),
        resigned(&artifact, |h, _| {
            let Some(Value::Map(mut claims)) = h.get(CWT_CLAIMS).cloned() else {
                panic!()
            };
            claims.push((Value::from(4), Value::from(1_800_000_000u64))); // exp is not a document claim
            h.insert(CWT_CLAIMS, Value::Map(claims));
        }),
        resigned(&artifact, |h, _| {
            h.set_alg("dmsg:bip340-sha256");
        }),
    ] {
        assert!(verify_artifact(&changed).is_err());
    }
}
#[test]
fn public_keys_and_signed_bytes_cannot_be_substituted() {
    let artifact = sign(&statement(), &[8; 12]);
    let mut changed = artifact.clone();
    let mut wire: Value = cbor2::from_slice(&changed.cose_sign1).unwrap();
    let Value::Tag(18, ref mut body) = wire else {
        panic!()
    };
    let Value::Array(ref mut fields) = **body else {
        panic!()
    };
    fields[2] = Value::Bytes(sha256(b"other").to_vec());
    changed.cose_sign1 = canonical(&wire).into();
    assert!(verify_artifact(&changed).is_err());
    let mut public = Key::from_slice(&artifact.cose_key).unwrap();
    let thumb = key_thumbprint(&artifact.cose_key).unwrap();
    public.set_kid(vec![4; 29]);
    assert_eq!(key_thumbprint(&public.to_vec().unwrap()).unwrap(), thumb);
    changed = artifact.clone();
    changed.cose_key = public.to_vec().unwrap().into();
    assert!(verify_artifact(&changed).is_err());
    public.insert(iana::OKPKeyParameterD, vec![7; 32]);
    changed.cose_key = public.to_vec().unwrap().into();
    assert!(verify_artifact(&changed).is_err());
}
#[test]
fn text_is_original_utf8_and_contains_no_execution_fields() {
    let statement = Statement {
        issuer: "urn:example:author".into(),
        subject: None,
        issued_at: None,
        content: StatementContent::Text("  原文\nunchanged  ".into()),
    };
    let artifact = sign(&statement, b"short-key-id");
    let message = Sign1Message::from_slice(&artifact.cose_sign1).unwrap();
    assert_eq!(message.payload.unwrap(), "  原文\nunchanged  ".as_bytes());
    assert!(!message.protected.contains_key(HASH_ALGORITHM));
    assert_eq!(verify_artifact(&artifact).unwrap(), statement);
    assert_eq!(sign(&statement, b"short-key-id"), artifact);
}
#[test]
fn adding_unverified_tsa_material_never_promotes_trust() {
    let artifact = sign(&statement(), b"kid");
    let before = timestamp_imprint(&artifact.cose_sign1).unwrap();
    let timestamped = attach_unverified_timestamp_token(&artifact, &[0x30, 0]).unwrap();
    assert_ne!(timestamped, artifact);
    assert_eq!(timestamp_imprint(&timestamped.cose_sign1).unwrap(), before);
    assert_eq!(
        artifact_signing_bytes(&artifact).unwrap(),
        artifact_signing_bytes(&timestamped).unwrap()
    );
    let report = verification_report(&timestamped, Some(b"document")).unwrap();
    assert_eq!(report.signature, VerificationStatus::Verified);
    assert_eq!(report.timestamp, VerificationStatus::NotChecked);
    assert_eq!(report.issuer_binding, VerificationStatus::NotChecked);
    assert_eq!(report.authorization, VerificationStatus::NotChecked);
    assert!(attach_unverified_timestamp_token(&timestamped, &[0x30, 0]).is_err());
}
#[test]
fn rfc9921_imprint_includes_the_cbor_byte_string_header() {
    // Published RFC 9921 section 3.1.1 example, not generated by this crate.
    let signature = hex::decode("8eb33e4ca31d1c465ab05aac34cc6b23d58fef5c083106c4d25a91aef0b0117e2af9a291aa32e14ab834dc56ed2a223444547e01f11d3b0916e5a4c345cacb36").unwrap();
    let mut message = Sign1Message::new(Some(b"This is the content.".to_vec()));
    message
        .prepare_signature(Some(iana::AlgorithmES256.into()), None, None)
        .unwrap();
    message.set_signature(signature.clone()).unwrap();
    let imprint = timestamp_imprint(&message.to_vec().unwrap()).unwrap();
    assert_eq!(
        hex::encode(imprint.as_slice()),
        "44c2419d131d53d55584b5dd33b788c24e551c6d44b1afc8b2b85e6954763b4e"
    );
    assert_ne!(imprint, sha256(&signature));
}

#[test]
fn es256k_compressed_keys_have_the_same_rfc9679_thumbprint() {
    use k256::ecdsa::{Signature as EcSignature, SigningKey as EcKey};
    let signer = EcKey::from_bytes((&[7; 32]).into()).unwrap();
    let (_, tbs) = prepare_cose(&statement(), &Algorithm::EcdsaSecp256k1, b"ec-key").unwrap();
    let signature: EcSignature = signer.sign(&tbs);
    let public = signer.verifying_key().to_sec1_point(false);
    let mut artifact = finish_cose(&tbs, public.as_bytes(), signature.to_bytes().to_vec()).unwrap();
    verify_artifact(&artifact).unwrap();
    let thumb = key_thumbprint(&artifact.cose_key).unwrap();
    let mut key = Key::from_slice(&artifact.cose_key).unwrap();
    key.insert(-3, Value::Bool(public.y().unwrap()[31] & 1 == 1));
    artifact.cose_key = key.to_vec().unwrap().into();
    verify_artifact(&artifact).unwrap();
    assert_eq!(key_thumbprint(&artifact.cose_key).unwrap(), thumb);
    key.set_kid(b"unrelated".to_vec());
    assert_eq!(key_thumbprint(&key.to_vec().unwrap()).unwrap(), thumb);
}

#[test]
fn rfc9679_published_thumbprint_and_strict_uri_inputs() {
    let encoded = hex::decode("a40102200121582065eda5a12577c2bae829437fe338701a10aaa375e1bb5b5de108de439c08551d2258201e52ed75701163f7f9e40ddf9f341b3dc9ba860af7e0ca7ca7e9eecd0084d19c").unwrap();
    assert_eq!(
        hex::encode(key_thumbprint(&encoded).unwrap().as_slice()),
        "496bd8afadf307e5b08c64b0421bf9dc01528a344a43bda88fadd1669da253ec"
    );
    for invalid in [
        "relative",
        "https://USER:pass@example.com/",
        "https://EXAMPLE.com/",
        "https://example.com/%xx",
        "https://example.com/%",
    ] {
        assert!(validate_uri(invalid).is_err(), "{invalid}");
    }
    for valid in [
        "urn:example:person",
        "did:example:alice",
        "https://example.com/%E4%B8%AD",
    ] {
        validate_uri(valid).unwrap();
    }
}

#[test]
fn verification_preserves_original_protected_map_order() {
    let artifact = sign(&statement(), b"kid");
    let old = Sign1Message::from_slice(&artifact.cose_sign1).unwrap();
    let Value::Map(mut entries) = cbor2::from_slice(old.protected_raw()).unwrap() else {
        panic!()
    };
    entries.reverse();
    let protected = cbor2::to_vec(&Value::Map(entries)).unwrap();
    assert_ne!(protected, old.protected_raw());
    let tbs = Sign1Message::to_be_signed(&protected, &[], old.payload.as_deref().unwrap()).unwrap();
    let wire = Value::Tag(
        18,
        Box::new(Value::Array(vec![
            Value::Bytes(protected),
            Value::Map(vec![]),
            Value::Bytes(old.payload.unwrap()),
            Value::Bytes(key().sign(&tbs).to_bytes().to_vec()),
        ])),
    );
    let changed = SignedArtifact {
        cose_sign1: canonical(&wire).into(),
        cose_key: artifact.cose_key,
    };
    assert_eq!(verify_artifact(&changed).unwrap(), statement());
    // The local signing entry point requires its own canonical preparation.
    assert!(parse_signing_input(&tbs).is_err());
}
