//! dmsg-connect/1 rules shared by devices, the bridge page, the relay and
//! the vectors (`docs/protocol/connect-v1_zh.md`): page commands, proof and
//! operation digests, pairing IDs and MACs, the sealing contexts, and the
//! structural checks of requests and grants. Nothing here touches a key.
use crate::{canonical, digest, validate_origin};
use base64::{Engine, engine::general_purpose::STANDARD};
use dmsg_types::{connect::*, *};
use hmac::{KeyInit, Mac, SimpleHmac};
use serde_bytes::Bytes;
use sha2::Sha256;

/// Methods a page may call.
pub const METHODS: [&str; 8] = [
    "authenticate",
    "signDocument",
    "signAction",
    "checkout",
    "getOperation",
    "openOperation",
    "cancelOperation",
    "acknowledge",
];
/// Methods that create an operation and carry its payload.
pub const CREATE_METHODS: [&str; 4] = ["authenticate", "signDocument", "signAction", "checkout"];
/// Domain of the page's proof message.
pub const PROOF_DOMAIN: &str = "dmsg/connect-proof/v1";
/// Domain of the operation commitment.
pub const OPERATION_DOMAIN: &str = "dmsg/connect-operation/v1";
/// Domain of a pairing ID.
pub const PAIRING_ID_DOMAIN: &str = "dmsg/connect-pairing-id/v1";
/// Domain of the message a pairing MAC authenticates.
pub const PAIR_DOMAIN: &str = "dmsg/connect-pair/v1";
/// First element of the request ciphertext's AAD.
pub const REQUEST_AAD: &str = "dmsg/connect-request/v1";
/// First element of a recipient wrapping's HPKE info.
pub const WRAP_INFO: &str = "dmsg/connect-wrap/v1";
/// First element of a reply's HPKE info.
pub const RESPONSE_INFO: &str = "dmsg/connect-response/v1";
/// COSE `typ` of a signed [`ConnectRequest`].
pub const REQUEST_TYP: &str = "application/vnd.dmsg.connect-request+cose;v=1";
/// COSE `typ` of a [`ConnectPop`].
pub const POP_TYP: &str = "application/vnd.dmsg.connect-pop+cose;v=1";
/// DER prefix of a P-256 SubjectPublicKeyInfo; the uncompressed point follows.
pub const P256_SPKI_PREFIX: [u8; 26] = [
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
];
/// Largest decoded payload of a creating command.
pub const MAX_COMMAND_PAYLOAD: usize = 65_536;
/// Longest pending period of a create request.
pub const CREATE_TTL_MS: u64 = DAY;
/// Lifetime of a read or open request.
pub const READ_TTL_MS: u64 = 2 * MINUTE;
/// How far a request's `created_at` may lead the device clock.
pub const CLOCK_SKEW_MS: u64 = MINUTE;
/// Lifetime of a pairing invitation.
pub const INVITE_TTL_MS: u64 = 10 * MINUTE;
/// Longest lifetime of a pairing.
pub const PAIRING_TTL_MS: u64 = 180 * DAY;
/// Longest lifetime of a hint token.
pub const HINT_TTL_MS: u64 = 30 * DAY;
/// Longest pairing label in UTF-8 bytes.
pub const MAX_LABEL: usize = 64;

/// The operation kind a creating method produces.
pub fn operation_kind(method: &str) -> Option<&'static str> {
    match method {
        "authenticate" => Some("authentication"),
        "signDocument" => Some("document"),
        "signAction" => Some("action"),
        "checkout" => Some("checkout"),
        _ => None,
    }
}

/// The relay request a method sends to the devices, if any.
pub fn request_kind(method: &str) -> Option<RequestKind> {
    match method {
        "getOperation" => Some(RequestKind::Read),
        "openOperation" => Some(RequestKind::Open),
        m if CREATE_METHODS.contains(&m) => Some(RequestKind::Create),
        _ => None,
    }
}

/// Canonical standard base64 with padding.
///
/// # Errors
/// Any other spelling returns `Error::InvalidInput`.
pub fn standard_base64(text: &str) -> Result<Vec<u8>> {
    let bytes = STANDARD.decode(text).map_err(|_| invalid("base64"))?;
    ensure_valid(STANDARD.encode(&bytes) == text, "base64")?;
    Ok(bytes)
}

fn lower_hex_id(text: &str) -> Result<Hash> {
    ensure_valid(
        text.len() == 64
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "hex identifier",
    )?;
    let mut id = [0u8; 32];
    for (i, byte) in id.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).map_err(|_| invalid("hex"))?;
    }
    Ok(Hash::new(id))
}

