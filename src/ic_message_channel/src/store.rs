use candid::Principal;
use ciborium::{from_reader, from_reader_with_buffer, into_writer};
use ic_cose_types::to_cbor_bytes;
use ic_oss_types::{
    cose::Token,
    file::{CreateFileInput, CreateFileOutput},
    folder::{CreateFolderInput, CreateFolderOutput},
    MapValue,
};
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

use crate::{call, types};

const MESSAGE_PER_USER_GAS: u64 = 10000;
const MESSAGE_PER_BYTE_GAS: u64 = 1000;
const FREE_GAS: u64 = 100_000_000;
const UPLOAD_FILE_GAS_THRESHOLD: u64 = 10_000_000;

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct State {
    pub name: String,
    pub managers: BTreeSet<Principal>,
    pub channel_id: u32,
    pub user_channels: HashMap<Principal, BTreeMap<u32, u64>>,
    #[serde(default)]
    pub incoming_gas: u128,
    #[serde(default)]
    pub burned_gas: u128,
    #[serde(default)]
    pub ic_oss_cluster: Option<Principal>,
    #[serde(default)]
    pub ic_oss_buckets: Vec<Principal>,
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
    #[serde(rename = "i")]
    pub image: String,
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
    #[serde(rename = "ms")]
    pub message_start: u32,
    #[serde(rename = "li")]
    pub latest_message_id: u32,
    #[serde(rename = "la")]
    pub latest_message_at: u64,
    #[serde(rename = "lb")]
    pub latest_message_by: Principal,
    #[serde(rename = "pa")]
    pub paid: u64,
    #[serde(rename = "g")]
    pub gas: u64,
    #[serde(default, rename = "dm")]
    pub deleted_messages: BTreeSet<u32>,
    #[serde(default, rename = "fs")]
    pub file_storage: Option<(Principal, u32)>, //  (ic-oss-bucket canister, folder_id)
    #[serde(default, rename = "fms")]
    pub file_max_size: u64,
    #[serde(default, rename = "ft")]
    pub files_total: u64,
    #[serde(default, rename = "fst")]
    pub files_size_total: u64,
}

