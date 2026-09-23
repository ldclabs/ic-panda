//! dmsg-commerce/1 arithmetic, calendar boundaries and authenticated leaf keys.
use crate::{digest, ensure_valid, membership::mul_div};
use chrono::{Datelike, TimeZone, Utc};
use dmsg_types::{billing::*, membership::*, *};

/// Construct the dMsg product/schema binding without authenticating its home.
pub fn beneficiary(home: candid::Principal, account: &AccountId) -> Beneficiary {
    Beneficiary {
        product_id: "dmsg".into(),
        authority_canister: home,
        subject_schema: "dmsg-account-v1".into(),
        subject_bytes: account.to_vec().into(),
    }
}

/// Decode a dMsg beneficiary's 12-byte account ID. Does not check authority trust.
/// Other product/schema names return UnsupportedProtocol; wrong size is InvalidInput.
pub fn beneficiary_account(b: &Beneficiary) -> Result<AccountId> {
    ensure(
        b.product_id == "dmsg" && b.subject_schema == "dmsg-account-v1",
        Error::UnsupportedProtocol,
    )?;
    Ok(AccountId(
        b.subject_bytes
            .as_slice()
            .try_into()
            .map_err(|_| invalid("account id"))?,
    ))
}

/// Single-segment certified resource-entitlement path for the complete beneficiary.
pub fn entitlement_key(b: &Beneficiary) -> Hash {
    digest("dmsg/commerce/entitlement-key/v1", b)
}

/// Single-segment certified path for the current dMsg catalog.
pub fn catalog_key() -> Hash {
    digest("dmsg/commerce/catalog-key/v1", &"dmsg")
}

/// Commit to an immutable plan snapshot, including prices, limits and weights.
pub fn plan_digest(plan: &PlanVersion) -> Hash {
    digest("dmsg/commerce/plan/v1", plan)
}

/// Single-segment certified execution-usage path; month is UTC YYYYMM.
pub fn usage_key(account: &AccountId, month: u32) -> Hash {
    digest("dmsg/commerce/usage-key/v1", &(account, month))
}

fn datetime(ms: u64) -> Result<chrono::DateTime<Utc>> {
    let n = i64::try_from(ms).map_err(|_| invalid("timestamp"))?;
    chrono::DateTime::from_timestamp_millis(n).ok_or_else(|| invalid("timestamp"))
}

/// Same UTC instant one calendar year later, clamping February 29 to February 28.
/// Input/output are Unix milliseconds; invalid timestamps return InvalidInput.
pub fn next_year(ms: u64) -> Result<u64> {
    let t = datetime(ms)?;
    let year = t.year().checked_add(1).ok_or(Error::QuotaExceeded)?;
    let next = t
        .with_year(year)
        .or_else(|| t.with_day(28).and_then(|d| d.with_year(year)))
        .ok_or_else(|| invalid("calendar year"))?;
    u64::try_from(next.timestamp_millis()).map_err(|_| invalid("timestamp"))
}

/// Convert Unix milliseconds to UTC YYYYMM; rejects unsupported timestamps.
pub fn month_utc(ms: u64) -> Result<u32> {
    let t = datetime(ms)?;
    u32::try_from(t.year())
        .ok()
        .and_then(|v| v.checked_mul(100))
        .and_then(|v| v.checked_add(t.month()))
        .ok_or_else(|| invalid("UTC month"))
}

/// Half-open UTC month interval in Unix milliseconds for YYYYMM.
/// Invalid months or dates before the Unix epoch return InvalidInput.
pub fn month_bounds(month: u32) -> Result<(u64, u64)> {
    let year = i32::try_from(month / 100).map_err(|_| invalid("year"))?;
    let m = month % 100;
    let start = Utc
        .with_ymd_and_hms(year, m, 1, 0, 0, 0)
        .single()
        .ok_or_else(|| invalid("UTC month"))?;
    let end = if m == 12 {
        Utc.with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0)
    } else {
        Utc.with_ymd_and_hms(year, m + 1, 1, 0, 0, 0)
    }
    .single()
    .ok_or_else(|| invalid("UTC month"))?;
    Ok((
        u64::try_from(start.timestamp_millis()).map_err(|_| invalid("UTC month"))?,
        u64::try_from(end.timestamp_millis()).map_err(|_| invalid("UTC month"))?,
    ))
}

