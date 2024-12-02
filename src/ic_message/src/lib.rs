use candid::{utils::ArgumentEncoder, Nat, Principal};
use ic_cdk::api::management_canister::main::CanisterStatusResponse;
use ic_cose_types::ANONYMOUS;
use ic_message_types::{
    channel::{ChannelInfo, ChannelKEKInput, ChannelTopupInput, CreateChannelInput},
    profile::{UpdateKVInput, UserInfo},
};
use icrc_ledger_types::icrc3::{
    archive::{GetArchivesArgs, GetArchivesResult},
    blocks::{GetBlocksRequest, GetBlocksResult, ICRC3DataCertificate, SupportedBlockType},
};
use icrc_ledger_types::{
    icrc1::{
        account::Account,
        transfer::{Memo, TransferArg, TransferError},
    },
    icrc2::transfer_from::{TransferFromArgs, TransferFromError},
};
use serde_bytes::{ByteArray, ByteBuf};
use std::collections::BTreeSet;

mod api_admin;
mod api_init;
mod api_query;
mod api_update;
mod schnorr;
mod store;
mod types;

use crate::api_init::ChainArgs;

// "druyg-tyaaa-aaaaq-aactq-cai" PANDA token canister id
static TOKEN_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 2, 0, 0, 167, 1, 1]);
// "dwv6s-6aaaa-aaaaq-aacta-cai" ICPanda DAO canister id
static DAO_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 2, 0, 0, 166, 1, 1]);
// "ql553-iqaaa-aaaap-anuyq-cai" dMsg minter canister id
static MINTER_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 1, 224, 109, 49, 1, 1]);

fn is_controller() -> Result<(), String> {
    let caller = ic_cdk::caller();
    if caller == DAO_CANISTER || ic_cdk::api::is_controller(&caller) {
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

async fn call<In, Out>(id: Principal, method: &str, args: In, cycles: u128) -> Result<Out, String>
where
    In: ArgumentEncoder + Send,
    Out: candid::CandidType + for<'a> candid::Deserialize<'a>,
{
    let (res,): (Out,) = ic_cdk::api::call::call_with_payment128(id, method, args, cycles)
        .await
        .map_err(|(code, msg)| {
            format!(
                "failed to call {} on {:?}, code: {}, message: {}",
                method, &id, code as u32, msg
            )
        })?;
    Ok(res)
}

async fn token_transfer_to(user: Account, amount: Nat, memo: String) -> Result<Nat, String> {
    let res: Result<Nat, TransferError> = call(
        TOKEN_CANISTER,
        "icrc1_transfer",
        (TransferArg {
            from_subaccount: None,
            to: user,
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
                owner: ic_cdk::id(),
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
    res.map_err(|err| format!("failed to transfer tokens from user, error: {:?}", err))
}

#[cfg(all(
    target_arch = "wasm32",
    target_vendor = "unknown",
    target_os = "unknown"
))]
/// A getrandom implementation that always fails
pub fn always_fail(_buf: &mut [u8]) -> Result<(), getrandom::Error> {
    Err(getrandom::Error::UNSUPPORTED)
}

#[cfg(all(
    target_arch = "wasm32",
    target_vendor = "unknown",
    target_os = "unknown"
))]
getrandom::register_custom_getrandom!(always_fail);

ic_cdk::export_candid!();
