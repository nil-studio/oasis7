//! Explicit world-ledger empty-group creation, with no synthetic member key.
use super::*;
use crate::{
    MaterialLedgerId,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_field_presence::{PresenceRecordWriteContext, write_field_group_v1},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionFence, PreparedGuard},
    incremental_value::ValueNodeWriter,
};
/// Continuous caller-owned work, separate from the original semantic/IO budgets.
pub struct InventoryWorldGroupWorkV1 {
    pub steps_remaining: u64,
}
impl InventoryWorldGroupWorkV1 {
    fn step(
        &mut self,
        read: &mut impl FnMut() -> bool,
        write: &mut impl FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        if read() || write() {
            return Err(ValueTreeError::Cancelled);
        }
        self.steps_remaining = self
            .steps_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("world ledger group steps"))?;
        Ok(())
    }
}
/// Return true only when the actual missing world ledger is created. The true
/// Current scope proves there are no orphan world members; a caller boolean or
/// record count cannot replace it. Existing groups are authenticated no-ops.
#[allow(clippy::too_many_arguments)]
pub fn ensure_world_ledger_group_overlay_v1<M, R, E, C, W, CW>(
    source: &ReservedValue<CurrentInventoryMaterialScopeV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut InventoryWorldGroupWorkV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
{
    let result = (|| {
        ctx.transaction.validate_guard(guard, live)?;
        if ctx.values.class != MemoryClass::Ordinary
            || !ctx.transaction.uses_budget(ctx.values.memory)
            || !source.uses_budget(ctx.values.memory)
            || !ctx.values.memory.same_pool(write.memory)
        {
            return Err(invalid().into());
        }
        source.validate_current(ctx, guard, live)?;
        work.step(&mut ctx.cancelled, &mut write.cancelled)?;
        let _phase = write
            .memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let field = descriptor("material_ledgers", FieldShape::NestedMap).map_err(|_| invalid())?;
        let metadata = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        ctx.transaction.contribute(|transaction| {
            transaction.validate_guard(guard, live)?;
            let root = lookup(
                transaction,
                &metadata,
                &mut records,
                &mut ctx.take_state_error,
            )?
            .ok_or_else(invalid)?;
            records.presence(&root, &metadata, 0)?;
            work.step(records.cancelled, &mut write.cancelled)?;
            let (group, semantic_bytes) = records.key(
                field,
                WorldFieldRecordKindV1::Group,
                &MaterialLedgerId::World,
            )?;
            let actual = lookup(transaction, &group, &mut records, &mut ctx.take_state_error)?;
            if let Some(root) = actual {
                records.presence(&root, &group, semantic_bytes)?;
            }
            if actual.is_some() != source.world_ledger_present() {
                return Err(invalid().into());
            }
            work.step(records.cancelled, &mut write.cancelled)?;
            if actual.is_some() {
                return Ok(false);
            }
            if source.world_materials().len() != 0 {
                return Err(invalid().into());
            }
            let record = write_field_group_v1(
                field,
                &MaterialLedgerId::World,
                &mut PresenceRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: PresenceRecordLimitsV1 {
                        semantic_bytes: write.limits.semantic_bytes,
                        record_bytes: write.limits.record_bytes,
                    },
                    cancelled: || (records.cancelled)() || (write.cancelled)(),
                },
            )
            .map_err(WorldFieldOverlayErrorV1::Member)?;
            if record.field_key.as_bytes() != group.as_bytes() {
                return Err(invalid().into());
            }
            work.step(records.cancelled, &mut write.cancelled)?;
            transaction.write(
                StateNamespaceV1::Business,
                group.as_bytes(),
                Some(record.value_root),
            )?;
            work.step(records.cancelled, &mut write.cancelled)?;
            Ok(true)
        })
    })();
    if result.is_err() {
        let _: Result<(), ExecutionError> =
            ctx.transaction.contribute(|_| Err(ExecutionError::Aborted));
    }
    result
}
#[cfg(test)]
#[path = "incremental_inventory_group_overlay_tests.rs"]
mod tests;
