use candid::Principal;
use ciborium::{from_reader, from_reader_with_buffer, into_writer};
use ic_cose_types::to_cbor_bytes;
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, Storable,
};
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, ByteBuf};
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap},
};

use crate::types;

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct State {
    pub name: String,
    pub managers: BTreeSet<Principal>,
    pub channel_id: u32,
    pub user_channels: HashMap<Principal, BTreeMap<u32, u32>>,
}

impl Storable for State {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode State data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode State data")
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Channel {
    #[serde(rename = "n")]
    pub name: String,
    #[serde(rename = "d")]
    pub description: String,
    #[serde(rename = "m")]
    pub managers: HashMap<Principal, ChannelSetting>,
    #[serde(rename = "p")]
    pub members: HashMap<Principal, ChannelSetting>,
    #[serde(rename = "k")]
    pub dek: ByteBuf,
    #[serde(rename = "ca")]
    pub created_at: u64,
    #[serde(rename = "cb")]
    pub created_by: Principal,
    #[serde(rename = "ua")]
    pub updated_at: u64,
    #[serde(rename = "la")]
    pub latest_message_at: u32,
    #[serde(rename = "lb")]
    pub latest_message_by: Principal,
    #[serde(rename = "pa")]
    pub paid: u64,
}

impl Channel {
    pub fn into_info(self, caller: Principal, canister: Principal, id: u32) -> types::ChannelInfo {
        let my_setting = if let Some(s) = self.managers.get(&caller) {
            s.to_owned().into()
        } else if let Some(s) = self.members.get(&caller) {
            s.to_owned().into()
        } else {
            types::ChannelSetting {
                last_read: 0,
                unread: 0,
                mute: false,
                ecdh_pub: None,
                ecdh_remote: None,
            }
        };

        types::ChannelInfo {
            id,
            canister,
            name: self.name,
            description: self.description,
            managers: self.managers.into_keys().collect(),
            members: self.members.into_keys().collect(),
            dek: self.dek,
            created_at: self.created_at,
            created_by: self.created_by,
            latest_message_at: self.latest_message_at,
            latest_message_by: self.latest_message_by,
            updated_at: self.updated_at,
            paid: self.paid,
            my_setting,
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct ChannelSetting {
    #[serde(rename = "l")]
    pub last_read: u32, // message id
    #[serde(rename = "u")]
    pub unread: u32, // unread message count
    #[serde(rename = "m")]
    pub mute: bool,
    #[serde(rename = "ep")]
    pub ecdh_pub: Option<ByteArray<32>>,
    #[serde(rename = "er")]
    pub ecdh_remote: Option<(ByteArray<32>, ByteBuf)>,
}

impl From<ChannelSetting> for types::ChannelSetting {
    fn from(s: ChannelSetting) -> Self {
        types::ChannelSetting {
            last_read: s.last_read,
            unread: s.unread,
            mute: s.mute,
            ecdh_pub: s.ecdh_pub,
            ecdh_remote: s.ecdh_remote,
        }
    }
}

impl From<types::ChannelECDHInput> for ChannelSetting {
    fn from(s: types::ChannelECDHInput) -> Self {
        ChannelSetting {
            last_read: 0,
            unread: 0,
            mute: false,
            ecdh_pub: s.ecdh_pub,
            ecdh_remote: s.ecdh_remote,
        }
    }
}

impl Storable for Channel {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode Channel data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode Channel data")
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct Message {
    #[serde(rename = "k")]
    pub kind: u8, // 0: created by user, encrypted, 1: created by system, not encrypted
    #[serde(rename = "r")]
    pub reply_to: u32, // 0 means not a reply
    #[serde(rename = "ca")]
    pub created_at: u64,
    #[serde(rename = "cb")]
    pub created_by: Principal,
    #[serde(rename = "p")]
    pub payload: ByteBuf,
}

impl Message {
    pub fn into_info(self, channel: u32, id: u32) -> types::Message {
        types::Message {
            id,
            channel,
            kind: self.kind,
            reply_to: self.reply_to,
            created_at: self.created_at,
            created_by: self.created_by,
            payload: self.payload,
        }
    }
}

impl Storable for Message {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode Message data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode Message data")
    }
}

// MessageId: (channel id, message id)
#[derive(Clone, Default, Deserialize, Serialize, Ord, PartialOrd, Eq, PartialEq)]
pub struct MessageId(pub u32, pub u32);
impl Storable for MessageId {
    const BOUND: Bound = Bound::Bounded {
        max_size: 11,
        is_fixed_size: false,
    };

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode MessageId data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode MessageId data")
    }
}

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);
const CHANNEL_MEMORY_ID: MemoryId = MemoryId::new(1);
const MESSAGE_MEMORY_ID: MemoryId = MemoryId::new(2);

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());

    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));

    static STATE_STORE: RefCell<StableCell<Vec<u8>, Memory>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(STATE_MEMORY_ID)),
            Vec::new()
        ).expect("failed to init STATE store")
    );

    static CHANNEL_STORE: RefCell<StableBTreeMap<u32, Channel, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(CHANNEL_MEMORY_ID)),
        )
    );

    static MESSAGE_STORE: RefCell<StableBTreeMap<MessageId, Message, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(MESSAGE_MEMORY_ID)),
        )
    );
}

