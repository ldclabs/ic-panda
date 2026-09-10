//! COSE document profiles. Wire payloads are UTF-8 text or RFC 9995 digests;
//! account routing, browser origins and execution IDs never enter that payload.
use crate::*;
use cose2::{iana, Header, Key, Label, Sign1Message, Value, Verifier};
use dmsg_types::{
    cose::{Algorithm, KeyPurpose},
    *,
};
use k256::elliptic_curve::sec1::ToSec1Point;
use serde_bytes::Bytes;

/// Experimental dMsg text document profile media type (v1), used in protected header 16.
pub const TEXT_PROFILE: &str = "application/vnd.dmsg.text-statement+cose;v=1";
/// Experimental dMsg SHA-256 document profile media type (v1), used in protected header 16.
pub const DIGEST_PROFILE: &str = "application/vnd.dmsg.digest-statement+cose;v=1";
/// Required content type of the text profile; not used in the digest profile.
pub const TEXT_CONTENT_TYPE: &str = "text/plain;charset=utf-8";
/// COSE header 15: protected CWT claims (issuer, optional subject and issued-at).
pub const CWT_CLAIMS: i64 = 15;
/// COSE header 16: protected document profile media type.
pub const TYPE_HEADER: i64 = 16;
/// COSE header 258: digest profile hash algorithm, fixed to SHA-256 (-16).
pub const HASH_ALGORITHM: i64 = 258;
/// COSE header 259: optional original-content media type in the digest profile.
pub const PREIMAGE_CONTENT_TYPE: i64 = 259;
/// COSE header 260: optional original-content URI in the digest profile.
pub const PAYLOAD_LOCATION: i64 = 260;
/// COSE header 270: unprotected timestamp token; its presence establishes no TSA trust.
pub const CTT_HEADER: i64 = 270;
/// Maximum document key identifier size in bytes (256); signing requires a nonempty kid.
pub const MAX_KID_BYTES: usize = 256;
/// Maximum opaque timestamp token size in bytes (131,072).
pub const MAX_TIMESTAMP_BYTES: usize = 131_072;
/// Maximum encoded public COSE_Key size in bytes (2,048).
pub const MAX_COSE_KEY_BYTES: usize = 2048;
/// Maximum COSE_Sign1 size in bytes (196,608), including optional timestamp evidence.
pub const MAX_ARTIFACT_BYTES: usize = MAX_PAYLOAD + MAX_TIMESTAMP_BYTES;
const CRITICAL: [Label; 3] = [
    Label::Int(CWT_CLAIMS),
    Label::Int(TYPE_HEADER),
    Label::Int(HASH_ALGORITHM),
];

fn malformed(_: cose2::Error) -> Error {
    Error::IntegrityFailed
}

/// Map Ed25519 to COSE -19 and ECDSA secp256k1 to COSE ES256K (-47).
///
/// # Errors
/// vetKD is a derivation algorithm and returns `Error::UnsupportedProtocol`.
pub fn cose_algorithm(algorithm: &Algorithm) -> Result<Label> {
    match algorithm {
        Algorithm::Ed25519 => Ok(iana::AlgorithmEd25519.into()),
        Algorithm::EcdsaSecp256k1 => Ok(iana::AlgorithmES256K.into()),
        Algorithm::VetKdBls12381 => Err(Error::UnsupportedProtocol),
    }
}

fn from_algorithm(label: &Label) -> Result<Algorithm> {
    match label {
        Label::Int(iana::AlgorithmEd25519) => Ok(Algorithm::Ed25519),
        Label::Int(iana::AlgorithmES256K) => Ok(Algorithm::EcdsaSecp256k1),
        _ => Err(Error::UnsupportedProtocol),
    }
}

/// Select Statement for text and FileAttestation for a digest document.
///
/// This selects a derivation domain without validating the statement.
pub fn statement_purpose(statement: &Statement) -> KeyPurpose {
    match statement.content {
        StatementContent::Text(_) => KeyPurpose::Statement,
        StatementContent::Digest { .. } => KeyPurpose::FileAttestation,
    }
}

