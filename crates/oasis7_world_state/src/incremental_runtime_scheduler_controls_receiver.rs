//! Qualified-parent inheritance of this exact eight-field group only.
use super::*;
use crate::{
    incremental_field_overlay::WorldFieldOverlayErrorV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_diff::{self, RecordDiffLabelsV1},
    incremental_world_field_key::WorldFieldKeyV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{TransitionRecordV1, TransitionVerifier},
    incremental_state::{MerkleNodeReader, StateRootV1},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::ReservedValue;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("runtime scheduler changed closure")
}
fn child<R: ValueNodeReader, C: FnMut() -> bool>(
    before: &StateRootV1,
    after: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    cells: &mut u64,
) -> Result<(), ValueTreeError> {
    if before.namespace != StateNamespaceV1::Scheduler
        || after.namespace != StateNamespaceV1::Scheduler
        || before.leaf_count != 8
        || after.leaf_count != 8
    {
        return Err(invalid());
    }
    let meta = WorldFieldKeyV1::new(descriptor(ROUTING), WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    incremental_record_diff::visit(
        before,
        after,
        records,
        cells,
        RecordDiffLabelsV1 {
            invalid: "runtime controls diff",
            cells: "runtime routing changed cells",
            stack: "runtime routing diff stack",
            tree: genesis::LABELS,
        },
        |old, new, records| {
            let entry = new.or(old).ok_or_else(invalid)?;
            if old.is_none() || new.is_none() || entry.namespace != StateNamespaceV1::Scheduler {
                return Err(invalid());
            }
            records.values.verify_closure(
                &entry.value,
                records.record_bytes,
                &mut *records.cancelled,
            )?;
            if entry.key == meta.as_bytes() {
                return records.presence(&entry.value, &meta, 0);
            }
            for column in
                std::iter::once(None).chain(RuntimeRoutingColumnV1::ALL.into_iter().map(Some))
            {
                let key = read::routing_key(column, records)?;
                if entry.key == key.0.as_bytes() {
                    read::routing_value(column, &entry.value, &key, records)?;
                    return Ok(());
                }
            }
            Err(invalid())
        },
    )?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(())
}
pub fn accept_runtime_scheduler_controls_transition_v1<
    M: MerkleNodeReader,
    S: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    parent: &ReservedValue<RuntimeSchedulerControlsQualificationV1>,
    verifier: &TransitionVerifier<'_, S>,
    record: &TransitionRecordV1,
    ctx: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    cells: &mut u64,
    max_cursor_bytes: u64,
) -> Result<ReservedValue<RuntimeSchedulerControlsQualificationV1>, WorldFieldOverlayErrorV1> {
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
    if let Some(e) = error {
        return Err(e.into());
    }
    let verified = verified?;
    if !verified.record.after.state.iter().any(|r| r == ctx.root) {
        return Err(invalid().into());
    }
    let mut routing = parent.routing_root().clone();
    for write in &verified.record.writes {
        *cells = cells.checked_sub(1).ok_or(ValueTreeError::ResourceLimited(
            "runtime controls changed writes",
        ))?;
        if !write
            .mutation
            .key
            .starts_with(b"oasis7.iws.world-field-key.v1")
        {
            continue;
        }
        let binding =
            WorldFieldKeyV1::from_bytes(&write.mutation.key).map_err(ValueTreeError::Invalid)?;
        if binding.owner != FieldOwner::Runtime {
            continue;
        }
        let scalar = RuntimeSchedulerScalarV1::ALL
            .into_iter()
            .find(|f| f.name() == binding.name);
        if scalar.is_none() && binding.name != ROUTING {
            continue;
        }
        if write.namespace != StateNamespaceV1::Scheduler
            || write.mutation.before.is_none()
            || write.mutation.after.is_none()
        {
            return Err(invalid().into());
        }
        let name = scalar.map_or(ROUTING, RuntimeSchedulerScalarV1::name);
        let meta = WorldFieldKeyV1::new(descriptor(name), WorldFieldRecordKindV1::Metadata, None)
            .map_err(ValueTreeError::Invalid)?;
        let value = write.mutation.after.ok_or_else(invalid)?;
        if binding.kind == WorldFieldRecordKindV1::Metadata {
            if meta.as_bytes() != write.mutation.key {
                return Err(invalid().into());
            }
            ctx.records().presence(&value, &meta, 0)?;
            continue;
        }
        if binding.kind != WorldFieldRecordKindV1::Entry {
            return Err(invalid().into());
        }
        let key = read::outer_key(name, &mut ctx.records())?;
        if key.0.as_bytes() != write.mutation.key {
            return Err(invalid().into());
        }
        ctx.values
            .verify_closure(&value, ctx.record_bytes, &mut ctx.cancelled)?;
        if let Some(field) = scalar {
            read::value(
                field.profile(),
                &value,
                &key,
                &mut ctx.records(),
                if field == RuntimeSchedulerScalarV1::Cursor {
                    max_cursor_bytes
                } else {
                    8
                },
                &mut std::io::sink(),
            )?;
        } else {
            let next = read::routing_root(&value, &key, &mut ctx.records())?;
            child(parent.routing_root(), &next, &mut ctx.records(), cells)?;
            routing = next;
        }
        ctx.check()?;
    }
    let result = qualification::mint(
        verified.record.after.digest().map_err(|_| invalid())?,
        routing,
        ctx.values.memory,
        ctx.values.class,
    )?;
    ctx.check()?;
    Ok(result)
}
