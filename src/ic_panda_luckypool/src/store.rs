use candid::{CandidType, Nat, Principal};
use ciborium::{from_reader, into_writer};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, StableLog, Storable,
};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

use crate::types;
use crate::utils::{luckycode_from_string, luckycode_to_string, sha3_256};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(CandidType, Clone, Default, Deserialize, Serialize)]
pub struct State {
    pub airdrop_balance: u64,
    pub total_airdrop: u64,
    pub total_airdrop_count: u64,
    pub total_luckydraw: u64,
    pub total_luckydraw_icp: u64,
    pub total_luckydraw_count: u64,
    pub latest_airdrop_logs: Vec<types::AirdropLog>, // latest 10 airdrop logs
    pub luckiest_luckydraw_logs: Vec<types::LuckyDrawLog>, // latest 10 luckiest luckydraw logs
    pub latest_luckydraw_logs: Vec<types::LuckyDrawLog>, // latest 10 luckydraw logs
    pub managers: Option<BTreeSet<Principal>>,
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

// AirdropState format: (lucky code, total claimed tokens, claimable tokens)
// If total claimed tokens is smaller than TOKEN_1, it is effective timestamp in hours since the UNIX epoch.
#[derive(Clone, Default, Deserialize, Serialize)]
pub struct AirdropState(pub u32, pub u64, pub u64);

impl Storable for AirdropState {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode AirdropState data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode AirdropState data")
    }
}

// AirdropLog format: (user, time, token_amount, lucky_code)
#[derive(Clone, Deserialize, Serialize)]
pub struct AirdropLog(Principal, u64, u64, u32);

impl From<(u64, AirdropLog)> for types::AirdropLog {
    fn from(log: (u64, AirdropLog)) -> Self {
        let (idx, log) = log;
        types::AirdropLog {
            id: Nat::from(idx),
            ts: log.1,
            caller: log.0,
            amount: Nat::from(log.2),
            lucky_code: luckycode_to_string(log.3),
        }
    }
}

impl Storable for AirdropLog {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode AirdropLog data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode AirdropLog data")
    }
}

// AirdropLog format: (user, time, token_amount, icp_amount, random_number)
#[derive(Clone, Deserialize, Serialize)]
pub struct LuckyDrawLog(Principal, u64, u64, u64, u64);

impl From<(u64, LuckyDrawLog)> for types::LuckyDrawLog {
    fn from(log: (u64, LuckyDrawLog)) -> Self {
        let (idx, log) = log;
        types::LuckyDrawLog {
            id: Nat::from(idx),
            ts: log.1,
            caller: log.0,
            amount: Nat::from(log.2),
            icp_amount: Nat::from(log.3),
            random: log.4,
        }
    }
}

impl Storable for LuckyDrawLog {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode LuckyDrawLog data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode LuckyDrawLog data")
    }
}

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);
const AIRDROP_MEMORY_ID: MemoryId = MemoryId::new(1);
const LUCKYCODE_MEMORY_ID: MemoryId = MemoryId::new(2);
const AIRDROP_LOG_INDEX_MEMORY_ID: MemoryId = MemoryId::new(3);
const AIRDROP_LOG_DATA_MEMORY_ID: MemoryId = MemoryId::new(4);
const LUCKYDRAW_LOG_INDEX_MEMORY_ID: MemoryId = MemoryId::new(5);
const LUCKYDRAW_LOG_DATA_MEMORY_ID: MemoryId = MemoryId::new(6);

thread_local! {
    static CAPTCHA_SECRET: RefCell<[u8; 32]> = const { RefCell::new([0; 32]) };

    static STATE_HEAP: RefCell<State> = RefCell::new(State::default());

    static ACTIVE_USERS: RefCell<BTreeSet<Principal>> = const { RefCell::new(BTreeSet::new()) };

    static MANAGERS: RefCell<BTreeSet<Principal>> = const { RefCell::new(BTreeSet::new()) };

    static NOTIFICATIONS: RefCell<BTreeMap<u8, types::Notification>> = const { RefCell::new(BTreeMap::new()) };

    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));

    static STATE: RefCell<StableCell<State, Memory>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(STATE_MEMORY_ID)),
            State::default()
        ).expect("failed to init STATE store")
    );

    static AIRDROP: RefCell<StableBTreeMap<Principal, AirdropState, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(AIRDROP_MEMORY_ID)),
        )
    );

    static LUCKYCODE: RefCell<StableBTreeMap<u32, Principal, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(LUCKYCODE_MEMORY_ID)),
        )
    );

    static AIRDROP_LOGS: RefCell<StableLog<AirdropLog, Memory, Memory>> = RefCell::new(
        StableLog::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(AIRDROP_LOG_INDEX_MEMORY_ID)),
            MEMORY_MANAGER.with_borrow(|m| m.get(AIRDROP_LOG_DATA_MEMORY_ID)),
        ).expect("failed to init AIRDROP_LOGS store")
    );

    static LUCKYDRAW_LOGS: RefCell<StableLog<LuckyDrawLog, Memory, Memory>> = RefCell::new(
        StableLog::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(LUCKYDRAW_LOG_INDEX_MEMORY_ID)),
            MEMORY_MANAGER.with_borrow(|m| m.get(LUCKYDRAW_LOG_DATA_MEMORY_ID)),
        ).expect("failed to init LUCKY_DRAW_LOGS store")
    );
}

