//! Governance observations from a verified formal projection or an explicitly
//! scoped engineering service assertion. Neither supplies runtime installation.
use super::*;
use oasis7_runtime::{GovernanceValidatorAdmissionStatus as Status, WorldGovernanceMembershipView};

pub fn public_response(
    view: WorldGovernanceMembershipView,
    scope: Option<String>,
) -> ReadMembershipResponse {
    let candidates = view
        .candidates
        .into_iter()
        .map(|(id, record)| {
            let status = match record.status {
                Status::Applied => MembershipCandidateStatus::Applied,
                Status::ApprovedCandidate => MembershipCandidateStatus::ApprovedCandidate,
                Status::ProbationReady => MembershipCandidateStatus::ProbationReady,
                Status::Active => MembershipCandidateStatus::Active,
                Status::Revoked => MembershipCandidateStatus::Revoked,
            };
            (
                id,
                PublicMembershipCandidate {
                    candidate_id: record.candidate_id,
                    node_id: record.node_id,
                    finality_signer_public_key: record.finality_signer_public_key,
                    stake: record.stake,
                    public_manifest_hash: record.public_manifest_hash,
                    requested_at_epoch: record.requested_at_epoch,
                    last_transition_tick: record.last_transition_tick,
                    approved_at_epoch: record.approved_at_epoch,
                    activation_epoch: record.activation_epoch,
                    revoked_at_epoch: record.revoked_at_epoch,
                    status,
                },
            )
        })
        .collect();
    ReadMembershipResponse {
        contract_version: WORLD_SERVICE_CONTRACT_VERSION,
        execution_evidence_scope: scope,
        commit: view.commit,
        state_root_hash: view.state_root_hash,
        governance_epoch: view.governance_epoch,
        history_profile: view.profile,
        candidates,
        selected_candidates: view
            .selected_records
            .into_iter()
            .map(|(node, record)| (node, record.candidate_id))
            .collect(),
        effective_registry: view
            .effective_registry
            .map(|registry| PublicMembershipRegistry {
                slot_id: registry.slot_id,
                threshold: registry.threshold,
                threshold_bps: registry.threshold_bps,
                signer_bindings: registry.signer_bindings,
                validator_stakes: registry.validator_stakes,
            }),
        effective_registry_digest: view.effective_registry_digest,
        supports_safe_runtime_update: false,
        applied_consensus_membership: None,
    }
}

pub use oasis7_game_client::membership::AuthenticatedMembershipObservation;

#[cfg(test)]
#[path = "membership_tests.rs"]
mod tests;
