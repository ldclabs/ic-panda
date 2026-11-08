//! ICRC transfers and cycles top-ups. Every transfer keeps its exact
//! arguments, so a retry after an unknown outcome is deduplicated by the
//! ledger instead of paying twice.
use candid::{Nat, Principal};
use cbor2::Cbor;
use dmsg_runtime::ledger::{MEMO_TOP_UP, cmc_subaccount};
pub(crate) use dmsg_runtime::ledger::{Notified, Sent, fee, notify_top_up, send};
use dmsg_types::{account::account_cbor, *};
use icrc_ledger_types::icrc1::{
    account::Account,
    transfer::{Memo, TransferArg},
};
use std::{cell::RefCell, collections::BTreeSet};

thread_local! {
    static TOP_UP_CALLS: RefCell<BTreeSet<u64>> = const { RefCell::new(BTreeSet::new()) };
}

/// Serialize live sends, notifications and reconciliation of one top-up.
/// Cancellation/upgrade releases this guard; the durable sent marker remains.
pub(crate) struct TopUpGuard(u64);

impl TopUpGuard {
    pub(crate) fn acquire(id: u64) -> Result<Self> {
        TOP_UP_CALLS.with_borrow_mut(|calls| {
            ensure(calls.insert(id), Error::Pending)?;
            Ok(Self(id))
        })
    }
}

impl Drop for TopUpGuard {
    fn drop(&mut self) {
        TOP_UP_CALLS.with_borrow_mut(|calls| calls.remove(&self.0));
    }
}

/// One ICRC transfer with its exact arguments.
#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub(crate) struct LedgerItem {
    #[cbor(key = 1)]
    #[serde(with = "account_cbor")]
    pub(crate) to: Account,
    #[cbor(key = 2)]
    pub(crate) amount: u128,
    #[cbor(key = 3)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) memo: Option<serde_bytes::ByteBuf>,
    #[cbor(key = 4)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) block: Option<u64>,
    /// Cleared before the first send. Older jobs conservatively count as sent.
    #[cbor(key = 5)]
    #[serde(default, skip_serializing_if = "dmsg_runtime::stable_types::is_zero")]
    pub(crate) unsent: bool,
}

/// Transfers of one proposal, sent in order from one subaccount.
#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub(crate) struct LedgerJob {
    #[cbor(key = 1)]
    pub(crate) ledger: Principal,
    #[cbor(key = 2)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) from: Option<Hash>,
    /// Fee and sender time, fixed before the first transfer.
    #[cbor(key = 3)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fee: Option<u128>,
    #[cbor(key = 4)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) created_at_time: Option<u64>,
    #[cbor(key = 5)]
    pub(crate) items: Vec<LedgerItem>,
}

/// ICP sent to the cycles minting canister and its notification.
#[derive(Clone, Debug, PartialEq, Eq, Cbor)]
pub(crate) struct TopUp {
    #[cbor(key = 1)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) from: Option<Hash>,
    #[cbor(key = 2)]
    pub(crate) amount: u64,
    #[cbor(key = 3)]
    pub(crate) created_at_time: u64,
    #[cbor(key = 4)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) block: Option<u64>,
    /// Cleared before sending; legacy jobs may already have moved funds.
    #[cbor(key = 5)]
    #[serde(default, skip_serializing_if = "dmsg_runtime::stable_types::is_zero")]
    pub(crate) unsent: bool,
}

fn transfer_arg(
    from: Option<Hash>,
    to: Account,
    amount: u128,
    fee: Option<u128>,
    memo: Option<Vec<u8>>,
    created_at_time: u64,
) -> TransferArg {
    TransferArg {
        from_subaccount: from.map(Hash::into_array),
        to,
        fee: fee.map(Nat::from),
        created_at_time: Some(created_at_time),
        memo: memo.map(|m| Memo(m.into())),
        amount: Nat::from(amount),
    }
}

impl LedgerJob {
    pub(crate) fn arg(&self, item: &LedgerItem) -> TransferArg {
        transfer_arg(
            self.from,
            item.to,
            item.amount,
            self.fee,
            item.memo.as_ref().map(|m| m.to_vec()),
            self.created_at_time.expect("fixed before sending"),
        )
    }
}

impl TopUp {
    pub(crate) fn arg(&self, cmc: Principal, canister: Principal) -> TransferArg {
        transfer_arg(
            self.from,
            Account {
                owner: cmc,
                subaccount: Some(cmc_subaccount(canister)),
            },
            u128::from(self.amount),
            None,
            Some(MEMO_TOP_UP.to_le_bytes().to_vec()),
            self.created_at_time,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_top_up_is_treated_as_already_sent() {
        // The released integer-keyed layout has no key 5. A pending payment
        // from that layout must never become a fresh, definitely-unsent job.
        #[derive(Cbor)]
        struct LegacyTopUp {
            #[cbor(key = 2)]
            amount: u64,
            #[cbor(key = 3)]
            created_at_time: u64,
        }
        let bytes = cbor2::to_vec(&LegacyTopUp {
            amount: 100_000_000,
            created_at_time: 123,
        })
        .unwrap();
        let mut job: TopUp = cbor2::from_slice(&bytes).unwrap();
        assert!(!job.unsent);
        assert_eq!(job.block, None);
        assert_eq!(job.amount, 100_000_000);
        assert_eq!(job.created_at_time, 123);
        for unsent in [true, false] {
            job.unsent = unsent;
            let restored: TopUp = cbor2::from_slice(&cbor2::to_vec(&job).unwrap()).unwrap();
            assert_eq!(restored, job);
        }
    }
}
