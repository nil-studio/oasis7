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
    incremental_state::MerkleNodeReader,
    incremental_value::ValueNodeReader,
};
use oasis7_proto::memory_budget::MemoryClass;
use std::io::Write;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactoryValueRefV1<'a> {
    Unit,
    Unsigned(u64),
    Signed(i64),
    Bool(bool),
    Text(&'a str),
    Ledger(&'a MaterialLedgerId),
}
impl<'a> FactoryValueRefV1<'a> {
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
            _ => return Err(ValueTreeError::Invalid("factory payload family")),
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
    pub(super) fn matches(self, current: FactoryValueV1) -> bool {
        match (self, current) {
            (Self::Unit, FactoryValueV1::Unit) => true,
            (Self::Unsigned(a), FactoryValueV1::Unsigned(b)) => a == b,
            (Self::Signed(a), FactoryValueV1::Signed(b)) => a == b,
            (Self::Bool(a), FactoryValueV1::Bool(b)) => a == b,
            (Self::Text(s), FactoryValueV1::Text { utf8_bytes }) => s.len() as u64 == utf8_bytes,
            (Self::Ledger(v), FactoryValueV1::Ledger(info)) => {
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
fn validate_payload(
    cell: FactoryCellV1<'_>,
    value: FactoryValueRefV1<'_>,
) -> Result<(), ValueTreeError> {
    let (codec, nullable) = cell.profile();
    if !(value.codec() == codec || nullable && value == FactoryValueRefV1::Unit)
        || matches!(value,FactoryValueRefV1::Unsigned(v) if v>cell.integer_max())
    {
        return Err(ValueTreeError::Invalid("factory after value type"));
    }
    Ok(())
}
struct Resolved {
    inner_key: WorldFieldKeyV1,
    before: Option<[u8; 32]>,
    outer: container::Outer,
    value: FactoryCellReadV1,
}
fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: FactoryRecordKeyV1<'_>,
    cell: FactoryCellV1<'_>,
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
    let keys = read::keys(key, cell, records)?;
    let (value, before) = if let Some(child) = &outer.child {
        let roots = tree::roots(child, &keys, records)?;
        (
            read::resolved(cell, &keys, &roots, records, max_payload_bytes, output)?,
            roots[2],
        )
    } else {
        (FactoryCellReadV1::RecordAbsent, None)
    };
    Ok(Resolved {
        inner_key: keys[2].as_ref().unwrap().0.clone(),
        before,
        outer,
        value,
    })
}
/// Current shared view, exact query scope; every failure poisons the transaction.
pub fn read_factory_cell_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: FactoryRecordKeyV1<'_>,
    cell: FactoryCellV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max_payload_bytes: u64,
    output: &mut impl Write,
) -> Result<FactoryCellReadV1, WorldFieldOverlayErrorV1> {
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
            return Err(ValueTreeError::Invalid("factory overlay pool/class").into());
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
pub struct FactoryCellChangeV1<'a> {
    pub key: FactoryRecordKeyV1<'a>,
    pub cell: FactoryCellV1<'a>,
    pub expected: Option<FactoryValueRefV1<'a>>,
    pub after: Option<FactoryValueRefV1<'a>>,
    pub max_current_payload_bytes: u64,
}
/// Precise column/member contribution. No unchanged Spec, snapshot or sibling
/// text/vector is scanned. Whole record creation/deletion uses the explicit
/// replacement API; the business owner commits related count/parent changes.
pub fn change_factory_cell_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: FactoryCellChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut FactoryWorkBudgetV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let FactoryCellChangeV1 {
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
            return Err(ValueTreeError::Invalid("factory overlay pool/class").into());
        }
        let _workspace = write
            .memory
            .reserve(8192, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        valid_cell(key, cell)?;
        if matches!(cell, FactoryCellV1::Record) {
            return Err(
                ValueTreeError::Invalid("factory record marker requires replacement").into(),
            );
        }
        if after.is_none()
            && !matches!(
                cell,
                FactoryCellV1::TextItem(..)
                    | FactoryCellV1::StackItem(..)
                    | FactoryCellV1::Balance(..)
            )
        {
            return Err(ValueTreeError::Invalid("factory required cell deletion").into());
        }
        for value in [expected, after].into_iter().flatten() {
            validate_payload(cell, value)?;
        }
        let mut compare = Compare::new(expected.map_or("", FactoryValueRefV1::text).as_bytes());
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
            (None, FactoryCellReadV1::MemberAbsent) => true,
            (Some(expected), FactoryCellReadV1::Value(value)) => expected.matches(value),
            _ => false,
        };
        if !matches || !compare.is_equal() {
            return Err(ValueTreeError::Invalid("factory expected cell").into());
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
                return Err(ValueTreeError::Invalid("factory writer key").into());
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
            .ok_or(ValueTreeError::Invalid("factory missing record root"))?;
        let child = tree::edit(
            child,
            &resolved.inner_key,
            (resolved.before, root),
            &mut context.values,
            write,
            cancelled,
            work,
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
            return Err(ValueTreeError::Invalid("factory outer writer key").into());
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

#[derive(Debug, Clone, Copy)]
pub struct FactoryRecordChangeV1<'a> {
    pub key: FactoryRecordKeyV1<'a>,
    pub expected: Option<FactoryRecordRefV1<'a>>,
    pub after: Option<FactoryRecordRefV1<'a>>,
    pub max_current_payload_bytes: u64,
}
/// Verify an explicitly supplied complete record, including its actual child
/// relation. This record proof does not qualify other records or the World.
pub fn verify_factory_record_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: FactoryRecordKeyV1<'_>,
    expected: FactoryRecordRefV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    work: &mut FactoryWorkBudgetV1,
    max_payload_bytes: u64,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<ReservedValue<FactoryRecordProofV1>, WorldFieldOverlayErrorV1> {
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(context.values.memory)
        {
            return Err(ValueTreeError::Invalid("factory overlay pool/class").into());
        }
        let charge = context
            .values
            .memory
            .reserve(4096, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
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
        let child = outer
            .child
            .ok_or(ValueTreeError::Invalid("factory expected record"))?;
        proof::verify(key, expected, &child, &mut records, max_payload_bytes, work)?;
        let owner = WorldFieldKeyV1::from_bytes(outer.key.as_bytes())
            .map_err(ValueTreeError::Invalid)?
            .semantic_key
            .unwrap();
        Ok(charge.retain(FactoryRecordProofV1 {
            parent_roots: transaction.parent_fence().parent_roots,
            schema: key.schema(),
            owner,
            root: child,
            value_root: outer.value_root.unwrap(),
        }))
    })
}
/// Explicit full-record replacement. A complete current-child relation is
/// verified before replacing its sole outer typed root; no loose old members
/// can be silently adopted or retained. Small changes use the point API.
fn replace_record<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: FactoryRecordChangeV1<'_>,
    qualification: Option<&ReservedValue<FactoryFamilyQualificationV1>>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut FactoryWorkBudgetV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let FactoryRecordChangeV1 {
        key,
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
            return Err(ValueTreeError::Invalid("factory overlay pool/class").into());
        }
        let _workspace = write
            .memory
            .reserve(16 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        if [expected, after]
            .into_iter()
            .flatten()
            .any(|value| !value.matches(key))
        {
            return Err(ValueTreeError::Invalid("factory record type").into());
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
            let outer = container::overlay(
                key,
                transaction,
                &mut records,
                &mut context.take_state_error,
            )?;
            if let Some(expected) = expected {
                let child = outer
                    .child
                    .as_ref()
                    .ok_or(ValueTreeError::Invalid("factory expected record"))?;
                proof::verify(
                    key,
                    expected,
                    child,
                    &mut records,
                    max_current_payload_bytes,
                    work,
                )?;
            } else {
                let qualification = qualification.ok_or(ValueTreeError::Invalid(
                    "factory creation requires family qualification",
                ))?;
                if !qualification.uses_budget(context.values.memory) {
                    return Err(ValueTreeError::Invalid("factory qualification pool").into());
                }
                qualification.validate_binding(&transaction.parent_fence().parent_roots)?;
                if outer.child.is_some() {
                    return Err(
                        ValueTreeError::Invalid("factory create expected absent record").into(),
                    );
                }
            }
            outer
        };
        if expected == after {
            if cancelled() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(false);
        }
        let root = if let Some(after) = after {
            let child = container::body(key, after, &mut context.values, write, cancelled, work)?;
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
            if record.field_key != outer.key {
                return Err(ValueTreeError::Invalid("factory outer writer key").into());
            }
            Some(record.value_root)
        } else {
            None
        };
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        transaction.write(key.namespace(), outer.key.as_bytes(), root)?;
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}

/// Explicit full replacement with a complete supplied expected record. Creation
/// uses the separately qualified entrypoint below.
pub fn replace_factory_record_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    change: FactoryRecordChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut FactoryWorkBudgetV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    replace_record(change, None, context, write, work, guard, live)
}

pub struct FactoryRecordCreateV1<'a> {
    pub key: FactoryRecordKeyV1<'a>,
    pub after: FactoryRecordRefV1<'a>,
    pub qualification: &'a ReservedValue<FactoryFamilyQualificationV1>,
}
/// Normal creation is allowed only from a complete same-parent family token.
/// The new child is built from empty, never from unqualified loose descendants.
/// A receiver must still validate the complete transaction, including raw writes.
pub fn create_factory_record_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: StatePageWriter,
    CW: FnMut() -> bool,
>(
    create: FactoryRecordCreateV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    work: &mut FactoryWorkBudgetV1,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    replace_record(
        FactoryRecordChangeV1 {
            key: create.key,
            expected: None,
            after: Some(create.after),
            max_current_payload_bytes: 0,
        },
        Some(create.qualification),
        context,
        write,
        work,
        guard,
        live,
    )
}
