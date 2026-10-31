use super::*;
use crate::{
    incremental_expected_bytes::Compare,
    incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1, lookup},
    incremental_inventory_value_read::InventoryValueReader,
    incremental_world_rule_parent::read_parent_overlay,
};
use oasis7_formal_protocol::incremental_execution::{
    ExecutionFence, ExecutionTransaction, PreparedGuard,
};
use oasis7_proto::memory_budget::MemoryClass;
fn resolve<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: u64,
    transaction: &mut ExecutionTransaction<'_, M>,
    records: &mut InventoryValueReader<'_, '_, R, C>,
    error: &mut E,
    max_payload_bytes: u64,
    outputs: &mut [&mut dyn Write; 4],
) -> Result<(Keys, WorldRuleShipmentValueV1), WorldFieldOverlayErrorV1> {
    let (_, present) = read_parent_overlay(transaction, records, error)?;
    let mut keys: [Option<(WorldFieldKeyV1, u64)>; 8] = std::array::from_fn(|_| None);
    for (slot, column) in keys.iter_mut().zip(COLUMNS) {
        *slot = Some(SupplyRecordKeyV1::Shipment(key).overlay_key(column, records)?);
    }
    let keys = keys.map(Option::unwrap);
    let mut roots = [None; 8];
    for (root, key) in roots.iter_mut().zip(&keys) {
        *root = lookup(transaction, &key.0, records, error)?;
    }
    let value = match shape(present, &roots)? {
        RecordPresenceV1::RulesAbsent => WorldRuleShipmentValueV1::RulesAbsent,
        RecordPresenceV1::RecordAbsent => WorldRuleShipmentValueV1::ShipmentAbsent,
        RecordPresenceV1::Present => read_columns(
            &keys,
            &roots,
            records.values,
            records.record_bytes,
            max_payload_bytes,
            outputs,
            records.cancelled,
        )?,
    };
    Ok((keys, value))
}
pub fn read_world_rule_shipment_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: u64,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    max_payload_bytes: u64,
    outputs: &mut [&mut dyn Write; 4],
) -> Result<WorldRuleShipmentValueV1, WorldFieldOverlayErrorV1> {
    let mut records = InventoryValueReader {
        values: &mut context.values,
        key_work: context.key_work,
        semantic_bytes: context.semantic_bytes,
        record_bytes: context.record_bytes,
        cancelled: &mut context.cancelled,
    };
    let error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        if records.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(records.values.memory)
        {
            return Err(ValueTreeError::Invalid("world rule shipment overlay pool/class").into());
        }
        resolve(
            key,
            transaction,
            &mut records,
            error,
            max_payload_bytes,
            outputs,
        )
        .map(|(_, value)| value)
    })
}
#[derive(Debug, Clone, Copy)]
pub enum WorldRuleShipmentExpectedV1<'a> {
    RulesAbsent,
    ShipmentAbsent,
    Value(&'a SupplyShipmentV1),
}
#[derive(Debug, Clone, Copy)]
pub struct WorldRuleShipmentChangeV1<'a> {
    pub key: u64,
    pub expected: WorldRuleShipmentExpectedV1<'a>,
    pub after: Option<&'a SupplyShipmentV1>,
    pub max_current_payload_bytes: u64,
}
fn equal(a: MemberPayloadRefV1<'_>, b: MemberPayloadRefV1<'_>) -> bool {
    match (a, b) {
        (MemberPayloadRefV1::Unit, MemberPayloadRefV1::Unit) => true,
        (MemberPayloadRefV1::Text(a), MemberPayloadRefV1::Text(b)) => a == b,
        (MemberPayloadRefV1::I64(a), MemberPayloadRefV1::I64(b)) => a == b,
        (MemberPayloadRefV1::Bool(a), MemberPayloadRefV1::Bool(b)) => a == b,
        _ => false,
    }
}
/// The original owner decides authority, settlement and complete expected data.
/// Every failure poisons the whole transaction; parent creation is forbidden.
pub fn change_world_rule_shipment_overlay_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeWriter,
    CW: FnMut() -> bool,
