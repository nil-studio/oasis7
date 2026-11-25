//! Dormant root-staged cognition balance projection for #6143.
//!
//! Only a caller with a genuinely durable Stage 2 confirmation may install
//! this delta. Existing shipping cognition mutations retain their original
//! clone/persist/recovery boundary until the full monetary owner is migrated.

use super::transition::{
    BorrowedWorldView, PreparedStagedCommit, RootInstallContext, StagedTransition,
    SubsystemDelta, TransitionContext, TransitionReject,
};
use crate::WorldError;

/// Prepare the original explicit bootstrap balance without cloning World.
/// The authoritative typed economy validates the account and resource before
/// the JSON projection is staged for a single confirmed root installation.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "pending original durable monetary owner cutover")
)]
pub(super) struct StagedCognitionResourceBalance<'a> {
    pub account_id: &'a str,
    pub resource: &'a str,
    pub available: u64,
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the non-live staged balance component is awaiting its root cutover")
)]
pub(super) struct CognitionResourceBalanceDelta {
    projection: serde_json::Value,
}

impl SubsystemDelta for CognitionResourceBalanceDelta {
    fn apply_infallible(self, ctx: &mut RootInstallContext<'_>) {
        ctx.world_mut().cognition["cognition_economy"] = self.projection;
    }
}

impl StagedTransition for StagedCognitionResourceBalance<'_> {
    type Delta = CognitionResourceBalanceDelta;

    fn prepare(
        &self,
        view: &BorrowedWorldView<'_>,
        ctx: &TransitionContext,
    ) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        if ctx.tick != view.time() {
            return Err(WorldError::DistributedValidationFailed {
                reason: "staged cognition balance requires original tick context".into(),
            }
            .into());
        }
        let mut economy = view.world().cognition_economy()?;
        economy
            .set_resource_balance(self.account_id, self.resource, self.available)
            .map_err(|error| WorldError::DistributedValidationFailed {
                reason: error.code().to_string(),
            })?;
        let projection = economy
            .snapshot_json()
            .map_err(|error| WorldError::DistributedValidationFailed {
                reason: error.code().to_string(),
            })?;
        Ok(PreparedStagedCommit::new(
            view.base_head().clone(),
            None,
            CognitionResourceBalanceDelta { projection },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::World;
    use super::super::transition::DurableCommitConfirmation;

    fn prepare(
        world: &World,
        mutation: &StagedCognitionResourceBalance<'_>,
    ) -> PreparedStagedCommit<CognitionResourceBalanceDelta> {
        let view = BorrowedWorldView::new(world).expect("borrowed canonical head");
        mutation
            .prepare(&view, &TransitionContext::new(view.time(), None))
            .expect("typed economy stage")
    }

    #[test]
    fn cognition_balance_stage_refuses_unknown_then_matches_original_success() {
        let mut staged = World::new();
        let original = staged.cognition.clone();
        let mutation = StagedCognitionResourceBalance {
            account_id: "account-agent-a",
            resource: "cognition_units",
            available: 37,
        };
        let unknown = prepare(&staged, &mutation).install_confirmed(
            &mut staged,
            DurableCommitConfirmation::StatusUnknown {
                reason: "lost monetary commit acknowledgment".into(),
            },
        );
        assert!(matches!(unknown, Err(TransitionReject::StatusUnknownFenced { .. })));
        assert_eq!(staged.cognition, original);

        let mut original_live = World::new();
        original_live
            .set_cognition_resource_balance("account-agent-a", "cognition_units", 37)
            .expect("original economy writer");
        let confirmed = prepare(&staged, &mutation);
        assert_eq!(staged.cognition, original);
        confirmed
            .install_confirmed(&mut staged, DurableCommitConfirmation::Confirmed)
            .expect("simulated external confirmation, no shipping cutover");
        assert_eq!(staged.cognition, original_live.cognition);
        assert_eq!(staged.state(), original_live.state());
    }

    #[test]
    fn cognition_balance_stage_rejects_changed_money_projection_and_bad_context() {
        let mut staged = World::new();
        let mutation = StagedCognitionResourceBalance {
            account_id: "account-agent-a",
            resource: "cognition_units",
            available: 37,
        };
        let prepared = prepare(&staged, &mutation);
        staged
            .set_cognition_resource_balance("account-agent-a", "cognition_units", 1)
            .expect("distinct live monetary change");
        let before = staged.cognition.clone();
        assert!(matches!(
            prepared.install_confirmed(&mut staged, DurableCommitConfirmation::Confirmed),
            Err(TransitionReject::BaseHeadStale)
        ));
        assert_eq!(staged.cognition, before);

        let view = BorrowedWorldView::new(&staged).unwrap();
        assert!(matches!(
            mutation.prepare(&view, &TransitionContext::new(view.time() + 1, None)),
            Err(TransitionReject::World(_))
        ));
    }
}
