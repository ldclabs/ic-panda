//! COSE profiles: three portable document profiles and a distinct typed app-action profile.
//! Document payloads exclude execution context; app actions explicitly bind their receiver and origin.
use crate::app_action::{validate_app_action, APP_ACTION_PROFILE};
use crate::*;
use cose2::{iana, Header, Key, Label, Sign1Message, Value, Verifier};
use dmsg_types::*;
use serde_bytes::Bytes;

/// Experimental dMsg text document profile media type (v1), used in protected header 16.
pub const TEXT_PROFILE: &str = "application/vnd.dmsg.text-statement+cose;v=1";
/// Experimental dMsg SHA-256 document profile media type (v1), used in protected header 16.
pub const DIGEST_PROFILE: &str = "application/vnd.dmsg.digest-statement+cose;v=1";
/// Experimental dMsg statement about one file (v1), used in protected header 16.
pub const FILE_STATEMENT_PROFILE: &str = "application/vnd.dmsg.file-statement+cose;v=1";
/// Content type of the deterministic CBOR file-statement payload.
pub const FILE_STATEMENT_CONTENT_TYPE: &str = "application/cbor";
/// Maximum encoded file-statement payload size (16,384 bytes).
pub const MAX_FILE_STATEMENT_BYTES: usize = 16_384;
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

/// Closed wire schema, separate from the public preparation enum. Canonical
/// decoding also rejects unknown/duplicate keys, explicit nulls and CBOR aliases.
#[derive(cbor2::Cbor)]
struct FileStatementPayload {
    #[cbor(key = 1)]
    text: String,
    #[cbor(key = 2)]
    sha256: Hash,
    #[cbor(key = 3)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content_type: Option<String>,
    #[cbor(key = 4)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    location: Option<String>,
}

pub(crate) fn malformed(_: cose2::Error) -> Error {
    Error::IntegrityFailed
}

/// The one document signature algorithm: Ed25519 (COSE -19).
pub const COSE_ALGORITHM: Label = Label::Int(iana::AlgorithmEd25519);

