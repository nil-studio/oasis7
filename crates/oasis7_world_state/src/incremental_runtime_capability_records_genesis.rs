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

/// One continuous allowance across all three families, all child cells and outer rows.
pub struct RuntimeCapabilityRecordsWorkV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
}
impl RuntimeCapabilityRecordsWorkV1 {
    pub(super) fn spend(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining =
            self.cells_remaining
                .checked_sub(1)
                .ok_or(ValueTreeError::ResourceLimited(
                    "Runtime capability execution cells",
                ))?;
        Ok(())
    }
}
pub(super) const FORMAT_KEY: &str = "runtime_capability_execution_closure_v1";
pub(super) const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "Runtime capability execution read",
    pending: "Runtime capability execution pending",
    tree: "Runtime capability execution tree",
    digest: "Runtime capability execution digest",
    read_allowance: "Runtime capability execution read allowance",
    admission: "Runtime capability execution admission",
    bound: "Runtime capability execution bound",
    producer_pool: "Runtime capability execution pool",
    page: "Runtime capability execution page",
    write_allowance: "Runtime capability execution write allowance",
};
fn metadata<W: ValueNodeWriter, C: FnMut() -> bool>(
    kind: RuntimeCapabilityRecordKindV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<PresenceRecordV1, MemberRecordError> {
    write_field_metadata_v1(
        kind.descriptor(),
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
pub(super) fn body<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    value: RuntimeCapabilityRecordRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeCapabilityRecordsWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let _scratch = ctx
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let kind = value.kind();
    let mut root = StateRootV1::empty(kind.namespace())
        .map_err(|_| MemberRecordError::Invalid("Runtime capability execution namespace"))?;
    let policy = RecordTreeWritePolicyV1 {
        max_pending_node_bytes: work.max_pending_node_bytes,
        labels: LABELS,
    };
    work.spend()?;
    let meta = metadata(kind, ctx)?;
    root = incremental_record_tree::append(root, &meta.field_key, meta.value_root, ctx, policy)?;
    visit_runtime_capability_record_v1(value, |cell, payload| -> Result<(), MemberRecordError> {
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        work.spend()?;
        let row = write_cell(kind, cell, payload, ctx)?;
        root = incremental_record_tree::append(
            root.clone(),
            &row.field_key,
            row.value_root,
            ctx,
            policy,
        )?;
        Ok(())
    })?;
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    if root.leaf_count != kind.leaf_count() {
        return Err(MemberRecordError::Invalid(
            "Runtime capability exhaustive leaf count",
        ));
    }
    Ok(root)
}
pub(super) fn write_root<W: ValueNodeWriter, C: FnMut() -> bool>(
    key: RuntimeCapabilityRecordKeyV1<'_>,
    child: StateRootV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let _scratch = ctx
        .memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let semantic = crate::incremental_semantic_key::hash_semantic_key_v1(
        &key.id,
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let reference = RecordRootRefV1::new(key.kind.schema(), semantic.digest, child)
        .map_err(|_| MemberRecordError::Invalid("Runtime capability execution root"))?;
    write_member_record_v1(
        key.kind.descriptor(),
        WorldFieldRecordKindV1::Entry,
        &key.id,
        MemberPayloadRefV1::RecordRoot(&reference),
        ctx,
    )
}
/// Provisional output only. No activation receipt or business permission is issued.
pub fn prepare_runtime_capability_records_genesis_v1<
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &RuntimeCapabilityRecordsRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RuntimeCapabilityRecordsWorkV1,
    sink: &mut S,
) -> Result<u64, MemberRecordError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let _operation = ctx
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    for kind in RuntimeCapabilityRecordKindV1::ALL {
        work.spend()?;
        let meta = metadata(kind, ctx)?;
        crate::incremental_field_write::emit(meta.field_key, meta.value_root, ctx, sink)?;
        work.spend()?;
        let marker = write_member_record_v1(
            kind.descriptor(),
            WorldFieldRecordKindV1::Entry,
            &(FORMAT_KEY, ()),
            MemberPayloadRefV1::Unit,
            ctx,
        )?;
        crate::incremental_field_write::emit(marker.field_key, marker.value_root, ctx, sink)?;
    }
    let mut count = 0u64;
    let mut emit = |id, value: RuntimeCapabilityRecordRefV1<'_>| -> Result<(), MemberRecordError> {
        let kind = value.kind();
        let child = body(value, ctx, work)?;
        work.spend()?;
        let row = write_root(RuntimeCapabilityRecordKeyV1 { kind, id }, child, ctx)?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
        count = count.checked_add(1).ok_or(MemberRecordError::Invalid(
            "Runtime capability execution count",
        ))?;
        Ok(())
    };
    for (id, value) in source.nonces {
        emit(id.as_str(), RuntimeCapabilityRecordRefV1::Nonce(value))?;
    }
    for (id, value) in source.budgets {
        emit(id.as_str(), RuntimeCapabilityRecordRefV1::Budget(value))?;
    }
    for (id, value) in source.effect_links {
        emit(id.as_str(), RuntimeCapabilityRecordRefV1::EffectLink(value))?;
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(count)
}
