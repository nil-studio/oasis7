//! Original supply-shipment fields, without supply or settlement authority.
use crate::{
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_utf8_output::read_text_value,
    incremental_world_rule_parent::read_parent_fixed,
    incremental_world_rule_record::{RecordPresenceV1, SupplyRecordKeyV1, marker, shape},
    state::WorldState,
    world_rules::{SupplyShipmentV1, WorldRulesStateV1},
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::io::{Cursor, Write};
const COLUMNS: [Option<&str>; 8] = [
    None,
    Some("authorization_id"),
    Some("batch_id"),
    Some("beneficiary_id"),
    Some("lineage_id"),
    Some("input_amount"),
    Some("delivered_amount"),
    Some("settled"),
];
type Keys = [(WorldFieldKeyV1, u64); 8];
fn payloads(value: &SupplyShipmentV1) -> [MemberPayloadRefV1<'_>; 8] {
    let SupplyShipmentV1 {
        authorization_id,
        batch_id,
        beneficiary_id,
        lineage_id,
        input_amount,
        delivered_amount,
        settled,
    } = value;
    [
        MemberPayloadRefV1::Unit,
        MemberPayloadRefV1::Text(authorization_id),
        MemberPayloadRefV1::Text(batch_id),
        MemberPayloadRefV1::Text(beneficiary_id),
        MemberPayloadRefV1::Text(lineage_id),
        MemberPayloadRefV1::I64(*input_amount),
        MemberPayloadRefV1::I64(*delivered_amount),
        MemberPayloadRefV1::Bool(*settled),
    ]
}
fn shipments(source: &WorldRulesStateV1) -> &std::collections::BTreeMap<u64, SupplyShipmentV1> {
    let WorldRulesStateV1 {
        requests: _,
        governance_effects: _,
        organization_cases: _,
        organization_estate: _,
        supply_authorizations: _,
        supply_batches: _,
        supply_lineages: _,
        supply_shipments,
        organization_continuity: _,
        emergency_supply: _,
        agent_orders: _,
        industrial_work: _,
        substitution: _,
    } = source;
    supply_shipments
}
/// Child records only. One frozen genesis owner first writes the optional parent
/// once, admits its source/batch storage, and discards all prefixes on error.
pub fn prepare_world_rule_shipments_genesis_members_v1<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    sink: &mut S,
) -> Result<u64, MemberRecordError> {
    crate::incremental_world_rule_parent::descriptor()?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let mut count = 0u64;
    if let Some(source) = &source.world_rules {
        for (&key, value) in shipments(source) {
            for (column, payload) in COLUMNS.into_iter().zip(payloads(value)) {
                if (context.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                *records = records
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "world rule shipment records",
                    ))?;
                let record = SupplyRecordKeyV1::Shipment(key).write(column, payload, context)?;
                crate::incremental_field_write::emit(
                    record.field_key,
                    record.value_root,
                    context,
                    sink,
                )?;
            }
            count = count
                .checked_add(1)
                .ok_or(MemberRecordError::Invalid("world rule shipment count"))?;
        }
    }
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(count)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldRuleShipmentValueV1 {
    RulesAbsent,
    ShipmentAbsent,
    Value {
        identity_utf8_bytes: [u64; 4],
        input_amount: i64,
        delivered_amount: i64,
        settled: bool,
    },
}
fn profile(
    codec: MemberPayloadCodecV1,
    bytes: u64,
    payload_bytes: u64,
    record_bytes: u64,
) -> MemberReadProfileV1 {
    MemberReadProfileV1 {
        codec,
        semantic_bytes: bytes,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: bytes,
            payload_bytes,
            record_bytes,
        },
    }
}
fn read_fixed<R: ValueNodeReader, const N: usize>(
    root: &[u8; 32],
    key: &(WorldFieldKeyV1, u64),
    codec: MemberPayloadCodecV1,
    values: &mut ValueReadContext<'_, R>,
    record_bytes: u64,
    cancelled: &mut impl FnMut() -> bool,
) -> Result<[u8; N], ValueTreeError> {
    let mut bytes = [0; N];
    read_member_record_profile_v1(
        root,
        &key.0,
        values,
        profile(codec, key.1, N as u64, record_bytes),
        &mut std::io::sink(),
        &mut Cursor::new(bytes.as_mut_slice()),
        &mut *cancelled,
    )?;
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(bytes)
}
fn read_columns<R: ValueNodeReader>(
    keys: &Keys,
    roots: &[Option<[u8; 32]>; 8],
    values: &mut ValueReadContext<'_, R>,
    record_bytes: u64,
    max_payload_bytes: u64,
    outputs: &mut [&mut dyn Write; 4],
    cancelled: &mut impl FnMut() -> bool,
) -> Result<WorldRuleShipmentValueV1, ValueTreeError> {
    marker(
        &roots[0].unwrap(),
        &keys[0].0,
        keys[0].1,
        values,
        record_bytes,
        cancelled,
    )?;
    let mut lengths = [0; 4];
    for index in 0..4 {
        lengths[index] = read_text_value(
            &roots[index + 1].unwrap(),
            &keys[index + 1].0,
            values,
            profile(
                MemberPayloadCodecV1::Text,
                keys[index + 1].1,
                max_payload_bytes,
                record_bytes,
            ),
            &mut outputs[index],
            "world rule shipment UTF-8",
            cancelled,
        )?;
    }
    let input_amount = i64::from_be_bytes(read_fixed(
        &roots[5].unwrap(),
        &keys[5],
        MemberPayloadCodecV1::I64,
        values,
        record_bytes,
        cancelled,
    )?);
    let delivered_amount = i64::from_be_bytes(read_fixed(
        &roots[6].unwrap(),
        &keys[6],
        MemberPayloadCodecV1::I64,
        values,
        record_bytes,
        cancelled,
    )?);
    let [settled] = read_fixed(
        &roots[7].unwrap(),
        &keys[7],
        MemberPayloadCodecV1::Bool,
        values,
        record_bytes,
        cancelled,
    )?;
    let settled = match settled {
        0 => false,
        1 => true,
        _ => return Err(ValueTreeError::Invalid("world rule shipment bool")),
    };
    if cancelled() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(WorldRuleShipmentValueV1::Value {
        identity_utf8_bytes: lengths,
        input_amount,
        delivered_amount,
        settled,
    })
}
/// Ten authenticated paths; four original UTF-8 outputs remain provisional until
/// this entire call succeeds. The caller admits output storage and payload limits.
pub fn read_world_rule_shipment_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: u64,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload_bytes: u64,
    outputs: &mut [&mut dyn Write; 4],
) -> Result<WorldRuleShipmentValueV1, ValueTreeError> {
    let (_, present) = read_parent_fixed(context)?;
    let mut keys: [Option<(WorldFieldKeyV1, u64)>; 8] = std::array::from_fn(|_| None);
    for (slot, column) in keys.iter_mut().zip(COLUMNS) {
        *slot = Some(SupplyRecordKeyV1::Shipment(key).fixed_key(column, context)?);
    }
    let keys = keys.map(Option::unwrap);
    let mut roots = [None; 8];
    for (root, key) in roots.iter_mut().zip(&keys) {
        *root = context.lookup(&key.0)?;
    }
    match shape(present, &roots)? {
        RecordPresenceV1::RulesAbsent => Ok(WorldRuleShipmentValueV1::RulesAbsent),
        RecordPresenceV1::RecordAbsent => Ok(WorldRuleShipmentValueV1::ShipmentAbsent),
        RecordPresenceV1::Present => read_columns(
            &keys,
            &roots,
            &mut context.values,
            context.record_bytes,
            max_payload_bytes,
            outputs,
            &mut context.cancelled,
        ),
    }
}
#[path = "incremental_world_rule_shipments_overlay.rs"]
mod overlay;
pub use overlay::{
    WorldRuleShipmentChangeV1, WorldRuleShipmentExpectedV1, change_world_rule_shipment_overlay_v1,
    read_world_rule_shipment_overlay_v1,
};
