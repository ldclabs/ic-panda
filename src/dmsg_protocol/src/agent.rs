//! Agent Delegation 1.0 rules for self-held controllers and principal publication.
//!
//! Principal documents follow the pinned `agent-protocols` SDK. This module
//! validates controller state, renders the principal document the directory
//! serves and routes accounts to their user homes. Delegation events are
//! signed by the client and checked by the delegation service against the
//! published document; nothing here authorizes an event.
use crate::*;
use agent_protocols::{delegation as sdk, identity as sdk_id};
use dmsg_types::{agent::*, *};
use std::collections::BTreeSet;

/// Maximum display label length in bytes (64).
pub const MAX_CONTROLLER_NAME_BYTES: usize = 64;
/// Maximum scopes in a restricted ceiling (8).
pub const MAX_CEILING_SCOPES: usize = 8;
/// Maximum audiences in a restricted ceiling (4).
pub const MAX_CEILING_AUDIENCES: usize = 4;
/// Maximum bytes of one scope string (64).
pub const MAX_SCOPE_BYTES: usize = 64;
/// Maximum bytes of one audience string (256).
pub const MAX_AUDIENCE_BYTES: usize = 256;
/// Maximum serialized principal or controller-source origin (512 bytes).
pub const MAX_PRINCIPAL_ORIGIN_BYTES: usize = 512;
/// Maximum directory query URL or profile prefix (2 KiB).
pub const MAX_DIRECTORY_URL_BYTES: usize = 2_048;
/// Maximum principal document (64 KiB), including future retirement fields.
pub const MAX_PRINCIPAL_DOCUMENT_BYTES: usize = 65_536;

fn sdk_error(error: agent_protocols::SdkError) -> Error {
    Error::InvalidInput(error.to_string())
}

fn millis(value: u64) -> Result<i64> {
    ensure_valid(value <= sdk_id::MAX_SAFE_NONCE, "timestamp")?;
    Ok(value as i64)
}

/// Validate a principal origin: an exact serialized HTTPS origin without a path.
///
/// # Errors
/// Anything else returns `Error::InvalidInput`.
pub fn validate_principal_origin(origin: &str) -> Result<()> {
    validate_document_url_bytes(origin, MAX_PRINCIPAL_ORIGIN_BYTES)?;
    sdk_id::validate_origin(origin).map_err(sdk_error)
}

fn validate_document_url_bytes(url: &str, limit: usize) -> Result<()> {
    // These public URLs need no JSON escaping, so their byte limits also bound
    // their contribution to a principal document. Encode special bytes in URLs.
    ensure_valid(
        url.len() <= limit
            && !url
                .bytes()
                .any(|b| b.is_ascii_control() || b == b'"' || b == b'\\'),
        "principal document URL",
    )
}

/// Canonical principal URL of an account: `origin/<account_id>`.
/// The origin must already satisfy [`validate_principal_origin`].
pub fn principal_id(origin: &str, account_id: &AccountId) -> String {
    format!("{origin}/{account_id}")
}

fn validate_authority(authority: &DelegationAuthority) -> Result<()> {
    let DelegationAuthority::Restricted { scopes, audiences } = authority else {
        return Ok(());
    };
    let unique = |values: &[String]| {
        let mut seen = BTreeSet::new();
        values.iter().all(|value| seen.insert(value))
    };
    ensure_valid(
        (1..=MAX_CEILING_SCOPES).contains(&scopes.len())
            && (1..=MAX_CEILING_AUDIENCES).contains(&audiences.len())
            && unique(scopes)
            && unique(audiences)
            && scopes
                .iter()
                .all(|s| s.len() <= MAX_SCOPE_BYTES && !s.trim().is_empty() && s != "*")
            && audiences.iter().all(|a| a.len() <= MAX_AUDIENCE_BYTES),
        "delegation ceiling",
    )?;
    for audience in audiences {
        sdk::validate_audience(audience).map_err(sdk_error)?;
    }
    Ok(())
}

