use dmsg_protocol::*;
use dmsg_types::*;

#[cfg(target_arch = "wasm32")]
use ic_certification::AsHashTree;
use ic_certification::RbTree;
use serde::Serialize;
use serde_bytes::ByteBuf;

/// Maximum Candid-encoded successful `Result<CertifiedBatch>` response.
pub const MAX_CERTIFIED_RESPONSE_BYTES: usize = 262_144;

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
        // The replica query cache keys on caller/method/arguments, not transport
        // nonce. Depending on batch time prevents a cached data_certificate from
        // outliving our 60-second client freshness window without any state write.
        // See dfinity/ic query_handler/query_cache.rs, EntryValue::new/is_valid.
        #[cfg(target_arch = "wasm32")]
        let _certificate_batch_time = ic_cdk::api::time();
        ensure(
            !keys.is_empty() && keys.len() <= MAX_BATCH,
            Error::QuotaExceeded,
        )?;
        let certificate = ic_cdk::api::data_certificate()
            .ok_or_else(|| Error::Unavailable("replicated call has no query certificate".into()))?;
        self.batch_with_certificate(canister, keys, certificate)
    }

    fn batch_with_certificate(
        &self,
        canister: candid::Principal,
        keys: Vec<Vec<u8>>,
        certificate: Vec<u8>,
    ) -> Result<CertifiedBatch> {
        ensure(
            !keys.is_empty()
                && keys.len() <= MAX_BATCH
                && certificate.len() <= MAX_CERTIFIED_RESPONSE_BYTES,
            Error::QuotaExceeded,
        )?;
        let mut bytes = certificate.len();
        let mut entries = Vec::with_capacity(keys.len());
        for key in keys {
            let value = self.0.get(&key);
            bytes = bytes
                .saturating_add(key.len())
                .saturating_add(value.map_or(0, Vec::len));
            ensure(bytes <= MAX_CERTIFIED_RESPONSE_BYTES, Error::QuotaExceeded)?;
            let witness = cbor2::to_vec(&self.0.witness(&key)).expect("witness");
            bytes = bytes.saturating_add(witness.len());
            ensure(bytes <= MAX_CERTIFIED_RESPONSE_BYTES, Error::QuotaExceeded)?;
            entries.push(CertifiedEntry {
                key: key.into(),
                value: value.cloned().map(ByteBuf::from),
                witness: witness.into(),
            });
        }
        let batch = CertifiedBatch {
            schema: 1,
            canister,
            certificate: certificate.into(),
            entries,
        };
        // Include Candid's type table, Result variant, vector lengths and
        // certificate. Raw field lengths alone are not a wire-response bound.
        ensure(
            candid::encode_one(Ok::<_, Error>(&batch))
                .expect("certified response")
                .len()
                <= MAX_CERTIFIED_RESPONSE_BYTES,
            Error::QuotaExceeded,
        )?;
        Ok(batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ic_certification::{AsHashTree, HashTree, LookupResult};

    #[test]
    fn membership_and_absence_witnesses_match_the_same_root() {
        let mut tree = Certification::default();
        tree.put(b"a".to_vec(), &1u64);
        tree.put(b"c".to_vec(), &2u64);
        let batch = tree
            .batch_with_certificate(
                candid::Principal::from_slice(&[1]),
                vec![b"a".to_vec(), b"b".to_vec(), b"z".to_vec()],
                vec![1; 1000],
            )
            .unwrap();
        for entry in &batch.entries {
            let witness: HashTree = cbor2::from_slice(&entry.witness).unwrap();
            assert_eq!(witness.digest(), tree.0.root_hash());
            match &entry.value {
                Some(value) => assert_eq!(
                    witness.lookup_path([entry.key.as_slice()]),
                    LookupResult::Found(value.as_slice())
                ),
                None => assert_eq!(
                    witness.lookup_path([entry.key.as_slice()]),
                    LookupResult::Absent
                ),
            }
        }
        tree.remove(b"a");
        let batch = tree
            .batch_with_certificate(batch.canister, vec![b"a".to_vec()], vec![])
            .unwrap();
        assert!(batch.entries[0].value.is_none());
    }

    #[test]
    fn batch_limits_include_certificate_and_candid_envelope() {
        let mut tree = Certification::default();
        for key in 0..64u8 {
            tree.put(vec![key], &ByteBuf::from(vec![1; 1903]));
        }
        let canister = candid::Principal::from_slice(&[1]);
        let keys = (0..64u8).map(|key| vec![key]).collect::<Vec<_>>();
        // The previous raw entries check accepted 262126 bytes here.
        assert_eq!(
            tree.batch_with_certificate(canister, keys.clone(), vec![0; 1000]),
            Err(Error::QuotaExceeded)
        );
        // Even with an empty certificate, Candid's envelope exceeds the limit.
        assert_eq!(
            tree.batch_with_certificate(canister, keys.clone(), vec![]),
            Err(Error::QuotaExceeded)
        );
        for key in 0..64u8 {
            tree.put(vec![key], &ByteBuf::from(vec![1; 1850]));
        }
        let batch = tree
            .batch_with_certificate(canister, keys, vec![0; 1000])
            .unwrap();
        assert!(
            candid::encode_one(Ok::<_, Error>(batch)).unwrap().len()
                <= MAX_CERTIFIED_RESPONSE_BYTES
        );
        for keys in [vec![], vec![vec![0]; 65]] {
            assert_eq!(
                tree.batch_with_certificate(canister, keys, vec![]),
                Err(Error::QuotaExceeded)
            );
        }
        assert_eq!(
            tree.batch_with_certificate(
                canister,
                vec![vec![0]],
                vec![0; MAX_CERTIFIED_RESPONSE_BYTES + 1]
            ),
            Err(Error::QuotaExceeded)
        );
    }
}