fn media_type(value: &str) -> Result<()> {
    ensure_valid(
        value.len() <= 256 && value.parse::<mime::Mime>().is_ok(),
        "media type",
    )
}

/// Check the supported document profile's claims and content limits.
///
/// Issuer must be a canonical URI. Optional subject is nonempty, at most 8192
/// UTF-8 bytes, has no control characters, and must be a URI when it contains
/// `:`, following the StringOrURI rule. Text is 1..4096 bytes. Digest media types
/// are at most 256 bytes; optional locations are canonical URIs. Claimed issued_at
/// is not checked against a clock, and original content is not fetched or hashed.
///
/// # Errors
/// Text size violations return `Error::QuotaExceeded`; invalid claims, media types
/// or locations return `Error::InvalidInput`.
pub fn validate_statement(statement: &Statement) -> Result<()> {
    validate_uri(&statement.issuer)?;
    if let Some(subject) = &statement.subject {
        ensure_valid(
            !subject.is_empty()
                && subject.len() <= MAX_URI_BYTES
                && !subject.chars().any(char::is_control),
            "statement subject",
        )?;
        if subject.contains(':') {
            validate_uri(subject)?;
        }
    }
    match &statement.content {
        StatementContent::Text(text) => {
            ensure(!text.is_empty() && text.len() <= 4096, Error::QuotaExceeded)?
        }
        StatementContent::Digest {
            content_type,
            location,
            ..
        } => {
            if let Some(value) = content_type {
                media_type(value)?;
            }
            if let Some(value) = location {
                validate_uri(value)?;
            }
        }
    }
    Ok(())
}

fn claims(statement: &Statement) -> Value {
    let mut values = vec![(Value::from(1), Value::Text(statement.issuer.clone()))];
    if let Some(subject) = &statement.subject {
        values.push((Value::from(2), Value::Text(subject.clone())));
    }
    if let Some(at) = statement.issued_at {
        values.push((Value::from(6), Value::from(at)));
    }
    Value::Map(values)
}

/// Prepare a document's COSE_Sign1 message and exact bytes to sign.
///
/// Returns `(unsigned_message, Sig_structure)`. Claims and profile headers are
/// protected, text/digest bytes are the payload, and external AAD is empty.
/// Ed25519 signs the returned bytes directly; ES256K signs their SHA-256 digest
/// when using a prehash signing API. The kid must contain 1..256 bytes.
/// No key access, signing, approval or network call occurs here.
///
/// # Errors
/// Returns statement/algorithm validation errors, `Error::InvalidInput` for kid
/// bounds, `Error::QuotaExceeded` for oversized signing input, or
/// `Error::IntegrityFailed` if COSE preparation fails.
pub fn prepare_cose(
    statement: &Statement,
    algorithm: &Algorithm,
    kid: &[u8],
) -> Result<(Sign1Message, Vec<u8>)> {
    let algorithm = cose_algorithm(algorithm)?;
    validate_statement(statement)?;
    ensure_valid(!kid.is_empty() && kid.len() <= MAX_KID_BYTES, "kid")?;
    let payload = match &statement.content {
        StatementContent::Text(text) => text.as_bytes().to_vec(),
        StatementContent::Digest { sha256, .. } => sha256.to_vec(),
    };
    let mut message = Sign1Message::new(Some(payload));
    message.protected.set_kid(kid.to_vec());
    message.protected.insert(CWT_CLAIMS, claims(statement));
    match &statement.content {
        StatementContent::Text(_) => {
            message.protected.set_crit([CWT_CLAIMS, TYPE_HEADER]);
            message.protected.insert(TYPE_HEADER, TEXT_PROFILE);
            message.protected.set_content_type(TEXT_CONTENT_TYPE);
        }
        StatementContent::Digest {
            content_type,
            location,
            ..
        } => {
            message
                .protected
                .set_crit([CWT_CLAIMS, TYPE_HEADER, HASH_ALGORITHM]);
            message.protected.insert(TYPE_HEADER, DIGEST_PROFILE);
            message
                .protected
                .insert(HASH_ALGORITHM, iana::AlgorithmSHA_256);
            if let Some(value) = content_type {
                message
                    .protected
                    .insert(PREIMAGE_CONTENT_TYPE, value.clone());
            }
            if let Some(value) = location {
                message.protected.insert(PAYLOAD_LOCATION, value.clone());
            }
        }
    }
    let tbs = message
        .prepare_signature(Some(algorithm), None, None)
        .map_err(malformed)?;
    ensure(tbs.len() <= MAX_PAYLOAD, Error::QuotaExceeded)?;
    Ok((message, tbs))
}

