//! Inventory's charged StateWrite becomes a separately charged execution write.
use super::*;
use crate::incremental_inventory_genesis::{
    InventoryGenesisContext, InventoryWriteSink, prepare_inventory_genesis_v1,
};
use oasis7_formal_protocol::incremental_state::StateWriteV1;
use oasis7_proto::memory_budget::MemoryBudget;
struct Adapter<'a, S> {
    memory: &'a MemoryBudget,
    sink: &'a mut S,
}
impl<S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>> InventoryWriteSink
    for Adapter<'_, S>
{
    fn put_write(&mut self, write: ReservedValue<StateWriteV1>) -> Result<(), ValueTreeError> {
        if !write.uses_budget(self.memory) {
            return Err(ValueTreeError::Invalid("inventory GEN write pool"));
        }
        let parsed = WorldFieldKeyV1::from_bytes(&write.key).map_err(ValueTreeError::Invalid)?;
        if parsed.namespace != StateNamespaceV1::Business || parsed.owner != FieldOwner::WorldState
        {
            return Err(ValueTreeError::Invalid("inventory GEN namespace"));
        }
        let bytes = std::mem::size_of::<ExecutionWriteV1>()
            .checked_add(write.key.len())
            .and_then(|n| n.checked_add(512))
            .ok_or(ValueTreeError::ResourceLimited(
                "inventory execution write admission",
            ))?;
        let charge = self.memory.reserve(bytes, MemoryClass::Ordinary)?;
        let value = ExecutionWriteV1 {
            namespace: StateNamespaceV1::Business,
            mutation: StateWriteV1 {
                key: write.key.clone(),
                before: write.before,
                after: write.after,
            },
        };
        if std::mem::size_of::<ExecutionWriteV1>()
            .checked_add(value.mutation.key.capacity())
            .is_none_or(|n| n > charge.bytes())
        {
            drop(value);
            return Err(ValueTreeError::ResourceLimited(
                "inventory execution write capacity",
            ));
        }
        // `write` retains the original StateWrite admission through the callback.
        let result = (self.sink)(charge.retain(value));
        drop(write);
        result
    }
}
pub(super) fn prepare<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    remaining: &mut u64,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    let mut adapter = Adapter {
        memory: ctx.memory,
        sink,
    };
    let mut inventory = InventoryGenesisContext {
        values: &mut *ctx.sink,
        writes: &mut adapter,
        memory: ctx.memory,
        io: &mut *ctx.io,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.limits.semantic_bytes,
        record_bytes: ctx.limits.record_bytes,
        records_remaining: *remaining,
        cancelled: &mut ctx.cancelled,
    };
    let result = prepare_inventory_genesis_v1(source, &mut inventory);
    *remaining = inventory.records_remaining;
    result?;
    Ok(())
}

#[cfg(test)]
#[path = "incremental_world_state_genesis_inventory_tests.rs"]
mod tests;
