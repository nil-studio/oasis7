use super::*;
use crate::merkle::Updater;
use catalog_codec::{CatalogCodec, CatalogLeaf, empty_hashes};
use wire::{Reader, Writer, digest, parse_hash};

type CatalogWork<'a> = dyn Fn(u64, u64) -> Result<(), ExecutionError> + 'a;
fn with_catalog_work<T>(
    work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    run: impl FnOnce(&CatalogWork<'_>, &crate::merkle::WorkMeter<'_>) -> Result<T, ExecutionError>,
) -> Result<T, ExecutionError> {
    let work = std::cell::RefCell::new(work);
    let first = std::cell::RefCell::new(None);
    let pay = |s, b| (work.borrow_mut())(s, b);
    let tree = |bytes: usize| {
        if first.borrow().is_some() {
            return Err("catalog meter stopped".to_owned());
        }
        match pay(1, bytes as u64) {
            Ok(()) => Ok(()),
            Err(error) => {
                *first.borrow_mut() = Some(error);
                Err("catalog meter stopped".to_owned())
            }
        }
    };
    let result = run(&pay, &tree);
    if let Some(error) = first.into_inner() {
        Err(error)
    } else {
        result
    }
}
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
        self.validate_with_work(&mut |_, _| Ok(()))
    }
    pub fn validate_with_work(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<(), ExecutionError> {
        work(1, (self.scope.world_id.len() + self.node_root.len()) as u64)?;
        self.scope.validate()?;
        parse_hash(&self.node_root)?;
        if (self.count == 0) != (self.node_root == catalog_codec::empty_hashes_with_work(work)?[0])
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
        self.canonical_bytes_with_work(&mut |_, _| Ok(()))
    }
    pub fn canonical_bytes_with_work(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<Vec<u8>, ExecutionError> {
        self.validate_with_work(work)?;
        let work = std::cell::RefCell::new(work);
        let meter = |s, b| (work.borrow_mut())(s, b);
        let mut out = Writer::with_work(&meter);
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
        self.digest_with_work(&mut |_, _| Ok(()))
    }
    pub fn digest_with_work(
        &self,
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<[u8; 32], ExecutionError> {
        let bytes = self.canonical_bytes_with_work(work)?;
        work(1, bytes.len() as u64)?;
        Ok(digest(&bytes))
    }

    fn lookup_leaf(
        &self,
        query: CatalogQueryV1,
        source: &impl MerkleNodeReader,
        work: &CatalogWork<'_>,
        tree: &crate::merkle::WorkMeter<'_>,
    ) -> Result<Option<CatalogLeaf>, ExecutionError> {
        let empty = catalog_codec::empty_hashes_with_work(&mut |s, b| work(s, b))?;
        let mut reader = Updater::<CatalogCodec, _>::new(source, empty, 0).with_meter(Some(tree));
        let key = catalog_codec::query_key_with_work(&self.scope, query, &mut |s, b| work(s, b))?;
        reader
            .lookup(&self.node_root, &key)
            .map_err(ExecutionError::Tree)
    }
    /// The root must come from a qualified body/prefix, never a local mtime or
    /// incomplete index. An in-prefix missing key is corruption, not absence.
    pub fn lookup(
        &self,
        query: CatalogQueryV1,
        source: &impl MerkleNodeReader,
    ) -> Result<Option<CatalogEntryV1>, ExecutionError> {
        self.lookup_with_work(query, source, &mut |_, _| Ok(()))
    }
    pub fn lookup_with_work(
        &self,
        query: CatalogQueryV1,
        source: &impl MerkleNodeReader,
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<Option<CatalogEntryV1>, ExecutionError> {
        with_catalog_work(work, |work, tree| {
            self.lookup_inner(query, source, work, tree)
        })
    }
    fn lookup_inner(
        &self,
        query: CatalogQueryV1,
        source: &impl MerkleNodeReader,
        work: &CatalogWork<'_>,
        tree: &crate::merkle::WorkMeter<'_>,
    ) -> Result<Option<CatalogEntryV1>, ExecutionError> {
        self.validate_with_work(&mut |s, b| work(s, b))?;
        let height = match query {
            CatalogQueryV1::Height(height) => height,
            CatalogQueryV1::Commit(commit) => match self.lookup_leaf(query, source, work, tree)? {
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
        match self.lookup_leaf(CatalogQueryV1::Height(height), source, work, tree)? {
            Some(CatalogLeaf::Height(entry))
                if entry.scope == self.scope && entry.height == height =>
            {
                if let CatalogQueryV1::Commit(commit) = query
                    && entry.commit != commit
                {
                    return Err(ExecutionError::Invalid("catalog locator/entry mismatch"));
                }
                work(1, entry.scope.world_id.len() as u64)?;
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
        self.extend_with_work(body, source, memory, max_pending_bytes, &mut |_, _| Ok(()))
    }
    pub fn extend_with_work(
        &self,
        body: &CommitBodyV1,
        source: &impl MerkleNodeReader,
        memory: &MemoryBudget,
        max_pending_bytes: usize,
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<ReservedValue<CatalogDeltaV1>, ExecutionError> {
        with_catalog_work(work, |work, tree| {
            self.extend_inner(body, source, memory, max_pending_bytes, work, tree)
        })
    }
    fn extend_inner(
        &self,
        body: &CommitBodyV1,
        source: &impl MerkleNodeReader,
        memory: &MemoryBudget,
        max_pending_bytes: usize,
        work: &CatalogWork<'_>,
        tree: &crate::merkle::WorkMeter<'_>,
    ) -> Result<ReservedValue<CatalogDeltaV1>, ExecutionError> {
        work(1, 0)?;
        crate::incremental_execution::validate_source_budget(
            source,
            memory,
            MemoryClass::Ordinary,
        )?;
        self.validate_with_work(&mut |s, b| work(s, b))?;
        work(
            1,
            (body.scope.world_id.len()
                + self.scope.world_id.len()
                + body.catalog_before.node_root.len()
                + self.node_root.len()) as u64,
        )?;
        body.validate_with_work(&mut |s, b| work(s, b))?;
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
                .lookup_inner(CatalogQueryV1::Height(self.count - 1), source, work, tree)?
                .ok_or(ExecutionError::Invalid("last catalog entry missing"))?;
            if Some(last.commit) != self.last_commit
                || last.latest_checkpoint != self.last_checkpoint
            {
                return Err(ExecutionError::Invalid("catalog tail metadata mismatch"));
            }
        }
        let commit = body.digest_with_work(&mut |s, b| work(s, b))?;
        let checkpoint = body.checkpoint.map(|descriptor| CheckpointPointerV1 {
            height: body.height,
            descriptor,
        });
        work(1, self.scope.world_id.len() as u64)?;
        let entry = CatalogEntryV1 {
            scope: self.scope.clone(),
            height: body.height,
            commit,
            parent_commit: body.parent_commit,
            checkpoint,
            latest_checkpoint: checkpoint.or(self.last_checkpoint),
        };
        entry.validate()?;
        work(2, (2 * self.scope.world_id.len()) as u64)?;
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
            .map(|leaf| {
                Ok((
                    CatalogCodec::key_digest_with_work(&leaf, &mut |s, b| work(s, b))?,
                    leaf,
                ))
            })
            .collect::<Result<Vec<_>, ExecutionError>>()?;
        work(1, 64)?;
        leaves.sort_unstable_by_key(|(key, _)| *key);
        let mut updater = Updater::<CatalogCodec, _>::new(
            source,
            catalog_codec::empty_hashes_with_work(&mut |s, b| work(s, b))?,
            max_pending_bytes,
        )
        .with_meter(Some(tree))
        .track_parent_nodes();
        work(1, self.node_root.len() as u64)?;
        let mut node_root = self.node_root.clone();
        for (key, leaf) in &leaves {
            work(1, 32)?;
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
            .map(|(digest, bytes)| {
                work(1, 0)?;
                Ok(CatalogNodeWriteV1 { digest, bytes })
            })
            .collect::<Result<Vec<_>, ExecutionError>>()?;
        work(1, self.scope.world_id.len() as u64)?;
        let after = Self {
            scope: self.scope.clone(),
            count,
            node_root,
            last_commit: Some(commit),
            last_checkpoint: entry.latest_checkpoint,
        };
        after.validate_with_work(&mut |s, b| work(s, b))?;
        work(1, (self.scope.world_id.len() + self.node_root.len()) as u64)?;
        let value = CatalogDeltaV1 {
            before: self.clone(),
            after,
            entry,
            new_nodes,
        };
        let bytes = value.heap_bytes_with_work(work)?;
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
        crate::incremental_execution::validate_source_budget(
            source,
            memory,
            MemoryClass::Ordinary,
        )?;
        use std::cell::RefCell;
        struct Recording<'a, R> {
            source: &'a R,
            nodes: RefCell<Vec<CatalogNodeWriteV1>>,
        }
        impl<R: MerkleNodeReader> MerkleNodeReader for Recording<'_, R> {
            type Node = R::Node;
            fn allocation_budget(&self) -> Option<&MemoryBudget> {
                self.source.allocation_budget()
            }
            fn read_node(&self, digest: &str, max: usize) -> Result<Option<Self::Node>, String> {
                let bytes = self.source.read_node(digest, max)?;
                if let Some(bytes) = &bytes {
                    if bytes.as_ref().len() > max {
                        return Err("catalog source byte bound".into());
                    }
                    self.nodes.borrow_mut().push(CatalogNodeWriteV1 {
                        digest: digest.into(),
                        bytes: bytes.as_ref().to_vec(),
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
    fn heap_bytes_with_work(&self, work: &CatalogWork<'_>) -> Result<usize, ExecutionError> {
        let nodes = self.new_nodes.iter().try_fold(
            self.new_nodes.capacity() * std::mem::size_of::<CatalogNodeWriteV1>(),
            |bytes, node| {
                work(1, 0)?;
                Ok::<_, ExecutionError>(bytes + node.digest.capacity() + node.bytes.capacity())
            },
        )?;
        Ok(std::mem::size_of::<Self>()
            + 64
            + nodes
            + self.before.scope.world_id.capacity()
            + self.after.scope.world_id.capacity()
            + self.before.node_root.capacity()
            + self.after.node_root.capacity()
            + self.entry.scope.world_id.capacity())
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
            type Node = Vec<u8>;
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
