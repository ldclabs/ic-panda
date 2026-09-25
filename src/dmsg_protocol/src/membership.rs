//! Exact threshold arithmetic shared by membership and product contracts.
use dmsg_types::*;
use num_bigint::BigUint;
use num_traits::ToPrimitive;

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
}
