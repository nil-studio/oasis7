//! Borrowed three-family genesis with typed per-record StateRoot edges.
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
pub struct AgentAuthorityWorkBudgetV1 {
    pub cells_remaining: u64,
    pub max_pending_node_bytes: usize,
    pub scan: StateScanBudget,
}
impl AgentAuthorityWorkBudgetV1 {
    pub(super) fn cell(&mut self) -> Result<(), ValueTreeError> {
        self.cells_remaining = self
            .cells_remaining
            .checked_sub(1)
            .ok_or(ValueTreeError::ResourceLimited("agent_authority cell work"))?;
        Ok(())
    }
}
pub(super) const LABELS: incremental_record_tree::RecordTreeLabelsV1 =
    incremental_record_tree::RecordTreeLabelsV1 {
        read: "agent_authority child state read",
        pending: "agent_authority pending node allowance",
        tree: "agent_authority child state tree",
        digest: "agent_authority state node digest",
        read_allowance: "agent_authority child read allowance",
        admission: "agent_authority child source admission",
        bound: "agent_authority child source bound",
        producer_pool: "agent_authority child producer pool",
        page: "agent_authority child page",
        write_allowance: "agent_authority child write allowance",
    };
fn spend(records: &mut u64, work: &mut AgentAuthorityWorkBudgetV1) -> Result<(), ValueTreeError> {
    work.cell()?;
    *records = records
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("agent_authority records"))?;
    Ok(())
}
pub(super) fn metadata<W: ValueNodeWriter, C: FnMut() -> bool>(
    key: AgentAuthorityRecordKeyV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<PresenceRecordV1, MemberRecordError> {
    write_field_metadata_v1(
        key.descriptor()?,
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
    key: AgentAuthorityRecordKeyV1,
    child: StateRootV1,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let _scratch = context
        .memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let semantic = crate::incremental_semantic_key::hash_semantic_key_v1(
        &key.id,
        context.memory,
        MemoryClass::Ordinary,
        context.key_work,
        &mut context.cancelled,
    )
    .map_err(MemberRecordError::Key)?;
    let reference = RecordRootRefV1::new(key.kind.schema(), semantic.digest, child)
        .map_err(|_| MemberRecordError::Invalid("agent_authority record root"))?;
    write_member_record_v1(
        key.descriptor()?,
        WorldFieldRecordKindV1::Entry,
        &key.id,
        MemberPayloadRefV1::RecordRoot(&reference),
        context,
    )
}
pub(super) fn body<W: StatePageWriter + ValueNodeReader, C: FnMut() -> bool>(
    key: AgentAuthorityRecordKeyV1,
    value: AgentAuthorityRecordRefV1<'_>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    work: &mut AgentAuthorityWorkBudgetV1,
) -> Result<StateRootV1, MemberRecordError> {
    if value.kind() != key.kind {
        return Err(MemberRecordError::Invalid("agent_authority record kind"));
    }
    let _scratch = context
        .memory
        .reserve(16 << 10, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let mut root = StateRootV1::empty(key.namespace())
        .map_err(|_| MemberRecordError::Invalid("agent_authority child namespace"))?;
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
    visit_agent_authority_record_v1(value, |cell, payload| -> Result<(), MemberRecordError> {
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
/// Emit metadata/profile markers and three original keyed record families. Output
/// is provisional until the caller authenticates and accepts its whole write-set.
pub fn prepare_agent_authority_records_genesis_v1<
    W: StatePageWriter + ValueNodeReader,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    work: &mut AgentAuthorityWorkBudgetV1,
    sink: &mut S,
) -> Result<u64, MemberRecordError> {
    for kind in AgentAuthorityRecordKindV1::ALL {
        if (context.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let key = AgentAuthorityRecordKeyV1 { kind, id: "" };
        spend(records, work)?;
        let meta = metadata(key, context)?;
        crate::incremental_field_write::emit(meta.field_key, meta.value_root, context, sink)?;
        spend(records, work)?;
        let marker = write_member_record_v1(
            key.descriptor()?,
            WorldFieldRecordKindV1::Entry,
            &(RECORD_FORMAT_KEY, ()),
            MemberPayloadRefV1::Unit,
            context,
        )?;
        crate::incremental_field_write::emit(marker.field_key, marker.value_root, context, sink)?;
    }
    let mut count = 0u64;
    let mut emit_record = |kind, id, value| -> Result<(), MemberRecordError> {
        let key = AgentAuthorityRecordKeyV1 { kind, id };
        let child = body(key, value, context, records, work)?;
        spend(records, work)?;
        let record = write_root(key, child, context)?;
        crate::incremental_field_write::emit(record.field_key, record.value_root, context, sink)?;
        count = count
            .checked_add(1)
            .ok_or(MemberRecordError::Invalid("agent_authority record count"))?;
        Ok(())
    };
    for (id, value) in &source.agent_intent_ledger {
        emit_record(
            AgentAuthorityRecordKindV1::IntentLedger,
            id.as_str(),
            AgentAuthorityRecordRefV1::IntentLedger(value),
        )?;
    }
    for (id, value) in &source.agent_location_authorities {
        emit_record(
            AgentAuthorityRecordKindV1::AgentLocation,
            id.as_str(),
            AgentAuthorityRecordRefV1::AgentLocation(value),
        )?;
    }
    for (id, value) in &source.location_anchors {
        emit_record(
            AgentAuthorityRecordKindV1::LocationAnchor,
            id.as_str(),
            AgentAuthorityRecordRefV1::LocationAnchor(value),
        )?;
    }
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(count)
}