fn text(header: &Header, label: i64) -> Result<Option<&str>> {
    match header.get(label) {
        None => Ok(None),
        Some(Value::Text(value)) => Ok(Some(value)),
        _ => Err(Error::IntegrityFailed),
    }
}

fn parse_claims(header: &Header) -> Result<(String, Option<String>, Option<i64>)> {
    let Some(Value::Map(values)) = header.get(CWT_CLAIMS) else {
        return Err(Error::IntegrityFailed);
    };
    let (mut issuer, mut subject, mut issued_at) = (None, None, None);
    for (key, value) in values {
        match (key, value) {
            (Value::Integer(key), Value::Text(value)) if *key == 1.into() && issuer.is_none() => {
                issuer = Some(value.clone())
            }
            (Value::Integer(key), Value::Text(value)) if *key == 2.into() && subject.is_none() => {
                subject = Some(value.clone())
            }
            (Value::Integer(key), Value::Integer(value))
                if *key == 6.into() && issued_at.is_none() =>
            {
                issued_at = Some(i64::try_from(*value).map_err(|_| Error::IntegrityFailed)?)
            }
            _ => return Err(Error::UnsupportedProtocol),
        }
    }
    Ok((issuer.ok_or(Error::IntegrityFailed)?, subject, issued_at))
}

fn parse_message(message: &Sign1Message) -> Result<(Statement, Algorithm, &[u8])> {
    let headers = &message.protected;
    let algorithm = from_algorithm(
        &headers
            .alg()
            .map_err(malformed)?
            .ok_or(Error::IntegrityFailed)?,
    )?;
    let kid = headers
        .kid()
        .map_err(malformed)?
        .ok_or(Error::IntegrityFailed)?;
    ensure(
        !kid.is_empty() && kid.len() <= MAX_KID_BYTES,
        Error::IntegrityFailed,
    )?;
    ensure(
        message.unprotected.iter().all(|(label, value)| {
            *label == Label::Int(CTT_HEADER)
                && matches!(value, Value::Bytes(b) if !b.is_empty() && b.len() <= MAX_TIMESTAMP_BYTES)
        }),
        Error::UnsupportedProtocol,
    )?;
    let payload = message.payload.as_deref().ok_or(Error::IntegrityFailed)?;
    let profile = text(headers, TYPE_HEADER)?.ok_or(Error::IntegrityFailed)?;
    let (allowed, critical, content): (&[i64], &[Label], StatementContent) = match profile {
        TEXT_PROFILE => {
            ensure(
                text(headers, iana::HeaderParameterContentType)? == Some(TEXT_CONTENT_TYPE),
                Error::UnsupportedProtocol,
            )?;
            (
                &[1, 2, 3, 4, 15, 16],
                &CRITICAL[..2],
                StatementContent::Text(
                    std::str::from_utf8(payload)
                        .map_err(|_| Error::IntegrityFailed)?
                        .into(),
                ),
            )
        }
        DIGEST_PROFILE => {
            ensure(
                headers.get_i64(HASH_ALGORITHM).map_err(malformed)? == Some(iana::AlgorithmSHA_256),
                Error::UnsupportedProtocol,
            )?;
            (
                &[1, 2, 4, 15, 16, 258, 259, 260],
                &CRITICAL,
                StatementContent::Digest {
                    sha256: Hash::new(payload.try_into().map_err(|_| Error::IntegrityFailed)?),
                    content_type: text(headers, PREIMAGE_CONTENT_TYPE)?.map(str::to_owned),
                    location: text(headers, PAYLOAD_LOCATION)?.map(str::to_owned),
                },
            )
        }
        _ => return Err(Error::UnsupportedProtocol),
    };
    ensure(
        headers
            .iter()
            .all(|(label, _)| matches!(label, Label::Int(id) if allowed.contains(id))),
        Error::UnsupportedProtocol,
    )?;
    let declared = headers
        .crit()
        .map_err(malformed)?
        .ok_or(Error::IntegrityFailed)?;
    ensure(
        declared.len() == critical.len() && critical.iter().all(|label| declared.contains(label)),
        Error::UnsupportedProtocol,
    )?;
    headers
        .ensure_crit_understood(&CRITICAL)
        .map_err(malformed)?;
    let (issuer, subject, issued_at) = parse_claims(headers)?;
    let statement = Statement {
        issuer,
        subject,
        issued_at,
        content,
    };
    validate_statement(&statement)?;
    Ok((statement, algorithm, kid))
}

