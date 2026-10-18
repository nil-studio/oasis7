use super::*;

#[cfg(not(target_arch = "wasm32"))]
pub(in super::super) fn queued_wait_cognition(
    world: &mut RuntimeWorld,
    agent_id: &str,
) -> crate::viewer::runtime_live::control_plane::llm_sidecar::RuntimeProviderActionContext {
    let mut context = lineage_recovery_tests::valid_test_provider_context(
        world,
        agent_id,
        "turn-queued",
        "request-queued",
    );
    let request = &mut context.request_context;
    request.observation_digest = crate::simulator::h_v1(
        "oasis7.cognition.observation.v1",
        &request.base_decision_request.observation,
    );
    request.memory_snapshot_digest = context.turn_context.memory_snapshot.digest.clone().into();
    request.goal_snapshot_digest = context.turn_context.goal_snapshot.digest.clone().into();
    request.continuation_digest = crate::simulator::h_v1(
        "oasis7.cognition.continuation.v1",
        &context.turn_context.continuation,
    );
    request.request_digest = request.request_digest();
    context.turn_context.request_digest = request.request_digest.clone();
    request
        .validate_production_lane()
        .expect("Runtime-bound Wait request");
    let lease = async_support::reserve_provider_cognition_lease(world, &context)
        .expect("reserve original Runtime Wait lease");
    let response = crate::simulator::DecisionResponse::wait("queued-completion-provider");
    crate::viewer::runtime_live::control_plane::llm_sidecar::RuntimeProviderActionContext {
        request: context.clone(),
        response: crate::simulator::ContinuousAgentResponseContextV1 {
            response_digest: crate::simulator::cognition_response_digest(&response),
            base_decision_response: response,
            context_discriminator: crate::simulator::CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR.into(),
            context_version: crate::simulator::CONTINUOUS_AGENT_CONTEXT_VERSION,
            agent_session_id: context.request_context.agent_session_id.clone(),
            agent_turn_id: context.request_context.agent_turn_id.clone(),
            decision_request_id: context.request_context.decision_request_id.clone(),
            retry_seq: context.request_context.retry_seq,
            transport_attempt: context.request_context.transport_attempt,
            request_digest: context.request_context.request_digest.clone(),
        },
        cognition_lease: Some(lease),
        memory_write_intents: Vec::new(),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn assert_queued_provider_decision_survives_restore(decision: AgentDecision) {
    let decision_kind = match &decision {
        AgentDecision::Act(_) => "action",
        AgentDecision::Wait => "wait",
        _ => "other",
    };
    let path = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-lineage-queued-completion-{}-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos(),
        decision_kind
    ));
    let agent_id = "agent-queued";
    let mut recovery_world = lineage_recovery_tests::bound_provider_lease_test_world(&[agent_id]);
    let mut context = lineage_recovery_tests::valid_test_provider_context(
        &recovery_world,
        agent_id,
        "turn-queued",
        "request-queued",
    );
    let cognition = if matches!(decision, AgentDecision::Wait) {
        let cognition = queued_wait_cognition(&mut recovery_world, agent_id);
        context = cognition.request.clone();
        Some(cognition)
    } else {
        None
    };
    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    first.provider_agent_ids.insert(agent_id.to_string());
    first
        .provider_contexts
        .insert(agent_id.to_string(), context.clone());
    first
        .provider_active_turns
        .insert(agent_id.to_string(), context.clone());
    first
        .provider_completed_decisions
        .push_back(async_support::RuntimeLlmDecision {
            agent_id: agent_id.to_string(),
            decision: decision.clone(),
            decision_trace: None,
            cognition,
            memory_write_intents: Vec::new(),
            continuation_admitted: false,
        });
    first
        .persist_provider_lineage()
        .expect("persist queued completed provider decision");
    if matches!(decision, AgentDecision::Wait) {
        let original = std::fs::read(&path).expect("read original Wait checkpoint");
        let mut missing: serde_json::Value = serde_json::from_slice(&original).unwrap();
        missing["provider_completed_decisions"][0]["cognition"] = serde_json::Value::Null;
        std::fs::write(&path, serde_json::to_vec(&missing).unwrap()).unwrap();
        let mut invalid = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
        invalid.configure_provider_lineage_store(path.clone());
        assert!(
            invalid.restore_provider_lineage(&recovery_world).is_err(),
            "queued Wait without original cognition must fail closed"
        );
        std::fs::write(&path, original).expect("restore authentic Wait checkpoint");
    }

    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    restored
        .restore_provider_lineage(&recovery_world)
        .expect("restore queued completed provider decision");
    assert!(
        !restored.provider_transport_exhausted.contains(agent_id),
        "queued completion must retain the active identity during recovery"
    );

    let provider = crate::simulator::MockDecisionProvider::new("queued-completion-provider");
    let provider_state = provider.shared_state();
    let behavior = crate::simulator::ProviderBackedAgentBehavior::new_legacy_compatibility(
        agent_id,
        provider,
        vec![crate::simulator::ActionCatalogEntry::new("wait", "wait")],
    );
    let mut runner = crate::simulator::AsyncAgentRunner::with_default_capacity();
    runner.register(behavior).expect("register provider actor");
    restored.runner = Some(RuntimeDecisionRunner::ProviderBacked(runner));
    let mut world = recovery_world;
    let mut kernel = WorldKernel::new();

    let delivered = restored
        .next_async_provider_decision(&mut world, &mut kernel, "world")
        .expect("queued completion must be delivered after restore");
    assert_eq!(delivered.agent_id, agent_id);
    assert_eq!(delivered.decision, decision);
    assert!(restored.provider_held_decisions.contains_key(agent_id));
    assert!(
        restored.provider_completed_decisions.is_empty(),
        "a restored completion must be removed from the queue after one delivery"
    );
    assert!(
        provider_state
            .lock()
            .expect("provider state lock")
            .recorded_requests
            .is_empty(),
        "replaying a queued completion must not readmit the provider"
    );
    let _ = std::fs::remove_file(path);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn provider_lineage_restore_delivers_queued_action_once_without_readmission() {
    assert_queued_provider_decision_survives_restore(AgentDecision::Act(
        oasis7_simulator::Action::MoveAgent {
            agent_id: "agent-queued".to_string(),
            to: "loc-queued".to_string(),
        },
    ));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn provider_lineage_restore_delivers_queued_wait_once_without_readmission() {
    assert_queued_provider_decision_survives_restore(AgentDecision::Wait);
}
