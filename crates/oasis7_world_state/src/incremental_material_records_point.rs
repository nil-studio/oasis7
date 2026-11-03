//! Qualified one-column edits. Route Option edits include their presence flag.
use super::*;
use crate::{
    MaterialLedgerId,
    incremental_expected_bytes::Compare,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_ledger_value::LedgerValueKindV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter, StateRootV1},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("material point column/expected/binding")
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaterialCellValueRefV1<'a> {
    Text(&'a str),
    U64(u64),
    I64(i64),
    Ledger(&'a MaterialLedgerId),
}
impl<'a> MaterialCellValueRefV1<'a> {
    fn payload(self) -> MemberPayloadRefV1<'a> {
        match self {
            Self::Text(v) => MemberPayloadRefV1::Text(v),
            Self::U64(v) => MemberPayloadRefV1::U64(v),
            Self::I64(v) => MemberPayloadRefV1::I64(v),
            Self::Ledger(v) => MemberPayloadRefV1::Ledger(v),
        }
    }
    fn validate(
        self,
        kind: MaterialRecordKindV1,
        cell: MaterialCellV1,
    ) -> Result<(), ValueTreeError> {
        if !cell.accepts(kind) {
            return Err(invalid());
        }
        let valid = match self {
            Self::Text(_) => cell.codec() == MemberPayloadCodecV1::Text,
            Self::U64(v) => cell.codec() == MemberPayloadCodecV1::U64 && v <= cell.integer_max(),
            Self::I64(_) => cell.codec() == MemberPayloadCodecV1::I64,
            Self::Ledger(_) => cell.codec() == MemberPayloadCodecV1::Ledger,
        };
        if valid { Ok(()) } else { Err(invalid()) }
    }
    fn text(self) -> &'a str {
        match self {
            Self::Text(v) => v,
            Self::Ledger(
                MaterialLedgerId::Agent(v)
                | MaterialLedgerId::Site(v)
                | MaterialLedgerId::Factory(v),
            ) => v,
            _ => "",
        }
    }
    fn matches(self, value: MaterialValueV1) -> bool {
        match (self, value) {
            (Self::Text(v), MaterialValueV1::Text { utf8_bytes }) => v.len() as u64 == utf8_bytes,
            (Self::U64(a), MaterialValueV1::Unsigned(b)) => a == b,
            (Self::I64(a), MaterialValueV1::Signed(b)) => a == b,
            (Self::Ledger(v), MaterialValueV1::Ledger(info)) => {
                let kind = match v {
                    MaterialLedgerId::World => LedgerValueKindV1::World,
                    MaterialLedgerId::Agent(_) => LedgerValueKindV1::Agent,
                    MaterialLedgerId::Site(_) => LedgerValueKindV1::Site,
                    MaterialLedgerId::Factory(_) => LedgerValueKindV1::Factory,
                };
                info.kind == kind && info.id_bytes == self.text().len() as u64
            }
            _ => false,
        }
    }
}
pub struct MaterialCellChangeV1<'a> {
    pub key: MaterialRecordKeyV1<'a>,
    pub cell: MaterialCellV1,
    pub qualification: &'a ReservedValue<MaterialRecordQualificationV1>,
    pub expected: Option<MaterialCellValueRefV1<'a>>,
    pub after: Option<MaterialCellValueRefV1<'a>>,
    pub max_current_payload_bytes: u64,
    pub max_pending_node_bytes: usize,
}
pub(super) fn current<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: MaterialRecordKeyV1<'_>,
    proof: &ReservedValue<MaterialRecordQualificationV1>,
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
    key: MaterialRecordKeyV1<'_>,
    tx: &mut oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let field = key.kind().descriptor()?;
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
        MaterialCellV1::Record,
        &value,
        &marker,
        records,
        0,
        &mut std::io::sink(),
    )?;
    Ok(())
}
/// Every error aborts the contribution, including final no-op cancellation.
/// The original expected/after bytes for this column remain caller-owned.
pub fn change_qualified_material_cell_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: MaterialCellChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<ReservedValue<MaterialRecordQualificationV1>>, WorldFieldOverlayErrorV1> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        let kind = change.key.kind();
        let cell = change.cell;
        if matches!(cell, MaterialCellV1::Record | MaterialCellV1::RoutePresent)
            || !cell.accepts(kind)
            || !tx.uses_budget(write.memory)
            || context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !change.qualification.uses_budget(context.values.memory)
        {
            return Err(invalid().into());
        }
        if cell != MaterialCellV1::RouteId && (change.expected.is_none() || change.after.is_none())
        {
            return Err(invalid().into());
        }
        for value in [change.expected, change.after].into_iter().flatten() {
            value.validate(kind, cell)?;
        }
        let _operation = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let mut new = [None; 2];
        new[0] = change.after.map(MaterialCellValueRefV1::payload);
        if cell == MaterialCellV1::RouteId {
            new[1] = Some(MemberPayloadRefV1::Bool(change.after.is_some()));
        }
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
                    .map_or("", MaterialCellValueRefV1::text)
                    .as_bytes(),
            );
            let resolved = overlay_read::resolve(
                MaterialCellRequestV1 {
                    key: change.key,
                    cell,
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
        let overlay_read::Resolved {
            outer,
            mut child,
            paths,
            roots,
            ..
        } = resolved;
        let unchanged = change.expected == change.after;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if unchanged {
            return Ok(None);
        }
        for (i, path) in paths.into_iter().enumerate() {
            let Some((cell, key)) = path else { continue };
            if i == 1 && change.expected.is_some() == change.after.is_some() {
                continue;
            }
            let after = if let Some(payload) = new[i] {
                let mut combined = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: || cancel() || (write.cancelled)(),
                };
                let row = write_cell(change.key, cell, payload, &mut combined)?;
                if row.field_key != key.0 {
                    return Err(invalid().into());
                }
                Some(row.value_root)
            } else {
                None
            };
            if cancel() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            child = crate::incremental_record_tree::edit(
                child,
                &key.0,
                (roots[i], after),
                &mut context.values,
                write,
                cancel,
                crate::incremental_record_tree::RecordTreeWritePolicyV1 {
                    max_pending_node_bytes: change.max_pending_node_bytes,
                    labels: genesis::LABELS,
                },
            )?;
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
            genesis::write_root(change.key, child.clone(), &mut combined)?
        };
        if row.field_key != outer {
            return Err(invalid().into());
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        tx.write(kind.namespace(), outer.as_bytes(), Some(row.value_root))?;
        let owner = WorldFieldKeyV1::from_bytes(outer.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .ok_or_else(invalid)?;
        let result = closure::mint(kind, owner, child, write.memory, MemoryClass::Ordinary)?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(Some(result))
    })
}
