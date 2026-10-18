use super::*;

impl WorldState {
    pub(super) fn apply_domain_event_industry(
        &mut self,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<(), WorldError> {
        match event {
            DomainEvent::LogisticsRouteRegistered { .. }
            | DomainEvent::LogisticsRouteAvailabilityChanged { .. } => {
                super::industry_transition::PreparedLogisticsTopology::prepare(self, event, now)?
                    .install(self);
            }
            DomainEvent::LogisticsPathRerouted { .. } => {}
            DomainEvent::MaterialTransferred { .. } => {
                super::industry_transition::PreparedMaterialTransfer::prepare(self, event, now)?
                    .install(self);
            }
            DomainEvent::MaterialTransitStarted { .. }
            | DomainEvent::MaterialTransitCompleted { .. } => {
                super::industry_transition::PreparedMaterialTransit::prepare(self, event, now)?
                    .install(self);
            }
            DomainEvent::FactoryBuildStarted { .. }
            | DomainEvent::FactoryBuilt { .. }
            | DomainEvent::FactoryDurabilityChanged { .. }
            | DomainEvent::FactoryMaintained { .. }
            | DomainEvent::FactoryRecycled { .. } => {
                super::industry_transition::PreparedFactoryLifecycle::prepare(self, event, now)?
                    .install(self);
            }
            DomainEvent::RecipeStarted { .. }
            | DomainEvent::RecipeCompleted { .. }
            | DomainEvent::FactoryProductionBlocked { .. }
            | DomainEvent::FactoryProductionResumed { .. }
            | DomainEvent::FactoryProductionPaused { .. } => {
                super::industry_transition::PreparedRecipeLifecycle::prepare(self, event, now)?
                    .install(self);
            }
            _ => unreachable!("apply_domain_event_industry received unsupported event variant"),
        }
        Ok(())
    }
}
