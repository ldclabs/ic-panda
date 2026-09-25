//! Fault-injecting ICRC test double. Never deploy this module with real assets.
use candid::Nat;
use dmsg_protocol::digest;
use dmsg_runtime::{
    self as stable,
    storage::{MapExt, Stored},
};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use icrc_ledger_types::{
    icrc::generic_value::ICRC3Value as Value,
    icrc1::{
        account::Account,
        transfer::{TransferArg, TransferError},
    },
    icrc2::{
        allowance::{Allowance, AllowanceArgs},
        transfer_from::{TransferFromArgs, TransferFromError},
    },
    icrc3::blocks::{BlockWithId, GetBlocksRequest, GetBlocksResult},
};
use num_traits::ToPrimitive;
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeMap};
type Memory = VirtualMemory<DefaultMemoryImpl>;
#[derive(Clone, Serialize, Deserialize)]
struct Config {
    fee: u128,
    next: u64,
    lose: bool,
    rejects: u32,
    delay: u8,
}

fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}
thread_local! {
    static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    static CONFIG: RefCell<StableCell<Stored<Config>, Memory>> = RefCell::new(StableCell::init(memory(0), Stored(Config { fee:10, next:0, lose:false, rejects:0, delay:0 })));
    static BALANCES: RefCell<StableBTreeMap<Vec<u8>, Stored<u128>, Memory>> = RefCell::new(StableBTreeMap::init(memory(1)));
    static DUPLICATES: RefCell<StableBTreeMap<Vec<u8>, Stored<u64>, Memory>> = RefCell::new(StableBTreeMap::init(memory(2)));
    static ALLOWANCES: RefCell<StableBTreeMap<Vec<u8>, Stored<u128>, Memory>> = RefCell::new(StableBTreeMap::init(memory(4)));
    static BLOCKS: RefCell<StableBTreeMap<Vec<u8>, Stored<Value>, Memory>> = RefCell::new(StableBTreeMap::init(memory(3)));
}

fn config() -> Config {
    CONFIG.with_borrow(|c| c.get().0.clone())
}

fn configure(f: impl FnOnce(&mut Config)) {
    CONFIG.with_borrow_mut(|c| {
        let mut v = c.get().0.clone();
        f(&mut v);
        c.set(Stored(v));
    });
}

fn balance(a: Account) -> u128 {
    BALANCES.with_borrow(|t| t.load(digest("balance", &a).as_slice()).unwrap_or(0))
}

fn write_balance(a: Account, n: u128) {
    BALANCES.with_borrow_mut(|t| t.put(digest("balance", &a).as_slice(), &n));
}

fn fee() -> u128 {
    config().fee
}

fn n(n: u128) -> Value {
    Value::Nat(n.into())
}

fn account(a: Account) -> Value {
    let mut v = vec![Value::Blob(a.owner.as_slice().to_vec().into())];
    if let Some(s) = a.subaccount {
        v.push(Value::Blob(s.to_vec().into()));
    }
    Value::Array(v)
}

#[ic_cdk::update]
fn mint_test(a: Account, n: u128) {
    write_balance(a, balance(a) + n);
}

// Fixture setup only: approval fees/history are outside this test double.
#[ic_cdk::update]
fn approve_test(from: Account, spender: Account, amount: u128) {
    ALLOWANCES
        .with_borrow_mut(|t| t.put(digest("allowance", &(from, spender)).as_slice(), &amount));
}

#[ic_cdk::query]
fn icrc2_allowance(args: AllowanceArgs) -> Allowance {
    Allowance {
        allowance: allowance(args.account, args.spender).into(),
        expires_at: None,
    }
}

fn allowance(from: Account, spender: Account) -> u128 {
    ALLOWANCES.with_borrow(|t| {
        t.load(digest("allowance", &(from, spender)).as_slice())
            .unwrap_or(0)
    })
}

#[ic_cdk::update]
fn set_fee(n: u128) {
    configure(|c| c.fee = n);
}

#[ic_cdk::update]
fn lose_next_response() {
    configure(|c| c.lose = true);
}

