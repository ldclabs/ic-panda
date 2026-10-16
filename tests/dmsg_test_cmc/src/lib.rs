//! Cycles minting canister test double for `notify_top_up`: it reads the
//! block from the test ledger, checks the top-up subaccount and memo, and
//! deposits 10,000 cycles per e8s. Never deploy it with real assets.
use candid::{CandidType, Nat, Principal};
use serde::Deserialize;
use std::{cell::RefCell, collections::BTreeSet};

const MEMO_TOP_UP: u64 = 0x5055_5054;
const CYCLES_PER_E8S: u128 = 10_000;

thread_local! {
    static LEDGER: RefCell<Option<Principal>> = const { RefCell::new(None) };
    static USED: RefCell<BTreeSet<u64>> = const { RefCell::new(BTreeSet::new()) };
}

#[ic_cdk::init]
fn init(ledger: Principal) {
    LEDGER.with_borrow_mut(|l| *l = Some(ledger));
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

fn subaccount(canister: Principal) -> [u8; 32] {
    let bytes = canister.as_slice();
    let mut sub = [0; 32];
    sub[0] = bytes.len() as u8;
    sub[1..=bytes.len()].copy_from_slice(bytes);
    sub
}

#[ic_cdk::update]
async fn notify_top_up(arg: NotifyTopUp) -> Result<Nat, NotifyError> {
    let ledger = LEDGER.with_borrow(|l| l.expect("ledger"));
    let invalid = |m: &str| NotifyError::InvalidTransaction(m.into());
    let t = dmsg_runtime::ledger::read_transfer(ledger, arg.block_index)
        .await
        .map_err(|e| invalid(&e.to_string()))?;
    let me = ic_cdk::api::canister_self();
    if t.to.owner != me || t.to.subaccount != Some(subaccount(arg.canister_id)) {
        return Err(invalid("not a top-up of this canister"));
    }
    if t.memo.as_deref() != Some(&MEMO_TOP_UP.to_le_bytes()[..]) {
        return Err(invalid("memo"));
    }
    if !USED.with_borrow_mut(|u| u.insert(arg.block_index)) {
        return Err(NotifyError::Processing);
    }
    let cycles = t.amount * CYCLES_PER_E8S;
    ic_cdk_management_canister::deposit_cycles(
        &ic_cdk_management_canister::CanisterIdRecord {
            canister_id: arg.canister_id,
        },
        cycles,
    )
    .await
    .map_err(|e| NotifyError::Other {
        error_code: 1,
        error_message: e.to_string(),
    })?;
    Ok(Nat::from(cycles))
}
