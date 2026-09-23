//! Closed application-action profile. Product authority must attest to this exact input.
use crate::{AccountId, Environment, Hash};
use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};

/// Full application decision, separate from portable document statements.
/// All timestamps are Unix milliseconds. No field is a caller-supplied display summary.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AppAction {
    /// Profile version, exactly one.
    pub version: u16,
    /// Deployment domain.
    pub environment: Environment,
    /// Governance registered application.
    pub app_id: String,
    /// Exact registration revision used for preparation.
    pub app_config_version: u64,
    /// Exact browser origin; unlike document profiles, this is in signed content.
    pub origin: String,
    /// Canister that commits the product action.
    pub receiver: Principal,
    /// Product's stable actor Xid, not the dMsg signing account.
    pub actor_id: AccountId,
    /// Exact dMsg account selected by the product preparation, distinct from actor_id.
    pub signing_account: AccountId,
    /// Product request identity; not the dMsg execution identity.
    pub operation_id: Hash,
    /// Commitment to the complete product intent, including its session signer.
    pub intent_hash: Hash,
    /// Exact complete command commitment in the product's canonical encoding.
    pub input_hash: Hash,
    /// Product object content commitment.
    pub subject_hash: Hash,
    /// Product's object/version precondition.
    pub precondition_hash: Hash,
    /// Roles accepted at preparation.
    pub role_snapshot_hash: Hash,
    /// Explicit signer trust policy.
    pub signing_policy_hash: Hash,
    /// Product's rule set at preparation.
    pub rule_set_hash: Hash,
    /// First valid time.
    pub issued_at_ms: u64,
    /// Exclusive end of the intent's validity window, at most five minutes.
    pub expires_at_ms: u64,
    /// Typed product command; unknown commands must be rejected.
    pub command: AppActionCommand,
    /// Immutable file/version references, ordered by file_id.
    pub files: Vec<ActionFile>,
}

/// Initial closed schema. Additional products require an explicitly versioned variant.
/// These are complete TokenList command projections, not commands for dMsg to execute.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub enum AppActionCommand {
    /// Certify one disclosure draft revision.
    TokenListCertifyDisclosure {
        /// Registry project ID.
        project_id: u64,
        /// Investment contract ID.
        contract_id: u64,
        /// Expected draft revision.
        revision: u64,
    },
    /// Decide one review round with the complete requested changes.
    TokenListDecideReview {
        /// Registry project ID.
        project_id: u64,
        /// Review case ID.
        case_id: u64,
        /// Round being decided.
        round: u64,
        /// Explicit decision.
        outcome: ActionReviewOutcome,
        /// Complete requested changes, preserving order.
        changes: Vec<ActionRequestedChange>,
        /// Verbatim decision rationale.
        rationale: String,
    },
    /// Certify a transition. Optional analysis is a file reference, not proof of review.
    TokenListCertifyTransition {
        /// Registry project ID.
        project_id: u64,
        /// Transition ID.
        transition_id: u64,
        /// Commitment to the officer's certification.
        statement_hash: Hash,
        /// Verbatim rationale.
        rationale: String,
        /// Exact optional analysis reference from the command.
        analysis: Option<ActionArtifact>,
    },
    /// Counsel approval or refusal of one transition.
    TokenListApproveTransition {
        /// Registry project ID.
        project_id: u64,
        /// Transition ID.
        transition_id: u64,
        /// True approves, false refuses.
        approve: bool,
        /// Commitment to the counsel statement.
        statement_hash: Hash,
        /// Verbatim rationale.
        rationale: String,
    },
}

/// Only terminal review decisions are signable; workflow states are not decisions.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ActionReviewOutcome {
    /// Approve this round.
    Approved,
    /// Refuse this round.
    Rejected,
    /// Return specific changes.
    ChangesRequested,
}

/// One full change request shown to the signer.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActionRequestedChange {
    /// Product field locator.
    pub locator: String,
    /// Verbatim requested change.
    pub detail: String,
    /// Whether the change blocks approval.
    pub blocking: bool,
}

/// Exact external artifact reference; no automatic URI fetching.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActionArtifact {
    /// HTTPS or IPFS reference.
    pub uri: String,
    /// Digest of exact bytes, not encoded digest text.
    pub sha256: Hash,
    /// Declared content type.
    pub content_type: String,
    /// Exact size in bytes.
    pub size: u64,
}

/// Exact product file version presented for this action.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ActionFile {
    /// Stable, product-scoped file locator.
    pub file_id: String,
    /// Immutable revision; positive.
    pub revision: u64,
    /// Exact SHA-256 bytes.
    pub sha256: Hash,
    /// Exact byte length.
    pub byte_length: u64,
    /// Media type, not inferred from the file name.
    pub media_type: String,
    /// Optional display name committed alongside the digest.
    pub display_name: Option<String>,
    /// Whether bytes are plaintext originals or ciphertext.
    pub representation: ActionFileRepresentation,
}

/// File digest interpretation. Neither value proves the original was read.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ActionFileRepresentation {
    /// Hash names the original bytes.
    Original,
    /// Hash names encrypted bytes.
    Encrypted,
}