/// Validated local signing-input view returned by [`parse_signing_input`].
///
/// Contains an unsigned COSE message and decoded fields, not a verified artifact,
/// wire DTO or persistence record. Mutating these fields does not revalidate them.
pub struct PreparedStatement {
    /// Unsigned COSE message reconstructed from the validated canonical input.
    pub message: Sign1Message,
    /// Decoded issuer, subject, issued-at and content view.
    pub statement: Statement,
    /// Supported formal signing algorithm selected by the protected header.
    pub algorithm: Algorithm,
    /// Opaque key identifier from the protected header.
    pub kid: Vec<u8>,
}

/// Parse and validate canonical local signing input, not a signed artifact.
///
/// Requires `CBOR(["Signature1", protected_bstr, h'', payload_bstr])`, valid
/// profile headers, and exact canonical reconstruction including the protected
/// map. Verification of external artifacts instead preserves their original
/// protected bytes; use [`verify_artifact`] for that path.
///
/// # Errors
/// Returns `Error::QuotaExceeded` above 65,536 bytes, `Error::IntegrityFailed`
/// for malformed/noncanonical input, or statement/profile validation errors.
pub fn parse_signing_input(bytes: &[u8]) -> Result<PreparedStatement> {
    ensure(bytes.len() <= MAX_PAYLOAD, Error::QuotaExceeded)?;
    let (context, protected, aad, payload): (&str, &Bytes, &Bytes, &Bytes) =
        cbor2::from_slice(bytes).map_err(|_| Error::IntegrityFailed)?;
    ensure(
        context == "Signature1" && aad.is_empty(),
        Error::IntegrityFailed,
    )?;
    let mut message = Sign1Message::new(Some(payload.to_vec()));
    message.protected = Header::from_slice(protected).map_err(malformed)?;
    let (statement, algorithm, kid) = parse_message(&message)?;
    let kid = kid.to_vec();
    let actual = message
        .prepare_signature(Some(cose_algorithm(&algorithm)?), None, None)
        .map_err(malformed)?;
    // This comparison checks the entire canonical framing, including the
    // protected map. No separate decode/reencode of the outer tuple is needed.
    ensure(actual == bytes, Error::IntegrityFailed)?;
    Ok(PreparedStatement {
        message,
        statement,
        algorithm,
        kid,
    })
}

