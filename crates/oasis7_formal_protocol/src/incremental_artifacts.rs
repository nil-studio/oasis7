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

type ArtifactWork<'a> = dyn Fn(u64, u64) -> Result<(), ExecutionError> + 'a;

fn with_artifact_work<T, E: From<ExecutionError>>(
    work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    run: impl FnOnce(&ArtifactWork<'_>, &crate::merkle::WorkMeter<'_>) -> Result<T, E>,
) -> Result<T, E> {
    let work = std::cell::RefCell::new(work);
    let first = std::cell::RefCell::new(None);
    let pay = |steps, bytes| (work.borrow_mut())(steps, bytes);
    let tree = |bytes: usize| {
        if first.borrow().is_some() {
            return Err("artifact meter stopped".to_owned());
        }
        match pay(1, bytes as u64) {
            Ok(()) => Ok(()),
            Err(error) => {
                *first.borrow_mut() = Some(error);
                Err("artifact meter stopped".to_owned())
            }
        }
    };
    let result = run(&pay, &tree);
    if let Some(error) = first.into_inner() {
        Err(error.into())
    } else {
        result
    }
}

fn artifact_work(work: Option<&ArtifactWork<'_>>, bytes: usize) -> Result<(), ExecutionError> {
    if let Some(work) = work {
        work(1, bytes as u64)?;
    }
    Ok(())
}

#[derive(Debug)]
pub(crate) enum ArtifactVisitError<E> {
    Execution(ExecutionError),
    Tree(String),
    Visitor(E),
}