/// Sum exact weighted milliseconds, then round once for the entire month.
/// At most 64 contiguous segments must cover the month from account creation
/// (or month start for older accounts). Gaps/overlaps return IntegrityFailed.
pub fn monthly_allowance(month: u32, created_at_ms: u64, segments: &[MonthSegment]) -> Result<u64> {
    let (start, end) = month_bounds(month)?;
    ensure(segments.len() <= 64, Error::QuotaExceeded)?;
    let mut cursor = start.max(created_at_ms).min(end);
    let mut total = 0u128;
    for s in segments {
        ensure(
            s.start_ms == cursor && s.end_ms > s.start_ms && s.end_ms <= end,
            Error::IntegrityFailed,
        )?;
        total = total
            .checked_add(u128::from(s.end_ms - s.start_ms) * u128::from(s.monthly_units))
            .ok_or(Error::QuotaExceeded)?;
        cursor = s.end_ms;
    }
    ensure(cursor == end, Error::IntegrityFailed)?;
    u64::try_from(total / u128::from(end - start)).map_err(|_| Error::QuotaExceeded)
}

/// Convert integer USD cents to ledger atomic units, rounding up once.
/// The caller selects a trusted asset; decimals above 18 return InvalidInput.
pub fn cents_atomic(cents: u64, decimals: u8) -> Result<u128> {
    ensure_valid(decimals <= 18, "ledger decimals")?;
    mul_div(cents.into(), 10u128.pow(decimals.into()), 100, true)
}

/// Build the four initial plan snapshots at the supplied catalog version.
/// This neither approves deployment prices nor validates a governance version.
pub fn default_plans(version: u64) -> Vec<PlanVersion> {
    [
        (PlanId::Free, 0, 104_857_600, 2, 3),
        (PlanId::Plus, 1000, 1_073_741_824, 10, 10),
        (PlanId::Pro, 5000, 10_737_418_240, 50, 50),
        (PlanId::Max, 20000, 107_374_182_400, 200, 200),
    ]
    .into_iter()
    .map(
        |(plan_id, price_cents, storage_bytes, active_channels, monthly_execution_units)| {
            PlanVersion {
                plan_id,
                catalog_version: version,
                price_cents,
                limits: ResourceLimits {
                    storage_bytes,
                    active_channels,
                    monthly_execution_units,
                },
                weights: ExecutionWeights {
                    version: 1,
                    ed25519: 1,
                    ecdsa_secp256k1: 1,
                },
                terms_version: 1,
            }
        },
    )
    .collect()
}

