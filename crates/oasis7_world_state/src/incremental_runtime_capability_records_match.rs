//! Fixed, exhaustive Current matching for the original three capability DTOs.
//! The expected DTO is untrusted cache data. It is never a source qualification;
//! success requires every original projected cell from the authenticated child.
use super::*;
use crate::{
    incremental_expected_bytes::Compare,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_tree as tree,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard, RootSetV1},
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};

/// Returns the actual paid child only if the original fixed projection matches
/// in full. This is data verification, not a mutation or permission capability.
/// Original outer keys remain explicit, independent of embedded DTO fields.
#[allow(clippy::too_many_arguments)]
pub fn match_current_runtime_capability_record_v1<M, R, E, C>(
    family: &ReservedValue<RuntimeCapabilityFamilyQualificationV1>,
    roots: &RootSetV1,
    key: RuntimeCapabilityRecordKeyV1<'_>,
    expected: RuntimeCapabilityRecordRefV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut RuntimeCapabilityRecordsWorkV1,
    max_text_bytes: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<StateRootV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if expected.kind() != key.kind
            || ctx.values.class != MemoryClass::Ordinary
            || family.allocation_class() != MemoryClass::Ordinary
            || !family.uses_budget(ctx.values.memory)
            || family.reserved_bytes() < 512
            || !tx.uses_budget(ctx.values.memory)
        {
            return Err(read::invalid().into());
        }
        let _phase = ctx
            .values
            .memory
            .reserve(16 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let digest = roots.digest().map_err(|_| read::invalid())?;
        family.validate_binding(&digest)?;
        if digest != tx.parent_fence().parent_roots {
            return Err(read::invalid().into());
        }
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        let metadata = WorldFieldKeyV1::new(
            key.kind.descriptor(),
            WorldFieldRecordKindV1::Metadata,
            None,
        )
        .map_err(ValueTreeError::Invalid)?;
        work.spend()?;
        let meta = lookup(tx, &metadata, &mut records, &mut ctx.take_state_error)?
            .ok_or_else(read::invalid)?;
        records.presence(&meta, &metadata, 0)?;
        work.spend()?;
        let marker = read::marker(key.kind, &mut records)?;
        let marker_value = lookup(tx, &marker.0, &mut records, &mut ctx.take_state_error)?
            .ok_or_else(read::invalid)?;
        read::value(
            None,
            &marker_value,
            &marker,
            &mut records,
            0,
            &mut std::io::sink(),
        )?;
        work.spend()?;
        let outer = read::outer_key(key, &mut records)?;
        let Some(value) = lookup(tx, &outer.0, &mut records, &mut ctx.take_state_error)? else {
            return Ok(None);
        };
        let paid = records
            .values
            .memory
            .reserve(1024, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let child = read::outer(key.kind, &outer, &value, &mut records)?;
        if child.leaf_count != key.kind.leaf_count() {
            return Err(read::invalid().into());
        }
        work.spend()?;
        let child_metadata = tree::lookup(&child, &metadata, &mut records, genesis::LABELS)?
            .ok_or_else(read::invalid)?;
        records.presence(&child_metadata, &metadata, 0)?;
        let mut visited = 1u64; // The authenticated child Metadata above.
        visit_runtime_capability_record_v1(
            expected,
            |column, payload| -> Result<(), ValueTreeError> {
                if (records.cancelled)() {
                    return Err(ValueTreeError::Cancelled);
                }
                work.spend()?;
                let expected = match payload {
                    MemberPayloadRefV1::Unit => RuntimeCapabilityValueRefV1::Unit,
                    MemberPayloadRefV1::U64(value) => RuntimeCapabilityValueRefV1::Unsigned(value),
                    MemberPayloadRefV1::I64(value) => RuntimeCapabilityValueRefV1::Signed(value),
                    MemberPayloadRefV1::Text(value) => RuntimeCapabilityValueRefV1::Text(value),
                    _ => return Err(read::invalid()),
                };
                if let Some(column) = column {
                    expected.validate(column)?;
                }
                let cell = read::cell_key(key.kind, column, &mut records)?;
                let value = tree::lookup(&child, &cell.0, &mut records, genesis::LABELS)?
                    .ok_or_else(read::invalid)?;
                let mut compare = Compare::new(expected.text().as_bytes());
                let actual = read::value(
                    column,
                    &value,
                    &cell,
                    &mut records,
                    max_text_bytes,
                    &mut compare,
                )?;
                if !expected.matches(actual) || !compare.is_equal() {
                    return Err(read::invalid());
                }
                visited = visited.checked_add(1).ok_or_else(read::invalid)?;
                Ok(())
            },
        )?;
        // Every projected key is fixed and distinct. Matching the exact closed
        // leaf count excludes additional columns without scanning unrelated maps.
        if visited != child.leaf_count {
            return Err(read::invalid().into());
        }
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        drop(records);
        if let Some(error) = (ctx.take_state_error)() {
            return Err(error.into());
        }
        tx.validate_guard(&tx.guard(), live)?;
        Ok(Some(paid.retain(child)))
    })
}
