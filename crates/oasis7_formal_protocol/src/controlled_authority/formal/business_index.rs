//! Content-addressed sparse Merkle index for formal business results.
//!
//! This transition is separate from ordinary ClosedRecord artifact closure.
//! Each endpoint verifies a delta from the parent's result_root and persists
//! only its new immutable CAS nodes.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

pub const BUSINESS_INDEX_SCHEMA_V1: u32 = 1;
/// Minimal final-new-reachable writes; node and result-root codecs remain v1.
pub const BUSINESS_INDEX_SCHEMA_V2: u32 = 2;
#[path = "business_index_v2.rs"]
mod v2;
pub const BUSINESS_INDEX_DEPTH: usize = 256;
pub const BUSINESS_INDEX_MAX_UPDATES: usize = 1024;
#[path = "business_index_codec.rs"]
mod codec;
pub const BUSINESS_INDEX_MAX_LEAF_BYTES: usize = 4 * 1024 * 1024;
pub const BUSINESS_INDEX_MAX_DELTA_BYTES: usize = 64 * 1024 * 1024;
const NODE_MAGIC: &[u8] = b"oasis7.formal.business-index.node.v1\0";
const ROOT_MAGIC: &[u8] = b"oasis7.formal.business-index.root.v1\0";
const METADATA_KEY: &[u8] = b"oasis7.formal.business-index.metadata-key.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BusinessIndexLeafKindV1 {
    CommitMetadata,
    Outcome,
    HistoricalCut,
}

