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
pub struct MainTokenPolicyWorkV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
}
impl MainTokenPolicyWorkV1 {
    pub(super) fn cell(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("main token policy cells"))?;
        Ok(())
    }
}
pub(super) const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "main token policy read",
    pending: "main token policy pending",
    tree: "main token policy tree",
    digest: "main token policy digest",
    read_allowance: "main token policy read allowance",
    admission: "main token policy admission",
    bound: "main token policy bound",
    producer_pool: "main token policy producer pool",
    page: "main token policy page",
    write_allowance: "main token policy write allowance",
};
pub(super) fn metadata<W: ValueNodeWriter, C: FnMut() -> bool>(
    kind: MainTokenPolicyKindV1,
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
    key: MainTokenPolicyKeyV1<'_>,
    child: StateRootV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let _scratch = ctx
        .memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let semantic =
        with_policy_key!(key,original=>crate::incremental_semantic_key::hash_semantic_key_v1(
            original,
            ctx.memory,
            MemoryClass::Ordinary,
            ctx.key_work,
            &mut ctx.cancelled,
        ))
        .map_err(MemberRecordError::Key)?;
    let reference = RecordRootRefV1::new(key.kind().schema(), semantic.digest, child)
        .map_err(|_| MemberRecordError::Invalid("main token policy root"))?;
    with_policy_key!(key,original=>write_member_record_v1(
        key.kind().descriptor()?, WorldFieldRecordKindV1::Entry, original, MemberPayloadRefV1::RecordRoot(&reference), ctx))
}
pub(super) fn body<W: ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
    key: MainTokenPolicyKeyV1<'_>,
    source: MainTokenPolicyRecordRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut MainTokenPolicyWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    if source.kind() != key.kind() {
        return Err(MemberRecordError::Invalid("main token policy source kind"));
    }
    let _scratch = ctx
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut child = StateRootV1::empty(key.kind().namespace())
        .map_err(|_| MemberRecordError::Invalid("main token policy namespace"))?;
    let policy = RecordTreeWritePolicyV1 {
        max_pending_node_bytes: work.max_pending_node_bytes,
        labels: LABELS,
    };
    work.cell()?;
    let row = metadata(key.kind(), ctx)?;
    child = incremental_record_tree::append(child, &row.field_key, row.value_root, ctx, policy)?;
    visit_main_token_policy_record_v1(source, |cell, payload| -> Result<(), MemberRecordError> {
        work.cell()?;
        let row = write_cell(key.kind(), cell, payload, ctx)?;
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
/// Complete provisional GEN; original outer keys and embedded identities are independent.
pub fn prepare_main_token_policy_genesis_v1<
    W: ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut MainTokenPolicyWorkV1,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let _operation = ctx
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    for kind in MainTokenPolicyKindV1::ALL {
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
    let mut emit = |key, value| -> Result<(), MemberRecordError> {
        let child = body(key, value, ctx, work)?;
        work.cell()?;
        let row = root(key, child, ctx)?;
        crate::incremental_field_write::emit(row.field_key, row.value_root, ctx, sink)?;
        Ok(())
    };
    emit(
        MainTokenPolicyKeyV1::Config,
        MainTokenPolicyRecordRefV1::Config(&source.main_token_config),
    )?;
    for (id, value) in &source.main_token_genesis_buckets {
        emit(
            MainTokenPolicyKeyV1::Bucket(id),
            MainTokenPolicyRecordRefV1::Bucket(value),
        )?;
    }
    for (epoch, value) in &source.main_token_epoch_issuance_records {
        emit(
            MainTokenPolicyKeyV1::Issuance(*epoch),
            MainTokenPolicyRecordRefV1::Issuance(value),
        )?;
    }
    for (id, value) in &source.restricted_starter_claim_grants {
        emit(
            MainTokenPolicyKeyV1::Grant(id),
            MainTokenPolicyRecordRefV1::Grant(value),
        )?;
    }
    for (id, value) in &source.main_token_scheduled_policy_updates {
        emit(
            MainTokenPolicyKeyV1::Scheduled(*id),
            MainTokenPolicyRecordRefV1::Scheduled(value),
        )?;
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
