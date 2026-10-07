//! Certified map whose Merkle structure lives in stable memory.
//!
//! Every key is certified as `labeled(key, child)` in byte order, so a witness
//! answers `lookup_path([key])` exactly like a heap `RbTree` map and clients
//! need no bucket labels. The structure is a crit-bit trie: an internal node is
//! `fork(left, right)`, split at the first bit where its keys differ. Each key
//! byte contributes a presence bit and then its eight bits, so a key sorts
//! before its extensions. The shape depends only on the key set: a write
//! rehashes one root-to-leaf path, an upgrade only republishes the root, and a
//! witness nests about log2(n) + 2 levels for well-spread keys.
//!
//! Internal nodes live in fixed-size slots addressed by node id, so walking a
//! path reads one slot per level instead of searching a B-tree. Slots of deleted
//! nodes form a free list and are reused. Keys are at most `MAX_KEY` bytes.
//!
//! The map stores hashes only. Callers keep the certified values in their own
//! records and supply them again when building a witness, which traps when a
//! value no longer matches its hash. Upgrades do not recertify leaves, so a
//! change to how a caller encodes a certified value must recertify every
//! affected leaf or come with a new stable schema.
use crate::certified::{certified_batch, query_certificate};
use dmsg_types::*;
use ic_certification::hash_tree::{fork_hash, labeled_hash};
use ic_certification::{empty, fork, labeled, leaf, pruned, Hash, HashTree};
use ic_stable_structures::{Memory, StableBTreeMap};

pub use ic_certification::hash_tree::leaf_hash;

/// Longest certified key, stored inline in its parent's slot.
pub const MAX_KEY: usize = 64;
// A child is its kind, key length, key or node id, and hash.
const CHILD: usize = 1 + 1 + MAX_KEY + 32;
// A slot is a tag, a crit bit and two children; the meta record fits too.
const SLOT: usize = 1 + 4 + 2 * CHILD;
const PAGE: u64 = 65_536;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Ref {
    Leaf(Vec<u8>),
    Node(u64),
}

/// A subtree reference with the hash of the subtree it names.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Child {
    at: Ref,
    hash: Hash,
}

/// Slot 0: the root, the next unused slot and the head of the free list.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Meta {
    root: Option<Child>,
    next: u64,
    free: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Slot {
    Meta(Meta),
    Node { crit: u32, children: [Child; 2] },
    Free { next: u64 },
}

// Zeroed memory reads as an empty map.
const EMPTY: u8 = 0;
const META: u8 = 1;
const NODE: u8 = 2;
const FREE: u8 = 3;

fn put_child(out: &mut [u8], c: &Child) {
    match &c.at {
        Ref::Leaf(key) => {
            out[1] = key.len() as u8;
            out[2..2 + key.len()].copy_from_slice(key);
        }
        Ref::Node(id) => {
            out[0] = 1;
            out[2..10].copy_from_slice(&id.to_be_bytes());
        }
    }
    out[CHILD - 32..CHILD].copy_from_slice(&c.hash);
}

fn take_child(bytes: &[u8]) -> Child {
    let at = match bytes[0] {
        0 => Ref::Leaf(bytes[2..2 + usize::from(bytes[1])].to_vec()),
        _ => Ref::Node(u64::from_be_bytes(bytes[2..10].try_into().expect("id"))),
    };
    let hash = bytes[CHILD - 32..CHILD].try_into().expect("hash");
    Child { at, hash }
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_be_bytes(bytes[at..at + 8].try_into().expect("u64"))
}

fn encode(slot: &Slot) -> [u8; SLOT] {
    let mut out = [0; SLOT];
    match slot {
        Slot::Meta(m) => {
            out[0] = META;
            out[1..9].copy_from_slice(&m.next.to_be_bytes());
            out[9..17].copy_from_slice(&m.free.to_be_bytes());
            if let Some(root) = &m.root {
                out[17] = 1;
                put_child(&mut out[18..18 + CHILD], root);
            }
        }
        Slot::Node { crit, children } => {
            out[0] = NODE;
            out[1..5].copy_from_slice(&crit.to_be_bytes());
            put_child(&mut out[5..5 + CHILD], &children[0]);
            put_child(&mut out[5 + CHILD..SLOT], &children[1]);
        }
        Slot::Free { next } => {
            out[0] = FREE;
            out[1..9].copy_from_slice(&next.to_be_bytes());
        }
    }
    out
}

