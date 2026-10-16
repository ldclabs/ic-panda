//! `dmsg-wallet/1` commitments, member approvals, policy evaluation,
//! address encodings and cosigner verification. Both the wallet canister and
//! clients use these functions, so a proposal digest, an address or a policy
//! decision computed by either side is the same.
use crate::{digest, expiry};
use candid::Principal;
use dmsg_types::{
    integration::{ApplicationApproval, ApprovalPurpose, AUTH_TTL_MS, INTEGRATION_VERSION},
    membership::Beneficiary,
    wallet::*,
    *,
};

pub mod address;
pub mod cosign;
pub mod policy;

pub use address::*;
pub use cosign::*;
pub use policy::*;

/// Domain of [`proposal_digest`].
pub const PROPOSAL_DOMAIN: &str = "dmsg-wallet/proposal/v1";
/// Domain of [`decision_digest`].
pub const DECISION_DOMAIN: &str = "dmsg-wallet/decision/v1";
/// Domain of [`join_digest`].
pub const JOIN_DOMAIN: &str = "dmsg-wallet/join/v1";
/// Domain of [`activation_digest`].
pub const ACTIVATION_DOMAIN: &str = "dmsg-wallet/activation/v1";
/// Domain of [`cosign_message`].
pub const COSIGN_DOMAIN: &str = "dmsg-wallet/cosign/v1";
/// Domain of [`audit_hash`].
pub const AUDIT_DOMAIN: &str = "dmsg-wallet/audit/v1";
/// Domain of the activation deposit subaccount.
pub const DEPOSIT_DOMAIN: &str = "dmsg-wallet/deposit/v1";

/// The commitment members approve. `op_id` makes it unique within the wallet;
/// the wallet-local proposal number is not part of it.
pub fn proposal_digest(
    environment: &Environment,
    wallet: Principal,
    op_id: &OpId,
    intent: &Intent,
    policy_version: u64,
    proposer: &MemberId,
    via_agent: &Option<String>,
) -> Hash {
    digest(
        PROPOSAL_DOMAIN,
        &(
            environment,
            wallet,
            op_id,
            intent,
            policy_version,
            proposer,
            via_agent,
        ),
    )
}

/// The `action_digest` of a member's decision on a proposal.
pub fn decision_digest(proposal: &Hash, decision: Decision) -> Hash {
    digest(DECISION_DOMAIN, &(proposal, decision))
}

/// The `action_digest` of an account member joining and binding `actor`.
pub fn join_digest(wallet: Principal, account: &AccountId, actor: Principal) -> Hash {
    digest(JOIN_DOMAIN, &(wallet, account, actor))
}

/// The `action_digest` of creating the wallet `spec` under `quote`.
pub fn activation_digest(quote: &ActivationQuote, spec: &WalletSpec) -> Hash {
    digest(ACTIVATION_DOMAIN, &(quote, spec))
}

/// The 32 bytes a cosigner signs for digest `index` of a plan. The wallet,
/// proposal and plan bind it, so a signature never serves another transaction.
pub fn cosign_message(
    wallet: Principal,
    proposal: u64,
    plan: u32,
    index: u32,
    message: &Hash,
) -> Hash {
    digest(COSIGN_DOMAIN, &(wallet, proposal, plan, index, message))
}

/// Hash of an audit event chained to `prev`.
pub fn audit_hash(
    prev: &Hash,
    seq: u64,
    at_ms: u64,
    actor: &Option<MemberId>,
    proposal: Option<u64>,
    kind: &AuditKind,
) -> Hash {
    digest(AUDIT_DOMAIN, &(prev, seq, at_ms, actor, proposal, kind))
}

/// Recompute an audit chain from `prev`; returns the last hash.
///
/// # Errors
/// Returns `Error::IntegrityFailed` at the first event whose sequence,
/// previous hash or hash does not match.
pub fn verify_audit(mut prev: Hash, mut seq: u64, events: &[AuditEvent]) -> Result<Hash> {
    for e in events {
        let hash = audit_hash(&prev, e.seq, e.at_ms, &e.actor, e.proposal, &e.kind);
        ensure(
            e.seq == seq && e.prev == prev && e.hash == hash,
            Error::IntegrityFailed,
        )?;
        prev = hash;
        seq += 1;
    }
    Ok(prev)
}

/// The subaccount an activation quote is paid into.
pub fn deposit_subaccount(hub: Principal, quote_id: &Hash) -> Hash {
    digest(DEPOSIT_DOMAIN, &(hub, quote_id))
}

/// Beneficiary of wallet approvals: the wallet for its decisions and joins,
/// the hub for creating a wallet. The hub is the authority of both.
pub fn wallet_beneficiary(hub: Principal, subject: Principal) -> Beneficiary {
    Beneficiary {
        product_id: WALLET_PRODUCT_ID.into(),
        authority_canister: hub,
        subject_schema: WALLET_SUBJECT_SCHEMA.into(),
        subject_bytes: subject.as_slice().to_vec().into(),
    }
}

/// The application approval fields a wallet or the hub requires; the user
/// home separately verifies the registration, the device and the deadline.
#[derive(Clone, Debug)]
pub struct ExpectedApproval<'a> {
    /// Deployment domain.
    pub environment: &'a Environment,
    /// Hub, the beneficiary authority.
    pub hub: Principal,
    /// Consuming service: the wallet, or the hub for `WalletCreate`.
    pub service: Principal,
    /// Purpose.
    pub purpose: ApprovalPurpose,
    /// Exact commitment.
    pub action_digest: Hash,
    /// Approving member account.
    pub account: &'a AccountId,
    /// Calling Principal.
    pub actor: Principal,
}

/// Check an application approval against what the consumer expects.
///
/// # Errors
/// Returns `Error::UnsupportedProtocol` for another version,
/// `Error::Forbidden` for any other field and `Error::Expired` past or too
/// far beyond its deadline.
pub fn check_wallet_approval(
    approval: &ApplicationApproval,
    expected: &ExpectedApproval<'_>,
    now: u64,
) -> Result<()> {
    ensure(
        approval.version == INTEGRATION_VERSION,
        Error::UnsupportedProtocol,
    )?;
    ensure(
        approval.environment == *expected.environment
            && approval.app_id == WALLET_APP_ID
            && approval.service == expected.service
            && approval.beneficiary == wallet_beneficiary(expected.hub, expected.service)
            && approval.purpose == expected.purpose
            && approval.action_digest == expected.action_digest
            && approval.approving_account == *expected.account
            && approval.actor == expected.actor,
        Error::Forbidden,
    )?;
    expiry(now, approval.expires_at_ms, AUTH_TTL_MS)
}

#[cfg(test)]
mod tests;
