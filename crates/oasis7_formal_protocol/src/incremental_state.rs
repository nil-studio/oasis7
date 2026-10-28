//! Mutable state-page adapter for new-format worlds. These roots provide
//! integrity only; a caller must authenticate the owning world/commit and its
//! complete RootSet with the original authority before using this view.
pub use crate::merkle::MerkleNodeReader;
use crate::merkle::{Codec, DEPTH, Updater};

#[path = "incremental_state_codec.rs"]
mod codec;
use codec::{StateCodec, empty_hashes, key_digest};

pub const STATE_MAX_KEY_BYTES: usize = 1024;
pub const STATE_MAX_WRITES: usize = 1024;
pub const STATE_MAX_PAGE_BYTES: usize = 1200;
pub const STATE_ROOT_BYTES: usize = b"oasis7.iws.state-root.v1\0".len() + 46;

/// Frozen format tags. Extension needs an explicit format version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u16)]
pub enum StateNamespaceV1 {
    Business = 1,
    Allocator = 2,
    Scheduler = 3,
    Authorization = 4,
    Effects = 5,
    Artifacts = 6,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateRootV1 {
    pub namespace: StateNamespaceV1,
    pub node_root: String,
    pub leaf_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateEntryV1 {
    pub namespace: StateNamespaceV1,
    pub key: Vec<u8>,
    /// ID of an independently typed, immutable value object/chunk tree. The
    /// owning closure verifier must validate the target, not only this leaf.
    pub value: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateWriteV1 {
    pub key: Vec<u8>,
    pub before: Option<[u8; 32]>,
    pub after: Option<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateNodeWriteV1 {
    pub digest: String,
    pub bytes: Vec<u8>,
}

/// Typed edges for closure verification, transfer and mark. An empty child is
/// a format sentinel, not an object that must exist in the physical store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatePageLinksV1 {
    Branch {
        depth: u16,
        children: [Option<String>; 2],
    },
    Value(StateEntryV1),
}

impl StateNodeWriteV1 {
    pub fn verified_links(&self) -> Result<StatePageLinksV1, String> {
        if blake3::hash(&self.bytes).to_hex().as_str() != self.digest {
            return Err("state page content hash mismatch".into());
        }
        match StateCodec::decode(&self.bytes)? {
            crate::merkle::Node::Branch { depth, left, right } => {
                let empty = &empty_hashes()?[depth + 1];
                Ok(StatePageLinksV1::Branch {
                    depth: depth as u16,
                    children: [left, right]
                        .map(|child| if child == *empty { None } else { Some(child) }),
                })
            }
            crate::merkle::Node::Leaf(entry) => Ok(StatePageLinksV1::Value(entry)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDeltaV1 {
    pub before: StateRootV1,
    pub after: StateRootV1,
    pub writes: Vec<StateWriteV1>,
    pub new_nodes: Vec<StateNodeWriteV1>,
}

impl StateRootV1 {
    pub fn empty(namespace: StateNamespaceV1) -> Result<Self, String> {
        Ok(Self {
            namespace,
            node_root: empty_hashes()?[0].clone(),
            leaf_count: 0,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        codec::parse_digest(&self.node_root)?;
        if (self.leaf_count == 0) != (self.node_root == empty_hashes()?[0]) {
            return Err("state root count/empty mismatch".into());
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let mut bytes = b"oasis7.iws.state-root.v1\0".to_vec();
        bytes.extend_from_slice(&42u32.to_be_bytes());
        bytes.extend_from_slice(&(self.namespace as u16).to_be_bytes());
        bytes.extend_from_slice(&self.leaf_count.to_be_bytes());
        bytes.extend_from_slice(&codec::parse_digest(&self.node_root)?);
        Ok(bytes)
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, String> {
        const MAGIC: &[u8] = b"oasis7.iws.state-root.v1\0";
        if !bytes.starts_with(MAGIC) || bytes.len() != MAGIC.len() + 46 {
            return Err("state root format/length".into());
        }
        let body = &bytes[MAGIC.len()..];
        if body[..4] != 42u32.to_be_bytes() {
            return Err("state root declared length".into());
        }
        let namespace = StateNamespaceV1::from_tag(u16::from_be_bytes([body[4], body[5]]))?;
        let root = Self {
            namespace,
            leaf_count: u64::from_be_bytes(body[6..14].try_into().map_err(|_| "state root count")?),
            node_root: hex::encode(&body[14..46]),
        };
        root.validate()?;
        Ok(root)
    }

    pub fn digest(&self) -> Result<String, String> {
        Ok(blake3::hash(&self.canonical_bytes()?).to_hex().to_string())
    }

    /// Missing/corrupt paths return errors, never authenticated absence.
    pub fn get(
        &self,
        key: &[u8],
        reader: &impl MerkleNodeReader,
    ) -> Result<Option<[u8; 32]>, String> {
        self.validate()?;
        let digest = key_digest(self.namespace, key)?;
        let empty = empty_hashes()?;
        let mut view = Updater::<StateCodec, _>::new(reader, empty, 0);
        let entry = view.lookup(&self.node_root, &digest)?;
        match entry {
            Some(entry) if entry.namespace == self.namespace && entry.key == key => {
                Ok(Some(entry.value))
            }
            Some(_) => Err("state namespace/key collision".into()),
            None => Ok(None),
        }
    }
}

impl StateNamespaceV1 {
    pub(crate) fn from_tag(tag: u16) -> Result<Self, String> {
        match tag {
            1 => Ok(Self::Business),
            2 => Ok(Self::Allocator),
            3 => Ok(Self::Scheduler),
            4 => Ok(Self::Authorization),
            5 => Ok(Self::Effects),
            6 => Ok(Self::Artifacts),
            _ => Err("state namespace version".into()),
        }
    }
}

impl StateDeltaV1 {
    /// Writes are the final coalesced write-set in bytewise key order. The
    /// operation quota limits construction work and cannot change the tree.
    pub fn build(
        before: StateRootV1,
        writes: Vec<StateWriteV1>,
        reader: &impl MerkleNodeReader,
        max_pending_bytes: usize,
    ) -> Result<Self, String> {
        before.validate()?;
        validate_writes(&writes)?;
        let empty = empty_hashes()?;
        let mut builder =
            Updater::<StateCodec, _>::new(reader, empty, max_pending_bytes).track_parent_nodes();
        let mut root = before.node_root.clone();
        let mut count = before.leaf_count;
        for write in &writes {
            let key = key_digest(before.namespace, &write.key)?;
            let old = builder.lookup(&root, &key)?;
            if old.as_ref().map(|entry| entry.value) != write.before
                || old.as_ref().is_some_and(|entry| {
                    entry.namespace != before.namespace || entry.key != write.key
                })
            {
                return Err("state write precondition mismatch".into());
            }
            if write.before == write.after {
                continue;
            }
            let entry = write.after.map(|value| StateEntryV1 {
                namespace: before.namespace,
                key: write.key.clone(),
                value,
            });
            let (next, _) = builder.replace(&root, 0, &key, entry.as_ref())?;
            root = next;
            match (write.before, write.after) {
                (None, Some(_)) => count = count.checked_add(1).ok_or("state count overflow")?,
                (Some(_), None) => count = count.checked_sub(1).ok_or("state count underflow")?,
                _ => {}
            }
        }
        let after = StateRootV1 {
            namespace: before.namespace,
            node_root: root,
            leaf_count: count,
        };
        after.validate()?;
        let new_nodes = builder
            .final_writes(&after.node_root)?
            .into_iter()
            .map(|(digest, bytes)| StateNodeWriteV1 { digest, bytes })
            .collect();
        Ok(Self {
            before,
            after,
            writes,
            new_nodes,
        })
    }

    /// Rebuild against the authenticated parent, independent of submitted
    /// nodes. Extra, missing, reordered or temporary nodes are rejected.
    pub fn verify(
        &self,
        expected_parent: &str,
        reader: &impl MerkleNodeReader,
        max_pending_bytes: usize,
    ) -> Result<StateRootV1, String> {
        if self.before.digest()? != expected_parent {
            return Err("state delta wrong parent".into());
        }
        let rebuilt = Self::build(
            self.before.clone(),
            self.writes.clone(),
            reader,
            max_pending_bytes,
        )?;
        if rebuilt != *self {
            return Err("state delta does not reproduce canonical final writes".into());
        }
        Ok(self.after.clone())
    }
}

fn validate_writes(writes: &[StateWriteV1]) -> Result<(), String> {
    if writes.len() > STATE_MAX_WRITES {
        return Err("state write count bound".into());
    }
    let mut previous: Option<&[u8]> = None;
    for write in writes {
        codec::validate_key(&write.key)?;
        if previous.is_some_and(|previous| previous >= write.key.as_slice()) {
            return Err("state writes must be sorted and unique".into());
        }
        previous = Some(&write.key);
    }
    Ok(())
}

/// A bounded exact path supports inclusion and authenticated absence. This is
/// not a world authorization proof or a complete closure certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatePathV1 {
    pub nodes: Vec<StateNodeWriteV1>,
}

impl StateRootV1 {
    pub fn path(&self, key: &[u8], reader: &impl MerkleNodeReader) -> Result<StatePathV1, String> {
        struct Recording<'a, R> {
            reader: &'a R,
            nodes: std::cell::RefCell<Vec<StateNodeWriteV1>>,
        }
        impl<R: MerkleNodeReader> MerkleNodeReader for Recording<'_, R> {
            fn read_node(&self, digest: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, String> {
                let bytes = self.reader.read_node(digest, max_bytes)?;
                if let Some(bytes) = &bytes {
                    self.nodes.borrow_mut().push(StateNodeWriteV1 {
                        digest: digest.to_owned(),
                        bytes: bytes.clone(),
                    });
                }
                Ok(bytes)
            }
        }
        let recording = Recording {
            reader,
            nodes: std::cell::RefCell::new(Vec::new()),
        };
        self.get(key, &recording)?;
        Ok(StatePathV1 {
            nodes: recording.nodes.into_inner(),
        })
    }
}

impl StatePathV1 {
    pub fn verify(&self, root: &StateRootV1, key: &[u8]) -> Result<Option<[u8; 32]>, String> {
        if self.nodes.len() > DEPTH + 1
            || self
                .nodes
                .iter()
                .any(|node| node.bytes.len() > STATE_MAX_PAGE_BYTES)
        {
            return Err("state path bound".into());
        }
        struct Ordered<'a> {
            nodes: &'a [StateNodeWriteV1],
            next: std::cell::Cell<usize>,
        }
        impl MerkleNodeReader for Ordered<'_> {
            fn read_node(&self, digest: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, String> {
                let next = self.next.get();
                let node = self.nodes.get(next).ok_or("state proof path incomplete")?;
                if node.digest != digest {
                    return Err("state proof order/digest mismatch".into());
                }
                if node.bytes.len() > max_bytes {
                    return Err("state proof page byte bound".into());
                }
                self.next.set(next + 1);
                Ok(Some(node.bytes.clone()))
            }
        }
        let reader = Ordered {
            nodes: &self.nodes,
            next: std::cell::Cell::new(0),
        };
        let value = root.get(key, &reader)?;
        if reader.next.get() != self.nodes.len() {
            return Err("state proof extra nodes".into());
        }
        Ok(value)
    }
}

#[cfg(test)]
#[path = "incremental_state_tests.rs"]
mod tests;