/// The producer supplies canonical CBOR and this shared layer treats it as
/// opaque bytes. It never interprets or exports business results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessIndexLeafV1 {
    pub key_digest: String,
    pub kind: BusinessIndexLeafKindV1,
    #[serde(
        serialize_with = "codec::encode_bytes",
        deserialize_with = "codec::decode_value"
    )]
    pub canonical_value: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BusinessIndexNodeKindV1 {
    Branch {
        depth: u16,
        left_hash: String,
        right_hash: String,
    },
    Leaf(BusinessIndexLeafV1),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusinessIndexNodeV1 {
    pub kind: BusinessIndexNodeKindV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessIndexRootV1 {
    pub node_root: String,
    /// Includes the single mutable CommitMetadata leaf.
    pub leaf_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessIndexNodeWriteV1 {
    pub digest: String,
    #[serde(
        serialize_with = "codec::encode_bytes",
        deserialize_with = "codec::decode_node"
    )]
    pub bytes: Vec<u8>,
}

/// Deterministic transition independent from ordinary artifact closure.
/// base_root is the parent's canonical result_root; new_root is the value the
/// candidate record must sign.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessIndexDeltaV1 {
    pub schema_version: u32,
    pub base_root: String,
    pub base: BusinessIndexRootV1,
    pub new_root: String,
    pub new: BusinessIndexRootV1,
    #[serde(deserialize_with = "codec::decode_updates")]
    pub updates: Vec<BusinessIndexLeafV1>,
    #[serde(deserialize_with = "codec::decode_writes")]
    pub node_writes: Vec<BusinessIndexNodeWriteV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessIndexInclusionV1 {
    pub leaf: BusinessIndexLeafV1,
    pub leaf_count: u64,
    /// Siblings ordered from leaf depth toward the root.
    pub siblings: Vec<String>,
}

#[path = "business_index_path.rs"]
mod path;
pub use path::BusinessIndexPathV1;

pub trait BusinessIndexNodeReader {
    /// Optionally fetch one bounded content path. This grants no authority;
    /// get/prove still traverse and verify the caller's exact root and key.
    fn prefetch_path(&self, _root: &BusinessIndexRootV1, _key: &str) -> Result<(), String> {
        Ok(())
    }

    /// Reads raw canonical CAS bytes by their BLAKE3 content hash.
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String>;
    /// Return an already cached immutable object without doing IO. Callers must
    /// still check its content hash and the qualified root that references it.
    fn read_cached_node(&self, _digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
    /// Optionally cache immutable bytes without IO. Implementations validate the
    /// content hash; caching grants no authority or qualification to a root.
    fn cache_content_node(&self, _digest: &str, _bytes: &[u8]) -> Result<(), String> {
        Ok(())
    }
}

impl BusinessIndexRootV1 {
    /// Canonical empty subtree digest at a fixed depth. This is a traversal
    /// sentinel only; a caller must already possess the trusted root descriptor.
    pub fn empty_subtree_hash(depth: usize) -> Result<String, String> {
        if depth > BUSINESS_INDEX_DEPTH {
            return Err("business index subtree depth out of bounds".into());
        }
        Ok(empty_hashes()?[depth].clone())
    }

    pub fn empty() -> Result<Self, String> {
        Ok(Self {
            node_root: empty_hashes()?[0].clone(),
            leaf_count: 0,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        let empty_root = empty_hashes()?[0].clone();
        if !valid_digest(&self.node_root)
            || (self.leaf_count == 0 && self.node_root != empty_root)
            || (self.leaf_count > 0 && self.node_root == empty_root)
        {
            return Err("business index root descriptor".into());
        }
        Ok(())
    }

    /// Hash committed by CanonicalRecordV1::result_root.
    pub fn canonical_result_root(&self) -> Result<String, String> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(ROOT_MAGIC.len() + 4 + 8 + 32);
        bytes.extend_from_slice(ROOT_MAGIC);
        bytes.extend_from_slice(&BUSINESS_INDEX_SCHEMA_V1.to_be_bytes());
        bytes.extend_from_slice(&self.leaf_count.to_be_bytes());
        bytes.extend_from_slice(&digest_bytes(&self.node_root)?);
        Ok(blake3_hex(&bytes))
    }

    pub fn get(
        &self,
        key_digest: &str,
        reader: &impl BusinessIndexNodeReader,
    ) -> Result<Option<BusinessIndexLeafV1>, String> {
        self.validate()?;
        digest_bytes(key_digest)?;
        reader.prefetch_path(self, key_digest)?;
        lookup(&self.node_root, &digest_bytes(key_digest)?, reader)
    }

    pub fn prove(
        &self,
        key_digest: &str,
        reader: &impl BusinessIndexNodeReader,
    ) -> Result<BusinessIndexInclusionV1, String> {
        self.validate()?;
        let key = digest_bytes(key_digest)?;
        reader.prefetch_path(self, key_digest)?;
        let empty = empty_hashes()?;
        let mut current = self.node_root.clone();
        let mut top_down = Vec::with_capacity(BUSINESS_INDEX_DEPTH);
        for (depth, empty_hash) in empty.iter().take(BUSINESS_INDEX_DEPTH).enumerate() {
            if current == *empty_hash {
                return Err("business index inclusion key absent".into());
            }
            let node = read_node(reader, &current, depth)?
                .ok_or("business index inclusion path missing")?;
            let BusinessIndexNodeKindV1::Branch {
                left_hash,
                right_hash,
                ..
            } = node.kind
            else {
                return Err("business index inclusion branch shape".into());
            };
            if key_bit(&key, depth) {
                top_down.push(left_hash);
                current = right_hash;
            } else {
                top_down.push(right_hash);
                current = left_hash;
            }
        }
        let node = read_node(reader, &current, BUSINESS_INDEX_DEPTH)?
            .ok_or("business index inclusion leaf missing")?;
        let BusinessIndexNodeKindV1::Leaf(leaf) = node.kind else {
            return Err("business index inclusion leaf shape".into());
        };
        if leaf.key_digest != key_digest {
            return Err("business index key digest collision".into());
        }
        top_down.reverse();
        Ok(BusinessIndexInclusionV1 {
            leaf,
            leaf_count: self.leaf_count,
            siblings: top_down,
        })
    }
}

impl BusinessIndexNodeV1 {
    pub fn branch(depth: usize, left_hash: String, right_hash: String) -> Result<Self, String> {
        if depth >= BUSINESS_INDEX_DEPTH || !valid_digest(&left_hash) || !valid_digest(&right_hash)
        {
            return Err("business index branch bounds".into());
        }
        Ok(Self {
            kind: BusinessIndexNodeKindV1::Branch {
                depth: depth as u16,
                left_hash,
                right_hash,
            },
        })
    }

    pub fn leaf(leaf: BusinessIndexLeafV1) -> Result<Self, String> {
        validate_leaf(&leaf)?;
        Ok(Self {
            kind: BusinessIndexNodeKindV1::Leaf(leaf),
        })
    }

    /// Raw deterministic encoding hashes to the exact LocalCasStore key.
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(NODE_MAGIC);
        match &self.kind {
            BusinessIndexNodeKindV1::Branch {
                depth,
                left_hash,
                right_hash,
            } => {
                if *depth as usize >= BUSINESS_INDEX_DEPTH
                    || !valid_digest(left_hash)
                    || !valid_digest(right_hash)
                {
                    return Err("business index branch bounds".into());
                }
                bytes.push(1);
                bytes.extend_from_slice(&depth.to_be_bytes());
                bytes.extend_from_slice(&digest_bytes(left_hash)?);
                bytes.extend_from_slice(&digest_bytes(right_hash)?);
            }
            BusinessIndexNodeKindV1::Leaf(leaf) => {
                validate_leaf(leaf)?;
                bytes.push(0);
                bytes.extend_from_slice(&digest_bytes(&leaf.key_digest)?);
                bytes.push(match leaf.kind {
                    BusinessIndexLeafKindV1::CommitMetadata => 0,
                    BusinessIndexLeafKindV1::Outcome => 1,
                    BusinessIndexLeafKindV1::HistoricalCut => 2,
                });
                let value_len = u32::try_from(leaf.canonical_value.len())
                    .map_err(|_| "business index leaf byte bound")?;
                bytes.extend_from_slice(&value_len.to_be_bytes());
                bytes.extend_from_slice(&leaf.canonical_value);
            }
        }
        if bytes.len() > BUSINESS_INDEX_MAX_LEAF_BYTES + 128 {
            return Err("business index node byte bound".into());
        }
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < NODE_MAGIC.len() + 1 || !bytes.starts_with(NODE_MAGIC) {
            return Err("business index node encoding/version".into());
        }
        let mut cursor = NODE_MAGIC.len();
        let tag = take_u8(bytes, &mut cursor)?;
        let kind = match tag {
            1 => {
                let depth = take_u16(bytes, &mut cursor)?;
                let left_hash = hex::encode(take_array::<32>(bytes, &mut cursor)?);
                let right_hash = hex::encode(take_array::<32>(bytes, &mut cursor)?);
                BusinessIndexNodeKindV1::Branch {
                    depth,
                    left_hash,
                    right_hash,
                }
            }
            0 => {
                let key_digest = hex::encode(take_array::<32>(bytes, &mut cursor)?);
                let kind = match take_u8(bytes, &mut cursor)? {
                    0 => BusinessIndexLeafKindV1::CommitMetadata,
                    1 => BusinessIndexLeafKindV1::Outcome,
                    2 => BusinessIndexLeafKindV1::HistoricalCut,
                    _ => return Err("business index leaf kind".into()),
                };
                let value_len = take_u32(bytes, &mut cursor)? as usize;
                if value_len > BUSINESS_INDEX_MAX_LEAF_BYTES {
                    return Err("business index leaf byte bound".into());
                }
                let canonical_value = take_bytes(bytes, &mut cursor, value_len)?.to_vec();
                BusinessIndexNodeKindV1::Leaf(BusinessIndexLeafV1 {
                    key_digest,
                    kind,
                    canonical_value,
                })
            }
            _ => return Err("business index node kind".into()),
        };
        if cursor != bytes.len() {
            return Err("business index node trailing bytes".into());
        }
        let node = Self { kind };
        if node.encode()?.as_slice() != bytes {
            return Err("business index node non-canonical encoding".into());
        }
        Ok(node)
    }

    pub fn digest(&self) -> Result<String, String> {
        Ok(blake3_hex(&self.encode()?))
    }
}

impl BusinessIndexDeltaV1 {
    pub fn build(
        base: BusinessIndexRootV1,
        updates: Vec<BusinessIndexLeafV1>,
        reader: &impl BusinessIndexNodeReader,
    ) -> Result<Self, String> {
        base.validate()?;
        validate_updates(&updates)?;
        let base_root = base.canonical_result_root()?;
        let (new, node_writes) = apply_updates(&base, &updates, reader)?;
        let new_root = new.canonical_result_root()?;
        let delta = Self {
            schema_version: BUSINESS_INDEX_SCHEMA_V1,
            base_root,
            base,
            new_root,
            new,
            updates,
            node_writes,
        };
        delta.validate_shape()?;
        Ok(delta)
    }

    pub fn verify(
        &self,
        expected_base_root: &str,
        reader: &impl BusinessIndexNodeReader,
    ) -> Result<BusinessIndexRootV1, String> {
        self.validate_shape()?;
        if self.base_root != expected_base_root
            || self.base.canonical_result_root()? != expected_base_root
        {
            return Err("business index delta baseRoot does not match parent result_root".into());
        }
        let rebuilt = match self.schema_version {
            BUSINESS_INDEX_SCHEMA_V1 => {
                Self::build(self.base.clone(), self.updates.clone(), reader)?
            }
            BUSINESS_INDEX_SCHEMA_V2 => {
                Self::build_v2(self.base.clone(), self.updates.clone(), reader)?
            }
            _ => return Err("business index delta unsupported version".into()),
        };
        if rebuilt != *self {
            return Err("business index delta does not reproduce its signed newRoot".into());
        }
        Ok(self.new.clone())
    }

    fn validate_shape(&self) -> Result<(), String> {
        if !matches!(
            self.schema_version,
            BUSINESS_INDEX_SCHEMA_V1 | BUSINESS_INDEX_SCHEMA_V2
        ) || !valid_digest(&self.base_root)
            || !valid_digest(&self.new_root)
            || self.base.canonical_result_root()? != self.base_root
            || self.new.canonical_result_root()? != self.new_root
        {
            return Err("business index delta schema/root bounds".into());
        }
        validate_updates(&self.updates)?;
        let mut previous: Option<&str> = None;
        let mut total_bytes = 0usize;
        for write in &self.node_writes {
            if !valid_digest(&write.digest)
                || blake3_hex(&write.bytes) != write.digest
                || previous.is_some_and(|old| old >= write.digest.as_str())
            {
                return Err("business index node writes are invalid or unordered".into());
            }
            previous = Some(&write.digest);
            total_bytes = total_bytes
                .checked_add(write.bytes.len())
                .ok_or("business index delta byte overflow")?;
        }
        if total_bytes > BUSINESS_INDEX_MAX_DELTA_BYTES
            || serde_cbor::to_vec(self)
                .map_err(|error| error.to_string())?
                .len()
                > BUSINESS_INDEX_MAX_DELTA_BYTES
        {
            return Err("business index delta byte bound".into());
        }
        Ok(())
    }
}

fn apply_updates(
    base: &BusinessIndexRootV1,
    updates: &[BusinessIndexLeafV1],
    reader: &impl BusinessIndexNodeReader,
) -> Result<(BusinessIndexRootV1, Vec<BusinessIndexNodeWriteV1>), String> {
    let empty = empty_hashes()?;
    let mut updater = Updater {
        reader,
        pending: BTreeMap::new(),
        empty: &empty,
    };
    let mut root = base.node_root.clone();
    let mut leaf_count = base.leaf_count;
    for leaf in updates {
        let existing = updater.lookup(&root, &leaf.key_digest)?;
        match (&existing, leaf.kind) {
            (
                Some(old),
                BusinessIndexLeafKindV1::Outcome | BusinessIndexLeafKindV1::HistoricalCut,
            ) if old == leaf => {}
            (
                Some(_),
                BusinessIndexLeafKindV1::Outcome | BusinessIndexLeafKindV1::HistoricalCut,
            ) => {
                return Err("business result leaves are immutable".into());
            }
            (Some(old), BusinessIndexLeafKindV1::CommitMetadata)
                if old.kind == BusinessIndexLeafKindV1::CommitMetadata => {}
            (Some(_), BusinessIndexLeafKindV1::CommitMetadata) => {
                return Err("business index metadata leaf kind changed".into());
            }
            (None, _) => {
                leaf_count = leaf_count
                    .checked_add(1)
                    .ok_or("business index leaf count overflow")?;
            }
        }
        let (next, inserted) = updater.insert(&root, 0, leaf)?;
        root = next;
        if !inserted && existing.is_none() {
            return Err("business index update lost a new leaf".into());
        }
    }
    let new = BusinessIndexRootV1 {
        node_root: root,
        leaf_count,
    };
    new.validate()?;
    let node_writes = updater
        .pending
        .into_iter()
        .map(|(digest, bytes)| BusinessIndexNodeWriteV1 { digest, bytes })
        .collect();
    Ok((new, node_writes))
}

fn validate_updates(updates: &[BusinessIndexLeafV1]) -> Result<(), String> {
    if updates.is_empty() || updates.len() > BUSINESS_INDEX_MAX_UPDATES {
        return Err("business index update count".into());
    }
    let mut previous: Option<&str> = None;
    for leaf in updates {
        validate_leaf(leaf)?;
        if previous.is_some_and(|old| old >= leaf.key_digest.as_str()) {
            return Err("business index updates must be sorted and unique".into());
        }
        previous = Some(&leaf.key_digest);
    }
    Ok(())
}

fn validate_leaf(leaf: &BusinessIndexLeafV1) -> Result<(), String> {
    if !valid_digest(&leaf.key_digest)
        || leaf.canonical_value.is_empty()
        || leaf.canonical_value.len() > BUSINESS_INDEX_MAX_LEAF_BYTES
    {
        return Err("business index leaf identity/byte bounds".into());
    }
    Ok(())
}

fn lookup(
    root: &str,
    key: &[u8; 32],
    reader: &impl BusinessIndexNodeReader,
) -> Result<Option<BusinessIndexLeafV1>, String> {
    let empty = empty_hashes()?;
    let mut current = root.to_owned();
    for (depth, empty_hash) in empty.iter().take(BUSINESS_INDEX_DEPTH).enumerate() {
        if current == *empty_hash {
            return Ok(None);
        }
        let node = read_node(reader, &current, depth)?.ok_or("business index node missing")?;
        let BusinessIndexNodeKindV1::Branch {
            left_hash,
            right_hash,
            ..
        } = node.kind
        else {
            return Err("business index leaf before depth 256".into());
        };
        current = if key_bit(key, depth) {
            right_hash
        } else {
            left_hash
        };
    }
    if current == empty[BUSINESS_INDEX_DEPTH] {
        return Ok(None);
    }
    let node =
        read_node(reader, &current, BUSINESS_INDEX_DEPTH)?.ok_or("business index leaf missing")?;
    let BusinessIndexNodeKindV1::Leaf(leaf) = node.kind else {
        return Err("business index branch at leaf depth".into());
    };
    if digest_bytes(&leaf.key_digest)? != *key {
        return Err("business index key digest collision".into());
    }
    Ok(Some(leaf))
}

struct Updater<'a, R> {
    reader: &'a R,
    pending: BTreeMap<String, Vec<u8>>,
    empty: &'a [String],
}

impl<R: BusinessIndexNodeReader> Updater<'_, R> {
    fn read_node(&self, digest: &str, depth: usize) -> Result<Option<BusinessIndexNodeV1>, String> {
        if digest == self.empty[depth] {
            return Ok(None);
        }
        let bytes = if let Some(bytes) = self.pending.get(digest) {
            bytes.clone()
        } else {
            self.reader
                .read_node(digest)?
                .ok_or("business index references a missing node")?
        };
        if blake3_hex(&bytes) != digest {
            return Err("business index node content hash mismatch".into());
        }
        let node = BusinessIndexNodeV1::decode(&bytes)?;
        match (&node.kind, depth) {
            (BusinessIndexNodeKindV1::Branch { depth: stored, .. }, expected)
                if *stored as usize == expected && expected < BUSINESS_INDEX_DEPTH => {}
            (BusinessIndexNodeKindV1::Leaf(_), BUSINESS_INDEX_DEPTH) => {}
            _ => return Err("business index node is at the wrong depth".into()),
        }
        Ok(Some(node))
    }

    fn lookup(&self, root: &str, key_digest: &str) -> Result<Option<BusinessIndexLeafV1>, String> {
        let key = digest_bytes(key_digest)?;
        let mut current = root.to_owned();
        for depth in 0..BUSINESS_INDEX_DEPTH {
            let Some(node) = self.read_node(&current, depth)? else {
                return Ok(None);
            };
            let BusinessIndexNodeKindV1::Branch {
                left_hash,
                right_hash,
                ..
            } = node.kind
            else {
                return Err("business index leaf before depth 256".into());
            };
            current = if key_bit(&key, depth) {
                right_hash
            } else {
                left_hash
            };
        }
        let Some(node) = self.read_node(&current, BUSINESS_INDEX_DEPTH)? else {
            return Ok(None);
        };
        let BusinessIndexNodeKindV1::Leaf(leaf) = node.kind else {
            return Err("business index branch at leaf depth".into());
        };
        if leaf.key_digest != key_digest {
            return Err("business index key digest collision".into());
        }
        Ok(Some(leaf))
    }

    fn insert(
        &mut self,
        root: &str,
        depth: usize,
        leaf: &BusinessIndexLeafV1,
    ) -> Result<(String, bool), String> {
        let key = digest_bytes(&leaf.key_digest)?;
        let mut current = root.to_owned();
        // Keep the 256-level path on the heap. Recursive hashing retained one
        // large hashing frame per level on small HTTP-worker stacks.
        let mut path = Vec::with_capacity(BUSINESS_INDEX_DEPTH - depth);
        for level in depth..BUSINESS_INDEX_DEPTH {
            let (left, right) = match self.read_node(&current, level)? {
                Some(node) => match node.kind {
                    BusinessIndexNodeKindV1::Branch {
                        left_hash,
                        right_hash,
                        ..
                    } => (left_hash, right_hash),
                    BusinessIndexNodeKindV1::Leaf(_) => {
                        return Err("business index leaf before depth 256".into());
                    }
                },
                None => (self.empty[level + 1].clone(), self.empty[level + 1].clone()),
            };
            let right_selected = key_bit(&key, level);
            current = if right_selected {
                right.clone()
            } else {
                left.clone()
            };
            path.push((level, left, right, right_selected));
        }
        let existing = self.read_node(&current, BUSINESS_INDEX_DEPTH)?;
        let inserted = match existing.as_ref().map(|node| &node.kind) {
            Some(BusinessIndexNodeKindV1::Leaf(old)) if old.key_digest == leaf.key_digest => false,
            Some(BusinessIndexNodeKindV1::Leaf(_)) => {
                return Err("business index key digest collision".into());
            }
            Some(BusinessIndexNodeKindV1::Branch { .. }) => {
                return Err("business index branch at leaf depth".into());
            }
            None => true,
        };
        let bytes = BusinessIndexNodeV1::leaf(leaf.clone())?.encode()?;
        let mut digest = blake3_hex(&bytes);
        self.pending.insert(digest.clone(), bytes);
        for (level, mut left, mut right, right_selected) in path.into_iter().rev() {
            if right_selected {
                right = digest;
            } else {
                left = digest;
            }
            let bytes = BusinessIndexNodeV1::branch(level, left, right)?.encode()?;
            digest = blake3_hex(&bytes);
            self.pending.insert(digest.clone(), bytes);
        }
        Ok((digest, inserted))
    }
}

