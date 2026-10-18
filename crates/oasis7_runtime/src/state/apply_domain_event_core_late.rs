use super::*;

impl WorldState {
    pub(super) fn apply_domain_event_core_late(
        &mut self,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<(), WorldError> {
        match event {
            DomainEvent::ModuleArtifactListed { .. }
            | DomainEvent::ModuleArtifactDelisted { .. }
            | DomainEvent::ModuleArtifactDestroyed { .. }
            | DomainEvent::ModuleArtifactBidPlaced { .. }
            | DomainEvent::ModuleArtifactBidCancelled { .. }
            | DomainEvent::ModuleArtifactSaleCompleted { .. } => {
                self.prepare_module_marketplace_event(event, now)?
                    .install_infallible(self);
            }
            event @ (DomainEvent::ResourceTransferred { .. }
            | DomainEvent::DataCollected { .. }
            | DomainEvent::DataCollectedAuthenticated { .. }
            | DomainEvent::DataAccessGranted { .. }
            | DomainEvent::DataAccessRevoked { .. }) => {
                crate::world::economy_data_publication::PreparedEconomyDataEvent::prepare(
                    self, event, now,
                )?
                .install_infallible(self);
            }
            event @ (DomainEvent::PowerRedeemed { .. }
            | DomainEvent::PowerRedeemRejected { .. }) => {
                crate::world::power_redemption_publication::PreparedPowerRedemptionEvent::prepare(
                    self, event, now,
                )?
                .install_infallible(self);
            }
            event @ DomainEvent::NodePointsSettlementApplied { .. } => {
                crate::world::node_points_settlement_publication::PreparedNodePointsSettlement::prepare(self, event)?
                    .install_infallible(self);
            }
            event @ DomainEvent::MainTokenGenesisInitialized { .. }
            | event @ DomainEvent::MainTokenVestingClaimed { .. }
            | event @ DomainEvent::MainTokenTransferred { .. }
            | event @ DomainEvent::MainTokenEpochIssued { .. }
            | event @ DomainEvent::MainTokenFeeSettled { .. }
            | event @ DomainEvent::MainTokenPolicyUpdateScheduled { .. }
            | event @ DomainEvent::MainTokenTreasuryDistributed { .. }
            | event @ DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp { .. }
            | event @ DomainEvent::RestrictedStarterClaimGrantIssued { .. }
            | event @ DomainEvent::RestrictedStarterClaimGrantExpired { .. }
            | event @ DomainEvent::RestrictedStarterClaimGrantRevoked { .. } => {
                self.apply_domain_event_main_token(event, now)?;
            }
            event @ DomainEvent::LogisticsRouteRegistered { .. }
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
            | event @ DomainEvent::FactoryProductionPaused { .. } => {
                self.apply_domain_event_industry(event, now)?;
            }
            DomainEvent::ProductProfileGoverned { .. }
            | DomainEvent::RecipeProfileGoverned { .. }
            | DomainEvent::FactoryProfileGoverned { .. } => {
                self.prepare_module_release_event(event, now)?
                    .install_infallible(self);
            }
            _ => unreachable!("apply_domain_event_core_late received unsupported event"),
        }
        Ok(())
    }
}