/// Validate a controller label: 1..64 bytes, not blank, no control characters.
///
/// # Errors
/// Invalid labels return `Error::InvalidInput`.
pub fn validate_controller_name(name: &str) -> Result<()> {
    ensure_valid(
        !name.trim().is_empty()
            && name.len() <= MAX_CONTROLLER_NAME_BYTES
            && !name.chars().any(char::is_control),
        "controller name",
    )
}

/// Validate the invariants of a principal state before it is committed or published.
///
/// Generations are unique and ascending, record counts are bounded, a key
/// appears once, current records carry no retirement fields, retirement and
/// compromise intervals are ordered, every `supersedes` entry names an earlier
/// record, and every timestamp is at or before `updated_at`.
/// The document budget reserves the largest configured URLs, names and all
/// retirement/compromise fields, so later safety changes always remain publishable.
///
/// # Errors
/// Violations return `Error::InvalidInput` or, for count/byte limits, `Error::QuotaExceeded`.
pub fn validate_principal_state(state: &PrincipalState) -> Result<()> {
    ensure(
        state.controllers.len() <= MAX_CONTROLLER_RECORDS
            && state
                .controllers
                .iter()
                .filter(|c| c.retired_at.is_none())
                .count()
                <= MAX_CURRENT_CONTROLLERS,
        Error::QuotaExceeded,
    )?;
    millis(state.updated_at)?;
    let mut keys = BTreeSet::new();
    for (index, c) in state.controllers.iter().enumerate() {
        ensure_valid(
            c.generation > 0
                && index
                    .checked_sub(1)
                    .is_none_or(|p| state.controllers[p].generation < c.generation)
                && keys.insert(c.public_key),
            "controller generation/key",
        )?;
        crate::validate_ed25519_key(c.public_key.as_slice())?;
        if let Some(name) = &c.name {
            validate_controller_name(name)?;
        }
        validate_authority(&c.delegation)?;
        ensure_valid(c.valid_from <= state.updated_at, "valid_from")?;
        match c.retired_at {
            Some(retired_at) => ensure_valid(
                c.valid_from <= retired_at
                    && retired_at <= state.updated_at
                    && c.invalid_from
                        .is_none_or(|t| c.valid_from <= t && t <= retired_at),
                "retirement interval",
            )?,
            None => ensure_valid(c.invalid_from.is_none(), "current controller")?,
        }
        let mut seen = BTreeSet::new();
        for generation in &c.supersedes {
            ensure_valid(
                seen.insert(*generation)
                    && state.controllers[..index]
                        .iter()
                        .any(|p| p.generation == *generation && p.valid_from < c.valid_from),
                "supersedes",
            )?;
        }
    }
    ensure(
        principal_document_size_bound(state) <= MAX_PRINCIPAL_DOCUMENT_BYTES,
        Error::QuotaExceeded,
    )
}

/// Conservative JCS byte budget after the count and field checks above.
/// 1 KiB covers the envelope syntax, account ID and full-width updated_at;
/// 512 bytes per controller covers syntax, Agent ID, a maximally escaped name
/// and three full-width timestamps. Variable immutable fields are added below.
fn principal_document_size_bound(state: &PrincipalState) -> usize {
    let mut bytes = 1_024 + MAX_PRINCIPAL_ORIGIN_BYTES + 2 * MAX_DIRECTORY_URL_BYTES;
    for c in &state.controllers {
        bytes += 512 + MAX_PRINCIPAL_ORIGIN_BYTES;
        // A did:agent ID is 53 ASCII bytes, plus two quotes and a comma.
        bytes += c.supersedes.len() * 56;
        if let DelegationAuthority::Restricted { scopes, audiences } = &c.delegation {
            for value in scopes.iter().chain(audiences) {
                // Include the JSON quotes and separating comma without allocating.
                bytes += 3 + value
                    .bytes()
                    .map(|b| match b {
                        b'"' | b'\\' | b'\x08' | b'\t' | b'\n' | b'\x0c' | b'\r' => 2,
                        0..=0x1f => 6,
                        _ => 1,
                    })
                    .sum::<usize>();
            }
        }
    }
    bytes
}

fn principal_type(value: &PrincipalType) -> &'static str {
    match value {
        PrincipalType::Person => "person",
        PrincipalType::Organization => "organization",
        PrincipalType::Team => "team",
        PrincipalType::Project => "project",
        PrincipalType::Other => "other",
    }
}

