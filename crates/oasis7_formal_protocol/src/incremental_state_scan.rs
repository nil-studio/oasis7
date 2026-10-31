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
pub(crate) enum Stop {
    Limited,
    Cancelled,
}
/// Reader view that charges all tree access to the active state scan.
///
/// Visitors may use this to perform bounded point lookups related to the
/// currently visited entry. Reads share the scan's node/byte allowance and
/// cancellation source. A point lookup does not consume another leaf-visit
/// allowance; `leaves_remaining` accounts only for the outer ordered scan.
pub struct StateScanReader<'a, R, C> {
    pub(crate) source: &'a R,
    pub(crate) memory: &'a MemoryBudget,
    pub(crate) class: MemoryClass,
    pub(crate) nodes: &'a Cell<u64>,
    pub(crate) bytes: &'a Cell<u64>,
    pub(crate) leaves: &'a Cell<u64>,
    pub(crate) cancelled: &'a RefCell<C>,
    pub(crate) stop: &'a Cell<Option<Stop>>,
}
pub(crate) type Guarded<'a, R, C> = StateScanReader<'a, R, C>;
impl<R: MerkleNodeReader, C: FnMut() -> bool> MerkleNodeReader for StateScanReader<'_, R, C> {
    type Node = R::Node;
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        if let Some(stop) = self.stop.get() {
            return Err(match stop {
                Stop::Limited => "state scan allowance",
                Stop::Cancelled => "state scan cancelled",
            }
            .into());
        }
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
        let result = self.source.read_node(id, admitted).inspect_err(|_| {
            // A string-only source cannot distinguish an over-bound page from
            // another read failure. With less than the requested maximum
            // admitted, treat rejection as incomplete work; retry with enough
            // allowance can still expose a genuine tree error. Explicit None
            // remains a missing-page result, never a resource-limit success.
            if admitted < max && self.stop.get().is_none() {
                self.stop.set(Some(Stop::Limited));
            }
        })?;
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
impl<R: MerkleNodeReader, C: FnMut() -> bool> StateScanReader<'_, R, C> {
    /// Whether the original scan source remains tied to the same classed
    /// memory pool used by an owner-specific value reader.
    pub fn source_matches(&self, memory: &MemoryBudget, class: MemoryClass) -> bool {
        self.class == class
            && self.memory.same_pool(memory)
            && self.source.allocation_class() == Some(class)
            && self
                .source
                .allocation_budget()
                .is_some_and(|source| source.same_pool(memory))
    }

    /// Visit every authenticated leaf in a child root using the same scan
    /// budget and cancellation state as the enclosing owner-coverage walk.
    /// This proves the child's physical leaf set is complete; a declared
    /// `leaf_count` alone is only a limit hint and is not a closure proof.
    pub fn visit_entries<E>(
        &self,
        root: &StateRootV1,
        mut visit: impl FnMut(&StateEntryV1) -> Result<(), E>,
    ) -> Result<u64, StateScanError<E>> {
        let _workspace = self
            .memory
            .reserve(128 * 1024, self.class)
            .map_err(StateScanError::Memory)?;
        if let Some(stop) = self.stop.get() {
            return Err(match stop {
                Stop::Limited => StateScanError::ResourceLimited,
                Stop::Cancelled => StateScanError::Cancelled,
            });
        }
        if self.source.allocation_class() != Some(self.class)
            || !self
                .source
                .allocation_budget()
                .is_some_and(|pool| pool.same_pool(self.memory))
        {
            return Err(StateScanError::Invalid("state scan source pool/class"));
        }
        root.validate().map_err(StateScanError::Tree)?;
        if root.leaf_count > self.leaves.get() {
            self.stop.set(Some(Stop::Limited));
            return Err(StateScanError::ResourceLimited);
        }
        if (self.cancelled.borrow_mut())() {
            self.stop.set(Some(Stop::Cancelled));
            return Err(StateScanError::Cancelled);
        }
        let mut view =
            Updater::<StateCodec, _>::new(self, empty_hashes().map_err(StateScanError::Tree)?, 0);
        let result = view.visit_leaves(&root.node_root, |entry| {
            if let Some(stop) = self.stop.get() {
                return Err(match stop {
                    Stop::Limited => StateScanError::ResourceLimited,
                    Stop::Cancelled => StateScanError::Cancelled,
                });
            }
            if (self.cancelled.borrow_mut())() {
                self.stop.set(Some(Stop::Cancelled));
                return Err(StateScanError::Cancelled);
            }
            if entry.namespace != root.namespace {
                return Err(StateScanError::Invalid("state scan namespace"));
            }
            if self.leaves.get() == 0 {
                self.stop.set(Some(Stop::Limited));
                return Err(StateScanError::ResourceLimited);
            }
            self.leaves.set(self.leaves.get() - 1);
            visit(entry).map_err(StateScanError::Visitor)
        });
        if let Some(stop) = self.stop.get() {
            return Err(match stop {
                Stop::Limited => StateScanError::ResourceLimited,
                Stop::Cancelled => StateScanError::Cancelled,
            });
        }
        let count = result.map_err(|error| match error {
            crate::merkle::VisitError::Tree(error) => StateScanError::Tree(error),
            crate::merkle::VisitError::Visitor(error) => error,
        })?;
        if count != root.leaf_count {
            return Err(StateScanError::Invalid("state scan leaf count"));
        }
        if (self.cancelled.borrow_mut())() {
            self.stop.set(Some(Stop::Cancelled));
            return Err(StateScanError::Cancelled);
        }
        Ok(count)
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
        self.visit_entries_with_reader(root, |entry, _reader| visit(entry), cancelled)
    }

    /// As [`Self::visit_entries`], while giving the visitor a guarded reader
    /// for related point lookups. Every read through `reader` spends the same
    /// node and byte allowances and observes the same cancellation callback
    /// as the scan. Only entries yielded by the outer ordered walk consume
    /// `leaves_remaining`.
    pub fn visit_entries_with_reader<E, C: FnMut() -> bool>(
        &mut self,
        root: &StateRootV1,
        mut visit: impl FnMut(&StateEntryV1, &StateScanReader<'_, R, C>) -> Result<(), E>,
        cancelled: C,
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
        let source = StateScanReader {
            source: self.source,
            memory: self.memory,
            class: self.class,
            nodes: Cell::from_mut(&mut self.io.nodes_remaining),
            bytes: Cell::from_mut(&mut self.io.read_bytes_remaining),
            leaves: Cell::from_mut(&mut self.io.leaves_remaining),
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
            if source.leaves.get() == 0 {
                return Err(StateScanError::ResourceLimited);
            }
            source.leaves.set(source.leaves.get() - 1);
            visit(entry, &source).map_err(StateScanError::Visitor)
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
