use super::super::agent_query::MicroDepotQuoteRequest;
use super::super::types::ResourceOwner;
use super::super::world_model::RegionalInfrastructure;
use super::WorldKernel;
use super::micro_depot::{
    MicroDepotActionContext, MicroDepotEvalInput, MicroDepotFacilityContext,
    MicroDepotPressureClass, MicroDepotQuotePreview, MicroDepotRegionContext, MicroDepotStatus,
};
use super::micro_depot_validation::measured_micro_depot_inventory_depleted;
use crate::geometry::space_distance_cm;

impl WorldKernel {
    /// Evaluates a micro-depot service quote against the current snapshot without
    /// reserving inventory, changing the model, or appending a world event.
    pub fn micro_depot_quote(
        &self,
        request: &MicroDepotQuoteRequest,
    ) -> Result<MicroDepotQuotePreview, String> {
        let MicroDepotQuoteRequest {
            agent_id,
            facility_id,
            target_id,
            action_kind,
            base_cost_class,
            base_risk_class,
            blocker_type,
        } = request;
        let evaluator = self
            .rule_hooks
            .micro_depot_wasm
            .as_ref()
            .ok_or_else(|| "micro_depot wasm evaluator is not configured".to_string())?;
        if !self.model.agents.contains_key(agent_id) {
            return Err(format!("micro_depot agent not found: {agent_id}"));
        }
        let facility = self
            .model
            .regional_infrastructure
            .get(facility_id)
            .ok_or_else(|| format!("micro_depot facility not found: {facility_id}"))?;
        if facility.kind != "micro_depot" {
            return Err(format!("facility {facility_id} is not a micro_depot"));
        }
        self.ensure_micro_depot_owner(facility_id, agent_id)
            .map_err(|reason| format!("micro_depot quote denied: {reason:?}"))?;
        if facility.status != "active" || !facility.upkeep_paid {
            return Err(format!("micro_depot {facility_id} is not active"));
        }
        if measured_micro_depot_inventory_depleted(facility) {
            return Err(format!("micro_depot {facility_id} is depleted"));
        }
        let distance_to_target_cm = self
            .micro_depot_distance_to_target(facility, target_id)
            .ok_or_else(|| format!("micro_depot target not found: {target_id}"))?;
        let input = MicroDepotEvalInput {
            schema_version: "micro_depot.eval.v2".to_string(),
            module_id: facility.module_id.clone(),
            module_version: facility.module_version.clone(),
            action: MicroDepotActionContext {
                // Quotes are not submitted intents, so use a fixed ID to keep
                // their evaluator input deterministic for an unchanged snapshot.
                action_id: 0,
                kind: *action_kind,
                target_id: target_id.clone(),
                base_cost_class: *base_cost_class,
                base_risk_class: *base_risk_class,
                blocker_type: blocker_type.clone(),
            },
            player: super::micro_depot::MicroDepotPlayerContext {
                account_id: self
                    .player_binding_for_agent(agent_id)
                    .unwrap_or(agent_id.as_str())
                    .to_string(),
                claim_id: facility.owner_claim_id.clone(),
            },
            depot: MicroDepotFacilityContext {
                facility_id: facility.facility_id.clone(),
                status: MicroDepotStatus::Active,
                owner_claim_id: facility.owner_claim_id.clone(),
                capacity_class: MicroDepotPressureClass::Medium,
                inventory_revision: facility.inventory_revision,
                current_epoch: facility.throughput_epoch,
                available_units_by_kind: facility.available_units_by_kind.clone(),
                throughput_remaining_units: facility.throughput_remaining_units,
                throughput_limit_units_per_epoch: facility.throughput_limit_units_per_epoch,
                supported_resource_kinds: facility.supported_resource_kinds.clone(),
                service_radius_cm: facility.service_radius_cm,
                upkeep_paid: facility.upkeep_paid,
            },
            context: MicroDepotRegionContext {
                distance_to_target_cm,
                resource_gap_class: *base_cost_class,
                route_pressure_class: MicroDepotPressureClass::Medium,
                region_pressure_class: MicroDepotPressureClass::Medium,
            },
        };
        let preview = evaluator(&input)?;
        if preview.module_id != facility.module_id
            || preview.wasm_hash != facility.wasm_hash
            || preview.entrypoint != facility.entrypoint
        {
            return Err(format!(
                "micro_depot wasm identity mismatch for facility {facility_id}"
            ));
        }
        Ok(preview)
    }

    pub(super) fn ensure_micro_depot_owner(
        &self,
        facility_id: &str,
        agent_id: &str,
    ) -> Result<(), super::types::RejectReason> {
        let Some(facility) = self.model.regional_infrastructure.get(facility_id) else {
            return Err(super::types::RejectReason::FacilityNotFound {
                facility_id: facility_id.to_string(),
            });
        };
        match &facility.owner {
            ResourceOwner::Agent {
                agent_id: owner_agent_id,
            } if owner_agent_id == agent_id => Ok(()),
            _ => Err(super::types::RejectReason::RuleDenied {
                notes: vec![format!(
                    "agent {agent_id} does not own micro_depot {facility_id}"
                )],
            }),
        }
    }

    pub(super) fn micro_depot_distance_to_target(
        &self,
        facility: &RegionalInfrastructure,
        target_id: &str,
    ) -> Option<i64> {
        let facility_pos = self.model.locations.get(&facility.location_id)?.pos;
        let target_pos = if let Some(location) = self.model.locations.get(target_id) {
            location.pos
        } else if let Some(factory) = self.model.factories.get(target_id) {
            self.model.locations.get(&factory.location_id)?.pos
        } else {
            return None;
        };
        Some(space_distance_cm(facility_pos, target_pos))
    }
}