/// Encode a public-only COSE_Key for a supported signing algorithm.
///
/// `public` is a raw 32-byte Ed25519 key or a SEC1 secp256k1 point, not CBOR.
/// The result declares the algorithm and verify operation. Empty kid is allowed
/// to compute a thumbprint before choosing a signing kid; otherwise kid is at
/// most 256 bytes. This does not establish key ownership.
///
/// # Errors
/// Invalid/weak Ed25519 keys and invalid EC points return `Error::IntegrityFailed`;
/// oversized kid returns `Error::InvalidInput`; vetKD returns `Error::UnsupportedProtocol`.
pub fn public_cose_key(algorithm: &Algorithm, kid: &[u8], public: &[u8]) -> Result<Vec<u8>> {
    ensure_valid(kid.len() <= MAX_KID_BYTES, "kid")?;
    let mut key = Key::new();
    key.set_alg(cose_algorithm(algorithm)?)
        .set_kid(kid.to_vec());
    key.set_ops([iana::KeyOperationVerify]);
    match algorithm {
        Algorithm::Ed25519 => {
            validate_ed25519_key(public)?;
            key.set_kty(iana::KeyTypeOKP);
            key.insert(iana::OKPKeyParameterCrv, iana::EllipticCurveEd25519);
            key.insert(iana::OKPKeyParameterX, public.to_vec());
        }
        Algorithm::EcdsaSecp256k1 => {
            let point = k256::PublicKey::from_sec1_bytes(public)
                .map_err(|_| Error::IntegrityFailed)?
                .to_sec1_point(false);
            key.set_kty(iana::KeyTypeEC2);
            key.insert(iana::EC2KeyParameterCrv, iana::EllipticCurveSecp256k1);
            key.insert(
                iana::EC2KeyParameterX,
                point.x().ok_or(Error::IntegrityFailed)?.to_vec(),
            );
            key.insert(
                iana::EC2KeyParameterY,
                point.y().ok_or(Error::IntegrityFailed)?.to_vec(),
            );
        }
        _ => return Err(Error::UnsupportedProtocol),
    }
    key.to_vec().map_err(malformed)
}

/// Compute the RFC 9679 SHA-256 thumbprint of required public COSE key members.
///
/// Excludes kid, alg and key_ops. Expands compressed secp256k1 y coordinates
/// before hashing, so compressed/uncompressed forms have the same thumbprint.
/// This is not raw-public-key SHA-256 or a complete key/profile verifier.
///
/// # Errors
/// Rejects keys above 2048 bytes, private d parameters, missing required members,
/// unsupported key types, malformed CBOR and invalid compressed EC coordinates.
pub fn key_thumbprint(encoded: &[u8]) -> Result<Hash> {
    ensure(encoded.len() <= MAX_COSE_KEY_BYTES, Error::QuotaExceeded)?;
    let key = Key::from_slice(encoded).map_err(malformed)?;
    thumbprint(&key)
}

pub(crate) fn thumbprint(key: &Key) -> Result<Hash> {
    ensure(
        !key.contains_key(iana::OKPKeyParameterD),
        Error::IntegrityFailed,
    )?;
    let key_type = key.kty().map_err(malformed)?;
    let labels: &[i64] = match key_type {
        Some(Label::Int(iana::KeyTypeOKP)) => &[1, -1, -2],
        Some(Label::Int(iana::KeyTypeEC2)) => &[1, -1, -2, -3],
        _ => return Err(Error::UnsupportedProtocol),
    };
    let mut required = Key::new();
    for label in labels {
        required.insert(
            *label,
            key.get(*label).ok_or(Error::IntegrityFailed)?.clone(),
        );
    }
    if key_type == Some(iana::KeyTypeEC2.into()) {
        if let Some(Value::Bool(odd)) = key.get(iana::EC2KeyParameterY) {
            ensure(
                key.get_i64(iana::EC2KeyParameterCrv).map_err(malformed)?
                    == Some(iana::EllipticCurveSecp256k1),
                Error::UnsupportedProtocol,
            )?;
            let x = key
                .get_bytes(iana::EC2KeyParameterX)
                .map_err(malformed)?
                .ok_or(Error::IntegrityFailed)?;
            ensure(x.len() == 32, Error::IntegrityFailed)?;
            let mut compressed = [0; 33];
            compressed[0] = if *odd { 3 } else { 2 };
            compressed[1..].copy_from_slice(x);
            let point = k256::PublicKey::from_sec1_bytes(&compressed)
                .map_err(|_| Error::IntegrityFailed)?
                .to_sec1_point(false);
            // RFC 9679 §4.2: thumbprints always use the uncompressed y coordinate.
            required.insert(
                iana::EC2KeyParameterY,
                point.y().ok_or(Error::IntegrityFailed)?.to_vec(),
            );
        }
    }
    Ok(sha256(&required.to_vec().map_err(malformed)?))
}

