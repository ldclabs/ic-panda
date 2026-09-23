//! Deterministic product-neutral membership identifiers and threshold arithmetic.
use crate::{authenticated, digest, ensure_valid};
use dmsg_types::{membership::*, *};
use num_bigint::BigUint;
use num_traits::ToPrimitive;

/// Maximum resource qualification lease duration in milliseconds (one hour).
pub const MAX_LEASE_MS: u64 = 60 * MINUTE;
/// Minimum new-claim cooling duration in milliseconds, including call allowance.
pub const MIN_COOLING_MS: u64 = MAX_LEASE_MS + 5 * MINUTE;

/// Compute a*b/denominator exactly, rounding up when requested, otherwise down.
/// Intermediate overflow uses big integers; an unrepresentable result returns
/// QuotaExceeded and a zero denominator returns InvalidInput.
pub fn mul_div(a: u128, b: u128, denominator: u128, round_up: bool) -> Result<u128> {
    ensure_valid(denominator > 0, "zero denominator")?;
    if let Some(n) = a.checked_mul(b) {
        // A nonzero remainder implies denominator >= 2, so incrementing the
        // quotient cannot overflow. Avoid n + denominator - 1 at the boundary.
        return Ok(n / denominator + u128::from(round_up && n % denominator != 0));
    }
    let n = BigUint::from(a) * BigUint::from(b);
    let d = BigUint::from(denominator);
    let value = if round_up { (n + &d - 1u8) / d } else { n / d };
    value.to_u128().ok_or(Error::QuotaExceeded)
}

/// Required PANDA atomic stake, rounded up. Only the fixed 8-decimal asset is
/// supported; threshold/price/R must be positive. Does not verify SNS ownership.
pub fn required_panda(threshold: &Threshold, decimals: u8) -> Result<u128> {
    ensure(decimals == 8, Error::UnsupportedProtocol)?;
    match threshold {
        Threshold::FixedPanda { atomic } => {
            ensure_valid(*atomic > 0, "zero threshold")?;
            Ok(*atomic)
        }
        Threshold::AnnualPrice {
            price_cents,
            r_num,
            r_den,
        } => {
            ensure_valid(*price_cents > 0 && *r_num > 0 && *r_den > 0, "price / R")?;
            let n =
                BigUint::from(*price_cents) * BigUint::from(*r_num) * BigUint::from(100_000_000u64);
            let d = BigUint::from(*r_den) * BigUint::from(100u8);
            ((n + &d - 1u8) / d).to_u128().ok_or(Error::QuotaExceeded)
        }
    }
}

/// Check authority shape and nonempty bounded product/schema/subject fields.
/// Does not authenticate that authority or interpret the product's subject schema.
pub fn validate_beneficiary(b: &Beneficiary) -> Result<()> {
    authenticated(b.authority_canister)?;
    ensure_valid(
        !b.product_id.is_empty()
            && b.product_id.len() <= 32
            && !b.subject_schema.is_empty()
            && b.subject_schema.len() <= 64
            && !b.subject_bytes.is_empty()
            && b.subject_bytes.len() <= 64,
        "beneficiary",
    )
}

/// Commit to the complete commercial intent under dmsg/commerce/intent/v1.
pub fn membership_intent_digest(intent: &MembershipIntent) -> Hash {
    digest("dmsg/commerce/intent/v1", intent)
}

/// Commit to claim terms and change action, excluding the enclosing authorization.
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

/// Derive an idempotent claim ID from service, actor and application ID.
/// Different terms under this ID must be rejected by the stateful consumer.
pub fn claim_id(service: candid::Principal, intent: &MembershipIntent) -> Hash {
    digest(
        "membership/claim-id/v1",
        &(service, intent.actor, intent.application_id),
    )
}

/// Single-segment certified path for a qualification claim.
pub fn claim_key(id: Hash) -> Hash {
    digest("membership/claim-key/v1", &id)
}

/// Product-independent neuron identity for the global exclusive-use index.
pub fn neuron_key(governance: candid::Principal, neuron: Hash) -> Hash {
    digest("membership/neuron-key/v1", &(governance, neuron))
}

/// Commit to a complete immutable membership decision; does not authorize it.
pub fn decision_digest(decision: &MembershipDecision) -> Hash {
    digest("membership/decision/v1", decision)
}

/// Single-segment path for an immutable governance policy.
pub fn policy_key(version: u64) -> Hash {
    digest("membership/policy-key/v1", &version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_fast_path_and_wide_intermediates_have_identical_rounding() {
        let values = [0, 1, 3, 10_000, 1_000_001, u64::MAX as u128, u128::MAX];
        for a in values {
            for b in values {
                for denominator in values {
                    for up in [false, true] {
                        if denominator == 0 {
                            assert!(matches!(
                                mul_div(a, b, denominator, up),
                                Err(Error::InvalidInput(_))
                            ));
                            continue;
                        }
                        let n = BigUint::from(a) * BigUint::from(b);
                        let d = BigUint::from(denominator);
                        let mut expected = &n / &d;
                        if up && &n % &d != BigUint::from(0u8) {
                            expected += 1u8;
                        }
                        assert_eq!(
                            mul_div(a, b, denominator, up),
                            expected.to_u128().ok_or(Error::QuotaExceeded)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn thresholds_require_positive_terms_and_the_pinned_asset() {
        assert_eq!(
            required_panda(&Threshold::FixedPanda { atomic: 1 }, 8),
            Ok(1)
        );
        assert_eq!(
            required_panda(&Threshold::FixedPanda { atomic: 1 }, 6),
            Err(Error::UnsupportedProtocol)
        );
        assert!(required_panda(&Threshold::FixedPanda { atomic: 0 }, 8).is_err());
        for (price_cents, r_num, r_den) in [(0, 1, 1), (1, 0, 1), (1, 1, 0)] {
            assert!(matches!(
                required_panda(
                    &Threshold::AnnualPrice {
                        price_cents,
                        r_num,
                        r_den
                    },
                    8
                ),
                Err(Error::InvalidInput(_))
            ));
        }
        assert_eq!(
            required_panda(
                &Threshold::AnnualPrice {
                    price_cents: 1,
                    r_num: 1,
                    r_den: 3
                },
                8
            ),
            Ok(333_334)
        );
        assert_eq!(
            required_panda(
                &Threshold::AnnualPrice {
                    price_cents: u64::MAX,
                    r_num: u128::MAX,
                    r_den: 1
                },
                8
            ),
            Err(Error::QuotaExceeded)
        );
    }

    #[test]
    fn beneficiary_fields_are_bounded_without_implying_authorization() {
        let valid = Beneficiary {
            product_id: "p".repeat(32),
            authority_canister: candid::Principal::from_slice(&[1]),
            subject_schema: "s".repeat(64),
            subject_bytes: vec![1; 64].into(),
        };
        assert_eq!(validate_beneficiary(&valid), Ok(()));
        let changes: &[fn(&mut Beneficiary)] = &[
            |b| b.product_id.clear(),
            |b| b.product_id.push('p'),
            |b| b.subject_schema.clear(),
            |b| b.subject_schema.push('s'),
            |b| b.subject_bytes.clear(),
            |b| b.subject_bytes.push(1),
            |b| b.authority_canister = candid::Principal::anonymous(),
            |b| b.authority_canister = candid::Principal::management_canister(),
        ];
        for change in changes {
            let mut b = valid.clone();
            change(&mut b);
            assert!(validate_beneficiary(&b).is_err());
        }
    }
}
