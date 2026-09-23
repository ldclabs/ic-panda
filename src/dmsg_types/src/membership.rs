//! `membership/1`: product-neutral PANDA qualification and durable benefit decisions.
//! All business timestamps are UTC Unix milliseconds; intervals are [start, end).
//! Constructing or decoding these records performs no validation or authorization.
use crate::{Environment, Hash};
use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

/// Product, authority and opaque subject binding; decoding does not establish control.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Beneficiary {
    /// Product identifier within this protocol.
    pub product_id: String,
    /// Canister authoritative for the beneficiary subject.
    pub authority_canister: Principal,
    /// Registered schema identifying how to interpret subject_bytes.
    pub subject_schema: String,
    /// Opaque subject bytes in the registered schema.
    pub subject_bytes: ByteBuf,
}

/// Shared qualification service deployment and admission limits.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct MembershipInit {
    /// Deployment separation.
    pub environment: Environment,
    /// PANDA SNS governance and this service's governance authority.
    pub governance: Principal,
    /// Pinned SNS root.
    pub sns_root: Principal,
    /// Pinned eight-decimal PANDA ledger.
    pub panda_ledger: Principal,
    /// Reviewed SNS governance module hash. Required outside Local fixtures.
    pub expected_governance_module_hash: Option<Hash>,
}

/// Qualification observation; unverifiable is distinct from known ineligibility.
#[derive(CandidType, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Eligibility {
    /// Known to satisfy the fixed qualification policy.
    Eligible,
    /// Known not to satisfy the policy.
    Ineligible,
    /// Current supporting facts cannot be verified.
    Unverifiable,
}