/// Assemble a tagged COSE_Sign1 artifact and public-only key after external signing.
///
/// `tbs` must be the exact canonical bytes returned by [`prepare_cose`]; `public`
/// is raw Ed25519 or SEC1 secp256k1 bytes. `signature` is 64 bytes: an Ed25519
/// signature or ES256K r||s, not DER. This validates structure and key encoding,
/// but does not mathematically verify the signature. Use [`match_signing_result`]
/// or [`verify_artifact`] before accepting a returned artifact.
///
/// # Errors
/// Returns `Error::IntegrityFailed` for wrong signature length, and propagates
/// signing-input/public-key validation errors.
pub fn finish_cose(tbs: &[u8], public: &[u8], signature: Vec<u8>) -> Result<SignedArtifact> {
    // Both currently enabled algorithms encode r||s / Ed25519 signatures in 64 bytes.
    ensure(signature.len() == 64, Error::IntegrityFailed)?;
    let mut prepared = parse_signing_input(tbs)?;
    let key = public_cose_key(&prepared.algorithm, &prepared.kid, public)?;
    prepared
        .message
        .set_signature(signature)
        .map_err(malformed)?;
    Ok(SignedArtifact {
        cose_sign1: prepared.message.to_vec().map_err(malformed)?.into(),
        cose_key: key.into(),
    })
}

enum ProfileVerifier {
    Ed(cose2::ed25519::Ed25519Verifier),
    Ec(k256::ecdsa::VerifyingKey),
}

impl Verifier for ProfileVerifier {
    fn alg(&self) -> Option<Label> {
        Some(match self {
            Self::Ed(_) => iana::AlgorithmEd25519.into(),
            Self::Ec(_) => iana::AlgorithmES256K.into(),
        })
    }
    fn understood_critical_headers(&self) -> &[Label] {
        &CRITICAL
    }
    fn verify(&self, bytes: &[u8], signature: &[u8]) -> std::result::Result<(), cose2::Error> {
        match self {
            Self::Ed(key) => key.verify(bytes, signature),
            Self::Ec(key) => {
                use k256::ecdsa::signature::hazmat::PrehashVerifier;
                let signature = k256::ecdsa::Signature::from_slice(signature)
                    .map_err(|_| cose2::Error::verify("signature encoding"))?
                    .normalize_s();
                key.verify_prehash(sha256(bytes).as_slice(), &signature)
                    .map_err(|_| cose2::Error::verify("invalid signature"))
            }
        }
    }
}

fn verifier(key: &Key, algorithm: &Algorithm) -> Result<ProfileVerifier> {
    ensure(
        !key.contains_key(iana::OKPKeyParameterD)
            && key
                .allows_any_operation(&[iana::KeyOperationVerify])
                .map_err(malformed)?,
        Error::IntegrityFailed,
    )?;
    ensure(
        key.alg().map_err(malformed)? == Some(cose_algorithm(algorithm)?),
        Error::IntegrityFailed,
    )?;
    match algorithm {
        Algorithm::Ed25519 => Ok(ProfileVerifier::Ed(
            cose2::ed25519::Ed25519Verifier::from_cose_key(key).map_err(malformed)?,
        )),
        Algorithm::EcdsaSecp256k1 => {
            ensure(
                key.kty().map_err(malformed)? == Some(iana::KeyTypeEC2.into())
                    && key.get_i64(iana::EC2KeyParameterCrv).map_err(malformed)?
                        == Some(iana::EllipticCurveSecp256k1),
                Error::IntegrityFailed,
            )?;
            let x = key
                .get_bytes(iana::EC2KeyParameterX)
                .map_err(malformed)?
                .ok_or(Error::IntegrityFailed)?;
            ensure(x.len() == 32, Error::IntegrityFailed)?;
            let mut public = [0; 65];
            public[1..33].copy_from_slice(x);
            let public = match key.get(iana::EC2KeyParameterY) {
                Some(Value::Bytes(y)) if y.len() == 32 => {
                    public[0] = 4;
                    public[33..].copy_from_slice(y);
                    &public[..]
                }
                Some(Value::Bool(odd)) => {
                    public[0] = if *odd { 3 } else { 2 };
                    &public[..33]
                }
                _ => return Err(Error::IntegrityFailed),
            };
            Ok(ProfileVerifier::Ec(
                k256::ecdsa::VerifyingKey::from_sec1_bytes(public)
                    .map_err(|_| Error::IntegrityFailed)?,
            ))
        }
        _ => Err(Error::UnsupportedProtocol),
    }
}

