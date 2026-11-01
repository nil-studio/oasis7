use super::*;
use crate::{
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_tree::{self as record_tree, RecordTreeWritePolicyV1},
    incremental_world_field_key::WorldFieldKeyV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter, StateRootV1},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("runtime routing expected/profile/binding")
}
pub struct RuntimeRoutingRequestV1<'a> {
    pub column: RuntimeRoutingColumnV1,
    pub family: &'a ReservedValue<RuntimeSchedulerControlsQualificationV1>,
    pub qualification: &'a ReservedValue<RuntimeRoutingQualificationV1>,
}
struct Resolved {
    outer: WorldFieldKeyV1,
    child: StateRootV1,
    key: (WorldFieldKeyV1, u64),
    before: [u8; 32],
    value: RuntimeSchedulerControlValueV1,
}
fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: RuntimeRoutingRequestV1<'_>,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<Resolved, WorldFieldOverlayErrorV1> {
    point::parent_binding(request.family, tx, records)?;
    if !request.qualification.uses_budget(records.values.memory) {
        return Err(invalid().into());
    }
    point::metadata(ROUTING, tx, records, error)?;
    let outer = read::outer_key(ROUTING, records)?;
    let value = lookup(tx, &outer.0, records, error)?.ok_or_else(invalid)?;
    let child = read::routing_root(&value, &outer, records)?;
    request.qualification.validate(&child)?;
    let key = read::routing_key(Some(request.column), records)?;
    let before =
        record_tree::lookup(&child, &key.0, records, genesis::LABELS)?.ok_or_else(invalid)?;
    let value = read::routing_value(Some(request.column), &before, &key, records)?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(Resolved {
        outer: outer.0,
        child,
        key,
        before,
        value,
    })
}
pub fn read_qualified_runtime_routing_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: RuntimeRoutingRequestV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<RuntimeSchedulerControlValueV1, WorldFieldOverlayErrorV1> {
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        point::parent_binding(request.family, tx, &records)?;
        let _admission = records
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        Ok(resolve(request, tx, &mut records, &mut ctx.take_state_error)?.value)
    })
}
pub struct RuntimeRoutingChangeV1<'a> {
    pub column: RuntimeRoutingColumnV1,
    pub family: &'a ReservedValue<RuntimeSchedulerControlsQualificationV1>,
    pub qualification: &'a ReservedValue<RuntimeRoutingQualificationV1>,
    pub expected: RuntimeSchedulerControlValueRefV1<'a>,
    pub after: RuntimeSchedulerControlValueRefV1<'a>,
    pub max_pending_node_bytes: usize,
}
pub fn change_qualified_runtime_routing_column_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: RuntimeRoutingChangeV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<RuntimeRoutingQualificationV1>>, WorldFieldOverlayErrorV1> {
    let cancel = &mut ctx.cancelled;
    let error = &mut ctx.take_state_error;
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if !tx.uses_budget(write.memory) {
            return Err(invalid().into());
        }
        change.expected.validate(change.column.profile())?;
        change.after.validate(change.column.profile())?;
        let _admission = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let resolved = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut combined,
            };
            resolve(
                RuntimeRoutingRequestV1 {
                    column: change.column,
                    family: change.family,
                    qualification: change.qualification,
                },
                tx,
                &mut records,
                error,
            )?
        };
        if !change.expected.matches(resolved.value) {
            return Err(invalid().into());
        }
        let unchanged = change.expected == change.after;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if unchanged {
            return Ok(None);
        }
        let row = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            genesis::routing_cell(Some(change.column), change.after.payload(), &mut combined)?
        };
        if row.field_key != resolved.key.0 {
            return Err(invalid().into());
        }
        let child = record_tree::edit(
            resolved.child,
            &resolved.key.0,
            (Some(resolved.before), Some(row.value_root)),
            &mut ctx.values,
            write,
            cancel,
            RecordTreeWritePolicyV1 {
                max_pending_node_bytes: change.max_pending_node_bytes,
                labels: genesis::LABELS,
            },
        )?;
        let row = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancel() || (write.cancelled)(),
            };
            genesis::routing_root(child.clone(), &mut combined)?
        };
        if row.field_key != resolved.outer {
            return Err(invalid().into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            StateNamespaceV1::Scheduler,
            resolved.outer.as_bytes(),
            Some(row.value_root),
        )?;
        let proof = qualification::mint_routing(child, write.memory, MemoryClass::Ordinary)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(proof))
    })
}