pub mod captcha {
    use super::*;

    pub fn with_secret<R>(f: impl FnOnce(&[u8]) -> R) -> R {
        CAPTCHA_SECRET.with(|r| f(r.borrow().as_slice()))
    }

    pub fn set_secret(secret: [u8; 32]) {
        CAPTCHA_SECRET.with(|r| *r.borrow_mut() = secret);
    }
}

pub mod luckycode {
    use super::*;

    pub fn get(code: u32) -> Option<Principal> {
        LUCKYCODE.with(|r| r.borrow().get(&code))
    }

    pub fn get_by_string(code: &str) -> Option<Principal> {
        match luckycode_from_string(code) {
            Ok(code) => LUCKYCODE.with(|r| r.borrow().get(&code)),
            Err(_) => None,
        }
    }

    pub fn new_from(user: Principal, random: &[u8]) -> u32 {
        LUCKYCODE.with(|r| {
            let b = sha3_256(random);
            let mut code = u32::from_be_bytes([b[0], b[1], b[2], b[3]]).saturating_add(1000001) - 1;
            {
                let m = r.borrow();
                let mut i = 0u32;
                while m.contains_key(&code) {
                    code = if code == u32::MAX { 1000001 } else { code + 1 };

                    i += 1;
                    if i > 10000 {
                        ic_cdk::trap("failed to generate a lucky code");
                    }
                }
            }
            r.borrow_mut().insert(code, user);
            code
        })
    }
}

pub mod airdrop {
    use crate::{AIRDROP_AMOUNT, TOKEN_1};

    use super::*;

    // check if a user has claimed airdrop.
    pub fn state_of(user: &Principal) -> Option<AirdropState> {
        AIRDROP.with(|r| r.borrow().get(user))
    }

    // update luckydraw state and append a log.
    // return the log or an error message when append failed.
    pub fn insert(
        user: Principal,
        referrer: Option<Principal>,
        now_sec: u64,
        amount: u64,
        caller_code: u32,
    ) -> Result<types::AirdropLog, String> {
        let referrer_code = AIRDROP.with(|r| {
            let mut m = r.borrow_mut();
            // effective_hours should be smaller than TOKEN_1 100_000_000
            let effective_hours = now_sec / 3600 + 24 + caller_code as u64 % 48;
            m.insert(user, AirdropState(caller_code, effective_hours, amount));

            match referrer {
                None => 0,
                Some(referrer) => match m.get(&referrer) {
                    None => 0,
                    Some(state) => {
                        m.insert(
                            referrer,
                            AirdropState(
                                state.0,
                                state.1,
                                state.2 + (AIRDROP_AMOUNT / 2) * TOKEN_1,
                            ),
                        );
                        state.0
                    }
                },
            }
        });

        let log = AirdropLog(user, now_sec, 0, referrer_code);
        let idx = AIRDROP_LOGS
            .with(|r| r.borrow_mut().append(&log))
            .map_err(|err| format!("failed to append airdrop log, error {:?}", err))?;
        Ok(types::AirdropLog::from((idx, log)))
    }

    pub fn harvest(
        user: Principal,
        now_sec: u64,
        amount: u64,
    ) -> Result<(AirdropState, types::AirdropLog), String> {
        let state = AIRDROP.with(|r| {
            let mut m = r.borrow_mut();
            match m.get(&user) {
                // should never happen, we have checked the state before calling this function
                None => Err("no claimable airdrop to harvest".to_string()),
                Some(state) => {
                    let state = AirdropState(
                        state.0,
                        if state.1 >= TOKEN_1 {
                            state.1.saturating_add(amount)
                        } else {
                            amount
                        },
                        state.2.saturating_sub(amount),
                    );
                    m.insert(user, state.clone());
                    Ok(state)
                }
            }
        })?;

        let log = AirdropLog(user, now_sec, amount, 0);
        let idx = AIRDROP_LOGS
            .with(|r| r.borrow_mut().append(&log))
            .map_err(|err| format!("failed to append airdrop log, error {:?}", err))?;
        Ok((state, types::AirdropLog::from((idx, log))))
    }

