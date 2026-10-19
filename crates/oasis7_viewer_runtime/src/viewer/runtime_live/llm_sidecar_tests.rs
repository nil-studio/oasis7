use super::*;

#[path = "llm_sidecar_legacy_wake_tests.rs"]
mod legacy_wake_tests;
#[path = "llm_sidecar_lineage_http_tests.rs"]
mod lineage_http_tests;
#[path = "llm_sidecar_lineage_tests.rs"]
mod lineage_tests;
#[path = "llm_sidecar_recovery_tests.rs"]
mod recovery_tests;

struct ProviderEnvSnapshot {
    previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
}
impl ProviderEnvSnapshot {
    fn capture(keys: &[&'static str]) -> Self {
        let previous = keys
            .iter()
            .map(|key| (*key, std::env::var_os(key)))
            .collect();
        for key in keys {
            // SAFETY: The caller holds the canonical provider env mutex.
            unsafe {
                oasis7::env_mut::remove_var(key);
            }
        }
        Self { previous }
    }
}

impl Drop for ProviderEnvSnapshot {
    fn drop(&mut self) {
        for (key, value) in self.previous.drain(..) {
            // SAFETY: The snapshot outlives the test and the caller still holds
            // the canonical provider env mutex while it is being dropped.
            unsafe {
                match value {
                    Some(value) => oasis7::env_mut::set_var(key, value),
                    None => oasis7::env_mut::remove_var(key),
                }
            }
        }
    }
}

pub(super) fn test_provider_context(
    agent_id: &str,
    agent_turn_id: &str,
    decision_request_id: &str,
    transport_attempt: u64,
) -> cognition_context::ProviderContextState {
    let request_context = crate::simulator::ContinuousAgentRequestContextV1 {
        base_decision_request: crate::simulator::DecisionRequest {
            observation: crate::simulator::ObservationEnvelope {
                agent_id: agent_id.to_string(),
                world_time: 12,
                mode: crate::simulator::ProviderExecutionMode::PlayerParity,
                observation_schema_version:
                    crate::simulator::DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION.to_string(),
                action_schema_version: crate::simulator::DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION
                    .to_string(),
                environment_class: Some("runtime_live".to_string()),
                fallback_reason: None,
                observation: crate::simulator::ProviderObservation {
                    self_state: crate::simulator::ProviderSelfState {
                        location_ref: "loc-0".to_string(),
                        pose_hint: "origin".to_string(),
                        status_flags: Vec::new(),
                        resource_summary: BTreeMap::new(),
                    },
                    mission_context: crate::simulator::ProviderMissionContext {
                        goal_summary: "test goal".to_string(),
                        blocked_reason: None,
                    },
                    nearby_entities: Vec::new(),
                    recent_events: Vec::new(),
                    local_navigation_graph: Vec::new(),
                    hazard_summary: Vec::new(),
                    interaction_targets: Vec::new(),
                },
                recent_event_summary: Vec::new(),
                memory_summary: None,
                action_catalog: Vec::new(),
                module_command_catalog: Vec::new(),
                timeout_budget_ms: 100,
            },
            provider_config_ref: None,
            agent_profile: None,
            fixture_id: None,
            replay_id: None,
            capability_catalog: None,
            capability_invocation_context: None,
            timeout_budget_ms: 100,
        },
        context_discriminator: crate::simulator::CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR.to_string(),
        context_version: crate::simulator::CONTINUOUS_AGENT_CONTEXT_VERSION,
        protocol_version: "oasis7.continuous-agent-request.v1".to_string(),
        agent_session_id: "session-test".to_string(),
        agent_turn_id: agent_turn_id.to_string(),
        decision_request_id: decision_request_id.to_string(),
        retry_seq: 1,
        transport_attempt,
        agent_subject: agent_id.to_string(),
        runtime_binding: crate::simulator::RuntimeBindingV1 {
            world_id: "world".to_string(),
            branch_id: "main".to_string(),
            finality_epoch: 0,
            finality_block_hash: None,
            finality_status: "pending".to_string(),
            base_tick: 12,
            base_world_hash: crate::simulator::Digest32::default(),
            reorg_epoch: 0,
            runtime_manifest_hash: crate::simulator::Digest32::default(),
        },
        observation_digest: crate::simulator::Digest32::default(),
        capability_catalog_digest: crate::simulator::Digest32::default(),
        capability_invocation_context_digest: crate::simulator::Digest32::default(),
        memory_snapshot_digest: crate::simulator::Digest32::default(),
        goal_snapshot_digest: crate::simulator::Digest32::default(),
        continuation_digest: crate::simulator::Digest32::default(),
        adapter_protocol_version: "test".to_string(),
        budget_contract: crate::simulator::BudgetContractV1 {
            max_latency_ms: 100,
            max_repair_attempts: 0,
            max_model_calls: 4,
            max_tool_calls: 3,
        },
        request_digest: crate::simulator::Digest32::default(),
    };
    let turn_context = crate::simulator::ContinuousAgentTurnContextV1 {
        agent_id: agent_id.to_string(),
        agent_session_id: "session-test".to_string(),
        agent_turn_id: agent_turn_id.to_string(),
        decision_request_id: decision_request_id.to_string(),
        request_digest: crate::simulator::Digest32::default(),
        memory_snapshot: crate::simulator::MemoryContextSnapshotV1::empty(agent_id),
        goal_snapshot: crate::simulator::GoalSnapshotV1::empty(),
        continuation: None,
    };
    cognition_context::ProviderContextState {
        turn_context,
        request_context,
    }
}

fn test_recipe_completion_event(
    agent_id: &str,
    event_id: u64,
) -> (crate::runtime::WorldEvent, WorldEvent) {
    let runtime_event = crate::runtime::WorldEvent {
        id: event_id,
        time: 83,
        caused_by: None,
        body: crate::runtime::WorldEventBody::Domain(
            crate::runtime::DomainEvent::RecipeCompleted {
                committed_recipe_origin: None,
                job_id: 7,
                requester_agent_id: agent_id.to_string(),
                factory_id: "factory-smelter".to_string(),
                recipe_id: "recipe.iron-ingot".to_string(),
                accepted_batches: 1,
                produce: Vec::new(),
                byproducts: Vec::new(),
                output_ledger: crate::runtime::MaterialLedgerId::world(),
                bottleneck_tags: Vec::new(),
                logistics_route_ids: Vec::new(),
                logistics_path_ids: Vec::new(),
            },
        ),
    };
    let mapped_event = WorldEvent {
        id: event_id,
        time: 83,
        kind: WorldEventKind::RuntimeEvent {
            kind: "runtime.economy.recipe_completed".to_string(),
            domain_kind: Some("recipe=recipe.iron-ingot".to_string()),
        },
        runtime_event: Some(runtime_event.clone()),
    };
    (runtime_event, mapped_event)
}

#[test]
fn bind_agent_player_emits_unbind_before_rebind_for_same_agent() {
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar
        .agent_player_bindings
        .insert("agent-1".to_string(), "player-a".to_string());
    sidecar
        .player_agent_bindings
        .insert("player-a".to_string(), "agent-1".to_string());
    sidecar
        .agent_public_key_bindings
        .insert("agent-1".to_string(), "pubkey-a".to_string());

    let events = sidecar
        .bind_agent_player("agent-1", "player-b", Some("pubkey-b"), false)
        .expect("rebind should succeed");
    assert_eq!(events.len(), 2);
    assert!(matches!(
        &events[0],
        WorldEventKind::AgentPlayerUnbound {
            agent_id,
            player_id,
            public_key
        } if agent_id == "agent-1"
            && player_id == "player-a"
            && public_key.as_deref() == Some("pubkey-a")
    ));
    assert!(matches!(
        &events[1],
        WorldEventKind::AgentPlayerBound {
            agent_id,
            player_id,
            public_key
        } if agent_id == "agent-1"
            && player_id == "player-b"
            && public_key.as_deref() == Some("pubkey-b")
    ));
    assert_eq!(
        sidecar
            .agent_player_bindings
            .get("agent-1")
            .map(String::as_str),
        Some("player-b")
    );
    assert_eq!(
        sidecar
            .player_agent_bindings
            .get("player-b")
            .map(String::as_str),
        Some("agent-1")
    );
    assert!(!sidecar.player_agent_bindings.contains_key("player-a"));
    assert_eq!(
        sidecar
            .agent_public_key_bindings
            .get("agent-1")
            .map(String::as_str),
        Some("pubkey-b")
    );
}

#[test]
fn sync_shadow_kernel_accepts_empty_synthetic_runtime_snapshot() {
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    let world = RuntimeWorld::default();

    sidecar
        .sync_shadow_kernel(&world, &WorldConfig::default())
        .expect("empty synthetic runtime snapshot should sync");

    let shadow = sidecar.shadow_kernel.as_ref().expect("shadow kernel");
    assert!(shadow.journal().is_empty());
}

#[test]
fn sync_shadow_kernel_preserves_generated_seed_locations() {
    let seed_pos = GeoPos::new(7, 8, 9);
    let mut seed_model = WorldModel::default();
    seed_model.locations.insert(
        "frag-shadow".to_string(),
        Location::new("frag-shadow", "shadow fragment", seed_pos),
    );
    let mut sidecar =
        RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm).with_runtime_seed_model(&seed_model);
    let world = RuntimeWorld::default();

