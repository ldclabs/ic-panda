use candid::Principal;
use ic_cdk::management_canister::{canister_status, CanisterStatusArgs, CanisterStatusResult};
use ic_cose_types::{format_error, to_cbor_bytes};
use ic_message_types::profile::UserInfo;
use icrc_ledger_types::icrc3::{
    archive::{GetArchivesArgs, GetArchivesResult},
    blocks::{GetBlocksRequest, GetBlocksResult, ICRC3DataCertificate, SupportedBlockType},
};
use serde_bytes::ByteBuf;
use std::collections::BTreeSet;

use crate::{is_authenticated, store, types};

#[ic_cdk::query]
fn get_state() -> Result<types::StateInfo, String> {
    Ok(store::state::with(|s| types::StateInfo {
        name: s.name.clone(),
        managers: s.managers.clone(),
        cose_canisters: s.cose_canisters.clone(),
        profile_canisters: s.profile_canisters.clone(),
        channel_canisters: s.channel_canisters.clone(),
        matured_channel_canisters: s.matured_channel_canisters.clone(),
        price: s.price.clone(),
        names_total: store::user::names_total(),
        users_total: store::user::users_total(),
        incoming_total: s.incoming_total,
        transfer_out_total: s.transfer_out_total,
        next_block_height: s.next_block_height,
        next_block_phash: s.next_block_phash,
        latest_usernames: s.latest_usernames.clone().into(),
    }))
}

#[ic_cdk::query]
async fn get_canister_status() -> Result<CanisterStatusResult, String> {
    store::state::is_manager(&ic_cdk::api::msg_caller())?;

    let res = canister_status(&CanisterStatusArgs {
        canister_id: ic_cdk::api::canister_self(),
    })
    .await
    .map_err(format_error)?;
    Ok(res)
}

#[ic_cdk::query]
pub fn icrc3_supported_block_types() -> Vec<SupportedBlockType> {
    vec![SupportedBlockType {
        block_type: "ic-message".to_string(),
        url: "https://github.com/ldclabs/ic-panda/tree/main/src/ic_message".to_string(),
    }]
}

#[ic_cdk::query]
pub fn icrc3_get_tip_certificate() -> Option<ICRC3DataCertificate> {
    let certificate = ByteBuf::from(ic_cdk::api::data_certificate()?);
    let hash_tree = store::state::with(|s| s.hash_tree());
    let buf = to_cbor_bytes(&hash_tree);
    Some(ICRC3DataCertificate {
        certificate,
        hash_tree: ByteBuf::from(buf),
    })
}

#[ic_cdk::query]
pub fn icrc3_get_archives(_args: GetArchivesArgs) -> GetArchivesResult {
    vec![] // TODO: implement
}

#[ic_cdk::query]
pub fn icrc3_get_blocks(args: Vec<GetBlocksRequest>) -> GetBlocksResult {
    store::user::get_blocks(args)
}

#[ic_cdk::query]
fn search_username(prefix: String) -> Result<Vec<String>, String> {
    Ok(store::user::search_username(prefix))
}

#[ic_cdk::query]
fn get_by_username(username: String) -> Result<UserInfo, String> {
    store::user::get_by_username(username.to_ascii_lowercase())
}

#[ic_cdk::query]
fn get_user(user: Option<Principal>) -> Result<UserInfo, String> {
    store::user::get(user.unwrap_or(ic_cdk::api::msg_caller()))
}

#[ic_cdk::query(guard = "is_authenticated")]
fn batch_get_users(ids: BTreeSet<Principal>) -> Result<Vec<UserInfo>, String> {
    Ok(store::user::batch_get(ids))
}

#[ic_cdk::query(guard = "is_authenticated")]
fn my_iv() -> Result<ByteBuf, String> {
    let pk = store::state::ed25519_public_key(&ic_cdk::api::msg_caller())?;
    Ok(pk.public_key)
}
