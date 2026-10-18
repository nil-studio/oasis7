use super::*;

#[path = "apply_domain_event_gameplay_claims.rs"]
mod claims;

impl WorldState {
    pub(super) fn apply_domain_event_gameplay(
        &mut self,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<(), WorldError> {
        match event {
            DomainEvent::StarterOcClaimed { .. } => {
                crate::world::starter_oc_claim_publication::PreparedStarterOcClaimed::prepare(
                    self, event, now,
                )?
                .install_infallible(self);
            }
            DomainEvent::EconomicContractOpened { .. }
            | DomainEvent::EconomicContractAccepted { .. }
            | DomainEvent::EconomicContractSettled { .. }
            | DomainEvent::EconomicContractExpired { .. } => {
                crate::world::economic_contract_publication::PreparedEconomicContractEvent::prepare(
                    self, event, now,
                )?
                .install_infallible(self);
            }
            DomainEvent::AgentClaimed { .. }
            | DomainEvent::AgentClaimReleaseRequested { .. }
            | DomainEvent::AgentClaimUpkeepSettled { .. }
            | DomainEvent::AgentClaimEnteredGrace { .. }
            | DomainEvent::AgentClaimIdleWarning { .. }
            | DomainEvent::AgentClaimReleased { .. }
            | DomainEvent::AgentClaimReclaimed { .. } => {
                self.apply_domain_event_gameplay_claims(event, now)?;
            }
            DomainEvent::AllianceFormed { .. }
            | DomainEvent::AllianceJoined { .. }
            | DomainEvent::AllianceLeft { .. }
            | DomainEvent::AllianceDissolved { .. }
            | DomainEvent::WarDeclared { .. }
            | DomainEvent::WarConcluded { .. } => {
                crate::world::alliance_war_publication::PreparedAllianceWarEvent::prepare(
                    self, event, now,
                )?
                .install_infallible(self);
            }
            _ => unreachable!("apply_domain_event_gameplay received unsupported event variant"),
        }
        Ok(())
    }
}
