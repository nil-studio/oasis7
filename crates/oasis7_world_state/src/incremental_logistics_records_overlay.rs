use super::*;
use crate::{
    MaterialLedgerId,
    incremental_expected_bytes::Compare,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_ledger_value::LedgerValueKindV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::MemoryClass;
use std::io::Write;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogisticsValueRefV1<'a> {
    Unit,
    Unsigned(u64),
    Signed(i64),
    Bool(bool),
    Text(&'a str),
    Ledger(&'a MaterialLedgerId),
}
impl<'a> LogisticsValueRefV1<'a> {
    fn payload(self) -> MemberPayloadRefV1<'a> {
        match self {
            Self::Unit => MemberPayloadRefV1::Unit,
            Self::Unsigned(v) => MemberPayloadRefV1::U64(v),
            Self::Signed(v) => MemberPayloadRefV1::I64(v),
            Self::Bool(v) => MemberPayloadRefV1::Bool(v),
            Self::Text(v) => MemberPayloadRefV1::Text(v),
            Self::Ledger(v) => MemberPayloadRefV1::Ledger(v),
        }
    }
    pub(super) fn from_payload(value: MemberPayloadRefV1<'a>) -> Result<Self, ValueTreeError> {
        Ok(match value {
            MemberPayloadRefV1::Unit => Self::Unit,
            MemberPayloadRefV1::U64(v) => Self::Unsigned(v),
            MemberPayloadRefV1::I64(v) => Self::Signed(v),
            MemberPayloadRefV1::Bool(v) => Self::Bool(v),
            MemberPayloadRefV1::Text(v) => Self::Text(v),
            MemberPayloadRefV1::Ledger(v) => Self::Ledger(v),
            _ => return Err(ValueTreeError::Invalid("logistics payload family")),
        })
    }
    fn codec(self) -> MemberPayloadCodecV1 {
        match self {
            Self::Unit => MemberPayloadCodecV1::Unit,
            Self::Unsigned(_) => MemberPayloadCodecV1::U64,
            Self::Signed(_) => MemberPayloadCodecV1::I64,
            Self::Bool(_) => MemberPayloadCodecV1::Bool,
            Self::Text(_) => MemberPayloadCodecV1::Text,
            Self::Ledger(_) => MemberPayloadCodecV1::Ledger,
        }
    }
    pub(super) fn text(self) -> &'a str {
        match self {
            Self::Text(s) => s,
            Self::Ledger(
                MaterialLedgerId::Agent(s)
                | MaterialLedgerId::Site(s)
                | MaterialLedgerId::Factory(s),
            ) => s,
            _ => "",
        }
    }
    pub(super) fn matches(self, current: LogisticsValueV1) -> bool {
        match (self, current) {
            (Self::Unit, LogisticsValueV1::Unit) => true,
            (Self::Unsigned(a), LogisticsValueV1::Unsigned(b)) => a == b,
            (Self::Signed(a), LogisticsValueV1::Signed(b)) => a == b,
            (Self::Bool(a), LogisticsValueV1::Bool(b)) => a == b,
            (Self::Text(s), LogisticsValueV1::Text { utf8_bytes }) => s.len() as u64 == utf8_bytes,
            (Self::Ledger(v), LogisticsValueV1::Ledger(info)) => {
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
pub(super) fn validate_payload(
    cell: LogisticsCellV1,
    value: LogisticsValueRefV1<'_>,
) -> Result<(), ValueTreeError> {
    let (codec, nullable) = cell.profile();
    if !(value.codec() == codec || nullable && value == LogisticsValueRefV1::Unit)
        || matches!(value,LogisticsValueRefV1::Unsigned(v) if v>cell.integer_max())
    {
        return Err(ValueTreeError::Invalid("logistics after value type"));
    }
    Ok(())
}
struct Resolved {
    inner_key: WorldFieldKeyV1,
    before: Option<[u8; 32]>,
    outer: container::Outer,
    value: LogisticsCellReadV1,
}
fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: LogisticsRecordKeyV1<'_>,
    cell: LogisticsCellV1,
    transaction: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
    max_payload_bytes: u64,
    output: &mut impl Write,
) -> Result<Resolved, WorldFieldOverlayErrorV1> {
    valid_cell(key, cell)?;
    let _workspace = records
        .values
        .memory
        .reserve(8192, records.values.class)
        .map_err(ValueTreeError::Memory)?;
    let outer = container::overlay(key, transaction, records, error)?;
    let keys = read::child_keys(key, cell, records)?;
    let (value, before) = if let Some(child) = &outer.child {
        let roots = read::inner_roots(child, &keys, records)?;
        (
            read::resolved(cell, &keys, &roots, records, max_payload_bytes, output)?,
            roots[2],
        )
    } else {
        (LogisticsCellReadV1::RecordAbsent, None)
    };
    Ok(Resolved {
        inner_key: keys[2].as_ref().unwrap().0.clone(),
        before,
        outer,
        value,
    })
}
/// Current shared view, exact query scope; every failure poisons the transaction.
pub fn read_logistics_cell_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: LogisticsRecordKeyV1<'_>,
    cell: LogisticsCellV1,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max_payload_bytes: u64,
    output: &mut impl Write,
) -> Result<LogisticsCellReadV1, WorldFieldOverlayErrorV1> {
    let mut records = InventoryValueReader {
        values: &mut context.values,
        key_work: context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        cancelled: &mut context.cancelled,
    };
    context.transaction.contribute(|transaction| {
        if records.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(records.values.memory)
        {
            return Err(ValueTreeError::Invalid("logistics overlay pool/class").into());
        }
        resolve(
            key,
            cell,
            transaction,
            &mut records,
            &mut context.take_state_error,
            max_payload_bytes,
            output,
        )
        .map(|resolved| resolved.value)
    })
}
#[derive(Debug, Clone, Copy)]
pub struct LogisticsCellChangeV1<'a> {
    pub key: LogisticsRecordKeyV1<'a>,
    pub cell: LogisticsCellV1,
    pub expected: Option<LogisticsValueRefV1<'a>>,
    pub after: Option<LogisticsValueRefV1<'a>>,
    pub max_current_payload_bytes: u64,
}
/// Precise column/member contribution. No unchanged Spec, snapshot or sibling
/// text/vector is scanned. Whole record creation/deletion uses the explicit
/// replacement API; the business owner commits related count/parent changes.
pub fn change_logistics_cell_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: LogisticsCellChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut LogisticsWorkBudgetV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let LogisticsCellChangeV1 {
        key,
        cell,
        expected,
        after,
        max_current_payload_bytes,
    } = change;
    let cancelled = &mut context.cancelled;
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(context.values.memory)
            || !transaction.uses_budget(write.memory)
        {
            return Err(ValueTreeError::Invalid("logistics overlay pool/class").into());
        }
        let _workspace = write
            .memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        valid_cell(key, cell)?;
        if matches!(cell, LogisticsCellV1::Record) {
            return Err(
                ValueTreeError::Invalid("logistics record marker requires replacement").into(),
            );
        }
        if after.is_none() && !matches!(cell, LogisticsCellV1::TextItem(..)) {
            return Err(ValueTreeError::Invalid("logistics required cell deletion").into());
        }
        for value in [expected, after].into_iter().flatten() {
            validate_payload(cell, value)?;
        }
        let mut compare = Compare::new(expected.map_or("", LogisticsValueRefV1::text).as_bytes());
        work.cell()?;
        let resolved = {
            let mut cancel = || cancelled() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut cancel,
            };
            resolve(
                key,
                cell,
                transaction,
                &mut records,
                &mut context.take_state_error,
                max_current_payload_bytes,
                &mut compare,
            )?
        };
        let matches = match (expected, resolved.value) {
            (None, LogisticsCellReadV1::MemberAbsent) => true,
            (Some(expected), LogisticsCellReadV1::Value(value)) => expected.matches(value),
            _ => false,
        };
        if !matches || !compare.is_equal() {
            return Err(ValueTreeError::Invalid("logistics expected cell").into());
        }
        if expected == after {
            if cancelled() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(false);
        }
        let root = if let Some(after) = after {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancelled() || (write.cancelled)(),
            };
            let record = write_cell(key, cell, after.payload(), &mut combined)?;
            if record.field_key != resolved.inner_key {
                return Err(ValueTreeError::Invalid("logistics writer key").into());
            }
            Some(record.value_root)
        } else {
            None
        };
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let child = resolved
            .outer
            .child
            .ok_or(ValueTreeError::Invalid("logistics missing record root"))?;
        let child = crate::incremental_record_tree::edit(
            child,
            &resolved.inner_key,
            (resolved.before, root),
            &mut context.values,
            write,
            cancelled,
            crate::incremental_record_tree::RecordTreeWritePolicyV1 {
                max_pending_node_bytes: work.max_pending_node_bytes,
                labels: genesis::LABELS,
            },
        )?;
        let record = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancelled() || (write.cancelled)(),
            };
            container::write(key, child, &mut combined)?
        };
        if record.field_key != resolved.outer.key {
            return Err(ValueTreeError::Invalid("logistics outer writer key").into());
        }
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        transaction.write(
            key.namespace(),
            record.field_key.as_bytes(),
            Some(record.value_root),
        )?;
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}

