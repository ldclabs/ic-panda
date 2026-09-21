//! Deterministic product-neutral membership identifiers and threshold arithmetic.
#![allow(missing_docs)]
use crate::{authenticated, digest};
use dmsg_types::{membership::*, *};
use num_bigint::BigUint;
use num_traits::ToPrimitive;

pub const MAX_LEASE_MS: u64 = 60 * MINUTE;
pub const MIN_COOLING_MS: u64 = MAX_LEASE_MS + 5 * MINUTE;

pub fn mul_div(a: u128, b: u128, denominator: u128, round_up: bool) -> Result<u128> {
    ensure(denominator > 0, invalid("zero denominator"))?;
    let n = BigUint::from(a) * BigUint::from(b);
    let d = BigUint::from(denominator);
    let value = if round_up { (n + &d - 1u8) / d } else { n / d };
    value.to_u128().ok_or(Error::QuotaExceeded)
}

pub fn required_panda(threshold: &Threshold, decimals: u8) -> Result<u128> {
    ensure(decimals == 8, Error::UnsupportedProtocol)?;
    match threshold {
        Threshold::FixedPanda { atomic } => {
            ensure(*atomic > 0, invalid("zero threshold"))?;
            Ok(*atomic)
        }
        Threshold::AnnualPrice {
            price_cents,
            r_num,
            r_den,
        } => {
            ensure(
                *price_cents > 0 && *r_num > 0 && *r_den > 0,
                invalid("price / R"),
            )?;
            let n =
                BigUint::from(*price_cents) * BigUint::from(*r_num) * BigUint::from(100_000_000u64);
            let d = BigUint::from(*r_den) * BigUint::from(100u8);
            ((n + &d - 1u8) / d).to_u128().ok_or(Error::QuotaExceeded)
        }
    }
}

pub fn validate_beneficiary(b: &Beneficiary) -> Result<()> {
    authenticated(b.authority_canister)?;
    ensure(
        !b.product_id.is_empty()
            && b.product_id.len() <= 32
            && !b.subject_schema.is_empty()
            && b.subject_schema.len() <= 64
            && !b.subject_bytes.is_empty()
            && b.subject_bytes.len() <= 64,
        invalid("beneficiary"),
    )
}

pub fn membership_intent_digest(intent: &MembershipIntent) -> Hash {
    digest("dmsg/commerce/intent/v1", intent)
}

pub fn claim_action_digest(request: &ClaimRequest) -> Hash {
    digest(
        "membership/claim-action/v1",
        &(
            request.neuron_id,
            request.policy_version,
            request.benefit_id,
            request.expected_business_revision,
            &request.term,
            &request.change,
        ),
    )
}

pub fn claim_id(service: candid::Principal, intent: &MembershipIntent) -> Hash {
    digest(
        "membership/claim-id/v1",
        &(service, intent.actor, intent.application_id),
    )
}

pub fn claim_key(id: Hash) -> Hash {
    digest("membership/claim-key/v1", &id)
}

pub fn neuron_key(governance: candid::Principal, neuron: Hash) -> Hash {
    digest("membership/neuron-key/v1", &(governance, neuron))
}

pub fn decision_digest(decision: &MembershipDecision) -> Hash {
    digest("membership/decision/v1", decision)
}

/// Single-segment path for an immutable governance policy.
pub fn policy_key(version: u64) -> Hash {
    digest("membership/policy-key/v1", &version)
}