impl<E> From<ExecutionError> for ArtifactVisitError<E> {
    fn from(error: ExecutionError) -> Self {
        Self::Execution(error)
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

    /// Charge actual empty-tree initialization on a cold cache and a cancellable
    /// cache access on a hit. Failed initialization never enters the cache.
    pub fn empty_with_work(
        kind: ArtifactKindV1,
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<Self, ExecutionError> {
        with_artifact_work(work, |work, tree| {
            let hashes = codec::empty_with_meter(Some(tree)).map_err(ExecutionError::Tree)?;
            work(1, hashes[0].len() as u64)?;
            let value = Self {
                kind,
                count: 0,
                node_root: hashes[0].clone(),
            };
            work(1, 0)?;
            Ok(value)
        })
    }

    pub fn validate(&self) -> Result<(), ExecutionError> {
        self.validate_inner(None, None)
    }

    fn validate_inner(
        &self,
        work: Option<&ArtifactWork<'_>>,
        tree: Option<&crate::merkle::WorkMeter<'_>>,
    ) -> Result<(), ExecutionError> {
        artifact_work(work, self.node_root.len())?;
        parse_hash(&self.node_root)?;
        let hashes = codec::empty_with_meter(tree).map_err(ExecutionError::Tree)?;
        artifact_work(work, self.node_root.len())?;
        if (self.count == 0) != (self.node_root == hashes[0]) {
            return Err(ExecutionError::Invalid("artifact count/empty"));
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ExecutionError> {
        self.canonical_bytes_inner(None, None)
    }

    pub fn canonical_bytes_with_work(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<Vec<u8>, ExecutionError> {
        with_artifact_work(work, |work, tree| {
            self.canonical_bytes_inner(Some(work), Some(tree))
        })
    }

    fn canonical_bytes_inner(
        &self,
        work: Option<&ArtifactWork<'_>>,
        tree: Option<&crate::merkle::WorkMeter<'_>>,
    ) -> Result<Vec<u8>, ExecutionError> {
        self.validate_inner(work, tree)?;
        artifact_work(work, self.node_root.len())?;
        let root = parse_hash(&self.node_root)?;
        let mut w = match work {
            Some(work) => Writer::with_work(work),
            None => Writer::new(),
        };
        w.u8(self.kind as u8);
        w.u64(self.count);
        w.hash(&root);
        let bytes = w.finish(b"oasis7.iws.artifact-root.v1\0", 128)?;
        artifact_work(work, 0)?;
        Ok(bytes)
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ExecutionError> {
        Self::from_canonical_bytes_inner(bytes, None, None)
    }

    pub fn from_canonical_bytes_with_work(
        bytes: &[u8],
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<Self, ExecutionError> {
        with_artifact_work(work, |work, tree| {
            Self::from_canonical_bytes_inner(bytes, Some(work), Some(tree))
        })
    }

    fn from_canonical_bytes_inner(
        bytes: &[u8],
        work: Option<&ArtifactWork<'_>>,
        tree: Option<&crate::merkle::WorkMeter<'_>>,
    ) -> Result<Self, ExecutionError> {
        artifact_work(work, bytes.len().min(128))?;
        let mut r = Reader::new(bytes, b"oasis7.iws.artifact-root.v1\0", 128)?;
        artifact_work(work, 41 + 64)?;
        let value = Self {
            kind: ArtifactKindV1::from_tag(r.u8()?)?,
            count: r.u64()?,
            node_root: hex::encode(r.array::<32>()?),
        };
        r.finish()?;
        value.validate_inner(work, tree)?;
        artifact_work(work, 0)?;
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
        self.get_inner(id, source, None, None)
    }

    /// The caller's work hook runs before source attempts, hash/decode and
    /// authenticated-path comparisons; it does not qualify the source.
    pub fn get_with_work(
        &self,
        id: &[u8; 32],
        source: &impl MerkleNodeReader,
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<Option<ArtifactEntryV1>, ExecutionError> {
        with_artifact_work(work, |work, tree| {
            self.get_inner(id, source, Some(work), Some(tree))
        })
    }

    fn get_inner(
        &self,
        id: &[u8; 32],
        source: &impl MerkleNodeReader,
        work: Option<&ArtifactWork<'_>>,
        tree: Option<&crate::merkle::WorkMeter<'_>>,
    ) -> Result<Option<ArtifactEntryV1>, ExecutionError> {
        self.validate_inner(work, tree)?;
        let mut view = Updater::<ArtifactCodec, _>::new(
            source,
            codec::empty_with_meter(tree).map_err(ExecutionError::Tree)?,
            0,
        )
        .with_meter(tree);
        let entry = view
            .lookup(&self.node_root, id)
            .map_err(ExecutionError::Tree)?;
        artifact_work(work, std::mem::size_of::<ArtifactEntryV1>())?;
        if entry.is_some_and(|e| e.kind != self.kind) {
            return Err(ExecutionError::Invalid("artifact catalog kind"));
        }
        artifact_work(work, 0)?;
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
        self.append_inner(entries, source, memory, max_pending, None, None)
    }

    /// The same append and strict source-pool contract, with caller-owned work
    /// for traversal, encoding, hashing, comparisons and retained metadata.
    pub fn append_with_work(
        &self,
        entries: &[ArtifactEntryV1],
        source: &impl MerkleNodeReader,
        memory: &MemoryBudget,
        max_pending: usize,
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<ReservedValue<ArtifactCatalogDeltaV1>, ExecutionError> {
        with_artifact_work(work, |work, tree| {
            self.append_inner(entries, source, memory, max_pending, Some(work), Some(tree))
        })
    }

    fn append_inner(
        &self,
        entries: &[ArtifactEntryV1],
        source: &impl MerkleNodeReader,
        memory: &MemoryBudget,
        max_pending: usize,
        work: Option<&ArtifactWork<'_>>,
        tree: Option<&crate::merkle::WorkMeter<'_>>,
    ) -> Result<ReservedValue<ArtifactCatalogDeltaV1>, ExecutionError> {
        artifact_work(work, 0)?;
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
        self.validate_inner(work, tree)?;
        let mut previous = None;
        let mut view = Updater::<ArtifactCodec, _>::new(
            source,
            codec::empty_with_meter(tree).map_err(ExecutionError::Tree)?,
            max_pending,
        )
        .with_meter(tree)
        .track_parent_nodes();
        artifact_work(work, self.node_root.len())?;
        let mut after = self.clone();
        for entry in entries {
            artifact_work(work, std::mem::size_of::<ArtifactEntryV1>() + 32)?;
            if entry.kind != self.kind || previous.is_some_and(|id| id >= entry.original.id) {
                return Err(ExecutionError::Invalid("artifact append kind/order"));
            }
            previous = Some(entry.original.id);
            if let Some(old) = view
                .lookup(&after.node_root, &entry.original.id)
                .map_err(ExecutionError::Tree)?
            {
                artifact_work(work, std::mem::size_of::<ArtifactEntryV1>())?;
                if old != *entry {
                    return Err(ExecutionError::Invalid("artifact identity length/kind"));
                }
                continue;
            }
            after.node_root = view
                .insert(&after.node_root, 0, entry)
                .map_err(ExecutionError::Tree)?
                .0;
            artifact_work(work, 0)?;
            after.count = after
                .count
                .checked_add(1)
                .ok_or(ExecutionError::Invalid("artifact count overflow"))?;
        }
        let final_nodes = view
            .final_writes(&after.node_root)
            .map_err(ExecutionError::Tree)?;
        let new_nodes = if let Some(work) = work {
            work(
                1,
                (final_nodes.len() * std::mem::size_of::<ArtifactPageWriteV1>()) as u64,
            )?;
            let mut nodes = Vec::with_capacity(final_nodes.len());
            for (digest, bytes) in final_nodes {
                work(1, 0)?;
                nodes.push(ArtifactPageWriteV1 { digest, bytes });
            }
            nodes
        } else {
            final_nodes
                .into_iter()
                .map(|(digest, bytes)| ArtifactPageWriteV1 { digest, bytes })
                .collect()
        };
        artifact_work(work, self.node_root.len())?;
        let before = self.clone();
        artifact_work(work, std::mem::size_of_val(entries))?;
        let delta = ArtifactCatalogDeltaV1 {
            before,
            after,
            entries: entries.to_vec(),
            new_nodes,
        };
        let mut node_bytes = 0;
        for node in &delta.new_nodes {
            artifact_work(work, std::mem::size_of::<ArtifactPageWriteV1>())?;
            node_bytes += node.bytes.capacity() + node.digest.capacity();
        }
        let size = 512
            + delta.entries.capacity() * std::mem::size_of::<ArtifactEntryV1>()
            + delta.new_nodes.capacity() * std::mem::size_of::<ArtifactPageWriteV1>()
            + node_bytes;
        let charge = phase
            .split(size)
            .map_err(|_| ExecutionError::Invalid("artifact delta admission"))?;
        artifact_work(work, 0)?;
        Ok(charge.retain(delta))
    }
}

impl ArtifactCatalogV1 {
    /// Caller admits DFS workspace and controls source/visitor quotas. All
    /// visitor effects remain provisional until the exact count is checked.
    pub(crate) fn visit<E>(
        &self,
        source: &impl MerkleNodeReader,
        visitor: impl FnMut(&ArtifactEntryV1) -> Result<(), E>,
    ) -> Result<u64, VisitError<E>> {
        self.visit_inner(source, visitor, None, None)
            .map_err(|error| match error {
                ArtifactVisitError::Execution(error) => VisitError::Tree(error.to_string()),
                ArtifactVisitError::Tree(error) => VisitError::Tree(error),
                ArtifactVisitError::Visitor(error) => VisitError::Visitor(error),
            })
    }

    /// Stream the same authenticated leaves without materializing the catalog.
    /// The hook pays before source attempts and keeps its original typed error.
    pub(crate) fn visit_with_work<E>(
        &self,
        source: &impl MerkleNodeReader,
        visitor: impl FnMut(&ArtifactEntryV1) -> Result<(), E>,
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<u64, ArtifactVisitError<E>> {
        with_artifact_work(work, |work, tree| {
            self.visit_inner(source, visitor, Some(work), Some(tree))
        })
    }

    fn visit_inner<E>(
        &self,
        source: &impl MerkleNodeReader,
        mut visitor: impl FnMut(&ArtifactEntryV1) -> Result<(), E>,
        work: Option<&ArtifactWork<'_>>,
        tree: Option<&crate::merkle::WorkMeter<'_>>,
    ) -> Result<u64, ArtifactVisitError<E>> {
        self.validate_inner(work, tree)?;
        let mut view = Updater::<ArtifactCodec, _>::new(
            source,
            codec::empty_with_meter(tree).map_err(ArtifactVisitError::Tree)?,
            0,
        )
        .with_meter(tree);
        let count = view
            .visit_leaves(&self.node_root, |entry| {
                artifact_work(work, std::mem::size_of::<ArtifactEntryV1>())?;
                if entry.kind != self.kind {
                    return Err(ArtifactVisitError::Tree("artifact catalog kind".into()));
                }
                visitor(entry).map_err(ArtifactVisitError::Visitor)
            })
            .map_err(|error| match error {
                VisitError::Tree(error) => ArtifactVisitError::Tree(error),
                VisitError::Visitor(error) => error,
            })?;
        artifact_work(work, std::mem::size_of::<u64>())?;
        if count != self.count {
            return Err(ArtifactVisitError::Tree("artifact catalog count".into()));
        }
        artifact_work(work, 0)?;
        Ok(count)
    }
}

#[cfg(test)]
#[path = "incremental_artifact_tests.rs"]
mod tests;
