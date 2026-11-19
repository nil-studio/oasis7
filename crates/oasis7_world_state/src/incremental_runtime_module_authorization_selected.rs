//! Actual same-transaction module authorization selection. No DTO input can
//! substitute for the current outer record or its qualified owner/schema.
use super::{overlay, read::*, *};
use crate::incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1};
use crate::incremental_inventory_value_read::InventoryValueReader;
use oasis7_formal_protocol::{
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;

/// Read the actual current ordered policy length after prior contributions.
pub fn read_module_policy_length_overlay_v1<M, R, E, C>(
    family: &ReservedValue<ModuleAuthorizationFamilyQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<u64, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut records = InventoryValueReader {
        values: &mut ctx.values,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    // Reuse the exact current-record kernel even for an empty policy list:
    // its metadata/length/membership checks must precede numeric length use.
    let _ = overlay::current(
        ModuleAuthorizationRecordKeyV1::Policy(0),
        family,
        ctx.transaction,
        &mut records,
        &mut ctx.take_state_error,
    )?;
    overlay::current_length(ctx.transaction, &mut records, &mut ctx.take_state_error)
}

/// Open precisely the current point body, retaining the same qualification as
/// the parent reader. `overlay::current` checks parent scope, field metadata,
/// policy membership/length, and exact outer record owner/schema.
pub fn open_module_authorization_body_overlay_v1<M, R, E, C>(
    key: ModuleAuthorizationRecordKeyV1<'_>,
    family: &ReservedValue<ModuleAuthorizationFamilyQualificationV1>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<Option<ReservedValue<ModuleAuthorizationBodyQualificationV1>>, WorldFieldOverlayErrorV1>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
{
    let mut records = InventoryValueReader {
        values: &mut ctx.values,
        key_work: &mut *ctx.key_work,
        semantic_bytes: ctx.semantic_bytes,
        record_bytes: ctx.record_bytes,
        cancelled: &mut ctx.cancelled,
    };
    let (_, child) = overlay::current(
        key,
        family,
        ctx.transaction,
        &mut records,
        &mut ctx.take_state_error,
    )?;
    let result = child
        .map(|(_, root)| {
            records
                .values
                .memory
                .reserve(512, records.values.class)
                .map(|charge| {
                    charge.retain(ModuleAuthorizationBodyQualificationV1 {
                        root: (*root).clone(),
                        field: key.field(),
                        class: records.values.class,
                    })
                })
        })
        .transpose()
        .map_err(ValueTreeError::Memory)?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(result)
}
