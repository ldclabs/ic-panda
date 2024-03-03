use candid::{CandidType, Principal};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, StableLog, Storable,
};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, cell::RefCell, collections::BTreeSet};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Debug, Serialize, Deserialize, Default, CandidType)]
pub struct State {
    pub airdrop_balance: u64,
    pub total_airdrop: u64,
    pub total_luckydraw: u64,
    pub total_luckydraw_icp: u64,
    pub total_luckydraw_count: u64,
}

// NOTE: the default configuration is dysfunctional, but it's convenient to have
// a Default impl for the initialization of the [STATE] variable above.
impl Storable for State {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        ciborium::ser::into_writer(self, &mut buf).expect("failed to encode STATE data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        ciborium::de::from_reader(&bytes[..]).expect("failed to decode STATE data")
    }
}

// AirdropLog format: [user, time, token_amount]
#[derive(Clone, Debug, Serialize, Deserialize, CandidType)]
pub struct AirdropLog(Principal, u64, u64);

impl Storable for AirdropLog {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        ciborium::ser::into_writer(self, &mut buf).expect("failed to encode airdrop log");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        ciborium::de::from_reader(&bytes[..]).expect("failed to decode airdrop log")
    }
}

// AirdropLog format: [user, time, token_amount, icp_amount, random_number]
#[derive(Clone, Debug, Serialize, Deserialize, CandidType)]
pub struct LuckyDrawLog(Principal, u64, u64, u64, u64);

impl Storable for LuckyDrawLog {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        ciborium::ser::into_writer(self, &mut buf).expect("failed to encode luckydraw log");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        ciborium::de::from_reader(&bytes[..]).expect("failed to decode luckydraw log")
    }
}

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);
const AIRDROP_MEMORY_ID: MemoryId = MemoryId::new(1);
const AIRDROP_LOG_INDEX_MEMORY_ID: MemoryId = MemoryId::new(2);
const AIRDROP_LOG_DATA_MEMORY_ID: MemoryId = MemoryId::new(3);
const LUCKYDRAW_LOG_INDEX_MEMORY_ID: MemoryId = MemoryId::new(4);
const LUCKYDRAW_LOG_DATA_MEMORY_ID: MemoryId = MemoryId::new(5);

thread_local! {
    static CAPTCHA_SECRET: RefCell<[u8; 32]> = RefCell::new([0; 32]);

    static ACTIVE_USERS: RefCell<BTreeSet<Principal>> = RefCell::new(BTreeSet::new());

    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));

    static STATE: RefCell<StableCell<State, Memory>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(STATE_MEMORY_ID)),
            State::default()
        ).expect("failed to init STATE store")
    );

    static AIRDROP: RefCell<StableBTreeMap<Principal, (), Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(AIRDROP_MEMORY_ID)),
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

pub mod airdrop {
    use super::*;

    pub fn total() -> u64 {
        AIRDROP_LOGS.with(|r| r.borrow().len())
    }

    pub fn balance() -> u64 {
        STATE.with(|r| r.borrow().get().airdrop_balance)
    }

    // check if a user has claimed airdrop.
    pub fn has(user: Principal) -> bool {
        AIRDROP
            .with(|r| r.borrow().get(&user))
            .map(|_| true)
            .unwrap_or(false)
    }

    // update luckydraw state and append a log.
    // return the log index or an error message when append failed.
    pub fn update(user: Principal, time: u64, amount: u64) -> Result<u64, String> {
        AIRDROP.with(|r| r.borrow_mut().insert(user, ()));
        STATE.with(|r| {
            let mut borrowed = r.borrow_mut();
            let mut state = borrowed.get().clone();
            state.airdrop_balance = state.airdrop_balance.saturating_sub(amount);
            state.total_airdrop = state.total_airdrop.saturating_add(amount);
            borrowed
                .set(state)
                .map_err(|err| format!("failed to update airdrop state, error {:?}", err))
        })?;
        AIRDROP_LOGS
            .with(|r| r.borrow_mut().append(&AirdropLog(user, time, amount)))
            .map_err(|err| format!("failed to append airdrop log, error {:?}", err))
    }

    // get airdrop logs in reverse order, return the next index to fetch.
    pub fn logs(size: usize, idx: Option<u64>) -> (Vec<AirdropLog>, Option<u64>) {
        AIRDROP_LOGS.with(|r| {
            let log_store = r.borrow();
            let latest = log_store.len();
            if latest == 0 {
                return (vec![], None);
            }
            let mut idx = idx.unwrap_or(latest - 1);
            if idx >= latest {
                return (vec![], None);
            }

            let mut logs = Vec::with_capacity(size);
            while let Some(log) = log_store.get(idx) {
                logs.push(log);
                if idx == 0 {
                    return (logs, None);
                }

                if logs.len() >= size {
                    break;
                }
                idx -= 1;
            }

            (logs, Some(idx))
        })
    }
}

pub mod luckydraw {
    use super::*;

    pub fn total() -> u64 {
        LUCKYDRAW_LOGS.with(|r| r.borrow().len())
    }

    // update luckydraw state and append a log.
    // return the log index or an error message when append failed.
    pub fn update(
        user: Principal,
        time: u64,
        token_amount: u64,
        icp_amount: u64,
        random: u64,
    ) -> Result<u64, String> {
        STATE.with(|r| {
            let mut borrowed = r.borrow_mut();
            let mut state = borrowed.get().clone();
            state.total_luckydraw = state.total_luckydraw.saturating_add(token_amount);
            state.total_luckydraw_icp = state.total_luckydraw_icp.saturating_add(icp_amount);
            state.total_luckydraw_count = state.total_luckydraw_count.saturating_add(1);
            borrowed
                .set(state)
                .map_err(|err| format!("failed to update luckydraw state, error {:?}", err))
        })?;
        LUCKYDRAW_LOGS
            .with(|r| {
                r.borrow_mut()
                    .append(&LuckyDrawLog(user, time, token_amount, icp_amount, random))
            })
            .map_err(|err| format!("failed to append luckydraw log, error {:?}", err))
    }

    // get luckydraw logs in reverse order, return the next index to fetch.
    pub fn logs(size: usize, idx: Option<u64>) -> (Vec<LuckyDrawLog>, Option<u64>) {
        LUCKYDRAW_LOGS.with(|r| {
            let log_store = r.borrow();
            let latest = log_store.len();
            if latest == 0 {
                return (vec![], None);
            }
            let mut idx = idx.unwrap_or(latest - 1);
            if idx >= latest {
                return (vec![], None);
            }

            let mut logs = Vec::with_capacity(size);
            while let Some(log) = log_store.get(idx) {
                logs.push(log);
                if idx == 0 {
                    return (logs, None);
                }

                if logs.len() >= size {
                    break;
                }
                idx -= 1;
            }

            (logs, Some(idx))
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

    pub fn get() -> State {
        STATE.with(|r| r.borrow().get().clone())
    }

    // /// A helper function to access the state.
    // pub fn with<R>(f: impl FnOnce(&State) -> R) -> R {
    //     STATE.with(|r| f(r.borrow().get()))
    // }

    // /// A helper function to change the state.
    // pub fn with_mut(f: impl FnOnce(&mut State)) {
    //     STATE
    //         .with(|r| {
    //             let mut borrowed = r.borrow_mut();
    //             let mut state = borrowed.get().clone();
    //             f(&mut state);
    //             borrowed.set(state)
    //         })
    //         .expect("failed to set STATE data");
    // }
}
