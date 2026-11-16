use super::*;
use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

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
    owner: Arc<TransactionOwnerV1>,
    revision: u64,
    mutation_revision: Option<u64>,
    fence: ExecutionFence,
}
impl PreparedGuard {
    /// Compare two real retained cuts without copying or minting a guard.
    /// This does not establish that either cut is still Current.
    pub fn same_cut(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner)
            && self.revision == other.revision
            && self.fence == other.fence
    }
}

// Only the installed transaction can create the next DFS child. Thus every
// unclosed older owner in this private domain is an actual active ancestor.
// Completed children are closed before their parent is installed again.
#[derive(Debug)]
struct TransactionLineageV1 {
    next: AtomicU64,
    poison: AtomicBool,
    _charge: MemoryReservation,
}
#[derive(Debug)]
struct TransactionOwnerV1 {
    domain: Arc<TransactionLineageV1>,
    start: u64,
    closed_end: AtomicU64,
    _charge: MemoryReservation,
}
impl TransactionOwnerV1 {
    fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self._charge.uses_budget(memory) && self.domain._charge.uses_budget(memory)
    }
}
const fn owner_admission_bytes() -> usize {
    std::mem::size_of::<TransactionOwnerV1>() + 2 * std::mem::size_of::<usize>()
}
const fn domain_admission_bytes() -> usize {
    std::mem::size_of::<TransactionLineageV1>() + 2 * std::mem::size_of::<usize>()
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
    mutation_revision: u64,
    owner: Arc<TransactionOwnerV1>,
    aborted: bool,
    branch_ancestor: Option<Arc<TransactionOwnerV1>>,
    world_event_counter_claimed: std::sync::atomic::AtomicBool,
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
        let identity = memory.reserve(
            owner_admission_bytes().max(GUARD_BYTES),
            MemoryClass::Ordinary,
        )?;
        let domain_fee = memory.reserve(domain_admission_bytes(), MemoryClass::Ordinary)?;
        let domain = Arc::new(TransactionLineageV1 {
            next: AtomicU64::new(2),
            poison: AtomicBool::new(false),
            _charge: domain_fee,
        });
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
            mutation_revision: 0,
            owner: Arc::new(TransactionOwnerV1 {
                domain,
                start: 1,
                closed_end: AtomicU64::new(0),
                _charge: identity,
            }),
            aborted: false,
            branch_ancestor: None,
            world_event_counter_claimed: std::sync::atomic::AtomicBool::new(false),
            _workspace: workspace,
        })
    }

    /// Claim the sole provisional WorldEvent counter. This grants no fact,
    /// original, business execution or committed-state authority.
    pub fn claim_world_event_counter(&self) -> Result<(), ExecutionError> {
        if self.is_aborted() || !self.facts.is_empty() {
            return Err(ExecutionError::Invalid(
                "WorldEvent counter requires fresh transaction",
            ));
        }
        self.world_event_counter_claimed
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .map(|_| ())
            .map_err(|_| ExecutionError::Invalid("WorldEvent counter already claimed"))
    }

    pub fn guard(&self) -> PreparedGuard {
        PreparedGuard {
            owner: self.owner.clone(),
            revision: self.revision,
            mutation_revision: Some(self.mutation_revision),
            fence: self.fence,
        }
    }

    fn is_aborted(&self) -> bool {
        self.aborted || self.owner.domain.poison.load(Ordering::Acquire)
    }
    /// Actual execution mode only, never a caller-supplied authorization bit.
    pub fn is_staged_branch_v1(&self) -> Result<bool, ExecutionError> {
        if self.is_aborted() {
            Err(ExecutionError::Aborted)
        } else {
            Ok(self.branch_ancestor.is_some())
        }
    }
    fn recognizes_guard_owner(&self, guard: &PreparedGuard) -> bool {
        Arc::ptr_eq(&guard.owner, &self.owner)
            || (Arc::ptr_eq(&guard.owner.domain, &self.owner.domain)
                && guard.owner.closed_end.load(Ordering::Acquire) == 0
                && guard.owner.start < self.owner.start)
    }
    /// Exact in-process branch identity, used by external mutable participants.
    /// A parent source can be read in a child but cannot stand in for that
    /// child's isolated WorldEvent counter or other provisional participant.
    pub fn validate_exact_transaction_identity(
        &self,
        guard: &PreparedGuard,
    ) -> Result<(), ExecutionError> {
        if self.is_aborted() {
            return Err(ExecutionError::Aborted);
        }
        if !Arc::ptr_eq(&guard.owner, &self.owner) || guard.fence != self.fence {
            return Err(ExecutionError::StaleGuard);
        }
        Ok(())
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
        if self.is_aborted() {
            return Err(ExecutionError::Aborted.into());
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| action(self)));
        match result {
            Ok(Ok(value)) if !self.is_aborted() => Ok(value),
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
        if self.is_aborted() {
            return Err(ExecutionError::Aborted);
        }
        if !self.recognizes_guard_owner(guard) || guard.fence != self.fence {
            return Err(ExecutionError::StaleGuard);
        }
        Ok(())
    }

    /// Exact branch comparison: no state serialization, history scan or hash.
    pub fn validate_guard(
        &self,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<(), ExecutionError> {
        if self.is_aborted() {
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

    /// Pristine bootstrap is a true read-only transaction, not merely an empty
    /// net delta (a same-value write or a resolved/discarded branch still fails).
    /// Registered reads may advance the exact revision without mutating state.
    pub fn validate_bootstrap_read_only_v1(
        &self,
        live: &ExecutionFence,
    ) -> Result<(), ExecutionError> {
        if self.is_aborted() {
            return Err(ExecutionError::Aborted);
        }
        if self.fence != *live {
            return Err(ExecutionError::ParentConflict);
        }
        if self.mutation_revision != 0 || !self.facts.is_empty() || self.branch_ancestor.is_some() {
            return Err(ExecutionError::Invalid(
                "bootstrap transaction is not pristine read-only",
            ));
        }
        Ok(())
    }

    /// Validate retained readonly DATA after additional registered reads.
    /// This is deliberately not an execution/mutation admission: callers of
    /// writes, branches and final sealing still use the exact validate_guard.
    /// Any write (even same-value), ordered fact, branch resolution, abort or
    /// parent-fence change invalidates this source. No guard is refreshed.
    pub fn validate_read_source_guard(
        &self,
        guard: &PreparedGuard,
        live: &ExecutionFence,
    ) -> Result<(), ExecutionError> {
        if self.is_aborted() {
            return Err(ExecutionError::Aborted);
        }
        if self.fence != *live {
            return Err(ExecutionError::ParentConflict);
        }
        if !Arc::ptr_eq(&guard.owner, &self.owner)
            || guard.fence != self.fence
            || guard.mutation_revision != Some(self.mutation_revision)
            || guard.revision > self.revision
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
        if self.is_aborted() {
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
        self.mutation_revision = revision;
        Ok(())
    }

    /// Current ordered position only, not proof of an accepted occurrence.
    pub fn ordered_fact_count(&self) -> usize {
        self.facts.len()
    }

    /// Borrow an already appended provisional fact. This supplies neither an
    /// accepted occurrence nor permission to interpret its original content.
    pub fn ordered_fact(&self, position: usize) -> Option<&ExecutionFactRefV1> {
        self.facts.get(&position).map(|fact| &fact.value)
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
        if self.is_aborted() {
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
        self.mutation_revision = revision;
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
            &mut |_, _| Ok(()),
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
            &mut |_, _| Ok(()),
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
        self.seal_inner(
            guard,
            live,
            next_tick,
            &mut cancelled,
            Some(produce),
            &mut |_, _| Ok(()),
        )
    }

    /// Matching-enabled profile: Runtime, Cognition V3 and the independent
    /// ProofLedger Result. The legacy two-Result profile remains unchanged.
    pub fn seal_with_proof_ledger_results<E>(
        self,
        guard: &PreparedGuard,
        live: &ExecutionFence,
        next_tick: u64,
        mut cancelled: impl FnMut() -> bool,
        produce: impl FnOnce(&RootSetV1) -> Result<[ExecutionFactRefV1; 3], E>,
    ) -> Result<ReservedValue<PreparedTransition>, SealWithFinalFactError<E>> {
        self.seal_inner(
            guard,
            live,
            next_tick,
            &mut cancelled,
            Some(produce),
            &mut |_, _| Ok(()),
        )
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
        work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<ReservedValue<PreparedTransition>, SealWithFinalFactError<E>> {
        work(1, 0)?;
        self.validate_guard(guard, live)?;
        if self.branch_ancestor.is_some() {
            return Err(ExecutionError::Invalid("staged branch cannot seal").into());
        }
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
        let (built, first) = {
            let work = std::cell::RefCell::new(&mut *work);
            let first = std::cell::RefCell::new(None);
            let meter = |bytes: usize| {
                if first.borrow().is_some() {
                    return Err("seal work stopped".to_owned());
                }
                match (work.borrow_mut())(1, bytes as u64) {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        *first.borrow_mut() = Some(error);
                        Err("seal work stopped".to_owned())
                    }
                }
            };
            let built = self.build_current_delta_v1(next_tick, height, cancelled, Some(&meter));
            (built, first.into_inner())
        };
        if let Some(error) = first {
            return Err(error.into());
        }
        let CurrentDeltaV1 {
            after,
            writes,
            mut new_nodes,
        } = built.map_err(|error| match error {
            DeltaBuildError::Execution(e) => SealWithFinalFactError::Execution(e),
            DeltaBuildError::Cancelled => SealWithFinalFactError::Cancelled,
        })?;
        work(
            1,
            (self.reads * std::mem::size_of::<ExecutionReadV1>()) as u64,
        )?;
        let mut reads = Vec::with_capacity(self.reads);
        for (namespace, entries) in NAMESPACES.into_iter().zip(self.entries) {
            work(1, 0)?;
            for (key, entry) in entries {
                work(1, 0)?;
                reads.push(ExecutionReadV1 {
                    namespace,
                    key,
                    before: entry.before,
                });
            }
        }
        work(
            1,
            (fact_count * std::mem::size_of::<ExecutionFactRefV1>()) as u64,
        )?;
        let mut facts = Vec::with_capacity(fact_count);
        for fact in self.facts.into_values() {
            work(1, std::mem::size_of::<ExecutionFactRefV1>() as u64)?;
            facts.push(fact.value);
        }
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
        record.validate_with_work(work)?;
        if cancelled() {
            return Err(SealWithFinalFactError::Cancelled);
        }
        if let Some(produce) = produce {
            let facts = produce(&record.after).map_err(SealWithFinalFactError::Producer)?;
            if cancelled() {
                return Err(SealWithFinalFactError::Cancelled);
            }
            work(
                N as u64,
                (N * std::mem::size_of::<ExecutionFactRefV1>()) as u64,
            )?;
            if N == 2
                && (facts
                    .iter()
                    .any(|fact| fact.kind != ExecutionFactKindV1::Result)
                    || facts[0].id == facts[1].id)
            {
                return Err(ExecutionError::Invalid("final Result pair").into());
            }
            if N == 3 {
                // The independent ProofLedger profile is three distinct
                // Result originals, not an arbitrary three-fact append.
                work(3, 3 * 64)?;
                if facts
                    .iter()
                    .any(|fact| fact.kind != ExecutionFactKindV1::Result)
                    || facts[0].id == facts[1].id
                    || facts[0].id == facts[2].id
                    || facts[1].id == facts[2].id
                {
                    return Err(ExecutionError::Invalid("final Result triple").into());
                }
            }
            record.facts.extend(facts);
            record.validate_with_work(work)?;
        }
        sort_seal_nodes_with_work(&mut new_nodes, work)?;
        let candidate = PreparedTransition { record, new_nodes };
        let bytes = candidate.heap_bytes_with_work(work)?;
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

struct CurrentDeltaV1 {
    after: RootSetV1,
    writes: Vec<ExecutionWriteV1>,
    new_nodes: Vec<StateNodeWriteV1>,
}
enum DeltaBuildError {
    Execution(ExecutionError),
    Cancelled,
}
impl<R: MerkleNodeReader> ExecutionTransaction<'_, R> {
    fn build_current_delta_v1(
        &self,
        next_tick: u64,
        height: u64,
        cancelled: &mut impl FnMut() -> bool,
        meter: Option<&crate::merkle::WorkMeter<'_>>,
    ) -> Result<CurrentDeltaV1, DeltaBuildError> {
        if let Some(meter) = meter {
            meter(self.parent.roots.world_id.len() + 6 * 128)
                .map_err(|e| DeltaBuildError::Execution(ExecutionError::Tree(e)))?;
        }
        let mut after = self.parent.roots.clone();
        after.height = height;
        after.simulation_tick = next_tick;
        if let Some(meter) = meter {
            meter(self.dirty * std::mem::size_of::<ExecutionWriteV1>())
                .map_err(|e| DeltaBuildError::Execution(ExecutionError::Tree(e)))?;
        }
        let mut writes = Vec::with_capacity(self.dirty);
        let mut new_nodes = Vec::new();
        for namespace in NAMESPACES {
            if cancelled() {
                return Err(DeltaBuildError::Cancelled);
            }
            let mut mutations = Vec::new();
            for (key, entry) in &self.entries[namespace_index(namespace)] {
                if let Some(meter) = meter {
                    meter(0).map_err(|e| DeltaBuildError::Execution(ExecutionError::Tree(e)))?;
                }
                if entry.before != entry.after {
                    if let Some(meter) = meter {
                        meter(key.len() + std::mem::size_of::<StateWriteV1>())
                            .map_err(|e| DeltaBuildError::Execution(ExecutionError::Tree(e)))?;
                    }
                    mutations.push(StateWriteV1 {
                        key: key.clone(),
                        before: entry.before,
                        after: entry.after,
                    });
                }
            }
            if let Some(meter) = meter {
                meter(self.parent.roots.state_root(namespace).node_root.len())
                    .map_err(|e| DeltaBuildError::Execution(ExecutionError::Tree(e)))?;
            }
            let delta = StateDeltaV1::build_with_meter(
                self.parent.roots.state_root(namespace).clone(),
                mutations,
                self.source,
                self.limits.max_pending_node_bytes,
                meter,
            )
            .map_err(|e| DeltaBuildError::Execution(ExecutionError::Tree(e)))?;
            after.state[namespace_index(namespace)] = delta.after;
            for mutation in delta.writes {
                if let Some(meter) = meter {
                    meter(std::mem::size_of::<ExecutionWriteV1>())
                        .map_err(|e| DeltaBuildError::Execution(ExecutionError::Tree(e)))?;
                }
                writes.push(ExecutionWriteV1 {
                    namespace,
                    mutation,
                });
            }
            if let Some(meter) = meter {
                meter(delta.new_nodes.len() * std::mem::size_of::<StateNodeWriteV1>())
                    .map_err(|e| DeltaBuildError::Execution(ExecutionError::Tree(e)))?;
            }
            new_nodes.reserve(delta.new_nodes.len());
            for node in delta.new_nodes {
                if let Some(meter) = meter {
                    meter(0).map_err(|e| DeltaBuildError::Execution(ExecutionError::Tree(e)))?;
                }
                new_nodes.push(node);
            }
        }
        Ok(CurrentDeltaV1 {
            after,
            writes,
            new_nodes,
        })
    }
}
#[path = "incremental_current_roots_preview.rs"]
mod current_roots_preview;
pub use current_roots_preview::{CurrentRootsPreviewErrorV1, CurrentRootsPreviewV1};

#[path = "incremental_transaction_branch.rs"]
mod staged_branch;
pub use staged_branch::{
    StagedBranchDispositionV1, StagedBranchRejoinEvidenceV1, StagedExecutionBranchErrorV1,
    StagedExecutionBranchV1,
};

fn sort_seal_nodes_with_work(
    nodes: &mut [StateNodeWriteV1],
    work: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
) -> Result<(), ExecutionError> {
    fn sift(
        v: &mut [StateNodeWriteV1],
        mut root: usize,
        end: usize,
        w: &mut dyn FnMut(u64, u64) -> Result<(), ExecutionError>,
    ) -> Result<(), ExecutionError> {
        loop {
            let Some(mut child) = root
                .checked_mul(2)
                .and_then(|n| n.checked_add(1))
                .filter(|n| *n < end)
            else {
                return Ok(());
            };
            if child + 1 < end {
                w(
                    1,
                    v[child].digest.len().min(v[child + 1].digest.len()) as u64,
                )?;
                if v[child].digest < v[child + 1].digest {
                    child += 1;
                }
            }
            w(1, v[root].digest.len().min(v[child].digest.len()) as u64)?;
            if v[root].digest >= v[child].digest {
                return Ok(());
            }
            w(1, (2 * std::mem::size_of::<StateNodeWriteV1>()) as u64)?;
            v.swap(root, child);
            root = child;
        }
    }
    for root in (0..nodes.len() / 2).rev() {
        sift(nodes, root, nodes.len(), work)?;
    }
    for end in (1..nodes.len()).rev() {
        work(1, (2 * std::mem::size_of::<StateNodeWriteV1>()) as u64)?;
        nodes.swap(0, end);
        sift(nodes, 0, end, work)?;
    }
    Ok(())
}
