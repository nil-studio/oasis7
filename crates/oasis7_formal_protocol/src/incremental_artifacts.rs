//! Immutable original artifact inventories and IWS manifest. These content
//! roots do not authorize module execution, read access or a serving head.
use crate::incremental_execution::ExecutionError;
use crate::incremental_history::{
    WorldScopeV1,
    wire::{Reader, Writer, parse_hash},
};
use crate::incremental_state::MerkleNodeReader;
use crate::merkle::{Codec, DEPTH, Node, Updater, VisitError};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
#[path = "incremental_artifact_codec.rs"]
mod codec;
use codec::{ArtifactCodec, empty};
#[path = "incremental_execution_manifest.rs"]
mod manifest;
pub use manifest::*;
pub const ARTIFACT_MAX_PAGE_BYTES: usize = 160;
pub const ARTIFACT_MAX_APPEND: usize = 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ArtifactKindV1 {
    Wasm = 1,
    Rules = 2,
}
impl ArtifactKindV1 {
    fn from_tag(tag: u8) -> Result<Self, ExecutionError> {
        match tag {
            1 => Ok(Self::Wasm),
            2 => Ok(Self::Rules),
            _ => Err(ExecutionError::Invalid("artifact kind")),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OriginalArtifactRefV1 {
    pub id: [u8; 32],
    pub byte_len: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtifactEntryV1 {
    pub kind: ArtifactKindV1,
    pub original: OriginalArtifactRefV1,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactCatalogV1 {
    pub kind: ArtifactKindV1,
    pub count: u64,
    pub node_root: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactPageWriteV1 {
    pub digest: String,
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactCatalogDeltaV1 {
    pub before: ArtifactCatalogV1,
    pub after: ArtifactCatalogV1,
    pub entries: Vec<ArtifactEntryV1>,
    pub new_nodes: Vec<ArtifactPageWriteV1>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactPageLinksV1 {
    Branch {
        depth: u16,
        children: [Option<[u8; 32]>; 2],
    },
    Original(ArtifactEntryV1),
}
impl ArtifactPageWriteV1 {
    pub fn verified_links(&self) -> Result<ArtifactPageLinksV1, ExecutionError> {
        if blake3::hash(&self.bytes).to_hex().as_str() != self.digest {
            return Err(ExecutionError::Invalid("artifact page hash"));
        }
        match ArtifactCodec::decode(&self.bytes).map_err(ExecutionError::Tree)? {
            Node::Leaf(entry) => Ok(ArtifactPageLinksV1::Original(entry)),
            Node::Branch { depth, left, right } => {
                let sentinel = &empty().map_err(ExecutionError::Tree)?[depth + 1];
                let edge = |id: String| {
                    if id == *sentinel {
                        Ok(None)
                    } else {
                        parse_hash(&id).map(Some)
                    }
                };
                Ok(ArtifactPageLinksV1::Branch {
                    depth: depth as u16,
                    children: [edge(left)?, edge(right)?],
                })
            }
        }
    }
}
impl ArtifactCatalogDeltaV1 {
    pub fn verify(
        &self,
        parent: [u8; 32],
        source: &impl MerkleNodeReader,
        memory: &MemoryBudget,
        max_pending: usize,
    ) -> Result<(), ExecutionError> {
        let _metadata = memory.reserve(4096, MemoryClass::Ordinary)?;
        if self.before.digest()? != parent {
            return Err(ExecutionError::Invalid("artifact delta parent"));
        }
        let rebuilt = self
            .before
            .append(&self.entries, source, memory, max_pending)?;
        if *rebuilt != *self {
            return Err(ExecutionError::Invalid("artifact delta final nodes"));
        }
        Ok(())
    }
}
impl ArtifactCatalogV1 {
    pub fn empty(kind: ArtifactKindV1) -> Result<Self, ExecutionError> {
        Ok(Self {
            kind,
            count: 0,
            node_root: empty().map_err(ExecutionError::Tree)?[0].clone(),
        })
    }
    pub fn validate(&self) -> Result<(), ExecutionError> {
        parse_hash(&self.node_root)?;
        if (self.count == 0) != (self.node_root == empty().map_err(ExecutionError::Tree)?[0]) {
            return Err(ExecutionError::Invalid("artifact count/empty"));
        }
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ExecutionError> {
        self.validate()?;
        let mut w = Writer::new();
        w.u8(self.kind as u8);
        w.u64(self.count);
        w.hash(&parse_hash(&self.node_root)?);
        w.finish(b"oasis7.iws.artifact-root.v1\0", 128)
    }
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ExecutionError> {
        let mut r = Reader::new(bytes, b"oasis7.iws.artifact-root.v1\0", 128)?;
        let value = Self {
            kind: ArtifactKindV1::from_tag(r.u8()?)?,
            count: r.u64()?,
            node_root: hex::encode(r.array::<32>()?),
        };
        r.finish()?;
        value.validate()?;
        Ok(value)
    }
    pub fn digest(&self) -> Result<[u8; 32], ExecutionError> {
        Ok(*blake3::hash(&self.canonical_bytes()?).as_bytes())
    }
    pub fn get(
        &self,
        id: &[u8; 32],
        source: &impl MerkleNodeReader,
    ) -> Result<Option<ArtifactEntryV1>, ExecutionError> {
        self.validate()?;
        let mut view =
            Updater::<ArtifactCodec, _>::new(source, empty().map_err(ExecutionError::Tree)?, 0);
        let entry = view
            .lookup(&self.node_root, id)
            .map_err(ExecutionError::Tree)?;
        if entry.is_some_and(|e| e.kind != self.kind) {
            return Err(ExecutionError::Invalid("artifact catalog kind"));
        }
        Ok(entry)
    }
    /// A bounded delta against the exact parent. Arbitrary CAS hits never
    /// suppress a final new page. Identical existing originals are idempotent.
    pub fn append(
        &self,
        entries: &[ArtifactEntryV1],
        source: &impl MerkleNodeReader,
        memory: &MemoryBudget,
        max_pending: usize,
    ) -> Result<ReservedValue<ArtifactCatalogDeltaV1>, ExecutionError> {
        if entries.len() > ARTIFACT_MAX_APPEND {
            return Err(ExecutionError::ResourceLimited("artifact append count"));
        }
        let mut phase = memory.reserve(
            entries.len() * 512 * 1024 + 128 * 1024,
            MemoryClass::Ordinary,
        )?;
        if source.allocation_class() != Some(MemoryClass::Ordinary)
            || !source
                .allocation_budget()
                .is_some_and(|pool| pool.same_pool(memory))
        {
            return Err(ExecutionError::Invalid("artifact producer pool/class"));
        }
        self.validate()?;
        let mut previous = None;
        let mut view = Updater::<ArtifactCodec, _>::new(
            source,
            empty().map_err(ExecutionError::Tree)?,
            max_pending,
        )
        .track_parent_nodes();
        let mut after = self.clone();
        for entry in entries {
            if entry.kind != self.kind || previous.is_some_and(|id| id >= entry.original.id) {
                return Err(ExecutionError::Invalid("artifact append kind/order"));
            }
            previous = Some(entry.original.id);
            if let Some(old) = view
                .lookup(&after.node_root, &entry.original.id)
                .map_err(ExecutionError::Tree)?
            {
                if old != *entry {
                    return Err(ExecutionError::Invalid("artifact identity length/kind"));
                }
                continue;
            }
            after.node_root = view
                .insert(&after.node_root, 0, entry)
                .map_err(ExecutionError::Tree)?
                .0;
            after.count = after
                .count
                .checked_add(1)
                .ok_or(ExecutionError::Invalid("artifact count overflow"))?;
        }
        let new_nodes = view
            .final_writes(&after.node_root)
            .map_err(ExecutionError::Tree)?
            .into_iter()
            .map(|(digest, bytes)| ArtifactPageWriteV1 { digest, bytes })
            .collect();
        let delta = ArtifactCatalogDeltaV1 {
            before: self.clone(),
            after,
            entries: entries.to_vec(),
            new_nodes,
        };
        let size = 512
            + delta.entries.capacity() * std::mem::size_of::<ArtifactEntryV1>()
            + delta.new_nodes.capacity() * std::mem::size_of::<ArtifactPageWriteV1>()
            + delta
                .new_nodes
                .iter()
                .map(|n| n.bytes.capacity() + n.digest.capacity())
                .sum::<usize>();
        let charge = phase
            .split(size)
            .map_err(|_| ExecutionError::Invalid("artifact delta admission"))?;
        Ok(charge.retain(delta))
    }
    /// Caller admits DFS workspace and controls source/visitor quotas. All
    /// visitor effects remain provisional until the exact count is checked.
    pub(crate) fn visit<E>(
        &self,
        source: &impl MerkleNodeReader,
        mut visitor: impl FnMut(&ArtifactEntryV1) -> Result<(), E>,
    ) -> Result<u64, VisitError<E>> {
        self.validate()
            .map_err(|e| VisitError::Tree(e.to_string()))?;
        let mut view =
            Updater::<ArtifactCodec, _>::new(source, empty().map_err(VisitError::Tree)?, 0);
        let count = view
            .visit_leaves(&self.node_root, |entry| {
                if entry.kind != self.kind {
                    return Err(VisitError::Tree("artifact catalog kind".into()));
                }
                visitor(entry).map_err(VisitError::Visitor)
            })
            .map_err(|error| match error {
                VisitError::Tree(error) => VisitError::Tree(error),
                VisitError::Visitor(error) => error,
            })?;
        if count != self.count {
            return Err(VisitError::Tree("artifact catalog count".into()));
        }
        Ok(count)
    }
}

#[cfg(test)]
#[path = "incremental_artifact_tests.rs"]
mod tests;
