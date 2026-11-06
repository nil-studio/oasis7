//! Runtime owner adapter for borrowed original rollback controls.
use super::World;
use oasis7_world_state::incremental_rollback_family_genesis::RollbackControlsRefV1;
impl World {
    pub fn borrow_iws_rollback_controls_v1(&self) -> RollbackControlsRefV1<'_> {
        RollbackControlsRefV1 {
            authority_registry: &self.rollback_authority_registry,
            consumed_nonces: &self.consumed_rollback_nonces,
            nonce_outcomes: &self.rollback_nonce_outcomes,
        }
    }
}
