use dmsg_protocol::*;
use dmsg_types::*;

use ic_certification::{leaf, leaf_hash, AsHashTree, HashTree, RbTree};
use serde::Serialize;
use serde_bytes::ByteBuf;
use std::borrow::Cow;

/// Maximum Candid-encoded successful `Result<CertifiedBatch>` response.
pub const MAX_CERTIFIED_RESPONSE_BYTES: usize = 262_144;

/// Leaf bytes with their cached hash. The tree recomputes every ancestor on a
/// write; without the cache each ancestor would rehash its complete leaf value.
struct Leaf {
    bytes: Vec<u8>,
    hash: ic_certification::Hash,
}

impl AsHashTree for Leaf {
    fn root_hash(&self) -> ic_certification::Hash {
        self.hash
    }

    fn as_hash_tree(&self) -> HashTree {
        leaf(Cow::from(self.bytes.as_slice()))
    }
}

/// Heap certification tree. Every mutation republishes the O(1) root, so a
/// message can never leave certified data behind the tree.
pub struct Certification(RbTree<Vec<u8>, Leaf>);

impl Default for Certification {
    fn default() -> Self {
        Self(RbTree::new())
    }
}

impl Certification {
    pub fn get(&self, key: &[u8]) -> Option<&[u8]> {
        self.0.get(key).map(|leaf| leaf.bytes.as_slice())
    }

    pub fn root_hash(&self) -> ic_certification::Hash {
        self.0.root_hash()
    }

    /// Certify the canonical CBOR of a public view.
    pub fn put<T: Serialize>(&mut self, key: Vec<u8>, value: &T) {
        self.insert(key, canonical(value));
    }

    /// Certify already encoded leaf bytes.
    pub fn insert(&mut self, key: Vec<u8>, bytes: Vec<u8>) {
        let hash = leaf_hash(&bytes);
        self.0.insert(key, Leaf { bytes, hash });
        self.publish();
    }

    pub fn remove(&mut self, key: &[u8]) {
        self.0.delete(key);
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
            let value = self.get(&key);
            bytes = bytes
                .saturating_add(key.len())
                .saturating_add(value.map_or(0, <[u8]>::len));
            ensure(bytes <= MAX_CERTIFIED_RESPONSE_BYTES, Error::QuotaExceeded)?;
            let witness = cbor2::to_vec(&self.0.witness(&key)).expect("witness");
            bytes = bytes.saturating_add(witness.len());
            ensure(bytes <= MAX_CERTIFIED_RESPONSE_BYTES, Error::QuotaExceeded)?;
            entries.push(CertifiedEntry {
                key: key.into(),
                value: value.map(|v| ByteBuf::from(v.to_vec())),
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
    use ic_certification::LookupResult;

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
            assert_eq!(witness.digest(), tree.root_hash());
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
    fn cached_leaf_hashes_produce_the_public_tree_hashes() {
        let mut cached = Certification::default();
        let mut plain = RbTree::<Vec<u8>, Vec<u8>>::new();
        for key in 0..100u8 {
            let value = vec![key; usize::from(key) * 7];
            cached.insert(vec![key], value.clone());
            plain.insert(vec![key], value);
        }
        for key in (0..100u8).step_by(3) {
            cached.remove(&[key]);
            plain.delete(&[key]);
        }
        assert_eq!(cached.root_hash(), plain.root_hash());
        for key in [vec![1], vec![3], vec![200]] {
            assert_eq!(
                cbor2::to_vec(&cached.0.witness(&key)).unwrap(),
                cbor2::to_vec(&plain.witness(&key)).unwrap()
            );
        }
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
