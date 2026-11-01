//! Shared one-target resolution for ordinary getters and updates.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::MemoryClass;
pub(super) struct Resolved {
    pub outer: WorldFieldKeyV1,
    pub child: StateRootV1,
    pub cell_key: (WorldFieldKeyV1, u64),
    pub cell_root: [u8; 32],
    pub value: FixedEconomyValueV1,
}
pub(super) fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: FixedEconomyCellRequestV1<'_>,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<Resolved, WorldFieldOverlayErrorV1> {
    if records.values.class != MemoryClass::Ordinary
        || !tx.uses_budget(records.values.memory)
        || !request.qualification.uses_budget(records.values.memory)
    {
        return Err(read::invalid().into());
    }
    let field = request.column.field();
    let meta = WorldFieldKeyV1::new(field.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let meta_root = lookup(tx, &meta, records, error)?.ok_or_else(read::invalid)?;
    records.presence(&meta_root, &meta, 0)?;
    let outer = read::cell_key(field, None, records)?;
    let value = lookup(tx, &outer.0, records, error)?.ok_or_else(read::invalid)?;
    let child = read::outer(field, &outer, &value, records)?;
    request.qualification.validate_binding(field, &child)?;
    let key = read::cell_key(field, Some(request.column), records)?;
    let root = crate::incremental_record_tree::lookup(&child, &key.0, records, genesis::LABELS)?
        .ok_or_else(read::invalid)?;
    let value =
        FixedEconomyValueV1::from_record(read::value(Some(request.column), &root, &key, records)?)?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(Resolved {
        outer: outer.0,
        child: child.clone(),
        cell_key: key,
        cell_root: root,
        value,
    })
}
/// Reads the current ordinary overlay, including earlier same-Txn typed writes.
/// The shared resolver's last cancellation check is the getter return boundary.
pub fn read_qualified_fixed_economy_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: FixedEconomyCellRequestV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<FixedEconomyValueV1, WorldFieldOverlayErrorV1> {
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let _operation = context
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut context.cancelled,
        };
        Ok(resolve(request, tx, &mut records, &mut context.take_state_error)?.value)
    })
}
