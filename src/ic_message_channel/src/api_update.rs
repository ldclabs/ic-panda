use candid::Principal;
use ic_cose_types::MILLISECONDS;
use ic_message_types::profile::UploadImageInput;
use ic_oss_types::MapValue;
use std::collections::hash_map::Entry;

use crate::{
    is_authenticated,
    store::{self, ChannelSetting},
    types,
};

#[ic_cdk::update(guard = "is_authenticated")]
fn update_channel(input: types::UpdateChannelInput) -> Result<types::Message, String> {
    input.validate()?;

    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::channel::manager_with_mut(caller, input.id, |c| {
        if let Some(name) = input.name {
            c.name = name;
        }
        if let Some(image) = input.image {
            c.image = image;
        }
        if let Some(description) = input.description {
            c.description = description;
        }
        c.updated_at = now_ms;
        c.latest_message_id += 1;
        c.latest_message_at = now_ms;
        c.latest_message_by = caller;

        let users: Vec<&Principal> = c.managers.keys().chain(c.members.keys()).collect();
        store::state::update_users_channel(&users, input.id, now_ms);
        Ok(store::channel::add_sys_message(
            caller,
            now_ms,
            store::MessageId(input.id, c.latest_message_id),
            types::SYS_MSG_CHANNEL_UPDATE_INFO.to_string(),
        ))
    })
}

#[ic_cdk::update(guard = "is_authenticated")]
async fn update_storage(input: types::UpdateChannelStorageInput) -> Result<types::Message, String> {
    input.validate()?;

    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::channel::update_storage(input.id, caller, input.file_max_size, now_ms).await
}

#[ic_cdk::update(guard = "is_authenticated")]
fn update_manager(
    input: types::UpdateChannelMemberInput,
) -> Result<(u64, Option<types::Message>), String> {
    input.validate()?;

    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::channel::manager_with_mut(caller, input.id, |c| {
        let is_new = match c.managers.entry(input.member) {
            Entry::Occupied(mut e) => {
                let s = e.get_mut();
                if s.ecdh_pub != input.ecdh.ecdh_pub {
                    Err("ecdh_pub mismatch".to_string())?;
                }
                s.ecdh_remote = input.ecdh.ecdh_remote;
                s.updated_at = now_ms;
                false
            }
            Entry::Vacant(e) => match c.members.remove(&input.member) {
                Some(mut s) => {
                    if s.ecdh_pub != input.ecdh.ecdh_pub {
                        Err("ecdh_pub mismatch".to_string())?;
                    }
                    s.ecdh_remote = input.ecdh.ecdh_remote;
                    s.updated_at = now_ms;
                    e.insert(s);
                    true
                }
                None => {
                    e.insert(ChannelSetting::from_ecdh(input.ecdh, now_ms));
                    true
                }
            },
        };

        if c.managers.len() > types::MAX_CHANNEL_MANAGERS {
            Err("too many managers".to_string())?;
        }

        c.updated_at = now_ms;
        if is_new {
            c.latest_message_id += 1;
            c.latest_message_at = now_ms;
            c.latest_message_by = caller;

            if !store::state::user_add_channel(input.member, input.id, now_ms) {
                Err("too many channels".to_string())?;
            }

            Ok((
                now_ms,
                Some(store::channel::add_sys_message(
                    caller,
                    now_ms,
                    store::MessageId(input.id, c.latest_message_id),
                    format!(
                        "{}: {}",
                        types::SYS_MSG_CHANNEL_ADD_MANAGER,
                        input.member.to_text()
                    ),
                )),
            ))
        } else {
            store::state::update_users_channel(&[&input.member], input.id, now_ms);
            Ok((now_ms, None))
        }
    })
}

