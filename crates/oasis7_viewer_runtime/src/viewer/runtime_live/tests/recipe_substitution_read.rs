//! Private read projection tests; synthetic graph records are never execution proof.
use super::*;
use sha2::{Digest, Sha256};

fn owner_fixture() -> (ViewerRuntimeLiveServer, String) {
    let (mut server, agent, _, _) =
        super::agent_memory_correction_control::owner_server_with_config(
            186,
            ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
                .with_decision_mode(ViewerLiveDecisionMode::Llm),
        );
    let mut state = server.world.state().clone();
    let owner = state.resolved_agent_owner(&agent).unwrap();
    let binding = hex::encode(Sha256::digest(
        serde_json::to_vec(&(
            owner.primary_agent_id,
            owner.player_id,
            owner.public_key,
            owner.source_id,
            owner.generation,
        ))
        .unwrap(),
    ));
    let reference = serde_json::json!({"world_binding":"read-fixture-world","domain":"industrial","schema":"industrial-work/v1","id":"private-root","revision":1,"digest":"a".repeat(64)});
    let root = serde_json::from_value(serde_json::json!({
        "root_ref":reference,"owner_id":agent,"owner_binding_digest":binding,
        "factory_id":"fixture-factory","pinned_recipe_id":"fixture-recipe","accepted_batches":1,
        "pinned_profile_digest":"b".repeat(64),"causal_digest":"c".repeat(64),
        "work":0,"first_sink":false,"slots":{},"allowed_dispositions":[],
        "output_ledger":format!("agent:{agent}"),"terminal_capacities":{"fixture-output":1},"started_job_id":null
    })).unwrap();
    state
        .world_rules
        .get_or_insert_with(Default::default)
        .industrial_work
        .roots
        .insert("private-root".into(), root);
    server.world = crate::runtime::World::new_with_state(state);
    (server, agent)
}

#[test]
fn selected_bound_agent_does_not_grant_anonymous_private_substitution_read() {
    let (mut server, agent) = owner_fixture();
    assert!(
        server
            .llm_sidecar
            .agent_player_bindings
            .contains_key(&agent)
    );
    let anonymous = server.compat_snapshot(None);
    assert!(
        anonymous
            .player_gameplay
            .unwrap()
            .recipe_substitution
            .is_none()
    );
    let wrong_player = server.compat_snapshot(Some("unrelated-observer"));
    assert!(
        wrong_player
            .player_gameplay
            .unwrap()
            .recipe_substitution
            .is_none()
    );
}

#[test]
fn authenticated_local_owner_gets_canonical_status_and_replaced_source_loses_old_roots() {
    let (mut server, agent) = owner_fixture();
    let owner = server.compat_snapshot(Some("agency-owner"));
    let projected = owner.player_gameplay.unwrap().recipe_substitution.unwrap();
    assert!(projected["roots"].get("private-root").is_some());
    assert_eq!(
        projected,
        server.world.world_rule_status(Some(&agent)).unwrap()["recipe_substitution"]
    );

    // Replacing the frozen economic source keeps the Agent registry key. The
    // current session may inspect current status, but never its old graph.
    let mut state = server.world.state().clone();
    state.starter_oc_claims.get_mut(&agent).unwrap().claimed_at += 1;
    server.world = crate::runtime::World::new_with_state(state);
    let replacement = server.compat_snapshot(Some("agency-owner"));
    let replacement = replacement
        .player_gameplay
        .unwrap()
        .recipe_substitution
        .unwrap();
    assert_eq!(replacement["roots"].as_object().unwrap().len(), 0);
}

#[test]
fn chain_backed_snapshot_never_falls_back_to_local_private_graph() {
    let (mut server, agent) = owner_fixture();
    assert!(
        server.world.world_rule_status(Some(&agent)).unwrap()["recipe_substitution"]["roots"]
            .get("private-root")
            .is_some()
    );
    server.config.chain_status_bind = Some("127.0.0.1:1".into());
    server.verified_world_view = None;
    let snapshot = server.compat_snapshot(Some("agency-owner"));
    assert!(
        snapshot
            .player_gameplay
            .unwrap()
            .recipe_substitution
            .is_none()
    );
}
