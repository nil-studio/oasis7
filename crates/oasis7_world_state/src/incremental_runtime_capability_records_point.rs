//! Ordinary precise point operations; no authorization/business transition.
use super::*;
use crate::{
    incremental_expected_bytes::Compare,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_record_tree::{self as tree, RecordTreeWritePolicyV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter, StateRootV1},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCapabilityValueRefV1<'a> {
    Unit,
    Unsigned(u64),
    Signed(i64),
    Text(&'a str),
}
impl<'a> RuntimeCapabilityValueRefV1<'a> {
    pub(super) fn payload(self) -> MemberPayloadRefV1<'a> {
        match self {
            Self::Unit => MemberPayloadRefV1::Unit,
            Self::Unsigned(n) => MemberPayloadRefV1::U64(n),
            Self::Signed(n) => MemberPayloadRefV1::I64(n),
            Self::Text(s) => MemberPayloadRefV1::Text(s),
        }
    }
    pub(super) fn text(self) -> &'a str {
        if let Self::Text(s) = self { s } else { "" }
    }
    pub(super) fn validate(self, c: RuntimeCapabilityColumnV1) -> Result<(), ValueTreeError> {
        let valid = match self {
            Self::Unit => c.nullable(),
            Self::Unsigned(_) => c.codec() == MemberPayloadCodecV1::U64,
            Self::Signed(_) => c.codec() == MemberPayloadCodecV1::I64,
            Self::Text(_) => c.codec() == MemberPayloadCodecV1::Text,
        };
        if valid { Ok(()) } else { Err(read::invalid()) }
    }
    pub(super) fn matches(self, value: RuntimeCapabilityValueV1) -> bool {
        match (self, value) {
            (Self::Unit, RuntimeCapabilityValueV1::Unit) => true,
            (Self::Unsigned(a), RuntimeCapabilityValueV1::Unsigned(b)) => a == b,
            (Self::Signed(a), RuntimeCapabilityValueV1::Signed(b)) => a == b,
            (Self::Text(a), RuntimeCapabilityValueV1::Text { utf8_bytes }) => {
                a.len() as u64 == utf8_bytes
            }
            _ => false,
        }
    }
}
pub struct RuntimeCapabilityPointRequestV1<'a> {
    pub key: RuntimeCapabilityRecordKeyV1<'a>,
    pub column: RuntimeCapabilityColumnV1,
    pub qualification: &'a ReservedValue<RuntimeCapabilityRecordQualificationV1>,
}
pub(super) struct Resolved {
    pub outer: WorldFieldKeyV1,
    pub child: StateRootV1,
    pub owner: [u8; 32],
}
pub(super) fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: RuntimeCapabilityRecordKeyV1<'_>,
    proof: &ReservedValue<RuntimeCapabilityRecordQualificationV1>,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<Resolved, WorldFieldOverlayErrorV1> {
    if records.values.class != MemoryClass::Ordinary
        || !tx.uses_budget(records.values.memory)
        || !proof.uses_budget(records.values.memory)
        || proof.parent() != &tx.parent_fence().parent_roots
    {
        return Err(read::invalid().into());
    }
    let meta = WorldFieldKeyV1::new(
        key.kind.descriptor(),
        WorldFieldRecordKindV1::Metadata,
        None,
    )
    .map_err(ValueTreeError::Invalid)?;
    let format = read::marker(key.kind, records)?;
    let outer = read::outer_key(key, records)?;
    let m = lookup(tx, &meta, records, error)?.ok_or_else(read::invalid)?;
    let f = lookup(tx, &format.0, records, error)?.ok_or_else(read::invalid)?;
    let id = lookup(tx, &outer.0, records, error)?.ok_or_else(read::invalid)?;
    records.presence(&m, &meta, 0)?;
    read::value(None, &f, &format, records, 0, &mut std::io::sink())?;
    let child = read::outer(key.kind, &outer, &id, records)?;
    let owner = WorldFieldKeyV1::from_bytes(outer.0.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .semantic_key
        .ok_or_else(read::invalid)?;
    proof.validate_binding(key.kind, &owner, &child, &tx.parent_fence().parent_roots)?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(Resolved {
        outer: outer.0,
        child,
        owner,
    })
}
pub(super) fn entry<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: RuntimeCapabilityRecordKindV1,
    column: RuntimeCapabilityColumnV1,
    child: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
) -> Result<((WorldFieldKeyV1, u64), [u8; 32]), ValueTreeError> {
    let key = read::cell_key(kind, Some(column), records)?;
    let id = tree::lookup(child, &key.0, records, genesis::LABELS)?.ok_or_else(read::invalid)?;
    Ok((key, id))
}
fn shape<R: ValueNodeReader, C: FnMut() -> bool>(
    kind: RuntimeCapabilityRecordKindV1,
    column: RuntimeCapabilityColumnV1,
    child: &StateRootV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    expected: RuntimeCapabilityValueRefV1<'_>,
    after: RuntimeCapabilityValueRefV1<'_>,
) -> Result<(), ValueTreeError> {
    expected.validate(column)?;
    after.validate(column)?;
    if column == RuntimeCapabilityColumnV1::SubjectKind {
        return Err(ValueTreeError::Invalid(
            "subject variant needs grouped operation",
        ));
    }
    let mask = column.subject_mask();
    if mask == 0 {
        return Ok(());
    }
    let (key, id) = entry(kind, RuntimeCapabilityColumnV1::SubjectKind, child, records)?;
    let mut bytes = [0; 6];
    let RuntimeCapabilityValueV1::Text { utf8_bytes } = read::value(
        Some(RuntimeCapabilityColumnV1::SubjectKind),
        &id,
        &key,
        records,
        6,
        &mut std::io::Cursor::new(bytes.as_mut_slice()),
    )?
    else {
        return Err(read::invalid());
    };
    let branch =
        closure::subject_kind(&bytes[..usize::try_from(utf8_bytes).map_err(|_| read::invalid())?])?;
    let active = mask == branch;
    if matches!(expected, RuntimeCapabilityValueRefV1::Unit) == active
        || matches!(after, RuntimeCapabilityValueRefV1::Unit) == active
    {
        return Err(read::invalid());
    }
    Ok(())
}
pub fn read_qualified_runtime_capability_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    request: RuntimeCapabilityPointRequestV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max: u64,
    output: &mut impl std::io::Write,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<RuntimeCapabilityValueV1, WorldFieldOverlayErrorV1> {
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if request.column.kind() != request.key.kind {
            return Err(read::invalid().into());
        }
        let _admission = ctx
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut records = InventoryValueReader {
            values: &mut ctx.values,
            key_work: &mut *ctx.key_work,
            semantic_bytes: ctx.semantic_bytes,
            record_bytes: ctx.record_bytes,
            cancelled: &mut ctx.cancelled,
        };
        let resolved = resolve(
            request.key,
            request.qualification,
            tx,
            &mut records,
            &mut ctx.take_state_error,
        )?;
        let (key, id) = entry(
            request.key.kind,
            request.column,
            &resolved.child,
            &mut records,
        )?;
        let decoded = read::value(Some(request.column), &id, &key, &mut records, max, output)?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(decoded)
    })
}
pub struct RuntimeCapabilityPointChangeV1<'a> {
    pub request: RuntimeCapabilityPointRequestV1<'a>,
    pub expected: RuntimeCapabilityValueRefV1<'a>,
    pub after: RuntimeCapabilityValueRefV1<'a>,
    pub max_current_payload_bytes: u64,
    pub max_pending_node_bytes: usize,
}
pub fn change_qualified_runtime_capability_cell_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: RuntimeCapabilityPointChangeV1<'_>,
    ctx: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<RuntimeCapabilityRecordQualificationV1>>, WorldFieldOverlayErrorV1>
{
    let cancel = &mut ctx.cancelled;
    let error = &mut ctx.take_state_error;
    ctx.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if !tx.uses_budget(write.memory) || change.request.column.kind() != change.request.key.kind
        {
            return Err(read::invalid().into());
        }
        let _admission = write
            .memory
            .reserve(64 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let (resolved, key, before) = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut ctx.values,
                key_work: &mut *ctx.key_work,
                semantic_bytes: ctx.semantic_bytes,
                record_bytes: ctx.record_bytes,
                cancelled: &mut combined,
            };
            let resolved = resolve(
                change.request.key,
                change.request.qualification,
                tx,
                &mut records,
                error,
            )?;
            shape(
                change.request.key.kind,
                change.request.column,
                &resolved.child,
                &mut records,
                change.expected,
                change.after,
            )?;
            let (key, id) = entry(
                change.request.key.kind,
                change.request.column,
                &resolved.child,
                &mut records,
            )?;
            let mut compare = Compare::new(change.expected.text().as_bytes());
            let value = read::value(
                Some(change.request.column),
                &id,
                &key,
                &mut records,
                change.max_current_payload_bytes,
                &mut compare,
            )?;
            if !change.expected.matches(value) || !compare.is_equal() {
                return Err(read::invalid().into());
            }
            if (records.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            (resolved, key, id)
        };
        let equal = change.expected == change.after;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if equal {
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
                change.request.key.kind,
                Some(change.request.column),
                change.after.payload(),
                &mut combined,
            )?
        };
        if row.field_key != key.0 {
            return Err(read::invalid().into());
        }
        let child = tree::edit(
            resolved.child,
            &key.0,
            (Some(before), Some(row.value_root)),
            &mut ctx.values,
            write,
            cancel,
            RecordTreeWritePolicyV1 {
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
            genesis::write_root(change.request.key, child.clone(), &mut combined)?
        };
        if row.field_key != resolved.outer {
            return Err(read::invalid().into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(
            change.request.key.kind.namespace(),
            resolved.outer.as_bytes(),
            Some(row.value_root),
        )?;
        let token = closure::mint(
            change.request.key.kind,
            resolved.owner,
            child,
            tx.parent_fence().parent_roots,
            write.memory,
            MemoryClass::Ordinary,
        )?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(token))
    })
}
