use ic_cdk::api::management_canister::main::{
    canister_status, CanisterIdRecord, CanisterStatusResponse,
};
use ic_cose_types::format_error;
use std::collections::{BTreeMap, BTreeSet};

use crate::{is_authenticated, store, types};

#[ic_cdk::query]
fn get_state() -> Result<types::StateInfo, String> {
    Ok(store::state::with(|s| types::StateInfo {
        name: s.name.clone(),
        managers: s.managers.clone(),
        channel_id: s.channel_id,
        channels_total: store::channel::channels_total(),
        messages_total: store::channel::messages_total(),
    }))
}

#[ic_cdk::query]
async fn get_canister_status() -> Result<CanisterStatusResponse, String> {
    store::state::is_manager(&ic_cdk::caller())?;

    let (res,) = canister_status(CanisterIdRecord {
        canister_id: ic_cdk::id(),
    })
    .await
    .map_err(format_error)?;
    Ok(res)
}

#[ic_cdk::query(guard = "is_authenticated")]
async fn get_channel_if_update(
    id: u32,
    updated_at: u64,
) -> Result<Option<types::ChannelInfo>, String> {
    store::channel::get_if_update(ic_cdk::caller(), id, updated_at)
}

#[ic_cdk::query(guard = "is_authenticated")]
async fn batch_get_channels(ids: BTreeSet<u32>) -> Result<Vec<types::ChannelBasicInfo>, String> {
    if ids.len() > 100 {
        return Err("too many channels".to_string());
    }

    Ok(store::channel::batch_get(ic_cdk::caller(), ids))
}

#[ic_cdk::query(guard = "is_authenticated")]
async fn my_channels() -> Result<Vec<types::ChannelBasicInfo>, String> {
    let caller = ic_cdk::caller();
    let ids: BTreeSet<u32> = store::state::with(|s| {
        s.user_channels
            .get(&caller)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default()
    });
    Ok(store::channel::batch_get(caller, ids))
}

#[ic_cdk::query(guard = "is_authenticated")]
async fn my_channels_latest() -> Result<BTreeMap<u32, u32>, String> {
    let caller = ic_cdk::caller();
    Ok(store::state::user_channels(&caller))
}

#[ic_cdk::query(guard = "is_authenticated")]
fn get_message(channel: u32, id: u32) -> Result<types::Message, String> {
    let caller = ic_cdk::caller();
    store::channel::get_message(caller, channel, id)
}

#[ic_cdk::query(guard = "is_authenticated")]
fn list_messages(
    channel: u32,
    prev: Option<u32>,
    take: Option<u32>,
    util: Option<u32>,
) -> Result<Vec<types::Message>, String> {
    let caller = ic_cdk::caller();
    let take = take.unwrap_or(10).min(100);
    store::channel::list_messages(caller, channel, prev.unwrap_or(0), take, util.unwrap_or(0))
}
