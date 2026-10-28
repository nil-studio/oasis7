//! Authenticated ordered wake index. A time tree points to full-digest instance
//! sets, avoiding truncated composite keys. This content index grants no
//! execution or serving authority. Runtime owns its bijection with wake fields.
use crate::incremental_execution::ExecutionError;
use crate::incremental_history::wire::{Reader, Writer, parse_hash};
use crate::incremental_state::MerkleNodeReader;
use crate::merkle::{Codec, DEPTH, Node, Updater};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

#[path = "incremental_schedule_codec.rs"]
mod codec;
use codec::{ScheduleCodec, empty, instance_key, time_key};
#[path = "incremental_schedule_scan.rs"]
mod scan;
#[path = "incremental_schedule_update.rs"]
mod update;
pub use crate::incremental_state::StateScanError as ScheduleScanError;
pub use scan::{ScheduleScanBudget, ScheduleScanContext};

pub const SCHEDULE_MAX_PAGE_BYTES: usize = 1200;
pub const SCHEDULE_MAX_INSTANCE_BYTES: usize = 1024;
const ROOT_MAGIC: &[u8] = b"oasis7.iws.schedule-index-root.v1\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SchedulePageKindV1 {
    Times = 1,
    Instances = 2,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleIndexV1 {
    pub buckets: u64,
    pub entries: u64,
    pub node_root: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulePageWriteV1 {
    pub kind: SchedulePageKindV1,
    pub digest: String,
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleDeltaV1 {
    pub before: ScheduleIndexV1,
    pub after: ScheduleIndexV1,
    pub instance: String,
    pub old_wake: Option<u64>,
    pub new_wake: Option<u64>,
    pub new_nodes: Vec<SchedulePageWriteV1>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulePageLinksV1 {
    Branch {
        kind: SchedulePageKindV1,
        depth: u16,
        children: [Option<[u8; 32]>; 2],
    },
    Bucket {
        wake: u64,
        count: u64,
        instances: [u8; 32],
    },
    Instance(String),
}
#[derive(Clone)]
enum Leaf {
    Time { wake: u64, count: u64, root: String },
    Instance(String),
}
impl ScheduleIndexV1 {
    pub fn empty() -> Result<Self, ExecutionError> {
        Ok(Self {
            buckets: 0,
            entries: 0,
            node_root: empty::<1>().map_err(ExecutionError::Tree)?[0].clone(),
        })
    }
    pub fn validate(&self) -> Result<(), ExecutionError> {
        parse_hash(&self.node_root)?;
        if self.buckets > self.entries
            || (self.entries == 0) != (self.buckets == 0)
            || (self.buckets == 0)
                != (self.node_root == empty::<1>().map_err(ExecutionError::Tree)?[0])
        {
            return Err(ExecutionError::Invalid("schedule counts/empty"));
        }
        Ok(())
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ExecutionError> {
        self.validate()?;
        let mut w = Writer::new();
        w.u64(self.buckets);
        w.u64(self.entries);
        w.hash(&parse_hash(&self.node_root)?);
        w.finish(ROOT_MAGIC, 128)
    }
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, ExecutionError> {
        let mut r = Reader::new(bytes, ROOT_MAGIC, 128)?;
        let root = Self {
            buckets: r.u64()?,
            entries: r.u64()?,
            node_root: hex::encode(r.array::<32>()?),
        };
        r.finish()?;
        root.validate()?;
        Ok(root)
    }
    pub fn digest(&self) -> Result<[u8; 32], ExecutionError> {
        Ok(*blake3::hash(&self.canonical_bytes()?).as_bytes())
    }
}
impl SchedulePageWriteV1 {
    /// Typed edges for retained-root traversal; this does not validate closure
    /// counts, scalar/index consistency, authorization or durable publication.
    pub fn verified_links(&self) -> Result<SchedulePageLinksV1, ExecutionError> {
        if blake3::hash(&self.bytes).to_hex().as_str() != self.digest {
            return Err(ExecutionError::Invalid("schedule page hash"));
        }
        match self.kind {
            SchedulePageKindV1::Times => self.links::<1>(),
            SchedulePageKindV1::Instances => self.links::<2>(),
        }
    }
    fn links<const K: u8>(&self) -> Result<SchedulePageLinksV1, ExecutionError> {
        match ScheduleCodec::<K>::decode(&self.bytes).map_err(ExecutionError::Tree)? {
            Node::Branch { depth, left, right } => {
                let sentinel = &empty::<K>().map_err(ExecutionError::Tree)?[depth + 1];
                let edge = |id: String| {
                    if id == *sentinel {
                        Ok(None)
                    } else {
                        parse_hash(&id).map(Some)
                    }
                };
                Ok(SchedulePageLinksV1::Branch {
                    kind: self.kind,
                    depth: depth as u16,
                    children: [edge(left)?, edge(right)?],
                })
            }
            Node::Leaf(Leaf::Time { wake, count, root }) => Ok(SchedulePageLinksV1::Bucket {
                wake,
                count,
                instances: parse_hash(&root)?,
            }),
            Node::Leaf(Leaf::Instance(id)) => Ok(SchedulePageLinksV1::Instance(id)),
        }
    }
}
impl ScheduleDeltaV1 {
    pub fn verify(
        &self,
        parent: [u8; 32],
        source: &impl MerkleNodeReader,
        memory: &MemoryBudget,
        max_pending: usize,
    ) -> Result<(), ExecutionError> {
        if self.before.digest()? != parent {
            return Err(ExecutionError::Invalid("schedule delta parent"));
        }
        let rebuilt = self.before.update(
            &self.instance,
            self.old_wake,
            self.new_wake,
            source,
            memory,
            max_pending,
        )?;
        if *rebuilt != *self {
            return Err(ExecutionError::Invalid("schedule delta final pages"));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "incremental_schedule_tests.rs"]
mod tests;
