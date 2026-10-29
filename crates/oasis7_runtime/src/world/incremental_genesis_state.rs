//! Bounded provisional state batches, shared by genesis field owners. The
//! mutable root is local preparation, never a durable/serving head.
use super::{
    incremental_body::{BodyMutationContext, BodyMutationError},
    incremental_scheduler_index::IndexReader,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionError,
    incremental_state::{StateDeltaV1, StatePageWriter, StateRootV1, StateWriteV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
pub(super) fn lookup<W: ValueNodeReader, C>(
    root: &StateRootV1,
    key: &[u8],
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<Option<[u8; 32]>, BodyMutationError> {
    let reader = IndexReader::new(
        &*context.sink,
        context.memory,
        MemoryClass::Ordinary,
        context.io,
    );
    reader.result(root.get(key, &reader).map_err(ExecutionError::Tree))
}
pub(super) fn apply<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    root: &mut StateRootV1,
    writes: Vec<StateWriteV1>,
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<(), BodyMutationError> {
    if writes.len() > 16 {
        return Err(ValueTreeError::ResourceLimited("genesis state batch").into());
    }
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let mut workspace = context
        .memory
        .reserve(4 << 20, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let delta = {
        let reader = IndexReader::new(
            &*context.sink,
            context.memory,
            MemoryClass::Ordinary,
            context.io,
        );
        reader.result(
            StateDeltaV1::build(root.clone(), writes, &reader, 1 << 20)
                .map_err(ExecutionError::Tree),
        )?
    };
    let size = 1024
        + delta.writes.capacity() * std::mem::size_of::<StateWriteV1>()
        + delta.writes.iter().map(|w| w.key.capacity()).sum::<usize>()
        + delta.new_nodes.capacity()
            * std::mem::size_of::<oasis7_formal_protocol::incremental_state::StateNodeWriteV1>()
        + delta
            .new_nodes
            .iter()
            .map(|n| n.digest.capacity() + n.bytes.capacity())
            .sum::<usize>();
    let charge = workspace.split(size).map_err(BodyMutationError::Invalid)?;
    let delta = charge.retain(delta);
    drop(workspace);
    let nodes = delta.new_nodes.len() as u64;
    let bytes = delta
        .new_nodes
        .iter()
        .try_fold(0u64, |sum, node| sum.checked_add(node.bytes.len() as u64))
        .ok_or(BodyMutationError::Invalid("genesis state bytes overflow"))?;
    if context.io.nodes_remaining < nodes || context.io.write_bytes_remaining < bytes {
        return Err(ValueTreeError::ResourceLimited("genesis state writes").into());
    }
    context.io.nodes_remaining -= nodes;
    context.io.write_bytes_remaining -= bytes;
    context.sink.put_state_pages(&delta, context.memory)?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    *root = delta.after.clone();
    Ok(())
}