fn decode(bytes: &[u8; SLOT]) -> Slot {
    match bytes[0] {
        EMPTY => Slot::Meta(Meta {
            root: None,
            next: 1,
            free: 0,
        }),
        META => Slot::Meta(Meta {
            next: u64_at(bytes, 1),
            free: u64_at(bytes, 9),
            root: (bytes[17] == 1).then(|| take_child(&bytes[18..18 + CHILD])),
        }),
        NODE => Slot::Node {
            crit: u32::from_be_bytes(bytes[1..5].try_into().expect("crit")),
            children: [
                take_child(&bytes[5..5 + CHILD]),
                take_child(&bytes[5 + CHILD..SLOT]),
            ],
        },
        _ => Slot::Free {
            next: u64_at(bytes, 1),
        },
    }
}

// Bit `i` of a key: bit 9p is 1 while byte p exists, bits 9p+1..=9p+8 are
// that byte's bits from the most significant. Absent bytes read as zero.
fn bit(key: &[u8], i: u32) -> usize {
    let (byte, offset) = ((i / 9) as usize, i % 9);
    match key.get(byte) {
        None => 0,
        Some(_) if offset == 0 => 1,
        Some(b) => usize::from((b >> (8 - offset)) & 1),
    }
}

// The first bit where two distinct keys differ.
fn first_diff(a: &[u8], b: &[u8]) -> u32 {
    let p = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    match (a.get(p), b.get(p)) {
        (Some(x), Some(y)) => 9 * p as u32 + (x ^ y).leading_zeros() + 1,
        _ => 9 * p as u32,
    }
}

/// What a witness shows of one revealed key.
enum Show {
    Child(HashTree),
    Label,
}

/// An internal node on a key's path, as read, with the side the key takes.
struct Step {
    id: u64,
    crit: u32,
    side: usize,
    children: [Child; 2],
}

pub struct CertMap<M: Memory> {
    /// Key -> hash of its labeled child, in byte order.
    leaves: StableBTreeMap<Vec<u8>, [u8; 32], M>,
    /// Slot 0 holds the meta record; other slots hold internal nodes by id.
    nodes: M,
}

impl<M: Memory> CertMap<M> {
    pub fn new(leaves: M, nodes: M) -> Self {
        Self {
            leaves: StableBTreeMap::init(leaves),
            nodes,
        }
    }

    fn read(&self, id: u64) -> Slot {
        let offset = id * SLOT as u64;
        let mut bytes = [0; SLOT];
        if offset + SLOT as u64 <= self.nodes.size() * PAGE {
            self.nodes.read(offset, &mut bytes);
        }
        decode(&bytes)
    }

    fn write(&mut self, id: u64, slot: &Slot) {
        let end = (id + 1) * SLOT as u64;
        let pages = end.div_ceil(PAGE);
        if pages > self.nodes.size() {
            assert!(
                self.nodes.grow(pages - self.nodes.size()) >= 0,
                "certified map memory"
            );
        }
        self.nodes.write(id * SLOT as u64, &encode(slot));
    }

    fn meta(&self) -> Meta {
        match self.read(0) {
            Slot::Meta(m) => m,
            _ => panic!("certified map meta"),
        }
    }

    fn node(&self, id: u64) -> (u32, [Child; 2]) {
        match self.read(id) {
            Slot::Node { crit, children } => (crit, children),
            _ => panic!("certified map node {id}"),
        }
    }

    // Take a free slot, or the next unused one.
    fn allocate(&self, meta: &mut Meta) -> u64 {
        if meta.free == 0 {
            meta.next += 1;
            return meta.next - 1;
        }
        let id = meta.free;
        match self.read(id) {
            Slot::Free { next } => meta.free = next,
            _ => panic!("certified map free slot {id}"),
        }
        id
    }

    pub fn root_hash(&self) -> Hash {
        self.meta()
            .root
            .map_or_else(|| empty().digest(), |root| root.hash)
    }

    pub fn len(&self) -> u64 {
        self.leaves.len()
    }

    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    /// Hash of the child certified under `key`.
    pub fn get(&self, key: &[u8]) -> Option<Hash> {
        self.leaves.get(&key.to_vec())
    }

    /// Publish the root as this canister's certified data.
    pub fn publish(&self) {
        #[cfg(target_arch = "wasm32")]
        ic_cdk::api::certified_data_set(self.root_hash());
    }

    /// Certify `leaf(value)` under `key` and publish the root.
    pub fn insert(&mut self, key: Vec<u8>, value: &[u8]) {
        if self.set(key, leaf_hash(value)) {
            self.publish();
        }
    }

