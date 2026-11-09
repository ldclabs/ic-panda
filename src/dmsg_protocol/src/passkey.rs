//! WebAuthn verification shared by the auth canister and protocol tests.
use crate::sha256;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use cbor2::Value;
use dmsg_types::{auth::*, *};
use p256::ecdsa::{Signature, VerifyingKey, signature::Verifier};
use serde::Deserialize;

/// DER prefix of an uncompressed P-256 SubjectPublicKeyInfo.
pub const P256_SPKI_PREFIX: &[u8] = &[
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
];
/// Maximum credential ID, in bytes.
pub const MAX_CREDENTIAL_ID: usize = 1024;
/// Maximum browser callback, with CBOR framing.
pub const MAX_PASSKEY_CALLBACK: usize = 24 * 1024;

/// Validate a hardware caller's public key and return its SEC1 point.
/// # Errors
/// Malformed or non-P-256 DER is refused.
pub fn hardware_public(der: &[u8]) -> Result<&[u8]> {
    ensure(
        der.len() == 91 && der.starts_with(P256_SPKI_PREFIX),
        Error::IntegrityFailed,
    )?;
    let point = &der[P256_SPKI_PREFIX.len()..];
    ensure(point[0] == 4, Error::IntegrityFailed)?;
    VerifyingKey::from_sec1_bytes(point).map_err(|_| Error::IntegrityFailed)?;
    Ok(point)
}

/// Encode a platform's SEC1 point for IC ingress authentication.
/// # Errors
/// Refuses an invalid point.
pub fn hardware_der(point: &[u8]) -> Result<Vec<u8>> {
    let key = VerifyingKey::from_sec1_bytes(point).map_err(|_| Error::IntegrityFailed)?;
    Ok([P256_SPKI_PREFIX, key.to_sec1_point(false).as_bytes()].concat())
}

/// Bound and hash a credential identifier.
/// # Errors
/// Empty or oversized identifiers are refused.
pub fn credential_hash(id: &[u8]) -> Result<Hash> {
    ensure(
        !id.is_empty() && id.len() <= MAX_CREDENTIAL_ID,
        Error::IntegrityFailed,
    )?;
    Ok(sha256(id))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientData {
    #[serde(rename = "type")]
    kind: String,
    challenge: String,
    origin: String,
    #[serde(default)]
    cross_origin: bool,
    top_origin: Option<String>,
}

fn client_data(response: &PasskeyResponse, challenge: &Hash, register: bool) -> Result<Hash> {
    credential_hash(&response.credential_id)?;
    ensure(
        response.client_data_json.len() <= 4096,
        Error::QuotaExceeded,
    )?;
    let data: ClientData =
        serde_json::from_slice(&response.client_data_json).map_err(|_| Error::IntegrityFailed)?;
    ensure(
        data.kind
            == (if register {
                "webauthn.create"
            } else {
                "webauthn.get"
            })
            && data.challenge == URL_SAFE_NO_PAD.encode(challenge.as_slice())
            && data.origin == PASSKEY_ORIGIN
            && !data.cross_origin
            && data.top_origin.is_none(),
        Error::IntegrityFailed,
    )?;
    Ok(sha256(&response.client_data_json))
}

fn authenticator(data: &[u8], registration: bool) -> Result<()> {
    ensure((37..=8192).contains(&data.len()), Error::IntegrityFailed)?;
    let flags = data[32];
    ensure(
        data[..32] == sha256(PASSKEY_RP.as_bytes())[..]
            && flags & 0x05 == 0x05
            && (flags & 0x40 != 0) == registration
            && (flags & 0x10 == 0 || flags & 0x08 != 0),
        Error::IntegrityFailed,
    )
}

fn cbor(bytes: &[u8]) -> Result<Value> {
    let mut de = cbor2::de::Deserializer::from_slice_with_recursion_limit(bytes, 16);
    let value = Value::deserialize(&mut de).map_err(|_| Error::IntegrityFailed)?;
    ensure(de.offset() == bytes.len(), Error::IntegrityFailed)?;
    Ok(value)
}

fn field<'a>(entries: &'a [(Value, Value)], key: &Value) -> Result<&'a Value> {
    let mut values = entries.iter().filter(|(k, _)| k == key).map(|(_, v)| v);
    let first = values.next().ok_or(Error::IntegrityFailed)?;
    ensure(values.next().is_none(), Error::IntegrityFailed)?;
    Ok(first)
}

