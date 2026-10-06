//! Certification tree for a large name map, with node hashes in stable memory.
//!
//! Names fall into `2^HANDLE_BUCKET_BITS` buckets by `handle_bucket`. The bucket
//! bits form a binary label trie: a node is `Empty` when no name lies below it,
//! otherwise `fork(labeled([0], left), labeled([1], right))`. A bucket holds its
//! names as a balanced fork tree of `labeled(name, leaf(value))` in byte order.
//! Every node hash sits in a dense stable array, so an upgrade restores the root
//! without visiting names, and a write rehashes one bucket and one path.
use dmsg_protocol::HANDLE_BUCKET_BITS;
use ic_certification::hash_tree::{fork_hash, labeled_hash, leaf_hash};
use ic_certification::{empty, fork, labeled, leaf, pruned, Hash, HashTree};
use ic_stable_structures::Memory;

const BITS: u32 = HANDLE_BUCKET_BITS;
// Heap-ordered node indices 1..2^(BITS + 1); bucket b is node 2^BITS + b.
const PAGES: u64 = (2u64 << BITS) * 32 / 65_536;

/// A bucket's names in byte order, each with the value bytes it certifies.
pub type Bucket = [(Vec<u8>, Vec<u8>)];

pub struct NameTree<M: Memory> {
    nodes: M,
}

fn empty_hash() -> Hash {
    empty().digest()
}

fn branch_hash(left: &Hash, right: &Hash) -> Hash {
    fork_hash(&labeled_hash(&[0], left), &labeled_hash(&[1], right))
}

#[derive(Clone, Copy)]
enum Show {
    Leaf,
    Label,
    Hide,
}

// A balanced fork tree over the bucket, with every subtree that shows nothing
// replaced by its hash.
fn subtree(names: &Bucket, show: &dyn Fn(usize) -> Show) -> HashTree {
    fn build(
        names: &Bucket,
        lo: usize,
        hi: usize,
        show: &dyn Fn(usize) -> Show,
    ) -> (HashTree, bool) {
        match hi - lo {
            0 => (empty(), false),
            1 => {
                let (name, value) = &names[lo];
                match show(lo) {
                    Show::Leaf => (labeled(name.clone(), leaf(value.clone())), true),
                    Show::Label => (labeled(name.clone(), pruned(leaf_hash(value))), true),
                    Show::Hide => (pruned(labeled_hash(name, &leaf_hash(value))), false),
                }
            }
            n => {
                let mid = lo + n / 2;
                let (left, shown_left) = build(names, lo, mid, show);
                let (right, shown_right) = build(names, mid, hi, show);
                if shown_left || shown_right {
                    (fork(left, right), true)
                } else {
                    (pruned(fork_hash(&left.digest(), &right.digest())), false)
                }
            }
        }
    }
    build(names, 0, names.len(), show).0
}

impl<M: Memory> NameTree<M> {
    pub fn new(nodes: M) -> Self {
        Self { nodes }
    }

    // None is an Empty subtree, stored as zero bytes.
    fn get(&self, index: u64) -> Option<Hash> {
        let offset = index * 32;
        if offset + 32 > self.nodes.size() * 65_536 {
            return None;
        }
        let mut hash = [0; 32];
        self.nodes.read(offset, &mut hash);
        (hash != [0; 32]).then_some(hash)
    }

    fn hash(&self, index: u64) -> Hash {
        self.get(index).unwrap_or_else(empty_hash)
    }

    fn set(&mut self, index: u64, hash: Option<Hash>) {
        let size = self.nodes.size();
        if size < PAGES {
            assert!(self.nodes.grow(PAGES - size) >= 0, "name tree memory");
        }
        self.nodes.write(index * 32, &hash.unwrap_or([0; 32]));
    }

    pub fn root_hash(&self) -> Hash {
        self.hash(1)
    }

    /// Rehash `bucket` from all of its names, then every node above it.
    pub fn update(&mut self, bucket: u32, names: &Bucket) {
        let mut index = (1 << BITS) | u64::from(bucket);
        let mut node = (!names.is_empty()).then(|| subtree(names, &|_| Show::Hide).digest());
        self.set(index, node);
        while index > 1 {
            let sibling = self.get(index ^ 1);
            node = (node.is_some() || sibling.is_some()).then(|| {
                let (here, there) = (
                    node.unwrap_or_else(empty_hash),
                    sibling.unwrap_or_else(empty_hash),
                );
                if index & 1 == 0 {
                    branch_hash(&here, &there)
                } else {
                    branch_hash(&there, &here)
                }
            });
            index >>= 1;
            self.set(index, node);
        }
    }

