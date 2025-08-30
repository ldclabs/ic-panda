use candid::{CandidType, Principal};
use ciborium::{from_reader, from_reader_with_buffer, into_writer};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, StableLog, Storable,
};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, cell::RefCell, collections::BTreeSet};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(CandidType, Clone, Default, Deserialize, Serialize)]
pub struct State {
    pub minted_total: u64,
    pub next_block_height: u64,
    pub preparers: BTreeSet<Principal>,
    pub committers: BTreeSet<Principal>,
}

impl Storable for State {
    const BOUND: Bound = Bound::Unbounded;

    fn into_bytes(self) -> Vec<u8> {
        let mut buf = vec![];
        into_writer(&self, &mut buf).expect("failed to encode State data");
        buf
    }

    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode State data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode State data")
    }
}

#[derive(CandidType, Clone, Deserialize, Serialize, Ord, PartialOrd, Eq, PartialEq)]
pub struct Linker(pub Principal, pub Principal);

impl Storable for Linker {
    const BOUND: Bound = Bound::Bounded {
        max_size: 63,
        is_fixed_size: false,
    };

    fn into_bytes(self) -> Vec<u8> {
        let mut buf = vec![];
        into_writer(&self, &mut buf).expect("failed to encode Linker data");
        buf
    }

    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode Linker data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode Linker data")
    }
}

#[derive(CandidType, Clone, Deserialize, Serialize)]
pub struct LinkLog {
    pub linker: Linker,
    pub rewards: u64,
    pub minted_at: u64,
}

impl Storable for LinkLog {
    const BOUND: Bound = Bound::Bounded {
        max_size: 78,
        is_fixed_size: false,
    };

    fn into_bytes(self) -> Vec<u8> {
        let mut buf = vec![];
        into_writer(&(&self.linker, &self.rewards, &self.minted_at), &mut buf)
            .expect("failed to encode LinkLog data");
        buf
    }

    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut buf = vec![];
        into_writer(&(&self.linker, &self.rewards, &self.minted_at), &mut buf)
            .expect("failed to encode LinkLog data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let res: (Linker, u64, u64) =
            from_reader(&bytes[..]).expect("failed to decode LinkLog data");
        Self {
            linker: res.0,
            rewards: res.1,
            minted_at: res.2,
        }
    }
}

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);
const LINKER_MEMORY_ID: MemoryId = MemoryId::new(1);
const LINKER_BLK_INDEX_MEMORY_ID: MemoryId = MemoryId::new(2);
const LINKER_BLK_DATA_MEMORY_ID: MemoryId = MemoryId::new(3);

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());

    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));

    static STATE_STORE: RefCell<StableCell<Vec<u8>, Memory>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(STATE_MEMORY_ID)),
            Vec::new()
        )
    );

    static LINKER_STORE: RefCell<StableBTreeMap<Linker, u64, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(LINKER_MEMORY_ID)),
        )
    );

    static LINKER_BLOCKS: RefCell<StableLog<LinkLog, Memory, Memory>> = RefCell::new(
        StableLog::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(LINKER_BLK_INDEX_MEMORY_ID)),
            MEMORY_MANAGER.with_borrow(|m| m.get(LINKER_BLK_DATA_MEMORY_ID)),
        )
    );
}

pub mod state {
    use super::*;

    pub fn with<R>(f: impl FnOnce(&State) -> R) -> R {
        STATE.with_borrow(f)
    }

    pub fn with_mut<R>(f: impl FnOnce(&mut State) -> R) -> R {
        STATE.with_borrow_mut(f)
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
                r.borrow_mut().set(buf);
            });
        });
    }
}

pub mod minter {
    use super::*;
    use crate::{token_transfer_to, TOKEN_1, TOKEN_FEE};

    const COINBASE_REWARDS: u64 = TOKEN_1;
    const BLOCK_HALVING: u64 = 21_000_000;

    pub fn try_prepare(linker: Linker) -> bool {
        let has_rewards = STATE.with_borrow(|s| {
            let height = s.next_block_height;
            let rewards = COINBASE_REWARDS >> (height / BLOCK_HALVING);
            rewards > TOKEN_FEE
        });

        if !has_rewards {
            return false;
        }

        LINKER_STORE.with_borrow_mut(|m| {
            if m.contains_key(&linker) {
                return false;
            }

            m.insert(linker, 0);
            true
        })
    }

    pub async fn try_commit(linker: Linker, now_ms: u64) -> Option<u64> {
        let user = linker.0;
        let res = LINKER_STORE.with_borrow_mut(|m| {
            match m.get(&linker) {
                None => return None,
                Some(next_block_height) if next_block_height > 0 => return None,
                _ => {}
            }

            let (height, rewards) = STATE.with_borrow_mut(|s| {
                let height = s.next_block_height;
                let rewards = COINBASE_REWARDS >> (height / BLOCK_HALVING);
                if rewards <= TOKEN_FEE {
                    return (height, 0);
                }

                s.next_block_height = s.next_block_height.saturating_add(1);
                (height, rewards)
            });

            if rewards == 0 {
                return None;
            }

            m.insert(linker.clone(), height + 1);
            LINKER_BLOCKS.with_borrow_mut(|logs| {
                logs.append(&LinkLog {
                    linker,
                    rewards,
                    minted_at: now_ms,
                })
                .expect("failed to append LinkLog data");
            });

            Some((height, rewards))
        });

        if let Some((height, rewards)) = res {
            match token_transfer_to(user, rewards.into(), format!("{}", height)).await {
                Ok(_) => {
                    STATE.with_borrow_mut(|s| {
                        s.minted_total = s.minted_total.saturating_add(rewards);
                    });
                    Some(height)
                }
                Err(_) => None,
            }
        } else {
            None
        }
    }

    pub fn get_log(height: u64) -> Option<LinkLog> {
        LINKER_BLOCKS.with_borrow(|logs| logs.get(height))
    }

    pub fn list_logs(prev: Option<u64>, take: usize) -> Vec<LinkLog> {
        LINKER_BLOCKS.with_borrow(|logs| {
            let latest = logs.len();
            if latest == 0 {
                return vec![];
            }

            let prev = prev.unwrap_or(latest);
            if prev > latest || prev == 0 {
                return vec![];
            }

            let mut idx = prev.saturating_sub(1);
            let mut res: Vec<LinkLog> = Vec::with_capacity(take);
            while let Some(log) = logs.get(idx) {
                res.push(log);
                if idx == 0 || res.len() >= take {
                    break;
                }
                idx -= 1;
            }

            res
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_bound_max_size() {
        let principal =
            Principal::from_text("i2gam-uue3y-uxwyd-mzyhb-nirhd-hz3l4-2hw3f-4fzvw-lpvvc-dqdrg-7qe")
                .unwrap();
        let v = Linker(principal, principal);
        let v = v.to_bytes();
        println!("Linker max_size: {}", v.len());

        let v = LinkLog {
            linker: Linker(principal, principal),
            rewards: 100000000,
            minted_at: u64::MAX,
        };
        let v = v.to_bytes();
        println!("LinkLog max_size: {}", v.len());
    }

    #[test]
    fn test_max_supply() {
        let mut height = 0u64;
        let mut max_supply = 0u64;
        loop {
            let rewards = 100000000 >> (height / 21_000_000);
            if rewards <= 10000 {
                break;
            }
            max_supply += rewards;
            height += 1;
        }

        println!("max_supply: {}, height: {}", max_supply, height - 1);
        // max_supply: 4199743632000000, height: 293999999
        // max_supply < 42,000,000 $DMSG
    }
}
