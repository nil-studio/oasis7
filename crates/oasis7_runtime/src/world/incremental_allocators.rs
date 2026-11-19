//! Original rolling ID state in the shared IWS allocator overlay.
use super::{
    World,
    incremental_body::{BodyMutationContext, BodyMutationError, write_field_bytes},
    incremental_body_event::read_fixed,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionTransaction},
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_world_state::incremental_event_fields::EVENT_SEQUENCE_BYTES;
pub use oasis7_world_state::incremental_runtime_allocators_codec::{
    RuntimeAllocatorV1, RuntimeSequenceV1,
};
const BYTES: usize =
    oasis7_world_state::incremental_runtime_allocators_codec::RUNTIME_SEQUENCE_BYTES_V1;
fn check<W, C: FnMut() -> bool>(
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<(), BodyMutationError> {
    if (context.cancelled)() {
        Err(ValueTreeError::Cancelled.into())
    } else {
        Ok(())
    }
}
fn read<R: MerkleNodeReader, V: ValueNodeReader, W, C: FnMut() -> bool>(
    allocator: RuntimeAllocatorV1,
    txn: &mut ExecutionTransaction<'_, R>,
    values: &V,
    context: &mut BodyMutationContext<'_, W, C>,
) -> Result<RuntimeSequenceV1, BodyMutationError> {
    if !txn.uses_budget(context.memory) {
        return Err(BodyMutationError::Invalid("runtime allocator memory pool"));
    }
    check(context)?;
    let (key, length) = allocator.key();
    let value = if allocator == RuntimeAllocatorV1::Event {
        allocator
            .from_canonical_bytes(&read_fixed::<EVENT_SEQUENCE_BYTES, _, _, _, _>(
                txn,
                Ns::Allocator,
                &key[..length],
                values,
                context,
            )?)
            .map_err(BodyMutationError::Invalid)?
    } else {
        allocator
            .from_canonical_bytes(&read_fixed::<BYTES, _, _, _, _>(
                txn,
                Ns::Allocator,
                &key[..length],
                values,
                context,
            )?)
            .map_err(BodyMutationError::Invalid)?
    };
    check(context)?;
    Ok(value)
}
impl World {
    /// Borrow all four original pairs. No journal scan, ID allocation or head publication.
    pub fn write_iws_allocator_genesis<
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        F: FnMut(&[u8], [u8; 32]) -> Result<(), BodyMutationError>,
    >(
        &self,
        context: &mut BodyMutationContext<'_, W, C>,
        mut emit: F,
    ) -> Result<(), BodyMutationError> {
        let _charge = context
            .memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ExecutionError::Memory)?;
        for (allocator, next, era) in [
            (
                RuntimeAllocatorV1::Event,
                self.next_event_id,
                self.next_event_id_era,
            ),
            (
                RuntimeAllocatorV1::Action,
                self.next_action_id,
                self.next_action_id_era,
            ),
            (
                RuntimeAllocatorV1::Intent,
                self.next_intent_id,
                self.next_intent_id_era,
            ),
            (
                RuntimeAllocatorV1::Proposal,
                self.next_proposal_id,
                self.next_proposal_id_era,
            ),
        ] {
            check(context)?;
            let (key, length) = allocator.key();
            let (bytes, size) = allocator.canonical_bytes(RuntimeSequenceV1 { next, era });
            let id = write_field_bytes(&bytes[..size], context)?;
            emit(&key[..length], id)?;
        }
        check(context)
    }
    /// Read exact original control values; a failure poisons the shared transaction.
    pub fn read_iws_allocator<R: MerkleNodeReader, V: ValueNodeReader, W, C: FnMut() -> bool>(
        allocator: RuntimeAllocatorV1,
        txn: &mut ExecutionTransaction<'_, R>,
        values: &V,
        context: &mut BodyMutationContext<'_, W, C>,
    ) -> Result<RuntimeSequenceV1, BodyMutationError> {
        txn.contribute(|txn| read(allocator, txn, values, context))
    }
    /// Allocate using the original rollover rule and exact overlay precondition.
    /// The caller owns all linked business, event, preallocation and acceptance obligations.
    pub fn contribute_iws_allocate<
        R: MerkleNodeReader,
        V: ValueNodeReader,
        W: ValueNodeWriter,
        C: FnMut() -> bool,
    >(
        allocator: RuntimeAllocatorV1,
        expected: RuntimeSequenceV1,
        txn: &mut ExecutionTransaction<'_, R>,
        values: &V,
        context: &mut BodyMutationContext<'_, W, C>,
    ) -> Result<(u64, RuntimeSequenceV1), BodyMutationError> {
        txn.contribute(|txn| {
            let current = read(allocator, txn, values, context)?;
            if current != expected {
                return Err(BodyMutationError::Invalid("runtime allocator precondition"));
            }
            let (allocated, next, era) =
                Self::preview_rolling_sequence_id(current.next, current.era);
            let after = RuntimeSequenceV1 { next, era };
            let (key, length) = allocator.key();
            let (bytes, size) = allocator.canonical_bytes(after);
            let id = write_field_bytes(&bytes[..size], context)?;
            txn.write(Ns::Allocator, &key[..length], Some(id))?;
            check(context)?;
            Ok((allocated, after))
        })
    }
}

#[cfg(test)]
#[path = "incremental_allocators_tests.rs"]
mod tests;
