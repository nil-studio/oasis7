//! The small, typed transition kernel used to stage runtime mutations.
//!
//! This module intentionally models only the Phase 0 kernel state.  It is an
//! overlay over an immutable base state, rather than a cloned production
//! world.  Larger runtime mutation surfaces can add typed deltas to the same
//! transaction boundary as they are migrated.

use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The state used by the Phase 0 transition-kernel contract.
///
/// The fields are public so the isolated kernel can be used by runtime tests
/// and by later migration slices without introducing a second state wrapper.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransitionKernelState {
    pub scalar: i64,
    pub entries: BTreeMap<String, i64>,
}

/// The canonical head against which a prepared transition is published.
///
/// The initial kernel binds only the state root.  The production world head
/// will extend this typed value with journal, sequence, queue, consensus, and
/// authorization roots as those surfaces migrate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionBaseHead {
    pub state_root: String,
}

impl TransitionBaseHead {
    pub fn from_state_root(state_root: impl Into<String>) -> Self {
        Self {
            state_root: state_root.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MapMutation {
    Set(i64),
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum UndoMutation {
    Scalar(Option<i64>),
    Entry {
        key: String,
        previous: Option<MapMutation>,
    },
}

/// A typed overlay for the kernel state.
///
/// `base` is borrowed for the lifetime of the transaction and is never
/// modified by this type.  The overlay stores only changed scalar/map values;
/// readers use [`TransitionKernelView`] to resolve staged values over base.
#[derive(Debug)]
pub struct TransitionBuffer<'a> {
    base: &'a TransitionKernelState,
    scalar: Option<i64>,
    entries: BTreeMap<String, MapMutation>,
    undo: Vec<UndoMutation>,
}

impl<'a> TransitionBuffer<'a> {
    fn new(base: &'a TransitionKernelState) -> Self {
        Self {
            base,
            scalar: None,
            entries: BTreeMap::new(),
            undo: Vec::new(),
        }
    }

    /// Stage a scalar replacement without modifying the base state.
    pub fn set_scalar(&mut self, value: i64) {
        self.undo.push(UndoMutation::Scalar(self.scalar));
        self.scalar = Some(value);
    }

    /// Stage an entry insertion/replacement without modifying the base state.
    pub fn map_insert(&mut self, key: impl Into<String>, value: i64) {
        let key = key.into();
        let previous = self.entries.get(&key).copied();
        self.undo.push(UndoMutation::Entry {
            key: key.clone(),
            previous,
        });
        self.entries.insert(key, MapMutation::Set(value));
    }

    /// Alias for callers that describe map writes as sets.
    pub fn map_set(&mut self, key: impl Into<String>, value: i64) {
        self.map_insert(key, value);
    }

    /// Stage an entry removal without modifying the base state.
    pub fn map_remove(&mut self, key: impl Into<String>) {
        let key = key.into();
        let previous = self.entries.get(&key).copied();
        self.undo.push(UndoMutation::Entry {
            key: key.clone(),
            previous,
        });
        self.entries.insert(key, MapMutation::Remove);
    }

    /// Resolve the staged state over the immutable base state.
    pub fn view(&self) -> TransitionKernelView<'_> {
        TransitionKernelView {
            scalar: self.scalar.unwrap_or(self.base.scalar),
            entries: TransitionKernelEntriesView {
                base: &self.base.entries,
                delta: &self.entries,
            },
        }
    }

    fn rollback_to(&mut self, undo_len: usize) {
        while self.undo.len() > undo_len {
            let mutation = self
                .undo
                .pop()
                .expect("undo length is checked before popping");
            match mutation {
                UndoMutation::Scalar(previous) => self.scalar = previous,
                UndoMutation::Entry { key, previous } => match previous {
                    Some(previous) => {
                        self.entries.insert(key, previous);
                    }
                    None => {
                        self.entries.remove(&key);
                    }
                },
            }
        }
    }
}

/// A read-only view which resolves typed staged values over canonical state.
#[derive(Debug, Clone, Copy)]
pub struct TransitionKernelView<'a> {
    pub scalar: i64,
    pub entries: TransitionKernelEntriesView<'a>,
}

/// A read-only map view over canonical entries plus typed map mutations.
#[derive(Debug, Clone, Copy)]
pub struct TransitionKernelEntriesView<'a> {
    base: &'a BTreeMap<String, i64>,
    delta: &'a BTreeMap<String, MapMutation>,
}

