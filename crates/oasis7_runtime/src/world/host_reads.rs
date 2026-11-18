//! Narrow initialization and read projections used by native application adapters.
use super::*;
use crate::RecipeExecutionPlan;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeExecutionReadiness {
    pub local_shortage_delay_ticks: i64,
    pub shortage_reason: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalEventPosition {
    pub era: u64,
    pub event_id: WorldEventId,
}
impl World {
    pub(crate) fn initialize_module_visual_entities(
        &mut self,
        entities: &BTreeMap<String, ModuleVisualEntity>,
    ) -> Result<(), WorldError> {
        if !self.journal.events.is_empty() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: "module visual entities can only be initialized before publication"
                    .to_string(),
            });
        }
        let mut next = self.state.module_visual_entities.clone();
        for (entity_id, entity) in entities {
            let entity = entity.clone().sanitized();
            if entity_id.trim().is_empty()
                || entity.entity_id != entity_id.trim()
                || entity.module_id.is_empty()
            {
                return Err(WorldError::ResourceBalanceInvalid {
                    reason: format!("invalid initial module visual entity: {entity_id}"),
                });
            }
            if next.insert(entity_id.clone(), entity).is_some() {
                return Err(WorldError::ResourceBalanceInvalid {
                    reason: format!("duplicate initial module visual entity: {entity_id}"),
                });
            }
        }
        self.state.module_visual_entities = next;
        Ok(())
    }

    /// Construct an unpublished world and validate all initial visual entities atomically.
    pub fn new_with_initial_module_visuals(
        policy: ReleaseSecurityPolicy,
        entities: &BTreeMap<String, ModuleVisualEntity>,
    ) -> Result<Self, WorldError> {
        let mut world = Self::new_with_release_security_policy(policy);
        world.initialize_module_visual_entities(entities)?;
        Ok(world)
    }
    /// Read the same site-bound material/maintenance readiness used by recipe admission.
    pub fn quote_recipe_execution_readiness(
        &self,
        factory_id: &str,
        recipe_id: &str,
        plan: &RecipeExecutionPlan,
    ) -> Result<RecipeExecutionReadiness, String> {
        let (local_shortage_delay_ticks, shortage_reason) = self
            .schedule_recipe_local_scarcity_delay_for_quote(
                factory_id,
                recipe_id,
                &plan.consume,
                &plan.produce,
            )?;
        Ok(RecipeExecutionReadiness {
            local_shortage_delay_ticks,
            shortage_reason,
        })
    }
    /// Full rolling journal cursor, including the era when scalar event IDs wrap.
    pub fn last_journal_event_position(&self) -> JournalEventPosition {
        JournalEventPosition {
            era: self.observer_last_event_era(),
            event_id: self
                .journal
                .events
                .last()
                .map(|event| event.id)
                .unwrap_or_else(|| self.next_event_id.saturating_sub(1)),
        }
    }

    /// Exact scalar inputs used by the committed projection's journal bounds.
    /// The era is the original next-event counter, before the projection's own
    /// wrap normalization; the observer position has different zero-ID rules.
    /// Reading these DATA fields neither clones a Snapshot nor grants a scope.
    pub fn formal_projection_journal_counters_v1(&self) -> (WorldEventId, u64) {
        (
            self.journal
                .events
                .last()
                .map(|event| event.id)
                .unwrap_or_else(|| self.next_event_id.saturating_sub(1)),
            self.next_event_id_era,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formal_projection_raw_counters_match_snapshot_at_wrap_and_empty_journal() {
        let mut world = World::new();
        for (next, era, last) in [
            (1, 0, None),
            (0, 3, None),
            (1, 3, Some(0)),
            (0, 3, Some(u64::MAX)),
            (10, 4, Some(9)),
        ] {
            world.next_event_id = next;
            world.next_event_id_era = era;
            world.journal.events.clear();
            if let Some(id) = last {
                world.journal.events.push(WorldEvent {
                    id,
                    time: 0,
                    caused_by: None,
                    body: WorldEventBody::Domain(DomainEvent::ActionAccepted {
                        action_id: 1,
                        action_kind: "fixture".into(),
                        actor_id: "actor".into(),
                        eta_ticks: 0,
                        notes: Vec::new(),
                    }),
                });
            }
            let snapshot = world.snapshot();
            assert_eq!(
                world.formal_projection_journal_counters_v1(),
                (snapshot.last_event_id, snapshot.event_id_era)
            );
            if next == 0 && last.is_none() {
                assert_ne!(
                    world.last_journal_event_position().era,
                    snapshot.event_id_era
                );
            }
        }
    }
}
