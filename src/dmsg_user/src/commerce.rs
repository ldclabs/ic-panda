use crate::store;
use dmsg_protocol::{billing::*, digest};
use dmsg_runtime::storage::{CompactStored, MapExt};
use dmsg_types::{billing::*, *};
use ic_stable_structures::{memory_manager::VirtualMemory, DefaultMemoryImpl, StableBTreeMap};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Month {
    pub(crate) usage: ExecutionUsage,
    pub(crate) weights: ExecutionWeights,
    pub(crate) entitlement_digest: Hash,
}

type Memory = VirtualMemory<DefaultMemoryImpl>;
thread_local! {
    static MONTHS: RefCell<StableBTreeMap<Vec<u8>, CompactStored<Month>, Memory>> =
        RefCell::new(StableBTreeMap::init(store::memory(6)));
}

// Rows of one account are contiguous: account ID, then the YYYYMM month.
fn key(id: &AccountId, month: u32) -> Vec<u8> {
    [id.as_slice(), &month.to_be_bytes()].concat()
}

fn load(id: &AccountId, month: u32) -> Option<Month> {
    MONTHS.with_borrow(|t| t.load(&key(id, month)))
}

/// Months are not certified data; the owner gets a certified copy as the
/// `refresh_execution_entitlement` update reply.
pub(crate) fn save(m: &Month) {
    MONTHS.with_borrow_mut(|t| t.put(&key(&m.usage.account_id, m.usage.month_utc), m));
}

/// Keep `month` and the month before it. Attestations are charged when they
/// commit, so no older month can still receive a settlement.
fn retain_recent(id: &AccountId, month: u32) {
    let previous = if month % 100 == 1 {
        month - 89
    } else {
        month - 1
    };
    MONTHS.with_borrow_mut(|t| {
        let old: Vec<Vec<u8>> = t.keys_range(key(id, 0)..key(id, previous)).collect();
        for k in old {
            t.remove(&k);
        }
    });
}

pub fn usage(id: &AccountId, month: u32) -> Result<ExecutionUsage> {
    load(id, month).map(|m| m.usage).ok_or(Error::NotFound)
}

pub fn is_current(id: &AccountId, at: u64) -> Result<bool> {
    Ok(load(id, month_utc(at)?).is_some_and(|m| at < m.usage.valid_until_ms))
}

/// Fetch current business terms even if a previous lease is still valid.
/// Returns the refreshed time for authorization after the remote call.
pub async fn refresh(id: &AccountId, at: u64) -> Result<u64> {
    let month = month_utc(at)?;
    let s = store::load(id)?;
    let home = store::config().init.commerce_canister;
    let b = beneficiary(s.home_user, id);
    let result: Result<ExecutionEntitlement> = dmsg_runtime::call(
        home,
        "get_execution_entitlement",
        (b.clone(), month, s.created_at_ms),
    )
    .await?;
    let e = result?;
    let now = nanos_to_millis(ic_cdk::api::time());
    ensure(
        month_utc(now)? == month
            && e.view.home_commerce == home
            && e.view.beneficiary == b
            && e.month.beneficiary == b
            && e.month.month_utc == month
            && e.month.calculation_version == 1
            && e.month.business_revision == e.view.business_revision
            && e.view.issued_at_ms <= now
            && now < e.view.valid_until_ms,
        Error::MembershipStale,
    )?;
    ensure(
        monthly_allowance(month, s.created_at_ms, &e.month.segments)? == e.month.allowed_units,
        Error::IntegrityFailed,
    )?;
    let old = load(id, month);
    let entitlement_digest = digest("dmsg/stored-execution-entitlement/v1", &e);
    if let Some(m) = &old {
        ensure(
            e.view.business_revision >= m.usage.business_revision
                && e.view.lease_revision >= m.usage.lease_revision
                && e.month.month_revision >= m.usage.month_revision,
            Error::PolicyStale,
        )?;
        // Revision 0 is the stateless Free projection of an account without a
        // commerce record; it follows the catalogs, not a stored lease.
        if e.view.lease_revision == m.usage.lease_revision && e.view.lease_revision > 0 {
            ensure(
                entitlement_digest == m.entitlement_digest,
                Error::IntegrityFailed,
            )?;
        }
    }
    let usage = ExecutionUsage {
        account_id: *id,
        month_utc: month,
        month_revision: e.month.month_revision,
        business_revision: e.view.business_revision,
        lease_revision: e.view.lease_revision,
        weight_policy_version: e.month.weights.version,
        allowed_units: e.month.allowed_units,
        charged_units: old.as_ref().map_or(0, |m| m.usage.charged_units),
        // Recheck business terms hourly even when the resource lease runs longer,
        // so an upgrade reaches the execution allowance without a manual refresh.
        valid_until_ms: e.view.valid_until_ms.min(now.saturating_add(60 * MINUTE)),
    };
    if old.is_none() {
        retain_recent(id, month);
    }
    save(&Month {
        usage,
        weights: e.month.weights,
        entitlement_digest,
    });
    Ok(now)
}

/// Charge one attestation to the current month. An attestation completes in
/// the message that authorizes it, so nothing is held and settled later.
pub fn charge(id: &AccountId, now: u64) -> Result<()> {
    let month = month_utc(now)?;
    let mut m = load(id, month).ok_or(Error::MembershipStale)?;
    ensure(now < m.usage.valid_until_ms, Error::MembershipStale)?;
    let charged = m
        .usage
        .charged_units
        .checked_add(m.weights.ed25519)
        .ok_or(Error::QuotaExceeded)?;
    ensure(charged <= m.usage.allowed_units, Error::QuotaExceeded)?;
    m.usage.charged_units = charged;
    save(&m);
    Ok(())
}