#[ic_cdk::update]
fn reject_next_transfers(count: u32) {
    configure(|c| c.rejects = count);
}

/// Hold the next transfer reply for `rounds` consensus rounds after committing
/// it, so a test can upgrade the caller before its callback runs.
#[ic_cdk::update]
fn delay_next_response(rounds: u8) {
    configure(|c| c.delay = rounds);
}

#[ic_cdk::update]
fn barrier() {}

#[ic_cdk::query]
fn icrc1_balance_of(a: Account) -> Nat {
    balance(a).into()
}

fn transfer(
    from: Account,
    a: &TransferArg,
    spender: Option<Account>,
    at: u64,
) -> std::result::Result<Nat, TransferError> {
    // Like the ICRC-1 ledger, deduplication spans 24 hours plus the permitted drift.
    const WINDOW_NS: u64 = (24 * 3600 + 60) * 1_000_000_000;
    if a.created_at_time
        .is_some_and(|t| t.saturating_add(WINDOW_NS) < at)
    {
        return Err(TransferError::TooOld);
    }
    let key = digest("transfer", &(from, a, spender));
    if let Some(block) = DUPLICATES.with_borrow(|t| t.load(key.as_slice())) {
        return Err(TransferError::Duplicate {
            duplicate_of: block.into(),
        });
    }
    let rejects = config().rejects;
    if rejects > 0 {
        configure(|c| c.rejects = rejects - 1);
        return Err(TransferError::TemporarilyUnavailable);
    }
    let amount = a.amount.0.to_u128().unwrap();
    let fee = fee();
    if a.fee.as_ref().is_some_and(|n| n.0.to_u128() != Some(fee)) {
        return Err(TransferError::BadFee {
            expected_fee: fee.into(),
        });
    }
    if balance(from) < amount + fee {
        return Err(TransferError::InsufficientFunds {
            balance: balance(from).into(),
        });
    }
    write_balance(from, balance(from) - amount - fee);
    write_balance(a.to, balance(a.to) + amount);
    let index: u64 = config().next;
    let mut tx = BTreeMap::from([
        ("op".into(), Value::Text("xfer".into())),
        ("from".into(), account(from)),
        ("to".into(), account(a.to)),
        ("amt".into(), n(amount)),
        ("fee".into(), n(fee)),
    ]);
    if let Some(memo) = &a.memo {
        tx.insert("memo".into(), Value::Blob(memo.0.clone()));
    }
    if let Some(time) = a.created_at_time {
        tx.insert("ts".into(), n(time.into()));
    }
    if let Some(s) = spender {
        tx.insert("spender".into(), account(s));
    }
    let block = Value::Map(BTreeMap::from([
        ("ts".into(), n(at.into())),
        ("tx".into(), Value::Map(tx)),
        (
            "btype".into(),
            Value::Text(if spender.is_some() { "2xfer" } else { "1xfer" }.into()),
        ),
    ]));
    BLOCKS.with_borrow_mut(|t| t.put(&index.to_be_bytes(), &block));
    configure(|c| c.next = index + 1);
    DUPLICATES.with_borrow_mut(|t| t.put(key.as_slice(), &index));
    Ok(index.into())
}

#[ic_cdk::update]
async fn icrc1_transfer(a: TransferArg) -> std::result::Result<Nat, TransferError> {
    let at = ic_cdk::api::time();
    let result = transfer(
        Account {
            owner: ic_cdk::api::msg_caller(),
            subaccount: a.from_subaccount,
        },
        &a,
        None,
        at,
    );
    let lose = config().lose;
    if lose && result.is_ok() {
        configure(|c| c.lose = false);
        // Commit funds in one message before losing the subsequent response.
        let _: () = stable::call(ic_cdk::api::canister_self(), "barrier", ())
            .await
            .unwrap();
        ic_cdk::trap("injected lost response after committing the transfer");
    }
    let delay = config().delay;
    if delay > 0 {
        configure(|c| c.delay = 0);
        // Each raw_rand reply arrives in a later round.
        for _ in 0..delay {
            let _: Vec<u8> = stable::call(candid::Principal::management_canister(), "raw_rand", ())
                .await
                .unwrap();
        }
    }
    result
}

