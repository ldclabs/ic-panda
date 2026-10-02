//! Agent Delegation 1.0 rules for hosted controllers and principal publication.
//!
//! Strict I-JSON, JCS, SHA3-256 event hashes and the Agent Delegation payload
//! rules come from the pinned `agent-protocols` SDK. This module adds dMsg's
//! hosted-signing policy and renders the principal document the directory
//! serves. It performs no network access and authorizes nothing by itself.
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
/// Hosted signing accepts `created_at` at most this far in the past (60 s).
pub const EVENT_PAST_SKEW: u64 = 60 * SECOND;
/// Hosted signing accepts `created_at` at most this far in the future (5 s).
pub const EVENT_FUTURE_SKEW: u64 = 5 * SECOND;

fn sdk_error(error: agent_protocols::SdkError) -> Error {
    Error::InvalidInput(error.to_string())
}

fn millis(value: u64) -> Result<i64> {
    ensure_valid(value <= sdk_id::MAX_SAFE_NONCE, "timestamp")?;
    Ok(value as i64)
}

/// Agent ID (`did:agent:` + unpadded base64url) of a raw Ed25519 public key.
pub fn agent_id(public_key: &Hash) -> String {
    sdk_id::AgentId::from_public_key(public_key).to_string()
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

/// Required prefix of delegation IDs signed by an account's hosted keys.
/// The prefix routes credential reads to the account without a global index.
pub fn delegation_id_prefix(account_id: &AccountId) -> String {
    format!("{account_id}.")
}

/// SHA3-256 of exact event bytes: the 32-byte message a controller signs.
/// Only meaningful for bytes already accepted by [`parse_delegation_event`].
pub fn event_hash(bytes: &[u8]) -> Hash {
    use sha3::Digest;
    Hash::new(sha3::Sha3_256::digest(bytes).into())
}

/// Grant fields that hosted-signing policy checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationGrant {
    /// Granted scopes.
    pub scopes: Vec<String>,
    /// Granted relying-party audiences.
    pub audiences: Vec<String>,
    /// Optional validity start.
    pub not_before: Option<u64>,
    /// Optional validity end.
    pub expires_at: Option<u64>,
    /// JCS size of `constraints`, or zero when absent.
    pub constraints_bytes: usize,
}

/// A strictly parsed Agent Delegation event and its SHA3-256 event hash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationEvent {
    /// SHA3-256 of the exact event bytes: the 32-byte message that is signed.
    pub hash: Hash,
    /// Raw Ed25519 public key of `actor`.
    pub actor: Hash,
    /// Signer-chosen creation time.
    pub created_at: u64,
    /// Agent Identity nonce of the actor.
    pub nonce: u64,
    /// Principal named by the payload.
    pub principal_id: String,
    /// Delegation ID named by the payload.
    pub id: String,
    /// Grant fields, or None for `delegation.revoke`.
    pub grant: Option<DelegationGrant>,
}

/// Strictly parse one Agent Delegation event from its exact JCS bytes.
///
/// Rejects anything but the six Agent Identity event fields, duplicate member
/// names, unpaired surrogates, unsafe integers, noncanonical text, unknown or
/// explicitly null payload members, and payloads that break Agent Delegation
/// Section 7. The bytes must be the JCS form, so their SHA3-256 is the event hash.
///
/// # Errors
/// Oversized input returns `Error::QuotaExceeded`, a foreign protocol or type
/// `Error::UnsupportedProtocol`, and any other violation `Error::InvalidInput`.
pub fn parse_delegation_event(bytes: &[u8]) -> Result<DelegationEvent> {
    ensure(bytes.len() <= MAX_AGENT_EVENT_BYTES, Error::QuotaExceeded)?;
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("event is not UTF-8"))?;
    let value = sdk_id::parse_strict_json(text).map_err(sdk_error)?;
    let event: sdk_id::Event<sdk::DelegationPayload> =
        serde_json::from_value(value).map_err(|error| Error::InvalidInput(error.to_string()))?;
    ensure(
        event.protocol == AGENT_DELEGATION_PROTOCOL,
        Error::UnsupportedProtocol,
    )?;
    sdk_id::validate_event_fields(&event, &[]).map_err(sdk_error)?;
    // Typed re-encoding drops unknown payload members and explicit nulls, so the
    // exact-bytes comparison also closes the payload object.
    ensure_valid(
        sdk_id::canonical_event_bytes(&event).map_err(sdk_error)? == bytes,
        "event is not canonical JCS",
    )?;
    // Validates the nonce range; equals event_hash(bytes) for canonical bytes.
    let hash = sdk_id::event_hash_bytes(&event).map_err(sdk_error)?;
    let created_at = u64::try_from(event.created_at).map_err(|_| invalid("created_at"))?;
    let actor = event.actor.public_key_bytes().map_err(sdk_error)?;
    let (principal_id, id, grant) = match (event.kind.as_str(), &event.payload) {
        (sdk::DELEGATION_GRANT, sdk::DelegationPayload::Grant(payload)) => {
            sdk::validate_delegation_grant_payload(payload, Some(event.created_at))
                .map_err(sdk_error)?;
            let constraints_bytes = match &payload.constraints {
                Some(constraints) => serde_jcs::to_vec(constraints)
                    .map_err(|error| Error::InvalidInput(error.to_string()))?
                    .len(),
                None => 0,
            };
            let time = |value: Option<i64>| {
                value
                    .map(|t| u64::try_from(t).map_err(|_| invalid("timestamp")))
                    .transpose()
            };
            (
                payload.principal_id.clone(),
                payload.id.clone(),
                Some(DelegationGrant {
                    scopes: payload.scopes.clone(),
                    audiences: payload.audiences.clone(),
                    not_before: time(payload.not_before)?,
                    expires_at: time(payload.expires_at)?,
                    constraints_bytes,
                }),
            )
        }
        (sdk::DELEGATION_REVOKE, sdk::DelegationPayload::Revoke(payload)) => {
            sdk::validate_delegation_revoke_payload(payload).map_err(sdk_error)?;
            (payload.principal_id.clone(), payload.id.clone(), None)
        }
        (sdk::DELEGATION_GRANT | sdk::DELEGATION_REVOKE, _) => {
            return Err(invalid("event type does not match its payload"))
        }
        _ => return Err(Error::UnsupportedProtocol),
    };
    Ok(DelegationEvent {
        hash: Hash::new(hash),
        actor: Hash::new(actor),
        created_at,
        nonce: event.nonce,
        principal_id,
        id,
        grant,
    })
}

