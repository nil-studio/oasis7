use super::*;
use crate::World;

pub(crate) fn origin() -> CommittedRecipeOrigin {
    CommittedRecipeOrigin {
        submission: GameplaySubmissionOrigin {
            verified_player_id: "browser-player".into(),
            public_key: "a".repeat(64),
            auth_nonce: 7,
            hosted_registration_nonce: Some("hosted-session".into()),
            requester_agent_id: "builder-a".into(),
            factory_id: "factory.test".into(),
            recipe_id: "recipe.test".into(),
        },
        consensus_action_id: 2,
        consensus_submitter_player_id: "node-transport".into(),
        action_payload_hash: "b".repeat(64),
        committed_height: 4,
        action_root: "c".repeat(64),
    }
}

#[test]
fn committed_recipe_origin_pending_snapshot_roundtrip_and_legacy_bytes() {
    let action = Action::ScheduleRecipe {
        requester_agent_id: "builder-a".into(),
        factory_id: "factory.test".into(),
        recipe_id: "recipe.test".into(),
        plan: oasis7_wasm_abi::RecipeExecutionPlan::accepted(1, vec![], vec![], vec![], 0, 1),
        logistics_route_ids: vec![],
        logistics_path_ids: vec![],
    };
    let mut world = World::new();
    let id = world
        .submit_recipe_action_with_origin(action.clone(), origin())
        .expect("submit origin");
    let snapshot = world.snapshot();
    assert_eq!(snapshot.pending_actions[0].id, id);
    assert_eq!(
        snapshot.pending_actions[0].committed_recipe_origin,
        Some(origin())
    );
    let restored =
        World::from_snapshot(snapshot, world.journal().clone()).expect("restore pending");
    assert_eq!(
        restored.snapshot().pending_actions[0].committed_recipe_origin,
        Some(origin())
    );
    let legacy = ActionEnvelope {
        id: 1,
        action,
        committed_recipe_origin: None,
    };
    let raw = serde_json::to_value(&legacy).unwrap();
    assert!(raw.get("committed_recipe_origin").is_none());
    assert_eq!(
        serde_json::from_value::<ActionEnvelope>(raw).unwrap(),
        legacy
    );
}

#[test]
fn committed_recipe_origin_rejects_action_identity_tampering() {
    let mut world = World::new();
    let action = Action::RegisterAgent {
        agent_id: "builder-a".into(),
        pos: GeoPos::new(0, 0, 0),
    };
    let before = serde_json::to_vec(&world.snapshot()).unwrap();
    assert!(
        world
            .submit_recipe_action_with_origin(action, origin())
            .is_err()
    );
    assert_eq!(serde_json::to_vec(&world.snapshot()).unwrap(), before);
}
