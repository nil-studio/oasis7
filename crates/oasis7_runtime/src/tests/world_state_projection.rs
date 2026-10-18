//! RED contract for the canonical typed `WorldStateProjection`.
//!
//! The projection must borrow the canonical state when no overlay is present,
//! preserving the exact serde representation and hash.  A typed body overlay
//! may change only the selected agent's body view and activity timestamp in
//! the projection; the borrowed canonical state must remain untouched.

use super::super::{
    Action, AgentIntentV2, BodyOverlay, DomainEvent, World, WorldState, WorldStateProjection,
};
use super::pos;
use crate::agent_state::BodyKernelView;
use crate::util::hash_json;
use oasis7_agent_api::world_entity::ModuleVisualAnchor;
use oasis7_agent_api::world_entity::ModuleVisualEntity;
use std::collections::BTreeMap;

fn registered_agent_world() -> World {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: "projection-fixture-agent".to_string(),
        pos: pos(7, -11),
    });
    world
        .step()
        .expect("register the real projection fixture agent");
    world
}

#[test]
fn borrowed_no_overlay_projection_preserves_world_state_bytes_and_hash() {
    let world = registered_agent_world();

    let direct_bytes = serde_json::to_vec(world.state()).expect("serialize direct WorldState");
    let direct_hash = hash_json(world.state()).expect("hash direct WorldState");

    let projection = WorldStateProjection::borrowed(world.state());
    let projected_bytes = serde_json::to_vec(&projection)
        .expect("serialize borrowed no-overlay WorldStateProjection");
    let projected_hash = hash_json(&projection).expect("hash borrowed no-overlay projection");

    assert_eq!(
        projected_bytes, direct_bytes,
        "a borrowed no-overlay projection must preserve the canonical WorldState serde bytes"
    );
    assert_eq!(
        projected_hash, direct_hash,
        "a borrowed no-overlay projection must preserve the canonical WorldState hash"
    );
}

#[test]
fn typed_body_overlay_matches_golden_state_without_mutating_original() {
    let world = registered_agent_world();
    let agent_id = "projection-fixture-agent";
    let body_view = BodyKernelView {
        mass_kg: 321,
        radius_cm: 144,
        thrust_limit: 987,
        cross_section_cm2: 12_345,
    };
    let last_active = world.state().time.saturating_add(37);

    let original_bytes = serde_json::to_vec(world.state()).expect("serialize original state");
    let original_body_view = world
        .state()
        .agents
        .get(agent_id)
        .expect("fixture agent exists")
        .state
        .body_view
        .clone();
    let original_last_active = world
        .state()
        .agents
        .get(agent_id)
        .expect("fixture agent exists")
        .last_active;

    // Clone is deliberately restricted to the expected-result oracle.  The
    // production projection must borrow `world.state()` and apply a typed
    // overlay without cloning or mutating canonical state.
    let mut golden = world.state().clone();
    let golden_agent = golden
        .agents
        .get_mut(agent_id)
        .expect("fixture agent exists in golden state");
    golden_agent.state.body_view = body_view.clone();
    golden_agent.last_active = last_active;

    let projection = WorldStateProjection::borrowed(world.state())
        .with_body_overlay(BodyOverlay::new(agent_id, body_view, last_active));
    let projected_bytes = serde_json::to_vec(&projection).expect("serialize body overlay");
    let golden_bytes = serde_json::to_vec(&golden).expect("serialize golden state");

    assert_eq!(
        projected_bytes, golden_bytes,
        "typed body overlay must serialize exactly like the equivalent golden state"
    );
    assert_eq!(
        hash_json(&projection).expect("hash body overlay projection"),
        hash_json(&golden).expect("hash golden state"),
        "typed body overlay must produce the equivalent canonical hash"
    );

    assert_eq!(
        serde_json::to_vec(world.state()).expect("serialize state after projection"),
        original_bytes,
        "building a projection must not mutate the borrowed canonical state"
    );
    let original_agent = world
        .state()
        .agents
        .get(agent_id)
        .expect("fixture agent remains present");
    assert_eq!(original_agent.state.body_view, original_body_view);
    assert_eq!(original_agent.last_active, original_last_active);
}

#[test]
fn borrowed_module_visual_overlay_matches_golden_state_without_mutating_original() {
    let world = registered_agent_world();
    let mut module_visual_entities = BTreeMap::new();
    module_visual_entities.insert(
        "projection-fixture-visual".to_string(),
        ModuleVisualEntity {
            entity_id: "projection-fixture-visual".to_string(),
            module_id: "projection-fixture-module".to_string(),
            kind: "relay".to_string(),
            label: Some("Fixture Relay".to_string()),
            anchor: ModuleVisualAnchor::Absolute { pos: pos(17, 23) },
        },
    );

    let original_bytes = serde_json::to_vec(world.state()).expect("serialize original state");
    let mut golden = world.state().clone();
    golden.module_visual_entities = module_visual_entities.clone();

    let projection = WorldStateProjection::borrowed(world.state())
        .with_module_visual_entities_overlay(&module_visual_entities);
    let projected_bytes = serde_json::to_vec(&projection).expect("serialize visual overlay");
    let golden_bytes = serde_json::to_vec(&golden).expect("serialize golden state");

    assert_eq!(
        projected_bytes, golden_bytes,
        "borrowed visual overlay must serialize exactly like the equivalent golden state"
    );
    assert_eq!(
        hash_json(&projection).expect("hash visual overlay projection"),
        hash_json(&golden).expect("hash golden state"),
        "borrowed visual overlay must produce the equivalent canonical hash"
    );
    assert_eq!(
        serde_json::to_vec(world.state()).expect("serialize state after projection"),
        original_bytes,
        "building a visual overlay must not mutate the borrowed canonical state"
    );
}

