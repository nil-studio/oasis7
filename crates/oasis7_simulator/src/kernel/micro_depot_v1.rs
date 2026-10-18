use serde::Serialize;
use sha2::{Digest, Sha256};

use super::super::types::ActionId;
use super::micro_depot::{
    MicroDepotActionContext, MicroDepotConsumedResourceClass, MicroDepotDecision,
    MicroDepotDeltaClass, MicroDepotEvalInput, MicroDepotPlayerContext, MicroDepotPressureClass,
    MicroDepotProposal, MicroDepotRegionContext, MicroDepotStatus,
};
use super::to_canonical_cbor;

#[derive(Serialize)]
struct LegacyMicroDepotEvalInput<'a> {
    schema_version: &'a str,
    module_id: &'a str,
    module_version: &'a str,
    action: &'a MicroDepotActionContext,
    player: &'a MicroDepotPlayerContext,
    depot: LegacyMicroDepotFacilityContext<'a>,
    context: &'a MicroDepotRegionContext,
}

#[derive(Serialize)]
struct LegacyMicroDepotFacilityContext<'a> {
    facility_id: &'a str,
    status: MicroDepotStatus,
    owner_claim_id: &'a str,
    capacity_class: MicroDepotPressureClass,
    supported_resource_kinds: &'a [String],
    service_radius_cm: i64,
    upkeep_paid: bool,
}

#[derive(Serialize)]
struct LegacyMicroDepotProposal<'a> {
    action_id: &'a ActionId,
    facility_id: &'a str,
    decision: MicroDepotDecision,
    cost_delta_class: MicroDepotDeltaClass,
    risk_delta_class: MicroDepotDeltaClass,
    wait_delta_class: MicroDepotDeltaClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    blocker_change: &'a Option<String>,
    consumed_resource_classes: &'a [MicroDepotConsumedResourceClass],
    explanation_code: &'a str,
    proposal_hash: &'a str,
}

#[derive(Serialize)]
struct LegacyMicroDepotProposalHashPayload<'a> {
    input: LegacyMicroDepotEvalInput<'a>,
    proposal: LegacyMicroDepotProposal<'a>,
}

fn project_input(input: &MicroDepotEvalInput) -> LegacyMicroDepotEvalInput<'_> {
    LegacyMicroDepotEvalInput {
        schema_version: &input.schema_version,
        module_id: &input.module_id,
        module_version: &input.module_version,
        action: &input.action,
        player: &input.player,
        depot: LegacyMicroDepotFacilityContext {
            facility_id: &input.depot.facility_id,
            status: input.depot.status,
            owner_claim_id: &input.depot.owner_claim_id,
            capacity_class: input.depot.capacity_class,
            supported_resource_kinds: &input.depot.supported_resource_kinds,
            service_radius_cm: input.depot.service_radius_cm,
            upkeep_paid: input.depot.upkeep_paid,
        },
        context: &input.context,
    }
}

fn project_proposal<'a>(
    proposal: &'a MicroDepotProposal,
    proposal_hash: &'a str,
) -> LegacyMicroDepotProposal<'a> {
    LegacyMicroDepotProposal {
        action_id: &proposal.action_id,
        facility_id: &proposal.facility_id,
        decision: proposal.decision,
        cost_delta_class: proposal.cost_delta_class,
        risk_delta_class: proposal.risk_delta_class,
        wait_delta_class: proposal.wait_delta_class,
        blocker_change: &proposal.blocker_change,
        consumed_resource_classes: &proposal.consumed_resource_classes,
        explanation_code: &proposal.explanation_code,
        proposal_hash,
    }
}

pub(super) fn input_bytes(input: &MicroDepotEvalInput) -> Result<Vec<u8>, String> {
    to_canonical_cbor(&project_input(input))
}

pub(super) fn proposal_hash(
    input: &MicroDepotEvalInput,
    proposal: &MicroDepotProposal,
) -> Result<String, String> {
    let payload = LegacyMicroDepotProposalHashPayload {
        input: project_input(input),
        proposal: project_proposal(proposal, ""),
    };
    let bytes = to_canonical_cbor(&payload)?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(&bytes))))
}
