use super::*;
use crate::{
    incremental_field_presence::*,
    incremental_record_tree::{self, RecordTreeLabelsV1, RecordTreeWritePolicyV1},
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_record_root::RecordRootRefV1,
    incremental_state::{StatePageWriter, StateRootV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
pub struct RuntimeSchedulerControlsWorkV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
}
impl RuntimeSchedulerControlsWorkV1 {
    pub(super) fn cell(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining =
            self.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "runtime scheduler control cells",
                ))?;
        Ok(())
    }
}
pub(super) const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "runtime controls read",
    pending: "runtime controls pending",
    tree: "runtime controls tree",
    digest: "runtime controls digest",
    read_allowance: "runtime controls read allowance",
    admission: "runtime controls admission",
    bound: "runtime controls bound",
    producer_pool: "runtime controls pool",
    page: "runtime controls page",
    write_allowance: "runtime controls write allowance",
};
pub(super) fn metadata<W: ValueNodeWriter, C: FnMut() -> bool>(
    name: &'static str,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<PresenceRecordV1, MemberRecordError> {
    write_field_metadata_v1(
        descriptor(name),
        &mut PresenceRecordWriteContext {
            sink: &mut *ctx.sink,
            memory: ctx.memory,
            io: &mut *ctx.io,
            key_work: &mut *ctx.key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: ctx.limits.semantic_bytes,
                record_bytes: ctx.limits.record_bytes,
            },
            cancelled: &mut ctx.cancelled,
        },
    )
}
pub(super) fn routing_cell<W: ValueNodeWriter, C: FnMut() -> bool>(
    cell: Option<RuntimeRoutingColumnV1>,
    payload: MemberPayloadRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    match cell {
        None => write_member_record_v1(
            descriptor(ROUTING),
            WorldFieldRecordKindV1::Entry,
            &(),
            payload,
            ctx,
        ),
        Some(cell) => write_member_record_v1(
            descriptor(ROUTING),
            WorldFieldRecordKindV1::Entry,
            &((), cell.name()),
            payload,
            ctx,
        ),
    }
}
pub(super) fn routing_root<W: ValueNodeWriter, C: FnMut() -> bool>(
    child: StateRootV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let _admission = ctx
        .memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let owner = crate::incremental_semantic_key::hash_semantic_key_v1(
        &(),
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let root = RecordRootRefV1::new(routing_schema(), owner.digest, child)
        .map_err(|_| MemberRecordError::Invalid("runtime routing root"))?;
    write_member_record_v1(
        descriptor(ROUTING),
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::RecordRoot(&root),
        ctx,
    )
}
/// Emits precisely this eight-descriptor group, never shadow allocator/module-wake rows.
pub fn prepare_runtime_scheduler_controls_genesis_v1<
    W: ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &RuntimeSchedulerControlsRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeSchedulerControlsWorkV1,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let _operation = ctx
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    visit_runtime_scheduler_scalars_v1(
        source,
        |field, payload| -> Result<(), MemberRecordError> {
            work.cell()?;
            let row = metadata(field.name(), ctx)?;
            crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
            work.cell()?;
            let row = write_member_record_v1(
                descriptor(field.name()),
                WorldFieldRecordKindV1::Entry,
                &(),
                payload,
                ctx,
            )?;
            crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)
        },
    )?;
    work.cell()?;
    let row = metadata(ROUTING, ctx)?;
    crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
    let mut child = StateRootV1::empty(StateNamespaceV1::Scheduler)
        .map_err(|_| MemberRecordError::Invalid("runtime routing namespace"))?;
    let policy = RecordTreeWritePolicyV1 {
        max_pending_node_bytes: work.max_pending_node_bytes,
        labels: LABELS,
    };
    work.cell()?;
    let row = metadata(ROUTING, ctx)?;
    child = incremental_record_tree::append(child, &row.field_key, row.value_root, ctx, policy)?;
    work.cell()?;
    let row = routing_cell(None, MemberPayloadRefV1::Unit, ctx)?;
    child = incremental_record_tree::append(child, &row.field_key, row.value_root, ctx, policy)?;
    visit_runtime_routing_columns_v1(
        source.routing,
        |cell, payload| -> Result<(), MemberRecordError> {
            work.cell()?;
            let row = routing_cell(Some(cell), payload, ctx)?;
            child = incremental_record_tree::append(
                child.clone(),
                &row.field_key,
                row.value_root,
                ctx,
                policy,
            )?;
            Ok(())
        },
    )?;
    work.cell()?;
    let row = routing_root(child, ctx)?;
    crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
