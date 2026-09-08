//! Each table has a permanent memory id. Writes are incremental, including
//! across awaits/upgrades; there is no pre_upgrade whole-heap serialization.
use crate::*;
#[cfg(target_arch = "wasm32")]
use ic_certification::AsHashTree;
use ic_certification::RbTree;
use ic_stable_structures::{
    memory_manager::{MemoryId, MemoryManager, VirtualMemory},
    DefaultMemoryImpl, StableBTreeMap,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_bytes::ByteBuf;
use std::cell::RefCell;

type Memory = VirtualMemory<DefaultMemoryImpl>;
thread_local! {
    static MEMORY: RefCell<MemoryManager<DefaultMemoryImpl>> = RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
}
pub struct Table(StableBTreeMap<Vec<u8>, Vec<u8>, Memory>);
impl Table {
    pub fn new(id: u8) -> Self {
        Self(StableBTreeMap::init(
            MEMORY.with_borrow(|m| m.get(MemoryId::new(id))),
        ))
    }
    pub fn get<T: DeserializeOwned>(&self, key: &[u8]) -> Option<T> {
        self.0
            .get(&key.to_vec())
            .map(|b| cbor2::from_slice(&b).expect("stable schema v1"))
    }
    pub fn put<T: Serialize>(&mut self, key: &[u8], value: &T) {
        self.0
            .insert(key.to_vec(), cbor2::to_vec(value).expect("stable encoding"));
    }
    pub fn remove(&mut self, key: &[u8]) {
        self.0.remove(&key.to_vec());
    }
    pub fn contains(&self, key: &[u8]) -> bool {
        self.0.contains_key(&key.to_vec())
    }
    pub fn len(&self) -> u64 {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn page<T: DeserializeOwned>(&self, after: Vec<u8>, limit: usize) -> Vec<(Vec<u8>, T)> {
        use std::ops::Bound::{Excluded, Unbounded};
        self.0
            .range((Excluded(after), Unbounded))
            .take(limit)
            .map(|item| {
                (
                    item.key().clone(),
                    cbor2::from_slice(&item.value()).expect("stable schema v1"),
                )
            })
            .collect()
    }
    pub fn for_each<T: DeserializeOwned>(&self, mut f: impl FnMut(Vec<u8>, T)) {
        for item in self.0.iter() {
            f(
                item.key().clone(),
                cbor2::from_slice(&item.value()).expect("stable schema v1"),
            );
        }
    }
}
#[derive(Default)]
pub struct Certification(pub RbTree<Vec<u8>, Vec<u8>>);
impl Certification {
    pub fn put<T: Serialize>(&mut self, key: Vec<u8>, value: &T) {
        self.0.insert(key, canonical(value));
        self.publish();
    }
    pub fn publish(&self) {
        #[cfg(target_arch = "wasm32")]
        ic_cdk::api::certified_data_set(self.0.root_hash());
    }
    pub fn batch(&self, canister: candid::Principal, keys: Vec<Vec<u8>>) -> Result<CertifiedBatch> {
        ensure(
            !keys.is_empty() && keys.len() <= MAX_BATCH,
            Error::QuotaExceeded,
        )?;
        let entries: Vec<_> = keys
            .into_iter()
            .map(|key| CertifiedEntry {
                value: self.0.get(&key).cloned().map(ByteBuf::from),
                witness: cbor2::to_vec(&self.0.witness(&key))
                    .expect("witness")
                    .into(),
                key: key.into(),
            })
            .collect();
        ensure(
            entries
                .iter()
                .map(|e| e.key.len() + e.witness.len() + e.value.as_ref().map_or(0, |v| v.len()))
                .sum::<usize>()
                <= 262_144,
            Error::QuotaExceeded,
        )?;
        Ok(CertifiedBatch {
            schema: 1,
            canister,
            certificate: ic_cdk::api::data_certificate()
                .ok_or_else(|| {
                    Error::Unavailable("replicated call has no query certificate".into())
                })?
                .into(),
            entries,
        })
    }
}

pub async fn call<In, Out>(id: candid::Principal, method: &str, args: In) -> Result<Out>
where
    In: candid::utils::ArgumentEncoder + Send,
    Out: candid::CandidType + DeserializeOwned,
{
    let response = ic_cdk::call::Call::bounded_wait(id, method)
        .with_args(&args)
        .await
        .map_err(|_| Error::ExecutionUnknown)?;
    response.candid().map_err(|_| Error::ExecutionUnknown)
}
