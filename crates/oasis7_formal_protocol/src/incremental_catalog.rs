use super::*;
use crate::merkle::Updater;
use catalog_codec::{CatalogCodec, CatalogLeaf, empty_hashes, query_key};
use wire::{Reader, Writer, digest, parse_hash};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRootV1 {
    pub scope: WorldScopeV1,
    /// A verified prefix contains exactly heights 0..count-1.
    pub count: u64,
    pub node_root: String,
    pub last_commit: Option<[u8; 32]>,
    pub last_checkpoint: Option<CheckpointPointerV1>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntryV1 {
    pub scope: WorldScopeV1,
    pub height: u64,
    pub commit: [u8; 32],
    pub parent_commit: Option<[u8; 32]>,
    pub checkpoint: Option<CheckpointPointerV1>,
    pub latest_checkpoint: Option<CheckpointPointerV1>,
}
impl CatalogEntryV1 {
    pub fn validate(&self) -> Result<(), ExecutionError> {
        self.scope.validate()?;
        if self.parent_commit.is_none() != (self.height == 0)
            || self.checkpoint.is_some_and(|cp| cp.height != self.height)
            || self
                .latest_checkpoint
                .is_some_and(|cp| cp.height > self.height)
            || (self.checkpoint.is_some() && self.latest_checkpoint != self.checkpoint)
        {
            return Err(ExecutionError::Invalid("catalog entry boundaries"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogQueryV1 {
    Height(u64),
    Commit([u8; 32]),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogNodeWriteV1 {
    pub digest: String,
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogDeltaV1 {
    pub before: CatalogRootV1,
    pub after: CatalogRootV1,
    pub entry: CatalogEntryV1,
    pub new_nodes: Vec<CatalogNodeWriteV1>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogProofV1 {
    pub root: [u8; 32],
    pub query: CatalogQueryV1,
    pub nodes: Vec<CatalogNodeWriteV1>,
}

impl CatalogRootV1 {
    pub fn empty(scope: WorldScopeV1) -> Result<Self, ExecutionError> {
        scope.validate()?;
        Ok(Self {
            scope,
            count: 0,
            node_root: empty_hashes()?[0].clone(),
            last_commit: None,
            last_checkpoint: None,
        })
    }
    pub fn validate(&self) -> Result<(), ExecutionError> {
        self.scope.validate()?;
        parse_hash(&self.node_root)?;
        if (self.count == 0) != (self.node_root == empty_hashes()?[0])
            || (self.count == 0) != self.last_commit.is_none()
            || self
                .last_checkpoint
                .is_some_and(|cp| cp.height >= self.count)
        {
            return Err(ExecutionError::Invalid("catalog root prefix metadata"));
        }
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ExecutionError> {
        self.validate()?;
        let mut out = Writer::new();
        out.scope(&self.scope);
        out.u64(self.count);
        out.hash(&parse_hash(&self.node_root)?);
        out.optional(&self.last_commit);
        out.checkpoint(&self.last_checkpoint);
        out.finish(b"oasis7.iws.catalog-root.v1\0", HISTORY_MAX_HEADER_BYTES)
    }
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ExecutionError> {
        let mut r = Reader::new(
            bytes,
            b"oasis7.iws.catalog-root.v1\0",
            HISTORY_MAX_HEADER_BYTES,
        )?;
        let value = Self {
            scope: r.scope()?,
            count: r.u64()?,
            node_root: hex::encode(r.array::<32>()?),
            last_commit: r.optional()?,
            last_checkpoint: r.checkpoint()?,
        };
        r.finish()?;
        value.validate()?;
        Ok(value)
    }
    pub fn digest(&self) -> Result<[u8; 32], ExecutionError> {
        Ok(digest(&self.canonical_bytes()?))
    }

    fn lookup_leaf(
        &self,
        query: CatalogQueryV1,
        source: &impl MerkleNodeReader,
    ) -> Result<Option<CatalogLeaf>, ExecutionError> {
        let mut reader = Updater::<CatalogCodec, _>::new(source, empty_hashes()?, 0);
        reader
            .lookup(&self.node_root, &query_key(&self.scope, query)?)
            .map_err(ExecutionError::Tree)
    }
    /// The root must come from a qualified body/prefix, never a local mtime or
    /// incomplete index. An in-prefix missing key is corruption, not absence.
    pub fn lookup(
        &self,
        query: CatalogQueryV1,
        source: &impl MerkleNodeReader,
    ) -> Result<Option<CatalogEntryV1>, ExecutionError> {
        self.validate()?;
        let height = match query {
            CatalogQueryV1::Height(height) => height,
            CatalogQueryV1::Commit(commit) => match self.lookup_leaf(query, source)? {
                Some(CatalogLeaf::Commit {
                    scope,
                    commit: found,
                    height,
                }) if scope == self.scope && found == commit && height < self.count => height,
                None => return Ok(None),
                _ => return Err(ExecutionError::Invalid("catalog commit locator binding")),
            },
        };
        if height >= self.count {
            return Ok(None);
        }
        match self.lookup_leaf(CatalogQueryV1::Height(height), source)? {
            Some(CatalogLeaf::Height(entry))
                if entry.scope == self.scope && entry.height == height =>
            {
                if let CatalogQueryV1::Commit(commit) = query
                    && entry.commit != commit
                {
                    return Err(ExecutionError::Invalid("catalog locator/entry mismatch"));
                }
                entry.validate()?;
                Ok(Some(entry))
            }
            _ => Err(ExecutionError::Invalid(
                "catalog prefix incomplete or wrong entry",
            )),
        }
    }

    /// Build only a content extension. Original certificate/qualification is a
    /// separate obligation before this candidate can become a selected head.
    pub fn extend(
        &self,
        body: &CommitBodyV1,
        source: &impl MerkleNodeReader,
        memory: &MemoryBudget,
        max_pending_bytes: usize,
    ) -> Result<ReservedValue<CatalogDeltaV1>, ExecutionError> {
        self.validate()?;
        body.validate()?;
        if body.catalog_before != *self {
            return Err(ExecutionError::ParentConflict);
        }
        let count = self
            .count
            .checked_add(1)
            .ok_or(ExecutionError::ResourceLimited("catalog count overflow"))?;
        let mut workspace = memory.reserve(2 * 1024 * 1024, MemoryClass::Ordinary)?;
        if self.count > 0 {
            let last = self
                .lookup(CatalogQueryV1::Height(self.count - 1), source)?
                .ok_or(ExecutionError::Invalid("last catalog entry missing"))?;
            if Some(last.commit) != self.last_commit
                || last.latest_checkpoint != self.last_checkpoint
            {
                return Err(ExecutionError::Invalid("catalog tail metadata mismatch"));
            }
        }
        let commit = body.digest()?;
        let checkpoint = body.checkpoint.map(|descriptor| CheckpointPointerV1 {
            height: body.height,
            descriptor,
        });
        let entry = CatalogEntryV1 {
            scope: self.scope.clone(),
            height: body.height,
            commit,
            parent_commit: body.parent_commit,
            checkpoint,
            latest_checkpoint: checkpoint.or(self.last_checkpoint),
        };
        entry.validate()?;
        let leaves = [
            CatalogLeaf::Height(entry.clone()),
            CatalogLeaf::Commit {
                scope: self.scope.clone(),
                commit,
                height: body.height,
            },
        ];
        let mut leaves = leaves
            .into_iter()
            .map(|leaf| Ok((CatalogCodec::key_digest(&leaf)?, leaf)))
            .collect::<Result<Vec<_>, ExecutionError>>()?;
        leaves.sort_unstable_by_key(|(key, _)| *key);
        let mut updater =
            Updater::<CatalogCodec, _>::new(source, empty_hashes()?, max_pending_bytes)
                .track_parent_nodes();
        let mut node_root = self.node_root.clone();
        for (key, leaf) in &leaves {
            if updater
                .lookup(&node_root, key)
                .map_err(ExecutionError::Tree)?
                .is_some()
            {
                return Err(ExecutionError::Invalid(
                    "catalog append overwrites existing key",
                ));
            }
            node_root = updater
                .insert(&node_root, 0, leaf)
                .map_err(ExecutionError::Tree)?
                .0;
        }
        let new_nodes = updater
            .final_writes(&node_root)
            .map_err(ExecutionError::Tree)?
            .into_iter()
            .map(|(digest, bytes)| CatalogNodeWriteV1 { digest, bytes })
            .collect();
        let after = Self {
            scope: self.scope.clone(),
            count,
            node_root,
            last_commit: Some(commit),
            last_checkpoint: entry.latest_checkpoint,
        };
        after.validate()?;
        let value = CatalogDeltaV1 {
            before: self.clone(),
            after,
            entry,
            new_nodes,
        };
        let bytes = value.heap_bytes();
        let charge = workspace
            .split(bytes)
            .map_err(|_| ExecutionError::Invalid("catalog extension accounting"))?;
        Ok(charge.retain(value))
    }

    pub fn prove(
        &self,
        query: CatalogQueryV1,
        source: &impl MerkleNodeReader,
        memory: &MemoryBudget,
    ) -> Result<ReservedValue<CatalogProofV1>, ExecutionError> {
        use std::cell::RefCell;
        struct Recording<'a, R> {
            source: &'a R,
            nodes: RefCell<Vec<CatalogNodeWriteV1>>,
        }
        impl<R: MerkleNodeReader> MerkleNodeReader for Recording<'_, R> {
            fn read_node(&self, digest: &str, max: usize) -> Result<Option<Vec<u8>>, String> {
                let bytes = self.source.read_node(digest, max)?;
                if let Some(bytes) = &bytes {
                    if bytes.len() > max {
                        return Err("catalog source byte bound".into());
                    }
                    self.nodes.borrow_mut().push(CatalogNodeWriteV1 {
                        digest: digest.into(),
                        bytes: bytes.clone(),
                    });
                }
                Ok(bytes)
            }
        }
        let mut workspace = memory.reserve(2 * 1024 * 1024, MemoryClass::Ordinary)?;
        let recording = Recording {
            source,
            nodes: RefCell::new(Vec::new()),
        };
        self.lookup(query, &recording)?;
        let value = CatalogProofV1 {
            root: self.digest()?,
            query,
            nodes: recording.nodes.into_inner(),
        };
        let bytes = node_bytes(&value.nodes) + std::mem::size_of::<CatalogProofV1>() + 64;
        let charge = workspace
            .split(bytes)
            .map_err(|_| ExecutionError::Invalid("catalog proof accounting"))?;
        Ok(charge.retain(value))
    }
}

impl CatalogDeltaV1 {
    pub fn verify(
        &self,
        expected_parent: &CatalogRootV1,
        body: &CommitBodyV1,
        source: &impl MerkleNodeReader,
        memory: &MemoryBudget,
        max_pending_bytes: usize,
    ) -> Result<(), ExecutionError> {
        if self.before != *expected_parent {
            return Err(ExecutionError::ParentConflict);
        }
        let rebuilt = expected_parent.extend(body, source, memory, max_pending_bytes)?;
        if *rebuilt != *self {
            return Err(ExecutionError::Invalid("catalog extension not canonical"));
        }
        Ok(())
    }
    fn heap_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + 64
            + node_bytes(&self.new_nodes)
            + self.before.scope.world_id.capacity()
            + self.after.scope.world_id.capacity()
            + self.before.node_root.capacity()
            + self.after.node_root.capacity()
            + self.entry.scope.world_id.capacity()
    }
}
fn node_bytes(nodes: &Vec<CatalogNodeWriteV1>) -> usize {
    nodes.capacity() * std::mem::size_of::<CatalogNodeWriteV1>()
        + nodes
            .iter()
            .map(|node| node.digest.capacity() + node.bytes.capacity())
            .sum::<usize>()
}
impl CatalogProofV1 {
    pub fn canonical_bytes(
        &self,
        memory: &MemoryBudget,
    ) -> Result<ReservedValue<Vec<u8>>, ExecutionError> {
        if self.nodes.len() > 514
            || self
                .nodes
                .iter()
                .any(|node| node.bytes.len() > CATALOG_MAX_NODE_BYTES)
        {
            return Err(ExecutionError::Invalid("catalog proof encoding bound"));
        }
        let mut workspace = memory.reserve(2 * 1024 * 1024, MemoryClass::Ordinary)?;
        let mut out = Writer::new();
        out.hash(&self.root);
        out.query(self.query);
        out.u16(self.nodes.len() as u16);
        for node in &self.nodes {
            out.hash(&parse_hash(&node.digest)?);
            out.nested(&node.bytes);
        }
        let bytes = out.finish(b"oasis7.iws.catalog-proof.v1\0", 600 * 1024)?;
        let charge = workspace
            .split(bytes.capacity() + 64)
            .map_err(|_| ExecutionError::Invalid("proof encoding accounting"))?;
        Ok(charge.retain(bytes))
    }

    /// Bind the authorized root/query before decoding the bounded node list.
    /// The returned packet still needs `verify` before any semantic use.
    pub fn from_canonical_bytes(
        bytes: &[u8],
        expected_root: &CatalogRootV1,
        expected_query: CatalogQueryV1,
        memory: &MemoryBudget,
    ) -> Result<ReservedValue<Self>, ExecutionError> {
        let mut r = Reader::new(bytes, b"oasis7.iws.catalog-proof.v1\0", 600 * 1024)?;
        let mut workspace = memory.reserve(2 * 1024 * 1024, MemoryClass::Ordinary)?;
        let root = r.array()?;
        let query = r.query()?;
        if root != expected_root.digest()? || query != expected_query {
            return Err(ExecutionError::Invalid(
                "proof requested root/range mismatch",
            ));
        }
        let count = usize::from(r.u16()?);
        if count > 514 {
            return Err(ExecutionError::Invalid("proof decode node count"));
        }
        let mut nodes = Vec::with_capacity(count);
        for _ in 0..count {
            let digest = hex::encode(r.array::<32>()?);
            let length = usize::from(r.u16()?);
            if length > CATALOG_MAX_NODE_BYTES {
                return Err(ExecutionError::Invalid("proof decode page bound"));
            }
            nodes.push(CatalogNodeWriteV1 {
                digest,
                bytes: r.take(length)?.to_vec(),
            });
        }
        r.finish()?;
        let value = Self { root, query, nodes };
        let bytes = node_bytes(&value.nodes) + std::mem::size_of::<Self>() + 64;
        let charge = workspace
            .split(bytes)
            .map_err(|_| ExecutionError::Invalid("proof decode accounting"))?;
        Ok(charge.retain(value))
    }

    pub fn verify(
        &self,
        expected_root: &CatalogRootV1,
        expected_query: CatalogQueryV1,
        memory: &MemoryBudget,
    ) -> Result<ReservedValue<Option<CatalogEntryV1>>, ExecutionError> {
        use std::cell::Cell;
        let mut workspace = memory.reserve(64 * 1024, MemoryClass::Ordinary)?;
        if self.root != expected_root.digest()?
            || self.query != expected_query
            || self.nodes.len() > 514
            || self
                .nodes
                .iter()
                .any(|node| node.bytes.len() > CATALOG_MAX_NODE_BYTES)
        {
            return Err(ExecutionError::Invalid("catalog proof binding/bound"));
        }
        struct Ordered<'a> {
            nodes: &'a [CatalogNodeWriteV1],
            next: Cell<usize>,
        }
        impl MerkleNodeReader for Ordered<'_> {
            fn read_node(&self, digest: &str, max: usize) -> Result<Option<Vec<u8>>, String> {
                let next = self.next.get();
                let node = self.nodes.get(next).ok_or("catalog proof incomplete")?;
                if node.digest != digest || node.bytes.len() > max {
                    return Err("catalog proof order/bound".into());
                }
                self.next.set(next + 1);
                Ok(Some(node.bytes.clone()))
            }
        }
        let ordered = Ordered {
            nodes: &self.nodes,
            next: Cell::new(0),
        };
        let value = expected_root.lookup(expected_query, &ordered)?;
        if ordered.next.get() != self.nodes.len() {
            return Err(ExecutionError::Invalid("catalog proof extra nodes"));
        }
        let retained = std::mem::size_of::<Option<CatalogEntryV1>>()
            + 64
            + value
                .as_ref()
                .map_or(0, |entry| entry.scope.world_id.capacity());
        let charge = workspace
            .split(retained)
            .map_err(|_| ExecutionError::Invalid("catalog proof output accounting"))?;
        Ok(charge.retain(value))
    }
}