impl Channel {
    pub fn into_info(self, caller: Principal, canister: Principal, id: u32) -> types::ChannelInfo {
        let (my_setting, is_manager) = if let Some(s) = self.managers.get(&caller) {
            (s.to_owned().into(), true)
        } else if let Some(s) = self.members.get(&caller) {
            (s.to_owned().into(), false)
        } else {
            (
                types::ChannelSetting {
                    last_read: 0,
                    unread: 0,
                    mute: false,
                    ecdh_pub: None,
                    ecdh_remote: None,
                    updated_at: 0,
                },
                false,
            )
        };
        let mut ecdh_request: HashMap<
            Principal,
            (ByteArray<32>, Option<(ByteArray<32>, ByteBuf)>),
        > = HashMap::new();
        if is_manager {
            for (p, s) in self.managers.iter() {
                if s.ecdh_pub.is_some() {
                    ecdh_request.insert(*p, (s.ecdh_pub.unwrap(), s.ecdh_remote.clone()));
                }
            }
            for (p, s) in self.members.iter() {
                if s.ecdh_pub.is_some() {
                    ecdh_request.insert(*p, (s.ecdh_pub.unwrap(), s.ecdh_remote.clone()));
                }
            }
        }

        types::ChannelInfo {
            id,
            canister,
            name: self.name,
            image: self.image,
            description: self.description,
            managers: self.managers.into_keys().collect(),
            members: self.members.into_keys().collect(),
            dek: self.dek,
            created_at: self.created_at,
            created_by: self.created_by,
            message_start: self.message_start,
            latest_message_id: self.latest_message_id,
            latest_message_at: self.latest_message_at,
            latest_message_by: self.latest_message_by,
            updated_at: self.updated_at,
            paid: self.paid,
            gas: self.gas,
            deleted_messages: self.deleted_messages,
            my_setting,
            ecdh_request,
            files_state: if let Some(storage) = self.file_storage {
                Some(types::ChannelFilesState {
                    file_storage: storage,
                    file_max_size: self.file_max_size,
                    files_total: self.files_total,
                    files_size_total: self.files_size_total,
                })
            } else {
                None
            },
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
    #[serde(default, rename = "ua")]
    pub updated_at: u64,
}

impl From<ChannelSetting> for types::ChannelSetting {
    fn from(s: ChannelSetting) -> Self {
        types::ChannelSetting {
            last_read: s.last_read,
            unread: s.unread,
            mute: s.mute,
            ecdh_pub: s.ecdh_pub,
            ecdh_remote: s.ecdh_remote,
            updated_at: s.updated_at,
        }
    }
}

impl ChannelSetting {
    pub fn from_ecdh(s: types::ChannelECDHInput, now_ms: u64) -> Self {
        ChannelSetting {
            last_read: 0,
            unread: 0,
            mute: false,
            ecdh_pub: s.ecdh_pub,
            ecdh_remote: s.ecdh_remote,
            updated_at: now_ms,
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
    pub fn into_info(self, id: u32) -> types::Message {
        types::Message {
            id,
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

    pub fn user_add_channel(user: Principal, id: u32, updated_at: u64) -> bool {
        with_mut(|s| {
            let map = s.user_channels.entry(user).or_default();
            if map.len() >= types::MAX_USER_CHANNELS && !map.contains_key(&id) {
                false
            } else {
                map.insert(id, updated_at);
                true
            }
        })
    }

    pub fn update_users_channel(users: &[&Principal], id: u32, updated_at: u64) {
        with_mut(|s| {
            for p in users {
                if let Some(channels) = s.user_channels.get_mut(p) {
                    channels.insert(id, updated_at);
                }
            }
        });
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
    use crate::MINTER_CANISTER;

    pub fn channels_total() -> u64 {
        CHANNEL_STORE.with(|r| r.borrow().len())
    }

    pub fn messages_total() -> u64 {
        MESSAGE_STORE.with(|r| r.borrow().len())
    }

    pub fn manager_with<R>(
        caller: Principal,
        id: u32,
        f: impl FnOnce(Channel) -> Result<R, String>,
    ) -> Result<R, String> {
        CHANNEL_STORE.with(|r| match r.borrow().get(&id) {
            None => Err("channel not found".to_string()),
            Some(v) => {
                if !v.managers.contains_key(&caller) {
                    Err("caller is not a manager".to_string())?;
                }
                f(v)
            }
        })
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

    pub fn add_sys_message(
        caller: Principal,
        now_ms: u64,
        mid: MessageId,
        message: String,
    ) -> types::Message {
        if mid.1 == u32::MAX {
            ic_cdk::trap("message id overflow");
        }
        let message = Message {
            kind: 1,
            reply_to: 0,
            created_at: now_ms,
            created_by: caller,
            payload: to_cbor_bytes(&message).into(),
        };
        let info = message.clone().into_info(mid.1);
        MESSAGE_STORE.with(|r| {
            r.borrow_mut().insert(mid, message);
        });
        info
    }

    pub fn create(
        caller: Principal,
        input: types::CreateChannelInput,
        now_ms: u64,
    ) -> Result<types::ChannelInfo, String> {
        let id = state::with_mut(|s| {
            if input.paid > 0 {
                s.incoming_gas = s.incoming_gas.saturating_add(input.paid as u128);
            }
            s.channel_id = s.channel_id.saturating_add(1);
            for p in input.managers.keys() {
                s.user_channels
                    .entry(*p)
                    .or_default()
                    .insert(s.channel_id, now_ms);
            }

            s.channel_id
        });

        if id == u32::MAX {
            ic_cdk::trap("channel id overflow");
        }

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
                image: input.image,
                description: input.description,
                managers: input
                    .managers
                    .into_iter()
                    .map(|p| (p.0, ChannelSetting::from_ecdh(p.1, now_ms)))
                    .collect(),
                members: HashMap::new(),
                dek: input.dek,
                created_at: now_ms,
                created_by: input.created_by,
                message_start: 1,
                latest_message_id: 1,
                latest_message_at: now_ms,
                latest_message_by: caller,
                paid: input.paid,
                gas: input.paid.max(FREE_GAS),
                updated_at: now_ms,
                deleted_messages: BTreeSet::new(),
                file_storage: None,
                file_max_size: 0,
                files_total: 0,
                files_size_total: 0,
            };

            r.borrow_mut().insert(id, channel.clone());
            Ok(channel.into_info(input.created_by, ic_cdk::id(), id))
        })
    }

    pub fn topup(
        payer: Principal,
        id: u32,
        amount: u64,
        now_ms: u64,
    ) -> Result<types::ChannelInfo, String> {
        CHANNEL_STORE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&id) {
                None => Err("channel not found".to_string()),
                Some(mut c) => {
                    if !c.managers.contains_key(&payer) && !c.members.contains_key(&payer) {
                        Err("caller is not a manager or member".to_string())?;
                    }

                    let gas = c.gas.saturating_add(amount);
                    c.gas = gas;
                    c.latest_message_id += 1;
                    c.latest_message_at = now_ms;
                    c.latest_message_by = payer;
                    c.paid = c.paid.saturating_add(amount);
                    add_sys_message(
                        payer,
                        now_ms,
                        MessageId(id, c.latest_message_id),
                        format!("{}: {}", types::SYS_MSG_CHANNEL_TOPUP, amount),
                    );
                    state::with_mut(|s| {
                        s.incoming_gas = s.incoming_gas.saturating_add(amount as u128);
                    });
                    m.insert(id, c.clone());
                    Ok(c.into_info(payer, ic_cdk::id(), id))
                }
            }
        })
    }

    pub async fn update_my_setting(
        caller: Principal,
        input: types::UpdateMySettingInput,
        now_ms: u64,
    ) -> Result<types::ChannelSetting, String> {
        let (try_mint_payer, setting) = CHANNEL_STORE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&input.id) {
                None => Err("channel not found".to_string()),
                Some(mut v) => {
                    let (setting, is_manager) = match v.members.get_mut(&caller) {
                        Some(s) => (s, false),
                        None => match v.managers.get_mut(&caller) {
                            Some(s) => (s, true),
                            None => Err("caller is not a manager or member".to_string())?,
                        },
                    };

                    if let Some(last_read) = input.last_read {
                        if last_read > v.latest_message_id {
                            Err("last_read too large".to_string())?;
                        }
                        if last_read == v.latest_message_id {
                            setting.unread = 0;
                        } else if last_read > setting.last_read {
                            setting.unread =
                                setting.unread.saturating_sub(last_read - setting.last_read);
                        }
                        setting.last_read = last_read;
                    }
                    if let Some(mute) = input.mute {
                        setting.mute = mute;
                    }

                    let mut try_mint_payer: Option<Principal> = None;
                    if let Some(ecdh) = input.ecdh {
                        setting.ecdh_pub = ecdh.ecdh_pub;
                        setting.ecdh_remote = ecdh.ecdh_remote;
                        // It maybe a miner accepted ECDH key
                        if is_manager
                            && caller != v.created_by
                            && setting.ecdh_pub.is_none()
                            && setting.ecdh_remote.is_none()
                        {
                            try_mint_payer = Some(v.created_by);
                        }
                    }
                    setting.updated_at = now_ms;

                    let rt = setting.to_owned().into();
                    state::update_users_channel(&[&caller], input.id, now_ms);
                    m.insert(input.id, v);
                    Ok((try_mint_payer, rt))
                }
            }
        })?;

