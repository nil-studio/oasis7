use super::*;

#[path = "llm_sidecar_scheduler_cooldown_tests.rs"]
mod scheduler_cooldown_tests;

#[path = "llm_sidecar_canonical_checkpoint_tests.rs"]
mod canonical_checkpoint_tests;

#[path = "llm_sidecar_lineage_recovery_tests.rs"]
mod lineage_recovery_tests;

#[cfg(not(target_arch = "wasm32"))]
#[path = "llm_sidecar_lineage_queued_tests.rs"]
mod queued_tests;

#[cfg(not(target_arch = "wasm32"))]
pub(super) use queued_tests::queued_wait_cognition;

#[test]
fn runtime_provider_continuation_recovery_fence_blocks_retained_context_after_restart() {
    let _env_guard = crate::viewer::runtime_live::canonical_runtime_provider_env_lock()
        .lock()
        .expect("provider env lock");
    let _provider_env_snapshot = ProviderEnvSnapshot::capture(&[
        VIEWER_AGENT_DECISION_SOURCE_ENV,
        VIEWER_AGENT_PROVIDER_BACKEND_ENV,
        VIEWER_AGENT_PROVIDER_CONTRACT_ENV,
        VIEWER_AGENT_PROVIDER_TRANSPORT_ENV,
        VIEWER_AGENT_PROVIDER_URL_ENV,
        VIEWER_AGENT_PROVIDER_AUTH_TOKEN_ENV,
        VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV,
        VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV,
        VIEWER_AGENT_PROVIDER_PROFILE_ENV,
        VIEWER_AGENT_EXECUTION_LANE_ENV,
        VIEWER_AGENT_PROVIDER_MODE_ENV,
    ]);
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, "http://127.0.0.1:9");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }
    let world_id = "fenced-continuation-world";
    let mut world = crate::runtime_host::world::new();
    world
        .bind_cognition_runtime(world_id, "continuation-fence-branch", 0, None, "pending", 0)
        .expect("Runtime cognition binding");
    for agent_id in ["agent-a", "agent-b"] {
        world.submit_action(RuntimeAction::RegisterAgent {
            agent_id: agent_id.to_string(),
            pos: GeoPos::new(0, 0, 0),
        });
    }
    world.step().expect("register sibling Runtime agents");
    world
        .install_test_provider_capability_fixture("agent-b")
        .expect("install serviceable sibling Runtime provider capability fixture");

    let provider_runner = || {
        let mut runner = crate::simulator::AsyncAgentRunner::with_default_capacity();
        let mut provider_states = Vec::new();
        for agent_id in ["agent-a", "agent-b"] {
            let provider =
                crate::simulator::MockDecisionProvider::new("continuation-fence-provider");
            provider_states.push(provider.shared_state());
            let behavior = crate::simulator::ProviderBackedAgentBehavior::new_legacy_compatibility(
                agent_id,
                provider,
                vec![crate::simulator::ActionCatalogEntry::new("wait", "wait")],
            );
            runner.register(behavior).expect("register provider actor");
        }
        (
            RuntimeDecisionRunner::ProviderBacked(runner),
            provider_states,
        )
    };
    let lineage_path = std::env::temp_dir().join(format!(
        "oasis7-runtime-live-provider-continuation-fence-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(lineage_path.clone());
    first.runner = Some(provider_runner().0);
    first
        .provider_agent_ids
        .extend(["agent-a".to_string(), "agent-b".to_string()]);
    first
        .sync_shadow_kernel(&world, &WorldConfig::default())
        .expect("sync provider shadow kernel");
    let mut kernel = first.shadow_kernel.take().expect("provider shadow kernel");
    first.provider_contexts.insert(
        "agent-a".to_string(),
        test_provider_context("agent-a", "turn-a", "request-a", 1),
    );
    first
        .prepare_provider_request_contexts(&mut world, &mut kernel, world_id)
        .expect("prepare retained provider contexts");
    first.shadow_kernel = Some(kernel);
    assert!(first.provider_contexts.contains_key("agent-a"));
    assert!(first.provider_contexts.contains_key("agent-b"));
    first.provider_continuation_recovery_pending.insert(
        "agent-a".to_string(),
        "continuation_hydration_failed".to_string(),
    );
    first
        .persist_provider_lineage()
        .expect("persist continuation recovery fence");

    let mut restarted = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restarted.configure_provider_lineage_store(lineage_path.clone());
    restarted
        .restore_provider_lineage(&world)
        .expect("restore provider lineage after restart");
    assert!(
        restarted
            .provider_continuation_recovery_pending
            .contains_key("agent-a")
    );
    assert!(restarted.provider_contexts.contains_key("agent-a"));
    assert!(restarted.provider_contexts.contains_key("agent-b"));
    let (provider_runner, provider_states) = provider_runner();
    restarted.runner = Some(provider_runner);
    restarted
        .sync_shadow_kernel(&world, &WorldConfig::default())
        .expect("sync restarted provider shadow kernel");
    let cognition_before = world.cognition().clone();
    let mut kernel = restarted
        .shadow_kernel
        .take()
        .expect("restarted shadow kernel");
    crate::viewer::runtime_live::llm_sidecar_feedback_recovery::restore_agent_feedback_history(
        &world,
        restarted
            .runner
            .as_mut()
            .and_then(RuntimeDecisionRunner::async_runner_mut)
            .expect("restarted native provider runner"),
    )
    .expect("restore Runtime feedback history before sibling dispatch");
    let decision = restarted.next_async_provider_decision(&mut world, &mut kernel, world_id);
    assert!(
        decision.is_none(),
        "fenced agents must not surface a decision: {decision:?}"
    );
    let cognition_events_before = cognition_before["cognition_journal"]["events"]
        .as_array()
        .expect("cognition journal events before dispatch")
        .len();
    let cognition_events = world.cognition()["cognition_journal"]["events"]
        .as_array()
        .expect("cognition journal events after dispatch");
    let dispatched_events = cognition_events
        .get(cognition_events_before..)
        .expect("new cognition lifecycle events");
    for event_kind in ["TurnStarted", "ContextCaptured", "RequestDispatched"] {
        assert!(
            dispatched_events.iter().any(|event| {
                event["event_kind"] == event_kind && event["agent_id"] == "agent-b"
            }),
            "serviceable sibling B must append {event_kind} while A remains fenced: {dispatched_events:?}"
        );
    }
    assert!(
        dispatched_events
            .iter()
            .all(|event| event["agent_id"] != "agent-a"),
        "fenced agent A must not append a Runtime lifecycle prefix: {dispatched_events:?}"
    );

    let dispatch_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let sibling_b_dispatched = loop {
        let state = provider_states[1].lock().expect("provider B state lock");
        let dispatched = !state.recorded_requests.is_empty()
            && !state.recorded_turn_contexts.is_empty()
            && !state.recorded_request_contexts.is_empty();
        drop(state);
        if dispatched {
            break true;
        }
        if std::time::Instant::now() >= dispatch_deadline {
            break false;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    assert!(
        sibling_b_dispatched,
        "serviceable sibling B must dispatch one provider request"
    );
    assert!(
        provider_states[0]
            .lock()
            .expect("provider A state lock")
            .recorded_requests
            .is_empty(),
        "fenced agent A must issue zero provider requests"
    );
    assert!(
        provider_states[0]
            .lock()
            .expect("provider A turn state lock")
            .recorded_turn_contexts
            .is_empty(),
        "fenced agent A must issue zero provider turn contexts"
    );
    assert!(
        provider_states[0]
            .lock()
            .expect("provider A request context state lock")
            .recorded_request_contexts
            .is_empty(),
        "fenced agent A must issue zero provider request contexts"
    );
    {
        let state = provider_states[1].lock().expect("provider B state lock");
        assert_eq!(state.recorded_requests.len(), 1);
        assert_eq!(state.recorded_turn_contexts.len(), 1);
        assert_eq!(state.recorded_request_contexts.len(), 1);
        assert_eq!(state.recorded_requests[0].observation.agent_id, "agent-b");
        assert_eq!(state.recorded_turn_contexts[0].agent_id, "agent-b");
        assert_eq!(state.recorded_request_contexts[0].agent_subject, "agent-b");
    }
    assert_eq!(
        restarted.provider_wait_until.get("agent-b"),
        None,
        "serviceable sibling B must not retain an artificial wait fence"
    );
    assert!(restarted.provider_contexts.contains_key("agent-a"));
    assert!(restarted.provider_contexts.contains_key("agent-b"));
    assert!(
        restarted
            .provider_continuation_recovery_pending
            .contains_key("agent-a")
    );
    let _ = std::fs::remove_file(lineage_path);
}

#[test]
fn provider_lineage_persists_and_restores_pending_lifecycle_markers() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-lineage-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    first
        .provider_session_ids
        .insert("agent-0".to_string(), "session-7".to_string());
    first.provider_context_seq.insert("agent-0".to_string(), 4);
    let request_context = crate::simulator::ContinuousAgentRequestContextV1 {
        base_decision_request: crate::simulator::DecisionRequest {
            observation: crate::simulator::ObservationEnvelope {
                agent_id: "agent-0".to_string(),
                world_time: 12,
                mode: crate::simulator::ProviderExecutionMode::PlayerParity,
                observation_schema_version:
                    crate::simulator::DEFAULT_PROVIDER_OBSERVATION_SCHEMA_VERSION.to_string(),
                action_schema_version: crate::simulator::DEFAULT_PROVIDER_ACTION_SCHEMA_VERSION
                    .to_string(),
                environment_class: Some("runtime_live".to_string()),
                fallback_reason: None,
                observation: crate::simulator::ProviderObservation::default(),
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
        agent_session_id: "session-7".to_string(),
        agent_turn_id: "turn-active".to_string(),
        decision_request_id: "request-active".to_string(),
        retry_seq: 4,
        transport_attempt: 2,
        agent_subject: "agent-0".to_string(),
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
        agent_id: "agent-0".to_string(),
        agent_session_id: "session-7".to_string(),
        agent_turn_id: "turn-active".to_string(),
        decision_request_id: "request-active".to_string(),
        request_digest: crate::simulator::Digest32::default(),
        memory_snapshot: crate::simulator::MemoryContextSnapshotV1::empty("agent-0"),
        goal_snapshot: crate::simulator::GoalSnapshotV1::empty(),
        continuation: None,
    };
    let active_context = cognition_context::ProviderContextState {
        turn_context,
        request_context,
    };
    first
        .provider_contexts
        .insert("agent-0".to_string(), active_context.clone());
    first
        .provider_retry_contexts
        .insert("agent-0".to_string(), active_context.clone());
    first
        .provider_active_turns
        .insert("agent-0".to_string(), active_context);
    first.provider_agent_ids.insert("agent-0".to_string());
    first.provider_wait_until.insert("agent-0".to_string(), 19);
    first.provider_terminal_states.insert(
        "agent-0".to_string(),
        super::lineage_persistence::ProviderTerminalState {
            feedback: None,
            agent_id: "agent-0".to_string(),
            agent_session_id: "session-7".to_string(),
            agent_turn_id: "turn-1".to_string(),
            decision_request_id: "request-1".to_string(),
            request_digest: crate::simulator::Digest32::default().to_string(),
            status: "rejected".to_string(),
            reject_reason: Some("no_effect".to_string()),
            feedback_id: Some("feedback-1".to_string()),
        },
    );
    first.schedule_provider_stale_replan("agent-0", "turn-2", "request-4");
    first.mark_provider_transport_exhausted("agent-0".to_string());
    first
        .persist_provider_lineage()
        .expect("persist provider lineage");

    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    restored
        .restore_provider_lineage(&RuntimeWorld::default())
        .expect("restore provider lineage");

    assert_eq!(
        restored.provider_session_ids.get("agent-0"),
        Some(&"session-7".to_string())
    );
    assert_eq!(restored.provider_context_seq.get("agent-0"), Some(&4));
    assert_eq!(
        restored
            .provider_contexts
            .get("agent-0")
            .map(|context| context.request_context.transport_attempt),
        Some(2)
    );
    assert!(!restored.provider_retry_contexts.contains_key("agent-0"));
    assert!(restored.provider_active_turns.contains_key("agent-0"));
    assert_eq!(restored.provider_wait_until.get("agent-0"), Some(&19));
    assert_eq!(
        restored
            .provider_terminal_states
            .get("agent-0")
            .and_then(|state| state.reject_reason.as_deref()),
        Some("no_effect")
    );
    assert!(restored.provider_stale_replans.contains_key("agent-0"));
    assert!(restored.provider_transport_exhausted.contains("agent-0"));
    let checkpoint_after_restore: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&path).expect("read fenced provider lineage checkpoint"),
    )
    .expect("decode fenced provider lineage checkpoint");
    assert!(
        checkpoint_after_restore["provider_retry_contexts"]
            .as_object()
            .is_some_and(|retry| retry.is_empty())
    );
    assert!(
        checkpoint_after_restore["provider_transport_exhausted"]
            .as_array()
            .is_some_and(|agents| agents.iter().any(|agent| agent == "agent-0"))
    );
    let _ = std::fs::remove_file(path);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn provider_dispatch_persists_active_marker_before_actor_start() {
    let _env_guard = crate::viewer::runtime_live::canonical_runtime_provider_env_lock()
        .lock()
        .expect("provider env lock");
    let _provider_env_snapshot = ProviderEnvSnapshot::capture(&[
        VIEWER_AGENT_DECISION_SOURCE_ENV,
        VIEWER_AGENT_PROVIDER_BACKEND_ENV,
        VIEWER_AGENT_PROVIDER_CONTRACT_ENV,
        VIEWER_AGENT_PROVIDER_TRANSPORT_ENV,
        VIEWER_AGENT_PROVIDER_URL_ENV,
        VIEWER_AGENT_PROVIDER_AUTH_TOKEN_ENV,
        VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV,
        VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV,
        VIEWER_AGENT_PROVIDER_PROFILE_ENV,
        VIEWER_AGENT_EXECUTION_LANE_ENV,
        VIEWER_AGENT_PROVIDER_MODE_ENV,
    ]);
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, "http://127.0.0.1:9");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }
    let world_id = "durable-dispatch-marker-world";
    let mut world = crate::runtime_host::world::new();
    world
        .bind_cognition_runtime(
            world_id,
            "durable-dispatch-marker-branch",
            0,
            None,
            "pending",
            0,
        )
        .expect("Runtime cognition binding");
    world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "agent-0".to_string(),
        pos: GeoPos::new(0, 0, 0),
    });
    world.step().expect("register Runtime agent");
    world
        .install_test_provider_capability_fixture("agent-0")
        .expect("install Runtime provider capability fixture");

    let path = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-lineage-dispatch-marker-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let provider = crate::simulator::MockDecisionProvider::new("durable-dispatch-marker-provider");
    let provider_state = provider.shared_state();
    let behavior = crate::simulator::ProviderBackedAgentBehavior::new_legacy_compatibility(
        "agent-0",
        provider,
        vec![crate::simulator::ActionCatalogEntry::new("wait", "wait")],
    );
    let mut runner = crate::simulator::AsyncAgentRunner::with_default_capacity();
    runner.register(behavior).expect("register provider actor");

    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    first.runner = Some(RuntimeDecisionRunner::ProviderBacked(runner));
    first.provider_agent_ids.insert("agent-0".to_string());
    first
        .sync_shadow_kernel(&world, &WorldConfig::default())
        .expect("sync provider shadow kernel");
    let mut kernel = first.shadow_kernel.take().expect("provider shadow kernel");
    crate::viewer::runtime_live::llm_sidecar_feedback_recovery::restore_agent_feedback_history(
        &world,
        first
            .runner
            .as_mut()
            .and_then(RuntimeDecisionRunner::async_runner_mut)
            .expect("fresh native provider runner"),
    )
    .expect("restore Runtime feedback history before first marker dispatch");

    let decision = first.next_async_provider_decision(&mut world, &mut kernel, world_id);
    assert!(
        decision.is_none(),
        "a durably marked async actor remains in flight: {decision:?}"
    );
    let checkpoint: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&path).expect("read pre-start provider lineage checkpoint"),
    )
    .expect("decode pre-start provider lineage checkpoint");
    assert_eq!(
        checkpoint["provider_active_turns"]["agent-0"]["request_context"]["agent_turn_id"],
        checkpoint["provider_contexts"]["agent-0"]["request_context"]["agent_turn_id"],
        "active marker must be durable before the provider actor starts"
    );
    assert_eq!(
        checkpoint["provider_active_turns"]["agent-0"]["request_context"]["decision_request_id"],
        checkpoint["provider_contexts"]["agent-0"]["request_context"]["decision_request_id"],
        "active marker must carry the same logical request identity"
    );

    let mut restarted = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restarted.configure_provider_lineage_store(path.clone());
    restarted
        .restore_provider_lineage(&world)
        .expect("restore pre-start provider lineage checkpoint");
    let restarted_provider =
        crate::simulator::MockDecisionProvider::new("durable-dispatch-marker-restarted-provider");
    let restarted_provider_state = restarted_provider.shared_state();
    let restarted_behavior =
        crate::simulator::ProviderBackedAgentBehavior::new_legacy_compatibility(
            "agent-0",
            restarted_provider,
            vec![crate::simulator::ActionCatalogEntry::new("wait", "wait")],
        );
    let mut restarted_runner = crate::simulator::AsyncAgentRunner::with_default_capacity();
    restarted_runner
        .register(restarted_behavior)
        .expect("register restarted provider actor");
    restarted.runner = Some(RuntimeDecisionRunner::ProviderBacked(restarted_runner));
    restarted
        .sync_shadow_kernel(&world, &WorldConfig::default())
        .expect("sync restarted provider shadow kernel");
    let mut restarted_kernel = restarted
        .shadow_kernel
        .take()
        .expect("restarted provider shadow kernel");
    crate::viewer::runtime_live::llm_sidecar_feedback_recovery::restore_agent_feedback_history(
        &world,
        restarted
            .runner
            .as_mut()
            .and_then(RuntimeDecisionRunner::async_runner_mut)
            .expect("restarted native provider runner"),
    )
    .expect("restore Runtime feedback history before restart-fenced dispatch");
    let restarted_decision =
        restarted.next_async_provider_decision(&mut world, &mut restarted_kernel, world_id);
    assert!(
        restarted_decision.is_none(),
        "restart fence must suppress a second provider dispatch: {restarted_decision:?}"
    );
    assert!(
        restarted.provider_transport_exhausted.contains("agent-0"),
        "an active marker from a crash prefix must fail closed on restart"
    );
    assert!(!restarted.provider_retry_contexts.contains_key("agent-0"));
    assert!(!restarted.provider_active_turns.contains_key("agent-0"));
    assert_eq!(
        restarted
            .provider_contexts
            .get("agent-0")
            .map(|context| context.request_context.agent_turn_id.as_str()),
        checkpoint["provider_contexts"]["agent-0"]["request_context"]["agent_turn_id"].as_str(),
        "restart fencing must retain the original identity for terminal feedback"
    );
    assert!(
        provider_state
            .lock()
            .expect("provider state lock")
            .recorded_requests
            .len()
            <= 1,
        "the crash-prefix recovery test must not issue a duplicate provider request"
    );
    assert!(
        restarted_provider_state
            .lock()
            .expect("restarted provider state lock")
            .recorded_requests
            .is_empty(),
        "restart fence must issue zero provider requests"
    );
    let _ = std::fs::remove_file(path);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn provider_dispatch_persistence_failure_does_not_start_actor() {
    let _env_guard = crate::viewer::runtime_live::canonical_runtime_provider_env_lock()
        .lock()
        .expect("provider env lock");
    let _provider_env_snapshot = ProviderEnvSnapshot::capture(&[
        VIEWER_AGENT_DECISION_SOURCE_ENV,
        VIEWER_AGENT_PROVIDER_BACKEND_ENV,
        VIEWER_AGENT_PROVIDER_CONTRACT_ENV,
        VIEWER_AGENT_PROVIDER_TRANSPORT_ENV,
        VIEWER_AGENT_PROVIDER_URL_ENV,
        VIEWER_AGENT_PROVIDER_AUTH_TOKEN_ENV,
        VIEWER_AGENT_PROVIDER_CONNECT_TIMEOUT_MS_ENV,
        VIEWER_AGENT_PROVIDER_DECISION_TIMEOUT_MS_ENV,
        VIEWER_AGENT_PROVIDER_PROFILE_ENV,
        VIEWER_AGENT_EXECUTION_LANE_ENV,
        VIEWER_AGENT_PROVIDER_MODE_ENV,
    ]);
    // SAFETY: This test/setup code mutates process environment in a controlled scope.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, "http://127.0.0.1:9");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }
    let world_id = "failed-dispatch-marker-world";
    let mut world = crate::runtime_host::world::new();
    world
        .bind_cognition_runtime(
            world_id,
            "failed-dispatch-marker-branch",
            0,
            None,
            "pending",
            0,
        )
        .expect("Runtime cognition binding");
    world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "agent-0".to_string(),
        pos: GeoPos::new(0, 0, 0),
    });
    world.step().expect("register Runtime agent");
    world
        .install_test_provider_capability_fixture("agent-0")
        .expect("install Runtime provider capability fixture");

    let blocker = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-lineage-dispatch-blocker-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::write(&blocker, b"lineage parent is a file")
        .expect("write lineage persistence blocker");
    let path = blocker.join("lineage.json");
    let provider = crate::simulator::MockDecisionProvider::new("failed-dispatch-marker-provider");
    let provider_state = provider.shared_state();
    let behavior = crate::simulator::ProviderBackedAgentBehavior::new_legacy_compatibility(
        "agent-0",
        provider,
        vec![crate::simulator::ActionCatalogEntry::new("wait", "wait")],
    );
    let mut runner = crate::simulator::AsyncAgentRunner::with_default_capacity();
    runner.register(behavior).expect("register provider actor");
    crate::viewer::runtime_live::llm_sidecar_feedback_recovery::restore_agent_feedback_history(
        &world,
        &mut runner,
    )
    .expect("empty Runtime history is still a host-validated recovery projection");

    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.configure_provider_lineage_store(path);
    // Skip initial hydration so the invalid parent is exercised by the strict
    // pre-start marker write, rather than being mistaken for a corrupt file.
    sidecar.provider_lineage_hydrated = true;
    sidecar.provider_lineage_restored = true;
    sidecar.runner = Some(RuntimeDecisionRunner::ProviderBacked(runner));
    sidecar.provider_agent_ids.insert("agent-0".to_string());
    sidecar
        .sync_shadow_kernel(&world, &WorldConfig::default())
        .expect("sync provider shadow kernel");
    let mut kernel = sidecar
        .shadow_kernel
        .take()
        .expect("provider shadow kernel");

    let decision = sidecar
        .next_async_provider_decision(&mut world, &mut kernel, world_id)
        .expect("failed marker persistence must return a typed decision");
    let error = decision
        .decision_trace
        .and_then(|trace| trace.llm_error)
        .expect("failed marker persistence error");
    assert!(error.contains("provider dispatch marker persistence failed"));
    assert!(!sidecar.provider_active_turns.contains_key("agent-0"));
    assert!(
        provider_state
            .lock()
            .expect("provider state lock")
            .recorded_requests
            .is_empty(),
        "provider actor must not start when its active marker cannot be persisted"
    );
    assert!(
        world.cognition()["cognition_journal"]["events"]
            .as_array()
            .expect("cognition journal events")
            .iter()
            .any(|event| event["event_kind"] == "CognitionTurnFailed"),
        "failed marker persistence must close the Runtime prefix"
    );
    let _ = std::fs::remove_file(blocker);
}

