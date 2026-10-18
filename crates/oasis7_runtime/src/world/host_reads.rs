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
        let (local_shortage_delay_ticks, shortage_reason) = self.schedule_recipe_local_scarcity_delay_for_quote(
            factory_id, recipe_id, &plan.consume, &plan.produce,
        )?;
        Ok(RecipeExecutionReadiness { local_shortage_delay_ticks, shortage_reason })
    }
    /// Full rolling journal cursor, including the era when scalar event IDs wrap.
    pub fn last_journal_event_position(&self) -> JournalEventPosition {
        JournalEventPosition {
            era: self.observer_last_event_era(),
            event_id: self.journal.events.last().map(|event| event.id).unwrap_or_else(|| self.next_event_id.saturating_sub(1)),
        }
    }
}
