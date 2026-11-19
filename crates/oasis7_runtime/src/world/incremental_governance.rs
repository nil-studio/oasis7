//! Borrow exact original governance controls for IWS producers.
use super::World;
use oasis7_world_state::incremental_runtime_governance::RuntimeGovernanceControlsRefV1;
impl World {
    pub fn borrow_iws_governance_controls_v1(&self) -> RuntimeGovernanceControlsRefV1<'_> {
        RuntimeGovernanceControlsRefV1 {
            policy: &self.governance_execution_policy,
            snapshots: &self.governance_finality_epoch_snapshots,
            penalties: &self.governance_identity_penalties,
        }
    }
}