pub mod state {
    use super::*;

    pub fn with<R>(f: impl FnOnce(&State) -> R) -> R {
        STATE.with(|r| f(&r.borrow()))
    }

    pub fn with_mut<R>(f: impl FnOnce(&mut State) -> R) -> R {
        STATE.with(|r| f(&mut r.borrow_mut()))
    }

    pub fn is_manager(caller: &Principal) -> Result<(), String> {
        STATE.with(|r| match r.borrow().managers.contains(caller) {
            true => Ok(()),
            false => Err("caller is not a manager".to_string()),
        })
    }

    pub fn user_add_channel(user: Principal, id: u32, mid: u32) -> bool {
        with_mut(|s| {
            let map = s.user_channels.entry(user).or_default();
            if map.len() >= types::MAX_USER_CHANNELS && !map.contains_key(&id) {
                false
            } else {
                map.insert(id, mid);
                true
            }
        })
    }

    pub fn user_channels(user: &Principal) -> BTreeMap<u32, u32> {
        with(|s| s.user_channels.get(user).cloned().unwrap_or_default())
    }

    pub fn load() {
        let mut scratch = [0; 4096];
        STATE_STORE.with(|r| {
            STATE.with(|h| {
                let v: State = from_reader_with_buffer(&r.borrow().get()[..], &mut scratch)
                    .expect("failed to decode STATE_STORE data");
                *h.borrow_mut() = v;
            });
        });
    }

    pub fn save() {
        STATE.with(|h| {
            STATE_STORE.with(|r| {
                let mut buf = vec![];
                into_writer(&(*h.borrow()), &mut buf).expect("failed to encode STATE_STORE data");
                r.borrow_mut()
                    .set(buf)
                    .expect("failed to set STATE_STORE data");
            });
        });
    }
}

pub mod channel {
    use super::*;

    pub fn channels_total() -> u64 {
        CHANNEL_STORE.with(|r| r.borrow().len())
    }

    pub fn messages_total() -> u64 {
        MESSAGE_STORE.with(|r| r.borrow().len())
    }