fn read_node(
    reader: &impl BusinessIndexNodeReader,
    digest: &str,
    depth: usize,
) -> Result<Option<BusinessIndexNodeV1>, String> {
    let Some(bytes) = reader.read_node(digest)? else {
        return Ok(None);
    };
    if blake3_hex(&bytes) != digest {
        return Err("business index node content hash mismatch".into());
    }
    let node = BusinessIndexNodeV1::decode(&bytes)?;
    match (&node.kind, depth) {
        (BusinessIndexNodeKindV1::Branch { depth: stored, .. }, expected)
            if *stored as usize == expected && expected < BUSINESS_INDEX_DEPTH => {}
        (BusinessIndexNodeKindV1::Leaf(_), BUSINESS_INDEX_DEPTH) => {}
        _ => return Err("business index node is at the wrong depth".into()),
    }
    Ok(Some(node))
}

/// Fixed-size traversal sentinels derived from the canonical codec. A caller
/// can reuse these immutable values within one walk without caching authority,
/// logical objects, or historical records.
pub fn empty_subtree_hashes() -> Result<[String; BUSINESS_INDEX_DEPTH + 1], String> {
    empty_hashes()?
        .try_into()
        .map_err(|_| "business index empty hash table width".into())
}

fn empty_hashes() -> Result<Vec<String>, String> {
    // These canonical sentinels depend only on the fixed codec and depth.
    // Reusing them does not cache a business root, object, or authority.
    static HASHES: OnceLock<Result<Vec<String>, String>> = OnceLock::new();
    HASHES.get_or_init(build_empty_hashes).clone()
}