/// Check a 91-byte P-256 SPKI DER key's encoding; the point is checked by
/// the verifier that uses it.
///
/// # Errors
/// Returns `Error::InvalidInput`.
pub fn validate_p256_spki(der: &[u8]) -> Result<()> {
    ensure_valid(
        der.len() == 91 && der.starts_with(&P256_SPKI_PREFIX) && der[26] == 0x04,
        "P-256 public key",
    )
}

/// Check a command's closed shape: a known method, the application ID, a
/// nonzero operation ID, a P-256 session key, a payload exactly for the
/// creating methods (at most 65,536 bytes) and a result digest exactly for
/// `acknowledge`.
///
/// # Errors
/// An unknown method returns `Error::UnsupportedProtocol`; anything else
/// `Error::InvalidInput`.
pub fn validate_command(command: &ConnectCommand) -> Result<()> {
    ensure(
        METHODS.contains(&command.method.as_str()),
        Error::UnsupportedProtocol,
    )?;
    crate::integration::validate_identifier(&command.app_id)?;
    crate::nonzero(lower_hex_id(&command.operation_id)?.as_slice())?;
    validate_p256_spki(&command_key(command)?)?;
    let create = CREATE_METHODS.contains(&command.method.as_str());
    match &command.payload {
        Some(payload) if create => ensure_valid(
            standard_base64(payload)?.len() <= MAX_COMMAND_PAYLOAD,
            "payload",
        )?,
        None if !create => {}
        _ => return Err(invalid("payload")),
    }
    match &command.result_digest {
        Some(value) if command.method == "acknowledge" => lower_hex_id(value).map(|_| ()),
        None if command.method != "acknowledge" => Ok(()),
        _ => Err(invalid("resultDigest")),
    }
}

/// The session key DER of a command.
///
/// # Errors
/// Returns `Error::InvalidInput` for a malformed key.
pub fn command_key(command: &ConnectCommand) -> Result<Vec<u8>> {
    ensure_valid(command.public_key.len() <= 128, "publicKey")?;
    let der = standard_base64(&command.public_key)?;
    validate_p256_spki(&der)?;
    Ok(der)
}

/// The decoded payload of a creating command.
///
/// # Errors
/// Returns `Error::InvalidInput` without one.
pub fn command_payload(command: &ConnectCommand) -> Result<Vec<u8>> {
    standard_base64(
        command
            .payload
            .as_deref()
            .ok_or_else(|| invalid("payload"))?,
    )
}

/// The operation ID of a command.
///
/// # Errors
/// Returns `Error::InvalidInput` for a malformed ID.
pub fn command_operation(command: &ConnectCommand) -> Result<Hash> {
    lower_hex_id(&command.operation_id)
}

/// The 32 bytes the page signs with its session key (ECDSA P-256 with SHA-256):
/// `digest("dmsg/connect-proof/v1", ["dmsg-connect/1", nonce, origin, connection_id, command])`.
pub fn proof_message(
    nonce: &Hash,
    origin: &str,
    connection_id: &Hash,
    command: &ConnectCommand,
) -> Hash {
    digest(
        PROOF_DOMAIN,
        &(CONNECT_PROTOCOL, nonce, origin, connection_id, command),
    )
}

/// The operation commitment
/// `digest("dmsg/connect-operation/v1", [origin, appId, operationId, method, public_key_der, payload])`.
///
/// # Errors
/// A command without a payload or with a malformed key returns `Error::InvalidInput`.
pub fn operation_digest(origin: &str, command: &ConnectCommand) -> Result<Hash> {
    ensure_valid(
        CREATE_METHODS.contains(&command.method.as_str()),
        "creating method",
    )?;
    let key = command_key(command)?;
    let payload = command_payload(command)?;
    Ok(digest(
        OPERATION_DOMAIN,
        &(
            origin,
            &command.app_id,
            &command.operation_id,
            &command.method,
            Bytes::new(&key),
            Bytes::new(&payload),
        ),
    ))
}

/// `digest("dmsg/connect-pairing-id/v1", pairing_pub)`.
pub fn pairing_id(pairing_pub: &[u8]) -> Hash {
    digest(PAIRING_ID_DOMAIN, &Bytes::new(pairing_pub))
}

/// A pairing label: 1 to 64 UTF-8 bytes without control characters.
///
/// # Errors
/// Returns `Error::InvalidInput`.
pub fn validate_label(label: &str) -> Result<()> {
    ensure_valid(
        !label.is_empty() && label.len() <= MAX_LABEL && !label.chars().any(char::is_control),
        "label",
    )
}

