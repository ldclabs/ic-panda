use candid::{Nat, Principal};
use ciborium::{from_reader, from_reader_with_buffer, into_writer};
use ic_certification::{HashTreeNode, Label};
use ic_cose_types::types::{
    namespace::{CreateNamespaceInput, NamespaceInfo},
    setting::{
        CreateSettingInput, CreateSettingOutput, SettingInfo, SettingPath, UpdateSettingOutput,
        UpdateSettingPayloadInput,
    },
};
use ic_message_types::{profile::UserInfo, NameBlock};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, StableLog, Storable,
};
use serde::{Deserialize, Serialize};
use serde_bytes::{ByteArray, ByteBuf};
use std::{borrow::Cow, cell::RefCell, collections::BTreeSet};

use crate::{call, token_transfer_from, types};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct State {
    pub name: String,
    pub managers: BTreeSet<Principal>,
    pub cose_canisters: Vec<Principal>,
    pub profile_canisters: Vec<Principal>,
    pub channel_canisters: Vec<Principal>,
    #[serde(default)]
    pub matured_channel_canisters: BTreeSet<Principal>,
    pub short_usernames: BTreeSet<String>, // names that length <= 7
    pub price: types::Price,
    pub incoming_total: u128,
    pub transfer_out_total: u128,
    pub next_block_height: u64,
    pub next_block_phash: ByteArray<32>,
}

impl State {
    pub fn root_hash(&self) -> [u8; 32] {
        self.hash_tree().digest()
    }

    pub fn hash_tree(&self) -> HashTreeNode {
        HashTreeNode::Fork(Box::new((
            HashTreeNode::Labeled(
                Label::from("next_block_height"),
                Box::new(HashTreeNode::Leaf(
                    self.next_block_height.to_be_bytes().to_vec(),
                )),
            ),
            HashTreeNode::Labeled(
                Label::from("next_block_phash"),
                Box::new(HashTreeNode::Leaf(self.next_block_phash.to_vec())),
            ),
        )))
    }
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
pub struct User {
    #[serde(rename = "n")]
    pub name: String,
    #[serde(rename = "i")]
    pub image: String,
    #[serde(rename = "p")]
    pub profile_canister: Principal, // profile canister
    #[serde(rename = "c")]
    pub cose_canister: Option<Principal>, // COSE canister
    #[serde(rename = "u")]
    pub username: Option<String>,
}

impl User {
    pub fn into_info(self, id: Principal) -> UserInfo {
        UserInfo {
            id,
            name: self.name,
            image: self.image,
            profile_canister: self.profile_canister,
            cose_canister: self.cose_canister,
            username: self.username,
        }
    }
}

impl Storable for User {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode User data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode User data")
    }
}

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);
const NAME_MEMORY_ID: MemoryId = MemoryId::new(1);
const USER_MEMORY_ID: MemoryId = MemoryId::new(2);
const NAME_BLK_INDEX_MEMORY_ID: MemoryId = MemoryId::new(3);
const NAME_BLK_DATA_MEMORY_ID: MemoryId = MemoryId::new(4);

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

    static NAME_STORE: RefCell<StableBTreeMap<String, Principal, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(NAME_MEMORY_ID)),
        )
    );

    static USER_STORE: RefCell<StableBTreeMap<Principal, User, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(USER_MEMORY_ID)),
        )
    );

    static NAME_BLOCKS: RefCell<StableLog<Vec<u8>, Memory, Memory>> = RefCell::new(
        StableLog::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(NAME_BLK_INDEX_MEMORY_ID)),
            MEMORY_MANAGER.with_borrow(|m| m.get(NAME_BLK_DATA_MEMORY_ID)),
        ).expect("failed to init NameBlock store")
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

pub mod user {
    use ic_cose_types::{cose::sha3_256, to_cbor_bytes};
    use icrc_ledger_types::icrc3::blocks::{
        BlockWithId, GetBlocksRequest, GetBlocksResult, ICRC3GenericBlock,
    };

    use super::*;

    pub fn names_total() -> u64 {
        NAME_STORE.with(|r| r.borrow().len())
    }

    pub fn users_total() -> u64 {
        USER_STORE.with(|r| r.borrow().len())
    }

