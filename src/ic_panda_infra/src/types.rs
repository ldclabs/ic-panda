use candid::{CandidType, Nat, Principal};
use ciborium::from_reader;
use ic_stable_structures::Storable;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

#[derive(CandidType, Clone, Deserialize, Serialize)]
pub struct ServiceInfo {
    pub id: Principal,     // canister id
    pub subnet: Principal, // subnet id
    pub nodes: u16,        // subnet nodes
}
