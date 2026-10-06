//! Agent Delegation 1.0 principal hosting: hosted controller keys, the principal
//! state published by the directory, and exact delegation-event signing.
//!
//! A dMsg account may enable an Agent Delegation principal at
//! `principal_origin/<account_id>`. Its controllers are threshold Ed25519 keys
//! derived by the COSE executor; every signature still needs device approval.
//! The user home is authoritative for controller changes and signing; the
//! directory only publishes the resulting principal document. Delegation
//! credentials are held by the delegation service, not by these canisters.
//! Times are Unix milliseconds.
use crate::{cose::*, *};
use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Agent Delegation protocol identifier used in events and documents.
pub const AGENT_DELEGATION_PROTOCOL: &str = "agent-delegation/1.0";
/// Maximum current (unretired) controllers per principal (8).
pub const MAX_CURRENT_CONTROLLERS: usize = 8;
/// Maximum controller records, current and retired, per principal (32).
/// Retired records are never deleted, so this is also the hosted-key generation limit.
pub const MAX_CONTROLLER_RECORDS: usize = 32;
/// Maximum exact event text accepted for hosted signing (16 KiB).
pub const MAX_AGENT_EVENT_BYTES: usize = 16_384;
/// Maximum serialized `constraints` of a hosted grant (4 KiB).
pub const MAX_GRANT_CONSTRAINTS_BYTES: usize = 4_096;
/// Maximum grant lifetime a hosted controller signs: `expires_at - created_at` (366 days).
pub const MAX_GRANT_LIFETIME: u64 = 366 * DAY;

/// Display type of a principal; it grants no authority.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum PrincipalType {
    /// A natural person.
    Person,
    /// A company or other legal organization.
    Organization,
    /// A team inside or across organizations.
    Team,
    /// A project.
    Project,
    /// Any other non-agent subject.
    Other,
}

/// Delegation ceiling of a controller, immutable once published.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum DelegationAuthority {
    /// `"*"`: any explicit scopes and audiences, and management of every credential
    /// of this principal. Always an explicit owner choice.
    Unrestricted,
    /// Only the listed exact scopes and relying-party audiences, and only credentials
    /// owned within this key's lineage. Both lists are nonempty.
    Restricted {
        /// Exact scope strings; no wildcard, prefix or hierarchy.
        scopes: Vec<String>,
        /// Relying-party origins or Agent IDs, matched exactly.
        audiences: Vec<String>,
    },
}

/// Hosted controller record. Its key is the COSE `AgentController` key of this
/// generation; key, `valid_from`, `delegation` and `supersedes` never change.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct HostedController {
    /// Positive hosted-key generation, allocated once and never reused.
    pub generation: u32,
    /// Raw Ed25519 public key of this generation; the Agent ID encodes it.
    pub public_key: Hash,
    /// Optional display label; the only field that may change.
    pub name: Option<String>,
    /// Binding start (inclusive): the user-home commit time.
    pub valid_from: u64,
    /// Delegation ceiling of this key.
    pub delegation: DelegationAuthority,
    /// Earlier generations of this principal whose credentials this key may manage.
    pub supersedes: Vec<u32>,
    /// Binding end (exclusive) once retired; None while current.
    pub retired_at: Option<u64>,
    /// Earliest untrusted signature time after compromise; requires retirement.
    pub invalid_from: Option<u64>,
}

/// Account principal state published by the directory.
/// `version` and `updated_at` both increase strictly on every change.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PrincipalState {
    /// Display type of the principal.
    pub principal_type: PrincipalType,
    /// Current and retired controllers in generation order.
    pub controllers: Vec<HostedController>,
    /// Publication version; the directory rejects rollback.
    pub version: u64,
    /// Document `updated_at`: `max(now, previous + 1)` on every change.
    pub updated_at: u64,
}

/// Owner view of a principal, including publication and replay state.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PrincipalInfo {
    /// Canonical principal URL `principal_origin/<account_id>`.
    pub principal_id: String,
    /// Current authoritative state in the user home.
    pub state: PrincipalState,
    /// Highest version the directory confirmed; the change is live only when it equals `state.version`.
    pub published_version: u64,
    /// Highest signed Agent Identity nonce per hosted generation.
    pub last_nonces: BTreeMap<u32, u64>,
}

/// Hosted-signing request for one exact Agent Delegation event.
/// `event` is the exact JCS text whose SHA3-256 is signed.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentEventSignRequest {
    /// Account whose principal and hosted key sign the event.
    pub account_id: AccountId,
    /// Current hosted-key generation; must equal the event actor.
    pub generation: u32,
    /// Exact JCS event text: the six Agent Identity fields only.
    pub event: String,
    /// Checked browser origin bound by the device approval.
    pub origin: String,
    /// Approved maximum execution cycles.
    pub max_cycles: u128,
    /// Device approval over the derived execution kind and cycle limit.
    pub approval: Approval,
}

impl AgentEventSignRequest {
    /// Convert to the low-level execution contract without validating or authorizing it.
    /// `principal_id` is the account's canonical principal URL.
    pub fn into_execution(self, principal_id: String) -> ExecuteRequest {
        ExecuteRequest {
            account_id: self.account_id,
            max_cycles: self.max_cycles,
            approval: self.approval,
            kind: ExecutionKind::AgentEvent {
                key: KeySelector::AgentController {
                    generation: self.generation,
                }
                .into(),
                event: self.event.into_bytes().into(),
                principal_id,
                origin: self.origin,
            },
        }
    }
}

/// Directory deployment configuration. The principal origin and document
/// fields are permanent; governance may append user homes and replace the
/// custom domains.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct DirectoryInit {
    /// Deployment domain; also part of each home's account-ID allocator fingerprint.
    pub environment: Environment,
    /// Account issuer namespace shared by the user homes.
    pub issuer_namespace: String,
    /// User homes allowed to publish accounts whose IDs they allocated;
    /// each account ID carries its home's allocator fingerprint. Append-only.
    pub user_homes: Vec<Principal>,
    /// HTTPS origin of principal IDs, such as `https://id.dmsg.net`.
    pub principal_origin: String,
    /// Controller `source` origin, such as `https://dmsg.net`.
    pub controller_source: String,
    /// Authoritative delegation query endpoint written into every document.
    pub delegation_query_url: String,
    /// Prefix of the public profile link, such as `https://dmsg.net/u/`.
    pub profile_url_prefix: String,
    /// Custom domains served at `/.well-known/ic-domains`.
    pub custom_domains: Vec<String>,
    /// Fixed SNS governance caller allowed, besides controllers, to run administrative operations.
    pub governance: Principal,
}

/// Directory publication summary for one account.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Publication {
    /// Canonical principal URL of the account.
    pub principal_id: String,
    /// User home that published the account.
    pub home_user: Principal,
    /// Published principal-state version.
    pub version: u64,
    /// Published document `updated_at`.
    pub updated_at: u64,
    /// SHA-256 of the exact published JSON document bytes.
    pub document_digest: Hash,
}
