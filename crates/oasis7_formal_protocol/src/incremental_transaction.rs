use super::*;
use std::collections::BTreeMap;
use std::sync::Arc;

const BASE_WORKSPACE: usize = 64 * 1024;
const GUARD_BYTES: usize = 64;

#[derive(Debug, Clone, Copy)]
pub struct TransactionLimits {
    pub max_reads: usize,
    pub max_writes: usize,
    pub max_facts: usize,
    pub max_pending_node_bytes: usize,
}

/// Failure while sealing one final, after-root-bound fact. Producer failure
/// retains its original type; no prepared transition escapes on any error.
#[derive(Debug)]
pub enum SealWithFinalFactError<E> {
    Execution(ExecutionError),
    Producer(E),
    Cancelled,
}
impl<E> From<ExecutionError> for SealWithFinalFactError<E> {
    fn from(error: ExecutionError) -> Self {
        Self::Execution(error)
    }
}

struct Entry {
    before: Option<[u8; 32]>,
    after: Option<[u8; 32]>,
    _charge: MemoryReservation,
}
struct Fact {
    value: ExecutionFactRefV1,
    _charge: MemoryReservation,
}

/// An opaque capability for one precise in-process transaction revision.
/// Its charged identity survives the transaction while any guard is retained.
#[derive(Debug)]
pub struct PreparedGuard {
    owner: Arc<ReservedValue<()>>,
    revision: u64,
    fence: ExecutionFence,
}

/// All nested execution entrypoints borrow this same overlay. It has no Clone
/// implementation and never owns a World, full state map or committed history.
pub struct ExecutionTransaction<'a, R> {
    parent: ExecutionParent,
    operation: [u8; 32],
    fence: ExecutionFence,
    source: &'a R,
    memory: &'a MemoryBudget,
    limits: TransactionLimits,
    entries: [BTreeMap<Vec<u8>, Entry>; 6],
    facts: BTreeMap<usize, Fact>,
    reads: usize,
    dirty: usize,
    revision: u64,
    owner: Arc<ReservedValue<()>>,
    aborted: bool,
    _workspace: MemoryReservation,
}

impl<'a, R: MerkleNodeReader> ExecutionTransaction<'a, R> {
    pub fn begin(
        parent: ExecutionParent,
        operation: [u8; 32],
        source: &'a R,
        memory: &'a MemoryBudget,
        limits: TransactionLimits,
    ) -> Result<Self, ExecutionError> {
        validate_source_budget(source, memory, MemoryClass::Ordinary)?;
        if limits.max_reads > EXECUTION_MAX_READS
            || limits.max_writes > EXECUTION_MAX_WRITES
            || limits.max_facts > EXECUTION_MAX_FACTS
        {
            return Err(ExecutionError::Invalid("transaction profile limits"));
        }
        let workspace = memory.reserve(BASE_WORKSPACE, MemoryClass::Ordinary)?;
        let identity = memory.reserve(GUARD_BYTES, MemoryClass::Ordinary)?;
        let fence = parent.fence()?;
        Ok(Self {
            parent,
            operation,
            fence,
            source,
            memory,
            limits,
            entries: std::array::from_fn(|_| BTreeMap::new()),
            facts: BTreeMap::new(),
            reads: 0,
            dirty: 0,
            revision: 0,
            owner: Arc::new(identity.retain(())),
            aborted: false,
            _workspace: workspace,
        })
    }

    pub fn guard(&self) -> PreparedGuard {
        PreparedGuard {
            owner: self.owner.clone(),
            revision: self.revision,
            fence: self.fence,
        }
    }

