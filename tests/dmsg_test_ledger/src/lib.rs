//! Fault-injecting ICRC test double. Never deploy this module with real assets.
use candid::Nat;
use dmsg_types::{
    digest,
    stable::{self, Table},
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
use std::{cell::RefCell, collections::BTreeMap};
thread_local! {static DATA:RefCell<Table>=RefCell::new(Table::new(0));}
fn balance(a: Account) -> u128 {
    DATA.with_borrow(|t| t.get(digest("balance", &a).as_slice()).unwrap_or(0))
}
fn write_balance(a: Account, n: u128) {
    DATA.with_borrow_mut(|t| t.put(digest("balance", &a).as_slice(), &n));
}
fn fee() -> u128 {
    DATA.with_borrow(|t| t.get(b"fee").unwrap_or(10))
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
    DATA.with_borrow_mut(|t| t.put(b"fee", &n));
}
#[ic_cdk::update]
fn lose_next_response() {
    DATA.with_borrow_mut(|t| t.put(b"lose", &true));
}
#[ic_cdk::update]
fn reject_next_transfers(count: u32) {
    DATA.with_borrow_mut(|t| t.put(b"rejects", &count));
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
    if let Some(block) = DATA.with_borrow(|t| t.get::<u64>(key.as_slice())) {
        return Err(TransferError::Duplicate {
            duplicate_of: block.into(),
        });
    }
    let rejects = DATA.with_borrow(|t| t.get::<u32>(b"rejects").unwrap_or(0));
    if rejects > 0 {
        DATA.with_borrow_mut(|t| t.put(b"rejects", &(rejects - 1)));
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
    let index: u64 = DATA.with_borrow(|t| t.get(b"next").unwrap_or(0));
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
    DATA.with_borrow_mut(|t| {
        t.put(&index.to_be_bytes(), &block);
        t.put(b"next", &(index + 1));
        t.put(key.as_slice(), &index);
    });
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
    let lose = DATA.with_borrow(|t| t.get::<bool>(b"lose").unwrap_or(false));
    if lose && result.is_ok() {
        DATA.with_borrow_mut(|t| t.put(b"lose", &false));
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
    if result.is_ok() && DATA.with_borrow(|t| t.get::<bool>(b"lose").unwrap_or(false)) {
        DATA.with_borrow_mut(|t| t.put(b"lose", &false));
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
            if let Some(block) = DATA.with_borrow(|t| t.get::<Value>(&index.to_be_bytes())) {
                blocks.push(BlockWithId {
                    id: index.into(),
                    block,
                });
            }
        }
    }
    GetBlocksResult {
        log_length: DATA
            .with_borrow(|t| t.get::<u64>(b"next").unwrap_or(0))
            .into(),
        blocks,
        archived_blocks: vec![],
    }
}
ic_cdk::export_candid!();
