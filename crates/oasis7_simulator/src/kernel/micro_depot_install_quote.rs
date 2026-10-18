use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::super::types::{ResourceKind, ResourceStock};
use super::micro_depot::{
    MICRO_DEPOT_UPKEEP_DATA_COST, MicroDepotActionContext, MicroDepotActionKind,
    MicroDepotDecision, MicroDepotEffectPreview, MicroDepotEvalInput, MicroDepotFacilityContext,
    MicroDepotPlayerContext, MicroDepotPressureClass, MicroDepotRegionContext, MicroDepotStatus,
    ensure_micro_depot_supported_resources, micro_depot_resource_debits,
    micro_depot_resource_kind_label,
};
use super::micro_depot_commissioning::MicroDepotCommissioning;
use super::types::{MicroDepotProposalResourceDebit, MicroDepotResourceDebit};
use super::{Action, WorldKernel};

const MICRO_DEPOT_FORECAST_STATUS_INSUFFICIENT: &str = "insufficient";
const MICRO_DEPOT_FORECAST_BASIS_UNAVAILABLE: &str = "canonical regional-pressure forecast evidence unavailable: current runtime has no canonical forecast snapshot or horizon";

/// A runtime-derived, prospective install projection. This is intentionally
/// separate from the install event: obtaining a quote never reserves resources
/// or creates a facility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicroDepotInstallQuote {
    pub deployable: bool,
    pub install_cost_resources: Vec<MicroDepotResourceDebit>,
    pub commissioning_sink_resources: Vec<MicroDepotResourceDebit>,
    pub commissioned_inventory_by_kind: BTreeMap<String, i64>,
    pub upkeep_per_epoch_resources: Vec<MicroDepotResourceDebit>,
    pub upkeep_horizon_epochs: i64,
    pub projected_commissioning_service_uses: i64,
    pub break_even_uses: i64,
    #[serde(default = "default_micro_depot_forecast_status")]
    pub forecast_status: String,
    #[serde(default = "default_micro_depot_forecast_basis")]
    pub forecast_basis: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_future_blockers_covered: Option<i64>,
    pub low_use_warning: bool,
    pub recommendation_reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_blocker_effect: Option<MicroDepotEffectPreview>,
    #[serde(default)]
    pub remaining_blockers: Vec<String>,
    #[serde(default)]
    pub reasons: Vec<String>,
}

