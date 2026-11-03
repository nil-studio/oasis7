//! Stable-ordinal row operations; never re-enumerate an existing Vec.
use super::*;
use crate::{
    agent_state::{BodyModuleSlot, CargoEntityEntry},
    incremental_expected_bytes::Compare,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_value_mutation::ValueMutationContextV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, PreparedGuard},
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
#[derive(Clone, Copy)]
pub enum AgentSubtreeRowRefV1<'a> {
    Slot(&'a BodyModuleSlot),
    Cargo(&'a CargoEntityEntry),
}
impl<'a> AgentSubtreeRowRefV1<'a> {
    fn group(self) -> schema::Group {
        match self {
            Self::Slot(_) => schema::Group::Slot,
            Self::Cargo(_) => schema::Group::Cargo,
        }
    }
    fn columns(self) -> Result<[Option<AgentScalarRefV1<'a>>; 53], ValueTreeError> {
        let mut values = [None; 53];
        let mut collect = |field: AgentFieldV1, value| -> Result<(), ValueTreeError> {
            field
                .validate_value(value)
                .map_err(ValueTreeError::Invalid)?;
            values[(field as u16 - 1) as usize] = Some(value);
            Ok(())
        };
        match self {
            Self::Slot(row) => visit_agent_slot_values_v1(row, &mut collect)?,
            Self::Cargo(row) => visit_agent_cargo_values_v1(row, &mut collect)?,
        }
        Ok(values)
    }
}
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("agent row expected/allocation")
}
pub struct AgentSubtreeRowChangeV1<'a> {
    pub agent: &'a str,
    pub qualification: &'a ReservedValue<AgentSubtreeStructureQualificationV1>,
    pub ordinal: u64,
    pub expected_count: u64,
    pub expected_next: u64,
    pub expected: Option<AgentSubtreeRowRefV1<'a>>,
    pub after: Option<AgentSubtreeRowRefV1<'a>>,
    pub max_scalar_bytes: u64,
}
fn verify_expected<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    change: &AgentSubtreeRowChangeV1<'_>,
    expected: &[Option<AgentScalarRefV1<'_>>; 53],
    kind: schema::Group,
    tx: &mut oasis7_formal_protocol::incremental_execution::ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<(), WorldFieldOverlayErrorV1> {
    use AgentFieldV1 as F;
    let (count, next) = if kind == schema::Group::Slot {
        (F::SlotCount, F::SlotNext)
    } else {
        (F::CargoCount, F::CargoNext)
    };
    let query = |field, ordinal| AgentSubtreeQualifiedRequestV1 {
        cell: AgentSubtreeScalarRequestV1 {
            agent: change.agent,
            field,
            ordinal,
        },
        qualification: change.qualification,
    };
    for (field, expected) in [(count, change.expected_count), (next, change.expected_next)] {
        let value = point::resolve(
            query(field, 0),
            tx,
            records,
            error,
            64,
            &mut std::io::sink(),
        )?;
        if !value.agent_exists || value.value != Some(AgentSubtreeValueV1::U64(expected)) {
            return Err(invalid().into());
        }
    }
    if change.expected.is_none() && change.ordinal != change.expected_next
        || change.expected.is_some()
            && (change.ordinal >= change.expected_next || change.expected_count == 0)
    {
        return Err(invalid().into());
    }
    for field in F::ALL
        .iter()
        .copied()
        .filter(|field| schema::group(*field) == kind)
    {
        let index = (field as u16 - 1) as usize;
        let mut compare = Compare::new(point::text(expected[index]).as_bytes());
        let actual = point::resolve(
            query(field, change.ordinal),
            tx,
            records,
            error,
            change.max_scalar_bytes,
            &mut compare,
        )?;
        if !point::matches(expected[index], actual.value) || !compare.is_equal() {
            return Err(invalid().into());
        }
    }
    Ok(())
}
/// A complete expected row read against the current shared overlay. This adds
/// exact dependencies and poisons the transaction on mismatch; it does not
/// grant Agent permissions, a history receipt, or an independently active root.
pub struct AgentSubtreeRowExpectedV1<'a> {
    pub agent: &'a str,
    pub qualification: &'a ReservedValue<AgentSubtreeStructureQualificationV1>,
    pub ordinal: u64,
    pub count: u64,
    pub next: u64,
    pub expected: AgentSubtreeRowRefV1<'a>,
    pub max_scalar_bytes: u64,
}
pub fn verify_agent_subtree_row_expected_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    query: AgentSubtreeRowExpectedV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<(), WorldFieldOverlayErrorV1> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !query.qualification.uses_budget(context.values.memory)
            || query.count > query.next
        {
            return Err(invalid().into());
        }
        let _scratch = context
            .values
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let expected = query.expected.columns()?;
        verify_expected(
            &AgentSubtreeRowChangeV1 {
                agent: query.agent,
                qualification: query.qualification,
                ordinal: query.ordinal,
                expected_count: query.count,
                expected_next: query.next,
                expected: Some(query.expected),
                after: Some(query.expected),
                max_scalar_bytes: query.max_scalar_bytes,
            },
            &expected,
            query.expected.group(),
            tx,
            &mut InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: cancel,
            },
            error,
        )?;
        if cancel() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(())
    })
}
/// Append uses exactly the current Next and advances it; removal decrements
/// Count while preserving Next and all surviving ordinals. Explicit four-column
/// replacement is bounded to this row; a scalar edit need not request it.
pub fn change_agent_subtree_row_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    change: AgentSubtreeRowChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut ValueMutationContextV1<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !tx.uses_budget(write.memory)
            || !change.qualification.uses_budget(context.values.memory)
        {
            return Err(invalid().into());
        }
        let _scratch = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let kind = change
            .expected
            .or(change.after)
            .ok_or_else(invalid)?
            .group();
        if change.expected.is_some_and(|row| row.group() != kind)
            || change.after.is_some_and(|row| row.group() != kind)
            || change.expected_count > change.expected_next
        {
            return Err(invalid().into());
        }
        let expected = change
            .expected
            .map(|row| row.columns())
            .transpose()?
            .unwrap_or([None; 53]);
        let after = change
            .after
            .map(|row| row.columns())
            .transpose()?
            .unwrap_or([None; 53]);
        use AgentFieldV1 as F;
        let (count, next) = if kind == schema::Group::Slot {
            (F::SlotCount, F::SlotNext)
        } else {
            (F::CargoCount, F::CargoNext)
        };
        {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            verify_expected(&change, &expected, kind, tx, &mut records, error)?;
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if expected == after {
            if cancel() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(false);
        }
        let (new_count, new_next) = match (change.expected.is_some(), change.after.is_some()) {
            (false, true) => (
                change.expected_count.checked_add(1).ok_or_else(invalid)?,
                change.expected_next.checked_add(1).ok_or_else(invalid)?,
            ),
            (true, false) => (
                change.expected_count.checked_sub(1).ok_or_else(invalid)?,
                change.expected_next,
            ),
            (true, true) => (change.expected_count, change.expected_next),
            (false, false) => return Err(invalid().into()),
        };
        macro_rules! put {
            ($field:expr,$ordinal:expr,$value:expr) => {{
                let field = $field;
                let value: Option<AgentScalarRefV1<'_>> = $value;
                let digest = if let Some(value) = value {
                    if value.encoded_len().map_err(ValueTreeError::Invalid)?
                        > change.max_scalar_bytes
                    {
                        return Err(ValueTreeError::ResourceLimited("agent scalar bytes").into());
                    }
                    let mut combined = ValueMutationContextV1 {
                        sink: &mut *write.sink,
                        memory: write.memory,
                        io: &mut *write.io,
                        cancelled: || cancel() || (write.cancelled)(),
                    };
                    Some(write_agent_scalar_stream_v1(value, &mut combined)?.digest()?)
                } else {
                    None
                };
                if cancel() || (write.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                let key = AgentFieldKeyV1::new(change.agent, field, $ordinal)
                    .map_err(ValueTreeError::Invalid)?;
                tx.write(field.namespace(), key.as_bytes(), digest)?;
                if cancel() || (write.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
            }};
        }
        for field in F::ALL
            .iter()
            .copied()
            .filter(|field| schema::group(*field) == kind)
        {
            let index = (field as u16 - 1) as usize;
            if expected[index] != after[index] {
                put!(field, change.ordinal, after[index]);
            }
        }
        if new_count != change.expected_count {
            put!(count, 0, Some(AgentScalarRefV1::U64(new_count)));
        }
        if new_next != change.expected_next {
            put!(next, 0, Some(AgentScalarRefV1::U64(new_next)));
        }
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}
