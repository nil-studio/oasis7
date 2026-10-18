use crate::WorldState;

#[test]
fn empty_starter_oc_claims_do_not_change_legacy_state_projection_json() {
    let state = WorldState::default();
    let value = serde_json::to_value(&state).expect("serialize world state");
    let object = value.as_object().expect("world state serializes as object");

    assert!(
        !object.contains_key("starter_oc_claims"),
        "empty starter OC claims must stay out of canonical state JSON so old state roots remain stable"
    );
}

#[test]
fn legacy_world_state_json_defaults_empty_starter_oc_claims() {
    let mut value = serde_json::to_value(WorldState::default()).expect("serialize world state");
    let object = value
        .as_object_mut()
        .expect("world state serializes as object");
    object.remove("starter_oc_claims");

    let decoded: WorldState = serde_json::from_value(value).expect("decode legacy world state");

    assert!(decoded.starter_oc_claims.is_empty());
}

#[test]
fn authenticated_collect_data_nonces_are_backward_compatible_and_omitted_when_empty() {
    let state = WorldState::default();
    let mut value = serde_json::to_value(&state).expect("serialize world state");
    let object = value.as_object_mut().expect("world state object");
    assert!(!object.contains_key("authenticated_collect_data_last_nonces"));

    object.remove("authenticated_collect_data_last_nonces");
    let decoded: WorldState = serde_json::from_value(value).expect("decode legacy world state");
    assert!(decoded.authenticated_collect_data_last_nonces.is_empty());
}

#[test]
fn module_visual_entities_are_backward_compatible_and_omitted_when_empty() {
    let state = WorldState::default();
    let mut value = serde_json::to_value(&state).expect("serialize world state");
    let object = value.as_object_mut().expect("world state object");
    assert!(!object.contains_key("module_visual_entities"));

    object.remove("module_visual_entities");
    let decoded: WorldState = serde_json::from_value(value).expect("decode legacy world state");
    assert!(decoded.module_visual_entities.is_empty());
}
