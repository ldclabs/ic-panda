use candid::Principal;
use ciborium::{from_reader, from_reader_with_buffer, into_writer};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, Storable,
};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{BTreeSet, HashMap},
};

use crate::types;

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct State {
    pub name: String,
    pub managers: BTreeSet<Principal>,
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
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct ChannelSetting {
    #[serde(rename = "a")]
    pub pin: u16,
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
    use super::*;

    pub fn profiles_total() -> u64 {
        PROFILE_STORE.with(|r| r.borrow().len())
    }

    pub fn upsert(user: Principal, now_ms: u64) -> Result<(), String> {
        PROFILE_STORE.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&user) {
                Some(mut p) => {
                    p.active_at = now_ms;
                    m.insert(user, p);
                }
                None => {
                    m.insert(
                        user,
                        Profile {
                            created_at: now_ms,
                            active_at: now_ms,
                            ..Default::default()
                        },
                    );
                }
            }

            Ok(())
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
                    let info = types::ProfileInfo {
                        id: user,
                        canister: ic_cdk::id(),
                        bio: p.bio.clone(),
                        active_at: p.active_at,
                        created_at: p.created_at,
                        following: Some(p.following.clone()),
                        channels: Some(
                            p.channels
                                .iter()
                                .map(|(k, v)| {
                                    (
                                        *k,
                                        types::ChannelSetting {
                                            pin: v.pin,
                                            alias: v.alias.clone(),
                                            tags: v.tags.clone(),
                                        },
                                    )
                                })
                                .collect(),
                        ),
                    };
                    m.insert(user, p);
                    Ok(info)
                }
                None => Err("profile not found".to_string()),
            }
        })
    }

    pub fn get(user: Principal, is_caller: bool) -> Result<types::ProfileInfo, String> {
        PROFILE_STORE.with(|r| match r.borrow_mut().get(&user) {
            Some(v) => Ok(types::ProfileInfo {
                id: user,
                canister: ic_cdk::id(),
                bio: v.bio,
                active_at: v.active_at,
                created_at: v.created_at,
                following: if is_caller { Some(v.following) } else { None },
                channels: if is_caller {
                    Some(
                        v.channels
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
            }),
            None => Err("profile not found".to_string()),
        })
    }
}
