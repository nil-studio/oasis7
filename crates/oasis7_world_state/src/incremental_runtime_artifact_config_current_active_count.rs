//! Actual Current RegistryActive cardinality. DATA only: registered but inactive
//! module records do not make the original active map nonempty.
use super::*;
use crate::incremental_field_member::*;
use crate::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup,
};
use crate::incremental_inventory_value_read::InventoryValueReader;
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
#[allow(clippy::too_many_arguments)]
pub fn read_current_registry_active_count_v1<M, R, E, C>(
    parent: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    remaining_entries: &mut usize,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<u64, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if !parent.uses_budget(ctx.values.memory)
            || parent.allocation_class() != MemoryClass::Ordinary
            || ctx.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
        {
            return Err(read::invalid().into());
        }
        parent.validate_binding(&tx.parent_fence().parent_roots)?;
        let _phase = ctx
            .values
            .memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let field = ArtifactConfigFieldV1::RegistryActive;
        let descriptor = field.descriptor();
        let mut count = parent.registry_active_count();
        tx.visit_pending_writes::<WorldFieldOverlayErrorV1>(
            StateNamespaceV1::Artifacts,
            remaining_entries,
            || {
                if (ctx.cancelled)() {
                    Err(ValueTreeError::Cancelled.into())
                } else {
                    Ok(())
                }
            },
            |key, before, after| {
                let Ok(binding) = WorldFieldKeyV1::from_bytes(key) else {
                    return Ok(());
                };
                if binding.owner != descriptor.owner || binding.name != descriptor.name {
                    return Ok(());
                }
                let exact = WorldFieldKeyV1::new(descriptor, binding.kind, binding.semantic_key)
                    .map_err(ValueTreeError::Invalid)?;
                if exact.as_bytes() != key {
                    return Err(read::invalid().into());
                }
                match binding.kind {
                    WorldFieldRecordKindV1::Metadata => {}
                    WorldFieldRecordKindV1::Entry => {
                        count = match (before.is_some(), after.is_some()) {
                            (false, true) => count.checked_add(1),
                            (true, false) => count.checked_sub(1),
                            _ => Some(count),
                        }
                        .ok_or_else(read::invalid)?;
                    }
                    _ => return Err(read::invalid().into()),
                }
                Ok(())
            },
        )?;
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        let metadata = WorldFieldKeyV1::new(descriptor, WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let id = lookup(tx, &metadata, &mut records, &mut ctx.take_state_error)?
            .ok_or_else(read::invalid)?;
        records.presence(&id, &metadata, 0)?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if let Some(error) = (ctx.take_state_error)() {
            return Err(error.into());
        }
        Ok(count)
    })
}