/// Verify supported COSE document profiles and their mathematical signature.
///
/// Checks tagged COSE_Sign1, protected algorithm/kid/claims/profile, critical
/// headers, public-key constraints and signature. Preserves original protected
/// bytes. An attached public key is not an identity credential; this does not
/// check original digest content, issuer ownership, authorization, current status,
/// or TSA trust. Use [`verification_report`] to also compare supplied content.
///
/// # Errors
/// Size bounds yield `Error::QuotaExceeded`, unknown semantics yield
/// `Error::UnsupportedProtocol`, and malformed data/key/signature mismatches
/// usually yield `Error::IntegrityFailed`. Claim validation errors also propagate.
pub fn verify_artifact(artifact: &SignedArtifact) -> Result<Statement> {
    Ok(verify_and_parse_artifact(artifact)?.statement)
}

/// Internal, per-call reuse of verified data; never a cache or a wire type.
pub(crate) struct VerifiedArtifact {
    pub statement: Statement,
    pub message: Sign1Message,
    pub key: Key,
}

impl VerifiedArtifact {
    pub fn signing_bytes(&self) -> Result<Vec<u8>> {
        Sign1Message::to_be_signed(
            self.message.protected_raw(),
            &[],
            self.message
                .payload
                .as_deref()
                .ok_or(Error::IntegrityFailed)?,
        )
        .map_err(malformed)
    }
}

pub(crate) fn verify_and_parse_artifact(artifact: &SignedArtifact) -> Result<VerifiedArtifact> {
    ensure(
        artifact.cose_sign1.len() <= MAX_ARTIFACT_BYTES
            && artifact.cose_key.len() <= MAX_COSE_KEY_BYTES,
        Error::QuotaExceeded,
    )?;
    ensure(
        artifact.cose_sign1.first() == Some(&0xd2),
        Error::IntegrityFailed,
    )?;
    let message = Sign1Message::from_slice(&artifact.cose_sign1).map_err(malformed)?;
    let (statement, algorithm, kid) = parse_message(&message)?;
    let key = Key::from_slice(&artifact.cose_key).map_err(malformed)?;
    ensure(
        key.kid().map_err(malformed)?.is_none_or(|id| id == kid),
        Error::IntegrityFailed,
    )?;
    message
        .verify(&verifier(&key, &algorithm)?, None)
        .map_err(malformed)?;
    Ok(VerifiedArtifact {
        statement,
        message,
        key,
    })
}

/// Verify a document and report separate content and evidence statuses.
///
/// If `content` is supplied, text must match exactly or its SHA-256 must match
/// the digest profile. Without it, embedded text is Verified and digest content
/// is NotProvided. Signature is Verified on success; issuer binding,
/// authorization and current status remain NotChecked. Timestamp is NotProvided
/// when absent and NotChecked when a token is attached: no TSA verification occurs.
///
/// # Errors
/// Propagates artifact verification errors; original-content mismatch returns
/// `Error::IntegrityFailed` rather than a partially successful report.
pub fn verification_report(
    artifact: &SignedArtifact,
    content: Option<&[u8]>,
) -> Result<VerificationReport> {
    let VerifiedArtifact {
        statement, message, ..
    } = verify_and_parse_artifact(artifact)?;
    let checked = match (&statement.content, content) {
        (StatementContent::Text(text), Some(bytes)) => {
            ensure(text.as_bytes() == bytes, Error::IntegrityFailed)?;
            VerificationStatus::Verified
        }
        (StatementContent::Text(_), None) => VerificationStatus::Verified,
        (
            StatementContent::Digest {
                sha256: expected, ..
            },
            Some(bytes),
        ) => {
            ensure(sha256(bytes) == *expected, Error::IntegrityFailed)?;
            VerificationStatus::Verified
        }
        (StatementContent::Digest { .. }, None) => VerificationStatus::NotProvided,
    };
    Ok(VerificationReport {
        statement,
        content: checked,
        signature: VerificationStatus::Verified,
        issuer_binding: VerificationStatus::NotChecked,
        authorization: VerificationStatus::NotChecked,
        timestamp: if message.unprotected.contains_key(CTT_HEADER) {
            VerificationStatus::NotChecked
        } else {
            VerificationStatus::NotProvided
        },
        current_status: VerificationStatus::NotChecked,
    })
}

