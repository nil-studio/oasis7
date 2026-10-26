//! Pure projection of an execution candidate, not a committed-state proof.

use crate::{GovernanceFinalitySignerRegistry, World, WorldGovernanceMembershipView};
use serde::{Deserialize, Serialize};

/// Derived material for a detached candidate World. The execution driver must
/// independently establish the parent, actions and resulting roots. This DTO
/// supplies neither a commit reference nor authentication or installation rights.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldGovernanceCandidateRegistryProjectionV2 {
    pub candidate_state_root_hash: String,
    pub candidate_time: u64,
    pub candidate_governance_epoch: u64,
    pub target_governance_epoch: u64,
    pub epoch_length_ticks: u64,
    pub effective_registry: Option<GovernanceFinalitySignerRegistry>,
    /// World-service digest (`blake3:<hex>`), using the same domain and encoding
    /// as committed registry reads. Wire consumers must explicitly decode its
    /// algorithm and bytes rather than treating this string as bare hex.
    pub effective_registry_digest: String,
    pub projection_digest: String,
}

impl World {
    /// Resolve only the governance records already present in this World at a
    /// specified future epoch. This does not predict subsequent governance
    /// actions, authenticate the World, schedule a transition, or install a set.
    /// Unlike committed membership reads, a candidate has no CommitRef to verify.
    pub fn project_governance_finality_registry_for_candidate_epoch(
        &self,
        target_epoch: u64,
    ) -> Result<WorldGovernanceCandidateRegistryProjectionV2, String> {
        let epoch_length_ticks = self.governance_execution_policy().epoch_length_ticks;
        if epoch_length_ticks == 0 {
            return Err("candidate governance epoch length must be positive".into());
        }
        let source_epoch = self.current_governance_epoch();
        if target_epoch <= source_epoch {
            return Err("candidate membership target epoch must exceed candidate epoch".into());
        }
        let candidate_state_root_hash = WorldGovernanceMembershipView::world_state_root_hash(self)?;
        let candidate_time = self.state().time;
        let effective_registry = self
            .resolve_governance_effective_finality_signer_registry_from_admissions_at_epoch(
                self.governance_validator_admissions(),
                target_epoch,
            )
            .map_err(|error| format!("{error:?}"))?;
        let effective_registry_digest = oasis7_client_api::world_service::request_digest(
            "governance-membership-registry-v1",
            &effective_registry,
        )?;
        let projection_digest = oasis7_client_api::world_service::request_digest(
            "governance-membership-candidate-registry-projection-v2",
            &(
                &candidate_state_root_hash,
                candidate_time,
                source_epoch,
                target_epoch,
                epoch_length_ticks,
                &effective_registry,
            ),
        )?;
        Ok(WorldGovernanceCandidateRegistryProjectionV2 {
            candidate_state_root_hash,
            candidate_time,
            candidate_governance_epoch: source_epoch,
            target_governance_epoch: target_epoch,
            epoch_length_ticks,
            effective_registry,
            effective_registry_digest,
            projection_digest,
        })
    }
}
