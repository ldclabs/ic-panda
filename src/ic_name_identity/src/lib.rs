use candid::{utils::ArgumentEncoder, Principal};
use serde_bytes::ByteBuf;
use std::collections::BTreeSet;

mod api;
mod api_admin;
mod api_init;
mod store;
mod types;

use api_init::ChainArgs;
use types::{Delegator, NameAccount, SignInResponse, SignedDelegation};

// "nscli-qiaaa-aaaaj-qa4pa-cai" dMsg minter canister id
static NAMECHAIN_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 1, 48, 7, 30, 1, 1]);
// "dwv6s-6aaaa-aaaaq-aacta-cai" ICPanda DAO canister id
static DAO_CANISTER: Principal = Principal::from_slice(&[0, 0, 0, 0, 2, 0, 0, 166, 1, 1]);

fn is_controller() -> Result<(), String> {
    let caller = ic_cdk::caller();
    if caller == DAO_CANISTER || ic_cdk::api::is_controller(&caller) {
        Ok(())
    } else {
        Err("caller is not a controller".to_string())
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