    pub async fn update_name(caller: Principal, name: String) -> Result<UserInfo, String> {
        let profile_canister = state::with(|s| s.profile_canisters.last().cloned());
        let profile_canister = profile_canister.ok_or_else(|| "no profile canister".to_string())?;

        let info = USER_STORE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&caller) {
                Some(mut user) => {
                    user.name = name;
                    m.insert(caller, user.clone());
                    user.into_info(caller)
                }
                None => {
                    let user = User {
                        name,
                        image: "".to_string(),
                        profile_canister,
                        cose_canister: None,
                        username: None,
                    };
                    m.insert(caller, user.clone());
                    user.into_info(caller)
                }
            }
        });

        let _: Result<(), String> = call(
            info.profile_canister,
            "admin_upsert_profile",
            (caller, None::<(Principal, u64)>),
            0,
        )
        .await?;
        Ok(info)
    }

    pub async fn update_image(caller: Principal, image: String) -> Result<(), String> {
        let user_profile_canister = USER_STORE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&caller) {
                Some(mut user) => {
                    user.image = image;
                    m.insert(caller, user.clone());
                    Ok(user.profile_canister)
                }
                None => Err("user not found".to_string()),
            }
        })?;

        let _: Result<(), String> = call(
            user_profile_canister,
            "admin_upsert_profile",
            (caller, None::<(Principal, u64)>),
            0,
        )
        .await?;
        Ok(())
    }

    pub async fn update_my_ecdh(
        caller: Principal,
        ecdh_pub: ByteArray<32>,
        encrypted_ecdh: ByteBuf,
    ) -> Result<(), String> {
        let (cose_canister, profile_canister) = USER_STORE
            .with(|r| {
                r.borrow()
                    .get(&caller)
                    .map(|u| (u.cose_canister, u.profile_canister))
            })
            .ok_or_else(|| "user not found".to_string())?;
        let cose_canister = cose_canister.ok_or_else(|| "user has no COSE service".to_string())?;
        let mut sp = SettingPath {
            ns: caller.to_text().replace("-", "_"),
            user_owned: false,
            subject: Some(caller),
            key: b"StaticECDH".to_vec().into(),
            version: 0,
        };
        let res: Result<SettingInfo, String> =
            call(cose_canister, "setting_get_info", (sp.clone(),), 0).await?;
        let res = match res {
            Ok(info) => {
                sp.version = info.version;
                let res: Result<UpdateSettingOutput, String> = call(
                    cose_canister,
                    "setting_update_payload",
                    (
                        sp,
                        UpdateSettingPayloadInput {
                            payload: None,
                            status: None,
                            deprecate_current: None,
                            dek: Some(encrypted_ecdh),
                        },
                    ),
                    0,
                )
                .await?;
                res.map(|_| ())
            }
            Err(_) => {
                let res: Result<CreateSettingOutput, String> = call(
                    cose_canister,
                    "setting_create",
                    (
                        sp,
                        CreateSettingInput {
                            payload: None,
                            desc: None,
                            status: None,
                            tags: None,
                            dek: Some(encrypted_ecdh),
                        },
                    ),
                    0,
                )
                .await?;
                res.map(|_| ())
            }
        };

        if res.is_ok() {
            let _: Result<(), String> = call(
                profile_canister,
                "admin_update_profile_ecdh_pub",
                (caller, ecdh_pub),
                0,
            )
            .await?;
        }
        res
    }

    pub async fn register_username(
        caller: Principal,
        username: String,
        name: String,
        now_ms: u64,
    ) -> Result<UserInfo, String> {
        let (cose_canister, profile_canister, price) = state::with(|s| {
            (
                s.cose_canisters.last().cloned(),
                s.profile_canisters.last().cloned(),
                s.price.clone(),
            )
        });
        let cose_canister = cose_canister.ok_or_else(|| "no COSE canister".to_string())?;
        let profile_canister = profile_canister.ok_or_else(|| "no profile canister".to_string())?;

        let ln = username.to_lowercase();
        let amount = price.get(ln.len()).saturating_sub(types::TOKEN_FEE);
        if amount == 0 {
            return Err("invalid username length".to_string());
        }

        NAME_STORE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&ln) {
                Some(_) => Err("username already registered".to_string()),
                None => {
                    m.insert(ln.clone(), caller);
                    Ok(())
                }
            }
        })?;

        let blk =
            match token_transfer_from(caller, amount.into(), format!("RU: {}", username)).await {
                Err(err) => {
                    NAME_STORE.with(|r| {
                        r.borrow_mut().remove(&ln);
                    });
                    return Err(err);
                }
                Ok(blk) => blk,
            };

        state::with_mut(|s| {
            if ln.len() <= 7 {
                s.short_usernames.insert(ln.clone());
            }
            s.incoming_total += amount as u128;
            let blk = NameBlock {
                height: s.next_block_height,
                phash: s.next_block_phash,
                name: ln,
                user: caller,
                from: None,
                value: amount,
                timestamp: now_ms,
            };
            let blk = to_cbor_bytes(&blk);
            s.next_block_height += 1;
            s.next_block_phash = sha3_256(&blk).into();
            NAME_BLOCKS.with(|r| {
                r.borrow_mut()
                    .append(&blk)
                    .expect("failed to append NameBlock");
            });
            ic_cdk::api::set_certified_data(s.root_hash().as_slice());
        });

        let _: Result<NamespaceInfo, String> = call(
            cose_canister,
            "admin_create_namespace",
            (CreateNamespaceInput {
                name: caller.to_text().replace("-", "_"),
                visibility: 0,
                desc: Some(format!(
                    "register_username: {}, $PANDA block: {}",
                    username, blk
                )),
                max_payload_size: Some(1024),
                managers: BTreeSet::from([ic_cdk::id()]),
                auditors: BTreeSet::from([caller]),
                users: BTreeSet::from([caller]),
            },),
            0,
        )
        .await?;

        let info = USER_STORE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&caller) {
                Some(mut user) => {
                    user.cose_canister = Some(cose_canister);
                    user.username = Some(username);
                    m.insert(caller, user.clone());
                    user.into_info(caller)
                }
                None => {
                    let user = User {
                        name: name,
                        image: "".to_string(),
                        profile_canister,
                        cose_canister: Some(cose_canister),
                        username: Some(username),
                    };
                    m.insert(caller, user.clone());
                    user.into_info(caller)
                }
            }
        });

        let _: Result<(), String> = call(
            info.profile_canister,
            "admin_upsert_profile",
            (caller, None::<(Principal, u64)>),
            0,
        )
        .await?;
        Ok(info)
    }

    pub fn search_username(prefix: String) -> Vec<String> {
        state::with(|s| {
            if prefix.len() <= 7 {
                s.short_usernames
                    .iter()
                    .filter(|n| n.starts_with(&prefix))
                    .cloned()
                    .collect()
            } else {
                NAME_STORE.with(|r| {
                    if r.borrow().contains_key(&prefix) {
                        vec![prefix]
                    } else {
                        vec![]
                    }
                })
            }
        })
    }

    pub fn get_by_username(username: String) -> Result<UserInfo, String> {
        state::with(|s| {
            if username.len() <= 7 && !s.short_usernames.contains(&username) {
                None
            } else {
                NAME_STORE.with(|r| match r.borrow().get(&username) {
                    Some(id) => USER_STORE.with(|m| m.borrow().get(&id).map(|u| u.into_info(id))),
                    None => None,
                })
            }
        })
        .ok_or_else(|| "user not found".to_string())
    }

    pub fn get(user: Principal) -> Result<UserInfo, String> {
        USER_STORE.with(|m| {
            m.borrow()
                .get(&user)
                .map(|u| u.into_info(user))
                .ok_or_else(|| "user not found".to_string())
        })
    }

    pub fn batch_get(ids: BTreeSet<Principal>) -> Vec<UserInfo> {
        USER_STORE.with(|r| {
            ids.iter()
                .filter_map(|id| r.borrow().get(id).map(|u| u.into_info(*id)))
                .collect()
        })
    }

    pub fn get_blocks(args: Vec<GetBlocksRequest>) -> GetBlocksResult {
        const MAX_BLOCKS_PER_RESPONSE: u64 = 100;

        let next_block_height = state::with(|s| s.next_block_height);

        NAME_BLOCKS.with(|r| {
            let logs = r.borrow();
            let logs_len = logs.len();
            let mut blocks = vec![];
            for arg in args {
                let (start, length) = arg
                    .as_start_and_length()
                    .unwrap_or_else(|msg| ic_cdk::api::trap(&msg));

                let max_length = MAX_BLOCKS_PER_RESPONSE.saturating_sub(blocks.len() as u64);
                if max_length == 0 {
                    break;
                }

                let length = max_length.min(length).min(logs_len - start);
                for i in start..start + length {
                    match logs.get(i) {
                        None => break,
                        Some(block) => {
                            blocks.push(BlockWithId {
                                id: Nat::from(i),
                                block: ICRC3GenericBlock::Blob(block.into()),
                            });
                        }
                    }
                }

                if blocks.len() as u64 >= MAX_BLOCKS_PER_RESPONSE {
                    break;
                }
            }

            GetBlocksResult {
                log_length: Nat::from(next_block_height),
                blocks,
                archived_blocks: vec![],
            }
        })
    }
}