/// Platform fee is added to, never subtracted from, the recipient's net amount.
/// Returns max(ceil(net*rate_bps/10000), minimum_atomic). Version must be positive
/// and rate <= 10000; policy timing and governance approval belong to the caller.
pub fn delivery_service_fee(
    net: u128,
    policy: &dmsg_types::payment::DeliveryFeePolicy,
) -> Result<u128> {
    ensure_valid(
        policy.version > 0 && policy.rate_bps <= 10_000,
        "fee policy",
    )?;
    Ok(mul_div(net, u128::from(policy.rate_bps), 10_000, true)?.max(policy.minimum_atomic))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_and_rounding() {
        assert_eq!(next_year(1_709_208_000_123).unwrap(), 1_740_744_000_123); // leap day noon
        let (a, b) = month_bounds(202609).unwrap();
        assert_eq!(
            monthly_allowance(
                202609,
                a,
                &[
                    MonthSegment {
                        start_ms: a,
                        end_ms: a + (b - a) / 2,
                        monthly_units: 3,
                        source_contract_id: None
                    },
                    MonthSegment {
                        start_ms: a + (b - a) / 2,
                        end_ms: b,
                        monthly_units: 10,
                        source_contract_id: None
                    }
                ]
            )
            .unwrap(),
            6
        );
        assert_eq!(cents_atomic(2, 6).unwrap(), 20_000);
        assert_eq!(
            crate::integration::required_panda_stake(10_000_000, 5000, 1).unwrap(),
            5_000_000_000_000
        );
        assert_eq!(
            mul_div(u128::MAX, u128::MAX, u128::MAX, true).unwrap(),
            u128::MAX
        );
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;

    #[test]
    fn month_segments_cover_only_the_accounts_lifetime_without_gaps_or_overlap() {
        let (a, b) = month_bounds(202609).unwrap();
        let segment = |start_ms, end_ms| MonthSegment {
            start_ms,
            end_ms,
            monthly_units: 10,
            source_contract_id: None,
        };
        let mid = a + (b - a) / 2;
        assert_eq!(monthly_allowance(202609, mid, &[segment(mid, b)]), Ok(5));
        assert_eq!(monthly_allowance(202609, b, &[]), Ok(0));
        for segments in [
            vec![],
            vec![segment(a, b - 1)],
            vec![segment(a, mid), segment(mid + 1, b)],
            vec![segment(a, mid), segment(mid - 1, b)],
            vec![segment(a, a)],
            vec![segment(a, b + 1)],
        ] {
            assert_eq!(
                monthly_allowance(202609, a, &segments),
                Err(Error::IntegrityFailed)
            );
        }
        assert_eq!(
            monthly_allowance(202609, mid, &[segment(a, b)]),
            Err(Error::IntegrityFailed)
        );
        let many = (0..64)
            .map(|i| segment(a + (b - a) * i / 64, a + (b - a) * (i + 1) / 64))
            .collect::<Vec<_>>();
        assert_eq!(monthly_allowance(202609, a, &many), Ok(10));
        assert_eq!(
            monthly_allowance(202609, a, &vec![segment(a, b); 65]),
            Err(Error::QuotaExceeded)
        );
    }

    #[test]
    fn calendars_fees_and_beneficiary_decoding_reject_invalid_inputs() {
        for month in [0, 202600, 202613, 196912, u32::MAX] {
            assert!(month_bounds(month).is_err());
        }
        assert!(next_year(u64::MAX).is_err());
        assert!(month_utc(u64::MAX).is_err());
        let (_, jan) = month_bounds(202612).unwrap();
        assert_eq!(month_utc(jan), Ok(202701));
        assert_eq!(cents_atomic(1, 0), Ok(1));
        assert!(cents_atomic(1, 19).is_err());
        let mut fee = dmsg_types::payment::DeliveryFeePolicy {
            version: 1,
            effective_at_ms: 0,
            rate_bps: 500,
            minimum_atomic: 20_000,
        };
        for (net, expected) in [(1, 20_000), (400_000, 20_000), (400_001, 20_001)] {
            assert_eq!(delivery_service_fee(net, &fee), Ok(expected));
        }
        fee.version = 0;
        assert!(delivery_service_fee(1, &fee).is_err());
        fee.version = 1;
        fee.rate_bps = 10_001;
        assert!(delivery_service_fee(1, &fee).is_err());
        let id = AccountId([1; 12]);
        let mut b = beneficiary(candid::Principal::from_slice(&[1]), &id);
        assert_eq!(beneficiary_account(&b), Ok(id));
        b.subject_bytes = vec![1; 11].into();
        assert!(matches!(
            beneficiary_account(&b),
            Err(Error::InvalidInput(_))
        ));
        b.product_id = "other".into();
        assert_eq!(beneficiary_account(&b), Err(Error::UnsupportedProtocol));
    }
}
