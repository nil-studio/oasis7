use std::collections::BTreeSet;

use super::super::world_model::RegionalInfrastructure;
use super::micro_depot::{
    MicroDepotDecision, MicroDepotEvalInput, MicroDepotProposal, MicroDepotStatus,
    compute_micro_depot_proposal_hash, micro_depot_resource_kind_label,
};

pub(super) fn measured_micro_depot_inventory_depleted(facility: &RegionalInfrastructure) -> bool {
    facility.measured_supply_schema_version == 2
        && !facility
            .available_units_by_kind
            .values()
            .any(|units| *units > 0)
}
use super::types::MicroDepotProposalResourceDebit;

pub(super) fn validate_micro_depot_exact_resource_debits(
    debits: &[MicroDepotProposalResourceDebit],
) -> Result<(), String> {
    let mut kinds = BTreeSet::new();
    let mut previous = None;
    for debit in debits {
        if debit.units <= 0 {
            return Err("micro_depot resource debit units must be positive".to_string());
        }
        let label = micro_depot_resource_kind_label(debit.resource_kind);
        if !kinds.insert(debit.resource_kind) {
            return Err("micro_depot resource debits must have unique kinds".to_string());
        }
        if previous.is_some_and(|value| value >= label) {
            return Err(
                "micro_depot resource debits must be in strict canonical order".to_string(),
            );
        }
        previous = Some(label);
    }
    Ok(())
}

pub(super) fn validate_micro_depot_input(input: &MicroDepotEvalInput) -> Result<(), String> {
    if input.schema_version != "micro_depot.eval.v1"
        && input.schema_version != "micro_depot.eval.v2"
    {
        return Err(format!(
            "unsupported micro_depot schema_version {}",
            input.schema_version
        ));
    }
    if input.module_id.trim().is_empty() {
        return Err("micro_depot input module_id is empty".to_string());
    }
    if input.module_version.trim().is_empty() {
        return Err("micro_depot input module_version is empty".to_string());
    }
    if input.action.target_id.trim().is_empty() {
        return Err("micro_depot action target_id is empty".to_string());
    }
    if input.player.account_id.trim().is_empty() || input.player.claim_id.trim().is_empty() {
        return Err("micro_depot player identity is incomplete".to_string());
    }
    if input.depot.facility_id.trim().is_empty() {
        return Err("micro_depot facility_id is empty".to_string());
    }
    if input.depot.owner_claim_id != input.player.claim_id {
        return Err("micro_depot owner_claim_id must match player claim_id".to_string());
    }
    if input.depot.status != MicroDepotStatus::Active {
        return Err("micro_depot facility must be active for quote evaluation".to_string());
    }
    if !input.depot.upkeep_paid {
        return Err("micro_depot upkeep must be paid for quote evaluation".to_string());
    }
    if input.depot.service_radius_cm <= 0 {
        return Err("micro_depot service_radius_cm must be positive".to_string());
    }
    if input.context.distance_to_target_cm < 0 {
        return Err("micro_depot distance_to_target_cm must be non-negative".to_string());
    }
    if input.context.distance_to_target_cm > input.depot.service_radius_cm {
        return Err("micro_depot target is outside service radius".to_string());
    }
    if input.depot.throughput_remaining_units < 0
        || input.depot.throughput_limit_units_per_epoch < 0
        || input
            .depot
            .available_units_by_kind
            .values()
            .any(|units| *units < 0)
    {
        return Err("micro_depot supply units must be non-negative".to_string());
    }
    if input.depot.throughput_remaining_units > input.depot.throughput_limit_units_per_epoch {
        return Err("micro_depot throughput remaining exceeds epoch limit".to_string());
    }
    Ok(())
}

pub(super) fn validate_micro_depot_proposal(
    input: &MicroDepotEvalInput,
    proposal: &MicroDepotProposal,
) -> Result<(), String> {
    if proposal.action_id != input.action.action_id {
        return Err(format!(
            "micro_depot proposal action_id mismatch expected {} got {}",
            input.action.action_id, proposal.action_id
        ));
    }
    if proposal.facility_id != input.depot.facility_id {
        return Err(format!(
            "micro_depot proposal facility_id mismatch expected {} got {}",
            input.depot.facility_id, proposal.facility_id
        ));
    }
    if proposal.explanation_code.trim().is_empty() {
        return Err("micro_depot proposal explanation_code is empty".to_string());
    }
    if proposal.evaluated_inventory_revision != input.depot.inventory_revision
        || proposal.evaluated_epoch != input.depot.current_epoch
    {
        return Err("micro_depot proposal supply snapshot mismatch".to_string());
    }
    validate_micro_depot_exact_resource_debits(&proposal.resource_debits)?;
    if input.schema_version == "micro_depot.eval.v1" && !proposal.resource_debits.is_empty() {
        return Err("micro_depot.eval.v1 requires class-only resource consumption".to_string());
    }
    if input.schema_version == "micro_depot.eval.v2"
        && !proposal.consumed_resource_classes.is_empty()
    {
        return Err("micro_depot.eval.v2 requires exact-only resource debits".to_string());
    }
    if input.schema_version == "micro_depot.eval.v2"
        && proposal.decision == MicroDepotDecision::Applicable
        && proposal.resource_debits.is_empty()
    {
        return Err("applicable micro_depot proposal requires resource debits".to_string());
    }
    if input.schema_version == "micro_depot.eval.v2"
        && proposal.decision != MicroDepotDecision::Applicable
        && !proposal.resource_debits.is_empty()
    {
        return Err("non-applicable micro_depot proposal cannot debit resources".to_string());
    }
    if proposal.proposal_hash.trim().is_empty() {
        return Err("micro_depot proposal_hash is empty".to_string());
    }
    let expected_hash = compute_micro_depot_proposal_hash(input, proposal)?;
    if proposal.proposal_hash != expected_hash {
        return Err(format!(
            "micro_depot proposal_hash mismatch expected {expected_hash} got {}",
            proposal.proposal_hash
        ));
    }
    Ok(())
}
