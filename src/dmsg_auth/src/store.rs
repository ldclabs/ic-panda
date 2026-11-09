//! Compact stable records and bounded transient interactive state.
use candid::Principal;
use cbor2::Cbor;
use dmsg_runtime::storage::Stored;
use dmsg_types::{Hash, auth::*};
use ic_stable_structures::{
    DefaultMemoryImpl, StableBTreeMap, StableCell,
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
};
use serde_bytes::ByteBuf;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Cbor)]
pub(crate) struct Config {
    #[cbor(key = 1)]
    pub schema: u8,
    #[cbor(key = 2)]
    pub governance: Principal,
    #[cbor(key = 3)]
    pub salt: Option<Hash>,
    #[cbor(key = 4)]
    pub nonce: u64,
    #[cbor(key = 5)]
    pub admission_key: Option<Hash>,
    #[cbor(key = 6)]
    pub limits: Limits,
    #[cbor(key = 7)]
    pub day: u64,
    #[cbor(key = 8)]
    pub registered: u32,
}

#[derive(Clone, Cbor)]
pub(crate) struct Limits {
    #[cbor(key = 1)]
    identities: u64,
    #[cbor(key = 2)]
    daily: u32,
    #[cbor(key = 3)]
    requests: u32,
    #[cbor(key = 4)]
    renewals: u32,
    #[cbor(key = 5)]
    challenges: u32,
}

impl From<AuthLimits> for Limits {
    fn from(l: AuthLimits) -> Self {
        Self {
            identities: l.max_identities,
            daily: l.daily_registrations,
            requests: l.requests_per_minute,
            renewals: l.renewals_per_minute,
            challenges: l.max_challenges,
        }
    }
}

impl From<Limits> for AuthLimits {
    fn from(l: Limits) -> Self {
        Self {
            max_identities: l.identities,
            daily_registrations: l.daily,
            requests_per_minute: l.requests,
            renewals_per_minute: l.renewals,
            max_challenges: l.challenges,
        }
    }
}

#[derive(Clone, Cbor)]
pub(crate) struct Identity {
    #[cbor(key = 1)]
    pub public: ByteBuf,
    #[cbor(key = 2)]
    pub sessions: Vec<Principal>,
    #[cbor(key = 3)]
    pub logout_nonce: u64,
}

#[derive(Clone, Cbor)]
pub(crate) struct Session {
    #[cbor(key = 1)]
    pub identity: Hash,
    #[cbor(key = 2)]
    pub renewed_at: u64,
    #[cbor(key = 3)]
    pub expiration: u64,
    #[cbor(key = 4)]
    pub public: ByteBuf,
}

#[derive(Clone)]
pub(crate) struct Pending {
    pub public: ByteBuf,
    pub challenge: AuthChallenge,
    pub nonce: u64,
}

#[derive(Default)]
pub(crate) struct Transient {
    pub pending: BTreeMap<Principal, Pending>,
    pub expirations: BTreeSet<(u64, Principal)>,
    pub minute: u64,
    pub requests: u32,
    pub preparations: u32,
    pub callers: BTreeMap<Principal, u8>,
    pub renewals: u32,
}

thread_local! {
    static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    static CONFIG: RefCell<StableCell<Stored<Option<Config>>, Memory>> = RefCell::new(StableCell::init(memory(0), Stored(None)));
    pub(crate) static IDENTITIES: RefCell<StableBTreeMap<[u8; 32], Stored<Identity>, Memory>> = RefCell::new(StableBTreeMap::init(memory(1)));
    pub(crate) static SESSIONS: RefCell<StableBTreeMap<Vec<u8>, Stored<Session>, Memory>> = RefCell::new(StableBTreeMap::init(memory(2)));
    // One registration per hardware caller keeps an admission ticket from
    // creating more identities by logging out and registering repeatedly.
    pub(crate) static REGISTRATIONS: RefCell<StableBTreeMap<Vec<u8>, [u8; 32], Memory>> = RefCell::new(StableBTreeMap::init(memory(3)));
    pub(crate) static TRANSIENT: RefCell<Transient> = RefCell::new(Transient::default());
}

fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

pub(crate) fn config() -> Config {
    CONFIG.with_borrow(|c| c.get().0.clone().expect("initialized"))
}

pub(crate) fn save_config(config: Config) {
    CONFIG.with_borrow_mut(|c| c.set(Stored(Some(config))));
}

pub(crate) fn identity(id: &Hash) -> Option<Identity> {
    IDENTITIES.with_borrow(|m| m.get(&**id).map(|v| v.0))
}

pub(crate) fn session(caller: Principal) -> Option<Session> {
    SESSIONS.with_borrow(|m| m.get(&caller.as_slice().to_vec()).map(|v| v.0))
}

pub(crate) fn remove_pending(caller: Principal) {
    TRANSIENT.with_borrow_mut(|t| {
        if let Some(p) = t.pending.remove(&caller) {
            t.expirations.remove(&(p.challenge.expires_at, caller));
        }
    });
}
