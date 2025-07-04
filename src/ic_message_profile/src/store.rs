use candid::Principal;
use ciborium::{from_reader, from_reader_with_buffer, into_writer};
use ic_oss_types::{
    cose::Token,
    file::{CreateFileInput, CreateFileOutput, UpdateFileInput, UpdateFileOutput},
    MapValue,
};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, Storable,
};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteArray;
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{BTreeSet, HashMap},
};

use crate::{call, types};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct State {
    pub name: String,
    pub managers: BTreeSet<Principal>,
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

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Profile {
    #[serde(rename = "b")]
    pub bio: String,
    #[serde(rename = "a")]
    pub active_at: u64,
    #[serde(rename = "f")]
    pub following: BTreeSet<Principal>,
    #[serde(rename = "c")]
    pub channels: HashMap<(Principal, u64), ChannelSetting>,
    #[serde(rename = "ca")]
    pub created_at: u64,
    #[serde(rename = "ep")]
    pub ecdh_pub: Option<ByteArray<32>>,
    #[serde(default, rename = "i")]
    pub image_file: Option<(Principal, u32)>, // image file: (ic-oss-bucket canister, file_id)
    #[serde(default, rename = "l")]
    pub links: Vec<types::Link>,
    #[serde(default, rename = "t")]
    pub tokens: Vec<Principal>, // token ledger canister
}

impl Profile {
    pub fn into_info(
        self,
        caller: Principal,
        canister: Principal,
        is_caller: bool,
    ) -> types::ProfileInfo {
        types::ProfileInfo {
            id: caller,
            canister,
            bio: self.bio,
            active_at: self.active_at,
            created_at: self.created_at,
            image_file: self.image_file,
            links: self.links,
            tokens: self.tokens,
            ecdh_pub: self.ecdh_pub,
            following: if is_caller {
                Some(self.following)
            } else {
                None
            },
            channels: if is_caller {
                Some(
                    self.channels
                        .into_iter()
                        .map(|(k, v)| {
                            (
                                k,
                                types::ChannelSetting {
                                    pin: v.pin,
                                    alias: v.alias,
                                    tags: v.tags,
                                },
                            )
                        })
                        .collect(),
                )
            } else {
                None
            },
        }
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct ChannelSetting {
    #[serde(rename = "p")]
    pub pin: u32,
    #[serde(rename = "a")]
    pub alias: String,
    #[serde(rename = "t")]
    pub tags: BTreeSet<String>,
}

impl Storable for Profile {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode Profile data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode Profile data")
    }
}

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);
const PROFILE_MEMORY_ID: MemoryId = MemoryId::new(1);

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

    static PROFILE_STORE: RefCell<StableBTreeMap<Principal, Profile, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(PROFILE_MEMORY_ID)),
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

pub mod profile {
    use serde_bytes::ByteBuf;

    use super::*;

    pub fn profiles_total() -> u64 {
        PROFILE_STORE.with(|r| r.borrow().len())
    }

