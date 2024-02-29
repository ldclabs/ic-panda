use candid::Principal;
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableBTreeMap, StableCell, StableLog, Storable,
};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, cell::RefCell};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct State {
    pub total_airdrop: u64,
    pub totaol_lucky_draw: u64,
    pub captcha_secret: [u8; 32],
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

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);
const AIRDROP_MEMORY_ID: MemoryId = MemoryId::new(1);
const AIRDROP_LOG_INDEX_MEMORY_ID: MemoryId = MemoryId::new(2);
const AIRDROP_LOG_DATA_MEMORY_ID: MemoryId = MemoryId::new(3);
const LUCKY_DRAW_MEMORY_ID: MemoryId = MemoryId::new(4);
const LUCKY_DRAW_LOG_INDEX_MEMORY_ID: MemoryId = MemoryId::new(5);
const LUCKY_DRAW_LOG_DATA_MEMORY_ID: MemoryId = MemoryId::new(6);

thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));

    static STATE: RefCell<StableCell<State, Memory>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(STATE_MEMORY_ID)),
            State::default()
        ).expect("failed to init STATE store")
    );

    static AIRDROP: RefCell<StableBTreeMap<Principal, u64, Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(AIRDROP_MEMORY_ID)),
        )
    );

    static AIRDROP_LOGS: RefCell<StableLog<Vec<u8>, Memory, Memory>> = RefCell::new(
        StableLog::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(AIRDROP_LOG_INDEX_MEMORY_ID)),
            MEMORY_MANAGER.with_borrow(|m| m.get(AIRDROP_LOG_DATA_MEMORY_ID)),
        ).expect("failed to init AIRDROP_LOGS store")
    );

    static LUCKY_DRAW: RefCell<StableBTreeMap<Principal, (u32, u64), Memory>> = RefCell::new(
        StableBTreeMap::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(LUCKY_DRAW_MEMORY_ID)),
        )
    );

    static LUCKY_DRAW_LOGS: RefCell<StableLog<Vec<u8>, Memory, Memory>> = RefCell::new(
        StableLog::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(LUCKY_DRAW_LOG_INDEX_MEMORY_ID)),
            MEMORY_MANAGER.with_borrow(|m| m.get(LUCKY_DRAW_LOG_DATA_MEMORY_ID)),
        ).expect("failed to init LUCKY_DRAW_LOGS store")
    );
}

/// A helper function to access the state.
pub fn with_state<R>(f: impl FnOnce(&State) -> R) -> R {
    STATE.with(|r| f(r.borrow().get()))
}

/// A helper function to change the state.
pub fn with_state_mut(f: impl FnOnce(&mut State)) {
    STATE
        .with(|r| {
            let mut borrowed = r.borrow_mut();
            let mut state = borrowed.get().clone();
            f(&mut state);
            borrowed.set(state)
        })
        .expect("failed to set STATE data");
}
