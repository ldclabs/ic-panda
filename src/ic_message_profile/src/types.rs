use candid::{CandidType, Principal};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub use ic_message_types::profile::*;

#[derive(CandidType, Clone, Debug, Deserialize, Serialize)]
pub struct StateInfo {
    pub name: String,
    pub managers: BTreeSet<Principal>,
    pub ic_oss_cluster: Option<Principal>,
    pub ic_oss_buckets: Vec<Principal>,
    pub profiles_total: u64,
}

#[derive(CandidType, Copy, Clone, Debug, Deserialize, Serialize)]
pub enum CanisterKind {
    OssCluster,
    OssBucket,
}