fn build_empty_hashes() -> Result<Vec<String>, String> {
    let mut hashes = vec![String::new(); BUSINESS_INDEX_DEPTH + 1];
    // An unpersisted, unconstructible leaf marker defines the empty subtree.
    let mut empty_leaf = NODE_MAGIC.to_vec();
    empty_leaf.push(2);
    hashes[BUSINESS_INDEX_DEPTH] = blake3_hex(&empty_leaf);
    for depth in (0..BUSINESS_INDEX_DEPTH).rev() {
        let node = BusinessIndexNodeV1::branch(
            depth,
            hashes[depth + 1].clone(),
            hashes[depth + 1].clone(),
        )?;
        hashes[depth] = blake3_hex(&node.encode()?);
    }
    Ok(hashes)
}

pub fn metadata_key_digest() -> String {
    blake3_hex(METADATA_KEY)
}

fn digest_bytes(value: &str) -> Result<[u8; 32], String> {
    if !valid_digest(value) {
        return Err("business index digest encoding".into());
    }
    hex::decode(value)
        .map_err(|error| error.to_string())?
        .try_into()
        .map_err(|_| "business index digest length".into())
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn key_bit(key: &[u8; 32], depth: usize) -> bool {
    key[depth / 8] & (0x80 >> (depth % 8)) != 0
}

fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

fn take_u8(bytes: &[u8], cursor: &mut usize) -> Result<u8, String> {
    Ok(take_bytes(bytes, cursor, 1)?[0])
}

fn take_u16(bytes: &[u8], cursor: &mut usize) -> Result<u16, String> {
    Ok(u16::from_be_bytes(take_array(bytes, cursor)?))
}

fn take_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, String> {
    Ok(u32::from_be_bytes(take_array(bytes, cursor)?))
}

fn take_array<const N: usize>(bytes: &[u8], cursor: &mut usize) -> Result<[u8; N], String> {
    take_bytes(bytes, cursor, N)?
        .try_into()
        .map_err(|_| "business index encoding length".into())
}

fn take_bytes<'a>(bytes: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8], String> {
    let end = cursor
        .checked_add(len)
        .ok_or("business index offset overflow")?;
    let value = bytes
        .get(*cursor..end)
        .ok_or("truncated business index node")?;
    *cursor = end;
    Ok(value)
}

#[cfg(test)]
#[path = "business_index_tests.rs"]
mod tests;
