use super::{WorldError, WorldState};
use oasis7_agent_api::world_entity::ModuleVisualAnchor;
use oasis7_agent_api::world_entity::ModuleVisualEntity;
use oasis7_wasm_abi::ModuleEmitEvent;
use std::collections::BTreeMap;
use oasis7_agent_api::module_visual::ModuleVisualMutation;


/// Bounds the durable map serialized into snapshots and included in the
/// state-root calculation. Updates and removals do not consume capacity, so
/// old snapshots can be loaded and drained without migration.
pub(crate) const MAX_MODULE_VISUAL_ENTITIES: usize = 4_096;

pub(crate) fn parse_module_visual_emit(event: &ModuleEmitEvent) -> Result<Option<ModuleVisualMutation>, WorldError> {
    oasis7_agent_api::module_visual::parse_module_visual_emit(event).map_err(WorldError::Serde)
}

impl WorldState {
    pub(crate) fn prepare_module_visual_event(
        &self,
        event: &ModuleEmitEvent,
    ) -> Result<Option<BTreeMap<String, ModuleVisualEntity>>, WorldError> {
        self.prepare_module_visual_event_at(event, self.time)
    }

    pub(crate) fn prepare_module_visual_event_at(
        &self,
        event: &ModuleEmitEvent,
        time: super::WorldTime,
    ) -> Result<Option<BTreeMap<String, ModuleVisualEntity>>, WorldError> {
        self.prepare_module_visual_event_at_with_entities(event, time, &self.module_visual_entities)
    }

    pub(crate) fn prepare_module_visual_event_at_with_entities(
        &self,
        event: &ModuleEmitEvent,
        time: super::WorldTime,
        current_entities: &BTreeMap<String, ModuleVisualEntity>,
    ) -> Result<Option<BTreeMap<String, ModuleVisualEntity>>, WorldError> {
        let Some(mutation) = parse_module_visual_emit(event)? else {
            return Ok(None);
        };
        let module_id = event.module_id.trim();
        match mutation {
            ModuleVisualMutation::Upsert(entity) => {
                validate_anchor(self, &entity.anchor, time)?;
                if let Some(existing) = current_entities.get(entity.entity_id.as_str()) {
                    if existing.module_id != module_id {
                        return Err(WorldError::ResourceBalanceInvalid {
                            reason: format!(
                                "module visual entity {} is owned by module {}",
                                entity.entity_id, existing.module_id
                            ),
                        });
                    }
                } else if current_entities.len() >= MAX_MODULE_VISUAL_ENTITIES {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "module visual entity maximum of {MAX_MODULE_VISUAL_ENTITIES} reached"
                        ),
                    });
                }
                let mut next = current_entities.clone();
                next.insert(entity.entity_id.clone(), entity);
                Ok(Some(next))
            }
            ModuleVisualMutation::Remove(entity_id) => {
                let Some(existing) = current_entities.get(entity_id.as_str()) else {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!("module visual entity unknown: {entity_id}"),
                    });
                };
                if existing.module_id != module_id {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "module visual entity {} is owned by module {}",
                            entity_id, existing.module_id
                        ),
                    });
                }
                let mut next = current_entities.clone();
                next.remove(entity_id.as_str());
                Ok(Some(next))
            }
        }
    }

    pub(crate) fn apply_module_visual_event_at(
        &mut self,
        event: &ModuleEmitEvent,
        time: super::WorldTime,
    ) -> Result<(), WorldError> {
        if let Some(next) = self.prepare_module_visual_event_at(event, time)? {
            self.module_visual_entities = next;
        }
        Ok(())
    }
}

fn validate_anchor(
    state: &WorldState,
    anchor: &ModuleVisualAnchor,
    time: super::WorldTime,
) -> Result<(), WorldError> {
    match anchor {
        ModuleVisualAnchor::Agent { agent_id } => {
            non_empty(agent_id.as_str(), "anchor.agent_id")?;
            if !state.agents.contains_key(agent_id.as_str()) {
                return Err(WorldError::AgentNotFound {
                    agent_id: agent_id.clone(),
                });
            }
        }
        ModuleVisualAnchor::Location { location_id } => {
            let location_id = non_empty(location_id.as_str(), "anchor.location_id")?;
            super::factory_authority::require_active_location_anchor(
                &state.location_anchors,
                location_id.as_str(),
                time,
            )?;
        }
        ModuleVisualAnchor::Absolute { .. } => {}
    }
    Ok(())
}

fn non_empty(value: &str, field: &str) -> Result<String, WorldError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid_payload(field, format!("{field} is required")));
    }
    Ok(value.to_string())
}

fn invalid_payload(kind: &str, detail: impl std::fmt::Display) -> WorldError {
    WorldError::Serde(format!("invalid {kind} module visual payload: {detail}"))
}
