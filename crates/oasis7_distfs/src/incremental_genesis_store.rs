//! Bounded physical assembly of the sole fresh-world state index.
//! This stores provisional pages; complete owners, history and original authority
//! must still qualify and accept the resulting RootSet before any head is served.
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionWriteV1, RootSetV1},
    incremental_history::GenesisConfigV1,
    incremental_state::{
        MerkleNodeReader, STATE_MAX_KEY_BYTES, StateDeltaV1, StatePageWriter, StateWriteV1,
    },
};
const BATCH_ROWS: usize = 64;
const ROW_BUILD_HEAP: usize = 768 * 1024;

pub struct GenesisIndexWorkV1 {
    pub rows_remaining: u64,
    pub new_nodes_remaining: u64,
    pub new_bytes_remaining: u64,
    pub max_pending_node_bytes: usize,
}

#[cfg(test)]
#[path = "incremental_genesis_store_tests.rs"]
mod tests;
/// One fresh RootSet, one process pool and caller-owned finite physical sessions.
/// An error, panic or swallowed sink failure makes finish permanently fail.
pub struct GenesisStateIndexBuilderV1<'a> {
    store: &'a IncrementalPageStore,
    state: StatePageSession<'a>,
    output: ValuePageSession<'a>,
    cancelled: &'a AtomicBool,
    roots: RootSetV1,
    rows: Vec<ReservedValue<ExecutionWriteV1>>,
    work: GenesisIndexWorkV1,
    failed: bool,
    metadata: MemoryReservation,
}
struct Cancellable<'a> {
    state: &'a StatePageSession<'a>,
    cancelled: &'a AtomicBool,
}
impl MerkleNodeReader for Cancellable<'_> {
    type Node = ChargedMerklePage;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.state.allocation_budget()
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.state.allocation_class()
    }
    fn read_node(&self, id: &str, max: usize) -> std::result::Result<Option<Self::Node>, String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("genesis index cancelled".into());
        }
        let result = self.state.read_node(id, max);
        if self.cancelled.load(Ordering::Acquire) {
            return Err("genesis index cancelled".into());
        }
        result
    }
}
impl IncrementalPageStore {
    pub fn genesis_state_index_builder<'a>(
        &'a self,
        config: &GenesisConfigV1,
        work: GenesisIndexWorkV1,
        reads: PageIoBudget,
        writes: PageIoBudget,
        cancelled: &'a AtomicBool,
    ) -> std::result::Result<GenesisStateIndexBuilderV1<'a>, ValueTreeError> {
        if cancelled.load(Ordering::Acquire) {
            return Err(ValueTreeError::Cancelled);
        }
        if work.max_pending_node_bytes == 0 {
            return Err(ValueTreeError::Invalid("genesis pending node bound"));
        }
        let metadata = self.memory.reserve(128 << 10, MemoryClass::Ordinary)?;
        let scope = config
            .scope()
            .map_err(|_| ValueTreeError::Invalid("genesis config scope"))?;
        let roots = RootSetV1::empty(scope.world_id, scope.genesis_config)
            .map_err(|_| ValueTreeError::Invalid("genesis root scope"))?;
        let rows = Vec::with_capacity(BATCH_ROWS);
        if rows.capacity() * std::mem::size_of::<ReservedValue<ExecutionWriteV1>>()
            > metadata.bytes()
        {
            return Err(ValueTreeError::Invalid("genesis row metadata"));
        }
        Ok(GenesisStateIndexBuilderV1 {
            store: self,
            state: self
                .state_session(reads, MemoryClass::Ordinary)
                .map_err(value_error)?,
            output: self.value_session(writes),
            cancelled,
            roots,
            rows,
            work,
            failed: false,
            metadata,
        })
    }
}
impl GenesisStateIndexBuilderV1<'_> {
    fn check_cancelled(&self) -> std::result::Result<(), ValueTreeError> {
        if self.cancelled.load(Ordering::Acquire) {
            Err(ValueTreeError::Cancelled)
        } else {
            Ok(())
        }
    }
    fn push(
        &mut self,
        row: ReservedValue<ExecutionWriteV1>,
    ) -> std::result::Result<(), ValueTreeError> {
        self.check_cancelled()?;
        if !row.uses_budget(&self.store.memory)
            || row.reserved_bytes()
                < std::mem::size_of::<ExecutionWriteV1>()
                    .checked_add(row.mutation.key.capacity())
                    .ok_or(ValueTreeError::Invalid("genesis row admission overflow"))?
            || row.mutation.before.is_some()
            || row.mutation.after.is_none()
            || row.mutation.key.is_empty()
            || row.mutation.key.len() > STATE_MAX_KEY_BYTES
        {
            return Err(ValueTreeError::Invalid(
                "genesis row pool or creation precondition",
            ));
        }
        self.work.rows_remaining = self
            .work
            .rows_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("genesis row allowance"))?;
        if self.rows.len() == BATCH_ROWS {
            self.flush()?;
        }
        self.rows.push(row);
        self.check_cancelled()
    }
    /// All typed producers contribute through this sink. No component receives
    /// a fresh work allowance or permission to publish the provisional roots.
    pub fn contribute<E: From<ValueTreeError>>(
        &mut self,
        produce: impl FnOnce(
            &mut dyn FnMut(
                ReservedValue<ExecutionWriteV1>,
            ) -> std::result::Result<(), ValueTreeError>,
        ) -> std::result::Result<(), E>,
    ) -> std::result::Result<(), E> {
        if self.failed {
            return Err(ValueTreeError::Invalid("genesis index aborted").into());
        }
        self.failed = true;
        let mut sink_failed = false;
        let result = produce(&mut |row| {
            if sink_failed {
                return Err(ValueTreeError::Invalid("genesis sink aborted"));
            }
            // A producer may catch an unwinding sink and return success. Mark
            // the call failed before entering it, and clear only on Ok.
            sink_failed = true;
            let result = self.push(row);
            sink_failed = result.is_err();
            result
        });
        result?;
        if sink_failed {
            return Err(ValueTreeError::Invalid("genesis producer swallowed sink failure").into());
        }
        self.check_cancelled()?;
        self.failed = false;
        Ok(())
    }
    fn flush(&mut self) -> std::result::Result<(), ValueTreeError> {
        self.check_cancelled()?;
        for index in 0..self.roots.state.len() {
            let namespace = self.roots.state[index].namespace;
            let count = self
                .rows
                .iter()
                .filter(|r| r.namespace == namespace)
                .count();
            if count == 0 {
                continue;
            }
            let heap = count
                .checked_add(1)
                .and_then(|n| n.checked_mul(ROW_BUILD_HEAP))
                .ok_or(ValueTreeError::ResourceLimited("genesis index build heap"))?;
            let phase = self.store.memory.reserve(heap, MemoryClass::Ordinary)?;
            let mut writes: Vec<StateWriteV1> = Vec::with_capacity(count);
            for row in self.rows.iter().filter(|r| r.namespace == namespace) {
                writes.push(row.mutation.clone());
            }
            writes.sort_by(|a, b| a.key.cmp(&b.key));
            let source = Cancellable {
                state: &self.state,
                cancelled: self.cancelled,
            };
            let result = StateDeltaV1::build(
                self.roots.state[index].clone(),
                writes,
                &source,
                self.work.max_pending_node_bytes,
            );
            self.check_cancelled()?;
            if let Some(error) = self.state.take_read_error() {
                return Err(value_error(error));
            }
            let delta = phase.retain(result.map_err(|error| {
                if error.starts_with("ResourceLimited:") {
                    ValueTreeError::ResourceLimited("genesis pending nodes")
                } else {
                    ValueTreeError::Invalid("genesis duplicate key or state index")
                }
            })?);
            let nodes = u64::try_from(delta.new_nodes.len())
                .map_err(|_| ValueTreeError::ResourceLimited("genesis node count"))?;
            let bytes = delta
                .new_nodes
                .iter()
                .try_fold(0u64, |sum, node| sum.checked_add(node.bytes.len() as u64))
                .ok_or(ValueTreeError::ResourceLimited("genesis node byte count"))?;
            self.work.new_nodes_remaining = self
                .work
                .new_nodes_remaining
                .checked_sub(nodes)
                .ok_or(ValueTreeError::ResourceLimited("genesis new nodes"))?;
            self.work.new_bytes_remaining = self
                .work
                .new_bytes_remaining
                .checked_sub(bytes)
                .ok_or(ValueTreeError::ResourceLimited("genesis new bytes"))?;
            self.output
                .put_state_pages_cancelled(&delta, &self.store.memory, &mut || {
                    self.cancelled.load(Ordering::Acquire)
                })?;
            self.roots.state[index] = delta.after.clone();
        }
        self.rows.clear();
        self.check_cancelled()
    }
    /// Final output remains an unaccepted, unqualified genesis state index.
    pub fn finish(mut self) -> std::result::Result<ReservedValue<RootSetV1>, ValueTreeError> {
        if self.failed {
            return Err(ValueTreeError::Invalid("genesis index aborted"));
        }
        self.failed = true;
        self.flush()?;
        self.roots
            .validate()
            .map_err(|_| ValueTreeError::Invalid("genesis roots"))?;
        self.check_cancelled()?;
        let charge = self.metadata.split(8192).map_err(ValueTreeError::Invalid)?;
        Ok(charge.retain(self.roots))
    }
}
