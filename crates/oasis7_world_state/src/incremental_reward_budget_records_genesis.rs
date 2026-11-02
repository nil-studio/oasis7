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
pub struct RewardRecordsWorkV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
}
impl RewardRecordsWorkV1 {
    pub(super) fn cell(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("reward record cells"))?;
        Ok(())
    }
}
pub(super) const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "reward record read",
    pending: "reward record pending",
    tree: "reward record tree",
    digest: "reward record digest",
    read_allowance: "reward record read allowance",
    admission: "reward record admission",
    bound: "reward record bound",
    producer_pool: "reward record producer pool",
    page: "reward record page",
    write_allowance: "reward record write allowance",
};
pub(super) fn metadata<W: ValueNodeWriter, C: FnMut() -> bool>(
    kind: RewardRecordKindV1,
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
pub(super) fn root<W: ValueNodeWriter, C: FnMut() -> bool>(
    key: RewardRecordKeyV1,
    child: StateRootV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let _scratch = ctx
        .memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let semantic = crate::incremental_semantic_key::hash_semantic_key_v1(
        &key.index,
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let reference = RecordRootRefV1::new(key.kind.schema(), semantic.digest, child)
        .map_err(|_| MemberRecordError::Invalid("reward record root"))?;
    write_member_record_v1(
        key.kind.descriptor()?,
        WorldFieldRecordKindV1::Entry,
        &key.index,
        MemberPayloadRefV1::RecordRoot(&reference),
        ctx,
    )
}
pub(super) fn body<W: ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
    key: RewardRecordKeyV1,
    source: RewardRecordRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RewardRecordsWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    if source.kind() != key.kind {
        return Err(MemberRecordError::Invalid("reward record source kind"));
    }
    let _scratch = ctx
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut child = StateRootV1::empty(key.kind.namespace())
        .map_err(|_| MemberRecordError::Invalid("reward record namespace"))?;
    let policy = RecordTreeWritePolicyV1 {
        max_pending_node_bytes: work.max_pending_node_bytes,
        labels: LABELS,
    };
    work.cell()?;
    let row = metadata(key.kind, ctx)?;
    child = incremental_record_tree::append(child, &row.field_key, row.value_root, ctx, policy)?;
    visit_reward_record_v1(source, |cell, payload| -> Result<(), MemberRecordError> {
        work.cell()?;
        let row = write_cell(key, cell, payload, ctx)?;
        child = incremental_record_tree::append(
            child.clone(),
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
    Ok(child)
}
/// Complete provisional GEN; Vec ordinal and original embedded epoch are independent.
pub fn prepare_reward_records_genesis_v1<
    W: ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut RewardRecordsWorkV1,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let _operation = ctx
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    for kind in RewardRecordKindV1::ALL {
        work.cell()?;
        let row = metadata(kind, ctx)?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
        work.cell()?;
        let row = write_member_record_v1(
            kind.descriptor()?,
            WorldFieldRecordKindV1::Entry,
            &(FORMAT, ()),
            MemberPayloadRefV1::Unit,
            ctx,
        )?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
    }
    let length = u64::try_from(source.reward_mint_records.len())
        .map_err(|_| MemberRecordError::Invalid("reward mint length"))?;
    work.cell()?;
    let row = write_member_record_v1(
        RewardRecordKindV1::Mint.descriptor()?,
        WorldFieldRecordKindV1::Entry,
        &(FORMAT, "length"),
        MemberPayloadRefV1::U64(length),
        ctx,
    )?;
    crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
    let mut emit = |key, value| -> Result<(), MemberRecordError> {
        let child = body(key, value, ctx, work)?;
        work.cell()?;
        let row = root(key, child, ctx)?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
        Ok(())
    };
    for (epoch, value) in &source.system_order_pool_budgets {
        emit(
            RewardRecordKeyV1 {
                kind: RewardRecordKindV1::Pool,
                index: *epoch,
            },
            RewardRecordRefV1::Pool(value),
        )?;
    }
    for (index, value) in source.reward_mint_records.iter().enumerate() {
        emit(
            RewardRecordKeyV1 {
                kind: RewardRecordKindV1::Mint,
                index: index as u64,
            },
            RewardRecordRefV1::Mint(value),
        )?;
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