    pub fn manager_with_mut<R>(
        caller: Principal,
        id: u32,
        f: impl FnOnce(&mut Channel) -> Result<R, String>,
    ) -> Result<R, String> {
        CHANNEL_STORE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&id) {
                None => Err("channel not found".to_string()),
                Some(mut v) => {
                    if !v.managers.contains_key(&caller) {
                        Err("caller is not a manager".to_string())?;
                    }
                    match f(&mut v) {
                        Err(err) => Err(err),
                        Ok(res) => {
                            m.insert(id, v);
                            Ok(res)
                        }
                    }
                }
            }
        })
    }

    pub fn add_sys_message(caller: Principal, now_ms: u64, mid: MessageId, message: String) {
        MESSAGE_STORE.with(|r| {
            r.borrow_mut().insert(
                mid,
                Message {
                    kind: 1,
                    reply_to: 0,
                    created_at: now_ms,
                    created_by: caller,
                    payload: to_cbor_bytes(&message).into(),
                },
            );
        });
    }

    pub fn create(
        caller: Principal,
        input: types::CreateChannelInput,
        now_ms: u64,
    ) -> Result<types::ChannelInfo, String> {
        let id = state::with_mut(|s| {
            s.channel_id = s.channel_id.saturating_add(1);
            s.channel_id
        });

        add_sys_message(
            caller,
            now_ms,
            MessageId(id, 1),
            format!(
                "{}: Channel {} created by {}",
                types::SYS_MSG_CHANNEL_CREATE,
                input.name,
                input.created_by.to_text()
            ),
        );

        CHANNEL_STORE.with(|r| {
            let channel = Channel {
                name: input.name,
                description: input.description,
                managers: input
                    .managers
                    .into_iter()
                    .map(|p| (p.0, p.1.into()))
                    .collect(),
                members: HashMap::new(),
                dek: input.dek,
                created_at: now_ms,
                created_by: input.created_by,
                latest_message_at: 1,
                latest_message_by: caller,
                paid: input.paid,
                updated_at: now_ms,
            };

            r.borrow_mut().insert(id, channel.clone());
            Ok(channel.into_info(input.created_by, ic_cdk::id(), id))
        })
    }

    pub fn update_my_setting(
        caller: Principal,
        input: types::UpdateMySettingInput,
    ) -> Result<(), String> {
        CHANNEL_STORE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&input.id) {
                None => Err("channel not found".to_string()),
                Some(mut v) => {
                    let setting = match v.members.get_mut(&caller) {
                        Some(s) => s,
                        None => match v.managers.get_mut(&caller) {
                            Some(s) => s,
                            None => Err("caller is not a manager or member".to_string())?,
                        },
                    };

                    if let Some(last_read) = input.last_read {
                        if last_read > setting.last_read {
                            setting.last_read = last_read;
                            setting.unread = 0;
                        }
                    }
                    if let Some(mute) = input.mute {
                        setting.mute = mute;
                    }
                    if let Some(ecdh) = input.ecdh {
                        setting.ecdh_pub = ecdh.ecdh_pub;
                        setting.ecdh_remote = ecdh.ecdh_remote;
                    }
                    m.insert(input.id, v);
                    Ok(())
                }
            }
        })
    }

    pub fn remove_member(caller: Principal, member: Principal, id: u32) -> Result<(), String> {
        CHANNEL_STORE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&id) {
                None => Err("channel not found".to_string()),
                Some(mut v) => {
                    if !v.managers.contains_key(&caller) {
                        Err("caller is not a manager".to_string())?;
                    }

                    v.members.remove(&member);
                    state::with_mut(|s| {
                        if let Some(channels) = s.user_channels.get_mut(&member) {
                            channels.remove(&id);
                        }
                    });

                    m.insert(id, v);
                    Ok(())
                }
            }
        })
    }

    pub fn quit(caller: Principal, id: u32, delete_channel: bool) -> Result<(), String> {
        CHANNEL_STORE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&id) {
                None => Err("channel not found".to_string()),
                Some(mut v) => {
                    v.members.remove(&caller);
                    v.managers.remove(&caller);
                    if v.managers.is_empty() && !delete_channel {
                        Err("no managers".to_string())?;
                    }

                    state::with_mut(|s| {
                        if let Some(channels) = s.user_channels.get_mut(&caller) {
                            channels.remove(&id);
                        }
                    });

                    // remove channel if no managers
                    if v.managers.is_empty() {
                        m.remove(&id);
                        state::with_mut(|s| {
                            for u in v.members.keys() {
                                if let Some(channels) = s.user_channels.get_mut(u) {
                                    channels.remove(&id);
                                }
                            }
                        });

                        MESSAGE_STORE.with(|r| {
                            let mut messages = r.borrow_mut();
                            for i in 1..v.latest_message_at + 1 {
                                messages.remove(&MessageId(id, i));
                            }
                        });
                    } else {
                        m.insert(id, v);
                    }
                    Ok(())
                }
            }
        })
    }

    pub fn add_message(id: u32, msg: Message) -> Result<u32, String> {
        CHANNEL_STORE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&id) {
                None => Err("channel not found".to_string()),
                Some(mut v) => {
                    if !v.managers.contains_key(&msg.created_by)
                        && !v.members.contains_key(&msg.created_by)
                    {
                        Err("caller is not a manager or member".to_string())?;
                    }

                    if v.latest_message_at >= types::MAX_CHANNEL_MESSAGES {
                        Err("too many messages".to_string())?;
                    }

                    v.latest_message_at += 1;
                    v.latest_message_by = msg.created_by;
                    let mid = v.latest_message_at;
                    state::with_mut(|s| {
                        for (p, c) in v.managers.iter_mut() {
                            if p != &msg.created_by {
                                c.unread += 1;
                            }
                            if let Some(channels) = s.user_channels.get_mut(p) {
                                channels.insert(id, mid);
                            }
                        }
                        for (p, c) in v.members.iter_mut() {
                            if p != &msg.created_by {
                                c.unread += 1;
                            }
                            if let Some(channels) = s.user_channels.get_mut(p) {
                                channels.insert(id, mid);
                            }
                        }
                    });
                    m.insert(id, v);
                    MESSAGE_STORE.with(|r| r.borrow_mut().insert(MessageId(id, mid), msg));
                    Ok(mid)
                }
            }
        })
    }

    pub fn get_if_update(
        caller: Principal,
        id: u32,
        updated_at: u64,
    ) -> Result<Option<types::ChannelInfo>, String> {
        CHANNEL_STORE.with(|r| match r.borrow().get(&id) {
            None => Err("channel not found".to_string()),
            Some(v) => {
                if !v.managers.contains_key(&caller) && !v.members.contains_key(&caller) {
                    Err("caller is not a manager or member".to_string())?;
                }

                if v.updated_at > updated_at {
                    Ok(Some(v.into_info(caller, ic_cdk::id(), id)))
                } else {
                    Ok(None)
                }
            }
        })
    }

    pub fn batch_get(caller: Principal, ids: BTreeSet<u32>) -> Vec<types::ChannelBasicInfo> {
        CHANNEL_STORE.with(|r| {
            let m = r.borrow();
            let mut output = Vec::with_capacity(ids.len());
            for id in ids {
                if let Some(v) = m.get(&id) {
                    let my_setting = if let Some(s) = v.managers.get(&caller) {
                        Some(s)
                    } else {
                        v.members.get(&caller)
                    };

                    if let Some(my_setting) = my_setting {
                        output.push(types::ChannelBasicInfo {
                            id,
                            name: v.name.clone(),
                            updated_at: v.updated_at,
                            latest_message_at: v.latest_message_at,
                            latest_message_by: v.latest_message_by,
                            paid: v.paid,
                            my_setting: my_setting.to_owned().into(),
                        });
                    }
                }
            }
            output
        })
    }

    pub fn get_message(caller: Principal, channel: u32, id: u32) -> Result<types::Message, String> {
        CHANNEL_STORE.with(|r| match r.borrow().get(&channel) {
            None => Err("channel not found".to_string()),
            Some(v) => {
                if !v.managers.contains_key(&caller) && !v.members.contains_key(&caller) {
                    Err("caller is not a manager or member".to_string())?;
                }

                MESSAGE_STORE.with(|r| {
                    r.borrow()
                        .get(&MessageId(channel, id))
                        .map(|msg| msg.into_info(channel, id))
                        .ok_or("message not found".to_string())
                })
            }
        })
    }

    pub fn list_messages(
        caller: Principal,
        channel: u32,
        prev: u32,
        take: u32,
        util: u32,
    ) -> Result<Vec<types::Message>, String> {
        CHANNEL_STORE.with(|r| match r.borrow().get(&channel) {
            None => Err("channel not found".to_string()),
            Some(v) => {
                if !v.managers.contains_key(&caller) && !v.members.contains_key(&caller) {
                    Err("caller is not a manager or member".to_string())?;
                }

                let end = if prev == 0 || prev > v.latest_message_at {
                    v.latest_message_at + 1
                } else {
                    prev
                };
                let start = if util == 0 || util >= prev { 1 } else { util };
                MESSAGE_STORE.with(|r| {
                    let m = r.borrow();
                    let mut output = Vec::with_capacity(take as usize);
                    for i in (start..end).rev() {
                        if output.len() >= take as usize {
                            break;
                        }
                        if let Some(msg) = m.get(&MessageId(channel, i)) {
                            output.push(msg.into_info(channel, i));
                        }
                    }
                    Ok(output)
                })
            }
        })
    }
}