    pub fn ban_users(users: Vec<Principal>) -> Result<(), String> {
        AIRDROP.with(|r| {
            let mut m = r.borrow_mut();
            for user in users {
                if let Some(state) = m.get(&user) {
                    m.insert(user, AirdropState(0, state.1, state.2));
                }
            }
        });
        Ok(())
    }

    // get airdrop logs in reverse order, return the next index to fetch.
    pub fn logs(prev: Option<u64>, take: usize) -> Vec<types::AirdropLog> {
        AIRDROP_LOGS.with(|r| {
            let log_store = r.borrow();
            let latest = log_store.len();
            if latest == 0 {
                return vec![];
            }

            let prev = prev.unwrap_or(latest);
            if prev > latest || prev == 0 {
                return vec![];
            }

            let mut idx = prev - 1;
            let mut logs: Vec<types::AirdropLog> = Vec::with_capacity(take);
            while let Some(log) = log_store.get(idx) {
                logs.push(types::AirdropLog::from((idx, log)));

                if idx == 0 || logs.len() >= take {
                    break;
                }
                idx -= 1;
            }

            logs
        })
    }
}

pub mod luckydraw {
    use super::*;

    // insert luckydraw state and append a log.
    // return the log or an error message when append failed.
    pub fn insert(
        user: Principal,
        now_sec: u64,
        token_amount: u64,
        icp_amount: u64,
        random: u64,
    ) -> Result<types::LuckyDrawLog, String> {
        let log = LuckyDrawLog(user, now_sec, token_amount, icp_amount, random);
        let idx = LUCKYDRAW_LOGS
            .with(|r| r.borrow_mut().append(&log))
            .map_err(|err| format!("failed to append luckydraw log, error {:?}", err))?;
        Ok(types::LuckyDrawLog::from((idx, log)))
    }

    // get luckydraw logs in reverse order, return the next index to fetch.
    pub fn logs(prev: Option<u64>, take: usize) -> Vec<types::LuckyDrawLog> {
        LUCKYDRAW_LOGS.with(|r| {
            let log_store = r.borrow();
            let latest = log_store.len();
            if latest == 0 {
                return vec![];
            }

            let prev = prev.unwrap_or(latest);
            if prev > latest || prev == 0 {
                return vec![];
            }

            let mut idx = prev - 1;
            let mut logs: Vec<types::LuckyDrawLog> = Vec::with_capacity(take);
            while let Some(log) = log_store.get(idx) {
                logs.push(types::LuckyDrawLog::from((idx, log)));

                if idx == 0 || logs.len() >= take {
                    break;
                }
                idx -= 1;
            }

            logs
        })
    }
}

pub mod user {
    use super::*;

    // add a user to the active user set, return true if the user is not in the set
    pub fn active(user: Principal) -> bool {
        ACTIVE_USERS.with(|r| r.borrow_mut().insert(user))
    }

    pub fn deactive(user: Principal) {
        ACTIVE_USERS.with(|r| r.borrow_mut().remove(&user));
    }
}

pub mod state {
    use super::*;

    pub fn is_manager(caller: &Principal) -> bool {
        STATE_HEAP.with(|r| {
            r.borrow()
                .managers
                .as_ref()
                .map(|ms| ms.contains(caller))
                .unwrap_or(false)
        })
    }

    pub fn airdrop_balance() -> u64 {
        STATE_HEAP.with(|r| r.borrow().airdrop_balance)
    }

    pub fn with<R>(f: impl FnOnce(&State) -> R) -> R {
        STATE_HEAP.with(|r| f(&r.borrow()))
    }

    pub fn with_mut<R>(f: impl FnOnce(&mut State) -> R) -> R {
        STATE_HEAP.with(|r| f(&mut r.borrow_mut()))
    }

    pub fn load() {
        STATE.with(|r| {
            STATE_HEAP.with(|h| {
                *h.borrow_mut() = r.borrow().get().clone();
            });
        });
    }

    pub fn save() {
        STATE_HEAP.with(|h| {
            STATE.with(|r| {
                r.borrow_mut()
                    .set(h.borrow().clone())
                    .expect("failed to set STATE data");
            });
        });
    }
}

pub mod notification {
    use super::*;

    pub fn list() -> Vec<types::Notification> {
        NOTIFICATIONS.with(|r| r.borrow().values().cloned().collect())
    }

    pub fn add(arg: types::Notification) {
        NOTIFICATIONS.with(|r| {
            r.borrow_mut().insert(arg.id, arg);
        });
    }

    pub fn remove(ids: Vec<u8>) {
        NOTIFICATIONS.with(|r| {
            r.borrow_mut().retain(|k, _| !ids.contains(k));
        });
    }
}
