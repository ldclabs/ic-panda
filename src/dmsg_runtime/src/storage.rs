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

/// A domain type with a dedicated, compact stable-memory representation.
///
/// Implementations keep storage schema details behind the store module seam,
/// so changing stable encoding does not change public protocol CBOR.
pub trait StableCodec: Sized {
    type Repr: Serialize + DeserializeOwned;

    fn to_repr(&self) -> Self::Repr;
    fn from_repr(repr: Self::Repr) -> Self;
}

impl<T: StableCodec> StableCodec for Option<T> {
    type Repr = Option<T::Repr>;

    fn to_repr(&self) -> Self::Repr {
        self.as_ref().map(StableCodec::to_repr)
    }

    fn from_repr(repr: Self::Repr) -> Self {
        repr.map(T::from_repr)
    }
}

/// Stable-memory adapter for [`StableCodec`] values.
#[derive(Clone, Debug)]
pub struct CompactStored<T>(pub T);

impl<T: StableCodec> Storable for CompactStored<T> {
    const BOUND: Bound = Bound::Unbounded;

    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(compact_bytes(&self.0))
    }

    fn into_bytes(self) -> Vec<u8> {
        compact_bytes(&self.0)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        Self(compact_from_bytes(&bytes))
    }
}

pub fn compact_bytes<T: StableCodec>(value: &T) -> Vec<u8> {
    cbor2::to_vec(&value.to_repr()).expect("compact stable encoding")
}

pub fn compact_from_bytes<T: StableCodec>(bytes: &[u8]) -> T {
    let repr = cbor2::from_slice(bytes).expect("compact stable record schema");
    T::from_repr(repr)
}

trait StableRecord<V>: Storable {
    fn new(value: &V) -> Self;
    fn into_value(self) -> V;
}

impl<V: Serialize + DeserializeOwned + Clone> StableRecord<V> for Stored<V> {
    fn new(value: &V) -> Self {
        Self(value.clone())
    }

    fn into_value(self) -> V {
        self.0
    }
}

impl<V: StableCodec + Clone> StableRecord<V> for CompactStored<V> {
    fn new(value: &V) -> Self {
        Self(value.clone())
    }

    fn into_value(self) -> V {
        self.0
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
impl<V, R, M> MapExt<V> for StableBTreeMap<Vec<u8>, R, M>
where
    R: StableRecord<V>,
    M: Memory,
{
    fn load(&self, key: &[u8]) -> Option<V> {
        self.get(&key.to_vec()).map(StableRecord::into_value)
    }

    fn put(&mut self, key: &[u8], value: &V) {
        self.insert(key.to_vec(), R::new(value));
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
            .map(|e| (e.key().clone(), e.value().into_value()))
            .collect()
    }

    fn for_each(&self, mut f: impl FnMut(Vec<u8>, V)) {
        for e in self.iter() {
            f(e.key().clone(), e.value().into_value());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbor2::Cbor;
    use ic_stable_structures::VectorMemory;

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
    struct Domain {
        verbose_identifier: u64,
        another_verbose_field: String,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Cbor)]
    struct DomainRepr {
        #[cbor(key = 1)]
        verbose_identifier: u64,
        #[cbor(key = 2)]
        another_verbose_field: String,
    }

    impl StableCodec for Domain {
        type Repr = DomainRepr;

        fn to_repr(&self) -> Self::Repr {
            DomainRepr {
                verbose_identifier: self.verbose_identifier,
                another_verbose_field: self.another_verbose_field.clone(),
            }
        }

        fn from_repr(repr: Self::Repr) -> Self {
            Self {
                verbose_identifier: repr.verbose_identifier,
                another_verbose_field: repr.another_verbose_field,
            }
        }
    }

    #[test]
    fn compact_adapter_preserves_the_map_interface() {
        let memory = VectorMemory::default();
        let mut map = StableBTreeMap::<Vec<u8>, CompactStored<Domain>, _>::init(memory);
        let first = Domain {
            verbose_identifier: 1,
            another_verbose_field: "one".into(),
        };
        let second = Domain {
            verbose_identifier: 2,
            another_verbose_field: "two".into(),
        };

        map.put(b"a", &first);
        map.put(b"b", &second);
        assert_eq!(map.load(b"a"), Some(first.clone()));
        assert!(map.contains(b"b"));
        assert_eq!(
            map.page(Vec::new(), 10),
            vec![(b"a".to_vec(), first), (b"b".to_vec(), second)]
        );
        map.delete(b"a");
        assert_eq!(map.load(b"a"), None);
    }

    #[test]
    fn compact_adapter_emits_integer_map_keys() {
        let value = Domain {
            verbose_identifier: 7,
            another_verbose_field: "value".into(),
        };
        let compact = compact_bytes(&value);
        let plain = cbor2::to_vec(&value).unwrap();
        let cbor2::Value::Map(entries) = cbor2::from_slice(&compact).unwrap() else {
            panic!("compact representation must be a map")
        };
        assert!(entries
            .iter()
            .all(|(key, _)| matches!(key, cbor2::Value::Integer(_))));
        assert!(compact.len() < plain.len());
        assert_eq!(compact_from_bytes::<Domain>(&compact), value);
    }

    #[test]
    fn compact_maps_ignore_future_unknown_keys() {
        let value = Domain {
            verbose_identifier: 7,
            another_verbose_field: "value".into(),
        };
        let mut encoded: cbor2::Value = cbor2::from_slice(&compact_bytes(&value)).unwrap();
        let cbor2::Value::Map(entries) = &mut encoded else {
            panic!("compact representation must be a map")
        };
        entries.push((99_u64.into(), "future-field".into()));

        let bytes = cbor2::to_vec(&encoded).unwrap();
        assert_eq!(compact_from_bytes::<Domain>(&bytes), value);
    }
}