/// Compute the RFC 9921 CTT SHA-256 imprint of the encoded signature bstr.
///
/// Includes the CBOR byte-string header: `SHA256(CBOR(signature_bstr))`, unlike
/// [`signature_digest`]. Requires canonical outer framing and a nonempty signature
/// of at most 16,384 bytes. It does not verify the signature, dMsg profile or TSA.
///
/// # Errors
/// Oversized artifacts return `Error::QuotaExceeded`; decoding, canonical framing
/// or signature-length failures return `Error::IntegrityFailed`.
pub fn timestamp_imprint(cose_sign1: &[u8]) -> Result<Hash> {
    ensure(cose_sign1.len() <= MAX_ARTIFACT_BYTES, Error::QuotaExceeded)?;
    let message = Sign1Message::from_slice(cose_sign1).map_err(malformed)?;
    ensure(
        !message.signature().is_empty()
            && message.signature().len() <= 16_384
            && message.to_vec().map_err(malformed)? == cose_sign1,
        Error::IntegrityFailed,
    )?;
    Ok(sha256(&canonical(&serde_bytes::Bytes::new(
        message.signature(),
    ))))
}

/// Attach an unverified timestamp token at unprotected COSE header 270.
///
/// First verifies the original artifact, then adds the token while retaining
/// the key. The token is opaque: CMS signature, certificate chain, imprint,
/// TSA policy and current status still need an independent TSA verifier.
///
/// # Errors
/// Empty/oversized tokens return `Error::QuotaExceeded`; an existing token returns
/// `Error::VersionConflict`. Original-artifact verification errors propagate.
pub fn attach_unverified_timestamp_token(
    artifact: &SignedArtifact,
    token: &[u8],
) -> Result<SignedArtifact> {
    ensure(
        !token.is_empty() && token.len() <= MAX_TIMESTAMP_BYTES,
        Error::QuotaExceeded,
    )?;
    let mut message = verify_and_parse_artifact(artifact)?.message;
    ensure(
        !message.unprotected.contains_key(CTT_HEADER),
        Error::VersionConflict,
    )?;
    message.unprotected.insert(CTT_HEADER, token.to_vec());
    Ok(SignedArtifact {
        cose_sign1: message.to_vec().map_err(malformed)?.into(),
        cose_key: artifact.cose_key.clone(),
    })
}

/// Hash raw signature bytes extracted from a COSE_Sign1 message.
///
/// Used for execution-receipt matching; unlike [`timestamp_imprint`], the hash
/// excludes CBOR framing. This extraction helper does not verify the signature,
/// dMsg profile, canonical outer encoding or key ownership.
///
/// # Errors
/// Oversized artifacts return `Error::QuotaExceeded`; COSE decoding errors return
/// `Error::IntegrityFailed`.
pub fn signature_digest(cose_sign1: &[u8]) -> Result<Hash> {
    ensure(cose_sign1.len() <= MAX_ARTIFACT_BYTES, Error::QuotaExceeded)?;
    Ok(sha256(
        Sign1Message::from_slice(cose_sign1)
            .map_err(malformed)?
            .signature(),
    ))
}