/// Select Statement for text/files, FileAttestation for digests and AppAction for typed actions.
///
/// This selects a derivation domain without validating the statement.
pub fn statement_purpose(statement: &Statement) -> KeyPurpose {
    match statement.content {
        StatementContent::AppAction(_) => KeyPurpose::AppAction,
        StatementContent::Text(_) | StatementContent::FileStatement { .. } => KeyPurpose::Statement,
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
/// `:`, following the StringOrURI rule. Text is 1..4096 bytes. File media types
/// are at most 256 bytes; optional locations are canonical URIs. Claimed issued_at
/// is not checked against a clock, and original content is not fetched or hashed.
///
/// # Errors
/// Text size violations return `Error::QuotaExceeded`; invalid claims, media types
/// or locations return `Error::InvalidInput`.
pub fn validate_statement(statement: &Statement) -> Result<()> {
    validate_uri(&statement.issuer)?;
    if let StatementContent::AppAction(action) = &statement.content {
        validate_app_action(action)?;
        // No independently supplied prose/subject/time may contradict the action.
        ensure(
            statement.subject.is_none() && statement.issued_at.is_none(),
            Error::UnsupportedProtocol,
        )?;
    }
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
    if let StatementContent::Text(text) | StatementContent::FileStatement { text, .. } =
        &statement.content
    {
        ensure(!text.is_empty() && text.len() <= 4096, Error::QuotaExceeded)?;
    }
    if let StatementContent::Digest {
        content_type,
        location,
        ..
    }
    | StatementContent::FileStatement {
        content_type,
        location,
        ..
    } = &statement.content
    {
        if let Some(value) = content_type {
            media_type(value)?;
        }
        if let Some(value) = location {
            validate_uri(value)?;
        }
    }
    Ok(())
}

fn parse_file_statement(payload: &[u8]) -> Result<StatementContent> {
    ensure(
        payload.len() <= MAX_FILE_STATEMENT_BYTES,
        Error::QuotaExceeded,
    )?;
    let FileStatementPayload {
        text,
        sha256,
        content_type,
        location,
    } = decode_canonical(payload)?;
    Ok(StatementContent::FileStatement {
        text,
        sha256,
        content_type,
        location,
    })
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
/// protected, content is encoded by its profile, and external AAD is empty.
/// Ed25519 signs the returned bytes directly. The kid must contain 1..256 bytes.
/// No key access, signing, approval or network call occurs here.
///
/// # Errors
/// Returns statement validation errors, `Error::InvalidInput` for kid
/// bounds, `Error::QuotaExceeded` for oversized signing input, or
/// `Error::IntegrityFailed` if COSE preparation fails.
pub fn prepare_cose(statement: &Statement, kid: &[u8]) -> Result<(Sign1Message, Vec<u8>)> {
    validate_statement(statement)?;
    ensure_valid(!kid.is_empty() && kid.len() <= MAX_KID_BYTES, "kid")?;
    let payload = match &statement.content {
        StatementContent::AppAction(action) => canonical(action),
        StatementContent::Text(text) => text.as_bytes().to_vec(),
        StatementContent::Digest { sha256, .. } => sha256.to_vec(),
        StatementContent::FileStatement {
            text,
            sha256,
            content_type,
            location,
        } => canonical(&FileStatementPayload {
            text: text.clone(),
            sha256: *sha256,
            content_type: content_type.clone(),
            location: location.clone(),
        }),
    };
    let mut message = Sign1Message::new(Some(payload));
    message.protected.set_kid(kid.to_vec());
    message.protected.insert(CWT_CLAIMS, claims(statement));
    match &statement.content {
        StatementContent::AppAction(_) => {
            message.protected.set_crit([CWT_CLAIMS, TYPE_HEADER]);
            message.protected.insert(TYPE_HEADER, APP_ACTION_PROFILE);
            message.protected.set_content_type("application/cbor");
        }
        StatementContent::Text(_) => {
            message.protected.set_crit([CWT_CLAIMS, TYPE_HEADER]);
            message.protected.insert(TYPE_HEADER, TEXT_PROFILE);
            message.protected.set_content_type(TEXT_CONTENT_TYPE);
        }
        StatementContent::FileStatement { .. } => {
            message.protected.set_crit([CWT_CLAIMS, TYPE_HEADER]);
            message
                .protected
                .insert(TYPE_HEADER, FILE_STATEMENT_PROFILE);
            message
                .protected
                .set_content_type(FILE_STATEMENT_CONTENT_TYPE);
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
        .prepare_signature(Some(COSE_ALGORITHM), None, None)
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

fn parse_message(message: &Sign1Message) -> Result<(Statement, &[u8])> {
    let headers = &message.protected;
    ensure(
        headers.alg().map_err(malformed)?.as_ref() == Some(&COSE_ALGORITHM),
        Error::UnsupportedProtocol,
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
        APP_ACTION_PROFILE => {
            ensure(
                text(headers, iana::HeaderParameterContentType)? == Some("application/cbor"),
                Error::UnsupportedProtocol,
            )?;
            (
                &[1, 2, 3, 4, 15, 16],
                &CRITICAL[..2],
                StatementContent::AppAction(Box::new(decode_canonical(payload)?)),
            )
        }
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
        FILE_STATEMENT_PROFILE => {
            ensure(
                text(headers, iana::HeaderParameterContentType)?
                    == Some(FILE_STATEMENT_CONTENT_TYPE),
                Error::UnsupportedProtocol,
            )?;
            (
                &[1, 2, 3, 4, 15, 16],
                &CRITICAL[..2],
                parse_file_statement(payload)?,
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
    Ok((statement, kid))
}

/// Validated local signing-input view returned by [`parse_signing_input`].
///
/// Immutable validated input, not a verified artifact, wire DTO or persistence record.
pub struct PreparedStatement {
    message: Sign1Message,
    statement: Statement,
    kid: Vec<u8>,
}

impl PreparedStatement {
    /// Decoded issuer, subject, issued-at and content.
    pub fn statement(&self) -> &Statement {
        &self.statement
    }

    /// Key identifier from the protected header.
    pub fn kid(&self) -> &[u8] {
        &self.kid
    }

    /// Validate and encode the public key before attaching the signature.
    /// The returned value only retains data needed to assemble the final artifact.
    pub fn into_signature(self, public: &[u8]) -> Result<PreparedSignature> {
        let cose_key = public_cose_key(&self.kid, public)?;
        Ok(PreparedSignature {
            message: self.message,
            cose_key,
        })
    }
}

/// Validated COSE framing and public key, ready for a 64-byte Ed25519 signature.
/// Construction is restricted to parsed canonical input and a validated public key.
pub struct PreparedSignature {
    message: Sign1Message,
    cose_key: Vec<u8>,
}

impl PreparedSignature {
    /// Attach a signature without reparsing the payload or public key.
    ///
    /// This checks encoding, not mathematical validity; verify before use.
    pub fn finish(mut self, signature: Vec<u8>) -> Result<SignedArtifact> {
        ensure(signature.len() == 64, Error::IntegrityFailed)?;
        self.message.set_signature(signature).map_err(malformed)?;
        Ok(SignedArtifact {
            cose_sign1: self.message.to_vec().map_err(malformed)?.into(),
            cose_key: self.cose_key.into(),
        })
    }
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
    let (statement, kid) = parse_message(&message)?;
    let kid = kid.to_vec();
    let actual = message
        .prepare_signature(Some(COSE_ALGORITHM), None, None)
        .map_err(malformed)?;
    // This comparison checks the entire canonical framing, including the
    // protected map. No separate decode/reencode of the outer tuple is needed.
    ensure(actual == bytes, Error::IntegrityFailed)?;
    Ok(PreparedStatement {
        message,
        statement,
        kid,
    })
}

/// Encode a public-only Ed25519 COSE_Key.
///
/// `public` is a raw 32-byte Ed25519 key, not CBOR. The result declares the
/// algorithm and verify operation. Empty kid is allowed to compute a thumbprint
/// before choosing a signing kid; otherwise kid is at most 256 bytes. This does
/// not establish key ownership.
///
/// # Errors
/// Invalid/weak Ed25519 keys return `Error::IntegrityFailed`; oversized kid
/// returns `Error::InvalidInput`.
pub fn public_cose_key(kid: &[u8], public: &[u8]) -> Result<Vec<u8>> {
    ensure_valid(kid.len() <= MAX_KID_BYTES, "kid")?;
    validate_ed25519_key(public)?;
    let mut key = Key::new();
    key.set_alg(COSE_ALGORITHM).set_kid(kid.to_vec());
    key.set_ops([iana::KeyOperationVerify]);
    key.set_kty(iana::KeyTypeOKP);
    key.insert(iana::OKPKeyParameterCrv, iana::EllipticCurveEd25519);
    key.insert(iana::OKPKeyParameterX, public.to_vec());
    key.to_vec().map_err(malformed)
}

/// Compute the RFC 9679 SHA-256 thumbprint of required public COSE key members.
///
/// Excludes kid, alg and key_ops. This is not raw-public-key SHA-256 or a
/// complete key/profile verifier.
///
/// # Errors
/// Rejects keys above 2048 bytes, private d parameters, missing required members,
/// key types other than OKP and malformed CBOR.
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
    ensure(
        key.kty().map_err(malformed)? == Some(Label::Int(iana::KeyTypeOKP)),
        Error::UnsupportedProtocol,
    )?;
    let mut required = Key::new();
    for label in [1, -1, -2] {
        required.insert(
            label,
            key.get(label).ok_or(Error::IntegrityFailed)?.clone(),
        );
    }
    Ok(sha256(&required.to_vec().map_err(malformed)?))
}

struct ProfileVerifier(cose2::ed25519::Ed25519Verifier);

impl Verifier for ProfileVerifier {
    fn alg(&self) -> Option<Label> {
        Some(COSE_ALGORITHM)
    }

    fn understood_critical_headers(&self) -> &[Label] {
        &CRITICAL
    }

    fn verify(&self, bytes: &[u8], signature: &[u8]) -> std::result::Result<(), cose2::Error> {
        self.0.verify(bytes, signature)
    }
}

fn verifier(key: &Key) -> Result<ProfileVerifier> {
    ensure(
        !key.contains_key(iana::OKPKeyParameterD)
            && key
                .allows_any_operation(&[iana::KeyOperationVerify])
                .map_err(malformed)?,
        Error::IntegrityFailed,
    )?;
    ensure(
        key.alg().map_err(malformed)? == Some(COSE_ALGORITHM),
        Error::IntegrityFailed,
    )?;
    Ok(ProfileVerifier(
        cose2::ed25519::Ed25519Verifier::from_cose_key(key).map_err(malformed)?,
    ))
}

/// Verify supported COSE document profiles and their mathematical signature.
///
/// Checks tagged COSE_Sign1, protected Ed25519 algorithm/kid/claims/profile,
/// critical headers, public-key constraints and signature. Preserves original
/// protected bytes. An attached public key is not an identity credential; this
/// does not check original digest content, issuer ownership, authorization,
/// current status, or TSA trust.
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
    let (statement, kid) = parse_message(&message)?;
    let key = Key::from_slice(&artifact.cose_key).map_err(malformed)?;
    ensure(
        key.kid().map_err(malformed)?.is_none_or(|id| id == kid),
        Error::IntegrityFailed,
    )?;
    message.verify(&verifier(&key)?, None).map_err(malformed)?;
    Ok(VerifiedArtifact {
        statement,
        message,
        key,
    })
}

/// Hash raw signature bytes extracted from a COSE_Sign1 message.
///
/// Used for execution-receipt matching; the hash excludes CBOR framing.
/// This extraction helper does not verify the signature,
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