/// The MAC a pairing candidate carries:
/// `HMAC-SHA256(secret, digest("dmsg/connect-pair/v1", [environment, account_id, invite_id, pairing_pub, label]))`.
pub fn pairing_mac(
    secret: &Hash,
    environment: &Environment,
    account: &AccountId,
    invite: &Hash,
    pairing_pub: &[u8],
    label: &str,
) -> Hash {
    let message = digest(
        PAIR_DOMAIN,
        &(environment, account, invite, Bytes::new(pairing_pub), label),
    );
    let mut mac =
        SimpleHmac::<Sha256>::new_from_slice(secret.as_slice()).expect("HMAC takes any key size");
    mac.update(message.as_slice());
    Hash::new(mac.finalize().into_bytes().into())
}

/// AAD of the request ciphertext: `CBOR(["dmsg/connect-request/v1", account_id, pairing_id, request_id])`.
pub fn request_aad(account: &AccountId, pairing: &Hash, request: &Hash) -> Vec<u8> {
    canonical(&(REQUEST_AAD, account, pairing, request))
}

/// HPKE info and AAD of one recipient's wrapping:
/// `CBOR(["dmsg/connect-wrap/v1", account_id, pairing_id, request_id, device_id])`.
pub fn wrap_info(account: &AccountId, pairing: &Hash, request: &Hash, device: &Hash) -> Vec<u8> {
    canonical(&(WRAP_INFO, account, pairing, request, device))
}

/// HPKE info and AAD of a reply:
/// `CBOR(["dmsg/connect-response/v1", account_id, pairing_id, request_id, version, device_id])`.
pub fn response_info(
    account: &AccountId,
    pairing: &Hash,
    request: &Hash,
    version: u64,
    device: &Hash,
) -> Vec<u8> {
    canonical(&(RESPONSE_INFO, account, pairing, request, version, device))
}

/// Check a decoded request against this deployment and clock: protocol,
/// environment, origin, command, the kind its method sends and the
/// lifetime of that kind (24 hours for create, 2 minutes for read and open),
/// with `created_at` at most 60 seconds ahead.
///
/// The page signature, the pairing grant and the payload binding are
/// checked by the caller.
///
/// # Errors
/// Another protocol or environment returns `Error::UnsupportedProtocol`, an
/// expired or overlong request `Error::Expired`, anything else
/// `Error::InvalidInput`.
pub fn validate_request(
    request: &ConnectRequest,
    environment: &Environment,
    now: u64,
) -> Result<()> {
    ensure(
        request.protocol == CONNECT_PROTOCOL && request.environment == *environment,
        Error::UnsupportedProtocol,
    )?;
    validate_origin(&request.origin, environment)?;
    validate_command(&request.command)?;
    ensure_valid(
        request_kind(&request.command.method) == Some(request.kind),
        "request kind",
    )?;
    for id in [
        &request.pairing_id,
        &request.request_id,
        &request.connection_id,
        &request.nonce,
        &request.result_key,
    ] {
        crate::nonzero(id.as_slice())?;
    }
    let ttl = match request.kind {
        RequestKind::Create => CREATE_TTL_MS,
        RequestKind::Read | RequestKind::Open => READ_TTL_MS,
    };
    ensure(
        request.created_at <= now + CLOCK_SKEW_MS
            && now < request.expires_at
            && request.created_at < request.expires_at
            && request.expires_at - request.created_at <= ttl,
        Error::Expired,
    )
}

/// Check a grant's own fields: the key, its ID, the label and a lifetime of
/// at most 180 days. Its signer, home and revocation are checked against
/// certified data by the caller.
///
/// # Errors
/// An expired grant returns `Error::Expired`; anything else `Error::InvalidInput`.
pub fn validate_grant(grant: &PairingGrant, now: u64) -> Result<()> {
    validate_p256_spki(&grant.pairing_pub)?;
    ensure_valid(
        grant.pairing_id == pairing_id(&grant.pairing_pub),
        "pairing_id",
    )?;
    validate_label(&grant.label)?;
    ensure_valid(
        grant.granted_at < grant.expires_at
            && grant.expires_at - grant.granted_at <= PAIRING_TTL_MS,
        "pairing lifetime",
    )?;
    ensure(now < grant.expires_at, Error::Expired)
}

#[cfg(test)]
#[path = "connect_tests.rs"]
mod tests;
