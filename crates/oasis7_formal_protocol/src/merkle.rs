//! Shared fixed-depth sparse Merkle traversal. Codecs own identity and policy;
//! this layer neither grants authority nor decides whether a leaf is mutable.
use std::collections::{BTreeMap, BTreeSet};
use std::marker::PhantomData;

pub(crate) const DEPTH: usize = 256;

pub(crate) enum Node<L> {
    Branch {
        depth: usize,
        left: String,
        right: String,
    },
    Leaf(L),
}

pub(crate) trait Codec {
    type Leaf: Clone;
    const MAX_NODE_BYTES: usize;
    fn key(leaf: &Self::Leaf) -> Result<[u8; 32], String>;
    fn encode(node: Node<Self::Leaf>) -> Result<Vec<u8>, String>;
    fn decode(bytes: &[u8]) -> Result<Node<Self::Leaf>, String>;
}

/// Sources must reserve and bound allocation before loading canonical bytes.
/// Every read is also checked against the bound and its content digest here.
pub trait MerkleNodeReader {
    /// The carrier can keep a reservation/reader lease alive through decoding.
    /// Legacy adapters may continue returning their original owned Vec.
    type Node: AsRef<[u8]>;
    /// Concrete charged readers expose their class for phase-specific admission.
    fn allocation_class(&self) -> Option<oasis7_proto::memory_budget::MemoryClass> {
        None
    }
    /// Concrete IWS stores expose their exact pool for producer admission.
    /// Legacy sources retain their caller-owned workspace contract.
    fn allocation_budget(&self) -> Option<&oasis7_proto::memory_budget::MemoryBudget> {
        None
    }
    fn read_node(&self, digest: &str, max_bytes: usize) -> Result<Option<Self::Node>, String>;
}

pub(crate) struct Updater<'a, C: Codec, R> {
    reader: &'a R,
    pub(crate) pending: BTreeMap<String, Vec<u8>>,
    parent_nodes: Option<BTreeSet<String>>,
    empty: &'a [String],
    pending_bytes: usize,
    max_pending_bytes: usize,
    codec: PhantomData<C>,
}

impl<'a, C: Codec, R: MerkleNodeReader> Updater<'a, C, R> {
    pub(crate) fn new(reader: &'a R, empty: &'a [String], max_pending_bytes: usize) -> Self {
        Self {
            reader,
            empty,
            pending: BTreeMap::new(),
            parent_nodes: None,
            pending_bytes: 0,
            max_pending_bytes,
            codec: PhantomData,
        }
    }

    /// Only final-new-node construction needs this set. Point readers and the
    /// historical business v1 writer must not pay for tracking old paths.
    pub(crate) fn track_parent_nodes(mut self) -> Self {
        self.parent_nodes = Some(BTreeSet::new());
        self
    }

    fn read(&mut self, digest: &str, depth: usize) -> Result<Option<Node<C::Leaf>>, String> {
        if digest == self.empty[depth] {
            return Ok(None);
        }
        if let Some(bytes) = self.pending.get(digest) {
            Self::decode(bytes, digest, depth)
        } else {
            let bytes = self
                .reader
                .read_node(digest, C::MAX_NODE_BYTES)?
                .ok_or("Merkle path node missing")?;
            if let Some(parent) = &mut self.parent_nodes
                && !parent.contains(digest)
            {
                parent.insert(digest.to_owned());
            }
            Self::decode(bytes.as_ref(), digest, depth)
        }
    }

    fn decode(bytes: &[u8], digest: &str, depth: usize) -> Result<Option<Node<C::Leaf>>, String> {
        if bytes.len() > C::MAX_NODE_BYTES {
            return Err("Merkle source node byte bound".into());
        }
        if blake3::hash(bytes).to_hex().as_str() != digest {
            return Err("Merkle node content hash mismatch".into());
        }
        let node = C::decode(bytes)?;
        match &node {
            Node::Branch { depth: stored, .. } if *stored == depth && depth < DEPTH => {}
            Node::Leaf(_) if depth == DEPTH => {}
            _ => return Err("Merkle node at wrong depth".into()),
        }
        Ok(Some(node))
    }

    /// Complete fixed-root scan. The caller admits bounded DFS metadata and
    /// governs source reads and visitor work. No growing visited/node map.
    pub(crate) fn visit_leaves<E>(
        &mut self,
        root: &str,
        mut visit: impl FnMut(&C::Leaf) -> Result<(), E>,
    ) -> Result<u64, VisitError<E>> {
        let mut stack = Vec::with_capacity(DEPTH + 1);
        stack.push((root.to_owned(), 0usize, [0u8; 32]));
        let mut count = 0u64;
        while let Some((id, depth, path)) = stack.pop() {
            let Some(node) = self.read(&id, depth).map_err(VisitError::Tree)? else {
                continue;
            };
            match node {
                Node::Branch { left, right, .. } => {
                    let mut right_path = path;
                    right_path[depth / 8] |= 0x80 >> (depth % 8);
                    stack.push((right, depth + 1, right_path));
                    stack.push((left, depth + 1, path));
                }
                Node::Leaf(leaf) => {
                    if C::key(&leaf).map_err(VisitError::Tree)? != path {
                        return Err(VisitError::Tree("Merkle scan key/path mismatch".into()));
                    }
                    visit(&leaf).map_err(VisitError::Visitor)?;
                    count = count
                        .checked_add(1)
                        .ok_or_else(|| VisitError::Tree("Merkle scan leaf overflow".into()))?;
                }
            }
        }
        Ok(count)
    }

