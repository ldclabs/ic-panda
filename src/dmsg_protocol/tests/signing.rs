use cose2::{iana, Header, Key, Label, Sign1Message, Value, Verifier};
use dmsg_protocol::*;
use dmsg_types::*;
use ed25519_dalek::{Signer, SigningKey};

fn key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}

fn statement() -> Statement {
    Statement {
        issuer: account_issuer("https://dmsg.test/u/", &AccountId([1; 12])),
        subject: Some("release/spec".into()),
        issued_at: Some(1_800_000_000),
        content: StatementContent::Digest {
            sha256: sha256(b"document"),
            content_type: Some("application/pdf".into()),
            location: None,
        },
    }
}

/// Sign `prepare_cose` output with any kid, as an external signer does.
fn sign(statement: &Statement, kid: &[u8]) -> SignedArtifact {
    let (mut message, tbs) = prepare_cose(statement, kid).unwrap();
    message
        .set_signature(key().sign(&tbs).to_bytes().to_vec())
        .unwrap();
    SignedArtifact {
        cose_sign1: message.to_vec().unwrap().into(),
        cose_key: public_cose_key(kid, &key().verifying_key().to_bytes())
            .unwrap()
            .into(),
    }
}

/// Sig_structure with the original protected bytes and empty external AAD.
fn signing_bytes(artifact: &SignedArtifact) -> Vec<u8> {
    let message = Sign1Message::from_slice(&artifact.cose_sign1).unwrap();
    Sign1Message::to_be_signed(
        message.protected_raw(),
        &[],
        message.payload.as_deref().unwrap(),
    )
    .unwrap()
}

/// Attach opaque CTT material at unprotected header 270 without signing again.
fn with_timestamp(artifact: &SignedArtifact, token: &[u8]) -> SignedArtifact {
    let mut message = Sign1Message::from_slice(&artifact.cose_sign1).unwrap();
    message.unprotected.insert(CTT_HEADER, token.to_vec());
    SignedArtifact {
        cose_sign1: message.to_vec().unwrap().into(),
        cose_key: artifact.cose_key.clone(),
    }
}

fn file_statement() -> Statement {
    Statement {
        content: StatementContent::FileStatement {
            text: "  第三章需要补充实验数据。\n".into(),
            sha256: sha256(b"document"),
            content_type: Some("application/pdf".into()),
            location: Some("urn:example:report".into()),
        },
        ..statement()
    }
}

#[test]
fn prepared_signatures_verify_for_every_profile() {
    for statement in [
        statement(),
        file_statement(),
        Statement {
            content: StatementContent::Text("x".repeat(4096)),
            ..statement()
        },
    ] {
        let public = Hash::new(key().verifying_key().to_bytes());
        let prepared = prepare_attestation(&statement, &public).unwrap();
        let signature = key().sign(&prepared.to_be_signed).to_bytes();
        let artifact = prepared.finish(&signature).unwrap();
        assert_eq!(verify_artifact(&artifact).unwrap(), statement);
    }
}

#[test]
fn file_statements_jointly_bind_text_and_file() {
    let value = file_statement();
    assert_eq!(statement_purpose(&value), KeyPurpose::Statement);
    {
        let (message, _) = prepare_cose(&value, b"kid").unwrap();
        assert_eq!(
            message.protected.get(16),
            Some(&Value::from(FILE_STATEMENT_PROFILE))
        );
        assert_eq!(
            message.protected.get(3),
            Some(&Value::from(FILE_STATEMENT_CONTENT_TYPE))
        );
        for header in [258, 259, 260] {
            assert!(!message.protected.contains_key(header));
        }
        let artifact = sign(&value, b"kid");
        assert_eq!(verify_artifact(&artifact).unwrap(), value);
        // Mutate each signed field without signing again, including optional metadata.
        for field in 1..=4 {
            let mut changed = Sign1Message::from_slice(&artifact.cose_sign1).unwrap();
            let Value::Map(mut fields) =
                cbor2::from_slice(changed.payload.as_ref().unwrap()).unwrap()
            else {
                panic!()
            };
            let entry = fields
                .iter_mut()
                .find(|(k, _)| *k == Value::from(field))
                .unwrap();
            entry.1 = match field {
                1 => Value::from("I approve this report."),
                2 => Value::Bytes(sha256(b"other").to_vec()),
                3 => Value::from("text/plain"),
                _ => Value::from("urn:example:other"),
            };
            changed.payload = Some(canonical(&Value::Map(fields)));
            let changed = SignedArtifact {
                cose_sign1: changed.to_vec().unwrap().into(),
                ..artifact.clone()
            };
            assert!(verify_artifact(&changed).is_err());
        }
    }
}

