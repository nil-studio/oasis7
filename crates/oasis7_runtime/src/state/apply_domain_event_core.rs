use super::*;

#[path = "apply_domain_event_core_late.rs"]
mod late;

impl WorldState {
    pub(super) fn apply_domain_event_core(
        &mut self,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<(), WorldError> {
        if crate::state::core_policy_transition::PreparedCorePolicyEvent::supports(event) {
            crate::state::core_policy_transition::PreparedCorePolicyEvent::prepare(
                self, event, now,
            )?
            .install_infallible(self);
            return Ok(());
        }
        match event {
            DomainEvent::AgentIntentProposed { .. }
            | DomainEvent::AgentIntentSubmitted { .. }
            | DomainEvent::AgentIntentAccepted { .. }
            | DomainEvent::AgentIntentReplaced { .. }
            | DomainEvent::AgentIntentTransitioned { .. } => {
                unreachable!("Agent Intent events are handled by apply_domain_event_intent")
            }
            DomainEvent::ModuleArtifactDeployed { .. } => {
                self.prepare_module_marketplace_event(event, now)?
                    .install_infallible(self);
            }
            DomainEvent::ModuleInstalled { .. }
            | DomainEvent::ModuleUpgraded { .. }
            | DomainEvent::ModuleRollbackApplied { .. } => {
                self.prepare_module_instance_event(event, now)?
                    .install_infallible(self);
            }
            DomainEvent::ModuleReleaseRequested { .. }
            | DomainEvent::ModuleReleaseShadowed { .. }
            | DomainEvent::ModuleReleaseAttested { .. }
            | DomainEvent::ModuleReleaseRoleApproved { .. }
            | DomainEvent::ModuleReleaseRolesBound { .. }
            | DomainEvent::ModuleReleaseRejected { .. }
            | DomainEvent::ModuleReleaseApplied { .. } => {
                self.prepare_module_release_event(event, now)?
                    .install_infallible(self);
            }
            event @ DomainEvent::ModuleArtifactListed { .. }
            | event @ DomainEvent::ModuleArtifactDelisted { .. }
            | event @ DomainEvent::ModuleArtifactDestroyed { .. }
            | event @ DomainEvent::ModuleArtifactBidPlaced { .. }
            | event @ DomainEvent::ModuleArtifactBidCancelled { .. }
            | event @ DomainEvent::ModuleArtifactSaleCompleted { .. }
            | event @ DomainEvent::ResourceTransferred { .. }
            | event @ DomainEvent::DataCollected { .. }
            | event @ DomainEvent::DataCollectedAuthenticated { .. }
            | event @ DomainEvent::DataAccessGranted { .. }
            | event @ DomainEvent::DataAccessRevoked { .. }
            | event @ DomainEvent::PowerRedeemed { .. }
            | event @ DomainEvent::PowerRedeemRejected { .. }
            | event @ DomainEvent::NodePointsSettlementApplied { .. }
            | event @ DomainEvent::MainTokenGenesisInitialized { .. }
            | event @ DomainEvent::MainTokenVestingClaimed { .. }
            | event @ DomainEvent::MainTokenTransferred { .. }
            | event @ DomainEvent::MainTokenEpochIssued { .. }
            | event @ DomainEvent::MainTokenFeeSettled { .. }
            | event @ DomainEvent::MainTokenPolicyUpdateScheduled { .. }
            | event @ DomainEvent::MainTokenTreasuryDistributed { .. }
            | event @ DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp { .. }
            | event @ DomainEvent::RestrictedStarterClaimGrantIssued { .. }
            | event @ DomainEvent::RestrictedStarterClaimGrantExpired { .. }
            | event @ DomainEvent::RestrictedStarterClaimGrantRevoked { .. }
            | event @ DomainEvent::LogisticsRouteRegistered { .. }
            | event @ DomainEvent::LogisticsRouteAvailabilityChanged { .. }
            | event @ DomainEvent::LogisticsPathRerouted { .. }
            | event @ DomainEvent::MaterialTransferred { .. }
            | event @ DomainEvent::MaterialTransitStarted { .. }
            | event @ DomainEvent::MaterialTransitCompleted { .. }
            | event @ DomainEvent::FactoryBuildStarted { .. }
            | event @ DomainEvent::FactoryBuilt { .. }
            | event @ DomainEvent::FactoryDurabilityChanged { .. }
            | event @ DomainEvent::FactoryMaintained { .. }
            | event @ DomainEvent::FactoryRecycled { .. }
            | event @ DomainEvent::RecipeStarted { .. }
            | event @ DomainEvent::RecipeCompleted { .. }
            | event @ DomainEvent::FactoryProductionBlocked { .. }
            | event @ DomainEvent::FactoryProductionResumed { .. }
            | event @ DomainEvent::FactoryProductionPaused { .. }
            | event @ DomainEvent::MaterialProfileGoverned { .. }
            | event @ DomainEvent::ProductProfileGoverned { .. }
            | event @ DomainEvent::RecipeProfileGoverned { .. }
            | event @ DomainEvent::FactoryProfileGoverned { .. } => {
                self.apply_domain_event_core_late(event, now)?;
            }
            _ => unreachable!("apply_domain_event_core received unsupported event variant"),
        }
        Ok(())
    }
}
