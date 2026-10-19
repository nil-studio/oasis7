use super::*;

fn delivered_world() -> (World, String, String) {
    let mut state = industrial_fixture();
    let id = reserve(&mut state);
    produce(&mut state, &id);
    apply(
        &mut state,
        &event(
            "owner",
            "read-delivery",
            WorldRuleCommandV1::DeliverAgentOrder {
                order_id: id.clone(),
            },
        ),
        24,
    );
    state.time = 24;
    let target = order(&state, &id).target_agent_id.clone();
    let mut world = World::new_with_state(state);
    world
        .install_capability_agent_identity("owner", "player", 1)
        .unwrap();
    (world, id, target)
}

#[test]
fn delivered_order_projects_live_owner_and_current_control_only_to_its_root() {
    let (mut world, id, target) = delivered_world();
    let owner = world.current_agent_owner(&target).unwrap();
    let status = world.world_rule_status(Some("owner")).unwrap();
    assert_eq!(
        status["agent_orders"][&id]["current_owner"],
        serde_json::json!({"primary_agent_id":"owner","player_id":"player"})
    );
    let control = &status["agent_orders"][&id]["current_control"];
    assert_eq!(control["status"], "active");
    assert_eq!(control["target_agent_id"], target);
    assert_eq!(control["source_id"], owner.source_id);
    assert_eq!(control["source_generation"], 1);
    assert_eq!(control["claim_owner_id"], "owner");
    assert_eq!(control["identity_generation"], 1);
    assert!(!control.to_string().contains("public_key"));
    assert!(!control.to_string().contains("owner-key"));
    world
        .install_capability_agent_identity("owner", "player", 2)
        .unwrap();
    let rotated = world.world_rule_status(Some("owner")).unwrap();
    assert_eq!(
        rotated["agent_orders"][&id]["current_control"]["identity_generation"],
        2
    );
    for reader in [None, Some("recipient"), Some(target.as_str())] {
        let public = world.world_rule_status(reader).unwrap();
        assert!(public["agent_orders"][&id]["current_owner"].is_null());
        assert!(public["agent_orders"][&id]["current_control"].is_null());
    }
}

#[test]
fn order_control_read_does_not_use_frozen_quote_after_current_claim_is_lost() {
    let (world, id, target) = delivered_world();
    let mut state = world.state().clone();
    state.agent_claims.remove(&target);
    let mut lost = World::new_with_state(state);
    lost.install_capability_agent_identity("owner", "player", 1)
        .unwrap();
    let status = lost.world_rule_status(Some("owner")).unwrap();
    assert!(status["agent_orders"][&id]["current_owner"].is_null());
    assert_eq!(
        status["agent_orders"][&id]["current_control"]["status"],
        "blocked"
    );
    lost.install_capability_agent_identity("owner", "new-player", 2)
        .unwrap();
    let revoked = lost.world_rule_status(Some("owner")).unwrap();
    assert!(revoked["agent_orders"][&id]["current_owner"].is_null());
    assert!(revoked["agent_orders"][&id]["current_control"].is_null());
}

#[test]
fn foreign_world_order_read_never_exposes_current_control_summary() {
    let (world, id, _) = delivered_world();
    let mut state = world.state().clone();
    state
        .world_rules
        .as_mut()
        .unwrap()
        .agent_orders
        .get_mut(&id)
        .unwrap()
        .quote
        .world_binding = Some("foreign-world".into());
    let mut foreign = World::new_with_state(state);
    foreign
        .install_capability_agent_identity("owner", "player", 1)
        .unwrap();
    let status = foreign.world_rule_status(Some("owner")).unwrap();
    assert!(status["agent_orders"][&id]["current_owner"].is_null());
    assert!(status["agent_orders"][&id]["current_control"].is_null());
}

#[test]
fn historical_buyer_cannot_read_reacquired_targets_new_controller() {
    let (world, id, target) = delivered_world();
    let mut state = world.state().clone();
    state.starter_oc_claims.insert(
        "recipient".into(),
        crate::gameplay_state::StarterOcClaimState {
            agent_id: "recipient".into(),
            player_id: "new-player".into(),
            public_key: Some("new-private-controller-key".into()),
            claimed_at: 1,
            ..Default::default()
        },
    );
    let claim = state.agent_claims.get_mut(&target).unwrap();
    claim.claim_owner_id = "recipient".into();
    let source = claim.control_source.as_mut().unwrap();
    source.root_agent_id = "recipient".into();
    source.player_id = "new-player".into();
    source.root_claimed_at = 1;
    source.acquisition_sequence = 2;
    source.acquired_event_digest = "a".repeat(64);
    state
        .agent_claim_acquisition_sequences
        .insert(target.clone(), 2);
    let mut reacquired = World::new_with_state(state);
    reacquired
        .install_capability_agent_identity("owner", "player", 1)
        .unwrap();
    reacquired
        .install_capability_agent_identity("recipient", "new-player", 1)
        .unwrap();
    assert_eq!(
        reacquired.current_agent_owner(&target).unwrap().player_id,
        "new-player"
    );
    let status = reacquired.world_rule_status(Some("owner")).unwrap();
    let row = &status["agent_orders"][&id];
    assert!(row["current_owner"].is_null());
    assert_eq!(row["current_control"]["status"], "blocked");
    assert!(!row.to_string().contains("new-player"));
    assert!(!row.to_string().contains("new-private-controller-key"));
}