>(
    change: WorldRuleShipmentChangeV1<'_>,
    context: &mut WorldFieldOverlayContextV1<'_, '_, '_, M, R, E, C>,
    write: &mut MemberRecordWriteContext<'_, W, CW>,
    guard: &PreparedGuard,
    live: &ExecutionFence,
) -> Result<bool, WorldFieldOverlayErrorV1> {
    let WorldRuleShipmentChangeV1 {
        key,
        expected,
        after,
        max_current_payload_bytes,
    } = change;
    let before = match expected {
        WorldRuleShipmentExpectedV1::Value(value) => Some(value),
        _ => None,
    };
    let texts = before
        .map(|value| {
            [
                value.authorization_id.as_str(),
                value.batch_id.as_str(),
                value.beneficiary_id.as_str(),
                value.lineage_id.as_str(),
            ]
        })
        .unwrap_or([""; 4]);
    let cancelled = &mut context.cancelled;
    let error = &mut context.take_state_error;
    context.transaction.contribute(|transaction| {
        transaction.validate_guard(guard, live)?;
        if context.values.class != MemoryClass::Ordinary
            || !transaction.uses_budget(context.values.memory)
            || !transaction.uses_budget(write.memory)
        {
            return Err(ValueTreeError::Invalid("world rule shipment overlay pool/class").into());
        }
        let mut compares = texts.map(|text| Compare::new(text.as_bytes()));
        let (keys, current) = {
            let [a, b, c, d] = &mut compares;
            let mut outputs: [&mut dyn Write; 4] = [a, b, c, d];
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
                transaction,
                &mut records,
                error,
                max_current_payload_bytes,
                &mut outputs,
            )?
        };
        let valid = match (current, expected) {
            (WorldRuleShipmentValueV1::RulesAbsent, WorldRuleShipmentExpectedV1::RulesAbsent)
            | (
                WorldRuleShipmentValueV1::ShipmentAbsent,
                WorldRuleShipmentExpectedV1::ShipmentAbsent,
            ) => true,
            (
                WorldRuleShipmentValueV1::Value {
                    input_amount,
                    delivered_amount,
                    settled,
                    ..
                },
                WorldRuleShipmentExpectedV1::Value(expected),
            ) => {
                compares.iter().all(Compare::is_equal)
                    && input_amount == expected.input_amount
                    && delivered_amount == expected.delivered_amount
                    && settled == expected.settled
            }
            _ => false,
        };
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        if !valid {
            return Err(ValueTreeError::Invalid("world rule shipment expected value").into());
        }
        if matches!(current, WorldRuleShipmentValueV1::RulesAbsent) && after.is_some() {
            return Err(ValueTreeError::Invalid("world rule shipment cannot create parent").into());
        }
        if before == after {
            if cancelled() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            return Ok(false);
        }
        let old = before.map(payloads);
        let new = after.map(payloads);
        for (index, column) in COLUMNS.into_iter().enumerate() {
            let unchanged = old
                .as_ref()
                .zip(new.as_ref())
                .is_some_and(|(old, new)| equal(old[index], new[index]));
            // Equal Text columns may require a long borrowed byte comparison.
            // Observe cancellation before skipping an unchanged column.
            if cancelled() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            if unchanged {
                continue;
            }
            let root = if let Some(new) = &new {
                let mut combined = MemberRecordWriteContext {
                    sink: &mut *write.sink,
                    memory: write.memory,
                    io: &mut *write.io,
                    key_work: &mut *write.key_work,
                    limits: write.limits,
                    cancelled: || cancelled() || (write.cancelled)(),
                };
                let record =
                    SupplyRecordKeyV1::Shipment(key).write(column, new[index], &mut combined)?;
                if record.field_key != keys[index].0 {
                    return Err(ValueTreeError::Invalid("world rule shipment writer key").into());
                }
                Some(record.value_root)
            } else {
                None
            };
            if cancelled() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
            transaction.write(
                oasis7_formal_protocol::incremental_state::StateNamespaceV1::Authorization,
                keys[index].0.as_bytes(),
                root,
            )?;
            if cancelled() || (write.cancelled)() {
                return Err(ValueTreeError::Cancelled.into());
            }
        }
        if cancelled() || (write.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(true)
    })
}
