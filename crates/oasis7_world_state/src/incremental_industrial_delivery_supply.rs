//! Actual Current supply opener through the original fixed-economy grammar.
//! The returned closed four-cell body is data, never claim/mint authority.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard, RootSetV1},
    incremental_state::{MerkleNodeReader, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
pub fn open_current_industrial_main_token_supply_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    family: &ReservedValue<FixedEconomyFamilyQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    state: &P,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<FixedEconomyRecordQualificationV1>, WorldFieldOverlayErrorV1> {
    open_current_fixed_economy_projection_body_v1(
        FixedEconomyFieldV1::MainTokenSupply,
        family,
        roots,
        ctx,
        state,
        scan,
        cells,
        guard,
        live,
    )
}
#[expect(
    clippy::too_many_arguments,
    reason = "Bounded inputs are independently validated"
)]
pub(super) fn open_current_fixed_economy_projection_body_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: FixedEconomyFieldV1,
    family: &ReservedValue<FixedEconomyFamilyQualificationV1>,
    roots: &RootSetV1,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    state: &P,
    scan: &mut StateScanBudget,
    cells: &mut u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<FixedEconomyRecordQualificationV1>, WorldFieldOverlayErrorV1> {
    let error = &mut ctx.take_state_error;
    let cancel = &mut ctx.cancelled;
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let _phase = ctx
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let digest = roots.digest().map_err(|_| read::invalid())?;
        if ctx.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(ctx.values.memory)
            || !family.uses_budget(ctx.values.memory)
            || family.allocation_class() != MemoryClass::Ordinary
            || digest != tx.parent_fence().parent_roots
        {
            return Err(read::invalid().into());
        }
        family.validate_binding(&digest)?;
        let child = {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut *cancel,
            };
            let metadata =
                WorldFieldKeyV1::new(field.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
                    .map_err(ValueTreeError::Invalid)?;
            let m = lookup(tx, &metadata, &mut records, error)?.ok_or_else(read::invalid)?;
            records.presence(&m, &metadata, 0)?;
            let key = read::cell_key(field, None, &mut records)?;
            let root = lookup(tx, &key.0, &mut records, error)?.ok_or_else(read::invalid)?;
            read::outer(field, &key, &root, &mut records)?
        };
        let mut selected = WorldFieldReadContextV1 {
            root: &child,
            state,
            values: ValueReadContext {
                source: ctx.values.source,
                memory: ctx.values.memory,
                class: MemoryClass::Ordinary,
                io: &mut *ctx.values.io,
            },
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            take_state_error: &mut *error,
            cancelled: &mut *cancel,
        };
        let result = qualify_fixed_economy_body_v1(field, &mut selected, scan, cells)?;
        selected.check()?;
        tx.validate_guard(&tx.guard(), live)?;
        Ok(result)
    })
}
