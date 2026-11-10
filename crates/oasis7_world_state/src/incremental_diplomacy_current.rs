//! Current data for the two closed scalar Diplomacy record families.
//! This observation supplies no action, Accepted, or publication permission.
use super::*;
use crate::incremental_field_overlay::{WorldFieldOverlayErrorV1, lookup};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard, RootSetV1,
};

/// Opens exactly one actual Current scalar-record child, including pending replacement
/// or deletion of its outer entry. The outer key and embedded record ID remain
/// independent. The returned record can be read through the original scalar
/// reader; publication must independently recheck its actual Current root.
///
/// The parent qualification authenticates the complete parent namespace. Only
/// this selected Current child's bounded typed closure is traversed here. All
/// IO, key, scan, cell and text allowances belong to the caller and are consumed
/// continuously, including on failure. Any failure poisons the transaction.
#[allow(clippy::too_many_arguments)]
pub(super) fn open_current_diplomacy_record_v1<
    M: MerkleNodeReader,
    P: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    field: DiplomacyFieldV1,
    id: &str,
    parent: &ReservedValue<DiplomacyFamilyQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, M>,
    ctx: &mut WorldFieldReadContextV1<'_, P, R, E, C>,
    inner_scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<DiplomacyRecordQualificationV1>>, WorldFieldOverlayErrorV1> {
    tx.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        ctx.check()?;
        if !matches!(
            field,
            DiplomacyFieldV1::Crises | DiplomacyFieldV1::EconomicContracts
        ) || ctx.values.class != MemoryClass::Ordinary
            || !parent.uses_budget(ctx.values.memory)
            || !parent.permits_class(MemoryClass::Ordinary)
            || !tx.uses_budget(ctx.values.memory)
        {
            return Err(closure::invalid().into());
        }
        // Covers root encoding and the temporary fixed-size context/key work.
        let _phase = ctx
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let digest = roots.digest().map_err(|_| closure::invalid())?;
        parent.validate_binding(&digest)?;
        if digest != tx.parent_fence().parent_roots
            || ctx.root != roots.state_root(StateNamespaceV1::Business)
        {
            return Err(closure::invalid().into());
        }
        let outer = ctx.key(field.descriptor()?, WorldFieldRecordKindV1::Entry, id)?;
        let child = {
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut ctx.cancelled,
            };
            access::metadata(field, tx, &mut records, &mut ctx.take_state_error)?;
            let Some(value) = lookup(tx, &outer.0, &mut records, &mut ctx.take_state_error)? else {
                ctx.check()?;
                // These authenticated reads legitimately advance this tx's
                // read revision; they do not refresh the caller's live fence.
                tx.validate_guard(&tx.guard(), live)?;
                return Ok(None);
            };
            root::decode(
                field,
                &outer,
                value,
                records.values,
                records.record_bytes,
                records.cancelled,
            )?
        };
        let result = {
            let mut body = WorldFieldReadContextV1 {
                root: &child,
                state: ctx.state,
                values: oasis7_formal_protocol::incremental_value::ValueReadContext {
                    source: ctx.values.source,
                    memory: ctx.values.memory,
                    class: ctx.values.class,
                    io: &mut *ctx.values.io,
                },
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                take_state_error: &mut ctx.take_state_error,
                cancelled: &mut ctx.cancelled,
            };
            qualify_diplomacy_body_v1(
                field,
                access::digest(&outer.0)?,
                &mut body,
                inner_scan,
                cells,
                max,
            )?
        };
        ctx.check()?;
        tx.validate_guard(&tx.guard(), live)?;
        Ok(Some(result))
    })
}