/// Render the exact JCS principal document the directory serves for an account.
///
/// The document names `principal_origin/<account_id>` as its `id`, lists
/// current and retired hosted controllers with the configured `source`, the
/// configured `delegation_query_url`, and one `profile` link. It is checked with
/// the SDK's `validate_principal_document` before the bytes are returned.
///
/// # Errors
/// Invalid state or configuration returns `Error::InvalidInput` or `Error::QuotaExceeded`.
pub fn render_principal_document(
    config: &DirectoryInit,
    account_id: &AccountId,
    state: &PrincipalState,
) -> Result<Vec<u8>> {
    validate_principal_state(state)?;
    let key = |generation: u32| {
        state
            .controllers
            .iter()
            .find(|c| c.generation == generation)
            .map(|c| sdk_id::AgentId::from_public_key(&c.public_key))
            .ok_or_else(|| invalid("supersedes"))
    };
    let mut controllers = Vec::new();
    let mut retired_controllers = Vec::new();
    for c in &state.controllers {
        let record = sdk::Controller {
            id: sdk_id::AgentId::from_public_key(&c.public_key),
            source: config.controller_source.clone(),
            valid_from: millis(c.valid_from)?,
            name: c.name.clone(),
            delegation: Some(match &c.delegation {
                DelegationAuthority::Unrestricted => {
                    sdk::DelegationAuthority::Unrestricted("*".into())
                }
                DelegationAuthority::Restricted { scopes, audiences } => {
                    sdk::DelegationAuthority::Restricted(sdk::DelegationPolicy {
                        scopes: scopes.clone(),
                        audiences: audiences.clone(),
                    })
                }
            }),
            supersedes: (!c.supersedes.is_empty())
                .then(|| c.supersedes.iter().map(|g| key(*g)).collect::<Result<_>>())
                .transpose()?,
            retired_at: c.retired_at.map(millis).transpose()?,
            invalid_from: c.invalid_from.map(millis).transpose()?,
        };
        if c.retired_at.is_some() {
            retired_controllers.push(record);
        } else {
            controllers.push(record);
        }
    }
    let document = sdk::PrincipalDocument {
        id: principal_id(&config.principal_origin, account_id),
        kind: Some(principal_type(&state.principal_type).into()),
        name: None,
        description: None,
        avatar_url: None,
        aliases: vec![],
        links: vec![sdk::PrincipalLink {
            name: "dMsg".into(),
            url: format!("{}{account_id}", config.profile_url_prefix),
            rel: "profile".into(),
        }],
        protocol: AGENT_DELEGATION_PROTOCOL.into(),
        controllers,
        retired_controllers,
        delegation_query_url: Some(config.delegation_query_url.clone()),
        updated_at: millis(state.updated_at)?,
        extra: Default::default(),
    };
    sdk::validate_principal_document(&document).map_err(sdk_error)?;
    serde_jcs::to_vec(&document).map_err(|error| Error::InvalidInput(error.to_string()))
}

/// Validate the domains served at `/.well-known/ic-domains`: at most eight
/// lowercase DNS names of 1..253 bytes.
pub fn validate_custom_domains(domains: &[String]) -> Result<()> {
    ensure_valid(
        domains.len() <= 8
            && domains.iter().all(|d| {
                !d.is_empty()
                    && d.len() <= 253
                    && d.bytes().all(|b| {
                        b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-'
                    })
            }),
        "directory domains",
    )
}

