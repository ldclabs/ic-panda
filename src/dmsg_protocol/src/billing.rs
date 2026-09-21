//! dmsg-commerce/1 arithmetic, calendar boundaries and authenticated leaf keys.
#![allow(missing_docs)]
use crate::{digest, membership::mul_div};
use chrono::{Datelike, TimeZone, Utc};
use dmsg_types::{billing::*, membership::*, *};

pub fn beneficiary(home: candid::Principal, account: &AccountId) -> Beneficiary {
    Beneficiary {
        product_id: "dmsg".into(),
        authority_canister: home,
        subject_schema: "dmsg-account-v1".into(),
        subject_bytes: account.to_vec().into(),
    }
}

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

pub fn entitlement_key(b: &Beneficiary) -> Hash {
    digest("dmsg/commerce/entitlement-key/v1", b)
}

pub fn catalog_key() -> Hash {
    digest("dmsg/commerce/catalog-key/v1", &"dmsg")
}

pub fn order_key(id: Hash) -> Hash {
    digest("dmsg/commerce/order-key/v1", &id)
}

pub fn order_digest(q: &OrderQuote) -> Hash {
    digest("dmsg/commerce/order/v1", q)
}

pub fn refund_digest(order_id: Hash) -> Hash {
    digest("dmsg/commerce/refund/v1", &order_id)
}

pub fn close_claim_digest(claim_id: Hash) -> Hash {
    digest("membership/close/v1", &claim_id)
}

pub fn plan_digest(plan: &PlanVersion) -> Hash {
    digest("dmsg/commerce/plan/v1", plan)
}

pub fn usage_key(account: &AccountId, month: u32) -> Hash {
    digest("dmsg/commerce/usage-key/v1", &(account, month))
}

fn datetime(ms: u64) -> Result<chrono::DateTime<Utc>> {
    let n = i64::try_from(ms).map_err(|_| invalid("timestamp"))?;
    chrono::DateTime::from_timestamp_millis(n).ok_or_else(|| invalid("timestamp"))
}

pub fn next_year(ms: u64) -> Result<u64> {
    let t = datetime(ms)?;
    let year = t.year().checked_add(1).ok_or(Error::QuotaExceeded)?;
    let next = t
        .with_year(year)
        .or_else(|| t.with_day(28).and_then(|d| d.with_year(year)))
        .ok_or_else(|| invalid("calendar year"))?;
    u64::try_from(next.timestamp_millis()).map_err(|_| invalid("timestamp"))
}

pub fn month_utc(ms: u64) -> Result<u32> {
    let t = datetime(ms)?;
    u32::try_from(t.year())
        .ok()
        .and_then(|v| v.checked_mul(100))
        .and_then(|v| v.checked_add(t.month()))
        .ok_or_else(|| invalid("UTC month"))
}

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

pub fn cents_atomic(cents: u64, decimals: u8) -> Result<u128> {
    ensure(decimals <= 18, invalid("ledger decimals"))?;
    mul_div(cents.into(), 10u128.pow(decimals.into()), 100, true)
}

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
                membership_policy_version: None,
            }
        },
    )
    .collect()
}

/// Platform fee is added to, never subtracted from, the recipient's net amount.
pub fn delivery_service_fee(
    net: u128,
    policy: &dmsg_types::payment::DeliveryFeePolicy,
) -> Result<u128> {
    ensure(
        policy.version > 0 && policy.rate_bps <= 10_000,
        invalid("fee policy"),
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
            crate::membership::required_panda(
                &Threshold::AnnualPrice {
                    price_cents: 1000,
                    r_num: 5000,
                    r_den: 1
                },
                8
            )
            .unwrap(),
            5_000_000_000_000
        );
        assert_eq!(
            mul_div(u128::MAX, u128::MAX, u128::MAX, true).unwrap(),
            u128::MAX
        );
    }
}
