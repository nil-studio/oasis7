use super::*;

// Keep full server construction and restore temporaries off the assertion frame.
#[inline(never)]
pub(super) fn setup_wake_handoff_recovery_fixture(
    lineage_path: &std::path::Path,
) -> Box<ViewerRuntimeLiveServer> {
    let mut world = crate::runtime_host::world::new();
    world
        .bind_cognition_runtime("world", "recovery-wake-branch", 0, None, "pending", 0)
        .expect("Runtime cognition binding");
    world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "agent-wake-recovery".to_string(),
        pos: GeoPos::new(0, 0, 0),
    });
    world.step().expect("register Runtime agent");
    let mut context = test_provider_context(
        "agent-wake-recovery",
        "turn-wake-recovery",
        "request-wake-recovery",
        1,
    );
    let digest = crate::simulator::h_v1("oasis7.test.wake-recovery-digest.v1", &"wake-recovery");
    context.request_context.observation_digest = digest.clone();
    context.request_context.capability_catalog_digest = digest.clone();
    context.request_context.capability_invocation_context_digest = digest.clone();
    context.request_context.memory_snapshot_digest = digest.clone();
    context.request_context.goal_snapshot_digest = digest.clone();
    context.request_context.continuation_digest = digest.clone();
    context.request_context.runtime_binding.base_world_hash = digest.clone();
    context
        .request_context
        .runtime_binding
        .runtime_manifest_hash = digest.clone();
    context.request_context.request_digest = digest.clone();
    context.turn_context.request_digest = digest;
    super::async_support::runtime_provider_prefix(&mut world, &context)
        .expect("persist provider Runtime prefix");

    let mut server = Box::new(
        ViewerRuntimeLiveServer::new(
            ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
                .with_decision_mode(ViewerLiveDecisionMode::Llm)
                .with_provider_lineage_store(lineage_path.to_path_buf()),
        )
        .expect("Runtime live server"),
    );
    server.world = world;
    server
        .llm_sidecar
        .provider_agent_ids
        .insert("agent-wake-recovery".to_string());
    server
        .llm_sidecar
        .provider_contexts
        .insert("agent-wake-recovery".to_string(), context.clone());
    server
        .llm_sidecar
        .provider_active_turns
        .insert("agent-wake-recovery".to_string(), context);
    server
        .llm_sidecar
        .provider_transport_exhausted
        .insert("agent-wake-recovery".to_string());
    // The sidecar mirrors a wake that Runtime no longer has. The resulting
    // consume error is the real authority failure path, rather than a
    // synthetic post-success branch.
    let wake: crate::runtime::SchedulerWakeV1 = serde_json::from_value(serde_json::json!({
        "schema_version": "scheduler-wake.v1",
        "wake_id": "wake-missing-in-runtime",
        "continuation_id": "continuation-wake-recovery",
        "world_id": "world",
        "branch_id": "recovery-wake-branch",
        "finality_epoch": 0,
        "finality_block_hash": null,
        "finality_status": "pending",
        "reorg_epoch": 0,
        "runtime_manifest_hash": "manifest-wake-recovery",
        "agent_id": "agent-wake-recovery",
        "agent_session_id": "session-test",
        "agent_turn_id": "turn-wake-recovery",
        "decision_request_id": "request-wake-recovery",
        "next_wake_tick": 0,
        "eligible_since_tick": 0,
        "starvation_deadline_tick": 1,
        "initial_priority": 0,
        "wake_seq": 1,
        "retry_seq": 0,
        "status": "pending",
        "pending_reason": "capacity_available"
    }))
    .expect("wake fixture");
    server
        .llm_sidecar
        .pending_runtime_wakes
        .insert(wake.wake_id.clone(), wake);
    // The fixture has already restored the durable sidecar state before the
    // control pass; avoid reloading the pre-fault checkpoint over the
    // in-memory wake/error transition while exercising sibling progress.
    server.llm_sidecar.provider_lineage_hydrated = true;

    server
}

#[inline(never)]
pub(super) fn restart_wake_handoff_recovery_fixture(
    server: &ViewerRuntimeLiveServer,
    lineage_path: &std::path::Path,
) -> Box<ViewerRuntimeLiveServer> {
    let mut restarted = Box::new(
        ViewerRuntimeLiveServer::new(
            ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
                .with_decision_mode(ViewerLiveDecisionMode::Llm)
                .with_provider_lineage_store(lineage_path.to_path_buf()),
        )
        .expect("restarted Runtime live server"),
    );
    restarted.world = server.world.clone();
    restarted
        .llm_sidecar
        .restore_provider_lineage(&restarted.world)
        .expect("restore wake recovery marker");
    restarted
}
