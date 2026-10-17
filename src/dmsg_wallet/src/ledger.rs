//! ICRC transfers and cycles top-ups. Every transfer keeps its exact
//! arguments, so a retry after an unknown outcome is deduplicated by the
//! ledger instead of paying twice.
use candid::{Nat, Principal};
use cbor2::Cbor;
use dmsg_runtime::ledger::{cmc_subaccount, MEMO_TOP_UP};
pub(crate) use dmsg_runtime::ledger::{fee, notify_top_up, send, Notified, Sent};
use dmsg_types::{account::account_cbor, *};
use icrc_ledger_types::icrc1::{
    account::Account,
    transfer::{Memo, TransferArg},
};

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