/// Normal record creation requires complete same-parent-family qualification;
/// missing marker alone never authorizes absorption of hidden child members.
pub struct LogisticsRecordCreateV1<'a> {
    pub key: LogisticsRecordKeyV1<'a>,
    pub after: LogisticsRecordRefV1<'a>,
    pub qualification: &'a ReservedValue<LogisticsFamilyQualificationV1>,
}
pub fn create_logistics_record_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: oasis7_formal_protocol::incremental_state::StatePageWriter + ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    change: LogisticsRecordCreateV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut LogisticsWorkBudgetV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let LogisticsRecordCreateV1 {
        key,
        after,
        qualification,
    } = change;
    let cancelled = &mut context.cancelled;
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(context.values.memory)
            || !transaction.uses_budget(write.memory)
            || !qualification.uses_budget(context.values.memory)
        {
            return Err(ValueTreeError::Invalid("logistics create pool/class").into());
        }
        qualification.validate_binding(&transaction.parent_fence().parent_roots)?;
        if key.kind != after.kind() {
            return Err(ValueTreeError::Invalid("logistics create kind").into());
        }
        let outer = {
            let mut combined = || cancelled() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            container::overlay(
                key,
                transaction,
                &mut records,
                &mut context.take_state_error,
            )?
        };
        if outer.child.is_some() {
            return Err(ValueTreeError::Invalid("logistics create expected absent").into());
        }
        let mut remaining = work.cells_remaining;
        let record = {
            let mut combined = MemberRecordWriteContext {
                sink: &mut *write.sink,
                memory: write.memory,
                io: &mut *write.io,
                key_work: &mut *write.key_work,
                limits: write.limits,
                cancelled: || cancelled() || (write.cancelled)(),
            };
            let child = genesis::body(key, after, &mut combined, &mut remaining, work)?;
            container::write(key, child, &mut combined)?
        };
        if record.field_key != outer.key {
            return Err(ValueTreeError::Invalid("logistics create outer key").into());
        }
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        transaction.write(
            key.namespace(),
            record.field_key.as_bytes(),
            Some(record.value_root),
        )?;
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })
}

