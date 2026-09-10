//! Identity identifiers are data, never automatic network discovery instructions.
use crate::ensure_valid;
use candid::Principal;
use dmsg_types::*;

#[cfg(test)]
mod tests;

/// Maximum canonical identity URI or statement subject size in bytes (8,192).
pub const MAX_URI_BYTES: usize = 8192;
const MAX_NAMESPACE_BYTES: usize = MAX_URI_BYTES - 64;

fn parse_uri(value: &str) -> Result<url::Url> {
    ensure_valid(
        !value.is_empty() && value.len() <= MAX_URI_BYTES,
        "absolute URI",
    )?;
    for (index, byte) in value.bytes().enumerate() {
        if !byte.is_ascii_graphic() {
            return Err(invalid("absolute URI"));
        }
        if byte == b'%' {
            ensure_valid(
                value
                    .as_bytes()
                    .get(index + 1..index + 3)
                    .is_some_and(|escape| escape.iter().all(u8::is_ascii_hexdigit)),
                "URI percent escape",
            )?;
        }
    }
    let uri = url::Url::parse(value).map_err(|_| invalid("absolute URI"))?;
    ensure_valid(
        uri.as_str() == value && uri.username().is_empty() && uri.password().is_none(),
        "canonical URI",
    )?;
    Ok(uri)
}

/// Validate a canonical absolute ASCII URI without rewriting it or fetching it.
///
/// Requires 1..8192 bytes, printable ASCII, valid percent escapes, no credentials,
/// and an exact round-trip through the URL parser. This does not require HTTPS.
///
/// # Errors
/// Malformed or noncanonical identifiers return `Error::InvalidInput`.
pub fn validate_uri(value: &str) -> Result<()> {
    parse_uri(value).map(|_| ())
}

/// Validate a fixed identity URI prefix before appending an identity.
///
/// Requires a canonical URI of at most 8128 bytes, ending in `/` or `:`, with
/// no query or fragment. This does not discover an identity service.
///
/// # Errors
/// Invalid prefixes return `Error::InvalidInput`.
pub fn validate_namespace(value: &str) -> Result<()> {
    ensure_valid(
        value.len() <= MAX_NAMESPACE_BYTES && (value.ends_with('/') || value.ends_with(':')),
        "identity namespace",
    )?;
    let uri = parse_uri(value)?;
    ensure_valid(
        uri.query().is_none() && uri.fragment().is_none(),
        "identity namespace",
    )
}

/// Append canonical Xid text to a validated identity namespace.
///
/// Does not allocate an ID, check account existence, or prove ownership.
///
/// # Errors
/// Returns namespace validation errors from [`validate_namespace`].
pub fn account_issuer(namespace: &str, id: &AccountId) -> Result<String> {
    validate_namespace(namespace)?;
    // Canonical Xid text contains only unreserved ASCII. Appending it to the
    // validated prefix cannot introduce escaping, credentials or dot segments.
    Ok(format!("{namespace}{id}"))
}

/// Append canonical Principal text to a validated identity namespace.
///
/// This formats an identifier; it does not authenticate the Principal or reject
/// anonymous/management Principals as [`crate::authenticated`] does.
///
/// # Errors
/// Returns namespace validation errors from [`validate_namespace`].
pub fn principal_issuer(namespace: &str, principal: Principal) -> Result<String> {
    validate_namespace(namespace)?;
    // Principal text is also unreserved ASCII and fits the 64-byte reservation.
    Ok(format!("{namespace}{}", principal.to_text()))
}

/// Extract a canonical Xid from an issuer in the exact expected namespace.
///
/// The namespace is explicit: no identity type is inferred from byte length.
/// Parsing does not authenticate an account or its signing key.
///
/// # Errors
/// Invalid namespaces return `Error::InvalidInput`; a mismatched prefix or
/// noncanonical Xid suffix returns `Error::IntegrityFailed`.
pub fn parse_account_issuer(namespace: &str, issuer: &str) -> Result<AccountId> {
    validate_namespace(namespace)?;
    // Xid::from_str enforces exact length, lowercase alphabet and zero padding
    // bits. A validated prefix plus that suffix is already a canonical URI.
    issuer
        .strip_prefix(namespace)
        .ok_or(Error::IntegrityFailed)?
        .parse::<AccountId>()
        .map_err(|_| Error::IntegrityFailed)
}

/// Validate an exact HTTPS origin or Chrome extension origin, at most 256 bytes.
///
/// HTTPS values must equal the URL parser's origin serialization (no path,
/// trailing slash, credentials, query or fragment). Extension IDs must contain
/// 32 lowercase letters in a..p. This validates syntax only; the extension must
/// independently obtain and check the actual browser origin.
///
/// # Errors
/// Invalid origins return `Error::InvalidInput`.
pub fn validate_origin(origin: &str) -> Result<()> {
    ensure_valid(origin.len() <= 256, "origin")?;
    if let Some(id) = origin.strip_prefix("chrome-extension://") {
        return ensure_valid(
            id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b)),
            "origin",
        );
    }
    let parsed = url::Url::parse(origin).map_err(|_| invalid("origin"))?;
    ensure_valid(
        parsed.scheme() == "https" && parsed.origin().ascii_serialization() == origin,
        "origin",
    )
}
