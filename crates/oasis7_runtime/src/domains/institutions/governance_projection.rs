//! Deterministic governance read DTO; process configuration is not input.
use crate::{
    GovernanceValidatorAdmissionRecord, GovernanceValidatorAdmissionStatus as Status, World,
};
use oasis7_client_api::world_service::*;
use std::collections::BTreeMap;

fn public_candidate(
    record: GovernanceValidatorAdmissionRecord,
) -> PublicMembershipCandidate {
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
        status: match record.status {
            Status::Applied => MembershipCandidateStatus::Applied,
            Status::ApprovedCandidate => MembershipCandidateStatus::ApprovedCandidate,
            Status::ProbationReady => MembershipCandidateStatus::ProbationReady,
            Status::Active => MembershipCandidateStatus::Active,
            Status::Revoked => MembershipCandidateStatus::Revoked,
        },
    }
}
impl World {
    /// Read the current deterministic governance body. The caller still binds it
    /// to an authenticated committed projection; this query grants no authority.
    pub fn governance_membership_projection(&self) -> Result<FormalGovernanceMembershipV1, String> {
    let world = self;
    let candidates = world.governance_validator_admissions();
    let selected_candidates = World::governance_effective_validator_admission_records(candidates)
        .into_iter()
        .map(|(node, record)| (node, record.candidate_id.clone()))
        .collect();
    let registry = world
        .resolve_governance_effective_finality_signer_registry()
        .map_err(|e| format!("{e:?}"))?;
    let effective_registry_digest =
        oasis7_client_api::world_service::request_digest("governance-membership-registry-v1", &registry)?;
    let effective_registry = registry.map(|registry| PublicMembershipRegistry {
        slot_id: registry.slot_id,
        threshold: registry.threshold,
        threshold_bps: registry.threshold_bps,
        signer_bindings: registry.signer_bindings,
        validator_stakes: registry.validator_stakes,
    });
    Ok(FormalGovernanceMembershipV1 {
        schema_version: 1,
        state_root_hash: world
            .current_state_root_hash()
            .map_err(|e| format!("{e:?}"))?,
        governance_epoch: world.current_governance_epoch(),
        candidates: candidates
            .iter()
            .map(|(id, record)| (id.clone(), public_candidate(record.clone())))
            .collect::<BTreeMap<_, _>>(),
        selected_candidates,
        effective_registry,
        effective_registry_digest,
    })
}

}