    /// Witness for `name`, given all names of its bucket: the leaf when present,
    /// otherwise the neighbouring labels or the Empty subtree that proves absence.
    pub fn witness(&self, bucket: u32, names: &Bucket, name: &[u8]) -> HashTree {
        let target = (1 << BITS) | u64::from(bucket);
        let mut depth = 0;
        let mut index = 1;
        let mut tree = loop {
            if self.get(index).is_none() {
                break empty();
            }
            if depth == BITS {
                break match names.binary_search_by(|(n, _)| n.as_slice().cmp(name)) {
                    Ok(i) => subtree(names, &|j| if j == i { Show::Leaf } else { Show::Hide }),
                    Err(i) => subtree(names, &|j| {
                        if j + 1 == i || j == i {
                            Show::Label
                        } else {
                            Show::Hide
                        }
                    }),
                };
            }
            depth += 1;
            index = target >> (BITS - depth);
        };
        while index > 1 {
            let bit = (index & 1) as u8;
            let here = labeled(vec![bit], tree);
            let there = pruned(labeled_hash(&[bit ^ 1], &self.hash(index ^ 1)));
            tree = if bit == 0 {
                fork(here, there)
            } else {
                fork(there, here)
            };
            index >>= 1;
        }
        tree
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ic_certification::LookupResult;
    use ic_stable_structures::VectorMemory;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Fixture {
        tree: Option<NameTree<VectorMemory>>,
        names: BTreeMap<(u32, Vec<u8>), Vec<u8>>,
    }

    impl Fixture {
        fn tree(&mut self) -> &mut NameTree<VectorMemory> {
            self.tree
                .get_or_insert_with(|| NameTree::new(VectorMemory::default()))
        }

        fn bucket(&self, bucket: u32) -> Vec<(Vec<u8>, Vec<u8>)> {
            let end = (bucket + 1, vec![]);
            self.names
                .range((bucket, vec![])..end)
                .map(|((_, name), value)| (name.clone(), value.clone()))
                .collect()
        }

        fn put(&mut self, bucket: u32, name: &str, value: &[u8]) {
            self.names
                .insert((bucket, name.as_bytes().to_vec()), value.to_vec());
            let names = self.bucket(bucket);
            self.tree().update(bucket, &names);
        }

        // Some(value) when found, None when proven absent.
        fn prove(&mut self, bucket: u32, name: &str) -> Option<Vec<u8>> {
            let names = self.bucket(bucket);
            let tree = self.tree();
            let witness = tree.witness(bucket, &names, name.as_bytes());
            assert_eq!(witness.digest(), tree.root_hash(), "{name}");
            // The witness must survive the CBOR encoding of CertifiedBatch.
            let witness: HashTree = cbor2::from_slice(&cbor2::to_vec(&witness).unwrap()).unwrap();
            let path: Vec<Vec<u8>> = (0..BITS)
                .map(|i| vec![((bucket >> (BITS - 1 - i)) & 1) as u8])
                .chain([name.as_bytes().to_vec()])
                .collect();
            match witness.lookup_path(path) {
                LookupResult::Found(value) => Some(value.to_vec()),
                LookupResult::Absent => None,
                other => panic!("{name}: {other:?}"),
            }
        }
    }

    #[test]
    fn witnesses_prove_values_and_absence_against_the_root() {
        let mut f = Fixture::default();
        // An unused tree is the Empty tree and proves every name absent.
        assert_eq!(f.tree().root_hash(), empty_hash());
        assert_eq!(f.prove(7, "alice"), None);
        let last = (1 << BITS) - 1;
        for (bucket, name) in [(7, "b"), (7, "d"), (7, "f"), (8, "x"), (last, "z")] {
            f.put(bucket, name, name.as_bytes());
        }
        for (bucket, name) in [(7, "b"), (7, "d"), (7, "f"), (8, "x"), (last, "z")] {
            assert_eq!(f.prove(bucket, name), Some(name.as_bytes().to_vec()));
        }
        // Absent before, between and after a bucket's names, in an empty bucket
        // beside a used one, and in an empty subtree.
        for (bucket, name) in [
            (7, "a"),
            (7, "c"),
            (7, "e"),
            (7, "g"),
            (9, "x"),
            (1 << 19, "y"),
        ] {
            assert_eq!(f.prove(bucket, name), None, "{bucket} {name}");
        }
        // A rewrite changes the root; rehashing an unchanged bucket keeps it.
        let root = f.tree().root_hash();
        f.put(7, "d", b"moved");
        assert_ne!(f.tree().root_hash(), root);
        assert_eq!(f.prove(7, "d"), Some(b"moved".to_vec()));
        let root = f.tree().root_hash();
        let names = f.bucket(8);
        f.tree().update(8, &names);
        assert_eq!(f.tree().root_hash(), root);
    }
}
