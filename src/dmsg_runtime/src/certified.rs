use dmsg_protocol::*;
use dmsg_types::*;
#[cfg(target_arch = "wasm32")]
use ic_certification::AsHashTree;
use ic_certification::RbTree;
use serde::Serialize;
use serde_bytes::ByteBuf;

#[derive(Default)]
pub struct Certification(pub RbTree<Vec<u8>, Vec<u8>>);

impl Certification {
    pub fn remove(&mut self, key: &[u8]) {
        self.0.delete(key);
        self.publish();
    }
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
