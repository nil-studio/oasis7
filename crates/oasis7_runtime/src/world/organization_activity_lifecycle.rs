use super::World;
use crate::{DomainEvent, WorldError, WorldEvent, WorldEventBody};
use oasis7_agent_api::world_action::{
    OrganizationContinuityCommandV1 as Command, WorldRuleCommandV1,
};

impl World {
    /// Close registered organization activity catalogs after all gameplay
    /// cycles and their settlement reducers have completed for this tick.
    pub(super) fn process_organization_activity_tick_close(
        &mut self,
    ) -> Result<Vec<WorldEvent>, WorldError> {
        let now = self.state.time;
        let Some(rules) = self.state.world_rules.as_ref() else {
            return Ok(Vec::new());
        };
        let current_binding = self.world_rule_binding();
        let registrations = rules
            .organization_continuity
            .registrations
            .values()
            .filter(|registration| {
                !registration.activity_sources.is_empty()
                    && registration.activity_observation_started_at_world_tick.is_some()
                    && registration.activity_close_cursor_world_tick < now
                    && current_binding.as_deref() == Some(registration.world_binding.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut emitted = Vec::with_capacity(registrations.len());
        for registration in registrations {
            let Some(last_event) = self.journal.events.last() else {
                // An enabled organization necessarily has a registration
                // event in the journal. Missing it is a fail-closed gap.
                continue;
            };
            let through_event_hash = crate::util::hash_json(last_event)?;
            let source_catalog_root =
                crate::world_rules::activity_catalog_root(&registration)?;
            let previous_close_hash = registration.activity_close_hash.clone();
            let close_hash = crate::world_rules::activity_close_commitment(
                &registration,
                now,
                previous_close_hash.as_deref(),
                &source_catalog_root,
                last_event.id,
                &through_event_hash,
            )?;
            let organization_digest =
                crate::util::sha256_hex(registration.organization_id.as_bytes());
            let actor_id = if self.state.agents.contains_key(&registration.founder_id) {
                registration.founder_id.clone()
            } else {
                let Some(id) = registration
                    .controller_bindings
                    .keys()
                    .find(|id| self.state.agents.contains_key(id.as_str()))
                else {
                    continue;
                };
                id.clone()
            };
            let event = DomainEvent::WorldRuleCommitted {
                action_id: 0,
                actor_id,
                request_id: format!("organization-activity-close:{}:{now}", &organization_digest[..16]),
                command: WorldRuleCommandV1::OrganizationContinuity {
                    command: Command::CloseActivityTick {
                        organization_id: registration.organization_id.clone(),
                        through_world_tick: now,
                        previous_close_hash,
                        source_catalog_root,
                        through_event_id: last_event.id,
                        through_event_hash,
                        close_hash,
                    },
                },
                governance_epoch: 0,
                world_binding: Some(registration.world_binding.clone()),
            };
            self.append_event(WorldEventBody::Domain(event), None)?;
            if let Some(event) = self.journal.events.last() {
                emitted.push(event.clone());
            }
        }
        Ok(emitted)
    }
}