#[ic_cdk::update]
async fn icrc2_transfer_from(a: TransferFromArgs) -> std::result::Result<Nat, TransferFromError> {
    let spender = Account {
        owner: ic_cdk::api::msg_caller(),
        subaccount: a.spender_subaccount,
    };
    let args = TransferArg {
        from_subaccount: a.from.subaccount,
        to: a.to,
        fee: a.fee,
        created_at_time: a.created_at_time,
        memo: a.memo,
        amount: a.amount,
    };
    let at = ic_cdk::api::time();
    if let Some(created) = args.created_at_time {
        if created.saturating_add(24 * 60 * 60 * 1_000_000_000) < at {
            return Err(TransferFromError::TooOld);
        }
        if created > at.saturating_add(60 * 1_000_000_000) {
            return Err(TransferFromError::CreatedInFuture { ledger_time: at });
        }
    }
    // A retry must still return Duplicate after the original debit consumed
    // the allowance. Approval is needed only for a new transfer.
    let key = digest("transfer", &(a.from, &args, Some(spender)));
    if let Some(block) = DUPLICATES.with_borrow(|t| t.load(key.as_slice())) {
        return Err(TransferFromError::Duplicate {
            duplicate_of: block.into(),
        });
    }
    let approved = allowance(a.from, spender);
    let debit = args.amount.0.to_u128().unwrap().checked_add(fee()).unwrap();
    if a.from != spender && approved < debit {
        return Err(TransferFromError::InsufficientAllowance {
            allowance: approved.into(),
        });
    }
    let result = transfer(a.from, &args, Some(spender), at).map_err(|e| match e {
        TransferError::Duplicate { duplicate_of } => TransferFromError::Duplicate { duplicate_of },
        TransferError::BadFee { expected_fee } => TransferFromError::BadFee { expected_fee },
        TransferError::InsufficientFunds { balance } => {
            TransferFromError::InsufficientFunds { balance }
        }
        _ => TransferFromError::TemporarilyUnavailable,
    });
    if result.is_ok() && a.from != spender {
        approve_test(a.from, spender, approved - debit);
    }
    if result.is_ok() && config().lose {
        configure(|c| c.lose = false);
        let _: () = stable::call(ic_cdk::api::canister_self(), "barrier", ())
            .await
            .unwrap();
        ic_cdk::trap("injected lost transfer_from response after committing funds");
    }
    result
}

#[ic_cdk::query]
fn icrc3_get_blocks(args: Vec<GetBlocksRequest>) -> GetBlocksResult {
    let mut blocks = vec![];
    for r in args {
        let (start, length) = r.as_start_and_length().unwrap();
        for index in start..start + length.min(64) {
            if let Some(block) = BLOCKS.with_borrow(|t| t.load(&index.to_be_bytes())) {
                blocks.push(BlockWithId {
                    id: index.into(),
                    block,
                });
            }
        }
    }
    GetBlocksResult {
        log_length: config().next.into(),
        blocks,
        archived_blocks: vec![],
    }
}
ic_cdk::export_candid!();

#[ic_cdk::query]
fn icrc1_decimals() -> u8 {
    6
}

#[ic_cdk::query]
fn icrc1_fee() -> Nat {
    fee().into()
}

#[derive(candid::CandidType, serde::Serialize)]
struct SupportedStandard {
    name: String,
    url: String,
}
#[ic_cdk::query]
fn icrc1_supported_standards() -> Vec<SupportedStandard> {
    vec![
        SupportedStandard {
            name: "ICRC-1".into(),
            url: "https://github.com/dfinity/ICRC-1".into(),
        },
        SupportedStandard {
            name: "ICRC-3".into(),
            url: "https://github.com/dfinity/ICRC-1/tree/main/standards/ICRC-3".into(),
        },
    ]
}