        if let Some(payer) = try_mint_payer {
            let _: Result<Option<u64>, String> =
                call(MINTER_CANISTER, "try_commit", (caller, payer), 0).await;
        }

        Ok(setting)
    }

    pub fn remove_member(
        caller: Principal,
        member: Principal,
        id: u32,
        now_ms: u64,
    ) -> Result<(), String> {
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
                            if channels.is_empty() {
                                s.user_channels.remove(&member);
                            }
                        }
                    });

                    v.updated_at = now_ms;
                    m.insert(id, v);
                    Ok(())
                }
            }
        })
    }

    pub async fn leave(
        caller: Principal,
        id: u32,
        delete_channel: bool,
        now_ms: u64,
    ) -> Result<(), String> {
        let file_storage = CHANNEL_STORE.with(|r| {
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
                            if channels.is_empty() {
                                s.user_channels.remove(&caller);
                            }
                        }
                    });

                    // remove channel if no managers
                    if v.managers.is_empty() {
                        m.remove(&id);
                        state::with_mut(|s| {
                            for u in v.members.keys() {
                                if let Some(channels) = s.user_channels.get_mut(u) {
                                    channels.remove(&id);
                                    if channels.is_empty() {
                                        s.user_channels.remove(u);
                                    }
                                }
                            }
                        });

                        MESSAGE_STORE.with(|r| {
                            let mut messages = r.borrow_mut();
                            for i in v.message_start..=v.latest_message_id {
                                messages.remove(&MessageId(id, i));
                            }
                        });
                        // remove file storage
                        Ok(v.file_storage)
                    } else {
                        v.updated_at = now_ms;
                        m.insert(id, v);
                        Ok(None)
                    }
                }
            }
        })?;

        if let Some((ic_oss_bucket, folder_id)) = file_storage {
            let ic_oss_cluster = state::with(|s| s.ic_oss_cluster);
            let ic_oss_cluster =
                ic_oss_cluster.ok_or_else(|| "ic_oss_cluster not set".to_string())?;
            let self_id = ic_cdk::id();
            let token: Result<ByteBuf, String> = call(
                ic_oss_cluster,
                "admin_weak_access_token",
                (
                    Token {
                        subject: self_id,
                        audience: ic_oss_bucket,
                        policies: "Bucket.Delete.Folder".to_string(),
                    },
                    now_ms / 1000,
                    60 * 10_u64,
                ),
                0,
            )
            .await?;
            let token = token?;

            let _: Result<bool, String> =
                call(ic_oss_bucket, "delete_folder", (folder_id, Some(token)), 0).await?;
        }
        Ok(())
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

                    if v.latest_message_id + 1 - v.message_start >= types::MAX_CHANNEL_MESSAGES {
                        Err("too many messages".to_string())?;
                    }

                    v.latest_message_id += 1;
                    if v.latest_message_id == u32::MAX {
                        Err("message id overflow".to_string())?;
                    }
                    let gas = MESSAGE_PER_USER_GAS * (v.managers.len() + v.members.len()) as u64
                        + MESSAGE_PER_BYTE_GAS * msg.payload.len() as u64;
                    if v.gas < gas {
                        Err("insufficient gas balance".to_string())?;
                    }
                    v.gas = v.gas.saturating_sub(gas);
                    v.latest_message_by = msg.created_by;
                    v.latest_message_at = msg.created_at;
                    let at = v.latest_message_at;
                    let mid = v.latest_message_id;
                    state::with_mut(|s| {
                        s.burned_gas = s.burned_gas.saturating_add(gas as u128);

                        for (p, c) in v.managers.iter_mut() {
                            if p != &msg.created_by {
                                c.unread += 1;
                            }
                            s.user_channels.entry(*p).or_default().insert(id, at);
                        }
                        for (p, c) in v.members.iter_mut() {
                            if p != &msg.created_by {
                                c.unread += 1;
                            }
                            s.user_channels.entry(*p).or_default().insert(id, at);
                        }
                    });
                    m.insert(id, v);
                    MESSAGE_STORE.with(|r| r.borrow_mut().insert(MessageId(id, mid), msg));
                    Ok(mid)
                }
            }
        })
    }

    pub async fn update_storage(
        id: u32,
        caller: Principal,
        file_max_size: u64,
        now_ms: u64,
    ) -> Result<types::Message, String> {
        let self_id = ic_cdk::id();
        let (ic_oss_cluster, ic_oss_bucket) =
            state::with(|s| (s.ic_oss_cluster, s.ic_oss_buckets.last().cloned()));
        let ic_oss_cluster = ic_oss_cluster.ok_or_else(|| "ic_oss_cluster not set".to_string())?;
        let ic_oss_bucket = ic_oss_bucket.ok_or_else(|| "ic_oss_cluster not set".to_string())?;

        let file_storage = manager_with(caller, id, |c| Ok(c.file_storage))?;
        let file_storage = if let Some(f) = file_storage {
            f
        } else {
            let token: Result<ByteBuf, String> = call(
                ic_oss_cluster,
                "admin_weak_access_token",
                (
                    Token {
                        subject: self_id,
                        audience: ic_oss_bucket,
                        policies: "Bucket.Write.Folder".to_string(),
                    },
                    now_ms / 1000,
                    60 * 10_u64,
                ),
                0,
            )
            .await?;
            let token = token?;
            let res: Result<CreateFolderOutput, String> = call(
                ic_oss_bucket,
                "create_folder",
                (
                    CreateFolderInput {
                        parent: 0,
                        name: format!("{}:{}", self_id.to_text(), id),
                    },
                    Some(token),
                ),
                0,
            )
            .await?;
            let res = res?;
            (ic_oss_bucket, res.id)
        };

        let msg = manager_with_mut(caller, id, |c| {
            if c.file_storage.is_none() {
                c.file_storage = Some(file_storage);
            }
            c.file_max_size = file_max_size;
            c.updated_at = now_ms;
            c.latest_message_id += 1;
            c.latest_message_at = now_ms;
            c.latest_message_by = caller;

            Ok(add_sys_message(
                caller,
                now_ms,
                MessageId(id, c.latest_message_id),
                format!(
                    "{}: file storage enabled, max file size {} bytes",
                    types::SYS_MSG_CHANNEL_UPDATE_INFO,
                    file_max_size
                ),
            ))
        })?;
        Ok(msg)
    }

    pub async fn upload_file_token(
        id: u32,
        caller: Principal,
        file_size: u64,
        file_name: String,
        content_type: String,
        custom: MapValue,
        now_ms: u64,
    ) -> Result<types::UploadFileOutput, String> {
        let self_id = ic_cdk::id();
        let ic_oss_cluster = state::with(|s| s.ic_oss_cluster);
        let ic_oss_cluster = ic_oss_cluster.ok_or_else(|| "ic_oss_cluster not set".to_string())?;

        let (channel, file_storage, gas) = CHANNEL_STORE.with(|r| match r.borrow().get(&id) {
            None => Err("channel not found".to_string()),
            Some(v) => {
                if !v.managers.contains_key(&caller) && !v.members.contains_key(&caller) {
                    Err("caller is not a manager or member".to_string())?;
                }
                let file_storage = match v.file_storage {
                    Some(f) => f,
                    None => Err("file storage not enabled".to_string())?,
                };
                if file_size > v.file_max_size {
                    Err("file size too large".to_string())?;
                }
                if v.latest_message_id + 1 - v.message_start >= types::MAX_CHANNEL_MESSAGES {
                    Err("too many messages".to_string())?;
                }

                let gas = MESSAGE_PER_USER_GAS * (v.managers.len() + v.members.len()) as u64
                    + MESSAGE_PER_BYTE_GAS * file_size;
                if v.gas < gas + UPLOAD_FILE_GAS_THRESHOLD {
                    Err("insufficient gas balance".to_string())?;
                }
                Ok((v, file_storage, gas))
            }
        })?;

        let token: Result<ByteBuf, String> = call(
            ic_oss_cluster,
            "admin_weak_access_token",
            (
                Token {
                    subject: self_id,
                    audience: file_storage.0,
                    policies: "Bucket.Write.File".to_string(),
                },
                now_ms / 1000,
                10 * 60_u64,
            ),
            0,
        )
        .await?;
        let token = token?;

        let res: Result<CreateFileOutput, String> = call(
            file_storage.0,
            "create_file",
            (
                CreateFileInput {
                    parent: file_storage.1,
                    name: file_name.clone(),
                    size: Some(file_size),
                    content_type,
                    custom: Some(custom),
                    ..Default::default()
                },
                Some(token),
            ),
            0,
        )
        .await?;
        let res = res?;

        let token: Result<ByteBuf, String> = call(
            ic_oss_cluster,
            "admin_weak_access_token",
            (
                Token {
                    subject: caller,
                    audience: file_storage.0,
                    policies: format!("File.Write:{}", res.id),
                },
                now_ms / 1000,
                30 * 60_u64, // 30 minutes
            ),
            0,
        )
        .await?;

        CHANNEL_STORE.with(|r| {
            let mut m = r.borrow_mut();
            let mut v = channel;
            v.gas = v.gas.saturating_sub(gas);
            v.latest_message_id += 1;
            v.latest_message_by = caller;
            v.latest_message_at = now_ms;
            v.files_size_total += file_size;
            v.files_total += 1;
            add_sys_message(
                caller,
                now_ms,
                MessageId(id, v.latest_message_id),
                format!(
                    "{}: file {}, {} bytes, created by {}",
                    types::SYS_MSG_CHANNEL_UPLOAD_FILE,
                    res.id,
                    file_size,
                    caller.to_text(),
                ),
            );
            m.insert(id, v);
        });
        Ok(types::UploadFileOutput {
            id: res.id,
            storage: file_storage,
            name: file_name,
            access_token: token?,
        })
    }

    pub async fn download_files_token(
        id: u32,
        caller: Principal,
        now_ms: u64,
    ) -> Result<types::DownloadFilesToken, String> {
        let ic_oss_cluster = state::with(|s| s.ic_oss_cluster);
        let ic_oss_cluster = ic_oss_cluster.ok_or_else(|| "ic_oss_cluster not set".to_string())?;

        let file_storage = CHANNEL_STORE.with(|r| match r.borrow().get(&id) {
            None => Err("channel not found".to_string()),
            Some(v) => {
                if !v.managers.contains_key(&caller) && !v.members.contains_key(&caller) {
                    Err("caller is not a manager or member".to_string())?;
                }
                match v.file_storage {
                    Some(f) => Ok(f),
                    None => Err("file storage not enabled".to_string())?,
                }
            }
        })?;

        let token: Result<ByteBuf, String> = call(
            ic_oss_cluster,
            "admin_weak_access_token",
            (
                Token {
                    subject: caller,
                    audience: file_storage.0,
                    policies: format!("Folder.Read:{}", file_storage.1),
                },
                now_ms / 1000,
                48 * 3600_u64, // 2 days
            ),
            0,
        )
        .await?;
        Ok(types::DownloadFilesToken {
            storage: file_storage,
            access_token: token?,
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
                let my_setting = if let Some(s) = v.managers.get(&caller) {
                    Some(s)
                } else {
                    v.members.get(&caller)
                };
                let my_setting = match my_setting {
                    Some(s) => s,
                    None => Err("caller is not a manager or member".to_string())?,
                };

                if v.updated_at > updated_at || my_setting.updated_at > updated_at {
                    Ok(Some(v.into_info(caller, ic_cdk::id(), id)))
                } else {
                    Ok(None)
                }
            }
        })
    }

    pub fn batch_get(caller: Principal, ids: BTreeSet<u32>) -> Vec<types::ChannelBasicInfo> {
        let canister = ic_cdk::id();
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
                            canister,
                            name: v.name.clone(),
                            image: v.image.clone(),
                            updated_at: v.updated_at,
                            latest_message_id: v.latest_message_id,
                            latest_message_at: v.latest_message_at,
                            latest_message_by: v.latest_message_by,
                            paid: v.paid,
                            gas: v.gas,
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
                        .map(|msg| msg.into_info(id))
                        .ok_or("message not found".to_string())
                })
            }
        })
    }

    pub fn list_messages(
        caller: Principal,
        channel: u32,
        start: u32,
        end: u32,
    ) -> Result<Vec<types::Message>, String> {
        CHANNEL_STORE.with(|r| match r.borrow().get(&channel) {
            None => Err("channel not found".to_string()),
            Some(v) => {
                if !v.managers.contains_key(&caller) && !v.members.contains_key(&caller) {
                    Err("caller is not a manager or member".to_string())?;
                }

                let start = if start < v.message_start {
                    v.message_start
                } else if start > v.latest_message_id {
                    v.latest_message_id + 1
                } else {
                    start
                };

                let end = if end <= start || end > v.latest_message_id {
                    v.latest_message_id + 1
                } else {
                    end
                };
                let start = if end - start > 100 { end - 100 } else { start };

                MESSAGE_STORE.with(|r| {
                    let m = r.borrow();
                    let mut output = Vec::with_capacity((end - start) as usize);
                    for i in start..end {
                        match m.get(&MessageId(channel, i)) {
                            Some(msg) => output.push(msg.into_info(i)),
                            None => break,
                        }
                    }
                    Ok(output)
                })
            }
        })
    }

    pub fn delete_message(
        caller: Principal,
        channel: u32,
        id: u32,
        now_ms: u64,
    ) -> Result<(), String> {
        CHANNEL_STORE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&channel) {
                None => Err("channel not found".to_string()),
                Some(mut v) => {
                    if !v.managers.contains_key(&caller) && !v.members.contains_key(&caller) {
                        Err("caller is not a manager or member".to_string())?;
                    }

                    MESSAGE_STORE.with(|rr| {
                        let mut mm = rr.borrow_mut();
                        match mm.get(&MessageId(channel, id)) {
                            None => Err("message not found".to_string()),
                            Some(mut msg) => {
                                if msg.created_by != caller {
                                    Err("caller is not the creator".to_string())?;
                                }
                                if msg.kind == 1 {
                                    Err("system message cannot be deleted".to_string())?;
                                }

                                msg.payload.clear();
                                mm.insert(MessageId(channel, id), msg);
                                v.updated_at = now_ms;
                                v.deleted_messages.insert(id);
                                m.insert(channel, v);
                                Ok(())
                            }
                        }
                    })
                }
            }
        })
    }

    pub fn truncate_messages(
        caller: Principal,
        channel: u32,
        to: u32,
        now_ms: u64,
    ) -> Result<(), String> {
        let message_start = CHANNEL_STORE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&channel) {
                None => Err("channel not found".to_string()),
                Some(mut v) => {
                    if !v.managers.contains_key(&caller) {
                        return Err("caller is not a manager".to_string());
                    }
                    if to <= v.message_start {
                        return Err("invalid 'to'".to_string());
                    }

                    let message_start = v.message_start;
                    v.message_start = to;
                    v.updated_at = now_ms;
                    v.deleted_messages.retain(|&i| i >= to);
                    m.insert(channel, v);
                    Ok(message_start)
                }
            }
        })?;
        MESSAGE_STORE.with(|rr| {
            let mut mm = rr.borrow_mut();
            for i in message_start..to {
                mm.remove(&MessageId(channel, i));
            }
            Ok(())
        })
    }
}
