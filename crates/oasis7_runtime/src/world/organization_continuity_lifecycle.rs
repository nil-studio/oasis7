//! Organization continuity consumes only activity and availability evidence
//! closed for the current committed tick.
use super::World;
use crate::{DomainEvent, WorldError, WorldEvent, WorldEventBody};
use oasis7_agent_api::world_action::WorldRuleCommandV1;

impl World {
    pub(super) fn process_organization_continuity_lifecycle(
        &mut self,
    ) -> Result<Vec<WorldEvent>, WorldError> {
        let now = self.state.time;
        let work = self
            .state
            .world_rules
            .as_ref()
            .map(|rules| rules.organization_continuity.lifecycle_commands(&self.state, now))
            .transpose()?
            .unwrap_or_default();
        let mut emitted = Vec::new();
        for (actor_id, request_id, command) in work {
            let event = DomainEvent::WorldRuleCommitted {
                action_id: 0,
                actor_id,
                request_id,
                command: WorldRuleCommandV1::OrganizationContinuity { command },
                governance_epoch: self.current_governance_epoch(),
                world_binding: self.world_rule_binding(),
            };
            if self.validate_manufactured_identity_acquisition(&event).is_err()
                || crate::world_rules::PreparedWorldRule::prepare(&self.state, &event, now)
                    .is_err()
            {
                continue;
            }
            self.append_event(WorldEventBody::Domain(event), None)?;
            if let Some(event) = self.journal.events.last() {
                emitted.push(event.clone());
            }
        }
        Ok(emitted)
    }
}
