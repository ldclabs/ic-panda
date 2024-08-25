use candid::{utils::ArgumentEncoder, CandidType, Nat, Principal};
use ic_cdk::api::management_canister::http_request::{CanisterHttpRequestArgument, HttpResponse};
use ic_cose_types::types::state::StateInfo;
use icrc_ledger_types::{
    icrc1::{
        account::Account,
        transfer::{Memo, TransferArg, TransferError},
    },
    icrc2::transfer_from::{TransferFromArgs, TransferFromError},
};
use std::collections::BTreeSet;

mod api_admin;
mod api_update;
mod store;
mod types;

static BURNING_PRINCIPAL: Principal = Principal::from_slice(&[0, 0, 0, 0, 2, 0, 0, 166, 1, 1]);

// "druyg-tyaaa-aaaaq-aactq-cai" PANDA token canister id
static TOKEN_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 2, 0, 0, 167, 1, 1]);

#[ic_cdk::query]
fn api_version() -> u16 {
    1
}

fn is_controller() -> Result<(), String> {
    let caller = ic_cdk::caller();
    if ic_cdk::api::is_controller(&caller) {
        Ok(())
    } else {
        Err("user is not a controller".to_string())
    }
}

fn is_controller_or_manager() -> Result<(), String> {
    let caller = ic_cdk::caller();
    if ic_cdk::api::is_controller(&caller) || store::state::is_manager(&caller) {
        Ok(())
    } else {
        Err("user is not a controller or manager".to_string())
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

async fn burn_token_from(user: Principal, amount: Nat) -> Result<Nat, String> {
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
                owner: BURNING_PRINCIPAL,
                subaccount: None,
            },
            fee: None,
            created_at_time: None,
            memo: None,
            amount,
        },),
        0,
    )
    .await?;
    res.map_err(|err| format!("failed to transfer tokens from user, error: {:?}", err))
}

async fn ips_add_caller(idempotent_proxy: Principal, subject: Principal) -> Result<bool, String> {
    let res: Result<bool, String> =
        call(idempotent_proxy, "admin_add_caller", (subject,), 0).await?;
    res
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
