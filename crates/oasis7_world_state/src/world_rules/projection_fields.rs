//! The original thirteen-field WorldRules serializer/order/skip kernel.
use super::*;
pub(super) fn emit_original_world_rule_fields_v1<O,F>(source:&WorldRulesContinuityProjection<'_>,create:F,slots:&mut dyn FnMut(crate::OriginalStateFieldSlotEventV1)->Result<(),O::Error>)->Result<O::Ok,O::Error>
where O:serde::ser::SerializeStruct,F:FnOnce(usize)->Result<O,O::Error>{
        let output = create(
            11 + usize::from(!industrial_work_is_empty(&source.base.industrial_work))
                + usize::from(!substitution_is_empty(&source.base.substitution)),
        )?;
        let mut output=crate::state::OriginalStateSlotOutputV1::new(output,slots);
        output.serialize_field("requests", &source.base.requests)?;
        output.serialize_field("governance_effects", &source.base.governance_effects)?;
        output.serialize_field("organization_cases", &source.base.organization_cases)?;
        output.serialize_field("organization_estate", &source.base.organization_estate)?;
        output.serialize_field("supply_authorizations", &source.base.supply_authorizations)?;
        output.serialize_field("supply_batches", &source.base.supply_batches)?;
        output.serialize_field("supply_lineages", &source.base.supply_lineages)?;
        output.serialize_field("supply_shipments", &source.base.supply_shipments)?;
        output.serialize_field("organization_continuity", source.organization_continuity)?;
        output.serialize_field("emergency_supply", &source.base.emergency_supply)?;
        output.serialize_field("agent_orders", &source.base.agent_orders)?;
        output.declare("industrial_work",crate::OriginalStateFieldEmissionV1::OriginalDefault)?;
        if !industrial_work_is_empty(&source.base.industrial_work) {
            output.serialize_field("industrial_work", &source.base.industrial_work)?;
        }
        output.declare("substitution",crate::OriginalStateFieldEmissionV1::OriginalDefault)?;
        if !substitution_is_empty(&source.base.substitution) {
            output.serialize_field("substitution", &source.base.substitution)?;
        }
        output.end()
}
impl Serialize for WorldRulesContinuityProjection<'_>{fn serialize<S:serde::Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{emit_original_world_rule_fields_v1(self,|count|serializer.serialize_struct("WorldRulesStateV1",count),&mut|_|Ok(()))}}
impl Serialize for WorldRulesStateV1{fn serialize<S:serde::Serializer>(&self,serializer:S)->Result<S::Ok,S::Error>{WorldRulesContinuityProjection{base:self,organization_continuity:&self.organization_continuity}.serialize(serializer)}}
impl WorldRulesStateV1{
 pub fn visit_original_serialized_fields_with_slots_v1<O,F>(&self,create:F,slots:&mut dyn FnMut(crate::OriginalStateFieldSlotEventV1)->Result<(),O::Error>)->Result<O::Ok,O::Error>
 where O:serde::ser::SerializeStruct,F:FnOnce(usize)->Result<O,O::Error>{emit_original_world_rule_fields_v1(&WorldRulesContinuityProjection{base:self,organization_continuity:&self.organization_continuity},create,slots)}
}