    sidecar
        .sync_shadow_kernel(&world, &WorldConfig::default())
        .expect("runtime seed shadow sync");

    let shadow = sidecar.shadow_kernel.as_ref().expect("shadow kernel");
    assert!(
        shadow
            .snapshot()
            .model
            .locations
            .contains_key("frag-shadow")
    );
}
#[test]
fn recipe_completion_before_runner_registration_is_durable_and_deduplicated() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-world-event-startup-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let (runtime_event, mapped_event) = test_recipe_completion_event("agent-0", 111);
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.configure_provider_lineage_store(path.clone());
    sidecar.notify_recipe_completion_if_needed(&runtime_event, mapped_event.clone());
    sidecar.notify_recipe_completion_if_needed(&runtime_event, mapped_event);
    assert_eq!(sidecar.pending_provider_world_events.len(), 1);
    let checkpoint = std::fs::read_to_string(&path).expect("startup event checkpoint");
    assert!(checkpoint.contains("pending_provider_world_events"));
    assert!(checkpoint.contains("111"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn queued_recipe_completion_from_replaced_runtime_head_is_quarantined() {
    let (runtime_event, mapped_event) = test_recipe_completion_event("agent-0", 112);
    let old_binding = test_provider_context("agent-0", "turn-old", "request-old", 1)
        .request_context
        .runtime_binding;
    let mut new_binding = old_binding.clone();
    new_binding.branch_id = "replacement".to_string();
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.provider_lineage_binding = Some(new_binding);
    sidecar.notify_recipe_completion_with_binding(&runtime_event, mapped_event, Some(old_binding));
    assert!(sidecar.pending_provider_world_events.is_empty());
    assert_eq!(
        sidecar.provider_world_event_quarantine.len(),
        1,
        "a replacement Runtime head must not deliver an old completion"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn provider_world_event_for_missing_actor_is_quarantined_once() {
    let (runtime_event, mapped_event) = test_recipe_completion_event("agent-missing", 113);
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.runner = Some(RuntimeDecisionRunner::ProviderBacked(
        crate::simulator::AsyncAgentRunner::with_default_capacity(),
    ));

    sidecar.notify_recipe_completion_if_needed(&runtime_event, mapped_event);

    assert!(sidecar.pending_provider_world_events.is_empty());
    assert_eq!(sidecar.provider_world_event_quarantine.len(), 1);
    assert!(
        sidecar
            .provider_world_event_quarantine
            .values()
            .any(|reason| reason == "provider_actor_missing:agent-missing")
    );
}

#[test]
fn provider_feedback_compatibility_cursor_is_partitioned_by_session() {
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    let first = test_provider_context("agent-0", "turn-a", "request-a", 1).request_context;
    let mut second = first.clone();
    second.agent_session_id = "session-other".to_string();
    let first_feedback = sidecar.provider_feedback_for_request(
        &first,
        None,
        "rejected",
        None,
        None,
        Some("no_effect".to_string()),
    );
    let second_feedback = sidecar.provider_feedback_for_request(
        &second,
        None,
        "rejected",
        None,
        None,
        Some("no_effect".to_string()),
    );
    assert_eq!(first_feedback.feedback_seq, 1);
    assert_eq!(second_feedback.feedback_seq, 1);
    assert_ne!(first_feedback.feedback_id, second_feedback.feedback_id);
    assert_eq!(sidecar.provider_feedback_seq_by_session.len(), 2);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn provider_world_event_mailbox_full_remains_durable_for_retry() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-world-event-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let (runtime_event, mapped_event) = test_recipe_completion_event("agent-0", 77);
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.configure_provider_lineage_store(path.clone());
    sidecar.runner = Some(RuntimeDecisionRunner::ProviderBacked(
        crate::simulator::AsyncAgentRunner::blocking_provider_fixture("agent-0"),
    ));
    let saturated = {
        let Some(RuntimeDecisionRunner::ProviderBacked(runner)) = sidecar.runner.as_mut() else {
            panic!("provider runner fixture");
        };
        runner
            .start_turn("agent-0")
            .expect("start blocking fixture turn");
        let capacity = runner.mailbox_capacity();
        let mut full = false;
        for _ in 0..capacity.saturating_add(1) {
            match runner.notify_world_event("agent-0", mapped_event.clone()) {
                Ok(()) => {}
                Err(crate::simulator::AsyncAgentRunnerError::FeedbackUnavailable(_)) => {
                    full = true;
                    break;
                }
                Err(error) => panic!("unexpected mailbox fixture error: {error}"),
            }
        }
        full
    };
    assert!(saturated, "fixture must exercise the full mailbox path");
    std::thread::sleep(std::time::Duration::from_millis(20));

    // The actor thread starts concurrently with the fixture setup. If it
    // consumes the first queued event before the sidecar call, refill the
    // bounded mailbox and exercise the same error path again.
    for _ in 0..4 {
        sidecar.notify_recipe_completion_if_needed(&runtime_event, mapped_event.clone());
        if sidecar.pending_provider_world_events.len() == 1 {
            break;
        }
        let Some(RuntimeDecisionRunner::ProviderBacked(runner)) = sidecar.runner.as_mut() else {
            panic!("provider runner fixture");
        };
        let capacity = runner.mailbox_capacity();
        let mut full = false;
        for _ in 0..capacity.saturating_add(1) {
            match runner.notify_world_event("agent-0", mapped_event.clone()) {
                Ok(()) => {}
                Err(crate::simulator::AsyncAgentRunnerError::FeedbackUnavailable(_)) => {
                    full = true;
                    break;
                }
                Err(error) => panic!("unexpected mailbox fixture error: {error}"),
            }
        }
        if !full {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    assert_eq!(sidecar.pending_provider_world_events.len(), 1);
    let checkpoint = std::fs::read_to_string(&path).expect("pending event checkpoint");
    assert!(checkpoint.contains("pending_provider_world_events"));
    assert!(checkpoint.contains("agent-0"));
    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    restored
        .restore_provider_lineage(&RuntimeWorld::default())
        .expect("restore pending provider world event");
    assert_eq!(restored.pending_provider_world_events.len(), 1);
    let _ = std::fs::remove_file(path);
}

#[test]
fn stale_provider_replans_stop_at_the_bounded_budget() {
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    for count in 1..=3 {
        assert!(sidecar.schedule_provider_stale_replan(
            "agent-0",
            format!("turn-{count}").as_str(),
            format!("request-{count}").as_str(),
        ));
        sidecar.mark_provider_stale_replan_dispatched("agent-0");
    }
    assert!(!sidecar.schedule_provider_stale_replan("agent-0", "turn-4", "request-4"));
    assert_eq!(
        sidecar.provider_stale_replan_exhausted_agent().as_deref(),
        Some("agent-0")
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn hosted_local_mock_seeded_agent_builds_runtime_bound_capability_context() {
    let _env_guard = crate::viewer::runtime_live::canonical_runtime_provider_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _provider_env_snapshot = ProviderEnvSnapshot::capture(&[
        VIEWER_AGENT_DECISION_SOURCE_ENV,
        VIEWER_AGENT_PROVIDER_BACKEND_ENV,
        VIEWER_AGENT_PROVIDER_CONTRACT_ENV,
        VIEWER_AGENT_PROVIDER_TRANSPORT_ENV,
        VIEWER_AGENT_PROVIDER_URL_ENV,
        VIEWER_AGENT_PROVIDER_PROFILE_ENV,
        VIEWER_AGENT_EXECUTION_LANE_ENV,
        VIEWER_AGENT_PROVIDER_MODE_ENV,
    ]);
    // SAFETY: This test/setup code mutates process environment while holding
    // the canonical provider environment lock.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_DECISION_SOURCE_ENV, "provider_backed");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_backed");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_BACKEND_ENV, "provider_local_mock");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_CONTRACT_ENV, "worldsim_provider_v1");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_TRANSPORT_ENV, "loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, "http://127.0.0.1:9");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }

    let agent_id = "hosted-local-mock-agent";
    let mut world = crate::runtime_host::world::new();
    world
        .bind_cognition_runtime(
            "hosted-local-mock-world",
            "hosted-local-mock-branch",
            0,
            None,
            "pending",
            0,
        )
        .expect("bind Hosted local-mock Runtime cognition");
    world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: GeoPos::new(0, 0, 0),
    });
    world
        .step()
        .expect("register seeded Hosted local-mock Agent");

    let provider = crate::simulator::MockDecisionProvider::new("hosted-local-mock-provider");
    let behavior = crate::simulator::ProviderBackedAgentBehavior::new_legacy_compatibility(
        agent_id,
        provider,
        vec![crate::simulator::ActionCatalogEntry::new("wait", "wait")],
    );
    let mut runner = crate::simulator::AsyncAgentRunner::with_default_capacity();
    runner
        .register(behavior)
        .expect("register seeded Hosted local-mock provider Agent");

    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.runner = Some(RuntimeDecisionRunner::ProviderBacked(runner));
    sidecar.provider_agent_ids.insert(agent_id.to_string());
    sidecar
        .sync_shadow_kernel(&world, &WorldConfig::default())
        .expect("sync Hosted local-mock shadow kernel");
    let mut kernel = sidecar
        .shadow_kernel
        .take()
        .expect("Hosted local-mock shadow kernel");
    sidecar
        .prepare_provider_request_contexts(&mut world, &mut kernel, "hosted-local-mock-world")
        .expect("Hosted local-mock seeded Agent must have a Runtime-bound capability context");
    let binding = world
        .current_cognition_runtime_binding()
        .expect("Runtime cognition binding");
    let context = sidecar
        .provider_contexts
        .get(agent_id)
        .expect("prepared provider context");
    let request_context = &context.request_context;
    let catalog = request_context
        .base_decision_request
        .capability_catalog
        .as_ref()
        .expect("provider capability catalog");
    let invocation = request_context
        .base_decision_request
        .capability_invocation_context
        .as_ref()
        .expect("provider capability invocation context");

    assert_eq!(catalog.world_id, binding.world_id);
    assert_eq!(catalog.branch_id, binding.branch_id);
    assert_eq!(catalog.finality_epoch, binding.finality_epoch);
    assert_eq!(catalog.logical_tick, binding.base_tick);
    assert_eq!(invocation.audience.world_id, binding.world_id);
    assert_eq!(invocation.audience.branch_id, binding.branch_id);
    assert_eq!(invocation.subject, catalog.subject);
    assert_eq!(invocation.presenter.presenter_kind, "provider");
    assert_eq!(
        request_context.agent_session_id,
        "runtime-test-session:hosted-local-mock-agent"
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn hosted_local_mock_lane_enabled_for_url(provider_url: &str) -> bool {
    let _env_guard = crate::viewer::runtime_live::canonical_runtime_provider_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _provider_env_snapshot = ProviderEnvSnapshot::capture(&[
        VIEWER_AGENT_DECISION_SOURCE_ENV,
        VIEWER_AGENT_PROVIDER_BACKEND_ENV,
        VIEWER_AGENT_PROVIDER_CONTRACT_ENV,
        VIEWER_AGENT_PROVIDER_TRANSPORT_ENV,
        VIEWER_AGENT_PROVIDER_URL_ENV,
        VIEWER_AGENT_PROVIDER_PROFILE_ENV,
        VIEWER_AGENT_EXECUTION_LANE_ENV,
        VIEWER_AGENT_PROVIDER_MODE_ENV,
    ]);
    // SAFETY: This test/setup code mutates process environment while holding
    // the canonical provider environment lock.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_DECISION_SOURCE_ENV, "provider_backed");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_backed");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_BACKEND_ENV, "provider_local_mock");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_CONTRACT_ENV, "worldsim_provider_v1");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_TRANSPORT_ENV, "loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, provider_url);
        oasis7::env_mut::set_var(
            VIEWER_AGENT_PROVIDER_PROFILE_ENV,
            DEFAULT_PROVIDER_AGENT_PROFILE,
        );
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }
    hosted_local_mock_test_lane_enabled(true)
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn hosted_local_mock_lane_rejects_provider_url_userinfo_host_spoof() {
    assert!(!hosted_local_mock_lane_enabled_for_url(
        "http://127.0.0.1:5841@evil.example/"
    ));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn hosted_local_mock_lane_rejects_provider_url_malformed_port() {
    assert!(!hosted_local_mock_lane_enabled_for_url(
        "http://127.0.0.1:not-a-port/"
    ));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn hosted_local_mock_lane_rejects_provider_url_malformed_authority_path() {
    assert!(!hosted_local_mock_lane_enabled_for_url(
        "http://127.0.0.1:5841:9999/provider"
    ));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn hosted_local_mock_lane_accepts_valid_loopback_provider_url_path() {
    assert!(hosted_local_mock_lane_enabled_for_url(
        "http://127.0.0.1:5841/provider"
    ));
}

#[test]
fn production_provider_backed_lane_keeps_prompt_control_disabled() {
    let _env_guard = crate::viewer::runtime_live::canonical_runtime_provider_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _provider_env_snapshot = ProviderEnvSnapshot::capture(&[
        VIEWER_AGENT_DECISION_SOURCE_ENV,
        VIEWER_AGENT_PROVIDER_BACKEND_ENV,
        VIEWER_AGENT_PROVIDER_URL_ENV,
        VIEWER_AGENT_PROVIDER_MODE_ENV,
    ]);
    // SAFETY: This test/setup code mutates process environment while holding
    // the canonical provider environment lock.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_DECISION_SOURCE_ENV, "provider_backed");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_backed");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_BACKEND_ENV, "provider_local_bridge");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, "http://127.0.0.1:9");
    }

    let sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    assert!(
        !sidecar.supports_prompt_control_result(),
        "production provider_backed must not inherit the test-only Hosted local-mock capability"
    );
}
