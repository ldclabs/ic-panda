//! ICRC transfers and cycles top-ups. Every transfer keeps its exact
//! arguments, so a retry after an unknown outcome is deduplicated by the
//! ledger instead of paying twice.
use candid::{CandidType, Nat, Principal};
use dmsg_runtime::{call_classified, CallFailure};
use dmsg_types::{account::account_cbor, *};
use icrc_ledger_types::icrc1::{
    account::Account,
    transfer::{Memo, TransferArg, TransferError},
};
use serde::{Deserialize, Serialize};

/// `TPUP`: the cycles minting canister's top-up memo, as ICRC-1 memo bytes.
pub(crate) const MEMO_TOP_UP: u64 = 0x5055_5054;

/// One ICRC transfer with its exact arguments.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LedgerItem {
    #[serde(with = "account_cbor")]
    pub(crate) to: Account,
    pub(crate) amount: u128,
    pub(crate) memo: Option<serde_bytes::ByteBuf>,
    pub(crate) block: Option<u64>,
}

/// Transfers of one proposal, sent in order from one subaccount.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LedgerJob {
    pub(crate) ledger: Principal,
    pub(crate) from: Option<Hash>,
    /// Fee and sender time, fixed before the first transfer.
    pub(crate) fee: Option<u128>,
    pub(crate) created_at_time: Option<u64>,
    pub(crate) items: Vec<LedgerItem>,
}

/// ICP sent to the cycles minting canister and its notification.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TopUp {
    pub(crate) from: Option<Hash>,
    pub(crate) amount: u64,
    pub(crate) created_at_time: u64,
    pub(crate) block: Option<u64>,
    pub(crate) cycles: Option<u128>,
}

/// Outcome of one attempt.
pub(crate) enum Sent {
    Block(u64),
    /// The ledger refused the arguments; nothing was transferred.
    Refused(String),
    /// Nothing is known; resend the same arguments.
    Unknown,
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

/// Send one transfer; a duplicate answers with the original block.
pub(crate) async fn send(ledger: Principal, arg: TransferArg) -> Sent {
    let reply: std::result::Result<std::result::Result<Nat, TransferError>, CallFailure> =
        call_classified(ledger, "icrc1_transfer", (arg,)).await;
    match reply {
        Ok(Ok(n)) | Ok(Err(TransferError::Duplicate { duplicate_of: n })) => {
            dmsg_runtime::ledger::block_index(n).map_or(Sent::Unknown, Sent::Block)
        }
        Ok(Err(TransferError::TemporarilyUnavailable)) => Sent::Unknown,
        Ok(Err(e)) => Sent::Refused(format!("{e:?}")),
        Err(_) => Sent::Unknown,
    }
}

/// The ledger's current fee.
pub(crate) async fn fee(ledger: Principal) -> Result<u128> {
    let n: Nat = dmsg_runtime::call(ledger, "icrc1_fee", ()).await?;
    dmsg_runtime::ledger::token_amount(n)
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

/// The cycles minting canister's subaccount for top-ups of `canister`.
pub(crate) fn cmc_subaccount(canister: Principal) -> [u8; 32] {
    let bytes = canister.as_slice();
    let mut sub = [0; 32];
    sub[0] = bytes.len() as u8;
    sub[1..=bytes.len()].copy_from_slice(bytes);
    sub
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

#[derive(CandidType, Deserialize)]
struct NotifyTopUp {
    block_index: u64,
    canister_id: Principal,
}

#[derive(CandidType, Deserialize, Debug)]
enum NotifyError {
    Refunded {
        reason: String,
        block_index: Option<u64>,
    },
    Processing,
    TransactionTooOld(u64),
    InvalidTransaction(String),
    Other {
        error_code: u64,
        error_message: String,
    },
}

/// Outcome of a top-up notification.
pub(crate) enum Notified {
    Cycles(u128),
    /// The CMC returned the ICP; the top-up is over.
    Refunded(String),
    /// Ask again later.
    Pending,
}

pub(crate) async fn notify_top_up(cmc: Principal, block: u64, canister: Principal) -> Notified {
    let reply: std::result::Result<std::result::Result<Nat, NotifyError>, CallFailure> =
        call_classified(
            cmc,
            "notify_top_up",
            (NotifyTopUp {
                block_index: block,
                canister_id: canister,
            },),
        )
        .await;
    match reply {
        Ok(Ok(n)) => {
            dmsg_runtime::ledger::token_amount(n).map_or(Notified::Pending, Notified::Cycles)
        }
        Ok(Err(NotifyError::Refunded { reason, .. })) => Notified::Refunded(reason),
        Ok(Err(NotifyError::InvalidTransaction(reason))) => Notified::Refunded(reason),
        Ok(Err(NotifyError::TransactionTooOld(_))) => {
            Notified::Refunded("transaction too old".into())
        }
        Ok(Err(_)) | Err(_) => Notified::Pending,
    }
}
