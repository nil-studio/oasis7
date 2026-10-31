//! Original optional rule state and governance effect IDs, not governance authority.
use crate::{
    incremental_field_member::*,
    incremental_field_presence::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_utf8_output::read_text_value,
    incremental_world_rule_parent::{descriptor, read_parent_fixed},
    state::WorldState,
    world_rules::WorldRulesStateV1,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use std::{collections::BTreeMap, io::Write};

const CHILD: &str = "governance_effects";
const UTF8_ERROR: &str = "world rule effect UTF-8";
fn effects(source: &WorldRulesStateV1) -> &BTreeMap<String, String> {
    let WorldRulesStateV1 {
        governance_effects,
        requests: _,
        organization_cases: _,
        organization_estate: _,
        supply_authorizations: _,
        supply_batches: _,
        supply_lineages: _,
        supply_shipments: _,
        organization_continuity: _,
        emergency_supply: _,
        agent_orders: _,
        industrial_work: _,
        substitution: _,
    } = source;
    governance_effects
}
fn start<W, C: FnMut() -> bool>(
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
) -> Result<(), MemberRecordError> {
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    *records = records
        .checked_sub(1)
        .ok_or(ValueTreeError::ResourceLimited("world rule effect records"))?;
    Ok(())
}
/// Produces only the optional parent marker and governance_effects child map.
/// The caller admits source/batch storage and discards the entire prefix on error.
/// Other rule children still require their own typed producers and qualification.
pub fn prepare_world_rule_effects_genesis_v1<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    sink: &mut S,
) -> Result<u64, MemberRecordError> {
    let field = descriptor()?;
    start(context, records)?;
    let metadata = write_field_metadata_v1(
        field,
        &mut PresenceRecordWriteContext {
            sink: &mut *context.sink,
            memory: context.memory,
            io: &mut *context.io,
            key_work: &mut *context.key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: context.limits.semantic_bytes,
                record_bytes: context.limits.record_bytes,
            },
            cancelled: &mut context.cancelled,
        },
    )?;
    crate::incremental_field_write::emit(metadata.field_key, metadata.value_root, context, sink)?;
    start(context, records)?;
    let record = write_member_record_v1(
        field,
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::Bool(source.world_rules.is_some()),
        context,
    )?;
    crate::incremental_field_write::emit(record.field_key, record.value_root, context, sink)?;
    let mut count = 0u64;
    if let Some(source) = &source.world_rules {
        for (key, value) in effects(source) {
            start(context, records)?;
            let record = write_member_record_v1(
                field,
                WorldFieldRecordKindV1::Entry,
                &(CHILD, key.as_str()),
                MemberPayloadRefV1::Text(value),
                context,
            )?;
            crate::incremental_field_write::emit(
                record.field_key,
                record.value_root,
                context,
                sink,
            )?;
            count = count
                .checked_add(1)
                .ok_or(MemberRecordError::Invalid("world rule effect count"))?;
        }
    }
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(count)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldRuleEffectValueV1 {
    RulesAbsent,
    EffectAbsent,
    /// Original UTF-8 result ID length; zero is present, not absence.
    Value(u64),
}
fn profile(semantic_bytes: u64, payload_bytes: u64, record_bytes: u64) -> MemberReadProfileV1 {
    MemberReadProfileV1 {
        codec: MemberPayloadCodecV1::Text,
        semantic_bytes,
        limits: MemberRecordLimitsV1 {
            semantic_bytes,
            payload_bytes,
            record_bytes,
        },
    }
}
/// Three states on the authenticated query path; never a complete rules-family
/// proof. Retained output and request storage are caller-admitted/provisional.
pub fn read_world_rule_effect_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: &str,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
    max_payload_bytes: u64,
    output: &mut impl Write,
) -> Result<WorldRuleEffectValueV1, ValueTreeError> {
    let (field, present) = read_parent_fixed(context)?;
    let (key, bytes) = context.key(field, WorldFieldRecordKindV1::Entry, &(CHILD, key))?;
    let root = context.lookup(&key)?;
    if !present {
        if root.is_some() {
            return Err(ValueTreeError::Invalid("orphan world rule effect"));
        }
        return Ok(WorldRuleEffectValueV1::RulesAbsent);
    }
    let Some(root) = root else {
        return Ok(WorldRuleEffectValueV1::EffectAbsent);
    };
    read_text_value(
        &root,
        &key,
        &mut context.values,
        profile(bytes, max_payload_bytes, context.record_bytes),
        output,
        UTF8_ERROR,
        &mut context.cancelled,
    )
    .map(WorldRuleEffectValueV1::Value)
}

#[path = "incremental_world_rule_effects_overlay.rs"]
mod overlay;
pub use overlay::{
    WorldRuleEffectChangeV1, WorldRuleEffectExpectedV1, change_world_rule_effect_overlay_v1,
    read_world_rule_effect_overlay_v1,
};