pub mod channel {
    use super::*;
    use ic_message_types::channel::{
        channel_kek_key, ChannelInfo, ChannelKEKInput, CreateChannelInput,
    };

    pub async fn create_channel(
        caller: Principal,
        now_ms: u64,
        mut input: CreateChannelInput,
    ) -> Result<ChannelInfo, String> {
        let (channel_canister, profile_canister, price) = state::with(|s| {
            let i = if s.channel_canisters.len() > 1 {
                now_ms % s.channel_canisters.len() as u64
            } else {
                0
            };
            (
                s.channel_canisters.get(i as usize).cloned(),
                s.profile_canisters.last().cloned(),
                s.price.clone(),
            )
        });

        let channel_canister = channel_canister.ok_or_else(|| "no channel canister".to_string())?;
        let profile_canister = profile_canister.ok_or_else(|| "no profile canister".to_string())?;

        let amount = price.channel.saturating_sub(types::TOKEN_FEE);
        if amount == 0 {
            return Err("invalid channel price".to_string());
        }

        let (user_profile_canister, is_new) = USER_STORE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&caller) {
                Some(user) => (user.profile_canister, false),
                None => {
                    let user = User {
                        name: input.name.clone(),
                        image: "".to_string(),
                        profile_canister,
                        cose_canister: None,
                        username: None,
                    };
                    m.insert(caller, user.clone());
                    (profile_canister, true)
                }
            }
        });

        if is_new {
            let _: Result<(), String> = call(
                user_profile_canister,
                "admin_upsert_profile",
                (caller, None::<(Principal, u64)>),
                0,
            )
            .await?;
        }

        token_transfer_from(caller, amount.into(), format!("CC")).await?;

        state::with_mut(|s| {
            s.incoming_total += amount as u128;
        });

        input.created_by = caller;
        input.paid = amount;
        let res: Result<ChannelInfo, String> =
            call(channel_canister, "admin_create_channel", (input,), 0).await?;
        let res = res?;

        let _: Result<(), String> = call(
            user_profile_canister,
            "admin_upsert_profile",
            (caller, Some((res.canister, res.id))),
            0,
        )
        .await?;
        Ok(res)
    }

    pub async fn save_channel_kek(caller: Principal, input: ChannelKEKInput) -> Result<(), String> {
        let cose_canister = USER_STORE
            .with(|r| r.borrow().get(&caller).map(|u| u.cose_canister))
            .ok_or_else(|| "user not found".to_string())?;
        let cose_canister = cose_canister.ok_or_else(|| "user has no COSE service".to_string())?;
        let mut sp = SettingPath {
            ns: caller.to_text().replace("-", "_"),
            user_owned: false,
            subject: Some(caller),
            key: channel_kek_key(&input.canister, input.id),
            version: 0,
        };
        let res: Result<SettingInfo, String> =
            call(cose_canister, "setting_get_info", (sp.clone(),), 0).await?;
        let res = match res {
            Ok(info) => {
                sp.version = info.version;
                let res: Result<UpdateSettingOutput, String> = call(
                    cose_canister,
                    "setting_update_payload",
                    (
                        sp,
                        UpdateSettingPayloadInput {
                            payload: None,
                            status: None,
                            deprecate_current: None,
                            dek: Some(input.kek),
                        },
                    ),
                    0,
                )
                .await?;
                res.map(|_| ())
            }
            Err(_) => {
                let res: Result<CreateSettingOutput, String> = call(
                    cose_canister,
                    "setting_create",
                    (
                        sp,
                        CreateSettingInput {
                            payload: None,
                            desc: None,
                            status: None,
                            tags: None,
                            dek: Some(input.kek),
                        },
                    ),
                    0,
                )
                .await?;
                res.map(|_| ())
            }
        };

        res.map(|_| ())
    }
}
