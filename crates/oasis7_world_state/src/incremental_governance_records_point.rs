//! Qualified one-column queries and edits over one shared public resolver.
use super::*;
use crate::{
    incremental_expected_bytes::Compare,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter, StateRootV1},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("agent authority point expected/profile/binding")
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernanceValueRefV1<'a> {
    Unit,
    U64(u64),
    I64(i64),
    Text(&'a str),
}
impl<'a> GovernanceValueRefV1<'a> {
    fn text(self) -> &'a str {
        match self {
            Self::Text(v) => v,
            _ => "",
        }
    }
    fn payload(self) -> MemberPayloadRefV1<'a> {
        match self {
            Self::Unit => MemberPayloadRefV1::Unit,
            Self::U64(v) => MemberPayloadRefV1::U64(v),
            Self::I64(v) => MemberPayloadRefV1::I64(v),
            Self::Text(v) => MemberPayloadRefV1::Text(v),
        }
    }
    fn validate(
        self,
        kind: GovernanceRecordKindV1,
        cell: GovernanceCellV1<'_>,
    ) -> Result<(), ValueTreeError> {
        let valid = cell.accepts(kind)
            && match self {
                Self::Unit => cell.nullable(),
                Self::U64(v) => {
                    cell.codec() == MemberPayloadCodecV1::U64 && v <= cell.integer_max()
                }
                Self::I64(_) => cell.codec() == MemberPayloadCodecV1::I64,
                Self::Text(_) => cell.codec() == MemberPayloadCodecV1::Text,
            };
        if valid { Ok(()) } else { Err(invalid()) }
    }
    fn matches(self, value: GovernanceValueV1) -> bool {
        match (self, value) {
            (Self::Unit, GovernanceValueV1::Unit) => true,
            (Self::U64(a), GovernanceValueV1::Unsigned(b)) => a == b,
            (Self::I64(a), GovernanceValueV1::Signed(b)) => a == b,
            (Self::Text(v), GovernanceValueV1::Text { utf8_bytes }) => v.len() as u64 == utf8_bytes,
            _ => false,
        }
    }
}
pub(super) fn current<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: GovernanceRecordKeyV1<'_>,
    proof: &ReservedValue<GovernanceRecordQualificationV1>,
    tx: &mut oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<(WorldFieldKeyV1, StateRootV1), WorldFieldOverlayErrorV1> {
    if records.values.class != MemoryClass::Ordinary
        || !tx.uses_budget(records.values.memory)
        || !proof.uses_budget(records.values.memory)
    {
        return Err(invalid().into());
    }
    metadata(key, tx, records, error)?;
    let outer = read::outer_key(key, records)?;
    let value = lookup(tx, &outer.0, records, error)?.ok_or_else(invalid)?;
    let child = read::outer(key.kind, &outer, &value, records)?;
    let owner = WorldFieldKeyV1::from_bytes(outer.0.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(invalid)?;
    proof.validate_binding(key.kind, &owner, &child)?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok((outer.0, child))
}
pub(super) fn metadata<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: GovernanceRecordKeyV1<'_>,
    tx: &mut oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let field = key.kind.descriptor()?;
    let meta = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let value = lookup(tx, &meta, records, error)?.ok_or_else(invalid)?;
    records.presence(&value, &meta, 0)?;
    let marker = records.key(
        field,
        WorldFieldRecordKindV1::Entry,
        &(RECORD_FORMAT_KEY, ()),
    )?;
    let value = lookup(tx, &marker.0, records, error)?.ok_or_else(invalid)?;
    read::value(
        GovernanceCellV1::Record,
        &value,
        &marker,
        records,
        0,
        &mut std::io::sink(),
    )?;
    Ok(())
}

