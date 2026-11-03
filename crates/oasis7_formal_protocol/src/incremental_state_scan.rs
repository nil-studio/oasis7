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

/// One authenticated page from a fixed-root scan. The digest is the last
/// visited sparse-Merkle path, not a semantic key. Callers that persist or
/// accept a continuation must bind it to the exact root and keep the cursor
/// opaque; this low-level result is not a completeness token by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateScanPageV1 {
    pub visited: u64,
    pub last_key_digest: Option<[u8; 32]>,
    pub has_more: bool,
}

const MAX_STATE_SCAN_PAGE_ENTRIES: usize = 4096;

enum PageVisitError<E> {
    Visitor(E),
    Full,
}

fn digest_successor(mut digest: [u8; 32]) -> Option<[u8; 32]> {
    for index in (0..digest.len()).rev() {
        if digest[index] != u8::MAX {
            digest[index] += 1;
            digest[index + 1..].fill(0);
            return Some(digest);
        }
    }
    None
}
#[derive(Clone, Copy)]
pub(crate) enum Stop {
    Limited,
    Cancelled,
}
pub(crate) struct Guarded<'a, R, C> {
    pub(crate) source: &'a R,
    pub(crate) nodes: &'a Cell<u64>,
    pub(crate) bytes: &'a Cell<u64>,
    pub(crate) cancelled: &'a RefCell<C>,
    pub(crate) stop: &'a Cell<Option<Stop>>,
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
        let result = self.source.read_node(id, admitted).inspect_err(|_| {
            // A string-only source cannot distinguish an over-bound page from
            // another read failure. With less than the requested maximum
            // admitted, treat rejection as incomplete work; retry with enough
            // allowance can still expose a genuine tree error. Explicit None
            // remains a missing-page result, never a resource-limit success.
            if admitted < max {
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

    /// Visit at most `max_entries` leaves after an authenticated sparse-Merkle
    /// path. This does not restart at the root on every page: paths below the
    /// exclusive lower bound are pruned before their pages are read. The next
    /// call must use `last_key_digest` and the same immutable root. A caller
    /// must still validate whole-root completeness before using paged output
    /// for retention or authority decisions.
    pub fn visit_entries_page<E>(
        &mut self,
        root: &StateRootV1,
        after_key_digest: Option<[u8; 32]>,
        max_entries: usize,
        mut visit: impl FnMut(&StateEntryV1) -> Result<(), E>,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<StateScanPageV1, StateScanError<E>> {
        if max_entries == 0 || max_entries > MAX_STATE_SCAN_PAGE_ENTRIES {
            return Err(StateScanError::Invalid("state scan page bound"));
        }
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
        if cancelled() {
            return Err(StateScanError::Cancelled);
        }
        let lower = match after_key_digest {
            Some(digest) => match digest_successor(digest) {
                Some(lower) => lower,
                None => {
                    if cancelled() {
                        return Err(StateScanError::Cancelled);
                    }
                    return Ok(StateScanPageV1 {
                        visited: 0,
                        last_key_digest: after_key_digest,
                        has_more: false,
                    });
                }
            },
            None => [0; 32],
        };

        let cancelled = RefCell::new(cancelled);
        let stop = Cell::new(None);
        let source = Guarded {
            source: self.source,
            nodes: Cell::from_mut(&mut self.io.nodes_remaining),
            bytes: Cell::from_mut(&mut self.io.read_bytes_remaining),
            cancelled: &cancelled,
            stop: &stop,
        };
        let mut updater = Updater::<StateCodec, _>::new(
            &source,
            empty_hashes().map_err(StateScanError::Tree)?,
            0,
        );
        let mut visited = 0u64;
        let mut last_key_digest = after_key_digest;
        let result = updater.visit_key_range(&root.node_root, &lower, &[u8::MAX; 32], |entry| {
            if (cancelled.borrow_mut())() {
                return Err(PageVisitError::Visitor(
                    StateScanPageVisitorError::Cancelled,
                ));
            }
            if visited >= max_entries as u64 {
                return Err(PageVisitError::Full);
            }
            if entry.namespace != root.namespace {
                return Err(PageVisitError::Visitor(StateScanPageVisitorError::Invalid(
                    "state scan namespace",
                )));
            }
            if self.io.leaves_remaining == 0 {
                return Err(PageVisitError::Visitor(
                    StateScanPageVisitorError::ResourceLimited,
                ));
            }
            let digest = key_digest(entry.namespace, &entry.key)
                .map_err(|error| PageVisitError::Visitor(StateScanPageVisitorError::Tree(error)))?;
            self.io.leaves_remaining -= 1;
            visit(entry).map_err(|error| {
                PageVisitError::Visitor(StateScanPageVisitorError::Visitor(error))
            })?;
            visited += 1;
            last_key_digest = Some(digest);
            Ok(())
        });
        match stop.get() {
            Some(Stop::Limited) => return Err(StateScanError::ResourceLimited),
            Some(Stop::Cancelled) => return Err(StateScanError::Cancelled),
            None => {}
        }
        let has_more = match result {
            Ok(_) => false,
            Err(crate::merkle::VisitError::Visitor(PageVisitError::Full)) => true,
            Err(crate::merkle::VisitError::Visitor(PageVisitError::Visitor(error))) => {
                return match error {
                    StateScanPageVisitorError::Visitor(error) => {
                        Err(StateScanError::Visitor(error))
                    }
                    StateScanPageVisitorError::Tree(error) => Err(StateScanError::Tree(error)),
                    StateScanPageVisitorError::Invalid(reason) => {
                        Err(StateScanError::Invalid(reason))
                    }
                    StateScanPageVisitorError::ResourceLimited => {
                        Err(StateScanError::ResourceLimited)
                    }
                    StateScanPageVisitorError::Cancelled => Err(StateScanError::Cancelled),
                };
            }
            Err(crate::merkle::VisitError::Tree(error)) => {
                return Err(StateScanError::Tree(error));
            }
        };
        if (cancelled.borrow_mut())() {
            return Err(StateScanError::Cancelled);
        }
        Ok(StateScanPageV1 {
            visited,
            last_key_digest,
            has_more,
        })
    }
}

enum StateScanPageVisitorError<E> {
    Visitor(E),
    Tree(String),
    Invalid(&'static str),
    ResourceLimited,
    Cancelled,
}