fn integer(n: i64) -> Value {
    Value::Integer(n.into())
}

/// Verify a none-attestation registration and return the ES256 SEC1 public key.
/// # Errors
/// Refuses another RP/origin/challenge, missing UV, unsupported keys or malformed data.
pub fn verify_registration(response: &PasskeyResponse, challenge: &Hash) -> Result<Vec<u8>> {
    client_data(response, challenge, true)?;
    ensure(
        response.signature.is_empty() && response.authenticator_data.len() <= 8192,
        Error::IntegrityFailed,
    )?;
    let Value::Map(attestation) = cbor(&response.authenticator_data)? else {
        return Err(Error::IntegrityFailed);
    };
    ensure(
        field(&attestation, &Value::Text("fmt".into()))? == &Value::Text("none".into()),
        Error::IntegrityFailed,
    )?;
    ensure(
        field(&attestation, &Value::Text("attStmt".into()))? == &Value::Map(vec![]),
        Error::IntegrityFailed,
    )?;
    let Value::Bytes(data) = field(&attestation, &Value::Text("authData".into()))? else {
        return Err(Error::IntegrityFailed);
    };
    authenticator(data, true)?;
    ensure(data.len() >= 55, Error::IntegrityFailed)?;
    let id_len = u16::from_be_bytes([data[53], data[54]]) as usize;
    ensure(
        id_len == response.credential_id.len() && data.len() > 55 + id_len,
        Error::IntegrityFailed,
    )?;
    ensure(
        data[55..55 + id_len] == response.credential_id[..],
        Error::IntegrityFailed,
    )?;
    let mut de = cbor2::de::Deserializer::from_slice_with_recursion_limit(&data[55 + id_len..], 16);
    let Value::Map(key) = Value::deserialize(&mut de).map_err(|_| Error::IntegrityFailed)? else {
        return Err(Error::IntegrityFailed);
    };
    if data[32] & 0x80 != 0 {
        let _: Value = Value::deserialize(&mut de).map_err(|_| Error::IntegrityFailed)?;
    }
    ensure(
        de.offset() == data.len() - 55 - id_len,
        Error::IntegrityFailed,
    )?;
    ensure(
        field(&key, &integer(1))? == &integer(2)
            && field(&key, &integer(3))? == &integer(-7)
            && field(&key, &integer(-1))? == &integer(1),
        Error::IntegrityFailed,
    )?;
    let (Value::Bytes(x), Value::Bytes(y)) =
        (field(&key, &integer(-2))?, field(&key, &integer(-3))?)
    else {
        return Err(Error::IntegrityFailed);
    };
    ensure(x.len() == 32 && y.len() == 32, Error::IntegrityFailed)?;
    let public = [&[4][..], x, y].concat();
    VerifyingKey::from_sec1_bytes(&public).map_err(|_| Error::IntegrityFailed)?;
    Ok(public)
}

/// Verify an assertion against the registered credential's public key.
/// # Errors
/// Refuses invalid client/authenticator data and incorrect DER ES256 signatures.
pub fn verify_assertion(response: &PasskeyResponse, challenge: &Hash, public: &[u8]) -> Result<()> {
    let client_hash = client_data(response, challenge, false)?;
    authenticator(&response.authenticator_data, false)?;
    if response.authenticator_data[32] & 0x80 == 0 {
        ensure(
            response.authenticator_data.len() == 37,
            Error::IntegrityFailed,
        )?;
    } else {
        cbor(&response.authenticator_data[37..])?;
    }
    ensure(response.signature.len() <= 80, Error::IntegrityFailed)?;
    let signature = Signature::from_der(&response.signature).map_err(|_| Error::IntegrityFailed)?;
    let key = VerifyingKey::from_sec1_bytes(public).map_err(|_| Error::IntegrityFailed)?;
    key.verify(
        &[response.authenticator_data.as_ref(), client_hash.as_slice()].concat(),
        &signature,
    )
    .map_err(|_| Error::IntegrityFailed)
}

#[cfg(test)]
#[path = "passkey_tests.rs"]
mod tests;
