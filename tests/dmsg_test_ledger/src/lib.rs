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
    icrc2::transfer_from::{TransferFromArgs, TransferFromError},
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
}
fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}
thread_local! {
    static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    static CONFIG: RefCell<StableCell<Stored<Config>, Memory>> = RefCell::new(StableCell::init(memory(0), Stored(Config { fee:10, next:0, lose:false, rejects:0 })));
    static BALANCES: RefCell<StableBTreeMap<Vec<u8>, Stored<u128>, Memory>> = RefCell::new(StableBTreeMap::init(memory(1)));
    static DUPLICATES: RefCell<StableBTreeMap<Vec<u8>, Stored<u64>, Memory>> = RefCell::new(StableBTreeMap::init(memory(2)));
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
) -> std::result::Result<Nat, TransferError> {
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
        ("ts".into(), n(ic_cdk::api::time().into())),
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
    let result = transfer(
        Account {
            owner: ic_cdk::api::msg_caller(),
            subaccount: a.from_subaccount,
        },
        &a,
        None,
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
    let result = transfer(a.from, &args, Some(spender)).map_err(|e| match e {
        TransferError::Duplicate { duplicate_of } => TransferFromError::Duplicate { duplicate_of },
        TransferError::BadFee { expected_fee } => TransferFromError::BadFee { expected_fee },
        TransferError::InsufficientFunds { balance } => {
            TransferFromError::InsufficientFunds { balance }
        }
        _ => TransferFromError::TemporarilyUnavailable,
    });
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