/// dMsg hosted-signing policy for one parsed event.
///
/// The controller must be current and be the actor; the payload must name this
/// principal and an ID under the account prefix; `created_at` must be within
/// `[now - 60 s, now + 5 s]` and not before `valid_from`. A grant must stay
/// within a restricted ceiling, carry `expires_at` at most 366 days after
/// `created_at`, and keep `constraints` within 4 KiB. Revocation lineage and
/// credential state are checked by the delegation service at acceptance.
///
/// # Errors
/// A retired or foreign controller returns `Error::Forbidden`, mismatched
/// identities `Error::IntegrityFailed`, a stale or future time `Error::Expired`,
/// and a policy violation `Error::InvalidInput`.
pub fn check_hosted_event(
    event: &DelegationEvent,
    account_id: &AccountId,
    principal_id: &str,
    controller: &HostedController,
    now: u64,
) -> Result<()> {
    ensure(controller.retired_at.is_none(), Error::Forbidden)?;
    ensure(
        event.actor == controller.public_key && event.principal_id == principal_id,
        Error::IntegrityFailed,
    )?;
    ensure_valid(
        event
            .id
            .strip_prefix(&delegation_id_prefix(account_id))
            .is_some_and(|suffix| !suffix.is_empty()),
        "delegation id must start with the account prefix",
    )?;
    ensure(
        event.created_at >= controller.valid_from
            && event.created_at.saturating_add(EVENT_PAST_SKEW) >= now
            && event.created_at <= now.saturating_add(EVENT_FUTURE_SKEW),
        Error::Expired,
    )?;
    if let Some(grant) = &event.grant {
        if let DelegationAuthority::Restricted { scopes, audiences } = &controller.delegation {
            ensure(
                grant.scopes.iter().all(|s| scopes.contains(s))
                    && grant.audiences.iter().all(|a| audiences.contains(a)),
                Error::Forbidden,
            )?;
        }
        ensure_valid(
            grant.expires_at.is_some_and(|expires_at| {
                expires_at > event.created_at && expires_at - event.created_at <= MAX_GRANT_LIFETIME
            }),
            "grant expires_at is required within 366 days",
        )?;
        ensure_valid(
            grant.constraints_bytes <= MAX_GRANT_CONSTRAINTS_BYTES,
            "grant constraints are too large",
        )?;
    }
    Ok(())
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

/// Validate a directory configuration without accessing any canister.
///
/// Requires a valid issuer namespace, 1..16 distinct authenticated user homes,
/// HTTPS origins for principals and controller source, an HTTPS query URL, an
/// HTTPS profile prefix ending in `/`, and 0..8 domain names. Origins are at
/// most 512 bytes and the query URL/profile prefix at most 2 KiB, without JSON
/// escape characters, matching the shared principal-document byte budget.
///
/// # Errors
/// Invalid configuration returns `Error::InvalidInput` or `Error::AuthRequired`.
pub fn validate_directory_init(config: &DirectoryInit) -> Result<()> {
    validate_namespace(&config.issuer_namespace)?;
    ensure_valid(
        (1..=16).contains(&config.user_homes.len())
            && config.custom_domains.len() <= 8
            && config.custom_domains.iter().all(|d| {
                !d.is_empty()
                    && d.len() <= 253
                    && d.bytes().all(|b| {
                        b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-'
                    })
            }),
        "directory homes/domains",
    )?;
    for (index, home) in config.user_homes.iter().enumerate() {
        authenticated(*home)?;
        ensure_valid(!config.user_homes[..index].contains(home), "duplicate home")?;
    }
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

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
