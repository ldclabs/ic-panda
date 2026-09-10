//! Identity identifiers are data, never automatic network discovery instructions.
use candid::Principal;
use dmsg_types::*;

pub const MAX_URI_BYTES: usize = 8192;
pub fn validate_uri(value: &str) -> Result<()> {
    ensure(
        !value.is_empty()
            && value.len() <= MAX_URI_BYTES
            && value.is_ascii()
            && !value
                .bytes()
                .any(|b| b.is_ascii_whitespace() || b.is_ascii_control()),
        invalid("absolute URI"),
    )?;
    for (index, byte) in value.bytes().enumerate() {
        if byte == b'%' {
            ensure(
                value
                    .as_bytes()
                    .get(index + 1)
                    .is_some_and(u8::is_ascii_hexdigit)
                    && value
                        .as_bytes()
                        .get(index + 2)
                        .is_some_and(u8::is_ascii_hexdigit),
                invalid("URI percent escape"),
            )?;
        }
    }
    let uri = url::Url::parse(value).map_err(|_| invalid("absolute URI"))?;
    ensure(
        uri.as_str() == value && uri.username().is_empty() && uri.password().is_none(),
        invalid("canonical URI"),
    )
}
pub fn validate_namespace(value: &str) -> Result<()> {
    validate_uri(value)?;
    let uri = url::Url::parse(value).map_err(|_| invalid("identity namespace"))?;
    ensure(
        (value.ends_with('/') || value.ends_with(':'))
            && uri.query().is_none()
            && uri.fragment().is_none()
            && value.len() <= MAX_URI_BYTES - 64,
        invalid("identity namespace"),
    )
}
pub fn account_issuer(namespace: &str, id: &AccountId) -> Result<String> {
    validate_namespace(namespace)?;
    let result = format!("{namespace}{id}");
    validate_uri(&result)?;
    Ok(result)
}
pub fn principal_issuer(namespace: &str, principal: Principal) -> Result<String> {
    validate_namespace(namespace)?;
    let result = format!("{namespace}{}", principal.to_text());
    validate_uri(&result)?;
    Ok(result)
}
pub fn parse_account_issuer(namespace: &str, issuer: &str) -> Result<AccountId> {
    validate_namespace(namespace)?;
    validate_uri(issuer)?;
    let id = issuer
        .strip_prefix(namespace)
        .ok_or(Error::IntegrityFailed)?
        .parse::<AccountId>()
        .map_err(|_| Error::IntegrityFailed)?;
    ensure(
        account_issuer(namespace, &id)? == issuer,
        Error::IntegrityFailed,
    )?;
    Ok(id)
}
pub fn validate_origin(origin: &str) -> Result<()> {
    ensure(origin.len() <= 256, invalid("origin"))?;
    let parsed = url::Url::parse(origin).map_err(|_| invalid("origin"))?;
    let extension = origin
        .strip_prefix("chrome-extension://")
        .is_some_and(|id| id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b)));
    ensure(
        (parsed.scheme() == "https" && parsed.origin().ascii_serialization() == origin)
            || extension,
        invalid("origin"),
    )
}
