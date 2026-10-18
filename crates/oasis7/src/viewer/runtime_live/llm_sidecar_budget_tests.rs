use super::*;

#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(not(target_arch = "wasm32"))]
struct NativeBudgetEnvSnapshot {
    previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

#[cfg(not(target_arch = "wasm32"))]
impl NativeBudgetEnvSnapshot {
    fn capture(keys: &[&'static str]) -> Self {
        let previous = keys
            .iter()
            .map(|key| (*key, std::env::var_os(key)))
            .collect();
        for key in keys {
            // SAFETY: the caller holds the canonical provider environment lock.
            unsafe { oasis7::env_mut::remove_var(key) };
        }
        Self { previous }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for NativeBudgetEnvSnapshot {
    fn drop(&mut self) {
        for (key, value) in self.previous.drain(..) {
            // SAFETY: the snapshot is dropped while the test retains the lock.
            unsafe {
                match value {
                    Some(value) => oasis7::env_mut::set_var(key, value),
                    None => oasis7::env_mut::remove_var(key),
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
struct NativeBudgetDeniedClient {
    calls: Arc<AtomicUsize>,
}

#[cfg(not(target_arch = "wasm32"))]
impl crate::simulator::LlmCompletionClient for NativeBudgetDeniedClient {
    fn complete(
        &self,
        request: &crate::simulator::LlmCompletionRequest,
    ) -> Result<crate::simulator::LlmCompletionResult, crate::simulator::LlmClientError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(crate::simulator::LlmCompletionResult {
            turns: vec![crate::simulator::LlmCompletionTurn::Decision {
                payload: serde_json::json!({
                    "type": "module_call",
                    "module": "agent.modules.list",
                    "args": {}
                }),
            }],
            output: r#"{"type":"module_call","module":"agent.modules.list","args":{}}"#.to_string(),
            model: Some(request.model.clone()),
            prompt_tokens: Some(1),
            completion_tokens: Some(1),
            total_tokens: Some(2),
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn native_budget_test_config() -> crate::simulator::LlmAgentConfig {
    crate::simulator::LlmAgentConfig {
        model: "gpt-runtime-budget-test".to_string(),
        base_url: "https://example.invalid/v1".to_string(),
        api_key: "test-key".to_string(),
        timeout_ms: 1_000,
        system_prompt: "runtime budget integration test".to_string(),
        short_term_goal: "test".to_string(),
        long_term_goal: "test".to_string(),
        max_module_calls: 3,
        max_decision_steps: 4,
        max_repair_rounds: 0,
        prompt_max_history_items: 4,
        prompt_profile: crate::simulator::LlmPromptProfile::Balanced,
        force_replan_after_same_action: 4,
        harvest_max_amount_cap: 100,
        execute_until_auto_reenter_ticks: 4,
        llm_debug_mode: false,
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_budget_denial_round_trips_through_live_sidecar_without_effects() {
    let _env_guard = super::super::canonical_runtime_provider_env_lock()
        .lock()
        .expect("provider env lock");
    let _provider_env_snapshot = NativeBudgetEnvSnapshot::capture(&[
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
        crate::simulator::ENV_LLM_MODEL,
        crate::simulator::ENV_LLM_BASE_URL,
        crate::simulator::ENV_LLM_API_KEY,
    ]);
    // SAFETY: the canonical provider lock serializes this test's environment setup.
    unsafe {
        oasis7::env_mut::set_var(crate::simulator::ENV_LLM_MODEL, "gpt-runtime-budget-test");
        oasis7::env_mut::set_var(
            crate::simulator::ENV_LLM_BASE_URL,
            "https://example.invalid/v1",
        );
        oasis7::env_mut::set_var(crate::simulator::ENV_LLM_API_KEY, "test-key");
    }
    let config =
        crate::viewer::ViewerRuntimeLiveServerConfig::new(oasis7_simulator::WorldScenario::Minimal)
            .with_world_id("runtime-budget-denial")
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_test_cognition_runtime_binding("main", 0, None, "pending", 0);
    let mut server =
        crate::viewer::ViewerRuntimeLiveServer::new(config).expect("create live budget fixture");
    let agent_id = "agent-0";
    assert!(server.world.state().agents.contains_key(agent_id));
    server.world = server.world.clone().with_cognition_scheduler(
        serde_json::from_value(serde_json::json!({
            "schema_version": "scheduler-policy.v1",
            "max_total_wakes_per_tick": 8,
            "max_wakes_per_agent_per_tick": 1,
            "aging_after_ticks": 2,
            "max_starvation_ticks": 4,
            "initial_priority": 0,
            "comparator": "deadline_due_desc,next_wake_tick_asc,effective_priority_desc,starvation_deadline_tick_asc,cursor_distance_asc,agent_id_asc,continuation_id_asc,wake_seq_asc",
            "service_order": "stable_round_robin"
        }))
        .expect("decode scheduler policy"),
        8,
    );
    server
        .world
        .install_test_provider_capability_fixture(agent_id)
        .expect("install valid Runtime provider capability fixture");

    let calls = Arc::new(AtomicUsize::new(0));
    let behavior = crate::simulator::LlmAgentBehavior::new(
        agent_id,
        native_budget_test_config(),
        NativeBudgetDeniedClient {
            calls: Arc::clone(&calls),
        },
    );
    let mut runner = crate::simulator::AsyncAgentRunner::new(16).expect("create native runner");
    runner.register(behavior).expect("register native behavior");
    super::super::llm_sidecar_feedback_recovery::restore_agent_feedback_history(
        &server.world,
        &mut runner,
    )
    .expect("restore actual Runtime-owned feedback history before provider admission");
    server.llm_sidecar.runner = Some(RuntimeDecisionRunner::Builtin(runner));
    server
        .llm_sidecar
        .provider_agent_ids
        .insert(agent_id.to_string());
    server
        .llm_sidecar
        .sync_shadow_kernel(&server.world, &server.snapshot_config)
        .expect("sync native shadow kernel");
    let mut kernel = server
        .llm_sidecar
        .shadow_kernel
        .take()
        .expect("native shadow kernel");
    server
        .llm_sidecar
        .prepare_provider_request_contexts(&mut server.world, &mut kernel, "runtime-budget-denial")
        .expect("build valid Runtime request context");
    server.llm_sidecar.shadow_kernel = Some(kernel);
    let mut context = server
        .llm_sidecar
        .provider_contexts
        .get(agent_id)
        .cloned()
        .expect("prepared provider context");
    context.request_context.budget_contract.max_model_calls = 1;
    context.request_context.budget_contract.max_tool_calls = 1;
    context.request_context.request_digest = context.request_context.request_digest();
    context.turn_context.request_digest = context.request_context.request_digest.clone();
    context
        .request_context
        .validate_production_lane()
        .expect("bounded request context remains production-valid");
    server
        .llm_sidecar
        .provider_contexts
        .insert(agent_id.to_string(), context);

    server.llm_sidecar.request_decision();
    assert!(
        server
            .enqueue_llm_action_from_sidecar()
            .expect("start native async turn")
            .is_none(),
        "first live control pass only starts the native actor"
    );
    let trace = loop {
        server.llm_sidecar.request_decision();
        match server
            .enqueue_llm_action_from_sidecar()
            .expect("budget denial must remain a live Wait")
        {
            Some(trace) => break trace,
            None => std::thread::yield_now(),
        }
    };
    assert_eq!(trace.decision, crate::simulator::AgentDecision::Wait);
    assert!(
        trace
            .llm_error
            .as_deref()
            .is_some_and(|error| error.starts_with("budget_exhausted:"))
    );
    assert!(trace.llm_effect_intents.is_empty());
    assert!(trace.llm_effect_receipts.is_empty());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "denied branch cannot re-call provider"
    );
    assert!(
        server
            .llm_sidecar
            .provider_held_decisions
            .get(agent_id)
            .is_some_and(|decision| decision.continuation_admitted)
    );
    let continuations = server
        .world
        .active_cognition_continuations()
        .expect("read Runtime active continuations");
    assert!(
        continuations
            .iter()
            .any(|continuation| continuation.agent_id == agent_id)
    );
    assert!(server.world.journal().events.iter().all(|event| {
        !matches!(
            event.body,
            crate::runtime::WorldEventBody::ReceiptAppended(_)
                | crate::runtime::WorldEventBody::Domain(
                    crate::runtime::DomainEvent::ActionAccepted { .. }
                )
        )
    }));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn budget_wait_without_response_context_enters_terminal_cleanup() {
    let _env_guard = super::super::canonical_runtime_provider_env_lock()
        .lock()
        .expect("provider env lock");
    let _provider_env_snapshot = NativeBudgetEnvSnapshot::capture(&[
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
        crate::simulator::ENV_LLM_MODEL,
        crate::simulator::ENV_LLM_BASE_URL,
        crate::simulator::ENV_LLM_API_KEY,
    ]);
    // SAFETY: the canonical provider lock serializes this test's environment setup.
    unsafe {
        oasis7::env_mut::set_var(crate::simulator::ENV_LLM_MODEL, "gpt-runtime-budget-test");
        oasis7::env_mut::set_var(
            crate::simulator::ENV_LLM_BASE_URL,
            "https://example.invalid/v1",
        );
        oasis7::env_mut::set_var(crate::simulator::ENV_LLM_API_KEY, "test-key");
    }
    let config =
        crate::viewer::ViewerRuntimeLiveServerConfig::new(oasis7_simulator::WorldScenario::Minimal)
            .with_world_id("runtime-budget-no-response")
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_test_cognition_runtime_binding("main", 0, None, "pending", 0);
    let mut server =
        crate::viewer::ViewerRuntimeLiveServer::new(config).expect("create live budget fixture");
    let agent_id = "agent-0";
    server.world = server.world.clone().with_cognition_scheduler(
        serde_json::from_value(serde_json::json!({
            "schema_version": "scheduler-policy.v1",
            "max_total_wakes_per_tick": 8,
            "max_wakes_per_agent_per_tick": 1,
            "aging_after_ticks": 2,
            "max_starvation_ticks": 4,
            "initial_priority": 0,
            "comparator": "deadline_due_desc,next_wake_tick_asc,effective_priority_desc,starvation_deadline_tick_asc,cursor_distance_asc,agent_id_asc,continuation_id_asc,wake_seq_asc",
            "service_order": "stable_round_robin"
        }))
        .expect("decode scheduler policy"),
        8,
    );
    server
        .world
        .install_test_provider_capability_fixture(agent_id)
        .expect("install valid Runtime provider capability fixture");
    let behavior = crate::simulator::LlmAgentBehavior::new(
        agent_id,
        native_budget_test_config(),
        NativeBudgetDeniedClient {
            calls: Arc::new(AtomicUsize::new(0)),
        },
    );
    let mut runner = crate::simulator::AsyncAgentRunner::new(16).expect("create native runner");
    runner.register(behavior).expect("register native behavior");
    super::super::llm_sidecar_feedback_recovery::restore_agent_feedback_history(
        &server.world,
        &mut runner,
    )
    .expect("restore actual Runtime feedback history before provider admission");
    server.llm_sidecar.runner = Some(RuntimeDecisionRunner::Builtin(runner));
    server
        .llm_sidecar
        .provider_agent_ids
        .insert(agent_id.to_string());
    server
        .llm_sidecar
        .sync_shadow_kernel(&server.world, &server.snapshot_config)
        .expect("sync native shadow kernel");
    let mut kernel = server
        .llm_sidecar
        .shadow_kernel
        .take()
        .expect("native shadow kernel");
    server
        .llm_sidecar
        .prepare_provider_request_contexts(
            &mut server.world,
            &mut kernel,
            "runtime-budget-no-response",
        )
        .expect("build valid Runtime request context");
    let mut context = server
        .llm_sidecar
        .provider_contexts
        .get(agent_id)
        .cloned()
        .expect("prepared provider context");
    context.request_context.budget_contract.max_model_calls = 1;
    context.request_context.budget_contract.max_tool_calls = 1;
    context.request_context.request_digest = context.request_context.request_digest();
    context.turn_context.request_digest = context.request_context.request_digest.clone();
    server
        .llm_sidecar
        .provider_contexts
        .insert(agent_id.to_string(), context.clone());
    let lease = super::async_support::reserve_provider_cognition_lease(&mut server.world, &context)
        .expect("reserve provider cognition lease");
    super::async_support::runtime_provider_prefix(&mut server.world, &context)
        .expect("prefix Runtime provider turn");
    server
        .llm_sidecar
        .bind_provider_cognition_lease(agent_id.to_string(), lease.clone());

    {
        let runner = server
            .llm_sidecar
            .runner
            .as_mut()
            .and_then(RuntimeDecisionRunner::async_runner_mut)
            .expect("native runner");
        runner.sync_logical_tick(server.world.state().time);
        runner
            .start_turn_with_request_context_and_lease(
                agent_id,
                context.turn_context.clone(),
                context.request_context.clone(),
                lease.clone(),
            )
            .expect("start budget-denied provider turn");
    }
    let mut outcome = loop {
        let completed = server
            .llm_sidecar
            .runner
            .as_mut()
            .and_then(RuntimeDecisionRunner::async_runner_mut)
            .expect("native runner")
            .poll_completed()
            .expect("poll budget-denied provider turn");
        if let Some(outcome) = completed.into_iter().next() {
            break outcome;
        }
        std::thread::yield_now();
    };
    assert!(
        outcome
            .decision_trace
            .as_ref()
            .is_some_and(super::super::decision_trace::is_budget_exhausted_wait),
        "native fixture must produce the normalized budget Wait"
    );
    // Simulate the provider lane's normalized Wait shape: the actor retained
    // its trusted request/lease identity, but no provider response artifact
    // survived the normalization boundary.
    outcome.prepared_response_context = None;
    let decision = server.llm_sidecar.provider_decision_from_async_outcome(
        &mut server.world,
        &mut kernel,
        outcome,
    );
    assert!(
        decision.cognition.is_none(),
        "the regression fixture must omit prepared response context"
    );
    assert!(
        server
            .llm_sidecar
            .provider_transport_exhausted
            .contains(agent_id),
        "normalized no-response Wait must enter durable terminal cleanup"
    );

    let trace = server
        .enqueue_llm_action_from_sidecar()
        .expect_err("exhausted no-response Wait must terminalize");
    assert!(
        trace
            .llm_error
            .as_deref()
            .is_some_and(|error| error.contains("failed_provider")),
        "terminal cleanup must report the failed provider disposition"
    );
    let economy = server
        .world
        .cognition_economy()
        .expect("read economy after no-response cleanup");
    assert_eq!(
        economy
            .leases
            .get(lease.lease_id.as_str())
            .expect("lease remains durable")
            .status,
        crate::runtime::CognitionLeaseStatusV1::Released,
        "no-response budget cleanup must release the reserved unit"
    );
    assert!(
        server.world.cognition_in_flight_wakes().unwrap().is_empty(),
        "no-response budget cleanup must not leave a Runtime wake waiting"
    );
}
