use super::init::WorldInitError;
use super::module_visual::{ModuleVisualAnchor, ModuleVisualEntity};
use super::world_model::{WorldConfig, WorldModel};

pub(super) fn ensure_module_visual_anchor_exists(
    model: &WorldModel,
    config: &WorldConfig,
    entity: &ModuleVisualEntity,
) -> Result<(), WorldInitError> {
    match &entity.anchor {
        ModuleVisualAnchor::Agent { agent_id } => {
            if model.agents.contains_key(agent_id) {
                Ok(())
            } else {
                Err(WorldInitError::ModuleVisualEntityAnchorNotFound {
                    entity_id: entity.entity_id.clone(),
                    anchor: entity.anchor.clone(),
                })
            }
        }
        ModuleVisualAnchor::Location { location_id } => {
            if model.locations.contains_key(location_id) {
                Ok(())
            } else {
                Err(WorldInitError::ModuleVisualEntityAnchorNotFound {
                    entity_id: entity.entity_id.clone(),
                    anchor: entity.anchor.clone(),
                })
            }
        }
        ModuleVisualAnchor::Absolute { pos } => {
            if config.space.contains(*pos) {
                Ok(())
            } else {
                Err(WorldInitError::ModuleVisualEntityAnchorNotFound {
                    entity_id: entity.entity_id.clone(),
                    anchor: entity.anchor.clone(),
                })
            }
        }
    }
}

pub(super) fn insert_module_visual_entity(
    model: &mut WorldModel,
    entity: ModuleVisualEntity,
) -> Result<(), WorldInitError> {
    if model
        .module_visual_entities
        .contains_key(entity.entity_id.as_str())
    {
        return Err(WorldInitError::ModuleVisualEntityIdConflict {
            entity_id: entity.entity_id,
        });
    }
    model
        .module_visual_entities
        .insert(entity.entity_id.clone(), entity);
    Ok(())
}
