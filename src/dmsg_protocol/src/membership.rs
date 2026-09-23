//! Deterministic product-neutral membership identifiers and threshold arithmetic.
use crate::{authenticated, ensure_valid};
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
