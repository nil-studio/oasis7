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
pub enum IndustryValueRefV1<'a> {
    Unit,
    Unsigned(u64),
    Signed(i64),
    Bool(bool),
    Text(&'a str),
    Ledger(&'a MaterialLedgerId),
}
impl<'a> IndustryValueRefV1<'a> {
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
            _ => return Err(ValueTreeError::Invalid("industry payload family")),
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
    pub(super) fn matches(self, current: IndustryValueV1) -> bool {
        match (self, current) {
            (Self::Unit, IndustryValueV1::Unit) => true,
            (Self::Unsigned(a), IndustryValueV1::Unsigned(b)) => a == b,
            (Self::Signed(a), IndustryValueV1::Signed(b)) => a == b,
            (Self::Bool(a), IndustryValueV1::Bool(b)) => a == b,
            (Self::Text(s), IndustryValueV1::Text { utf8_bytes }) => s.len() as u64 == utf8_bytes,
            (Self::Ledger(v), IndustryValueV1::Ledger(info)) => {
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
    cell: IndustryCellV1<'_>,
    value: IndustryValueRefV1<'_>,
) -> Result<(), ValueTreeError> {
    let (codec, nullable) = cell.profile();
    if !(value.codec() == codec || nullable && value == IndustryValueRefV1::Unit)
        || matches!(value,IndustryValueRefV1::Unsigned(v) if v>cell.integer_max())
    {
        return Err(ValueTreeError::Invalid("industry after value type"));
    }
    Ok(())
}
struct Resolved {
    inner_key: WorldFieldKeyV1,
    before: Option<[u8; 32]>,
    outer: container::Outer,
    value: IndustryCellReadV1,
}
fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: IndustryRecordKeyV1,
    cell: IndustryCellV1<'_>,
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
        (IndustryCellReadV1::RecordAbsent, None)
    };
    Ok(Resolved {
        inner_key: keys[2].as_ref().unwrap().0.clone(),
        before,
        outer,
        value,
    })
}
/// Current shared view, exact query scope; every failure poisons the transaction.
pub fn read_industry_cell_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: IndustryRecordKeyV1,
    cell: IndustryCellV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max_payload_bytes: u64,
    output: &mut impl Write,
) -> Result<IndustryCellReadV1, WorldFieldOverlayErrorV1> {
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
            return Err(ValueTreeError::Invalid("industry overlay pool/class").into());
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
pub struct IndustryCellChangeV1<'a> {
    pub key: IndustryRecordKeyV1,
    pub cell: IndustryCellV1<'a>,
    pub expected: Option<IndustryValueRefV1<'a>>,
    pub after: Option<IndustryValueRefV1<'a>>,
    pub max_current_payload_bytes: u64,
}
/// Precise column/member contribution. No unchanged Spec, snapshot or sibling
/// text/vector is scanned. Whole record creation/deletion uses the explicit
/// replacement API; the business owner commits related count/parent changes.
pub fn change_industry_cell_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: IndustryCellChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut IndustryWorkBudgetV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let IndustryCellChangeV1 {
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
            return Err(ValueTreeError::Invalid("industry overlay pool/class").into());
        }
        let _workspace = write
            .memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        valid_cell(key, cell)?;
        if matches!(cell, IndustryCellV1::Record) {
            return Err(
                ValueTreeError::Invalid("industry record marker requires replacement").into(),
            );
        }
        if after.is_none()
            && !matches!(
                cell,
                IndustryCellV1::TextItem(..)
                    | IndustryCellV1::StackItem(..)
                    | IndustryCellV1::Balance(..)
            )
        {
            return Err(ValueTreeError::Invalid("industry required cell deletion").into());
        }
        for value in [expected, after].into_iter().flatten() {
            validate_payload(cell, value)?;
        }
        let mut compare = Compare::new(expected.map_or("", IndustryValueRefV1::text).as_bytes());
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
            (None, IndustryCellReadV1::MemberAbsent) => true,
            (Some(expected), IndustryCellReadV1::Value(value)) => expected.matches(value),
            _ => false,
        };
        if !matches || !compare.is_equal() {
            return Err(ValueTreeError::Invalid("industry expected cell").into());
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
                return Err(ValueTreeError::Invalid("industry writer key").into());
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
            .ok_or(ValueTreeError::Invalid("industry missing record root"))?;
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
            return Err(ValueTreeError::Invalid("industry outer writer key").into());
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
pub struct IndustryRecordCreateV1<'a> {
    pub key: IndustryRecordKeyV1,
    pub after: IndustryRecordRefV1<'a>,
    pub qualification: &'a ReservedValue<IndustryFamilyQualificationV1>,
}
pub fn create_industry_record_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: oasis7_formal_protocol::incremental_state::StatePageWriter + ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    change: IndustryRecordCreateV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut IndustryWorkBudgetV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let IndustryRecordCreateV1 {
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
            return Err(ValueTreeError::Invalid("industry create pool/class").into());
        }
        qualification.validate_binding(&transaction.parent_fence().parent_roots)?;
        if key.kind != after.kind() {
            return Err(ValueTreeError::Invalid("industry create kind").into());
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
            return Err(ValueTreeError::Invalid("industry create expected absent").into());
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
            return Err(ValueTreeError::Invalid("industry create outer key").into());
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
pub fn read_industry_record_root_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: IndustryRecordKeyV1,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
) -> Result<Option<[u8; 32]>, WorldFieldOverlayErrorV1> {
    context.transaction.contribute(|transaction| {
        if context.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(context.values.memory)
        {
            return Err(ValueTreeError::Invalid("industry root read pool/class").into());
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
pub struct IndustryRecordDeleteV1<'a> {
    pub key: IndustryRecordKeyV1,
    pub expected_root: Option<[u8; 32]>,
    pub qualification: &'a ReservedValue<IndustryFamilyQualificationV1>,
}
pub fn delete_industry_record_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: oasis7_formal_protocol::incremental_value::ValueNodeWriter,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    CW: FnMut() -> bool,
>(
    change: IndustryRecordDeleteV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut IndustryWorkBudgetV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let IndustryRecordDeleteV1 {
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
            return Err(ValueTreeError::Invalid("industry delete pool/class").into());
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
            return Err(ValueTreeError::Invalid("industry delete expected root").into());
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
