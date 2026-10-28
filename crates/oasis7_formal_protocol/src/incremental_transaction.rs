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
        validate_source_budget(source, memory)?;
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

    pub fn uses_budget(&self, memory: &MemoryBudget) -> bool {
        self.memory.same_pool(memory)
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
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(ExecutionError::ResourceLimited("transaction revision"))?;
        self.read_inner(namespace, key)?;
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
        self.validate_guard(guard, live)?;
        if next_tick < self.parent.roots.simulation_tick {
            return Err(ExecutionError::Invalid("tick regression"));
        }
        let height = self
            .parent
            .roots
            .height
            .checked_add(1)
            .ok_or(ExecutionError::Invalid("height overflow"))?;
        let mut workspace = self.memory.reserve(
            transition::phase_bound(self.dirty, self.reads, self.facts.len())?,
            MemoryClass::Ordinary,
        )?;
        let mut after = self.parent.roots.clone();
        after.height = height;
        after.simulation_tick = next_tick;
        let mut writes = Vec::with_capacity(self.dirty);
        let mut new_nodes = Vec::new();
        for namespace in NAMESPACES {
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
        let facts = self.facts.into_values().map(|fact| fact.value).collect();
        let record = TransitionRecordV1 {
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
        new_nodes.sort_unstable_by(|a, b| a.digest.cmp(&b.digest));
        let candidate = PreparedTransition { record, new_nodes };
        let bytes = candidate.heap_bytes()?;
        let charge = workspace
            .split(bytes)
            .map_err(|_| ExecutionError::Invalid("transition exceeded workspace accounting"))?;
        Ok(charge.retain(candidate))
    }
}
