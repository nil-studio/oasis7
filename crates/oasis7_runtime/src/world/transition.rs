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

// ============================================================================
// StagedTransition Root Contract (Stage 1 Prepare -> Stage 2 Gate -> Stage 3 Install)
// ============================================================================

use super::prepared_base_head::WorldPreparedBaseHead;
use super::{World, WorldError};
use crate::{CausedBy, TickConsensusRecord, WorldEvent, WorldEventId, WorldState, WorldTime};

/// Read-only borrowed view over the canonical [`World`] and its base head during Stage 1 preparation.
///
/// Nested domain operations and rule-settlement policies inspect canonical state through this view
/// and stage typed [`SubsystemDelta`]s into a [`StagedRootTransaction`] without cloning [`World`]
/// or committing independently.
pub struct BorrowedWorldView<'a> {
    world: &'a World,
    base_head: WorldPreparedBaseHead,
}

impl<'a> BorrowedWorldView<'a> {
    pub fn new(world: &'a World) -> Result<Self, WorldError> {
        let base_head = WorldPreparedBaseHead::capture(world)?;
        Ok(Self { world, base_head })
    }

    pub(crate) fn from_captured(world: &'a World, base_head: WorldPreparedBaseHead) -> Self {
        Self { world, base_head }
    }

    pub fn world(&self) -> &'a World {
        self.world
    }

    pub fn state(&self) -> &'a WorldState {
        &self.world.state
    }

    pub fn time(&self) -> WorldTime {
        self.world.state.time
    }

    pub(crate) fn base_head(&self) -> &WorldPreparedBaseHead {
        &self.base_head
    }
}

/// Execution context passed into [`StagedTransition::prepare`] during Stage 1.
#[derive(Debug, Clone, Default)]
pub struct TransitionContext {
    pub tick: WorldTime,
    pub caused_by: Option<CausedBy>,
}

impl TransitionContext {
    pub fn new(tick: WorldTime, caused_by: Option<CausedBy>) -> Self {
        Self { tick, caused_by }
    }
}

/// Stage 2 durable commit confirmation outcome before Stage 3 canonical installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DurableCommitConfirmation {
    /// Post-prepare gate and durable persistence succeeded; Stage 3 installation is authorized.
    Confirmed,
    /// Post-prepare gate or durable commit status is indeterminate (`StatusUnknown`);
    /// canonical memory MUST NOT be mutated and the caller must enter Fenced/Reconcile recovery.
    StatusUnknown { reason: String },
}

/// Rejection raised during Stage 1 preparation or Stage 2 commit confirmation.
#[derive(Debug)]
pub enum TransitionReject {
    World(WorldError),
    BaseHeadStale,
    StatusUnknownFenced { reason: String },
    InvalidSavepoint,
}

impl From<WorldError> for TransitionReject {
    fn from(error: WorldError) -> Self {
        Self::World(error)
    }
}

impl TransitionReject {
    pub fn into_world_error(self) -> WorldError {
        match self {
            Self::World(err) => err,
            Self::BaseHeadStale => WorldPreparedBaseHead::stale_error(),
            Self::StatusUnknownFenced { reason } => WorldError::DistributedValidationFailed {
                reason: format!("staged transition fenced on StatusUnknown: {reason}"),
            },
            Self::InvalidSavepoint => WorldError::ResourceBalanceInvalid {
                reason: "invalid staged transition savepoint".to_string(),
            },
        }
    }
}

/// Restricted root installation context used exclusively during Stage 3 (`apply_infallible`).
///
/// Only [`PreparedStagedCommit::install_confirmed`] can construct this context after
/// verifying the captured [`WorldPreparedBaseHead`] and [`DurableCommitConfirmation::Confirmed`].
pub struct RootInstallContext<'a> {
    world: &'a mut World,
}

impl<'a> RootInstallContext<'a> {
    pub(crate) fn new(world: &'a mut World) -> Self {
        Self { world }
    }

    pub fn world(&self) -> &World {
        self.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        self.world
    }

    pub fn state_mut(&mut self) -> &mut WorldState {
        &mut self.world.state
    }

    pub fn install_journal_tail(
        &mut self,
        next_event_id: WorldEventId,
        next_event_id_era: u64,
        journal_events: Vec<WorldEvent>,
        journal_events_evicted: u64,
    ) {
        self.world.next_event_id = next_event_id;
        self.world.next_event_id_era = next_event_id_era;
        self.world.journal.events = journal_events;
        self.world.runtime_backpressure_stats.journal_events_evicted = self
            .world
            .runtime_backpressure_stats
            .journal_events_evicted
            .saturating_add(journal_events_evicted);
    }
}

/// A typed subsystem or domain delta staged inside a root transaction.
///
/// Implementations perform all fallible validation before producing the delta;
/// [`SubsystemDelta::apply_infallible`] is invoked only in Stage 3 after confirmed commit.
pub trait SubsystemDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>);
}

impl<A: SubsystemDelta, B: SubsystemDelta> SubsystemDelta for (A, B) {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        self.0.apply_infallible(ctx);
        self.1.apply_infallible(ctx);
    }
}

impl<D: SubsystemDelta> SubsystemDelta for Vec<D> {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        for delta in self {
            delta.apply_infallible(ctx);
        }
    }
}

impl<D: SubsystemDelta> SubsystemDelta for Option<D> {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        if let Some(delta) = self {
            delta.apply_infallible(ctx);
        }
    }
}

/// Unified two-phase / three-stage atomic transition contract (`Stage 1 Prepare -> Stage 2 Gate -> Stage 3 Install`).
pub trait StagedTransition {
    type Delta: SubsystemDelta;

    fn prepare(
        &self,
        view: &BorrowedWorldView<'_>,
        ctx: &TransitionContext,
    ) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject>;
}