/// Exact current outer dependency for an explicit whole-record removal. This
/// scope still does not mint family qualification or prove raw world absence.
pub fn read_logistics_record_root_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: LogisticsRecordKeyV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<Option<[u8; 32]>, WorldFieldOverlayErrorV1> {
    context.transaction.contribute(|transaction| {
        if context.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(context.values.memory)
        {
            return Err(ValueTreeError::Invalid("logistics root read pool/class").into());
        }
        let mut records = InventoryValueReader {
            values: &mut context.values,
            key_work: context.key_work,
            semantic_bytes: context.semantic_bytes,
            record_bytes: context.record_bytes,
            cancelled: &mut context.cancelled,
        };
        let outer = container::overlay(
            key,
            transaction,
            &mut records,
            &mut context.take_state_error,
        )?;
        if (records.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(outer.value_root)
    })
}
/// A qualified complete child can be removed by its exact authenticated outer
/// value dependency. Its original pages remain reachable through history.
pub struct LogisticsRecordDeleteV1<'a> {
    pub key: LogisticsRecordKeyV1<'a>,
    pub expected_root: Option<[u8; 32]>,
    pub qualification: &'a ReservedValue<LogisticsFamilyQualificationV1>,
}
pub fn delete_logistics_record_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: oasis7_formal_protocol::incremental_value::ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    change: LogisticsRecordDeleteV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut LogisticsWorkBudgetV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let LogisticsRecordDeleteV1 {
        key,
        expected_root,
        qualification,
    } = change;
    let cancelled = &mut context.cancelled;
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(context.values.memory)
            || !transaction.uses_budget(write.memory)
            || !qualification.uses_budget(context.values.memory)
        {
            return Err(ValueTreeError::Invalid("logistics delete pool/class").into());
        }
        qualification.validate_binding(&transaction.parent_fence().parent_roots)?;
        work.cell()?;
        let outer = {
            let mut combined = || cancelled() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            container::overlay(
                key,
                transaction,
                &mut records,
                &mut context.take_state_error,
            )?
        };
        if outer.value_root != expected_root {
            return Err(ValueTreeError::Invalid("logistics delete expected root").into());
        }
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if outer.value_root.is_none() {
            return Ok(false);
        }
        transaction.write(key.namespace(), outer.key.as_bytes(), None)?;
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}