impl WorldKernel {
    pub fn micro_depot_install_quote(
        &self,
        action: &Action,
    ) -> Result<MicroDepotInstallQuote, String> {
        let Action::InstallMicroDepot {
            installer_agent_id,
            facility_id,
            location_id,
            owner_claim_id,
            regional_blocker_receipt_id,
            module_id,
            module_version,
            wasm_hash,
            entrypoint,
            service_radius_cm,
            supported_resource_kinds,
        } = action
        else {
            return Err("micro_depot install quote requires InstallMicroDepot action".to_string());
        };

        let commissioning = MicroDepotCommissioning::v2();
        let upkeep_per_epoch_resources = vec![MicroDepotResourceDebit {
            kind: ResourceKind::Data,
            amount: MICRO_DEPOT_UPKEEP_DATA_COST,
        }];
        let upkeep_horizon_epochs = self
            .model
            .agents
            .get(installer_agent_id)
            .map(|agent| {
                micro_depot_post_install_upkeep_horizon_epochs(
                    &agent.resources,
                    &commissioning.install_cost_resources,
                    &upkeep_per_epoch_resources,
                )
            })
            .unwrap_or(0);
        let mut reasons = Vec::new();

        if self.model.regional_infrastructure.contains_key(facility_id) {
            reasons.push(format!("facility already exists: {facility_id}"));
        }
        if !self.model.agents.contains_key(installer_agent_id) {
            reasons.push(format!("installer agent not found: {installer_agent_id}"));
        }
        if !self.model.locations.contains_key(location_id) {
            reasons.push(format!("install location not found: {location_id}"));
        }
        if owner_claim_id.trim().is_empty()
            || regional_blocker_receipt_id.trim().is_empty()
            || module_id.trim().is_empty()
            || module_version.trim().is_empty()
            || wasm_hash.trim().is_empty()
            || entrypoint.trim().is_empty()
            || *service_radius_cm <= 0
        {
            reasons.push("install requires owner claim, regional blocker receipt, module identity, entrypoint, and positive radius".to_string());
        }
        if supported_resource_kinds.as_slice() != ["data"] {
            reasons.push(
                "invalid supported resource kinds; canonical micro_depot resources are [data]"
                    .to_string(),
            );
        }
        if self.model.regional_infrastructure.values().any(|facility| {
            facility.kind == "micro_depot"
                && facility.location_id == *location_id
                && facility.owner_claim_id == *owner_claim_id
        }) {
            reasons.push(format!(
                "micro_depot already exists for claim {owner_claim_id} at location {location_id}"
            ));
        }
        if let Some(stock) = self
            .model
            .agents
            .get(installer_agent_id)
            .map(|agent| &agent.resources)
        {
            for debit in &commissioning.install_cost_resources {
                let available = stock.get(debit.kind);
                if available < debit.amount {
                    reasons.push(format!(
                        "insufficient install resources: {} requires {} but only {} available",
                        micro_depot_resource_kind_label(debit.kind),
                        debit.amount,
                        available
                    ));
                }
            }
        }

        let prospective_preview = if let Some(evaluator) = self.rule_hooks.micro_depot_wasm.as_ref()
        {
            let input = MicroDepotEvalInput {
                schema_version: "micro_depot.eval.v2".to_string(),
                module_id: module_id.clone(),
                module_version: module_version.clone(),
                action: MicroDepotActionContext {
                    action_id: self.next_action_id,
                    kind: MicroDepotActionKind::Repair,
                    target_id: location_id.clone(),
                    base_cost_class: MicroDepotPressureClass::Medium,
                    base_risk_class: MicroDepotPressureClass::Low,
                    blocker_type: Some(regional_blocker_receipt_id.clone()),
                },
                player: MicroDepotPlayerContext {
                    account_id: self
                        .player_binding_for_agent(installer_agent_id)
                        .unwrap_or(installer_agent_id)
                        .to_string(),
                    claim_id: owner_claim_id.clone(),
                },
                depot: MicroDepotFacilityContext {
                    facility_id: facility_id.clone(),
                    status: MicroDepotStatus::Active,
                    owner_claim_id: owner_claim_id.clone(),
                    capacity_class: MicroDepotPressureClass::Medium,
                    inventory_revision: 0,
                    current_epoch: 0,
                    available_units_by_kind: commissioning.inventory_by_kind.clone(),
                    throughput_remaining_units: commissioning.throughput_remaining_units,
                    throughput_limit_units_per_epoch: commissioning
                        .throughput_limit_units_per_epoch,
                    supported_resource_kinds: supported_resource_kinds.clone(),
                    service_radius_cm: *service_radius_cm,
                    upkeep_paid: true,
                },
                context: MicroDepotRegionContext {
                    distance_to_target_cm: 0,
                    resource_gap_class: MicroDepotPressureClass::Medium,
                    route_pressure_class: MicroDepotPressureClass::Medium,
                    region_pressure_class: MicroDepotPressureClass::Medium,
                },
            };
            match evaluator(&input) {
                Ok(preview)
                    if preview.module_id == *module_id
                        && preview.wasm_hash == *wasm_hash
                        && preview.entrypoint == *entrypoint
                        && preview.proposal.decision == MicroDepotDecision::Applicable =>
                {
                    Some(preview)
                }
                Ok(preview) => {
                    reasons.push(format!(
                        "micro_depot prospective quote is not applicable or does not match configured module identity: {:?}",
                        preview.proposal.decision
                    ));
                    None
                }
                Err(error) => {
                    reasons.push(format!("micro_depot prospective quote failed: {error}"));
                    None
                }
            }
        } else {
            reasons.push("micro_depot wasm evaluator is not configured".to_string());
            None
        };
        let expected_blocker_effect = prospective_preview
            .as_ref()
            .map(|preview| preview.effect.clone());
        let (projected_commissioning_service_uses, break_even_uses) = prospective_preview
            .as_ref()
            .map(|preview| {
                micro_depot_resource_debits(&preview.proposal).and_then(|debits| {
                    ensure_micro_depot_supported_resources(supported_resource_kinds, &debits)?;
                    micro_depot_projected_commissioning_uses(&commissioning, &debits)
                })
            })
            .map_or_else(
                || (0, 0),
                |result| match result {
                    Ok(projection) => projection,
                    Err(reason) => {
                        reasons.push(reason);
                        (0, 0)
                    }
                },
            );
        if projected_commissioning_service_uses == 0
            && prospective_preview.is_some()
            && reasons.is_empty()
        {
            reasons.push(
                "prospective service debit cannot be supported by commissioning inventory"
                    .to_string(),
            );
        }
        // The current runtime has no canonical regional-pressure forecast snapshot or horizon.
        // Keep capacity and break-even values inspectable, but do not present them as strategic
        // evidence until a deterministic forecast provider can bind them to regional state.
        let forecast_status = default_micro_depot_forecast_status();
        let forecast_basis = default_micro_depot_forecast_basis();
        let forecast_insufficient = forecast_status == MICRO_DEPOT_FORECAST_STATUS_INSUFFICIENT;
        let low_use_warning = projected_commissioning_service_uses < break_even_uses
            || projected_commissioning_service_uses == 0
            || forecast_insufficient;
        let recommendation_reason = if forecast_insufficient {
            format!(
                "not recommended: {forecast_basis}; projected uses {projected_commissioning_service_uses} vs break_even {break_even_uses}"
            )
        } else if projected_commissioning_service_uses == 0 {
            "not recommended: prospective commissioning service uses are zero".to_string()
        } else if low_use_warning {
            format!(
                "low-use warning: projected uses {projected_commissioning_service_uses} are below break_even {break_even_uses}"
            )
        } else {
            format!(
                "recommended: projected uses {projected_commissioning_service_uses} meet break_even {break_even_uses}"
            )
        };

        let mut quote = MicroDepotInstallQuote {
            deployable: false,
            install_cost_resources: commissioning.install_cost_resources,
            commissioning_sink_resources: commissioning.sink_resources,
            commissioned_inventory_by_kind: commissioning.inventory_by_kind,
            upkeep_per_epoch_resources,
            upkeep_horizon_epochs,
            projected_commissioning_service_uses,
            break_even_uses,
            forecast_status,
            forecast_basis,
            expected_future_blockers_covered: None,
            low_use_warning,
            recommendation_reason,
            expected_blocker_effect,
            remaining_blockers: vec![regional_blocker_receipt_id.clone()],
            reasons,
        };
        quote.deployable =
            quote.reasons.is_empty() && quote.projected_commissioning_service_uses > 0;
        Ok(quote)
    }
}

