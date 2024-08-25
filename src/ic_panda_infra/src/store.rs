use candid::Principal;
use ciborium::{from_reader, into_writer};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    storable::Bound,
    DefaultMemoryImpl, StableCell, StableLog, Storable,
};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct State {
    pub managers: BTreeSet<Principal>,
    pub sponsor_orders: BTreeMap<Principal, BTreeSet<u32>>,
    pub ips_setup_fee: u64, // price in PANDA tokens
    pub ips_discount: u16,  // percent, [0, 100]
    pub ips_canisters: BTreeMap<Principal, (Principal, u16)>, // canister -> (subnet, nodes)
    pub ips_states: BTreeMap<Principal, IpsState>, // subject -> IpsState
    pub incoming_tokens: u64, // in PANDA tokens
}

#[derive(Clone, Deserialize, Serialize)]
pub struct IpsState {
    pub created_at: u64,                    // unix timestamp in milliseconds
    pub services: BTreeMap<Principal, u64>, // service_id -> joined_at
    pub orders: BTreeSet<u32>,
    pub sponsors: BTreeSet<Principal>,
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
pub struct Order {
    pub payer: Principal,
    pub kind: String,
    pub subject: Principal,
    pub service: Principal,
    pub amount: u64,     // in PANDA tokens
    pub discount: u16,   // percent, [0, 100]
    pub paid: u64,       // actually paid in PANDA tokens
    pub created_at: u64, // unix timestamp in milliseconds
}

impl Storable for Order {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<[u8]> {
        let mut buf = vec![];
        into_writer(self, &mut buf).expect("failed to encode Order data");
        Cow::Owned(buf)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        from_reader(&bytes[..]).expect("failed to decode Order data")
    }
}

const STATE_MEMORY_ID: MemoryId = MemoryId::new(0);
const ORDERS_INDEX_MEMORY_ID: MemoryId = MemoryId::new(1);
const ORDERS_DATA_MEMORY_ID: MemoryId = MemoryId::new(2);

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());

    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));

    static STATE_STORE: RefCell<StableCell<State, Memory>> = RefCell::new(
        StableCell::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(STATE_MEMORY_ID)),
            State::default()
        ).expect("failed to init STATE_STORE store")
    );

    static ORDERS: RefCell<StableLog<Order, Memory, Memory>> = RefCell::new(
        StableLog::init(
            MEMORY_MANAGER.with_borrow(|m| m.get(ORDERS_INDEX_MEMORY_ID)),
            MEMORY_MANAGER.with_borrow(|m| m.get(ORDERS_DATA_MEMORY_ID)),
        ).expect("failed to init AIRDROP_LOGS store")
    );

}

pub mod state {
    use super::*;

    pub fn is_manager(caller: &Principal) -> bool {
        STATE.with(|r| r.borrow().managers.contains(caller))
    }

    pub fn with<R>(f: impl FnOnce(&State) -> R) -> R {
        STATE.with(|r| f(&r.borrow()))
    }

    pub fn with_mut<R>(f: impl FnOnce(&mut State) -> R) -> R {
        STATE.with(|r| f(&mut r.borrow_mut()))
    }

    pub fn load() {
        STATE_STORE.with(|r| {
            let s = r.borrow_mut().get().clone();
            STATE.with(|h| {
                *h.borrow_mut() = s;
            });
        });
    }

    pub fn save() {
        STATE.with(|h| {
            STATE_STORE.with(|r| {
                r.borrow_mut()
                    .set(h.borrow().clone())
                    .expect("failed to set STATE data");
            });
        });
    }
}
