//! Private read projection tests; synthetic graph records are never execution proof.
use super::*;
use sha2::{Digest, Sha256};

struct ProviderScope {
    previous_llm: Vec<(&'static str, Option<std::ffi::OsString>)>,
    _mock: super::auth_actions::RuntimeLiveMockHttpServer,
    _environment: std::sync::MutexGuard<'static, ()>,
}
impl Drop for ProviderScope {
    fn drop(&mut self) {
        clear_runtime_provider_env();
        for (name, previous) in &self.previous_llm {
            // SAFETY: the shared environment guard remains held through drop.
            unsafe {
                match previous {
                    Some(value) => oasis7::env_mut::set_var(name, value),
                    None => oasis7::env_mut::remove_var(name),
                }
            }
        }
    }
}

fn owner_fixture() -> (ViewerRuntimeLiveServer, String, ProviderScope) {
    let environment = runtime_provider_env_lock()
        .lock()
        .expect("provider environment lock");
    clear_runtime_provider_env();
    // The owner fixture initializes its provider while accepting a genuine
    // signed starter claim. Keep all configuration local for the entire read.
    let mock = super::auth_actions::spawn_runtime_live_mock_http_server_until_drop(|request| {
        super::auth_actions::MockHttpResponse {
            status_code: if request.path.contains("feedback") { 200 } else { 500 },
            body: serde_json::json!({"ok":request.path.contains("feedback"),"error":"read_only_fixture_does_not_execute_provider_decisions"}).to_string(),
        }
    });
    let previous_llm = [
        crate::simulator::ENV_LLM_BASE_URL,
        crate::simulator::ENV_LLM_MODEL,
        crate::simulator::ENV_LLM_API_KEY,
    ]
    .into_iter()
    .map(|name| (name, std::env::var_os(name)))
    .collect();
    // SAFETY: this scope holds the shared provider environment lock until drop.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, mock.base_url());
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
        oasis7::env_mut::set_var(crate::simulator::ENV_LLM_BASE_URL, mock.base_url());
        oasis7::env_mut::set_var(crate::simulator::ENV_LLM_MODEL, "read-only-local-fixture");
        oasis7::env_mut::set_var(crate::simulator::ENV_LLM_API_KEY, "local-fixture-key");
    }
    let provider = ProviderScope {
        previous_llm,
        _mock: mock,
        _environment: environment,
    };
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
    (server, agent, provider)
}

#[test]
fn selected_bound_agent_does_not_grant_anonymous_private_substitution_read() {
    let (mut server, agent, _provider) = owner_fixture();
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
    let (mut server, agent, _provider) = owner_fixture();
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
    let (mut server, agent, _provider) = owner_fixture();
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
