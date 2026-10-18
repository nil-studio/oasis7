use super::*;

#[path = "apply_domain_event_main_token_helpers.rs"]
pub(crate) mod helpers;

impl WorldState {
    pub(super) fn apply_domain_event_main_token(
        &mut self,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<(), WorldError> {
        if matches!(
            event,
            DomainEvent::MainTokenGenesisInitialized { .. }
                | DomainEvent::MainTokenVestingClaimed { .. }
                | DomainEvent::MainTokenTransferred { .. }
                | DomainEvent::MainTokenEpochIssued { .. }
                | DomainEvent::MainTokenFeeSettled { .. }
        ) {
            crate::world::main_token_monetary_publication::PreparedMainTokenMonetaryEvent::prepare(self, event, now)?.install_infallible(self);
            return Ok(());
        }
        if matches!(
            event,
            DomainEvent::MainTokenPolicyUpdateScheduled { .. }
                | DomainEvent::MainTokenTreasuryDistributed { .. }
        ) {
            crate::world::main_token_governance_monetary_publication::PreparedMainTokenGovernanceMonetaryEvent::prepare(self, event, now)?.install_infallible(self);
            return Ok(());
        }
        if matches!(
            event,
            DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp { .. }
                | DomainEvent::RestrictedStarterClaimGrantIssued { .. }
                | DomainEvent::RestrictedStarterClaimGrantExpired { .. }
                | DomainEvent::RestrictedStarterClaimGrantRevoked { .. }
        ) {
            crate::world::main_token_restricted_claim_publication::PreparedMainTokenRestrictedClaimEvent::prepare(self, event)?.install_infallible(self);
            return Ok(());
        }
        unreachable!("apply_domain_event_main_token received unsupported event variant")
    }
}
