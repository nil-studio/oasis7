//! Replay integrity plus changed-path closure inherited from a qualified parent.
use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_diff::{self, RecordDiffLabelsV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{TransitionRecordV1, TransitionVerifier},
    incremental_state::{MerkleNodeReader, StateEntryV1, StateRootV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueReadContext},
};
use oasis7_proto::memory_budget::ReservedValue;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("material changed closure")
}
fn changed<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: MaterialRecordKindV1,
    entry: &StateEntryV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    max: u64,
) -> Result<Option<MaterialCellV1>, ValueTreeError> {
    if entry.namespace != kind.namespace() {
        return Err(invalid());
    }
    let meta = WorldFieldKeyV1::new(kind.descriptor()?, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    if entry.key == meta.as_bytes() {
        records.presence(&entry.value, &meta, 0)?;
        return Ok(None);
    }
    for cell in MaterialCellV1::ALL.into_iter().filter(|c| c.accepts(kind)) {
        let key = read::cell_key(kind, cell, records)?;
        if entry.key == key.0.as_bytes() {
            read::value(cell, &entry.value, &key, records, max, &mut std::io::sink())?;
            return Ok(Some(cell));
        }
    }
    Err(invalid())
}
fn child<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: MaterialRecordKindV1,
    before: &StateRootV1,
    after: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    cells: &mut u64,
    max: u64,
) -> Result<(), ValueTreeError> {
    if before.namespace != kind.namespace() || after.namespace != kind.namespace() {
        return Err(invalid());
    }
    incremental_record_diff::visit(
        before,
        after,
        records,
        cells,
        RecordDiffLabelsV1 {
            invalid: "material diff",
            cells: "material changed paths",
            stack: "material diff stack",
            tree: genesis::LABELS,
        },
        |old, new, records| {
            let entry = new.or(old).ok_or_else(invalid)?;
            let cell = changed(kind, entry, records, max)?;
            if cell != Some(MaterialCellV1::RouteId) && (old.is_none() || new.is_none()) {
                return Err(invalid());
            }
            Ok(())
        },
    )?;
    if kind == MaterialRecordKindV1::Transfer {
        let flag = read::cell_key(kind, MaterialCellV1::RoutePresent, records)?;
        let flag_root =
            crate::incremental_record_tree::lookup(after, &flag.0, records, genesis::LABELS)?
                .ok_or_else(invalid)?;
        let value = read::value(
            MaterialCellV1::RoutePresent,
            &flag_root,
            &flag,
            records,
            1,
            &mut std::io::sink(),
        )?;
        let route = read::cell_key(kind, MaterialCellV1::RouteId, records)?;
        let present =
            crate::incremental_record_tree::lookup(after, &route.0, records, genesis::LABELS)?
                .is_some();
        if value != MaterialValueV1::Bool(present) {
            return Err(invalid());
        }
    }
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
/// Requires the exact parent family token and independent generic replay.
/// Non-material owners retain responsibility for their own typed closures.
pub fn accept_material_transition_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<MaterialFamilyQualificationV1>,
    verifier: &TransitionVerifier<'_, S>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    inner_scan: &mut StateScanBudget,
    cells: &mut u64,
    max: u64,
) -> Result<ReservedValue<MaterialFamilyQualificationV1>, WorldFieldOverlayErrorV1> {
    ctx.check()?;
    if !parent.uses_budget(ctx.values.memory)
        || !verifier.memory.same_pool(ctx.values.memory)
        || verifier.class != ctx.values.class
    {
        return Err(invalid().into());
    }
    parent.validate_binding(&record.before.digest().map_err(|_| invalid())?)?;
    let verified = verifier.verify(record);
    let error = (ctx.take_state_error)();
    ctx.check()?;
    if let Some(error) = error {
        return Err(error.into());
    }
    let verified = verified?;
    if !verified.record.after.state.iter().any(|r| r == ctx.root) {
        return Err(invalid().into());
    }
    let mut counts = parent.record_counts();
    for write in &verified.record.writes {
        *cells = cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited(
            "material outer changed cells",
        ))?;
        if !write
            .mutation
            .key
            .starts_with(b"oasis7.iws.world-field-key.v1\0")
        {
            continue;
        }
        let binding =
            WorldFieldKeyV1::from_bytes(&write.mutation.key).map_err(ValueTreeError::Invalid)?;
        let Some(kind) = MaterialRecordKindV1::ALL
            .into_iter()
            .find(|k| k.field_name() == binding.name)
        else {
            continue;
        };
        let edge = |value: Option<[u8; 32]>, ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>| {
            value
                .map(|value| {
                    family::outer(
                        &StateEntryV1 {
                            namespace: write.namespace,
                            key: write.mutation.key.clone(),
                            value,
                        },
                        ctx,
                    )
                })
                .transpose()
                .map(Option::flatten)
        };
        let old = edge(write.mutation.before, ctx)?;
        let new = edge(write.mutation.after, ctx)?;
        if binding.kind == WorldFieldRecordKindV1::Metadata || old.is_none() && new.is_none() {
            // Metadata and format markers cannot be removed or freshly introduced.
            if write.mutation.before.is_none() || write.mutation.after.is_none() {
                return Err(invalid().into());
            }
            continue;
        }
        let index = match kind {
            MaterialRecordKindV1::Profile => 0,
            MaterialRecordKindV1::Transfer => 1,
        };
        match (old, new) {
            (None, Some((owner_kind, owner, root))) => {
                let mut body = WorldFieldReadContextV1 {
                    root: &root,
                    state: ctx.state,
                    values: ValueReadContext {
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
                qualify_material_body_v1(owner_kind, owner, &mut body, inner_scan, cells, max)?;
                counts[index] = counts[index].checked_add(1).ok_or_else(invalid)?;
            }
            (Some((old_kind, old_owner, old)), Some((new_kind, new_owner, new))) => {
                if old_kind != new_kind || old_owner != new_owner {
                    return Err(invalid().into());
                }
                child(kind, &old, &new, &mut ctx.records(), cells, max)?;
            }
            (Some(_), None) => {
                counts[index] = counts[index].checked_sub(1).ok_or_else(invalid)?;
            }
            (None, None) => return Err(invalid().into()),
        }
        ctx.check()?;
    }
    let result = family::mint(
        verified.record.after.digest().map_err(|_| invalid())?,
        counts,
        ctx.values.memory,
        ctx.values.class,
    )?;
    ctx.check()?;
    Ok(result)
}