#[test]
fn file_statement_schema_is_closed_bounded_and_canonical_even_when_signed() {
    let artifact = sign(&file_statement(), b"kid");
    let fields = vec![
        (Value::from(1), Value::from("opinion")),
        (Value::from(2), Value::Bytes(sha256(b"document").to_vec())),
    ];
    let minimal = canonical(&Value::Map(fields.clone()));
    let valid = resigned(&artifact, |_, payload| *payload = minimal.clone());
    assert!(verify_artifact(&valid).is_ok());
    for bad in [
        Value::Map(vec![fields[0].clone()]),
        Value::Map(vec![fields[1].clone()]),
        Value::Map(vec![
            fields[0].clone(),
            fields[0].clone(),
            fields[1].clone(),
        ]),
        Value::Map(vec![
            fields[0].clone(),
            fields[1].clone(),
            (3.into(), Value::Null),
        ]),
        Value::Map(vec![
            fields[0].clone(),
            fields[1].clone(),
            (5.into(), Value::Bool(true)),
        ]),
        Value::Map(vec![(1.into(), Value::from("")), fields[1].clone()]),
        Value::Map(vec![
            (1.into(), Value::from("中".repeat(1366))),
            fields[1].clone(),
        ]),
        Value::Map(vec![
            fields[0].clone(),
            (2.into(), Value::Bytes(vec![0; 31])),
        ]),
        Value::Map(vec![
            fields[0].clone(),
            (2.into(), Value::from("00".repeat(32))),
        ]),
        Value::Map(vec![
            fields[0].clone(),
            fields[1].clone(),
            (3.into(), Value::from("invalid")),
        ]),
        Value::Map(vec![
            fields[0].clone(),
            fields[1].clone(),
            (4.into(), Value::from("relative")),
        ]),
    ] {
        let changed = resigned(&artifact, |_, payload| {
            *payload = cbor2::to_vec(&bad).unwrap()
        });
        assert!(verify_artifact(&changed).is_err(), "{bad:?}");
    }
    let mut reversed = fields.clone();
    reversed.reverse();
    let mut nonminimal = minimal.clone();
    nonminimal.splice(1..2, [0x18, 0x01]);
    for bad in [
        cbor2::to_vec(&Value::Map(reversed)).unwrap(),
        nonminimal,
        [minimal.clone(), vec![0]].concat(),
        vec![0; MAX_FILE_STATEMENT_BYTES + 1],
    ] {
        assert!(verify_artifact(&resigned(&artifact, |_, payload| *payload = bad)).is_err());
    }
    for header in [258, 259, 260] {
        assert!(verify_artifact(&resigned(&artifact, |headers, _| {
            headers.insert(header, -16);
        }))
        .is_err());
    }
    for profile in [TEXT_PROFILE, DIGEST_PROFILE, "application/unknown+cose"] {
        assert!(verify_artifact(&resigned(&artifact, |headers, _| {
            headers.insert(16, profile);
        }))
        .is_err());
    }
    let mut boundary = file_statement();
    if let StatementContent::FileStatement {
        text,
        content_type,
        location,
        ..
    } = &mut boundary.content
    {
        *text = "a".repeat(4096);
        *content_type = Some(format!("text/{}", "a".repeat(251)));
        *location = Some(format!("urn:{}", "a".repeat(MAX_URI_BYTES - 4)));
    }
    assert_eq!(verify_artifact(&sign(&boundary, b"kid")).unwrap(), boundary);
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
    }
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
fn rfc9679_thumbprints_cover_okp_keys_only_and_uris_are_strict() {
    // The RFC 9679 published EC2 example is no longer a supported key type.
    let encoded = hex::decode("a40102200121582065eda5a12577c2bae829437fe338701a10aaa375e1bb5b5de108de439c08551d2258201e52ed75701163f7f9e40ddf9f341b3dc9ba860af7e0ca7ca7e9eecd0084d19c").unwrap();
    assert_eq!(key_thumbprint(&encoded), Err(Error::UnsupportedProtocol));
    let artifact = sign(&statement(), b"kid");
    let key = Key::from_slice(&artifact.cose_key).unwrap();
    let mut required = Key::new();
    for label in [1, -1, -2] {
        required.insert(label, key.get(label).unwrap().clone());
    }
    assert_eq!(
        key_thumbprint(&artifact.cose_key).unwrap(),
        sha256(&required.to_vec().unwrap())
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
    let receipt = receipt(&changed);
    // The receipt binds the Sig_structure over the original protected bytes.
    assert_eq!(receipt.to_be_signed_digest, sha256(&tbs));
    match_execution_receipt(&changed, &receipt).unwrap();
    let timestamped = with_timestamp(&changed, &[0x30, 0]);
    match_execution_receipt(&timestamped, &receipt).unwrap();
}

fn receipt(artifact: &SignedArtifact) -> ExecutionReceipt {
    ExecutionReceipt {
        schema: 2,
        account_id: AccountId([1; 12]),
        issuer: statement().issuer,
        request_id: Hash::new([2; 32]),
        device_id: Hash::new([3; 32]),
        security_epoch: 1,
        approved_at: 10,
        expires_at: 20,
        origin: "https://app.test".into(),
        to_be_signed_digest: sha256(&signing_bytes(artifact)),
        public_key_fingerprint: key_thumbprint(&artifact.cose_key).unwrap(),
        signature_digest: signature_digest(&artifact.cose_sign1).unwrap(),
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
fn receipts_bind_every_artifact_field() {
    let artifact = sign(&statement(), b"kid");
    let receipt = receipt(&artifact);
    match_execution_receipt(&artifact, &receipt).unwrap();
    let changes: &[fn(&mut ExecutionReceipt)] = &[
        |r| r.schema = 1,
        |r| r.issuer = "urn:other:author".into(),
        |r| r.to_be_signed_digest = Hash::new([0; 32]),
        |r| r.public_key_fingerprint = Hash::new([0; 32]),
        |r| r.signature_digest = Hash::new([0; 32]),
    ];
    for change in changes {
        let mut changed = receipt.clone();
        change(&mut changed);
        assert_eq!(
            match_execution_receipt(&artifact, &changed),
            Err(Error::IntegrityFailed)
        );
    }
    let timestamped = with_timestamp(&artifact, &[0x30, 0]);
    match_execution_receipt(&timestamped, &receipt).unwrap();
}

#[test]
fn reused_artifact_helpers_still_verify_the_signature() {
    let artifact = sign(&statement(), b"kid");
    let receipt = receipt(&artifact);
    let mut message = Sign1Message::from_slice(&artifact.cose_sign1).unwrap();
    message.set_signature(vec![0; 64]).unwrap();
    let changed = SignedArtifact {
        cose_sign1: message.to_vec().unwrap().into(),
        ..artifact
    };
    assert!(verify_artifact(&changed).is_err());
    assert!(match_execution_receipt(&changed, &receipt).is_err());
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
fn preparation_rejects_bad_kids_keys_and_signature_lengths() {
    for size in [0, MAX_KID_BYTES + 1] {
        assert!(prepare_cose(&statement(), &vec![1; size]).is_err());
    }
    verify_artifact(&sign(&statement(), &vec![1; MAX_KID_BYTES])).unwrap();
    let public = Hash::new(key().verifying_key().to_bytes());
    for size in [0, 63, 65] {
        let prepared = prepare_attestation(&statement(), &public).unwrap();
        assert_eq!(prepared.finish(&vec![0; size]), Err(Error::IntegrityFailed));
    }
    for size in [0, 31, 33] {
        assert!(public_cose_key(b"kid", &vec![0; size]).is_err());
    }
    let public = key().verifying_key().to_bytes();
    assert!(public_cose_key(&[], &public).is_ok());
    assert!(public_cose_key(&vec![1; MAX_KID_BYTES], &public).is_ok());
    assert!(public_cose_key(&vec![1; MAX_KID_BYTES + 1], &public).is_err());
    assert!(public_cose_key(b"kid", &[0; 32]).is_err());
    // An ES256K artifact is no longer a supported profile.
    let mut es256k = Key::from_slice(&sign(&statement(), b"kid").cose_key).unwrap();
    es256k.set_alg(iana::AlgorithmES256K);
    assert!(key_thumbprint(&es256k.to_vec().unwrap()).is_ok());
    es256k.set_kty(iana::KeyTypeEC2);
    assert_eq!(
        key_thumbprint(&es256k.to_vec().unwrap()),
        Err(Error::UnsupportedProtocol)
    );
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
    let timestamped = with_timestamp(&artifact, &vec![1; MAX_TIMESTAMP_BYTES]);
    assert!(timestamped.cose_sign1.len() <= MAX_ARTIFACT_BYTES);
    assert_eq!(verify_artifact(&timestamped).unwrap(), statement());
    assert_eq!(
        signature_digest(&timestamped.cose_sign1),
        signature_digest(&artifact.cose_sign1)
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
        signature_digest(&vec![0; MAX_ARTIFACT_BYTES + 1]),
        Err(Error::QuotaExceeded)
    );
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