/// Savepoint for nested domain/fee staging within a single [`StagedRootTransaction`].
#[derive(Debug, Clone)]
pub struct StagedSubtransactionSavepoint {
    transaction_marker: Arc<()>,
    staged_len: usize,
}

/// Root staging transaction that enforces "nested operations only stage deltas and cannot commit independently".
pub struct StagedRootTransaction<'a, D> {
    view: BorrowedWorldView<'a>,
    ctx: TransitionContext,
    deltas: Vec<D>,
    consensus_record: Option<TickConsensusRecord>,
    transaction_marker: Arc<()>,
}

impl<'a, D: SubsystemDelta> StagedRootTransaction<'a, D> {
    pub fn begin(world: &'a World, ctx: TransitionContext) -> Result<Self, WorldError> {
        let view = BorrowedWorldView::new(world)?;
        Ok(Self {
            view,
            ctx,
            deltas: Vec::new(),
            consensus_record: None,
            transaction_marker: Arc::new(()),
        })
    }

    pub fn view(&self) -> &BorrowedWorldView<'a> {
        &self.view
    }

    pub fn context(&self) -> &TransitionContext {
        &self.ctx
    }

    pub fn staged_count(&self) -> usize {
        self.deltas.len()
    }

    /// Stage a nested domain or fee delta without committing it independently.
    pub fn stage_delta(&mut self, delta: D) {
        self.deltas.push(delta);
    }

    /// Stage a nested fallible operation atomically: if `op` fails, all deltas staged inside `op`
    /// are rolled back to the pre-call savepoint while earlier staged deltas remain intact.
    pub fn stage_nested<R>(
        &mut self,
        op: impl FnOnce(&mut Self) -> Result<R, TransitionReject>,
    ) -> Result<R, TransitionReject> {
        let sp = self.savepoint();
        match op(self) {
            Ok(output) => Ok(output),
            Err(err) => {
                let _ = self.rollback_to(sp);
                Err(err)
            }
        }
    }

    pub fn set_consensus_record(&mut self, record: TickConsensusRecord) {
        self.consensus_record = Some(record);
    }

    pub fn savepoint(&self) -> StagedSubtransactionSavepoint {
        StagedSubtransactionSavepoint {
            transaction_marker: Arc::clone(&self.transaction_marker),
            staged_len: self.deltas.len(),
        }
    }

    pub fn rollback_to(
        &mut self,
        savepoint: StagedSubtransactionSavepoint,
    ) -> Result<(), TransitionReject> {
        if !Arc::ptr_eq(&self.transaction_marker, &savepoint.transaction_marker)
            || savepoint.staged_len > self.deltas.len()
        {
            return Err(TransitionReject::InvalidSavepoint);
        }
        self.deltas.truncate(savepoint.staged_len);
        Ok(())
    }

    pub fn finish_prepare(self) -> PreparedStagedCommit<Vec<D>> {
        PreparedStagedCommit {
            expected_base_head: self.view.base_head,
            consensus_record: self.consensus_record,
            delta: self.deltas,
        }
    }
}

/// Root prepared commit produced by Stage 1 (`prepare`) and consumed in Stage 3 (`install_confirmed`).
#[derive(Debug, Clone)]
pub struct PreparedStagedCommit<D> {
    expected_base_head: WorldPreparedBaseHead,
    consensus_record: Option<TickConsensusRecord>,
    delta: D,
}

impl<D: SubsystemDelta> PreparedStagedCommit<D> {
    pub(crate) fn new(
        expected_base_head: WorldPreparedBaseHead,
        consensus_record: Option<TickConsensusRecord>,
        delta: D,
    ) -> Self {
        Self {
            expected_base_head,
            consensus_record,
            delta,
        }
    }

    pub fn delta(&self) -> &D {
        &self.delta
    }

    pub fn consensus_record(&self) -> Option<&TickConsensusRecord> {
        self.consensus_record.as_ref()
    }

    /// Stage 2 -> Stage 3 boundary: verifies durable commit confirmation, checks that the canonical
    /// [`WorldPreparedBaseHead`] has not drifted since Stage 1, installs the prepared consensus
    /// record (if any), and then executes [`SubsystemDelta::apply_infallible`] via [`RootInstallContext`].
    pub fn install_confirmed(
        self,
        world: &mut World,
        confirmation: DurableCommitConfirmation,
    ) -> Result<(), TransitionReject> {
        self.install_confirmed_inner(world, confirmation, true)
    }

    /// Same as [`Self::install_confirmed`], used when the caller prepared the commit against
    /// `&mut World` immediately before Stage 2 failpoint check in the same synchronous call frame.
    pub(crate) fn install_confirmed_unchecked_head(
        self,
        world: &mut World,
        confirmation: DurableCommitConfirmation,
    ) -> Result<(), TransitionReject> {
        self.install_confirmed_inner(world, confirmation, false)
    }

    fn install_confirmed_inner(
        self,
        world: &mut World,
        confirmation: DurableCommitConfirmation,
        verify_base_head: bool,
    ) -> Result<(), TransitionReject> {
        match confirmation {
            DurableCommitConfirmation::Confirmed => {}
            DurableCommitConfirmation::StatusUnknown { reason } => {
                return Err(TransitionReject::StatusUnknownFenced { reason });
            }
        }
        if verify_base_head {
            let actual_head = WorldPreparedBaseHead::capture(world)?;
            if actual_head != self.expected_base_head {
                return Err(TransitionReject::BaseHeadStale);
            }
        }
        if let Some(record) = self.consensus_record {
            world.install_prepared_tick_consensus_record(record)?;
        }
        let mut ctx = RootInstallContext::new(world);
        self.delta.apply_infallible(&mut ctx);
        Ok(())
    }
}
