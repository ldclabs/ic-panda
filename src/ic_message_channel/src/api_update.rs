use ic_cose_types::MILLISECONDS;
use std::collections::hash_map::Entry;

use crate::{is_authenticated, store, types};

#[ic_cdk::update(guard = "is_authenticated")]
fn create_channel(input: types::ChannelCreateInput) -> Result<types::ChannelInfo, String> {
    input.validate()?;

    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::state::is_manager(&caller)?;
    store::channel::create(caller, input, now_ms)
}

#[ic_cdk::update(guard = "is_authenticated")]
fn update_channel(input: types::ChannelUpdateInput) -> Result<u64, String> {
    input.validate()?;

    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::channel::manager_with_mut(caller, input.id, |c| {
        if let Some(name) = input.name {
            c.name = name;
        }
        if let Some(description) = input.description {
            c.description = description;
        }
        c.updated_at = now_ms;
        c.last_message_at += 1;
        c.last_message_by = caller;

        store::channel::add_sys_message(
            caller,
            now_ms,
            store::MessageId(input.id, c.last_message_at),
            types::SYS_MSG_CHANNEL_UPDATE_INFO.to_string(),
        );
        Ok(c.updated_at)
    })
}

#[ic_cdk::update(guard = "is_authenticated")]
fn update_manager(input: types::ChannelUpdateMemberInput) -> Result<u64, String> {
    input.validate()?;

    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::channel::manager_with_mut(caller, input.id, |c| {
        let is_new = match c.managers.entry(input.member) {
            Entry::Occupied(mut e) => {
                let s = e.get_mut();
                s.ecdh_pub = input.ecdh.ecdh_pub;
                s.ecdh_remote = input.ecdh.ecdh_remote;
                false
            }
            Entry::Vacant(e) => match c.members.remove(&input.member) {
                Some(mut s) => {
                    s.ecdh_pub = input.ecdh.ecdh_pub;
                    s.ecdh_remote = input.ecdh.ecdh_remote;
                    e.insert(s);
                    true
                }
                None => {
                    e.insert(input.ecdh.into());
                    true
                }
            },
        };

        if c.managers.len() > types::MAX_CHANNEL_MANAGERS {
            Err("too many managers".to_string())?;
        }

        c.updated_at = now_ms;
        if is_new {
            c.last_message_at += 1;
            c.last_message_by = caller;

            store::channel::add_sys_message(
                caller,
                now_ms,
                store::MessageId(input.id, c.last_message_at),
                format!(
                    "{}: {}",
                    types::SYS_MSG_CHANNEL_ADD_MANAGER,
                    input.member.to_text()
                ),
            );
        }
        Ok(c.updated_at)
    })
}

#[ic_cdk::update(guard = "is_authenticated")]
fn update_member(input: types::ChannelUpdateMemberInput) -> Result<u64, String> {
    input.validate()?;

    let caller = ic_cdk::caller();
    let now_ms = ic_cdk::api::time() / MILLISECONDS;
    store::channel::manager_with_mut(caller, input.id, |c| {
        if c.managers.contains_key(&input.member) {
            Err("member is a manager".to_string())?;
        }

        let mut is_new = true;
        c.members
            .entry(input.member)
            .and_modify(|s| {
                s.ecdh_pub = input.ecdh.ecdh_pub;
                s.ecdh_remote = input.ecdh.ecdh_remote.clone();
                is_new = false;
            })
            .or_insert(input.ecdh.into());

        if c.members.len() > types::MAX_CHANNEL_MEMBERS {
            Err("too many members".to_string())?;
        }

        c.updated_at = now_ms;
        if is_new {
            c.last_message_at += 1;
            c.last_message_by = caller;

            store::channel::add_sys_message(
                caller,
                now_ms,
                store::MessageId(input.id, c.last_message_at),
                format!(
                    "{}: {}",
                    types::SYS_MSG_CHANNEL_ADD_MEMBER,
                    input.member.to_text()
                ),
            );
        }
        Ok(c.updated_at)
    })
}

#[ic_cdk::update(guard = "is_authenticated")]
fn update_my_setting(input: types::ChannelUpdateMySettingInput) -> Result<(), String> {
    input.validate()?;

    let caller = ic_cdk::caller();
    store::channel::update_my_setting(caller, input)?;
    Ok(())
}

#[ic_cdk::update(guard = "is_authenticated")]
fn quit_channel(input: types::ChannelUpdateMySettingInput) -> Result<(), String> {
    input.validate()?;

    let caller = ic_cdk::caller();
    store::channel::quit(caller, input.id)?;
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
