//! Closed three-i64 lineage records; supply policy and parent creation stay outside.
use crate::{
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_rule_parent::{descriptor, read_parent_fixed},
    incremental_world_rule_record::{SupplyRecordKeyV1, marker},
    state::WorldState,
    world_rules::{SupplyLineageV1, WorldRulesStateV1},
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::MerkleNodeReader,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;

#[derive(Clone, Copy)]
enum Part {
    Present,
    Target,
    Fulfilled,
    Reserved,
}
impl Part {
    const ALL: [Self; 4] = [Self::Present, Self::Target, Self::Fulfilled, Self::Reserved];
    fn name(self) -> &'static str {
        match self {
            Self::Present => "",
            Self::Target => "target",
            Self::Fulfilled => "fulfilled",
            Self::Reserved => "reserved",
        }
    }
}
fn amounts(value: &SupplyLineageV1) -> [i64; 3] {
    let SupplyLineageV1 {
        target,
        fulfilled,
        reserved,
    } = value;
    [*target, *fulfilled, *reserved]
}
fn lineages(source: &WorldRulesStateV1) -> &std::collections::BTreeMap<String, SupplyLineageV1> {
    let WorldRulesStateV1 {
        supply_lineages,
        requests: _,
        governance_effects: _,
        organization_cases: _,
        organization_estate: _,
        supply_authorizations: _,
        supply_batches: _,
        supply_shipments: _,
        organization_continuity: _,
        emergency_supply: _,
        agent_orders: _,
        industrial_work: _,
        substitution: _,
    } = source;
    supply_lineages
}
fn write_part<W: ValueNodeWriter, C: FnMut() -> bool>(
    key: &str,
    part: Part,
    value: Option<i64>,
    context: &mut MemberRecordWriteContext<'_, W, C>,
) -> Result<MemberRecordV1, MemberRecordError> {
    let payload = if let Some(value) = value {
        MemberPayloadRefV1::I64(value)
    } else {
        MemberPayloadRefV1::Unit
    };
    SupplyRecordKeyV1::Lineage(key).write(
        if matches!(part, Part::Present) {
            None
        } else {
            Some(part.name())
        },
        payload,
        context,
    )
}
/// Child records only: the genesis owner must first produce the optional parent
/// Metadata/Bool once from the same frozen WorldState, and discard all prefixes
/// on any child error. This does not certify complete world_rules coverage.
pub fn prepare_world_rule_lineages_genesis_members_v1<
    W: ValueNodeWriter,
    C: FnMut() -> bool,
    S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
>(
    source: &WorldState,
    context: &mut MemberRecordWriteContext<'_, W, C>,
    records: &mut u64,
    sink: &mut S,
) -> Result<u64, MemberRecordError> {
    descriptor()?;
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    let mut count = 0u64;
    if let Some(source) = &source.world_rules {
        for (key, value) in lineages(source) {
            let [target, fulfilled, reserved] = amounts(value);
            for (part, value) in [
                (Part::Present, None),
                (Part::Target, Some(target)),
                (Part::Fulfilled, Some(fulfilled)),
                (Part::Reserved, Some(reserved)),
            ] {
                if (context.cancelled)() {
                    return Err(ValueTreeError::Cancelled.into());
                }
                *records = records
                    .checked_sub(1)
                    .ok_or(ValueTreeError::ResourceLimited(
                        "world rule lineage records",
                    ))?;
                let record = write_part(key, part, value, context)?;
                crate::incremental_field_write::emit(
                    record.field_key,
                    record.value_root,
                    context,
                    sink,
                )?;
            }
            count = count
                .checked_add(1)
                .ok_or(MemberRecordError::Invalid("world rule lineage count"))?;
        }
    }
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled.into());
    }
    Ok(count)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldRuleLineageValueV1 {
    RulesAbsent,
    LineageAbsent,
    Value(SupplyLineageV1),
}
fn shape(
    present: bool,
    roots: &[Option<[u8; 32]>; 4],
) -> Result<Option<WorldRuleLineageValueV1>, ValueTreeError> {
    if !present {
        if roots.iter().any(Option::is_some) {
            return Err(ValueTreeError::Invalid("orphan world rule lineage"));
        }
        return Ok(Some(WorldRuleLineageValueV1::RulesAbsent));
    }
    if roots[0].is_none() {
        if roots[1..].iter().any(Option::is_some) {
            return Err(ValueTreeError::Invalid("orphan world rule lineage column"));
        }
        return Ok(Some(WorldRuleLineageValueV1::LineageAbsent));
    }
    if roots[1..].iter().any(Option::is_none) {
        return Err(ValueTreeError::Invalid("world rule lineage missing column"));
    }
    Ok(None)
}
/// Authenticates six paths, preserving absent parent/record and all-zero value.
/// It proves this closed query path, not other children or the whole rule family.
pub fn read_world_rule_lineage_v1<
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
>(
    key: &str,
    context: &mut WorldFieldReadContextV1<'_, M, R, E, C>,
) -> Result<WorldRuleLineageValueV1, ValueTreeError> {
    let (_, present) = read_parent_fixed(context)?;
    let mut make = |part| {
        SupplyRecordKeyV1::Lineage(key).fixed_key(
            if matches!(part, Part::Present) {
                None
            } else {
                Some(part.name())
            },
            context,
        )
    };
    let keys = [
        make(Part::Present)?,
        make(Part::Target)?,
        make(Part::Fulfilled)?,
        make(Part::Reserved)?,
    ];
    let roots = [
        context.lookup(&keys[0].0)?,
        context.lookup(&keys[1].0)?,
        context.lookup(&keys[2].0)?,
        context.lookup(&keys[3].0)?,
    ];
    if let Some(value) = shape(present, &roots)? {
        return Ok(value);
    }
    marker(
        &roots[0].unwrap(),
        &keys[0].0,
        keys[0].1,
        &mut context.values,
        context.record_bytes,
        &mut context.cancelled,
    )?;
    let [target, fulfilled, reserved] = [
        context.quantity(&roots[1].unwrap(), &keys[1].0, keys[1].1)?,
        context.quantity(&roots[2].unwrap(), &keys[2].0, keys[2].1)?,
        context.quantity(&roots[3].unwrap(), &keys[3].0, keys[3].1)?,
    ];
    if (context.cancelled)() {
        return Err(ValueTreeError::Cancelled);
    }
    Ok(WorldRuleLineageValueV1::Value(SupplyLineageV1 {
        target,
        fulfilled,
        reserved,
    }))
}

#[path = "incremental_world_rule_lineages_overlay.rs"]
mod overlay;
pub use overlay::{
    WorldRuleLineageChangeV1, WorldRuleLineageExpectedV1, change_world_rule_lineage_overlay_v1,
    read_world_rule_lineage_overlay_v1,
};
