use ic_stable_structures::{storable::Bound, Memory, StableBTreeMap, Storable};
use serde::{de::DeserializeOwned, Serialize};
use std::borrow::Cow;

#[derive(Clone, Debug)]
pub struct Stored<T>(pub T);
impl<T: Serialize + DeserializeOwned> Storable for Stored<T> {
    const BOUND: Bound = Bound::Unbounded;
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(cbor2::to_vec(&self.0).expect("stable encoding"))
    }
    fn into_bytes(self) -> Vec<u8> {
        cbor2::to_vec(&self.0).expect("stable encoding")
    }
    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        Self(cbor2::from_slice(&bytes).expect("stable record schema"))
    }
}
/// Convenience operations for typed values. V is fixed by the map, not by each read.
pub trait MapExt<V> {
    fn load(&self, key: &[u8]) -> Option<V>;
    fn put(&mut self, key: &[u8], value: &V);
    fn delete(&mut self, key: &[u8]);
    fn contains(&self, key: &[u8]) -> bool;
    fn page(&self, after: Vec<u8>, limit: usize) -> Vec<(Vec<u8>, V)>;
    fn for_each(&self, f: impl FnMut(Vec<u8>, V));
}
impl<V: Serialize + DeserializeOwned + Clone, M: Memory> MapExt<V>
    for StableBTreeMap<Vec<u8>, Stored<V>, M>
{
    fn load(&self, key: &[u8]) -> Option<V> {
        self.get(&key.to_vec()).map(|r| r.0)
    }
    fn put(&mut self, key: &[u8], value: &V) {
        self.insert(key.to_vec(), Stored(value.clone()));
    }
    fn delete(&mut self, key: &[u8]) {
        self.remove(&key.to_vec());
    }
    fn contains(&self, key: &[u8]) -> bool {
        self.contains_key(&key.to_vec())
    }
    fn page(&self, after: Vec<u8>, limit: usize) -> Vec<(Vec<u8>, V)> {
        use std::ops::Bound::{Excluded, Unbounded};
        self.range((Excluded(after), Unbounded))
            .take(limit)
            .map(|e| (e.key().clone(), e.value().0))
            .collect()
    }
    fn for_each(&self, mut f: impl FnMut(Vec<u8>, V)) {
        for e in self.iter() {
            f(e.key().clone(), e.value().0);
        }
    }
}