pub(super) struct Resolved {
    pub outer: WorldFieldKeyV1,
    pub child: StateRootV1,
    pub key: (WorldFieldKeyV1, u64),
    pub before: Option<[u8; 32]>,
    pub value: Option<GovernanceValueV1>,
}
pub(super) fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: GovernanceCellRequestV1<'_>,
    tx: &mut oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
    max: u64,
    output: &mut impl std::io::Write,
) -> Result<Resolved, WorldFieldOverlayErrorV1> {
    if !request.cell.accepts(request.key.kind) {
        return Err(invalid().into());
    }
    let (outer, child) = current(request.key, request.qualification, tx, records, error)?;
    let key = read::cell_key(request.key.kind, request.cell, records)?;
    let before = crate::incremental_record_tree::lookup(&child, &key.0, records, genesis::LABELS)?;
    let value = match before {
        Some(root) => Some(read::value(
            request.cell,
            &root,
            &key,
            records,
            max,
            output,
        )?),
        None if request.cell.is_member() => None,
        None => return Err(invalid().into()),
    };
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(Resolved {
        outer,
        child,
        key,
        before,
        value,
    })
}
/// Exact same-TX query; outputs remain provisional until its final shared check.
/// Failure poisons any already successful contribution in this transaction.
pub fn read_qualified_governance_cell_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: GovernanceCellRequestV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max: u64,
    output: &mut impl std::io::Write,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<GovernanceValueV1>, WorldFieldOverlayErrorV1> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !request.qualification.uses_budget(context.values.memory)
        {
            return Err(invalid().into());
        }
        let _workspace = context
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: &mut *context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: cancel,
        };
        Ok(resolve(request, tx, &mut records, error, max, output)?.value)
    })
}
pub struct GovernanceCellChangeV1<'a> {
    pub key: GovernanceRecordKeyV1<'a>,
    pub cell: GovernanceCellV1<'a>,
    pub qualification: &'a ReservedValue<GovernanceRecordQualificationV1>,
    pub expected: Option<GovernanceValueRefV1<'a>>,
    pub after: Option<GovernanceValueRefV1<'a>>,
    pub max_current_payload_bytes: u64,
    pub max_pending_node_bytes: usize,
}
/// Existing scalar/member value edits; structural membership has a separate operation.
/// Nullable original fields encode None as Some(Unit), distinct from absent members.
pub fn change_qualified_governance_cell_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: GovernanceCellChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<GovernanceRecordQualificationV1>>, WorldFieldOverlayErrorV1> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if matches!(
            change.cell,
            GovernanceCellV1::Record | GovernanceCellV1::Count(_)
        ) || !tx.uses_budget(write.memory)
            || context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !change.qualification.uses_budget(context.values.memory)
        {
            return Err(invalid().into());
        }
        if !change.cell.accepts(change.key.kind)
            || change.expected.is_some() != change.after.is_some()
            || (!change.cell.is_member() && change.expected.is_none())
        {
            return Err(invalid().into());
        }
        for value in [change.expected, change.after].into_iter().flatten() {
            value.validate(change.key.kind, change.cell)?;
        }
        let _operation = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let resolved = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            let mut compare = Compare::new(
                change
                    .expected
                    .map_or("", GovernanceValueRefV1::text)
                    .as_bytes(),
            );
            let resolved = resolve(
                GovernanceCellRequestV1 {
                    key: change.key,
                    cell: change.cell,
                    qualification: change.qualification,
                },
                tx,
                &mut records,
                error,
                change.max_current_payload_bytes,
                &mut compare,
            )?;
            let matches = match (change.expected, resolved.value) {
                (None, None) => true,
                (Some(expected), Some(actual)) => expected.matches(actual),
                _ => false,
            };
            if !matches || !compare.is_equal() {
                return Err(invalid().into());
            }
            resolved
        };
        let unchanged = change.expected == change.after;
        // Distinct N+1 after the public shared resolver and every equality comparison.
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
            write_cell(
                change.key,
                change.cell,
                change.after.ok_or_else(invalid)?.payload(),
                &mut combined,
            )?
        };
        if row.field_key != resolved.key.0 {
            return Err(invalid().into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let child = crate::incremental_record_tree::edit(
            resolved.child,
            &resolved.key.0,
            (resolved.before, Some(row.value_root)),
            &mut context.values,
            write,
            cancel,
            crate::incremental_record_tree::RecordTreeWritePolicyV1 {
                max_pending_node_bytes: change.max_pending_node_bytes,
                labels: genesis::LABELS,
            },
        )?;
        Ok(Some(write::publish(
            change.key,
            child,
            resolved.outer,
            tx,
            write,
            cancel,
        )?))
    })
}
