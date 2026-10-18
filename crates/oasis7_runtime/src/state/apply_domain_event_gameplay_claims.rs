use super::*;

impl WorldState {
    pub(super) fn apply_domain_event_gameplay_claims(
        &mut self,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<(), WorldError> {
        match event {
            DomainEvent::AgentClaimed { .. } | DomainEvent::AgentClaimUpkeepSettled { .. } => {
                crate::world::agent_claim_economic_publication::PreparedAgentClaimEconomic::prepare(
                    self, event, now,
                )?
                .install_infallible(self);
            }
            DomainEvent::AgentClaimReleased { .. } | DomainEvent::AgentClaimReclaimed { .. } => {
                crate::world::agent_claim_terminal_publication::PreparedAgentClaimTerminal::prepare(
                    self, event, now,
                )?
                .install_infallible(self);
            }
            DomainEvent::AgentClaimReleaseRequested { .. }
            | DomainEvent::AgentClaimEnteredGrace { .. }
            | DomainEvent::AgentClaimIdleWarning { .. } => {
                crate::world::agent_claim_light_lifecycle_publication::PreparedAgentClaimLightLifecycle::prepare(
                    self, event, now,
                )?
                .install_infallible(self);
            }
            _ => unreachable!(
                "apply_domain_event_gameplay_claims received unsupported event variant"
            ),
        }
        Ok(())
    }
}
