use candid::{utils::ArgumentEncoder, Nat, Principal};
use ic_stable_structures::Storable;
use icrc_ledger_types::{
    icrc1::{
        account::Account,
        transfer::{Memo, TransferArg, TransferError},
    },
    icrc2::transfer_from::{TransferFromArgs, TransferFromError},
};
use num_traits::cast::ToPrimitive;
use serde_bytes::ByteBuf;
use std::collections::BTreeSet;
use std::convert::Into;

mod api_admin;
mod api_init;
mod api_query;
mod api_update;
mod store;
mod types;
mod utils;

const SECOND: u64 = 1_000_000_000;
const TRANS_FEE: u64 = 10_000; // 0.0001 ICP
const TOKEN_1: u64 = 100_000_000;
const TOKEN_SMALL_UNIT: u64 = 10_000; // 0.0001 token
const MAX_PRIZE_CLAIMABLE: u64 = 420_000; // 420_000 tokens * TOKEN_SMALL_UNIT < u32::MAX
const ICP_1: u64 = 100_000_000;

// https://dashboard.internetcomputer.org/sns/d7wvo-iiaaa-aaaaq-aacsq-cai/proposal/108
const AIRDROP108_TIME_NS: u64 = 1731283200 * SECOND; // '2024-11-11T00:00:00.000Z'
const AIRDROP108_TOKENS: u64 = 320_000_000 * TOKEN_1;

static ANONYMOUS: Principal = Principal::anonymous();
// MAINNET_LEDGER_CANISTER_ID
static ICP_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 0, 0, 0, 2, 1, 1]);
// "druyg-tyaaa-aaaaq-aactq-cai" PANDA token canister id
static TOKEN_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 2, 0, 0, 167, 1, 1]);
// "dwv6s-6aaaa-aaaaq-aacta-cai" ICPanda DAO canister id
static DAO_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 2, 0, 0, 166, 1, 1]);

fn nat_to_u64(nat: &Nat) -> u64 {
    nat.0.to_u64().unwrap_or(0)
}

fn is_controller() -> Result<(), String> {
    let caller = ic_cdk::api::msg_caller();
    if caller == DAO_CANISTER || ic_cdk::api::is_controller(&caller) {
        Ok(())
    } else {
        Err("user is not a controller".to_string())
    }
}

fn is_authenticated() -> Result<(), String> {
    if ic_cdk::api::msg_caller() == ANONYMOUS {
        Err("anonymous user is not allowed".to_string())
    } else {
        Ok(())
    }
}

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

async fn token_balance_of(canister: Principal, user: Principal) -> Result<Nat, String> {
    let balance = call(
        canister,
        "icrc1_balance_of",
        (Account {
            owner: user,
            subaccount: None,
        },),
        0,
    )
    .await?;
    Ok(balance)
}

async fn icp_transfer_from(user: Principal, amount: Nat, memo: String) -> Result<Nat, String> {
    let res: Result<Nat, TransferFromError> = call(
        ICP_CANISTER,
        "icrc2_transfer_from",
        (TransferFromArgs {
            spender_subaccount: None,
            from: Account {
                owner: user,
                subaccount: None,
            },
            to: Account {
                owner: ic_cdk::api::canister_self(),
                subaccount: None,
            },
            fee: None,
            created_at_time: None,
            memo: Some(Memo(ByteBuf::from(memo.to_bytes()))),
            amount,
        },),
        0,
    )
    .await?;
    res.map_err(|err| format!("failed to transfer ICP from user, error: {:?}", err))
}

async fn token_transfer_to(account: Account, amount: Nat, memo: String) -> Result<Nat, String> {
    let res: Result<Nat, TransferError> = call(
        TOKEN_CANISTER,
        "icrc1_transfer",
        (TransferArg {
            from_subaccount: None,
            to: account,
            fee: None,
            created_at_time: None,
            memo: Some(Memo(ByteBuf::from(memo.to_bytes()))),
            amount,
        },),
        0,
    )
    .await?;
    res.map_err(|err| format!("failed to transfer tokens, error: {:?}", err))
}

async fn token_transfer_from(user: Principal, amount: Nat, memo: String) -> Result<Nat, String> {
    let res: Result<Nat, TransferFromError> = call(
        TOKEN_CANISTER,
        "icrc2_transfer_from",
        (TransferFromArgs {
            spender_subaccount: None,
            from: Account {
                owner: user,
                subaccount: None,
            },
            to: Account {
                owner: ic_cdk::api::canister_self(),
                subaccount: None,
            },
            fee: None,
            created_at_time: None,
            memo: Some(Memo(ByteBuf::from(memo.to_bytes()))),
            amount,
        },),
        0,
    )
    .await?;
    res.map_err(|err| format!("failed to transfer tokens from user, error: {:?}", err))
}

async fn icp_transfer_to(user: Principal, amount: Nat, memo: String) -> Result<Nat, String> {
    let res: Result<Nat, TransferError> = call(
        ICP_CANISTER,
        "icrc1_transfer",
        (TransferArg {
            from_subaccount: None,
            to: Account {
                owner: user,
                subaccount: None,
            },
            fee: None,
            created_at_time: None,
            memo: Some(Memo(ByteBuf::from(memo.to_bytes()))),
            amount,
        },),
        0,
    )
    .await?;
    res.map_err(|err| format!("failed to transfer ICP, error: {:?}", err))
}

ic_cdk::export_candid!();

#[cfg(test)]
mod test {
    use ic_stable_structures::Storable;

    use super::*;

    #[test]
    fn get_principal() {
        let s = "dwv6s-6aaaa-aaaaq-aacta-cai";
        let id = Principal::from_text(s).expect("invalid principal");
        println!("principal bytes: {:?}", id.to_bytes());

        let id2 = Principal::from_slice(&id.to_bytes());
        assert_eq!(s, id2.to_string());
    }
}