impl<'a> TransitionKernelEntriesView<'a> {
    pub fn get<Q>(&self, key: &Q) -> Option<&'a i64>
    where
        String: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        if let Some(mutation) = self.delta.get(key) {
            return match mutation {
                MapMutation::Set(value) => Some(value),
                MapMutation::Remove => None,
            };
        }
        self.base.get(key)
    }

    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        String: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.get(key).is_some()
    }

    pub fn len(&self) -> usize {
        let mut len = self.base.len();
        for (key, mutation) in self.delta {
            match (self.base.contains_key(key), mutation) {
                (false, MapMutation::Set(_)) => len += 1,
                (true, MapMutation::Remove) => len -= 1,
                _ => {}
            }
        }
        len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A savepoint for a nested operation in one root transaction.
#[derive(Debug, Clone)]
pub struct TransitionSavepoint {
    transaction_marker: Arc<()>,
    undo_len: usize,
}

/// Errors raised while rolling a nested operation back to a savepoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionRollbackError {
    SavepointFromDifferentTransaction,
    InvalidSavepoint,
}

/// A root execution transaction over the typed kernel state.
#[derive(Debug)]
pub struct ExecutionTransaction<'a> {
    expected_head: TransitionBaseHead,
    buffer: TransitionBuffer<'a>,
    transaction_marker: Arc<()>,
}

impl<'a> ExecutionTransaction<'a> {
    pub fn begin(base: &'a TransitionKernelState, expected_head: TransitionBaseHead) -> Self {
        Self {
            expected_head,
            buffer: TransitionBuffer::new(base),
            transaction_marker: Arc::new(()),
        }
    }

    pub fn buffer(&self) -> &TransitionBuffer<'a> {
        &self.buffer
    }

    pub fn buffer_mut(&mut self) -> &mut TransitionBuffer<'a> {
        &mut self.buffer
    }

    pub fn view(&self) -> TransitionKernelView<'_> {
        self.buffer.view()
    }

    pub fn savepoint(&self) -> TransitionSavepoint {
        TransitionSavepoint {
            transaction_marker: Arc::clone(&self.transaction_marker),
            undo_len: self.buffer.undo.len(),
        }
    }

    pub fn rollback_to(
        &mut self,
        savepoint: TransitionSavepoint,
    ) -> Result<(), TransitionRollbackError> {
        if !Arc::ptr_eq(&self.transaction_marker, &savepoint.transaction_marker) {
            return Err(TransitionRollbackError::SavepointFromDifferentTransaction);
        }
        if savepoint.undo_len > self.buffer.undo.len() {
            return Err(TransitionRollbackError::InvalidSavepoint);
        }
        self.buffer.rollback_to(savepoint.undo_len);
        Ok(())
    }

    /// Discard the overlay.  The borrowed canonical state is untouched.
    pub fn abort(self) {}

    /// Freeze the typed overlay into a prepared, publishable delta.
    pub fn prepare(self) -> Result<PreparedCommit, TransitionPrepareError> {
        let Self {
            expected_head,
            buffer: TransitionBuffer {
                scalar, entries, ..
            },
            ..
        } = self;
        Ok(PreparedCommit {
            expected_head,
            scalar,
            entries,
        })
    }
}

/// Preparation currently has no runtime failure path; the error type keeps
/// the API explicit so validation/capacity checks can be added before commit
/// without changing callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionPrepareError;

/// A fully prepared typed delta.  Publication is the only operation allowed
/// to mutate canonical state, and the base-head check happens before any write.
#[derive(Debug)]
pub struct PreparedCommit {
    expected_head: TransitionBaseHead,
    scalar: Option<i64>,
    entries: BTreeMap<String, MapMutation>,
}

/// Errors raised when publishing a prepared transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionCommitError {
    BaseHeadMismatch {
        expected: TransitionBaseHead,
        actual: TransitionBaseHead,
    },
}

impl TransitionCommitError {
    pub fn is_base_head_mismatch(&self) -> bool {
        matches!(self, Self::BaseHeadMismatch { .. })
    }
}

impl PreparedCommit {
    /// Publish the prepared delta if the canonical head is unchanged.
    ///
    /// The mismatch check is deliberately before all canonical writes.  Once
    /// it succeeds, this kernel's typed installation contains no fallible
    /// business validation and can be treated as the commit seam.
    pub fn commit_into(
        self,
        canonical: &mut TransitionKernelState,
        actual_head: &TransitionBaseHead,
    ) -> Result<(), TransitionCommitError> {
        if &self.expected_head != actual_head {
            return Err(TransitionCommitError::BaseHeadMismatch {
                expected: self.expected_head.clone(),
                actual: actual_head.clone(),
            });
        }

        if let Some(scalar) = self.scalar {
            canonical.scalar = scalar;
        }
        for (key, mutation) in &self.entries {
            match mutation {
                MapMutation::Set(value) => {
                    canonical.entries.insert(key.clone(), *value);
                }
                MapMutation::Remove => {
                    canonical.entries.remove(key);
                }
            }
        }
        Ok(())
    }
}