#[ic_cdk::update(guard = "is_authenticated")]
fn update_member(
    input: types::UpdateChannelMemberInput,
) -> Result<(u64, Option<types::Message>), String> {
    input.validate()?;

    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::channel::manager_with_mut(caller, input.id, |c| {
        if c.managers.contains_key(&input.member) {
            Err("member is a manager".to_string())?;
        }

        let is_new = match c.members.entry(input.member) {
            Entry::Occupied(mut e) => {
                let s = e.get_mut();
                if s.ecdh_pub != input.ecdh.ecdh_pub {
                    Err("ecdh_pub mismatch".to_string())?;
                }
                s.ecdh_remote = input.ecdh.ecdh_remote.clone();
                s.updated_at = now_ms;
                false
            }
            Entry::Vacant(e) => {
                e.insert(ChannelSetting::from_ecdh(input.ecdh, now_ms));
                true
            }
        };

        if c.members.len() > types::MAX_CHANNEL_MEMBERS {
            Err("too many members".to_string())?;
        }

        c.updated_at = now_ms;
        if is_new {
            c.latest_message_id += 1;
            c.latest_message_at = now_ms;
            c.latest_message_by = caller;

            if !store::state::user_add_channel(input.member, input.id, c.latest_message_at) {
                Err("too many channels".to_string())?;
            }
            Ok((
                now_ms,
                Some(store::channel::add_sys_message(
                    caller,
                    now_ms,
                    store::MessageId(input.id, c.latest_message_id),
                    format!(
                        "{}: {}",
                        types::SYS_MSG_CHANNEL_ADD_MEMBER,
                        input.member.to_text()
                    ),
                )),
            ))
        } else {
            store::state::update_users_channel(&[&input.member], input.id, now_ms);
            Ok((now_ms, None))
        }
    })
}

#[ic_cdk::update(guard = "is_authenticated")]
fn remove_member(input: types::UpdateChannelMemberInput) -> Result<(), String> {
    input.validate()?;

    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::channel::remove_member(caller, input.member, input.id, now_ms)?;
    Ok(())
}

#[ic_cdk::update(guard = "is_authenticated")]
fn update_my_setting(input: types::UpdateMySettingInput) -> Result<types::ChannelSetting, String> {
    input.validate()?;

    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::channel::update_my_setting(caller, input, now_ms)
}

#[ic_cdk::update(guard = "is_authenticated")]
fn leave_channel(input: types::UpdateMySettingInput, delete_channel: bool) -> Result<(), String> {
    input.validate()?;

    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::channel::leave(caller, input.id, delete_channel, now_ms)?;
    Ok(())
}

#[ic_cdk::update(guard = "is_authenticated")]
fn add_message(input: types::AddMessageInput) -> Result<types::AddMessageOutput, String> {
    input.validate()?;

    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    let id = store::channel::add_message(
        input.channel,
        store::Message {
            kind: 0,
            reply_to: input.reply_to.unwrap_or_default(),
            created_by: ic_cdk::caller(),
            created_at: now_ms,
            payload: input.payload,
        },
    )?;

    Ok(types::AddMessageOutput {
        id,
        channel: input.channel,
        kind: 0,
        created_at: now_ms,
    })
}

#[ic_cdk::update(guard = "is_authenticated")]
fn delete_message(input: types::DeleteMessageInput) -> Result<(), String> {
    input.validate()?;
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::channel::delete_message(ic_cdk::caller(), input.channel, input.id, now_ms)
}

#[ic_cdk::update(guard = "is_authenticated")]
fn truncate_messages(input: types::TruncateMessageInput) -> Result<(), String> {
    input.validate()?;

    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::channel::truncate_messages(ic_cdk::caller(), input.channel, input.to, now_ms)
}

#[ic_cdk::update]
async fn upload_image_token(
    input: types::UploadFileInput,
) -> Result<types::UploadFileOutput, String> {
    input.validate()?;
    let image = UploadImageInput {
        size: input.size,
        content_type: input.content_type,
    };
    image.validate()?;
    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    // image should not be encrypted
    let custom = MapValue::from([("by_hash".to_string(), "read".into())]);
    store::channel::upload_file_token(
        input.channel,
        caller,
        input.size,
        image.filename(now_ms.to_string()),
        image.content_type,
        Some(custom),
        now_ms,
    )
    .await
}

#[ic_cdk::update]
async fn upload_file_token(
    input: types::UploadFileInput,
) -> Result<types::UploadFileOutput, String> {
    input.validate()?;
    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    // file should be encrypted by the caller with COSE_Encrypt0
    let custom = MapValue::from([("content_type".to_string(), input.content_type.into())]);
    store::channel::upload_file_token(
        input.channel,
        caller,
        input.size,
        format!("{}.cbor", now_ms),
        "application/cbor".to_string(),
        Some(custom),
        now_ms,
    )
    .await
}

#[ic_cdk::update]
async fn download_files_token(channel: u32) -> Result<types::DownloadFilesToken, String> {
    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::channel::download_files_token(channel, caller, now_ms).await
}
