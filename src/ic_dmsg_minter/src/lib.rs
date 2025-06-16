use candid::{utils::ArgumentEncoder, Nat, Principal};
use icrc_ledger_types::icrc1::{
    account::Account,
    transfer::{Memo, TransferArg, TransferError},
};
use serde_bytes::ByteBuf;

mod api;
mod store;

use crate::api::MinterArgs;

// "ocqzv-tyaaa-aaaar-qal4a-cai" DMSG token canister id
static TOKEN_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 2, 48, 2, 248, 1, 1]);

const TOKEN_1: u64 = 100_000_000; // 1 $DMSG
const TOKEN_FEE: u64 = 10_000; // 0.0001 $DMSG

async fn call<In, Out>(id: Principal, method: &str, args: In, cycles: u128) -> Result<Out, String>
where
    In: ArgumentEncoder + Send,
    Out: candid::CandidType + for<'a> candid::Deserialize<'a>,
{
    let res = ic_cdk::call::Call::bounded_wait(id, method)
        .with_args(&args)
        .with_cycles(cycles)
        .await
        .map_err(|err| format!("failed to call {} on {:?}, error: {:?}", method, &id, err))?;
    res.candid().map_err(|err| {
        format!(
            "failed to decode response from {} on {:?}, error: {:?}",
            method, &id, err
        )
    })
}

async fn token_transfer_to(user: Principal, amount: Nat, memo: String) -> Result<Nat, String> {
    let res: Result<Nat, TransferError> = call(
        TOKEN_CANISTER,
        "icrc1_transfer",
        (TransferArg {
            from_subaccount: None,
            to: Account {
                owner: user,
                subaccount: None,
            },
            fee: None,
            created_at_time: None,
            memo: Some(Memo(ByteBuf::from(memo.into_bytes()))),
            amount,
        },),
        0,
    )
    .await?;
    res.map_err(|err| format!("failed to transfer tokens, error: {:?}", err))
}

ic_cdk::export_candid!();
