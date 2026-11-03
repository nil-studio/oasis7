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
    ValueTreeError::Invalid("main token policy point expected/profile/binding")
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainTokenPolicyValueRefV1<'a> {
    Unit,
    U64(u64),
    Text(&'a str),
}
impl<'a> MainTokenPolicyValueRefV1<'a> {
    fn payload(self) -> MemberPayloadRefV1<'a> {
        match self {
            Self::Unit => MemberPayloadRefV1::Unit,
            Self::U64(v) => MemberPayloadRefV1::U64(v),
            Self::Text(v) => MemberPayloadRefV1::Text(v),
        }
    }
    fn validate(
        self,
        kind: MainTokenPolicyKindV1,
        cell: MainTokenPolicyCellV1,
    ) -> Result<(), ValueTreeError> {
        if cell.accepts(kind, self.payload()) {
            Ok(())
        } else {
            Err(invalid())
        }
    }

    fn text(self) -> &'a str {
        if let Self::Text(v) = self { v } else { "" }
    }
    fn matches(self, value: MainTokenPolicyValueV1) -> bool {
        match (self, value) {
            (Self::Unit, MainTokenPolicyValueV1::Unit) => true,
            (Self::U64(a), MainTokenPolicyValueV1::Unsigned(b)) => a == b,
            (Self::Text(v), MainTokenPolicyValueV1::Text { utf8_bytes }) => {
                v.len() as u64 == utf8_bytes
            }
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
    key: MainTokenPolicyKeyV1<'_>,
    proof: &ReservedValue<MainTokenPolicyRecordQualificationV1>,
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
    let child = read::outer(key.kind(), &outer, &value, records)?;
    let owner = WorldFieldKeyV1::from_bytes(outer.0.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(invalid)?;
    proof.validate_binding(key.kind(), &owner, &child)?;
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
    key: MainTokenPolicyKeyV1<'_>,
    tx: &mut oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let field = key.kind().descriptor()?;
    let meta = WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Metadata, None)
        .map_err(ValueTreeError::Invalid)?;
    let value = lookup(tx, &meta, records, error)?.ok_or_else(invalid)?;
    records.presence(&value, &meta, 0)?;
    let marker = records.key(field, WorldFieldRecordKindV1::Entry, &(FORMAT, ()))?;
    let value = lookup(tx, &marker.0, records, error)?.ok_or_else(invalid)?;
    read::value(
        MainTokenPolicyCellV1::Record,
        &value,
        &marker,
        records,
        0,
        &mut std::io::sink(),
    )?;
    Ok(())
}

struct Resolved {
    outer: WorldFieldKeyV1,
    child: StateRootV1,
    key: (WorldFieldKeyV1, u64),
    before: [u8; 32],
    value: MainTokenPolicyValueV1,
}
fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: MainTokenPolicyCellRequestV1<'_>,
    tx: &mut oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
    max: u64,
    output: &mut impl std::io::Write,
) -> Result<Resolved, WorldFieldOverlayErrorV1> {
    if !request.cell.belongs(request.key.kind()) {
        return Err(invalid().into());
    }
    let (outer, child) = current(request.key, request.qualification, tx, records, error)?;
    let key = read::cell_key(request.key.kind(), request.cell, records)?;
    let before = crate::incremental_record_tree::lookup(&child, &key.0, records, genesis::LABELS)?
        .ok_or_else(invalid)?;
    let value = read::value(request.cell, &before, &key, records, max, output)?;
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
pub fn read_qualified_main_token_policy_cell_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: MainTokenPolicyCellRequestV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max: u64,
    output: &mut impl std::io::Write,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<MainTokenPolicyValueV1, WorldFieldOverlayErrorV1> {
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
pub struct MainTokenPolicyCellChangeV1<'a> {
    pub key: MainTokenPolicyKeyV1<'a>,
    pub cell: MainTokenPolicyCellV1,
    pub qualification: &'a ReservedValue<MainTokenPolicyRecordQualificationV1>,
    pub expected: MainTokenPolicyValueRefV1<'a>,
    pub after: MainTokenPolicyValueRefV1<'a>,
    pub max_current_payload_bytes: u64,
    pub max_pending_node_bytes: usize,
}
/// Fixed required leaves encode original Option None as Unit. No leaf deletion.
pub fn change_qualified_main_token_policy_cell_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: MainTokenPolicyCellChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<MainTokenPolicyRecordQualificationV1>>, WorldFieldOverlayErrorV1> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if change.cell == MainTokenPolicyCellV1::Record
            || !tx.uses_budget(write.memory)
            || context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !change.qualification.uses_budget(context.values.memory)
        {
            return Err(invalid().into());
        }
        change.expected.validate(change.key.kind(), change.cell)?;
        change.after.validate(change.key.kind(), change.cell)?;
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
            let mut compare = Compare::new(change.expected.text().as_bytes());
            let resolved = resolve(
                MainTokenPolicyCellRequestV1 {
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
            if !change.expected.matches(resolved.value) || !compare.is_equal() {
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
                change.key.kind(),
                change.cell,
                change.after.payload(),
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
            (Some(resolved.before), Some(row.value_root)),
            &mut context.values,
            write,
            cancel,
            crate::incremental_record_tree::RecordTreeWritePolicyV1 {
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
            genesis::root(change.key, child.clone(), &mut combined)?
        };
        if row.field_key != resolved.outer {
            return Err(invalid().into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            change.key.kind().namespace(),
            resolved.outer.as_bytes(),
            Some(row.value_root),
        )?;
        let owner = WorldFieldKeyV1::from_bytes(resolved.outer.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(invalid)?;
        let proof = closure::mint(
            change.key.kind(),
            owner,
            child,
            write.memory,
            MemoryClass::Ordinary,
        )?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(proof))
    })
}
