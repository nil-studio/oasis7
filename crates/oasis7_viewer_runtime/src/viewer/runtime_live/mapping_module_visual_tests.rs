use super::runtime_state_to_simulator_model;
use super::{RuntimeLlmSidecar, ViewerLiveDecisionMode};
use crate::geometry::GeoPos;
use crate::runtime::WorldState;
use oasis7_agent_api::world_entity::{ModuleVisualAnchor, ModuleVisualEntity};
use oasis7_simulator::WorldModel;

#[test]
fn runtime_state_to_simulator_model_projects_persisted_module_visual_entities() {
    let mut state = WorldState::default();
    let entity = ModuleVisualEntity {
        entity_id: "runtime-relay".to_string(),
        module_id: "fixture.visual-module".to_string(),
        kind: "relay".to_string(),
        label: Some("Relay".to_string()),
        anchor: ModuleVisualAnchor::Absolute {
            pos: GeoPos::new(100, 200, 0),
        },
    };
    state
        .module_visual_entities
        .insert(entity.entity_id.clone(), entity.clone());
    let sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Script);

    let model = runtime_state_to_simulator_model(&state, &sidecar, None);
    assert_eq!(
        model.module_visual_entities.get("runtime-relay"),
        Some(&entity)
    );
}

#[test]
fn runtime_state_to_simulator_model_does_not_resurrect_removed_seed_visuals() {
    let mut seed_model = WorldModel::default();
    seed_model.module_visual_entities.insert(
        "seed-relay".to_string(),
        ModuleVisualEntity {
            entity_id: "seed-relay".to_string(),
            module_id: "fixture.visual-module".to_string(),
            kind: "relay".to_string(),
            label: Some("Relay".to_string()),
            anchor: ModuleVisualAnchor::Absolute {
                pos: GeoPos::new(100, 200, 0),
            },
        },
    );
    let sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Script);

    let model =
        runtime_state_to_simulator_model(&WorldState::default(), &sidecar, Some(&seed_model));
    assert!(!model.module_visual_entities.contains_key("seed-relay"));
}
