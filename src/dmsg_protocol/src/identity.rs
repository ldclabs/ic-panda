//! Identity identifiers are data, never automatic network discovery instructions.
use candid::Principal;
use dmsg_types::*;

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

/// Append canonical Xid text to a namespace accepted by [`validate_namespace`].
///
/// Validate the namespace once when it is configured. Canonical Xid text contains
/// only unreserved ASCII, so appending it to a validated prefix cannot introduce
/// escaping, credentials or dot segments. This does not allocate an ID, check
/// account existence, or prove ownership.
pub fn account_issuer(namespace: &str, id: &AccountId) -> String {
    format!("{namespace}{id}")
}

/// Append canonical Principal text to a namespace accepted by [`validate_namespace`].
///
/// Principal text is also unreserved ASCII and fits the 64-byte namespace reservation.
/// This formats an identifier; it does not authenticate the Principal or reject
/// anonymous/management Principals as [`crate::authenticated`] does.
pub fn principal_issuer(namespace: &str, principal: Principal) -> String {
    format!("{namespace}{}", principal.to_text())
}

/// Extract a canonical Xid from an issuer in the exact expected namespace.
///
/// The namespace must already be accepted by [`validate_namespace`]; no identity
/// type is inferred from byte length. Parsing does not authenticate an account or
/// its signing key.
///
/// # Errors
/// A mismatched prefix or noncanonical Xid suffix returns `Error::IntegrityFailed`.
pub fn parse_account_issuer(namespace: &str, issuer: &str) -> Result<AccountId> {
    // Xid::from_str enforces exact length, lowercase alphabet and zero padding
    // bits. A validated prefix plus that suffix is already a canonical URI.
    issuer
        .strip_prefix(namespace)
        .ok_or(Error::IntegrityFailed)?
        .parse::<AccountId>()
        .map_err(|_| Error::IntegrityFailed)
}

/// Validate an exact browser origin of at most 256 bytes.
///
/// Accepts a Chrome extension origin whose ID contains 32 lowercase letters in
/// a..p, or an HTTPS origin equal to the URL parser's origin serialization (no
/// path, trailing slash, credentials, query or fragment). Local deployments also
/// accept exact loopback HTTP origins (`localhost`, `127.0.0.1`, `[::1]`). This
/// validates syntax only; the extension must independently obtain and check the
/// actual browser origin.
///
/// # Errors
/// Invalid origins return `Error::InvalidInput`.
pub fn validate_origin(origin: &str, environment: &Environment) -> Result<()> {
    ensure_valid(origin.len() <= 256, "origin")?;
    if let Some(id) = origin.strip_prefix("chrome-extension://") {
        return ensure_valid(
            id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b)),
            "origin",
        );
    }
    let url = url::Url::parse(origin).map_err(|_| invalid("origin"))?;
    let loopback = *environment == Environment::Local
        && url.scheme() == "http"
        && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    ensure_valid(
        (url.scheme() == "https" || loopback) && url.origin().ascii_serialization() == origin,
        "origin",
    )
}

#[cfg(test)]
mod tests;