    /// Remove `key` and publish the root.
    pub fn remove(&mut self, key: &[u8]) {
        if self.delete(key) {
            self.publish();
        }
    }

    // Internal nodes from `root` along `key`'s bits, read once, and the child
    // reached at the bottom. Crit bits increase along the path.
    fn walk(&self, root: Option<Child>, key: &[u8]) -> (Vec<Step>, Option<Child>) {
        let mut steps = vec![];
        let mut at = root;
        while let Some(Child {
            at: Ref::Node(id), ..
        }) = at
        {
            let (crit, children) = self.node(id);
            let side = bit(key, crit);
            at = Some(children[side].clone());
            steps.push(Step {
                id,
                crit,
                side,
                children,
            });
        }
        (steps, at)
    }

    // Store `child` below the last of `steps`, rehash every node above it and
    // record the new root in `meta`.
    fn rehash(&mut self, steps: Vec<Step>, mut child: Option<Child>, mut meta: Meta) {
        for Step {
            id,
            crit,
            side,
            mut children,
        } in steps.into_iter().rev()
        {
            children[side] = child.expect("a node keeps both children");
            let hash = fork_hash(&children[0].hash, &children[1].hash);
            self.write(id, &Slot::Node { crit, children });
            child = Some(Child {
                at: Ref::Node(id),
                hash,
            });
        }
        meta.root = child;
        self.write(0, &Slot::Meta(meta));
    }

    /// Certify a child with hash `child` under `key`, without publishing.
    /// Returns false when the key already certifies the same hash.
    pub fn set(&mut self, key: Vec<u8>, child: Hash) -> bool {
        assert!(key.len() <= MAX_KEY, "certified key longer than {MAX_KEY}");
        let old = self.leaves.insert(key.clone(), child);
        if old == Some(child) {
            return false;
        }
        let leaf = Child {
            at: Ref::Leaf(key.clone()),
            hash: labeled_hash(&key, &child),
        };
        let mut meta = self.meta();
        let (mut steps, bottom) = self.walk(meta.root.clone(), &key);
        let closest = match (old, bottom) {
            // A changed key keeps its place; the first key becomes the root.
            (Some(_), _) | (None, None) => {
                self.rehash(steps, Some(leaf), meta);
                return true;
            }
            (None, Some(closest)) => closest,
        };
        // A new key splits off at the first bit where it differs from the
        // closest key, above the first node on its path that splits later.
        let Ref::Leaf(closest) = &closest.at else {
            unreachable!()
        };
        let d = first_diff(&key, closest);
        let kept = steps.partition_point(|step| step.crit < d);
        let below = match kept.checked_sub(1) {
            Some(i) => steps[i].children[steps[i].side].clone(),
            None => meta.root.clone().expect("a non-empty map"),
        };
        steps.truncate(kept);
        let mut children = [below.clone(), below];
        children[bit(&key, d)] = leaf;
        let hash = fork_hash(&children[0].hash, &children[1].hash);
        let id = self.allocate(&mut meta);
        self.write(id, &Slot::Node { crit: d, children });
        let node = Child {
            at: Ref::Node(id),
            hash,
        };
        self.rehash(steps, Some(node), meta);
        true
    }

    /// Remove `key` without publishing; false when it is not certified.
    pub fn delete(&mut self, key: &[u8]) -> bool {
        if self.leaves.remove(&key.to_vec()).is_none() {
            return false;
        }
        let mut meta = self.meta();
        let (mut steps, _) = self.walk(meta.root.clone(), key);
        // The leaf's parent is replaced by the leaf's sibling; its slot is freed.
        let sibling = steps.pop().map(|parent| {
            self.write(parent.id, &Slot::Free { next: meta.free });
            meta.free = parent.id;
            parent.children[1 - parent.side].clone()
        });
        self.rehash(steps, sibling, meta);
        true
    }

    fn reveal(&self, at: &Child, targets: Vec<(Vec<u8>, Show)>) -> HashTree {
        if targets.is_empty() {
            return pruned(at.hash);
        }
        match &at.at {
            Ref::Leaf(key) => {
                let (_, show) = targets.into_iter().next().expect("one target");
                match show {
                    Show::Child(child) => labeled(key.clone(), child),
                    Show::Label => {
                        labeled(key.clone(), pruned(self.get(key).expect("certified leaf")))
                    }
                }
            }
            Ref::Node(id) => {
                let (crit, children) = self.node(*id);
                let (right, left): (Vec<_>, Vec<_>) =
                    targets.into_iter().partition(|(k, _)| bit(k, crit) == 1);
                fork(
                    self.reveal(&children[0], left),
                    self.reveal(&children[1], right),
                )
            }
        }
    }