#[test]
fn default_world_state_round_trips_through_cbor_with_omitted_optional_fields() {
    let state = WorldState::default();
    let bytes = serde_cbor::to_vec(&state).expect("serialize default WorldState as CBOR");
    let decoded: WorldState = serde_cbor::from_slice(&bytes)
        .expect("default WorldState with omitted optional fields must decode from CBOR");

    assert_eq!(decoded, state);
}

#[test]
fn registered_world_state_round_trips_through_cbor_with_omitted_agent_optionals() {
    let world = registered_agent_world();
    let mut state = world.state().clone();
    let agent = state
        .agents
        .get_mut("projection-fixture-agent")
        .expect("fixture agent exists");
    // The real registration path records activity. Normalize only this test
    // fixture so the nested AgentCell serializer exercises omitted optionals.
    agent.activity = None;
    agent.intent = None;
    assert!(agent.activity.is_none());
    assert!(agent.intent.is_none());

    let bytes = serde_cbor::to_vec(&state)
        .expect("serialize registered WorldState with omitted optionals as CBOR");
    let decoded: WorldState = serde_cbor::from_slice(&bytes)
        .expect("registered WorldState with omitted optionals must decode from CBOR");

    assert_eq!(decoded, state);
}

#[test]
fn no_overlay_projection_round_trips_through_cbor_as_world_state() {
    let world = registered_agent_world();
    let projection = WorldStateProjection::borrowed(world.state());
    let bytes =
        serde_cbor::to_vec(&projection).expect("serialize no-overlay WorldStateProjection as CBOR");
    let decoded: WorldState = serde_cbor::from_slice(&bytes)
        .expect("no-overlay WorldStateProjection must decode from CBOR");

    assert_eq!(&decoded, world.state());
}

#[test]
fn body_overlay_with_routed_mailbox_event_round_trips_through_cbor() {
    let world = registered_agent_world();
    let agent_id = "projection-fixture-agent";
    let body_view = BodyKernelView {
        mass_kg: 321,
        radius_cm: 144,
        thrust_limit: 987,
        cross_section_cm2: 12_345,
    };
    let last_active = world.state().time.saturating_add(37);
    let routed_event = DomainEvent::AgentMoved {
        agent_id: agent_id.to_string(),
        from: pos(7, -11),
        to: pos(8, -11),
    };

    let projection = WorldStateProjection::borrowed(world.state()).with_body_overlay(
        BodyOverlay::new(agent_id, body_view.clone(), last_active)
            .with_routed_domain_event(routed_event.clone()),
    );
    let bytes = serde_cbor::to_vec(&projection)
        .expect("serialize body overlay and routed mailbox projection as CBOR");
    let decoded: WorldState = serde_cbor::from_slice(&bytes)
        .expect("body overlay with routed mailbox event must decode from CBOR");
    let decoded_agent = decoded
        .agents
        .get(agent_id)
        .expect("decoded fixture agent exists");

    assert_eq!(decoded_agent.state.body_view, body_view);
    assert_eq!(decoded_agent.last_active, last_active);
    assert_eq!(decoded_agent.mailbox.back(), Some(&routed_event));

    let original_agent = world
        .state()
        .agents
        .get(agent_id)
        .expect("original fixture agent exists");
    assert_ne!(original_agent.state.body_view, body_view);
    assert_ne!(original_agent.last_active, last_active);
}

#[test]
fn populated_optional_world_state_fields_round_trip_through_cbor() {
    let mut state = WorldState::default();
    state.agent_intent_ledger.insert(
        "projection-fixture-agent".to_string(),
        AgentIntentV2 {
            schema_version: 2,
            agent_id: "projection-fixture-agent".to_string(),
            intent_id: "projection-intent".to_string(),
            kind: "test".to_string(),
            summary: "CBOR projection fixture".to_string(),
            target_id: None,
            effect_intent_id: None,
            intent_tick: None,
            world_id: None,
            reorg_epoch: None,
            authority_scope: None,
            status: "accepted".to_string(),
            source: "test".to_string(),
            logical_time: state.time,
            event_seq: 1,
            updated_at: state.time,
            receipt_ref: None,
            reason_code: None,
            reason_summary: None,
            replaced_by: None,
            actor_id: String::new(),
            request_digest: String::new(),
        },
    );

    let bytes = serde_cbor::to_vec(&state)
        .expect("serialize WorldState with populated optional fields as CBOR");
    let decoded: WorldState = serde_cbor::from_slice(&bytes)
        .expect("WorldState with populated optional fields must decode from CBOR");

    assert_eq!(decoded, state);
}
