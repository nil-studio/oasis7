use super::*;

impl WorldState {
    pub(super) fn apply_domain_event_governance_meta(
        &mut self,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<(), WorldError> {
        crate::world::governance_meta_publication::PreparedGovernanceMetaEvent::prepare(
            self, event, now,
        )?
        .install_infallible(self);
        Ok(())
    }
}