/// Validate a directory configuration without accessing any canister.
///
/// Requires a valid issuer namespace, user homes accepted by
/// [`validate_user_homes`], an authenticated governance Principal, HTTPS
/// origins for principals and controller source, an HTTPS query URL, an HTTPS
/// profile prefix ending in `/`, and domains accepted by
/// [`validate_custom_domains`]. Origins are at most 512 bytes and the query
/// URL/profile prefix at most 2 KiB, without JSON escape characters, matching
/// the shared principal-document byte budget.
///
/// # Errors
/// Invalid configuration returns `Error::InvalidInput`, `Error::AuthRequired`,
/// `Error::QuotaExceeded` or, for colliding home fingerprints, `Error::IntegrityFailed`.
pub fn validate_directory_init(config: &DirectoryInit) -> Result<()> {
    validate_namespace(&config.issuer_namespace)?;
    validate_user_homes(
        &config.environment,
        &config.issuer_namespace,
        &config.user_homes,
    )?;
    validate_custom_domains(&config.custom_domains)?;
    authenticated(config.governance)?;
    validate_principal_origin(&config.principal_origin)?;
    validate_principal_origin(&config.controller_source)?;
    for url in [&config.delegation_query_url, &config.profile_url_prefix] {
        validate_document_url_bytes(url, MAX_DIRECTORY_URL_BYTES)?;
        let parsed = url::Url::parse(url).map_err(|_| invalid("directory URL"))?;
        ensure_valid(
            parsed.scheme() == "https"
                && parsed.as_str() == url.as_str()
                && parsed.query().is_none()
                && parsed.fragment().is_none(),
            "directory URL",
        )?;
    }
    ensure_valid(config.profile_url_prefix.ends_with('/'), "profile prefix")
}

/// Digest committing to an account-ID allocator's deployment namespace.
///
/// The first five bytes are the allocator fingerprint embedded at bytes 4..9 of
/// every account ID the user home allocates, which lets the directory route an
/// account to the home that created it.
pub fn account_allocator_digest(
    environment: &Environment,
    namespace: &str,
    canister: candid::Principal,
) -> Hash {
    digest(
        "dmsg/account-id-generator/v1",
        &("dmsg", environment, namespace, canister),
    )
}

/// Whether an account ID was allocated by the home with this allocator digest.
pub fn allocated_by(account_id: &AccountId, allocator_digest: &Hash) -> bool {
    account_id.as_slice()[4..9] == allocator_digest[..5]
}

/// Maximum user homes one service routes accounts to (64).
pub const MAX_USER_HOMES: usize = 64;

/// Check that `home` may be appended to `homes`; false when it is already listed.
///
/// Distinct allocator fingerprints keep every account routed to exactly one
/// home. The home's `dmsg_user` must share the environment and namespace.
pub fn check_user_home(
    environment: &Environment,
    namespace: &str,
    homes: &[candid::Principal],
    home: candid::Principal,
) -> Result<bool> {
    if homes.contains(&home) {
        return Ok(false);
    }
    authenticated(home)?;
    ensure(homes.len() < MAX_USER_HOMES, Error::QuotaExceeded)?;
    let fingerprint = account_allocator_digest(environment, namespace, home);
    ensure(
        homes
            .iter()
            .all(|h| account_allocator_digest(environment, namespace, *h)[..5] != fingerprint[..5]),
        Error::IntegrityFailed,
    )?;
    Ok(true)
}

/// Validate a non-empty initial user-home list, in order, with [`check_user_home`].
pub fn validate_user_homes(
    environment: &Environment,
    namespace: &str,
    homes: &[candid::Principal],
) -> Result<()> {
    ensure_valid(!homes.is_empty(), "user homes")?;
    for (index, home) in homes.iter().enumerate() {
        ensure_valid(
            check_user_home(environment, namespace, &homes[..index], *home)?,
            "duplicate user home",
        )?;
    }
    Ok(())
}

/// The listed user home whose allocator fingerprint the account ID carries.
pub fn account_home(
    environment: &Environment,
    namespace: &str,
    homes: &[candid::Principal],
    account_id: &AccountId,
) -> Option<candid::Principal> {
    homes.iter().copied().find(|home| {
        allocated_by(
            account_id,
            &account_allocator_digest(environment, namespace, *home),
        )
    })
}

/// Whether `home` is listed and allocated the account; one digest, no scan.
pub fn is_account_home(
    environment: &Environment,
    namespace: &str,
    homes: &[candid::Principal],
    home: candid::Principal,
    account_id: &AccountId,
) -> bool {
    homes.contains(&home)
        && allocated_by(
            account_id,
            &account_allocator_digest(environment, namespace, home),
        )
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
