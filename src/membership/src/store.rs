use crate::model::Claim;
use dmsg_runtime::{
    storage::{CompactStored, MapExt, Stored},
    Certification,
};
use dmsg_types::{membership::*, *};
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap, StableCell,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;

type Memory = VirtualMemory<DefaultMemoryImpl>;

fn memory(id: u8) -> Memory {
    MEMORY.with_borrow(|m| m.get(MemoryId::new(id)))
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub init: MembershipInit,
    pub sns_verified: bool,
    pub paused: bool,
    pub reserved: u64,
    pub hour: u64,
    pub applications: u32,
    pub minute: u64,
    pub reads: u32,
}

thread_local! {
    static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    pub static CONFIG: RefCell<StableCell<CompactStored<Option<Config>>, Memory>> =
        RefCell::new(StableCell::init(memory(0), CompactStored(None)));
    pub static CLAIMS: RefCell<StableBTreeMap<Vec<u8>, CompactStored<Claim>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(1)));
    pub static NEURONS: RefCell<StableBTreeMap<Vec<u8>, Stored<Vec<Hash>>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(2)));
    pub static POLICIES: RefCell<StableBTreeMap<Vec<u8>, Stored<MembershipPolicy>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(3)));
    pub static PENDING: RefCell<StableBTreeMap<Vec<u8>, Stored<Hash>, Memory>> =
        RefCell::new(StableBTreeMap::init(memory(4)));
    pub static CERT: RefCell<Certification> = RefCell::new(Certification::default());
}

pub fn config() -> Config {
    CONFIG.with_borrow(|t| t.get().0.clone().expect("initialized"))
}

pub fn save_config(c: &Config) {
    CONFIG.with_borrow_mut(|t| t.set(CompactStored(Some(c.clone()))));
}

pub fn load(id: Hash) -> Result<Claim> {
    CLAIMS.with_borrow(|t| t.load(id.as_slice()).ok_or(Error::NotFound))
}

pub fn save(c: &Claim) {
    let mut c = c.clone();
    if let Ok(previous) = load(c.view.claim_id) {
        if previous.view != c.view {
            c.view.lease_revision = c.view.lease_revision.max(
                previous
                    .view
                    .lease_revision
                    .checked_add(1)
                    .expect("claim revision exhausted"),
            );
        }
    } else {
        c.view.lease_revision = c.view.lease_revision.max(1);
    }
    let b = dmsg_protocol::digest("membership/pending-subject/v1", &c.view.beneficiary);
    if matches!(
        c.view.status,
        ClaimStatus::Checking | ClaimStatus::CoolingDown | ClaimStatus::Applying
    ) {
        PENDING.with_borrow_mut(|t| t.put(b.as_slice(), &c.view.claim_id));
    } else if PENDING.with_borrow(|t| t.load(b.as_slice())) == Some(c.view.claim_id) {
        PENDING.with_borrow_mut(|t| t.delete(b.as_slice()));
    }
    CLAIMS.with_borrow_mut(|t| t.put(c.view.claim_id.as_slice(), &c));
    CERT.with_borrow_mut(|t| {
        t.put(
            dmsg_protocol::membership::claim_key(c.view.claim_id).to_vec(),
            &c.view,
        )
    });
}

pub fn rebuild() {
    CERT.with_borrow_mut(|c| {
        CLAIMS.with_borrow(|t| {
            t.for_each(|_, v| {
                c.0.insert(
                    dmsg_protocol::membership::claim_key(v.view.claim_id).to_vec(),
                    dmsg_protocol::canonical(&v.view),
                );
            })
        });
        POLICIES.with_borrow(|t| {
            t.for_each(|_, p| {
                c.0.insert(
                    dmsg_protocol::membership::policy_key(p.version).to_vec(),
                    dmsg_protocol::canonical(&p),
                );
            })
        });
        c.publish();
    });
}
