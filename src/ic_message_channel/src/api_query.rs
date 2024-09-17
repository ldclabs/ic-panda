use ic_cdk::api::management_canister::main::{
    canister_status, CanisterIdRecord, CanisterStatusResponse,
};
use ic_cose_types::format_error;
use std::collections::BTreeSet;

use crate::{is_authenticated, store, types};

#[ic_cdk::query]
fn get_state() -> Result<types::StateInfo, String> {
    Ok(store::state::with(|s| types::StateInfo {
        name: s.name.clone(),
        managers: s.managers.clone(),
        channel_id: s.channel_id,
        incoming_gas: s.incoming_gas,
        burned_gas: s.burned_gas,
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
async fn my_channel_ids() -> Result<Vec<u32>, String> {
    let caller = ic_cdk::caller();
    let ids: Vec<u32> = store::state::with(|s| {
        s.user_channels
            .get(&caller)
            .map(|m| m.keys().map(|k| *k).collect())
            .unwrap_or_default()
    });
    Ok(ids)
}

#[ic_cdk::query(guard = "is_authenticated")]
async fn my_channels_if_update(
    updated_at: Option<u64>,
) -> Result<Vec<types::ChannelBasicInfo>, String> {
    let caller = ic_cdk::caller();
    let updated_at = updated_at.unwrap_or(0);
    let ids: BTreeSet<u32> = store::state::with(|s| {
        s.user_channels
            .get(&caller)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| if v > &updated_at { Some(*k) } else { None })
                    .collect()
            })
            .unwrap_or_default()
    });
    Ok(store::channel::batch_get(caller, ids))
}

#[ic_cdk::query(guard = "is_authenticated")]
fn get_message(channel: u32, id: u32) -> Result<types::Message, String> {
    let caller = ic_cdk::caller();
    store::channel::get_message(caller, channel, id)
}

#[ic_cdk::query(guard = "is_authenticated")]
fn list_messages(
    channel: u32,
    start: Option<u32>,
    end: Option<u32>,
) -> Result<Vec<types::Message>, String> {
    let caller = ic_cdk::caller();
    store::channel::list_messages(caller, channel, start.unwrap_or(0), end.unwrap_or(0))
}