    pub(crate) fn lookup(&mut self, root: &str, key: &[u8; 32]) -> Result<Option<C::Leaf>, String> {
        let mut current = root.to_owned();
        for depth in 0..=DEPTH {
            let Some(node) = self.read(&current, depth)? else {
                return Ok(None);
            };
            match node {
                Node::Branch { left, right, .. } => {
                    current = if key_bit(key, depth) { right } else { left };
                }
                Node::Leaf(leaf) => {
                    if C::key(&leaf)? != *key {
                        return Err("Merkle key digest collision".into());
                    }
                    return Ok(Some(leaf));
                }
            }
        }
        Err("Merkle path depth".into())
    }

    pub(crate) fn insert(
        &mut self,
        root: &str,
        depth: usize,
        leaf: &C::Leaf,
    ) -> Result<(String, bool), String> {
        let key = C::key(leaf)?;
        let (root, old) = self.replace(root, depth, &key, Some(leaf))?;
        Ok((root, old.is_none()))
    }

    /// Empty subtrees collapse to codec-defined sentinels; no sentinel objects
    /// are persisted. The 256-level update path stays on the heap.
    pub(crate) fn replace(
        &mut self,
        root: &str,
        depth: usize,
        key: &[u8; 32],
        leaf: Option<&C::Leaf>,
    ) -> Result<(String, Option<C::Leaf>), String> {
        if depth > DEPTH || leaf.is_some_and(|leaf| C::key(leaf).ok().as_ref() != Some(key)) {
            return Err("Merkle replacement key/depth".into());
        }
        let mut current = root.to_owned();
        let mut path = Vec::with_capacity(DEPTH - depth);
        for level in depth..DEPTH {
            let (left, right) = match self.read(&current, level)? {
                Some(Node::Branch { left, right, .. }) => (left, right),
                None => (self.empty[level + 1].clone(), self.empty[level + 1].clone()),
                _ => return Err("Merkle leaf before terminal depth".into()),
            };
            let right_selected = key_bit(key, level);
            current = if right_selected {
                right.clone()
            } else {
                left.clone()
            };
            path.push((level, left, right, right_selected));
        }
        let old = match self.read(&current, DEPTH)? {
            Some(Node::Leaf(old)) if C::key(&old)? == *key => Some(old),
            None => None,
            _ => return Err("Merkle replacement leaf mismatch".into()),
        };
        let mut digest = match leaf {
            Some(leaf) => self.store(Node::Leaf(leaf.clone()))?,
            None => self.empty[DEPTH].clone(),
        };
        for (level, mut left, mut right, right_selected) in path.into_iter().rev() {
            if right_selected {
                right = digest;
            } else {
                left = digest;
            }
            digest = if left == self.empty[level + 1] && right == self.empty[level + 1] {
                self.empty[level].clone()
            } else {
                self.store(Node::Branch {
                    depth: level,
                    left,
                    right,
                })?
            };
        }
        Ok((digest, old))
    }

    fn store(&mut self, node: Node<C::Leaf>) -> Result<String, String> {
        let bytes = C::encode(node)?;
        let digest = blake3::hash(&bytes).to_hex().to_string();
        if !self.pending.contains_key(&digest) {
            let next = self
                .pending_bytes
                .checked_add(bytes.len())
                .filter(|total| *total <= self.max_pending_bytes)
                .ok_or("ResourceLimited: Merkle transaction pending bytes")?;
            self.pending.insert(digest.clone(), bytes);
            self.pending_bytes = next;
        }
        Ok(digest)
    }

    /// Emit only final reachable nodes absent from the authenticated parent
    /// paths. Unrelated CAS hits must never affect a deterministic delta.
    pub(crate) fn final_writes(&self, root: &str) -> Result<BTreeMap<String, Vec<u8>>, String> {
        let parent_nodes = self
            .parent_nodes
            .as_ref()
            .ok_or("Merkle parent tracking not enabled")?;
        let mut result = BTreeMap::new();
        let mut stack = vec![(root.to_owned(), 0)];
        while let Some((digest, depth)) = stack.pop() {
            if digest == self.empty[depth] || parent_nodes.contains(&digest) {
                continue;
            }
            let Some(bytes) = self.pending.get(&digest) else {
                continue;
            };
            if result.contains_key(&digest) {
                continue;
            }
            match C::decode(bytes)? {
                Node::Branch {
                    depth: stored,
                    left,
                    right,
                } if stored == depth && depth < DEPTH => {
                    stack.push((left, depth + 1));
                    stack.push((right, depth + 1));
                }
                Node::Leaf(_) if depth == DEPTH => {}
                _ => return Err("Merkle final node depth".into()),
            }
            result.insert(digest, bytes.clone());
        }
        Ok(result)
    }
}

fn key_bit(key: &[u8; 32], depth: usize) -> bool {
    key[depth / 8] & (0x80 >> (depth % 8)) != 0
}

pub(crate) fn build_empty_hashes<C: Codec>(empty_leaf: &[u8]) -> Result<Vec<String>, String> {
    let mut hashes = vec![String::new(); DEPTH + 1];
    hashes[DEPTH] = blake3::hash(empty_leaf).to_hex().to_string();
    for depth in (0..DEPTH).rev() {
        hashes[depth] = blake3::hash(&C::encode(Node::Branch {
            depth,
            left: hashes[depth + 1].clone(),
            right: hashes[depth + 1].clone(),
        })?)
        .to_hex()
        .to_string();
    }
    Ok(hashes)
}

#[derive(Debug)]
pub(crate) enum VisitError<E> {
    Tree(String),
    Visitor(E),
}
