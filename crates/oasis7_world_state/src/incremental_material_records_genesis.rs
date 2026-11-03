//! Borrowed two-family genesis with typed per-record StateRoot edges.
use super::*;
use crate::{WorldState, incremental_field_presence::*, incremental_record_tree};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_record_root::RecordRootRefV1,
    incremental_state::{StatePageWriter, StateRootV1, StateScanBudget},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
/// Continuous caller-owned logical work; heap admission is separately retained.
pub struct MaterialWorkBudgetV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
    pub scan: StateScanBudget,
}
impl MaterialWorkBudgetV1 {
    pub(super) fn cell(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("material cell work"))?;
        Ok(())
    }
}
pub(super) const LABELS: incremental_record_tree::RecordTreeLabelsV1 =
    incremental_record_tree::RecordTreeLabelsV1 {
        read: "material child state read",
        pending: "material pending node allowance",
        tree: "material child state tree",
        digest: "material state node digest",
        read_allowance: "material child read allowance",
        admission: "material child source admission",
        bound: "material child source bound",
        producer_pool: "material child producer pool",
        page: "material child page",
        write_allowance: "material child write allowance",
    };
fn spend(records: &mut u64, work: &mut MaterialWorkBudgetV1) -> Result<(), ValueTreeError> {
    work.cell()?;
    *records = records
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("material records"))?;
    Ok(())
}
pub(super) fn metadata<W: ValueNodeWriter, C: FnMut() -> bool>(
    key: MaterialRecordKeyV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<PresenceRecordV1, MemberRecordError> {
    write_field_metadata_v1(
        key.kind().descriptor()?,
        &mut PresenceRecordWriteContext {
            sink: &mut *context.sink,
            memory: context.memory,
            io: &mut *context.io,
            key_work: &mut *context.key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: context.limits.semantic_bytes,
                record_bytes: context.limits.record_bytes,
            },
            cancelled: &mut context.cancelled,
        },
    )
}
pub(super) fn write_root<W: ValueNodeWriter, C: FnMut() -> bool>(
    key: MaterialRecordKeyV1<'_>,
    child: StateRootV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let _scratch = context
        .memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let semantic = with_material_outer_key!(key, encoded =>
        crate::incremental_semantic_key::hash_semantic_key_v1(
            encoded, context.memory, MemoryClass::Ordinary, context.key_work, &mut context.cancelled,
        ).map_err(MemberRecordError::Key)?
    );
    let reference = RecordRootRefV1::new(key.kind().schema(), semantic.digest, child)
        .map_err(|_| MemberRecordError::Invalid("material record root"))?;
    with_material_outer_key!(key, encoded => write_member_record_v1(
        key.kind().descriptor()?, WorldFieldRecordKindV1::Entry,
        encoded, MemberPayloadRefV1::RecordRoot(&reference), context,
    ))
}
pub(super) fn body<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    key: MaterialRecordKeyV1<'_>,
    value: MaterialRecordRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    work: &mut MaterialWorkBudgetV1,
) -> Result<StateRootV1, MemberRecordError> {
    if value.kind() != key.kind() {
        return Err(MemberRecordError::Invalid("material record kind"));
    }
    let _scratch = context
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut root = StateRootV1::empty(key.kind().namespace())
        .map_err(|_| MemberRecordError::Invalid("material child namespace"))?;
    spend(records, work)?;
    let meta = metadata(key, context)?;
    root = incremental_record_tree::append(
        root,
        &meta.field_key,
        meta.value_root,
        context,
        incremental_record_tree::RecordTreeWritePolicyV1 {
            max_pending_node_bytes: work.max_pending_node_bytes,
            labels: LABELS,
        },
    )?;
    visit_material_record_v1(value, |cell, payload| -> Result<(), MemberRecordError> {
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        spend(records, work)?;
        let record = write_cell(key, cell, payload, context)?;
        root = incremental_record_tree::append(
            root.clone(),
            &record.field_key,
            record.value_root,
            context,
            incremental_record_tree::RecordTreeWritePolicyV1 {
                max_pending_node_bytes: work.max_pending_node_bytes,
                labels: LABELS,
            },
        )?;
        Ok(())
    })?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(root)
}
/// Emit metadata/profile markers and both original keyed record families. Output
/// is provisional until the caller authenticates and accepts its whole write-set.
pub fn prepare_material_records_genesis_v1<
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    work: &mut MaterialWorkBudgetV1,
    sink: &mut S,
) -> Result<u64, MemberRecordError> {
    for kind in MaterialRecordKindV1::ALL {
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let key = match kind {
            MaterialRecordKindV1::Profile => MaterialRecordKeyV1::Profile(""),
            MaterialRecordKindV1::Transfer => MaterialRecordKeyV1::Transfer(0),
        };
        spend(records, work)?;
        let meta = metadata(key, context)?;
        crate::incremental_field_write::emit(meta.field_key, meta.value_root, context, sink)?;
        spend(records, work)?;
        let marker = write_member_record_v1(
            key.kind().descriptor()?,
            WorldFieldRecordKindV1::Entry,
            &(RECORD_FORMAT_KEY, ()),
            MemberPayloadRefV1::Unit,
            context,
        )?;
        crate::incremental_field_write::emit(marker.field_key, marker.value_root, context, sink)?;
    }
    let mut count = 0u64;
    let mut emit_record = |key: MaterialRecordKeyV1<'_>, value| -> Result<(), MemberRecordError> {
        let child = body(key, value, context, records, work)?;
        spend(records, work)?;
        let record = write_root(key, child, context)?;
        crate::incremental_field_write::emit(record.field_key, record.value_root, context, sink)?;
        count = count
            .checked_add(1)
            .ok_or(MemberRecordError::Invalid("material record count"))?;
        Ok(())
    };
    for (id, value) in &source.material_profiles {
        emit_record(
            MaterialRecordKeyV1::Profile(id),
            MaterialRecordRefV1::Profile(value),
        )?;
    }
    for (&id, value) in &source.direct_material_transfer_receipts {
        emit_record(
            MaterialRecordKeyV1::Transfer(id),
            MaterialRecordRefV1::Transfer(value),
        )?;
    }
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(count)
}
