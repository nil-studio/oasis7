//! Full fixed-root scan for genesis/bootstrap/recovery, never ordinary commit.
use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, MemoryLimitExceeded};
use std::cell::{Cell, RefCell};

pub struct StateScanBudget {
    pub nodes_remaining: u64,
    pub read_bytes_remaining: u64,
    pub leaves_remaining: u64,
}
#[derive(Debug)]
pub enum StateScanError<E> {
    Tree(String),
    Visitor(E),
    Memory(MemoryLimitExceeded),
    Invalid(&'static str),
    ResourceLimited,
    Cancelled,
}
impl<E: std::fmt::Display> std::fmt::Display for StateScanError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tree(error) => write!(f, "state scan tree: {error}"),
            Self::Visitor(error) => error.fmt(f),
            Self::Memory(error) => error.fmt(f),
            Self::Invalid(reason) => write!(f, "invalid state scan: {reason}"),
            Self::ResourceLimited => f.write_str("state scan resource limited"),
            Self::Cancelled => f.write_str("state scan cancelled"),
        }
    }
}
impl<E: std::error::Error + 'static> std::error::Error for StateScanError<E> {}
pub struct StateScanContext<'a, R> {
    pub source: &'a R,
    pub memory: &'a MemoryBudget,
    pub class: MemoryClass,
    pub io: &'a mut StateScanBudget,
}
#[derive(Clone, Copy)]
enum Stop {
    Limited,
    Cancelled,
}
struct Guarded<'a, R, C> {
    source: &'a R,
    nodes: &'a Cell<u64>,
    bytes: &'a Cell<u64>,
    cancelled: &'a RefCell<C>,
    stop: &'a Cell<Option<Stop>>,
}
impl<R: MerkleNodeReader, C: FnMut() -> bool> MerkleNodeReader for Guarded<'_, R, C> {
    type Node = R::Node;
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        if (self.cancelled.borrow_mut())() {
            self.stop.set(Some(Stop::Cancelled));
            return Err("state scan cancelled".into());
        }
        if self.nodes.get() == 0 || self.bytes.get() == 0 {
            self.stop.set(Some(Stop::Limited));
            return Err("state scan allowance".into());
        }
        self.nodes.set(self.nodes.get() - 1);
        let admitted = (max as u64).min(self.bytes.get()) as usize;
        self.bytes.set(self.bytes.get() - admitted as u64);
        let result = self.source.read_node(id, admitted)?;
        if let Some(bytes) = &result {
            if bytes.as_ref().len() > admitted {
                return Err("state scan source bound".into());
            }
            self.bytes
                .set(self.bytes.get() + (admitted - bytes.as_ref().len()) as u64);
        }
        Ok(result)
    }
}
impl<R: MerkleNodeReader> StateScanContext<'_, R> {
    /// Visitor borrows one decoded, authenticated leaf. Retained copies need
    /// their own admission. Visitor effects are provisional until scan success;
    /// a visitor error cannot return a partial scan count.
    pub fn visit_entries<E>(
        &mut self,
        root: &StateRootV1,
        mut visit: impl FnMut(&StateEntryV1) -> Result<(), E>,
        cancelled: impl FnMut() -> bool,
    ) -> Result<u64, StateScanError<E>> {
        let _workspace = self
            .memory
            .reserve(128 * 1024, self.class)
            .map_err(StateScanError::Memory)?;
        if self.source.allocation_class() != Some(self.class)
            || !self
                .source
                .allocation_budget()
                .is_some_and(|pool| pool.same_pool(self.memory))
        {
            return Err(StateScanError::Invalid("state scan source pool/class"));
        }
        root.validate().map_err(StateScanError::Tree)?;
        if root.leaf_count > self.io.leaves_remaining {
            return Err(StateScanError::ResourceLimited);
        }
        let cancelled = RefCell::new(cancelled);
        if (cancelled.borrow_mut())() {
            return Err(StateScanError::Cancelled);
        }
        let stop = Cell::new(None);
        let source = Guarded {
            source: self.source,
            nodes: Cell::from_mut(&mut self.io.nodes_remaining),
            bytes: Cell::from_mut(&mut self.io.read_bytes_remaining),
            cancelled: &cancelled,
            stop: &stop,
        };
        let mut view = Updater::<StateCodec, _>::new(
            &source,
            empty_hashes().map_err(StateScanError::Tree)?,
            0,
        );
        let result = view.visit_leaves(&root.node_root, |entry| {
            if (cancelled.borrow_mut())() {
                return Err(StateScanError::Cancelled);
            }
            if entry.namespace != root.namespace {
                return Err(StateScanError::Invalid("state scan namespace"));
            }
            if self.io.leaves_remaining == 0 {
                return Err(StateScanError::ResourceLimited);
            }
            self.io.leaves_remaining -= 1;
            visit(entry).map_err(StateScanError::Visitor)
        });
        match stop.get() {
            Some(Stop::Limited) => return Err(StateScanError::ResourceLimited),
            Some(Stop::Cancelled) => return Err(StateScanError::Cancelled),
            None => {}
        }
        let count = result.map_err(|error| match error {
            crate::merkle::VisitError::Tree(error) => StateScanError::Tree(error),
            crate::merkle::VisitError::Visitor(error) => error,
        })?;
        if count != root.leaf_count {
            return Err(StateScanError::Invalid("state scan leaf count"));
        }
        if (cancelled.borrow_mut())() {
            return Err(StateScanError::Cancelled);
        }
        Ok(count)
    }
}
