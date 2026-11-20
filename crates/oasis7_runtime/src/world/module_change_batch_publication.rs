//! Atomic publication for the standalone multi-`ModuleEvent` compatibility API,
//! delegated to `domains::module_lifecycle` and installed via `PreparedStagedCommit`.
use super::super::{ModuleChangeSet, ProposalId, WorldError};
use super::World;
use crate::world::prepared_base_head::WorldPreparedBaseHead;
use crate::world::transition::{DurableCommitConfirmation, PreparedStagedCommit};

impl World {
    #[cfg(any(test, feature = "test_tier_required", feature = "test-support"))]
    pub(super) fn apply_prepared_module_change_batch(
        &mut self,
        proposal_id: ProposalId,
        changes: &ModuleChangeSet,
        actor: &str,
    ) -> Result<(), WorldError> {
        let base_head = WorldPreparedBaseHead::capture(self)?;
        let Some((record, delta)) =
            crate::domains::module_lifecycle::prepare_module_change_batch_staged(
                self,
                proposal_id,
                changes,
                actor,
            )?
        else {
            return Ok(());
        };
        let prepared = PreparedStagedCommit::new(base_head, Some(record), delta);
        prepared
            .install_confirmed_unchecked_head(self, DurableCommitConfirmation::Confirmed)
            .map_err(|reject| reject.into_world_error())
    }
}