    /// Immutable original namespace root, never the current overlay.
    pub fn parent_state_root(&self, namespace: StateNamespaceV1) -> &StateRootV1 {
        self.parent.roots.state_root(namespace)
    }

    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.memory.same_pool(memory)
    }

    /// Fixed at begin; callers recheck the original authority's live fence
    /// without serializing state or recomputing even this bounded RootSet.
    pub fn parent_fence(&self) -> ExecutionFence {
        self.fence
    }

    /// Contribute a nested reducer/codec/storage operation to this overlay.
    /// Any error (including a caught unwind or a swallowed inner transaction
    /// error) permanently poisons the complete transaction. Durable provisional
    /// pages may remain; they never make a failed partial debit publishable.
    pub fn contribute<T, E: From<ExecutionError>>(
        &mut self,
        action: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E> {
        if self.aborted {
            return Err(ExecutionError::Aborted.into());
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| action(self)));
        match result {
            Ok(Ok(value)) if !self.aborted => Ok(value),
            Ok(Ok(_)) => Err(ExecutionError::Aborted.into()),
            Ok(Err(error)) => {
                self.aborted = true;
                Err(error)
            }
            Err(payload) => {
                self.aborted = true;
                std::panic::resume_unwind(payload)
            }
        }
    }

    /// Bind a retained overlay token to this in-process transaction across
    /// read/write revisions. Mutation and sealing still require a fresh guard
    /// and the original authority's live fence through `validate_guard`.
    pub fn validate_transaction_identity(
        &self,
        guard: &PreparedGuard,
    ) -> Result<(), ExecutionError> {
        if self.aborted {
            return Err(ExecutionError::Aborted);
        }
        if !Arc::ptr_eq(&guard.owner, &self.owner) || guard.fence != self.fence {
            return Err(ExecutionError::StaleGuard);
        }
        Ok(())
    }

    /// Constant-size comparison: no state serialization, history scan or hash.
    pub fn validate_guard(
        &self,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<(), ExecutionError> {
        if self.aborted {
            return Err(ExecutionError::Aborted);
        }
        if self.fence != *live {
            return Err(ExecutionError::ParentConflict);
        }
        if !Arc::ptr_eq(&guard.owner, &self.owner)
            || guard.revision != self.revision
            || guard.fence != self.fence
        {
            return Err(ExecutionError::StaleGuard);
        }
        Ok(())
    }

    pub fn read(
        &mut self,
        namespace: StateNamespaceV1,
        key: &[u8],
    ) -> Result<Option<[u8; 32]>, ExecutionError> {
        self.checked(|txn| txn.read_inner(namespace, key))
    }

    fn checked<T>(
        &mut self,
        action: impl FnOnce(&mut Self) -> Result<T, ExecutionError>,
    ) -> Result<T, ExecutionError> {
        if self.aborted {
            return Err(ExecutionError::Aborted);
        }
        // Keep the abort state on both Result errors and caught unwinds.
        self.aborted = true;
        let result = action(self);
        if result.is_ok() {
            self.aborted = false;
        }
        result
    }

    fn read_inner(
        &mut self,
        namespace: StateNamespaceV1,
        key: &[u8],
    ) -> Result<Option<[u8; 32]>, ExecutionError> {
        validate_key(key)?;
        let index = namespace_index(namespace);
        if let Some(entry) = self.entries[index].get(key) {
            return Ok(entry.after);
        }
        if self.reads == self.limits.max_reads {
            return Err(ExecutionError::ResourceLimited("read-set entries"));
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(ExecutionError::ResourceLimited("transaction revision"))?;
        let charge = self
            .memory
            .reserve(key.len() + 512, MemoryClass::Ordinary)?;
        let before = self
            .parent
            .roots
            .state_root(namespace)
            .get(key, self.source)
            .map_err(ExecutionError::Tree)?;
        self.entries[index].insert(
            key.to_vec(),
            Entry {
                before,
                after: before,
                _charge: charge,
            },
        );
        self.reads += 1;
        self.revision = revision;
        Ok(before)
    }

    pub fn write(
        &mut self,
        namespace: StateNamespaceV1,
        key: &[u8],
        value: Option<[u8; 32]>,
    ) -> Result<(), ExecutionError> {
        self.checked(|txn| txn.write_inner(namespace, key, value))
    }

    fn write_inner(
        &mut self,
        namespace: StateNamespaceV1,
        key: &[u8],
        value: Option<[u8; 32]>,
    ) -> Result<(), ExecutionError> {
        self.read_inner(namespace, key)?;
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(ExecutionError::ResourceLimited("transaction revision"))?;
        let entry = self.entries[namespace_index(namespace)]
            .get_mut(key)
            .ok_or(ExecutionError::Invalid("read-set lost entry"))?;
        let was_dirty = entry.before != entry.after;
        let will_dirty = entry.before != value;
        if !was_dirty && will_dirty && self.dirty == self.limits.max_writes {
            return Err(ExecutionError::ResourceLimited("write-set entries"));
        }
        if was_dirty && !will_dirty {
            self.dirty -= 1;
        }
        if !was_dirty && will_dirty {
            self.dirty += 1;
        }
        entry.after = value;
        self.revision = revision;
        Ok(())
    }

    /// Current ordered position only, not proof of an accepted occurrence.
    pub fn ordered_fact_count(&self) -> usize {
        self.facts.len()
    }

    /// Visit the final writes already admitted to this overlay, borrowing keys
    /// without allocating or reading the parent tree. Every inspected entry,
    /// including unchanged reads, consumes work. This does not seal the overlay
    /// or replace the caller's live-fence check before publication.
    pub fn visit_pending_writes<E: From<ExecutionError>>(
        &self,
        namespace: StateNamespaceV1,
        remaining_entries: &mut usize,
        mut check_cancelled: impl FnMut() -> Result<(), E>,
        mut visit: impl FnMut(&[u8], Option<[u8; 32]>, Option<[u8; 32]>) -> Result<(), E>,
    ) -> Result<(), E> {
        if self.aborted {
            return Err(ExecutionError::Aborted.into());
        }
        check_cancelled()?;
        for (key, entry) in &self.entries[namespace_index(namespace)] {
            *remaining_entries = remaining_entries
                .checked_sub(1)
                .ok_or(ExecutionError::ResourceLimited("pending write inspection"))?;
            check_cancelled()?;
            if entry.before != entry.after {
                visit(key, entry.before, entry.after)?;
            }
        }
        check_cancelled()
    }

    pub fn append_fact(&mut self, value: ExecutionFactRefV1) -> Result<(), ExecutionError> {
        self.checked(|txn| txn.append_fact_inner(value))
    }

    fn append_fact_inner(&mut self, value: ExecutionFactRefV1) -> Result<(), ExecutionError> {
        if self.facts.len() == self.limits.max_facts {
            return Err(ExecutionError::ResourceLimited("ordered facts"));
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(ExecutionError::ResourceLimited("transaction revision"))?;
        let charge = self.memory.reserve(512, MemoryClass::Ordinary)?;
        self.facts.insert(
            self.facts.len(),
            Fact {
                value,
                _charge: charge,
            },
        );
        self.revision = revision;
        Ok(())
    }

    /// Freeze the complete final delta. Failure discards only this overlay;
    /// independently durable external-effect obligations are owned elsewhere.
    pub fn seal(
        self,
        guard: &PreparedGuard,
        live: &ExecutionFence,
        next_tick: u64,
    ) -> Result<ReservedValue<PreparedTransition>, ExecutionError> {
        let result = self.seal_inner(
            guard,
            live,
            next_tick,
            &mut || false,
            None::<fn(&RootSetV1) -> Result<[ExecutionFactRefV1; 0], std::convert::Infallible>>,
        );
        match result {
            Ok(value) => Ok(value),
            Err(SealWithFinalFactError::Execution(error)) => Err(error),
            Err(SealWithFinalFactError::Producer(never)) => match never {},
            // The legacy wrapper's cancellation predicate is always false.
            Err(SealWithFinalFactError::Cancelled) => unreachable!(),
        }
    }

    /// Freeze writes first, then produce exactly one final fact from the actual
    /// after RootSet. The callback cannot modify roots, writes or the consumed
    /// transaction. Its physical output stays provisional until full acceptance.
    /// Capacity and workspace for the additional fact are admitted before it runs.
    pub fn seal_with_final_fact<E>(
        self,
        guard: &PreparedGuard,
        live: &ExecutionFence,
        next_tick: u64,
        mut cancelled: impl FnMut() -> bool,
        produce: impl FnOnce(&RootSetV1) -> Result<ExecutionFactRefV1, E>,
    ) -> Result<ReservedValue<PreparedTransition>, SealWithFinalFactError<E>> {
        self.seal_inner(
            guard,
            live,
            next_tick,
            &mut cancelled,
            Some(|roots: &RootSetV1| produce(roots).map(|fact| [fact])),
        )
    }

    /// Produce the complete profile's two Result facts only after state writes
    /// are frozen. The producer cannot mutate the consumed transaction. Both
    /// slots and their workspace are admitted before any producer I/O; no
    /// transition escapes if either result or the final cancellation fails.
    pub fn seal_with_final_results<E>(
        self,
        guard: &PreparedGuard,
        live: &ExecutionFence,
        next_tick: u64,
        mut cancelled: impl FnMut() -> bool,
        produce: impl FnOnce(&RootSetV1) -> Result<[ExecutionFactRefV1; 2], E>,
    ) -> Result<ReservedValue<PreparedTransition>, SealWithFinalFactError<E>> {
        self.seal_inner(guard, live, next_tick, &mut cancelled, Some(produce))
    }

    fn seal_inner<
        E,
        const N: usize,
        F: FnOnce(&RootSetV1) -> Result<[ExecutionFactRefV1; N], E>,
    >(
        self,
        guard: &PreparedGuard,
        live: &ExecutionFence,
        next_tick: u64,
        cancelled: &mut impl FnMut() -> bool,
        produce: Option<F>,
    ) -> Result<ReservedValue<PreparedTransition>, SealWithFinalFactError<E>> {
        self.validate_guard(guard, live)?;
        if cancelled() {
            return Err(SealWithFinalFactError::Cancelled);
        }
        let fact_count = self
            .facts
            .len()
            .checked_add(if produce.is_some() { N } else { 0 })
            .ok_or(ExecutionError::ResourceLimited("ordered facts"))?;
        if fact_count > self.limits.max_facts {
            return Err(ExecutionError::ResourceLimited("ordered facts").into());
        }
        if next_tick < self.parent.roots.simulation_tick {
            return Err(ExecutionError::Invalid("tick regression").into());
        }
        let height = self
            .parent
            .roots
            .height
            .checked_add(1)
            .ok_or(ExecutionError::Invalid("height overflow"))?;
        let mut workspace = self
            .memory
            .reserve(
                transition::phase_bound(self.dirty, self.reads, fact_count)?,
                MemoryClass::Ordinary,
            )
            .map_err(ExecutionError::Memory)?;
        let mut after = self.parent.roots.clone();
        after.height = height;
        after.simulation_tick = next_tick;
        let mut writes = Vec::with_capacity(self.dirty);
        let mut new_nodes = Vec::new();
        for namespace in NAMESPACES {
            if cancelled() {
                return Err(SealWithFinalFactError::Cancelled);
            }
            let mutations = self.entries[namespace_index(namespace)]
                .iter()
                .filter(|(_, entry)| entry.before != entry.after)
                .map(|(key, entry)| StateWriteV1 {
                    key: key.clone(),
                    before: entry.before,
                    after: entry.after,
                })
                .collect();
            let delta = StateDeltaV1::build(
                self.parent.roots.state_root(namespace).clone(),
                mutations,
                self.source,
                self.limits.max_pending_node_bytes,
            )
            .map_err(ExecutionError::Tree)?;
            after.state[namespace_index(namespace)] = delta.after;
            writes.extend(delta.writes.into_iter().map(|mutation| ExecutionWriteV1 {
                namespace,
                mutation,
            }));
            new_nodes.extend(delta.new_nodes);
        }
        let mut reads = Vec::with_capacity(self.reads);
        for (namespace, entries) in NAMESPACES.into_iter().zip(self.entries) {
            reads.extend(entries.into_iter().map(|(key, entry)| ExecutionReadV1 {
                namespace,
                key,
                before: entry.before,
            }));
        }
        let mut facts = Vec::with_capacity(fact_count);
        facts.extend(self.facts.into_values().map(|fact| fact.value));
        let mut record = TransitionRecordV1 {
            parent_commit: self.parent.commit,
            operation: self.operation,
            authority: self.parent.authority,
            before: self.parent.roots,
            after,
            reads,
            writes,
            facts,
        };
        record.validate()?;
        if cancelled() {
            return Err(SealWithFinalFactError::Cancelled);
        }
        if let Some(produce) = produce {
            let facts = produce(&record.after).map_err(SealWithFinalFactError::Producer)?;
            if cancelled() {
                return Err(SealWithFinalFactError::Cancelled);
            }
            if N == 2
                && (facts
                    .iter()
                    .any(|fact| fact.kind != ExecutionFactKindV1::Result)
                    || facts[0].id == facts[1].id)
            {
                return Err(ExecutionError::Invalid("final Result pair").into());
            }
            record.facts.extend(facts);
            record.validate()?;
        }
        new_nodes.sort_unstable_by(|a, b| a.digest.cmp(&b.digest));
        let candidate = PreparedTransition { record, new_nodes };
        let bytes = candidate.heap_bytes()?;
        let charge = workspace
            .split(bytes)
            .map_err(|_| ExecutionError::Invalid("transition exceeded workspace accounting"))?;
        if cancelled() {
            return Err(SealWithFinalFactError::Cancelled);
        }
        Ok(charge.retain(candidate))
    }
}

#[path = "incremental_seal_binding.rs"]
mod seal_binding;
pub use seal_binding::{SealedExecutionBindingV1, SealedExecutionTransitionV1};
