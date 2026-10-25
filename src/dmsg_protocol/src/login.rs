//! dmsg-login/1 checks shared by the login page vectors and local clients.
//!
//! Messages are canonical CBOR; the base64url fragment wrapping is left to
//! the transport. See `docs/protocol/login-v1_zh.md`.
use crate::{canonical, decode_canonical};
use dmsg_types::{login::*, *};

/// Derivation origins a login request may name.
pub const DERIVATION_ORIGINS: [&str; 2] = ["https://dmsg.net", "https://panda.fans"];
/// Shortest delegation lifetime a request may ask for (one minute).
pub const MIN_TTL_MS: u64 = MINUTE;
/// Longest delegation lifetime a request may ask for (thirty days, Internet
/// Identity's own limit).
pub const MAX_TTL_MS: u64 = 30 * DAY;
/// Path of the client's callback page.
pub const CALLBACK_PATH: &str = "/auth/callback";
/// Fixed Associated Domains callback for the native iOS application.
pub const IOS_CALLBACK: &str = "https://dmsg.net/_/ios/auth/callback";
/// A callback's closed set of transports; arbitrary HTTPS redirects are refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallbackTarget {
    /// A desktop client's bound loopback listener.
    Loopback(u16),
    /// The iOS app's associated HTTPS callback.
    Ios,
}

/// Validate the exact callback spelling before contacting Internet Identity.
///
/// # Errors
/// Any target outside this closed set returns `InvalidInput`.
pub fn callback_target(next_url: &str) -> Result<CallbackTarget> {
    if next_url == IOS_CALLBACK {
        Ok(CallbackTarget::Ios)
    } else {
        callback_port(next_url).map(CallbackTarget::Loopback)
    }
}
/// DER prefix of an Ed25519 SubjectPublicKeyInfo; the raw key follows.
pub const ED25519_SPKI_PREFIX: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

/// The port of a callback URL, which must be exactly
/// `http://127.0.0.1:<port>/auth/callback` or `http://[::1]:<port>/auth/callback`.
///
/// # Errors
/// Any other host, scheme, path, query, fragment or port spelling returns
/// `Error::InvalidInput`.
pub fn callback_port(next_url: &str) -> Result<u16> {
    let rest = next_url
        .strip_prefix("http://127.0.0.1:")
        .or_else(|| next_url.strip_prefix("http://[::1]:"))
        .ok_or_else(|| invalid("next_url host"))?;
    let port = rest
        .strip_suffix(CALLBACK_PATH)
        .ok_or_else(|| invalid("next_url path"))?;
    ensure_valid(
        !port.is_empty()
            && port.len() <= 5
            && port.bytes().all(|b| b.is_ascii_digit())
            && !port.starts_with('0'),
        "next_url port",
    )?;
    port.parse().map_err(|_| invalid("next_url port"))
}

/// Check every field of a request; the page contacts Internet Identity only
/// after this passes.
///
/// # Errors
/// Returns `Error::InvalidInput` naming the first field that fails.
pub fn validate_request(request: &LoginRequest) -> Result<()> {
    ensure_valid(request.v == LOGIN_VERSION, "v")?;
    crate::nonzero(request.state.as_slice())?;
    ensure_valid(
        request.session_pubkey.len() == 44
            && request.session_pubkey.starts_with(&ED25519_SPKI_PREFIX),
        "session_pubkey",
    )?;
    ensure_valid(
        (MIN_TTL_MS..=MAX_TTL_MS).contains(&request.ttl_ms),
        "ttl_ms",
    )?;
    ensure_valid(
        DERIVATION_ORIGINS.contains(&request.derivation_origin.as_str()),
        "derivation_origin",
    )?;
    callback_target(&request.next_url).map(|_| ())
}

/// Canonical CBOR of a login message.
pub fn encode_login<T: serde::Serialize>(message: &T) -> Vec<u8> {
    canonical(message)
}

/// Decode and check a canonical request.
///
/// # Errors
/// Non-canonical bytes or unknown fields return `IntegrityFailed`; a failed
/// field check returns `InvalidInput`.
pub fn decode_request(bytes: &[u8]) -> Result<LoginRequest> {
    let request: LoginRequest = decode_canonical(bytes)?;
    validate_request(&request)?;
    Ok(request)
}

/// Decode a canonical callback body. The caller still checks `state` and the
/// delegation against its own request.
///
/// # Errors
/// Non-canonical bytes, unknown fields or another version return `IntegrityFailed`.
pub fn decode_callback(bytes: &[u8]) -> Result<LoginCallback> {
    let callback: LoginCallback = decode_canonical(bytes)?;
    let v = match &callback {
        LoginCallback::Result(result) => result.v,
        LoginCallback::Error(error) => error.v,
    };
    ensure(v == LOGIN_VERSION, Error::IntegrityFailed)?;
    Ok(callback)
}

#[cfg(test)]
#[path = "login_tests.rs"]
mod tests;
