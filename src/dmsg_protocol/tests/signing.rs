use cose2::{iana, Header, Key, Label, Sign1Message, Value, Verifier};
use dmsg_protocol::*;
use dmsg_types::{
    cose::{Algorithm, ExecutionReceipt, ExecutionStatus},
    *,
};
use ed25519_dalek::{Signer, SigningKey};
fn key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}
fn statement() -> Statement {
    Statement {
        issuer: account_issuer("https://dmsg.test/u/", &AccountId([1; 12])).unwrap(),
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
    let account = AccountId([1; 12]);
    assert_eq!(
        parse_account_issuer(
            "https://dmsg.test/u/",
            &account_issuer("https://dmsg.test/u/", &account).unwrap()
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
    assert_eq!(artifact_signing_bytes(&changed).unwrap(), tbs);
    assert_eq!(
        verification_report(&changed, None).unwrap().statement,
        statement()
    );
    let receipt = receipt(&changed);
    match_execution_receipt(&changed, &receipt).unwrap();
    match_signing_result(&changed, &tbs, receipt.public_key_fingerprint).unwrap();
    let timestamped = attach_unverified_timestamp_token(&changed, &[0x30, 0]).unwrap();
    assert_eq!(artifact_signing_bytes(&timestamped).unwrap(), tbs);
    match_execution_receipt(&timestamped, &receipt).unwrap();
    // The local signing entry point requires its own canonical preparation.
    assert!(parse_signing_input(&tbs).is_err());
}

fn receipt(artifact: &SignedArtifact) -> ExecutionReceipt {
    ExecutionReceipt {
        schema: 1,
        account_id: AccountId([1; 12]),
        issuer: statement().issuer,
        request_id: Hash::new([2; 32]),
        device_id: Hash::new([3; 32]),
        security_epoch: 1,
        approved_at: 10,
        expires_at: 20,
        origin: "https://app.test".into(),
        max_cycles: 100,
        to_be_signed_digest: sha256(&artifact_signing_bytes(artifact).unwrap()),
        public_key_fingerprint: key_thumbprint(&artifact.cose_key).unwrap(),
        status: ExecutionStatus::Completed,
        signature_digest: Some(signature_digest(&artifact.cose_sign1).unwrap()),
    }
}

#[test]
fn receipt_paths_use_binary_account_and_request_ids() {
    let account = AccountId([1; 12]);
    let request = Hash::new([2; 32]);
    let path = execution_receipt_key(&account, request);
    assert_eq!(path.len(), 54);
    assert_eq!(&path[..10], b"execution/");
    assert_eq!(&path[10..22], account.as_slice());
    assert_eq!(&path[22..], request.as_slice());
}

#[test]
fn receipts_and_signing_results_bind_every_artifact_field() {
    let artifact = sign(&statement(), b"kid");
    let receipt = receipt(&artifact);
    match_execution_receipt(&artifact, &receipt).unwrap();
    let tbs = artifact_signing_bytes(&artifact).unwrap();
    match_signing_result(&artifact, &tbs, receipt.public_key_fingerprint).unwrap();
    assert_eq!(
        match_signing_result(&artifact, b"other", receipt.public_key_fingerprint),
        Err(Error::IntegrityFailed)
    );
    assert_eq!(
        match_signing_result(&artifact, &tbs, Hash::new([0; 32])),
        Err(Error::IntegrityFailed)
    );
    let changes: &[fn(&mut ExecutionReceipt)] = &[
        |r| r.schema = 2,
        |r| r.issuer = "urn:other:author".into(),
        |r| r.to_be_signed_digest = Hash::new([0; 32]),
        |r| r.public_key_fingerprint = Hash::new([0; 32]),
        |r| r.signature_digest = Some(Hash::new([0; 32])),
        |r| r.signature_digest = None,
    ];
    for change in changes {
        let mut changed = receipt.clone();
        change(&mut changed);
        assert_eq!(
            match_execution_receipt(&artifact, &changed),
            Err(Error::IntegrityFailed)
        );
    }
    for status in [
        ExecutionStatus::Authorized,
        ExecutionStatus::Executing,
        ExecutionStatus::Failed,
        ExecutionStatus::Unknown,
        ExecutionStatus::ResultExpired,
    ] {
        let mut changed = receipt.clone();
        changed.status = status;
        assert_eq!(
            match_execution_receipt(&artifact, &changed),
            Err(Error::IntegrityFailed)
        );
    }
    let timestamped = attach_unverified_timestamp_token(&artifact, &[0x30, 0]).unwrap();
    match_execution_receipt(&timestamped, &receipt).unwrap();
}

#[test]
fn reused_artifact_helpers_still_verify_the_signature() {
    let artifact = sign(&statement(), b"kid");
    let receipt = receipt(&artifact);
    let tbs = artifact_signing_bytes(&artifact).unwrap();
    let mut message = Sign1Message::from_slice(&artifact.cose_sign1).unwrap();
    message.set_signature(vec![0; 64]).unwrap();
    let changed = SignedArtifact {
        cose_sign1: message.to_vec().unwrap().into(),
        ..artifact
    };
    assert!(artifact_signing_bytes(&changed).is_err());
    assert!(match_signing_result(&changed, &tbs, receipt.public_key_fingerprint).is_err());
    assert!(match_execution_receipt(&changed, &receipt).is_err());
    assert!(verification_report(&changed, None).is_err());
    assert!(attach_unverified_timestamp_token(&changed, &[1]).is_err());
}

#[test]
fn statement_subject_text_and_metadata_boundaries() {
    let mut value = statement();
    for subject in [
        None,
        Some("  中文 / subject  ".into()),
        Some("urn:object:1".into()),
        Some("a".repeat(MAX_URI_BYTES)),
    ] {
        value.subject = subject;
        assert_eq!(validate_statement(&value), Ok(()));
    }
    for subject in [
        "".into(),
        "a\n".into(),
        "a\u{85}".into(),
        "has space:uri".into(),
        "a".repeat(MAX_URI_BYTES + 1),
    ] {
        value.subject = Some(subject);
        assert!(validate_statement(&value).is_err());
    }
    value.subject = None;
    for length in [1, 4096] {
        value.content = StatementContent::Text("a".repeat(length));
        assert_eq!(validate_statement(&value), Ok(()));
    }
    for text in [String::new(), "a".repeat(4097), "中".repeat(1366)] {
        value.content = StatementContent::Text(text);
        assert_eq!(validate_statement(&value), Err(Error::QuotaExceeded));
    }
    for media in ["application/pdf", "text/plain; charset=utf-8"] {
        value.content = StatementContent::Digest {
            sha256: sha256(b"x"),
            content_type: Some(media.into()),
            location: Some("urn:file:1".into()),
        };
        assert_eq!(validate_statement(&value), Ok(()));
    }
    for media in [
        "".into(),
        "not a media type".into(),
        format!("text/{}", "a".repeat(252)),
    ] {
        value.content = StatementContent::Digest {
            sha256: sha256(b"x"),
            content_type: Some(media),
            location: None,
        };
        assert!(validate_statement(&value).is_err());
    }
    value.content = StatementContent::Digest {
        sha256: sha256(b"x"),
        content_type: None,
        location: Some("relative".into()),
    };
    assert!(validate_statement(&value).is_err());
}

#[test]
fn preparation_rejects_unsupported_algorithms_bad_keys_and_signature_lengths() {
    assert_eq!(
        cose_algorithm(&Algorithm::VetKdBls12381),
        Err(Error::UnsupportedProtocol)
    );
    assert!(prepare_cose(&statement(), &Algorithm::VetKdBls12381, b"kid").is_err());
    for size in [0, MAX_KID_BYTES + 1] {
        assert!(prepare_cose(&statement(), &Algorithm::Ed25519, &vec![1; size]).is_err());
    }
    let (_, tbs) =
        prepare_cose(&statement(), &Algorithm::Ed25519, &vec![1; MAX_KID_BYTES]).unwrap();
    assert_eq!(parse_signing_input(&tbs).unwrap().kid.len(), MAX_KID_BYTES);
    for size in [0, 63, 65] {
        assert!(finish_cose(&tbs, &key().verifying_key().to_bytes(), vec![0; size]).is_err());
    }
    for size in [0, 31, 33] {
        assert!(public_cose_key(&Algorithm::Ed25519, b"kid", &vec![0; size]).is_err());
    }
    let public = key().verifying_key().to_bytes();
    assert!(public_cose_key(&Algorithm::Ed25519, &[], &public).is_ok());
    assert!(public_cose_key(&Algorithm::Ed25519, &vec![1; MAX_KID_BYTES], &public).is_ok());
    assert!(public_cose_key(&Algorithm::Ed25519, &vec![1; MAX_KID_BYTES + 1], &public).is_err());
    assert!(public_cose_key(&Algorithm::Ed25519, b"kid", &[0; 32]).is_err());
    assert!(public_cose_key(&Algorithm::EcdsaSecp256k1, b"kid", &[0; 33]).is_err());
    assert!(public_cose_key(&Algorithm::VetKdBls12381, b"kid", &[0; 48]).is_err());
}

#[test]
fn signing_input_requires_canonical_framing_empty_aad_and_embedded_payload() {
    let (_, tbs) = prepare_cose(&statement(), &Algorithm::Ed25519, b"kid").unwrap();
    let Value::Array(fields) = cbor2::from_slice::<Value>(&tbs).unwrap() else {
        panic!()
    };
    for (index, replacement) in [
        (0, Value::Text("Signature".into())),
        (1, Value::Bytes(vec![])),
        (2, Value::Bytes(vec![1])),
        (3, Value::Null),
        (3, Value::Text("not a byte string".into())),
    ] {
        let mut changed = fields.clone();
        changed[index] = replacement;
        assert!(parse_signing_input(&canonical(&Value::Array(changed))).is_err());
    }
    let mut nonminimal = vec![0x98, 4];
    nonminimal.extend_from_slice(&tbs[1..]);
    assert!(parse_signing_input(&nonminimal).is_err());
    let mut indefinite = vec![0x9f];
    indefinite.extend_from_slice(&tbs[1..]);
    indefinite.push(0xff);
    assert!(parse_signing_input(&indefinite).is_err());
    let mut trailing = tbs.clone();
    trailing.push(0);
    assert!(parse_signing_input(&trailing).is_err());
    for length in 0..tbs.len() {
        assert!(
            parse_signing_input(&tbs[..length]).is_err(),
            "length={length}"
        );
    }
    assert!(matches!(
        parse_signing_input(&vec![0; MAX_PAYLOAD + 1]),
        Err(Error::QuotaExceeded)
    ));
}

#[test]
fn artifact_framing_and_size_errors_are_distinct() {
    let artifact = sign(&statement(), b"kid");
    for length in 0..artifact.cose_sign1.len() {
        let changed = SignedArtifact {
            cose_sign1: artifact.cose_sign1[..length].to_vec().into(),
            ..artifact.clone()
        };
        assert!(verify_artifact(&changed).is_err(), "length={length}");
    }
    let mut changed = artifact.clone();
    changed.cose_sign1 = artifact.cose_sign1[1..].to_vec().into();
    assert_eq!(verify_artifact(&changed), Err(Error::IntegrityFailed));
    changed.cose_sign1 = vec![0xd2; MAX_ARTIFACT_BYTES + 1].into();
    assert_eq!(verify_artifact(&changed), Err(Error::QuotaExceeded));
    changed = artifact;
    changed.cose_key = vec![0; MAX_COSE_KEY_BYTES + 1].into();
    assert_eq!(verify_artifact(&changed), Err(Error::QuotaExceeded));
    assert_eq!(key_thumbprint(&changed.cose_key), Err(Error::QuotaExceeded));
}

#[test]
fn key_algorithm_operations_curve_and_private_parameters_are_checked() {
    let artifact = sign(&statement(), b"kid");
    let changes: &[fn(&mut Key)] = &[
        |k| {
            k.set_alg(iana::AlgorithmES256K);
        },
        |k| {
            k.set_ops([iana::KeyOperationSign]);
        },
        |k| {
            k.set_kty(iana::KeyTypeEC2);
        },
        |k| {
            k.insert(iana::OKPKeyParameterCrv, iana::EllipticCurveX25519);
        },
        |k| {
            k.insert(iana::OKPKeyParameterX, vec![0; 31]);
        },
        |k| {
            k.insert(iana::OKPKeyParameterX, vec![0; 32]);
        },
        |k| {
            k.insert(iana::OKPKeyParameterD, vec![0; 32]);
        },
    ];
    for change in changes {
        let mut public = Key::from_slice(&artifact.cose_key).unwrap();
        change(&mut public);
        let changed = SignedArtifact {
            cose_key: public.to_vec().unwrap().into(),
            ..artifact.clone()
        };
        assert!(verify_artifact(&changed).is_err());
    }
    // Metadata is optional and not part of the thumbprint, while alg is required for verification.
    let public = Key::from_slice(&artifact.cose_key).unwrap();
    let fields = public
        .iter()
        .filter(|(label, _)| ![Label::Int(2), Label::Int(4)].contains(label))
        .map(|(label, value)| (Value::from(label.clone()), value.clone()))
        .collect();
    let changed = SignedArtifact {
        cose_key: canonical(&Value::Map(fields)).into(),
        ..artifact.clone()
    };
    assert_eq!(verify_artifact(&changed).unwrap(), statement());
    assert_eq!(
        key_thumbprint(&changed.cose_key),
        key_thumbprint(&artifact.cose_key)
    );
}

#[test]
fn timestamp_tokens_have_strict_bounds_and_never_change_signed_bytes() {
    let artifact = sign(&statement(), b"kid");
    assert!(attach_unverified_timestamp_token(&artifact, &[]).is_err());
    assert!(
        attach_unverified_timestamp_token(&artifact, &vec![1; MAX_TIMESTAMP_BYTES + 1]).is_err()
    );
    let timestamped =
        attach_unverified_timestamp_token(&artifact, &vec![1; MAX_TIMESTAMP_BYTES]).unwrap();
    assert!(timestamped.cose_sign1.len() <= MAX_ARTIFACT_BYTES);
    assert_eq!(verify_artifact(&timestamped).unwrap(), statement());
    assert_eq!(
        signature_digest(&timestamped.cose_sign1),
        signature_digest(&artifact.cose_sign1)
    );
    assert_eq!(
        timestamp_imprint(&timestamped.cose_sign1),
        timestamp_imprint(&artifact.cose_sign1)
    );
    for value in [
        Value::Text("token".into()),
        Value::Bytes(vec![]),
        Value::Bytes(vec![1; MAX_TIMESTAMP_BYTES + 1]),
    ] {
        let mut message = Sign1Message::from_slice(&artifact.cose_sign1).unwrap();
        message.unprotected.insert(CTT_HEADER, value);
        let changed = SignedArtifact {
            cose_sign1: message.to_vec().unwrap().into(),
            ..artifact.clone()
        };
        assert!(verify_artifact(&changed).is_err());
    }
    let mut message = Sign1Message::from_slice(&artifact.cose_sign1).unwrap();
    message.unprotected.insert(1000, "unknown");
    let changed = SignedArtifact {
        cose_sign1: message.to_vec().unwrap().into(),
        ..artifact.clone()
    };
    assert!(verify_artifact(&changed).is_err());
    assert_eq!(
        timestamp_imprint(&vec![0; MAX_ARTIFACT_BYTES + 1]),
        Err(Error::QuotaExceeded)
    );
    assert_eq!(
        signature_digest(&vec![0; MAX_ARTIFACT_BYTES + 1]),
        Err(Error::QuotaExceeded)
    );
    assert!(timestamp_imprint(&artifact.cose_sign1[1..]).is_err());
}

#[test]
fn text_reports_compare_exact_bytes_and_do_not_infer_identity_trust() {
    let mut value = statement();
    value.content = StatementContent::Text("  原文\n".into());
    let artifact = sign(&value, b"kid");
    for content in [None, Some("  原文\n".as_bytes())] {
        let report = verification_report(&artifact, content).unwrap();
        assert_eq!(report.content, VerificationStatus::Verified);
        assert_eq!(report.issuer_binding, VerificationStatus::NotChecked);
        assert_eq!(report.authorization, VerificationStatus::NotChecked);
        assert_eq!(report.current_status, VerificationStatus::NotChecked);
        assert_eq!(report.timestamp, VerificationStatus::NotProvided);
    }
    assert!(verification_report(&artifact, Some("原文".as_bytes())).is_err());
}

// Construct externally signed bytes without asking the COSE builder to validate
// the headers first. This exercises the verifier's own rejection paths.
fn signed_headers(
    original: &SignedArtifact,
    change: impl FnOnce(&mut Vec<(Value, Value)>),
) -> SignedArtifact {
    let message = Sign1Message::from_slice(&original.cose_sign1).unwrap();
    let Value::Map(mut headers) = cbor2::from_slice(message.protected_raw()).unwrap() else {
        panic!()
    };
    change(&mut headers);
    let protected = cbor2::to_vec(&Value::Map(headers)).unwrap();
    let tbs =
        Sign1Message::to_be_signed(&protected, &[], message.payload.as_deref().unwrap()).unwrap();
    let wire = Value::Tag(
        18,
        Box::new(Value::Array(vec![
            Value::Bytes(protected),
            Value::Map(vec![]),
            Value::Bytes(message.payload.unwrap()),
            Value::Bytes(key().sign(&tbs).to_bytes().to_vec()),
        ])),
    );
    SignedArtifact {
        cose_sign1: canonical(&wire).into(),
        cose_key: original.cose_key.clone(),
    }
}

#[test]
fn missing_duplicate_and_mistyped_protected_headers_are_rejected() {
    let artifact = sign(&statement(), b"kid");
    for label in [1, 2, 4, 15, 16, 258] {
        let changed = signed_headers(&artifact, |headers| {
            headers.retain(|(key, _)| *key != Value::from(label))
        });
        assert!(verify_artifact(&changed).is_err(), "missing {label}");
    }
    for (label, value) in [
        (1, Value::Null),
        (4, Value::Text("kid".into())),
        (4, Value::Bytes(vec![])),
        (4, Value::Bytes(vec![1; MAX_KID_BYTES + 1])),
        (15, Value::Array(vec![])),
        (16, Value::from(1)),
        (258, Value::Text("sha256".into())),
        (259, Value::from(1)),
        (260, Value::from(1)),
        (2, Value::Array(vec![15.into(), 16.into(), 16.into()])),
    ] {
        let changed = signed_headers(&artifact, |headers| {
            headers.retain(|(key, _)| *key != Value::from(label));
            headers.push((label.into(), value));
        });
        assert!(verify_artifact(&changed).is_err(), "mistyped {label}");
    }
    let changed = signed_headers(&artifact, |headers| headers.push(headers[0].clone()));
    assert!(verify_artifact(&changed).is_err());
    let changed = signed_headers(&artifact, |headers| {
        headers.push((Value::Text("unknown".into()), Value::Null))
    });
    assert!(verify_artifact(&changed).is_err());
}

#[test]
fn claims_require_unique_supported_fields_with_exact_types() {
    let artifact = sign(&statement(), b"kid");
    let issuer = (Value::from(1), Value::Text(statement().issuer));
    for claims in [
        vec![],
        vec![(1.into(), Value::from(42))],
        vec![issuer.clone(), issuer.clone()],
        vec![issuer.clone(), (2.into(), Value::from(42))],
        vec![issuer.clone(), (6.into(), Value::Text("1800000000".into()))],
        vec![issuer.clone(), (6.into(), Value::from(u64::MAX))],
        vec![issuer.clone(), (4.into(), Value::from(42))],
        vec![
            issuer.clone(),
            (2.into(), Value::Text("a".into())),
            (2.into(), Value::Text("b".into())),
        ],
        vec![issuer.clone(), (6.into(), 1.into()), (6.into(), 2.into())],
    ] {
        let changed = signed_headers(&artifact, |headers| {
            headers
                .iter_mut()
                .find(|(key, _)| *key == Value::from(CWT_CLAIMS))
                .unwrap()
                .1 = Value::Map(claims);
        });
        assert!(verify_artifact(&changed).is_err());
    }
    for at in [i64::MIN, -1, 0, i64::MAX] {
        let mut value = statement();
        value.issued_at = Some(at);
        assert_eq!(verify_artifact(&sign(&value, b"kid")).unwrap(), value);
    }
}

#[test]
fn text_profiles_reject_invalid_utf8_and_conflicting_headers() {
    let mut value = statement();
    value.content = StatementContent::Text("text".into());
    let artifact = sign(&value, b"kid");
    for changed in [
        resigned(&artifact, |_, p| *p = vec![0xff]),
        resigned(&artifact, |_, p| p.clear()),
        resigned(&artifact, |_, p| *p = vec![b'a'; 4097]),
        resigned(&artifact, |h, _| {
            h.set_content_type("text/html");
        }),
        resigned(&artifact, |h, _| {
            h.insert(HASH_ALGORITHM, iana::AlgorithmSHA_256);
        }),
    ] {
        assert!(verify_artifact(&changed).is_err());
    }
}

#[test]
fn secp256k1_verification_checks_coordinates_curve_and_signature() {
    use k256::ecdsa::{Signature as EcSignature, SigningKey as EcKey};
    let signer = EcKey::from_bytes((&[7; 32]).into()).unwrap();
    let (_, tbs) = prepare_cose(&statement(), &Algorithm::EcdsaSecp256k1, b"kid").unwrap();
    let signature: EcSignature = signer.sign(&tbs);
    let public = signer.verifying_key().to_sec1_point(false);
    let artifact = finish_cose(&tbs, public.as_bytes(), signature.to_bytes().to_vec()).unwrap();
    let changes: &[fn(&mut Key)] = &[
        |k| {
            k.set_kty(iana::KeyTypeOKP);
        },
        |k| {
            k.insert(iana::EC2KeyParameterCrv, iana::EllipticCurveP_256);
        },
        |k| {
            k.insert(iana::EC2KeyParameterX, vec![0; 31]);
        },
        |k| {
            k.insert(iana::EC2KeyParameterY, vec![0; 31]);
        },
        |k| {
            k.insert(iana::EC2KeyParameterY, vec![0; 32]);
        },
        |k| {
            k.insert(iana::EC2KeyParameterY, Value::Null);
        },
        |k| {
            k.insert(iana::EC2KeyParameterD, vec![0; 32]);
        },
    ];
    for change in changes {
        let mut key = Key::from_slice(&artifact.cose_key).unwrap();
        change(&mut key);
        let changed = SignedArtifact {
            cose_key: key.to_vec().unwrap().into(),
            ..artifact.clone()
        };
        assert!(verify_artifact(&changed).is_err());
    }
    for signature in [vec![0; 63], vec![0; 64], vec![1; 64]] {
        let mut message = Sign1Message::from_slice(&artifact.cose_sign1).unwrap();
        message.set_signature(signature).unwrap();
        let changed = SignedArtifact {
            cose_sign1: message.to_vec().unwrap().into(),
            ..artifact.clone()
        };
        assert!(verify_artifact(&changed).is_err());
    }
}