fn default_micro_depot_forecast_status() -> String {
    MICRO_DEPOT_FORECAST_STATUS_INSUFFICIENT.to_string()
}

fn default_micro_depot_forecast_basis() -> String {
    MICRO_DEPOT_FORECAST_BASIS_UNAVAILABLE.to_string()
}

fn micro_depot_post_install_upkeep_horizon_epochs(
    resources: &ResourceStock,
    install_cost_resources: &[MicroDepotResourceDebit],
    upkeep_per_epoch_resources: &[MicroDepotResourceDebit],
) -> i64 {
    let install_debit_for_kind = |kind| {
        install_cost_resources
            .iter()
            .filter(|debit| debit.kind == kind)
            .fold(0_i64, |total, debit| total.saturating_add(debit.amount))
    };

    if install_cost_resources
        .iter()
        .any(|debit| resources.get(debit.kind) < install_debit_for_kind(debit.kind))
    {
        return 0;
    }

    upkeep_per_epoch_resources
        .iter()
        .map(|upkeep| {
            if upkeep.amount <= 0 {
                return 0;
            }
            let post_install_balance = resources
                .get(upkeep.kind)
                .saturating_sub(install_debit_for_kind(upkeep.kind));
            post_install_balance / upkeep.amount
        })
        .min()
        .unwrap_or(0)
}

fn micro_depot_projected_commissioning_uses(
    commissioning: &MicroDepotCommissioning,
    debits: &[MicroDepotProposalResourceDebit],
) -> Result<(i64, i64), String> {
    if debits.is_empty() || debits.iter().any(|debit| debit.units <= 0) {
        return Err("prospective service debit is zero or inapplicable".to_string());
    }
    let total_debit = debits
        .iter()
        .try_fold(0_i64, |total, debit| total.checked_add(debit.units))
        .ok_or_else(|| "prospective service debit total overflow".to_string())?;
    let throughput_uses = commissioning
        .throughput_remaining_units
        .checked_div(total_debit)
        .ok_or_else(|| "prospective service debit is zero or inapplicable".to_string())?;
    let inventory_uses = debits
        .iter()
        .map(|debit| {
            commissioning
                .inventory_by_kind
                .get(micro_depot_resource_kind_label(debit.resource_kind))
                .copied()
                .unwrap_or(0)
                / debit.units
        })
        .min()
        .unwrap_or(0);
    let install_cost = commissioning
        .install_cost_resources
        .iter()
        .try_fold(0_i64, |total, debit| total.checked_add(debit.amount))
        .ok_or_else(|| "micro_depot install cost overflow".to_string())?;
    let break_even_uses = install_cost
        .checked_add(total_debit - 1)
        .and_then(|cost| cost.checked_div(total_debit))
        .ok_or_else(|| "prospective service debit is zero or inapplicable".to_string())?;
    Ok((throughput_uses.min(inventory_uses), break_even_uses))
}