    /// Witness revealing each of `keys`: its child when certified, built by
    /// `child` and checked against the certified hash, otherwise the
    /// neighbouring labels that prove its absence.
    pub fn witness(&self, keys: &[&[u8]], mut child: impl FnMut(&[u8]) -> HashTree) -> HashTree {
        let Some(root) = self.meta().root else {
            return empty();
        };
        let mut targets = std::collections::BTreeMap::new();
        for key in keys {
            if let Some(hash) = self.get(key) {
                let child = child(key);
                assert_eq!(child.digest(), hash, "certified value changed");
                targets.insert(key.to_vec(), Show::Child(child));
                continue;
            }
            let key = key.to_vec();
            let before = self.leaves.range(..key.clone()).next_back();
            let after = self.leaves.range(key..).next();
            for neighbour in before.into_iter().chain(after) {
                targets
                    .entry(neighbour.key().clone())
                    .or_insert(Show::Label);
            }
        }
        self.reveal(&root, targets.into_iter().collect())
    }

    /// Certified batch of leaf values. `value` returns the current value of a
    /// certified key; keys it does not certify get absence proofs.
    pub fn batch(
        &self,
        canister: candid::Principal,
        keys: Vec<Vec<u8>>,
        mut value: impl FnMut(&[u8]) -> Option<Vec<u8>>,
    ) -> Result<CertifiedBatch> {
        certified_batch(canister, keys, query_certificate()?, |key| {
            // The witness asks for the value only when the key is certified.
            let mut bytes = None;
            let witness = self.witness(&[key], |key| {
                let certified = value(key).expect("certified record");
                bytes = Some(certified.clone());
                leaf(certified)
            });
            (bytes, witness)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ic_certification::LookupResult;
    use ic_stable_structures::VectorMemory;
    use std::collections::BTreeMap;

    fn map() -> CertMap<VectorMemory> {
        CertMap::new(VectorMemory::default(), VectorMemory::default())
    }

    // Independent hash of the crit-bit tree over sorted (key, value) pairs.
    fn reference(entries: &[(Vec<u8>, Vec<u8>)]) -> Hash {
        match entries {
            [] => empty().digest(),
            [(k, v)] => labeled_hash(k, &leaf_hash(v)),
            _ => {
                let d = first_diff(&entries[0].0, &entries[entries.len() - 1].0);
                let split = entries.partition_point(|(k, _)| bit(k, d) == 0);
                fork_hash(&reference(&entries[..split]), &reference(&entries[split..]))
            }
        }
    }

    fn depth(node: &ic_certification::hash_tree::HashTreeNode<Vec<u8>>) -> usize {
        use ic_certification::hash_tree::HashTreeNode::*;
        match node {
            Fork(f) => 1 + depth(&f.0).max(depth(&f.1)),
            Labeled(_, t) => 1 + depth(t),
            _ => 1,
        }
    }

    fn check(m: &CertMap<VectorMemory>, expected: &BTreeMap<Vec<u8>, Vec<u8>>, probes: &[Vec<u8>]) {
        let entries: Vec<_> = expected.clone().into_iter().collect();
        assert_eq!(m.root_hash(), reference(&entries));
        assert_eq!(m.len(), entries.len() as u64);
        for key in probes.iter().chain(expected.keys()) {
            let value = expected.get(key);
            let witness = m.witness(&[key.as_slice()], |_| leaf(value.unwrap().clone()));
            // Witnesses must survive the CBOR encoding of CertifiedBatch.
            let witness: HashTree = cbor2::from_slice(&cbor2::to_vec(&witness).unwrap()).unwrap();
            assert_eq!(witness.digest(), m.root_hash());
            match (witness.lookup_path([key.as_slice()]), value) {
                (LookupResult::Found(found), Some(v)) => assert_eq!(found, v.as_slice()),
                (LookupResult::Absent, None) => {}
                (other, _) => panic!("{key:?}: {other:?}"),
            }
        }
    }

    #[test]
    fn witnesses_prove_values_and_absence_in_byte_order() {
        let mut m = map();
        let mut expected = BTreeMap::new();
        let probes: Vec<Vec<u8>> = [
            &b""[..],
            b"\0",
            b"a",
            b"ab",
            b"abc",
            b"abd",
            b"b",
            b"\xff\xff",
        ]
        .iter()
        .map(|k| k.to_vec())
        .collect();
        check(&m, &expected, &probes);
        // Prefixes sort before their extensions; zero bytes are not padding.
        for key in [&b"ab"[..], b"a", b"abc", b"a\0", b"b", b"", b"\x80", b"abd"] {
            m.insert(key.to_vec(), key);
            expected.insert(key.to_vec(), key.to_vec());
            check(&m, &expected, &probes);
        }
        m.insert(b"ab".to_vec(), b"changed");
        expected.insert(b"ab".to_vec(), b"changed".to_vec());
        check(&m, &expected, &probes);
        let root = m.root_hash();
        assert!(!m.set(b"ab".to_vec(), leaf_hash(b"changed")));
        assert_eq!(m.root_hash(), root);
        for key in [
            &b"a"[..],
            b"abd",
            b"",
            b"zz",
            b"b",
            b"ab",
            b"abc",
            b"a\0",
            b"\x80",
        ] {
            m.remove(key);
            expected.remove(key);
            check(&m, &expected, &probes);
        }
        assert!(m.is_empty());
        assert_eq!(m.root_hash(), empty().digest());
    }

    #[test]
    #[should_panic(expected = "certified key longer than 64")]
    fn keys_longer_than_a_slot_are_refused() {
        map().insert(vec![1; MAX_KEY + 1], b"value");
    }

    #[test]
    fn longest_keys_survive_their_slot_encoding() {
        let mut m = map();
        let mut expected = BTreeMap::new();
        for n in 0u8..8 {
            let key = [vec![n; 1], vec![255; MAX_KEY - 1]].concat();
            m.insert(key.clone(), &key);
            expected.insert(key.clone(), key);
        }
        check(&m, &expected, &[vec![255; MAX_KEY]]);
    }

    #[test]
    fn one_witness_reveals_several_keys() {
        let mut m = map();
        for key in [&b"b"[..], b"d", b"f", b"h"] {
            m.insert(key.to_vec(), key);
        }
        let keys: [&[u8]; 4] = [b"d", b"e", b"a", b"h"];
        let witness = m.witness(&keys, |k| leaf(k.to_vec()));
        assert_eq!(witness.digest(), m.root_hash());
        for (key, expected) in [
            (&b"d"[..], LookupResult::Found(&b"d"[..])),
            (b"h", LookupResult::Found(b"h")),
            (b"e", LookupResult::Absent),
            (b"a", LookupResult::Absent),
        ] {
            assert_eq!(witness.lookup_path([key]), expected);
        }
        // Neighbours stay pruned values; unrevealed keys stay unknown.
        assert_eq!(witness.lookup_path([b"f"]), LookupResult::Unknown);
    }

    #[test]
    fn shape_depends_only_on_the_keys_and_stays_shallow() {
        let keys: Vec<Vec<u8>> = (0u32..2_000)
            .map(|i| dmsg_protocol::sha256(&i.to_be_bytes())[..12].to_vec())
            .collect();
        let mut forward = map();
        let mut backward = map();
        for key in &keys {
            forward.insert(key.clone(), key);
        }
        for key in keys.iter().rev() {
            backward.insert(key.clone(), key);
        }
        assert_eq!(forward.root_hash(), backward.root_hash());
        let deepest = keys
            .iter()
            .map(|k| {
                depth(
                    forward
                        .witness(&[k.as_slice()], |_| leaf(k.clone()))
                        .as_ref(),
                )
            })
            .max()
            .unwrap();
        assert!(deepest <= 32, "witness depth {deepest}");
        // Removing half the keys leaves the tree of the other half.
        let mut half = map();
        for (i, key) in keys.iter().enumerate() {
            if i % 2 == 0 {
                forward.remove(key);
            } else {
                half.insert(key.clone(), key);
            }
        }
        assert_eq!(forward.root_hash(), half.root_hash());
        // Freed slots are reused: adding the removed keys back allocates none.
        let next = forward.meta().next;
        for (i, key) in keys.iter().enumerate() {
            if i % 2 == 0 {
                forward.insert(key.clone(), key);
            }
        }
        assert_eq!(forward.meta().next, next);
        assert_eq!(forward.meta().free, 0);
        assert_eq!(forward.root_hash(), backward.root_hash());
    }
}
