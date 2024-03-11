use candid::{Nat, Principal};
use num_traits::cast::ToPrimitive;
use std::convert::Into;

use icrc_ledger_types::{
    icrc1::{
        account::Account,
        transfer::{TransferArg, TransferError},
    },
    icrc2::transfer_from::{TransferFromArgs, TransferFromError},
};

mod api_admin;
mod api_init;
mod api_query;
mod api_update;
mod store;
mod types;
mod utils;

const SECOND: u64 = 1_000_000_000;
const TRANS_FEE: u64 = 10_000;
const TOKEN_1: u64 = 100_000_000;
const ICP_1: u64 = ic_ledger_types::Tokens::SUBDIVIDABLE_BY;
const AIRDROP_AMOUNT: u64 = 10; // 10 PANDA tokens

static ANONYMOUS: Principal = Principal::anonymous();
static ICP_CANISTER: Principal = ic_ledger_types::MAINNET_LEDGER_CANISTER_ID;

// "bkyz2-fmaaa-aaaaa-qaaaq-cai" TODO: update token canister id
static TOKEN_CANISTER: Principal = Principal::from_slice(&[128, 0, 0, 0, 0, 16, 0, 1, 1, 1]);
// "a7cug-2qaaa-aaaap-ab3la-cai" TODO: update dao canister id
static DAO_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 1, 224, 14, 214, 1, 1]);

fn nat_to_u64(nat: &Nat) -> u64 {
    nat.0.to_u64().unwrap_or(0)
}

fn is_controller() -> Result<(), String> {
    if ic_cdk::api::is_controller(&ic_cdk::caller()) {
        Ok(())
    } else {
        Err("user is not a controller".to_string())
    }
}

fn is_authenticated() -> Result<(), String> {
    if ic_cdk::caller() == ANONYMOUS {
        Err("anonymous user is not allowed".to_string())
    } else {
        Ok(())
    }
}

async fn token_balance_of(user: Principal) -> Result<Nat, String> {
    let (balance,) = ic_cdk::call(
        TOKEN_CANISTER,
        "icrc1_balance_of",
        (Account {
            owner: user,
            subaccount: None,
        },),
    )
    .await
    .map_err(|err| format!("failed to get balance, error: {:?}", err))?;
    Ok(balance)
}

async fn token_transfer_to(user: Principal, amount: Nat) -> Result<Nat, String> {
    let (res,): (Result<Nat, TransferError>,) = ic_cdk::call(
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
            memo: None,
            amount,
        },),
    )
    .await
    .map_err(|err| format!("failed to call icrc1_transfer, error: {:?}", err))?;
    res.map_err(|err| format!("failed to transfer tokens, error: {:?}", err))
}

async fn icp_transfer_to(user: Principal, amount: Nat) -> Result<Nat, String> {
    let (res,): (Result<Nat, TransferError>,) = ic_cdk::call(
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
            memo: None,
            amount,
        },),
    )
    .await
    .map_err(|err| format!("failed to call icrc1_transfer, error: {:?}", err))?;
    res.map_err(|err| format!("failed to transfer ICP, error: {:?}", err))
}

async fn icp_transfer_from(user: Principal, amount: Nat) -> Result<Nat, String> {
    let (res,): (Result<Nat, TransferFromError>,) = ic_cdk::call(
        ICP_CANISTER,
        "icrc2_transfer_from",
        (TransferFromArgs {
            spender_subaccount: None,
            from: Account {
                owner: user,
                subaccount: None,
            },
            to: Account {
                owner: ic_cdk::id(),
                subaccount: None,
            },
            fee: None,
            created_at_time: None,
            memo: None,
            amount,
        },),
    )
    .await
    .map_err(|err| format!("failed to call icrc2_transfer_from, error: {:?}", err))?;
    res.map_err(|err| format!("failed to transfer ICP from user, error: {:?}", err))
}

ic_cdk::export_candid!();

#[cfg(test)]
mod test {
    use ic_stable_structures::Storable;

    use super::*;

    #[test]
    fn get_principal() {
        let s = "a7cug-2qaaa-aaaap-ab3la-cai";
        let id = Principal::from_text(s).expect("invalid principal");
        println!("principal bytes: {:?}", id.to_bytes());

        let id2 = Principal::from_slice(&id.to_bytes());
        assert_eq!(s, id2.to_string());
    }
}
