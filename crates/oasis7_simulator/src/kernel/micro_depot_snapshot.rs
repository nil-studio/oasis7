use super::super::world_model::WorldModel;
use super::WorldKernel;
use super::micro_depot::MicroDepotPlayerFacilitySnapshot;
use super::micro_depot_validation::measured_micro_depot_inventory_depleted;

impl WorldKernel {
    pub fn micro_depot_player_facility_snapshots(&self) -> Vec<MicroDepotPlayerFacilitySnapshot> {
        Self::micro_depot_player_facility_snapshots_from_model(&self.model)
    }

    pub(crate) fn micro_depot_player_facility_snapshots_from_model(
        model: &WorldModel,
    ) -> Vec<MicroDepotPlayerFacilitySnapshot> {
        let mut snapshots: Vec<_> = model
            .regional_infrastructure
            .values()
            .filter(|facility| facility.kind == "micro_depot")
            .map(|facility| {
                let mut available_actions = Vec::new();
                if measured_micro_depot_inventory_depleted(facility) {
                    available_actions.push("reclaim_micro_depot".to_string());
                } else if facility.status == "active" && facility.upkeep_paid {
                    available_actions.push("evaluate_micro_depot_quote".to_string());
                    available_actions.push("service_micro_depot_repair".to_string());
                    available_actions.push("service_micro_depot_logistics".to_string());
                    available_actions.push("suspend_micro_depot".to_string());
                } else if facility.status == "suspended" || !facility.upkeep_paid {
                    available_actions.push("pay_micro_depot_upkeep".to_string());
                    available_actions.push("reclaim_micro_depot".to_string());
                }
                MicroDepotPlayerFacilitySnapshot {
                    facility_id: facility.facility_id.clone(),
                    owner_claim_id: facility.owner_claim_id.clone(),
                    status: if measured_micro_depot_inventory_depleted(facility) {
                        "depleted".to_string()
                    } else {
                        facility.status.clone()
                    },
                    location_id: facility.location_id.clone(),
                    service_radius_cm: facility.service_radius_cm,
                    inventory_revision: facility.inventory_revision,
                    available_units_by_kind: facility.available_units_by_kind.clone(),
                    throughput_epoch: facility.throughput_epoch,
                    throughput_remaining_units: facility.throughput_remaining_units,
                    throughput_limit_units_per_epoch: facility.throughput_limit_units_per_epoch,
                    supported_resource_kinds: facility.supported_resource_kinds.clone(),
                    module_id: facility.module_id.clone(),
                    module_version: facility.module_version.clone(),
                    wasm_hash: facility.wasm_hash.clone(),
                    upkeep_paid: facility.upkeep_paid,
                    last_receipt_id: facility.last_receipt_id.clone(),
                    last_proposal_hash: facility.last_proposal_hash.clone(),
                    available_actions,
                }
            })
            .collect();
        snapshots.sort_by(|left, right| left.facility_id.cmp(&right.facility_id));
        snapshots
    }
}
