use super::*;
use crate::{
    WorldState,
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

/// One continuous allowance across both families, all child cells and outer rows.
pub struct FactoryAuthorityWorkV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
}
impl FactoryAuthorityWorkV1 {
    pub(super) fn spend(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("factory authority cells"))?;
        Ok(())
    }
}
pub(super) const FORMAT_KEY: &str = "factory_authority_closure_v1";
pub(super) const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "factory authority read",
    pending: "factory authority pending",
    tree: "factory authority tree",
    digest: "factory authority digest",
    read_allowance: "factory authority read allowance",
    admission: "factory authority admission",
    bound: "factory authority bound",
    producer_pool: "factory authority pool",
    page: "factory authority page",
    write_allowance: "factory authority write allowance",
};
fn metadata<W: ValueNodeWriter, C: FnMut() -> bool>(
    kind: FactoryAuthorityRecordKindV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<PresenceRecordV1, MemberRecordError> {
    write_field_metadata_v1(
        kind.descriptor()?,
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
    value: FactoryAuthorityRecordRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut FactoryAuthorityWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let _scratch = ctx
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let kind = value.kind();
    let mut root = StateRootV1::empty(kind.namespace())
        .map_err(|_| MemberRecordError::Invalid("factory authority namespace"))?;
    let policy = RecordTreeWritePolicyV1 {
        max_pending_node_bytes: work.max_pending_node_bytes,
        labels: LABELS,
    };
    work.spend()?;
    let meta = metadata(kind, ctx)?;
    root = incremental_record_tree::append(root, &meta.field_key, meta.value_root, ctx, policy)?;
    visit_factory_authority_record_v1(value, |cell, payload| -> Result<(), MemberRecordError> {
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
    Ok(root)
}
pub(super) fn write_root<W: ValueNodeWriter, C: FnMut() -> bool>(
    key: FactoryAuthorityRecordKeyV1<'_>,
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
        .map_err(|_| MemberRecordError::Invalid("factory authority root"))?;
    write_member_record_v1(
        key.kind.descriptor()?,
        WorldFieldRecordKindV1::Entry,
        &key.id,
        MemberPayloadRefV1::RecordRoot(&reference),
        ctx,
    )
}
/// Provisional output only. No activation receipt or business permission is issued.
pub fn prepare_factory_authority_genesis_v1<
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut FactoryAuthorityWorkV1,
    sink: &mut S,
) -> Result<u64, MemberRecordError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let _operation = ctx
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    for kind in FactoryAuthorityRecordKindV1::ALL {
        work.spend()?;
        let meta = metadata(kind, ctx)?;
        crate::incremental_field_write::emit(meta.field_key, meta.value_root, ctx, sink)?;
        work.spend()?;
        let marker = write_member_record_v1(
            kind.descriptor()?,
            WorldFieldRecordKindV1::Entry,
            &(FORMAT_KEY, ()),
            MemberPayloadRefV1::Unit,
            ctx,
        )?;
        crate::incremental_field_write::emit(marker.field_key, marker.value_root, ctx, sink)?;
    }
    let mut count = 0u64;
    let mut emit = |id, value: FactoryAuthorityRecordRefV1<'_>| -> Result<(), MemberRecordError> {
        let kind = value.kind();
        let child = body(value, ctx, work)?;
        work.spend()?;
        let row = write_root(FactoryAuthorityRecordKeyV1 { kind, id }, child, ctx)?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
        count = count
            .checked_add(1)
            .ok_or(MemberRecordError::Invalid("factory authority count"))?;
        Ok(())
    };
    for (id, value) in &source.factory_site_authorities {
        emit(id.as_str(), FactoryAuthorityRecordRefV1::Site(value))?;
    }
    for (id, value) in &source.factory_construction_power_profiles {
        emit(id.as_str(), FactoryAuthorityRecordRefV1::Power(value))?;
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(count)
}
