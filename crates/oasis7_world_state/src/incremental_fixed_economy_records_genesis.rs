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
pub struct FixedEconomyWorkV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
}
impl FixedEconomyWorkV1 {
    pub(super) fn cell(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("fixed economy cells"))?;
        Ok(())
    }
}
pub(super) const LABELS: RecordTreeLabelsV1 = RecordTreeLabelsV1 {
    read: "fixed economy tree read",
    pending: "fixed economy pending",
    tree: "fixed economy tree",
    digest: "fixed economy digest",
    read_allowance: "fixed economy tree allowance",
    admission: "fixed economy page allocation",
    bound: "fixed economy page bound",
    producer_pool: "fixed economy producer pool",
    page: "fixed economy page",
    write_allowance: "fixed economy page allowance",
};
pub(super) fn metadata<W: ValueNodeWriter, C: FnMut() -> bool>(
    field: FixedEconomyFieldV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<PresenceRecordV1, MemberRecordError> {
    write_field_metadata_v1(
        field.descriptor()?,
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
pub(super) fn write_root<W: ValueNodeWriter, C: FnMut() -> bool>(
    field: FixedEconomyFieldV1,
    root: StateRootV1,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let _scratch = ctx
        .memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let key = crate::incremental_semantic_key::hash_semantic_key_v1(
        &(),
        ctx.memory,
        MemoryClass::Ordinary,
        ctx.key_work,
        &mut ctx.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let reference = RecordRootRefV1::new(field.schema(), key.digest, root)
        .map_err(|_| MemberRecordError::Invalid("fixed economy root"))?;
    write_member_record_v1(
        field.descriptor()?,
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::RecordRoot(&reference),
        ctx,
    )
}
fn body<W: ValueNodeReader + StatePageWriter, C: FnMut() -> bool>(
    source: FixedEconomyRecordRefV1<'_>,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut FixedEconomyWorkV1,
) -> Result<StateRootV1, MemberRecordError> {
    let field = source.field();
    let mut root = StateRootV1::empty(field.namespace())
        .map_err(|_| MemberRecordError::Invalid("fixed economy namespace"))?;
    let policy = RecordTreeWritePolicyV1 {
        max_pending_node_bytes: work.max_pending_node_bytes,
        labels: LABELS,
    };
    work.cell()?;
    let meta = metadata(field, ctx)?;
    root = incremental_record_tree::append(root, &meta.field_key, meta.value_root, ctx, policy)?;
    work.cell()?;
    let marker = write_cell(field, None, MemberPayloadRefV1::Unit, ctx)?;
    root =
        incremental_record_tree::append(root, &marker.field_key, marker.value_root, ctx, policy)?;
    visit_fixed_economy_record_v1(source, |column, payload| -> Result<(), MemberRecordError> {
        work.cell()?;
        let cell = write_cell(field, Some(column), payload, ctx)?;
        root = incremental_record_tree::append(
            root.clone(),
            &cell.field_key,
            cell.value_root,
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
/// Provisional borrowed production of all four mandatory global values.
pub fn prepare_fixed_economy_genesis_v1<
    W: ValueNodeReader + StatePageWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    ctx: &mut MemberRecordWriteContext<'_, W, C>,
    work: &mut FixedEconomyWorkV1,
    sink: &mut S,
) -> Result<(), MemberRecordError> {
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    // Retain bounded bookkeeping admission across returned child roots/rows.
    let _operation = ctx
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    for value in [
        FixedEconomyRecordRefV1::Supply(&source.main_token_supply),
        FixedEconomyRecordRefV1::AssetConfig(&source.reward_asset_config),
        FixedEconomyRecordRefV1::PowerReserve(&source.protocol_power_reserve),
        FixedEconomyRecordRefV1::SignaturePolicy(&source.reward_signature_governance_policy),
    ] {
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let field = value.field();
        work.cell()?;
        let meta = metadata(field, ctx)?;
        crate::incremental_field_write::emit(meta.field_key, meta.value_root, ctx, sink)?;
        let root = body(value, ctx, work)?;
        work.cell()?;
        let reference = write_root(field, root, ctx)?;
        crate::incremental_field_write::emit(reference.field_key, reference.value_root, ctx, sink)?;
    }
    if (ctx.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(())
}