    pub fn with_mut<R>(
        user: Principal,
        f: impl FnOnce(&mut Profile) -> Result<R, String>,
    ) -> Result<R, String> {
        PROFILE_STORE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&user) {
                None => Err("profile not found".to_string()),
                Some(mut p) => f(&mut p).inspect(|_r| {
                    m.insert(user, p);
                }),
            }
        })
    }

    pub fn upsert(
        user: Principal,
        now_ms: u64,
        channel: Option<(Principal, u64)>,
    ) -> Result<(), String> {
        PROFILE_STORE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&user) {
                Some(mut p) => {
                    p.active_at = now_ms;
                    if let Some(cid) = channel {
                        let max_pin = p.channels.values().map(|v| v.pin).max().unwrap_or_default();
                        p.channels.insert(
                            cid,
                            ChannelSetting {
                                pin: max_pin.saturating_add(1),
                                alias: "".to_string(),
                                tags: BTreeSet::new(),
                            },
                        );
                    }
                    m.insert(user, p);
                }
                None => {
                    let mut p = Profile {
                        created_at: now_ms,
                        active_at: now_ms,
                        ..Default::default()
                    };
                    if let Some(cid) = channel {
                        p.channels.insert(cid, ChannelSetting::default());
                    }
                    m.insert(user, p);
                }
            }

            Ok(())
        })
    }

    pub fn update_profile_ecdh_pub(
        user: Principal,
        now_ms: u64,
        ecdh_pub: ByteArray<32>,
    ) -> Result<(), String> {
        PROFILE_STORE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&user) {
                Some(mut p) => {
                    p.ecdh_pub = Some(ecdh_pub);
                    p.active_at = now_ms;
                    m.insert(user, p);
                    Ok(())
                }
                None => Err("profile not found".to_string()),
            }
        })
    }

    pub fn update(
        user: Principal,
        now_ms: u64,
        mut input: types::UpdateProfileInput,
    ) -> Result<types::ProfileInfo, String> {
        PROFILE_STORE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&user) {
                Some(mut p) => {
                    if let Some(bio) = input.bio {
                        p.bio = bio;
                    }

                    if !input.unfollow.is_empty() {
                        for u in input.unfollow {
                            p.following.remove(&u);
                        }
                    }
                    if !input.follow.is_empty() {
                        p.following.append(&mut input.follow);
                    }
                    if p.following.len() > types::MAX_PROFILE_FOLLOWING {
                        return Err("following limit exceeded".to_string());
                    }

                    if !input.remove_channels.is_empty() {
                        for k in input.remove_channels {
                            p.channels.remove(&k);
                        }
                    }
                    if !input.upsert_channels.is_empty() {
                        for (k, v) in input.upsert_channels {
                            p.channels.insert(
                                k,
                                ChannelSetting {
                                    pin: v.pin,
                                    alias: v.alias,
                                    tags: v.tags,
                                },
                            );
                        }
                    }

                    p.active_at = now_ms;
                    m.insert(user, p.clone());
                    Ok(p.into_info(user, ic_cdk::api::canister_self(), true))
                }
                None => Err("profile not found".to_string()),
            }
        })
    }

    pub async fn upload_image_token(
        user: Principal,
        now_ms: u64,
        input: types::UploadImageInput,
    ) -> Result<types::UploadImageOutput, String> {
        let (ic_oss_cluster, ic_oss_bucket) =
            state::with(|s| (s.ic_oss_cluster, s.ic_oss_buckets.last().cloned()));
        let ic_oss_cluster = ic_oss_cluster.ok_or_else(|| "ic_oss_cluster not set".to_string())?;
        let ic_oss_bucket = ic_oss_bucket.ok_or_else(|| "ic_oss_cluster not set".to_string())?;

        let image = PROFILE_STORE.with(|r| match r.borrow().get(&user) {
            None => Err("profile not found".to_string()),
            Some(p) => Ok(p.image_file),
        })?;

        let name = input.filename(now_ms.to_string());
        let token: Result<ByteBuf, String> = call(
            ic_oss_cluster,
            "admin_weak_access_token",
            (
                Token {
                    subject: ic_cdk::api::canister_self(),
                    audience: image.map(|i| i.0).unwrap_or(ic_oss_bucket),
                    policies: "File.Write".to_string(),
                },
                now_ms / 1000,
                60 * 10_u64, // 10 minutes
            ),
            0,
        )
        .await?;
        let token = token?;
        let custom = MapValue::from([("by_hash".to_string(), "read".into())]);
        let image = match image {
            Some(image) => {
                let res: Result<UpdateFileOutput, String> = call(
                    image.0,
                    "update_file_info",
                    (
                        UpdateFileInput {
                            id: image.1,
                            name: Some(name.clone()),
                            content_type: Some(input.content_type),
                            size: Some(input.size),
                            custom: Some(custom),
                            ..Default::default()
                        },
                        Some(token),
                    ),
                    0,
                )
                .await?;
                res?;
                image
            }
            None => {
                let res: Result<CreateFileOutput, String> = call(
                    ic_oss_bucket,
                    "create_file",
                    (
                        CreateFileInput {
                            parent: 0,
                            name: name.clone(),
                            content_type: input.content_type,
                            size: Some(input.size),
                            custom: Some(custom),
                            ..Default::default()
                        },
                        Some(token),
                    ),
                    0,
                )
                .await?;
                let res = res?;
                with_mut(user, |p| {
                    p.image_file = Some((ic_oss_bucket, res.id));
                    Ok(())
                })?;
                (ic_oss_bucket, res.id)
            }
        };

        let token: Result<ByteBuf, String> = call(
            ic_oss_cluster,
            "admin_weak_access_token",
            (
                Token {
                    subject: user,
                    audience: image.0,
                    policies: format!("File.Write:{}", image.1),
                },
                now_ms / 1000,
                60 * 10_u64, // 10 minutes
            ),
            0,
        )
        .await?;
        Ok(types::UploadImageOutput {
            name,
            image,
            access_token: token?,
        })
    }

    pub fn get(user: Principal, is_caller: bool) -> Result<types::ProfileInfo, String> {
        PROFILE_STORE.with(|r| match r.borrow_mut().get(&user) {
            Some(v) => Ok(v.into_info(user, ic_cdk::api::canister_self(), is_caller)),
            None => Err("profile not found".to_string()),
        })
    }
}
