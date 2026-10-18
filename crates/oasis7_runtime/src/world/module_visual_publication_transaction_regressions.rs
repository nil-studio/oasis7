use super::*;
use oasis7_agent_api::world_action::GeoPos;
use crate::WorldEventBody;
use crate::cognition::{MAX_EXPECTED_VALUE_BYTES, MAX_IDENTIFIER_BYTES};
use oasis7_agent_api::world_entity::ModuleVisualAnchor;
use oasis7_agent_api::world_entity::ModuleVisualEntity;
use oasis7_wasm_abi::ModuleEmitEvent;

const MODULE_ID: &str = "fixture.visual-module";
const ENTITY_ID: &str = "module-relay";
const MODULE_VISUAL_ENTITY_TEST_CAP: usize = 4_096;

fn visual_entity(entity_id: impl Into<String>) -> ModuleVisualEntity {
    ModuleVisualEntity {
        entity_id: entity_id.into(),
        module_id: MODULE_ID.to_string(),
        kind: "relay".to_string(),
        label: Some("Existing".to_string()),
        anchor: ModuleVisualAnchor::Absolute {
            pos: GeoPos::new(100, 200, 0),
        },
    }
}

fn state_with_capped_visual_entities() -> WorldState {
    let mut state = WorldState::default();
    for index in 0..MODULE_VISUAL_ENTITY_TEST_CAP {
        let entity_id = format!("existing-{index}");
        state
            .module_visual_entities
            .insert(entity_id.clone(), visual_entity(entity_id));
    }
    state
}

fn visual_upsert_body_for(entity_id: &str, module_id: &str) -> WorldEventBody {
    WorldEventBody::ModuleEmitted(ModuleEmitEvent {
        module_id: module_id.to_string(),
        trace_id: "trace-visual-upsert".to_string(),
        kind: "ModuleVisualEntityUpserted".to_string(),
        payload: serde_json::json!({
            "entity": {
                "entity_id": entity_id,
                "module_id": module_id,
                "kind": "relay",
                "label": "Relay",
                "anchor": {
                    "type": "absolute",
                    "data": { "pos": GeoPos::new(100, 200, 0) }
                }
            }
        }),
    })
}

fn visual_upsert_body(module_id: &str) -> WorldEventBody {
    visual_upsert_body_for(ENTITY_ID, module_id)
}

fn visual_remove_body_for(entity_id: &str) -> WorldEventBody {
    WorldEventBody::ModuleEmitted(ModuleEmitEvent {
        module_id: MODULE_ID.to_string(),
        trace_id: "trace-visual-remove".to_string(),
        kind: "ModuleVisualEntityRemoved".to_string(),
        payload: serde_json::json!({ "entity_id": entity_id }),
    })
}

fn visual_remove_body() -> WorldEventBody {
    visual_remove_body_for(ENTITY_ID)
}

fn set_visual_entity_field(body: &mut WorldEventBody, field: &str, value: &str) {
    let WorldEventBody::ModuleEmitted(event) = body else {
        unreachable!()
    };
    if field == "module_id" {
        event.module_id = value.to_string();
    }
    event.payload["entity"][field] = serde_json::json!(value);
}

fn state_json(world: &World) -> serde_json::Value {
    serde_json::to_value(world.snapshot()).expect("encode runtime snapshot")
}

#[test]
fn production_module_visual_upsert_rejects_new_id_at_global_cap_atomically() {
    let mut world = World::new_with_state(state_with_capped_visual_entities());
    let before = world.snapshot();
    let journal_before = world.journal().clone();

    let error = world
        .append_event_for_test(visual_upsert_body(MODULE_ID), None)
        .expect_err("a new visual entity must be rejected at the durable map cap");

    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { reason } if reason.contains("maximum"))
    );
    assert_eq!(world.snapshot(), before);
    assert_eq!(world.journal(), &journal_before);
}

#[test]
fn production_module_visual_existing_entity_mutations_remain_available_at_global_cap() {
    let mut state = state_with_capped_visual_entities();
    state.module_visual_entities.remove("existing-0");
    state
        .module_visual_entities
        .insert(ENTITY_ID.to_string(), visual_entity(ENTITY_ID));
    let mut world = World::new_with_state(state);

    world
        .append_event_for_test(visual_upsert_body(MODULE_ID), None)
        .expect("an existing visual entity update remains valid at the cap");
    assert_eq!(
        world.state().module_visual_entities.len(),
        MODULE_VISUAL_ENTITY_TEST_CAP
    );
    assert_eq!(
        world.state().module_visual_entities[ENTITY_ID]
            .label
            .as_deref(),
        Some("Relay")
    );

    world
        .append_event_for_test(visual_remove_body(), None)
        .expect("an existing visual entity removal remains valid at the cap");
    assert_eq!(
        world.state().module_visual_entities.len(),
        MODULE_VISUAL_ENTITY_TEST_CAP - 1
    );
    assert!(!world.state().module_visual_entities.contains_key(ENTITY_ID));
}

