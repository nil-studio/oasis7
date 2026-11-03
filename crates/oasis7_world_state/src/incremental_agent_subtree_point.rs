//! Provisional single-column transactions over the original Agent keys.
use super::*;
use crate::{
    incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
    incremental_expected_bytes::Compare,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup_raw},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_value_mutation::ValueMutationContextV1,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionFence, ExecutionTransaction, PreparedGuard},
    incremental_state::{MerkleNodeReader, StateNamespaceV1 as Ns},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use std::io::Write;
fn invalid() -> ValueTreeError {
    ValueTreeError::Invalid("agent point binding/profile/expected")
}
pub struct AgentSubtreeQualifiedRequestV1<'a> {
    pub cell: AgentSubtreeScalarRequestV1<'a>,
    pub qualification: &'a ReservedValue<AgentSubtreeStructureQualificationV1>,
}
pub struct AgentSubtreeCellChangeV1<'a> {
    pub query: AgentSubtreeQualifiedRequestV1<'a>,
    pub expected: Option<AgentScalarRefV1<'a>>,
    pub after: Option<AgentScalarRefV1<'a>>,
    pub max_scalar_bytes: u64,
}
pub(super) struct Resolved {
    key: AgentFieldKeyV1,
    pub(super) before: Option<[u8; 32]>,
    pub(super) value: Option<AgentSubtreeValueV1>,
    pub(super) agent_exists: bool,
}
pub(super) fn text(value: Option<AgentScalarRefV1<'_>>) -> &str {
    match value {
        Some(AgentScalarRefV1::Text(s) | AgentScalarRefV1::OptionalText(Some(s))) => s,
        _ => "",
    }
}
pub(super) fn matches(
    expected: Option<AgentScalarRefV1<'_>>,
    actual: Option<AgentSubtreeValueV1>,
) -> bool {
    use AgentScalarRefV1 as V;
    use AgentSubtreeValueV1 as O;
    match (expected, actual) {
        (None, None) => true,
        (Some(V::U8(a)), Some(O::U8(b))) => a == b,
        (Some(V::U16(a)), Some(O::U16(b))) => a == b,
        (Some(V::U32(a)), Some(O::U32(b))) => a == b,
        (Some(V::U64(a)), Some(O::U64(b))) => a == b,
        (Some(V::I64(a)), Some(O::I64(b))) => a == b,
        (Some(V::Bool(a)), Some(O::Bool(b))) => a == b,
        (Some(V::Text(a)), Some(O::Text { utf8_bytes: b })) => a.len() as u64 == b,
        (Some(V::OptionalText(a)), Some(O::OptionalText(b))) => a.map(|s| s.len() as u64) == b,
        (Some(V::OptionalU64(a)), Some(O::OptionalU64(b))) => a == b,
        _ => false,
    }
}
fn leaf<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    tx: &mut ExecutionTransaction<'_, M>,
    owner: [u8; 32],
    field: AgentFieldV1,
    ordinal: u64,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<Option<[u8; 32]>, WorldFieldOverlayErrorV1> {
    let key = AgentFieldKeyV1::from_owner_digest(owner, field, ordinal)
        .map_err(ValueTreeError::Invalid)?;
    lookup_raw(tx, field.namespace(), key.as_bytes(), records, error)
}
fn control<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    tx: &mut ExecutionTransaction<'_, M>,
    owner: [u8; 32],
    field: AgentFieldV1,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
) -> Result<AgentSubtreeValueV1, WorldFieldOverlayErrorV1> {
    let id = leaf(tx, owner, field, 0, records, error)?.ok_or_else(invalid)?;
    Ok(read_agent_scalar_tree_v1(
        field,
        &id,
        records.values,
        64,
        &mut std::io::sink(),
        &mut *records.cancelled,
    )?)
}
pub(super) fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    query: AgentSubtreeQualifiedRequestV1<'_>,
    tx: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
    max: u64,
    output: &mut impl Write,
) -> Result<Resolved, WorldFieldOverlayErrorV1> {
    use AgentFieldV1 as F;
    use schema::Group;
    if records.values.class != MemoryClass::Ordinary
        || !tx.uses_budget(records.values.memory)
        || !query.qualification.uses_budget(records.values.memory)
    {
        return Err(invalid().into());
    }
    query
        .qualification
        .validate_digest(&tx.parent_fence().parent_roots)?;
    let request = query.cell;
    if request.agent.len() as u64 > records.semantic_bytes {
        return Err(ValueTreeError::ResourceLimited("agent identity allowance").into());
    }
    records.key_work.bytes_remaining = records
        .key_work
        .bytes_remaining
        .checked_sub(AGENT_FIELD_KEY_BYTES as u64 + request.agent.len() as u64)
        .ok_or(ValueTreeError::ResourceLimited("agent identity work"))?;
    let key = AgentFieldKeyV1::new(request.agent, request.field, request.ordinal)
        .map_err(ValueTreeError::Invalid)?;
    let owner = AgentFieldKeyV1::from_bytes(key.as_bytes())
        .map_err(ValueTreeError::Invalid)?
        .0;
    let body_key = AgentBodyKeyV1::new(request.agent).map_err(ValueTreeError::Invalid)?;
    let body = lookup_raw(tx, Ns::Business, body_key.as_bytes(), records, error)?;
    let before = lookup_raw(
        tx,
        request.field.namespace(),
        key.as_bytes(),
        records,
        error,
    )?;
    if let Some(body) = body {
        AgentBodyValueV1::from_canonical_bytes(&read::fixed_tree::<AGENT_BODY_VALUE_BYTES, _, _>(
            &body,
            records.values,
            &mut *records.cancelled,
        )?)
        .map_err(ValueTreeError::Invalid)?;
        let identity = leaf(tx, owner, F::Identity, 0, records, error)?.ok_or_else(invalid)?;
        let mut compare = Compare::new(request.agent.as_bytes());
        let decoded = read_agent_scalar_tree_v1(
            F::Identity,
            &identity,
            records.values,
            2048,
            &mut compare,
            &mut *records.cancelled,
        )?;
        if decoded
            != (AgentSubtreeValueV1::Text {
                utf8_bytes: request.agent.len() as u64,
            })
            || !compare.is_equal()
        {
            return Err(invalid().into());
        }
        match schema::group(request.field) {
            Group::Activity | Group::Intent => {
                let flag = if schema::group(request.field) == Group::Activity {
                    F::ActivityPresent
                } else {
                    F::IntentPresent
                };
                let present = match control(tx, owner, flag, records, error)? {
                    AgentSubtreeValueV1::Bool(v) => v,
                    _ => return Err(invalid().into()),
                };
                if present != before.is_some() {
                    return Err(invalid().into());
                }
                if present && schema::group(request.field) == Group::Intent {
                    let id =
                        leaf(tx, owner, F::IntentAgent, 0, records, error)?.ok_or_else(invalid)?;
                    let mut compare = Compare::new(request.agent.as_bytes());
                    let decoded = read_agent_scalar_tree_v1(
                        F::IntentAgent,
                        &id,
                        records.values,
                        2048,
                        &mut compare,
                        &mut *records.cancelled,
                    )?;
                    if decoded
                        != (AgentSubtreeValueV1::Text {
                            utf8_bytes: request.agent.len() as u64,
                        })
                        || !compare.is_equal()
                    {
                        return Err(invalid().into());
                    }
                }
            }
            Group::Slot | Group::Cargo => {
                let (columns, next, count) = if schema::group(request.field) == Group::Slot {
                    (
                        [F::SlotId, F::SlotType, F::SlotInstalled, F::SlotLocked],
                        F::SlotNext,
                        F::SlotCount,
                    )
                } else {
                    (
                        [F::CargoId, F::CargoKind, F::CargoQuantity, F::CargoUnitSize],
                        F::CargoNext,
                        F::CargoCount,
                    )
                };
                let mut presence = None;
                for field in columns {
                    let current =
                        leaf(tx, owner, field, request.ordinal, records, error)?.is_some();
                    if presence.is_some_and(|v| v != current) {
                        return Err(invalid().into());
                    }
                    presence = Some(current);
                }
                let next = match control(tx, owner, next, records, error)? {
                    AgentSubtreeValueV1::U64(v) => v,
                    _ => return Err(invalid().into()),
                };
                let count = match control(tx, owner, count, records, error)? {
                    AgentSubtreeValueV1::U64(v) => v,
                    _ => return Err(invalid().into()),
                };
                if count > next || before.is_some() && (request.ordinal >= next || count == 0) {
                    return Err(invalid().into());
                }
            }
            Group::Base if before.is_none() => return Err(invalid().into()),
            _ => {}
        }
    } else if before.is_some() {
        return Err(invalid().into());
    }
    let value = before
        .map(|id| {
            read_agent_scalar_tree_v1(
                request.field,
                &id,
                records.values,
                max,
                output,
                &mut *records.cancelled,
            )
        })
        .transpose()?;
    if (records.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(Resolved {
        key,
        before,
        value,
        agent_exists: body.is_some(),
    })
}
/// The exact shared resolver used by the setter. Output is provisional until
/// both this return and the outer transaction's typed/history acceptance pass.
pub fn read_agent_subtree_scalar_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    query: AgentSubtreeQualifiedRequestV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max: u64,
    output: &mut impl Write,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<Option<AgentSubtreeValueV1>, WorldFieldOverlayErrorV1> {
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !query.qualification.uses_budget(context.values.memory)
        {
            return Err(invalid().into());
        }
        let _scratch = context
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
        Ok(resolve(query, tx, &mut records, error, max, output)?.value)
    })
}
/// Existing fixed/row columns and the two optional ResourceStock members only.
/// Control/presence/row lifecycles require their complete structural operations.
/// Errors poison any earlier successful contribution; no owner/history token is minted.
pub fn change_agent_subtree_scalar_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    change: AgentSubtreeCellChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut ValueMutationContextV1<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    use AgentFieldV1 as F;
    let cancel = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|tx| {
        tx.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !tx.uses_budget(context.values.memory)
            || !tx.uses_budget(write.memory)
            || !change
                .query
                .qualification
                .uses_budget(context.values.memory)
        {
            return Err(invalid().into());
        }
        if matches!(
            change.query.cell.field,
            F::SlotCount
                | F::SlotNext
                | F::CargoCount
                | F::CargoNext
                | F::ActivityPresent
                | F::IntentPresent
        ) {
            return Err(invalid().into());
        }
        for value in [change.expected, change.after].into_iter().flatten() {
            change
                .query
                .cell
                .field
                .validate_value(value)
                .map_err(ValueTreeError::Invalid)?;
        }
        if matches!(change.query.cell.field, F::Identity | F::IntentAgent)
            && change
                .after
                .is_some_and(|v| v != AgentScalarRefV1::Text(change.query.cell.agent))
        {
            return Err(invalid().into());
        }
        let _scratch = write
            .memory
            .reserve(32 << 10, MemoryClass::Ordinary)
            .map_err(ValueTreeError::Memory)?;
        let query = AgentSubtreeQualifiedRequestV1 {
            cell: change.query.cell,
            qualification: change.query.qualification,
        };
        let resolved = {
            let mut combined = || cancel() || (write.cancelled)();
            let mut records = InventoryValueReader {
                values: &mut context.values,
                key_work: &mut *context.key_work,
                semantic_bytes: context.semantic_bytes,
                record_bytes: context.record_bytes,
                cancelled: &mut combined,
            };
            let mut compare = Compare::new(text(change.expected).as_bytes());
            let resolved = resolve(
                query,
                tx,
                &mut records,
                error,
                change.max_scalar_bytes,
                &mut compare,
            )?;
            if !matches(change.expected, resolved.value) || !compare.is_equal() {
                return Err(invalid().into());
            }
            resolved
        };
        let unchanged = change.expected == change.after;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if unchanged {
            return Ok(false);
        }
        if !resolved.agent_exists
            || resolved.before.is_some() != change.after.is_some()
                && schema::group(change.query.cell.field) != schema::Group::Resource
        {
            return Err(invalid().into());
        }
        let after = if let Some(value) = change.after {
            let length = value.encoded_len().map_err(ValueTreeError::Invalid)?;
            if length > change.max_scalar_bytes {
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
        tx.write(
            change.query.cell.field.namespace(),
            resolved.key.as_bytes(),
            after,
        )?;
        if cancel() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}
