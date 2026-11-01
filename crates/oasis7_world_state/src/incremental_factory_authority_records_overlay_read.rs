//! The same guarded current-view read feeds getters and exact point edits.
use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::MemoryClass;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("factory_authority overlay cell/Option")
}
type CellPath = (FactoryAuthorityCellV1, (WorldFieldKeyV1, u64));
pub(super) struct Resolved {
    pub outer: WorldFieldKeyV1,
    pub child: StateRootV1,
    pub paths: [Option<CellPath>; 2],
    pub roots: [Option<[u8; 32]>; 2],
    pub value: Option<FactoryAuthorityValueV1>,
}
pub(super) fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: FactoryAuthorityCellRequestV1<'_>,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
    max: u64,
    output: &mut impl std::io::Write,
) -> Result<Resolved, WorldFieldOverlayErrorV1> {
    let kind = request.key.kind();
    let cell = request.cell;
    if !cell.accepts(kind) {
        return Err(invalid().into());
    }
    let (outer, child) = point::current(request.key, request.qualification, tx, records, error)?;
    let key = read::cell_key(kind, cell, records)?;
    let root = crate::incremental_record_tree::lookup(&child, &key.0, records, genesis::LABELS)?;
    let value = match root {
        Some(value) => Some(read::value(cell, &value, &key, records, max, output)?),
        None if cell == FactoryAuthorityCellV1::SourceModuleId => None,
        None => return Err(invalid().into()),
    };
    let mut paths = [Some((cell, key)), None];
    let mut roots = [root, None];
    if cell == FactoryAuthorityCellV1::SourceModuleId {
        let flag = read::cell_key(kind, FactoryAuthorityCellV1::SourceModulePresent, records)?;
        let root =
            crate::incremental_record_tree::lookup(&child, &flag.0, records, genesis::LABELS)?
                .ok_or_else(invalid)?;
        let actual = read::value(
            FactoryAuthorityCellV1::SourceModulePresent,
            &root,
            &flag,
            records,
            1,
            &mut std::io::sink(),
        )?;
        if actual != FactoryAuthorityValueV1::Bool(value.is_some()) {
            return Err(invalid().into());
        }
        paths[1] = Some((FactoryAuthorityCellV1::SourceModulePresent, flag));
        roots[1] = Some(root);
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(Resolved {
        outer,
        child,
        paths,
        roots,
        value,
    })
}
/// Read the actual shared ExecutionTransaction view, including previous point
/// edits. Output is provisional; any failure poisons the complete contribution.
pub fn read_qualified_factory_authority_cell_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: FactoryAuthorityCellRequestV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max: u64,
    output: &mut impl std::io::Write,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<FactoryAuthorityValueV1>, WorldFieldOverlayErrorV1> {
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !request.qualification.uses_budget(context.values.memory)
        {
            return Err(invalid().into());
        }
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
        let result = resolve(
            request,
            tx,
            &mut records,
            &mut context.take_state_error,
            max,
            output,
        )?;
        Ok(result.value)
    })
}