#[test]
fn production_module_visual_legacy_snapshot_above_cap_remains_loadable_and_drainable() {
    let legacy_entity_id = "legacy-over-cap";
    let mut state = state_with_capped_visual_entities();
    state.module_visual_entities.insert(
        legacy_entity_id.to_string(),
        visual_entity(legacy_entity_id),
    );
    let encoded = serde_json::to_value(&state).expect("encode legacy visual snapshot");
    let restored: WorldState = serde_json::from_value(encoded).expect("decode legacy snapshot");
    let mut world = World::new_with_state(restored);

    assert_eq!(
        world.state().module_visual_entities.len(),
        MODULE_VISUAL_ENTITY_TEST_CAP + 1
    );
    world
        .append_event_for_test(visual_remove_body_for(legacy_entity_id), None)
        .expect("legacy visual entities remain removable after loading");
    assert_eq!(
        world.state().module_visual_entities.len(),
        MODULE_VISUAL_ENTITY_TEST_CAP
    );
}

#[test]
fn production_module_visual_text_fields_reject_over_limit_atomically() {
    for (field, max_bytes) in [
        ("entity_id", MAX_IDENTIFIER_BYTES),
        ("module_id", MAX_IDENTIFIER_BYTES),
        ("kind", MAX_IDENTIFIER_BYTES),
        ("label", MAX_EXPECTED_VALUE_BYTES),
    ] {
        let mut world = World::new();
        let before = world.snapshot();
        let journal_before = world.journal().clone();
        let mut body = visual_upsert_body(MODULE_ID);
        set_visual_entity_field(&mut body, field, &"x".repeat(max_bytes + 1));

        let error = world
            .append_event_for_test(body, None)
            .expect_err("oversized visual text must be rejected");
        assert!(
            matches!(error, WorldError::Serde(reason) if reason.contains(field) && reason.contains("bytes"))
        );
        assert_eq!(world.snapshot(), before);
        assert_eq!(world.journal(), &journal_before);
    }
}

#[test]
fn production_module_visual_text_fields_accept_exact_byte_limits() {
    let module_id = "m".repeat(MAX_IDENTIFIER_BYTES);
    let entity_id = "e".repeat(MAX_IDENTIFIER_BYTES);
    let kind = "k".repeat(MAX_IDENTIFIER_BYTES);
    let label = "l".repeat(MAX_EXPECTED_VALUE_BYTES);
    let mut body = visual_upsert_body_for(&entity_id, &module_id);
    set_visual_entity_field(&mut body, "kind", &kind);
    set_visual_entity_field(&mut body, "label", &label);
    let mut world = World::new();

    world
        .append_event_for_test(body, None)
        .expect("visual text at each exact byte limit remains valid");
    let entity = world
        .state()
        .module_visual_entities
        .get(&entity_id)
        .expect("bounded visual entity was published");
    assert_eq!(entity.entity_id.len(), MAX_IDENTIFIER_BYTES);
    assert_eq!(entity.module_id.len(), MAX_IDENTIFIER_BYTES);
    assert_eq!(entity.kind.len(), MAX_IDENTIFIER_BYTES);
    assert_eq!(
        entity.label.as_deref().map(str::len),
        Some(MAX_EXPECTED_VALUE_BYTES)
    );
}

#[test]
fn production_module_visual_upsert_remove_persist_and_replay() {
    let mut world = World::new();
    let baseline = world.snapshot();

    world
        .append_event_for_test(visual_upsert_body(MODULE_ID), None)
        .expect("publish visual upsert");
    let updated = state_json(&world);
    assert_eq!(
        updated["state"]["module_visual_entities"][ENTITY_ID]["module_id"],
        MODULE_ID
    );

    world
        .append_event_for_test(visual_remove_body(), None)
        .expect("publish visual removal");
    let removed = state_json(&world);
    assert!(
        removed["state"]["module_visual_entities"]
            .get(ENTITY_ID)
            .is_none()
    );

    let replayed = World::from_snapshot(baseline, world.journal().clone())
        .expect("replay visual mutations from baseline");
    assert!(
        state_json(&replayed)["state"]["module_visual_entities"]
            .get(ENTITY_ID)
            .is_none()
    );
}

#[test]
fn production_module_visual_payload_requires_emitter_owned_module() {
    let mut world = World::new();
    let mut body = visual_upsert_body(MODULE_ID);
    let WorldEventBody::ModuleEmitted(event) = &mut body else {
        unreachable!()
    };
    event.payload["entity"]["module_id"] = serde_json::json!("different-module");

    assert!(world.append_event_for_test(body, None).is_err());
}

#[test]
fn production_module_visual_payload_requires_module_and_anchor_fields() {
    for field in ["module_id", "anchor"] {
        let mut world = World::new();
        let mut body = visual_upsert_body(MODULE_ID);
        let WorldEventBody::ModuleEmitted(event) = &mut body else {
            unreachable!()
        };
        event.payload["entity"]
            .as_object_mut()
            .unwrap()
            .remove(field);

        assert!(
            world.append_event_for_test(body, None).is_err(),
            "missing {field} must reject the durable mutation"
        );
    }
}

#[test]
fn production_module_visual_remove_requires_existing_owner() {
    let mut world = World::new();
    world
        .append_event_for_test(visual_upsert_body(MODULE_ID), None)
        .expect("publish visual upsert");

    let mut remove = visual_remove_body();
    let WorldEventBody::ModuleEmitted(event) = &mut remove else {
        unreachable!()
    };
    event.module_id = "different-module".to_string();
    assert!(world.append_event_for_test(remove, None).is_err());
}