#[test]
fn provider_lineage_restore_fails_closed_for_retryable_held_outcome() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-lineage-held-retry-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    first.provider_agent_ids.insert("agent-0".to_string());
    first.provider_contexts.insert(
        "agent-0".to_string(),
        test_provider_context("agent-0", "turn-retry", "request-retry", 1),
    );
    first.provider_held_decisions.insert(
        "agent-0".to_string(),
        async_support::RuntimeLlmDecision {
            agent_id: "agent-0".to_string(),
            decision: crate::simulator::AgentDecision::Wait,
            decision_trace: Some(crate::simulator::AgentDecisionTrace {
                agent_id: "agent-0".to_string(),
                time: 12,
                decision: crate::simulator::AgentDecision::Wait,
                llm_input: None,
                llm_output: Some(r#"{"provider_error":{"retryable":true}}"#.to_string()),
                llm_error: None,
                parse_error: None,
                llm_diagnostics: None,
                llm_effect_intents: Vec::new(),
                llm_effect_receipts: Vec::new(),
                llm_step_trace: Vec::new(),
                llm_prompt_section_trace: Vec::new(),
                llm_chat_messages: Vec::new(),
            }),
            cognition: None,
            memory_write_intents: Vec::new(),
            continuation_admitted: false,
        },
    );
    first
        .persist_provider_lineage()
        .expect("persist held retryable provider lineage");

    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    restored
        .restore_provider_lineage(&RuntimeWorld::default())
        .expect("restore held retryable provider lineage");

    assert!(!restored.provider_held_decisions.contains_key("agent-0"));
    assert!(restored.provider_completed_decisions.is_empty());
    assert!(
        !restored.provider_retry_contexts.contains_key("agent-0"),
        "restart must not redispatch a retryable identity with unknown consumed budget"
    );
    assert!(restored.provider_transport_exhausted.contains("agent-0"));
    assert_eq!(
        restored
            .provider_contexts
            .get("agent-0")
            .map(|context| context.request_context.agent_turn_id.as_str()),
        Some("turn-retry")
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn provider_lineage_restore_fails_closed_for_orphaned_active_context_without_runtime_wake() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-lineage-orphan-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    let context = test_provider_context("agent-0", "turn-orphaned", "request-orphaned", 1);
    first
        .provider_contexts
        .insert("agent-0".to_string(), context.clone());
    first
        .provider_active_turns
        .insert("agent-0".to_string(), context);
    first.provider_agent_ids.insert("agent-0".to_string());
    first
        .persist_provider_lineage()
        .expect("persist orphaned provider lineage");

    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    restored
        .restore_provider_lineage(&RuntimeWorld::default())
        .expect("restore orphaned provider lineage");

    assert!(
        !restored.provider_active_turns.contains_key("agent-0"),
        "process-local active marker must not survive a restart"
    );
    assert!(
        !restored.provider_retry_contexts.contains_key("agent-0"),
        "restart must not redispatch a logical request with unknown consumed budget"
    );
    assert!(restored.provider_transport_exhausted.contains("agent-0"));
    assert_eq!(
        restored
            .provider_contexts
            .get("agent-0")
            .map(|context| context.request_context.agent_turn_id.as_str()),
        Some("turn-orphaned")
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn provider_lineage_restore_migrates_legacy_budget_fields_to_zero_deny() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-lineage-legacy-budget-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    let context = test_provider_context("agent-legacy", "turn-legacy", "request-legacy", 1);
    first
        .provider_contexts
        .insert("agent-legacy".to_string(), context.clone());
    first
        .provider_active_turns
        .insert("agent-legacy".to_string(), context);
    first.provider_agent_ids.insert("agent-legacy".to_string());
    first
        .persist_provider_lineage()
        .expect("persist current provider lineage before legacy conversion");

    let mut legacy: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read provider lineage checkpoint"))
            .expect("decode current provider lineage checkpoint");
    fn remove_new_budget_fields(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(fields) => {
                if fields.contains_key("max_latency_ms")
                    && fields.contains_key("max_repair_attempts")
                {
                    fields.remove("max_model_calls");
                    fields.remove("max_tool_calls");
                }
                for child in fields.values_mut() {
                    remove_new_budget_fields(child);
                }
            }
            serde_json::Value::Array(values) => {
                for child in values {
                    remove_new_budget_fields(child);
                }
            }
            _ => {}
        }
    }
    remove_new_budget_fields(&mut legacy);
    legacy["schema_version"] = serde_json::json!(1);
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&legacy).expect("encode legacy provider lineage checkpoint"),
    )
    .expect("write legacy provider lineage checkpoint");

    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    restored
        .restore_provider_lineage(&RuntimeWorld::default())
        .expect("legacy provider lineage checkpoint must migrate explicitly");

    let migrated = restored
        .provider_contexts
        .get("agent-legacy")
        .expect("legacy context identity must survive migration");
    assert_eq!(migrated.request_context.budget_contract.max_model_calls, 0);
    assert_eq!(migrated.request_context.budget_contract.max_tool_calls, 0);
    assert_eq!(migrated.request_context.agent_turn_id, "turn-legacy");
    assert_eq!(
        migrated.request_context.decision_request_id,
        "request-legacy"
    );
    assert!(
        restored
            .provider_transport_exhausted
            .contains("agent-legacy")
    );
    assert!(
        !restored
            .provider_retry_contexts
            .contains_key("agent-legacy"),
        "legacy active identity must remain fenced instead of being redispatched"
    );
    let migrated_checkpoint: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&path).expect("read migrated provider lineage checkpoint"),
    )
    .expect("decode migrated provider lineage checkpoint");
    assert_eq!(migrated_checkpoint["schema_version"], serde_json::json!(2));
    let _ = std::fs::remove_file(path);
}

#[test]
fn provider_lineage_hydration_fences_undecodable_checkpoint() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-lineage-hydration-fence-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::write(
        &path,
        serde_json::json!({
            "schema_version": 99,
            "provider_contexts": {
                "agent-0": { "request_context": { "decision_request_id": "unknown" } }
            }
        })
        .to_string(),
    )
    .expect("write undecodable provider lineage checkpoint");

    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.configure_provider_lineage_store(path.clone());
    sidecar.hydrate_provider_lineage(&RuntimeWorld::default());

    assert!(sidecar.provider_lineage_hydrated);
    assert!(sidecar.provider_lineage_recovery_pending.is_some());
    assert!(sidecar.provider_contexts.is_empty());
    let _ = std::fs::remove_file(path);
}
