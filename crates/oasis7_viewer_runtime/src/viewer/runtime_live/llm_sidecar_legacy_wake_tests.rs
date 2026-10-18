use super::*;
use crate::viewer::runtime_live::WorldScenario;
use crate::viewer::{ViewerRuntimeLiveServer, ViewerRuntimeLiveServerConfig};
use serde_json::{Value, json};

fn legacy_sidecar_wake_policy() -> crate::runtime::SchedulerPolicyV1 {
    serde_json::from_value(json!({
        "schema_version": "scheduler-policy.v1",
        "max_total_wakes_per_tick": 8,
        "max_wakes_per_agent_per_tick": 1,
        "aging_after_ticks": 2,
        "max_starvation_ticks": 4,
        "initial_priority": 0,
        "comparator": "deadline_due_desc,next_wake_tick_asc,effective_priority_desc,starvation_deadline_tick_asc,cursor_distance_asc,agent_id_asc,continuation_id_asc,wake_seq_asc",
        "service_order": "stable_round_robin"
    }))
    .expect("legacy sidecar wake scheduler policy")
}

fn legacy_sidecar_wake_fixture() -> (
    RuntimeWorld,
    crate::runtime::SchedulerWakeV1,
    cognition_context::ProviderContextState,
    std::path::PathBuf,
) {
    static FIXTURE_NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let world_id = "world-legacy-sidecar-wake";
    let branch_id = "legacy-sidecar-wake-branch";
    let agent_id = "agent-legacy-sidecar-wake";
    let session_id = "session-test";
    let turn_id = "turn-legacy-sidecar-wake";
    let request_id = "request-legacy-sidecar-wake";
    let mut world =
        crate::runtime_host::world::new().with_cognition_scheduler(legacy_sidecar_wake_policy(), 1);
    world
        .bind_cognition_runtime(world_id, branch_id, 0, None, "pending", 0)
        .expect("Runtime cognition binding");
    world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: GeoPos::new(0, 0, 0),
    });
    world.step().expect("register Runtime agent");
    let binding = world
        .current_cognition_runtime_binding()
        .expect("Runtime cognition binding readback");
    let manifest_hash = world.current_manifest_hash().expect("manifest hash");
    let request_digest =
        crate::simulator::h_v1("oasis7.test.legacy-sidecar-wake-request.v1", &request_id);
    world
        .start_cognition_turn(
            agent_id,
            session_id,
            turn_id,
            request_id,
            request_digest.to_string().as_str(),
        )
        .expect("register Runtime cognition turn");
    let mut proposal: crate::runtime::CognitionContinuationProposalV1 =
        serde_json::from_value(json!({
            "schema_version": 1,
            "continuation_proposal_id": "proposal-legacy-sidecar-wake",
            "world_id": binding.world_id,
            "branch_id": binding.branch_id,
            "finality_epoch": binding.finality_epoch,
            "finality_block_hash": binding.finality_block_hash,
            "finality_status": binding.finality_status,
            "reorg_epoch": binding.reorg_epoch,
            "runtime_manifest_hash": manifest_hash,
            "agent_id": agent_id,
            "agent_session_id": session_id,
            "agent_turn_id": turn_id,
            "decision_request_id": request_id,
            "origin_turn_id": turn_id,
            "origin_request_digest": request_digest,
            "action_or_plan_kind": "wait",
            "proposal_digest": "pending",
            "action_or_envelope_digest": null,
            "baseline_observation_digest": crate::simulator::h_v1(
                "oasis7.test.legacy-sidecar-wake-observation.v1",
                &request_id,
            ),
            "goal_digest": crate::simulator::h_v1(
                "oasis7.test.legacy-sidecar-wake-goal.v1",
                &request_id,
            ),
            "policy_digest": crate::simulator::h_v1(
                "oasis7.test.legacy-sidecar-wake-policy.v1",
                &request_id,
            ),
            "policy_revision": 1,
            "precondition_summary": "ready",
            "wake_conditions": [{
                "schema_version": "wake-condition.v1",
                "kind": "at_or_after_tick",
                "logical_tick": 1
            }],
            "next_wake_tick": 1,
            "remaining_budget": {"unit": "steps", "value": 2},
            "valid_until_tick": 100,
            "precondition_digest": crate::simulator::h_v1(
                "oasis7.test.legacy-sidecar-wake-precondition.v1",
                &request_id,
            ),
            "source": "runtime-test"
        }))
        .expect("legacy sidecar wake proposal");
    proposal.proposal_digest = proposal.proposal_digest();
    world
        .admit_cognition_continuation(proposal)
        .expect("admit Runtime continuation");
    world
        .select_ready_cognition_wakes(1)
        .expect("select Runtime wake");
    let mut wake = world
        .cognition_in_flight_wakes()
        .expect("Runtime in-flight wakes")
        .into_iter()
        .next()
        .expect("authoritative Runtime wake");
    let authoritative_digest = wake.request_digest.clone();
    wake.request_digest.clear();
    let mut context = test_provider_context(agent_id, turn_id, request_id, 1);
    context.request_context.request_digest =
        crate::simulator::Digest32::from(authoritative_digest.clone());
    context.turn_context.request_digest = crate::simulator::Digest32::from(authoritative_digest);

    let path = std::env::temp_dir().join(format!(
        "oasis7-legacy-sidecar-wake-{}-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos(),
        FIXTURE_NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    first.provider_agent_ids.insert(agent_id.to_string());
    first
        .provider_contexts
        .insert(agent_id.to_string(), context.clone());
    first
        .provider_active_turns
        .insert(agent_id.to_string(), context.clone());
    first.provider_terminal_states.insert(
        agent_id.to_string(),
        lineage_persistence::ProviderTerminalState {
            feedback: None,
            agent_id: agent_id.to_string(),
            agent_session_id: session_id.to_string(),
            agent_turn_id: turn_id.to_string(),
            decision_request_id: request_id.to_string(),
            request_digest: context.request_context.request_digest.to_string(),
            status: "rejected".to_string(),
            reject_reason: Some("legacy_sidecar_wake_test".to_string()),
            feedback_id: Some("feedback-legacy-sidecar-wake".to_string()),
        },
    );
    first.provider_wake_recovery_pending.insert(
        agent_id.to_string(),
        lineage_persistence::ProviderWakeRecoveryPending {
            active: context.clone(),
            status: crate::runtime::ContinuationStatusV1::Rejected,
            reason: "legacy_sidecar_wake_test".to_string(),
        },
    );
    first
        .pending_runtime_wakes
        .insert(wake.wake_id.clone(), wake.clone());
    first
        .persist_provider_lineage()
        .expect("persist legacy sidecar wake checkpoint");
    let mut checkpoint: Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read legacy sidecar wake checkpoint"))
            .expect("decode legacy sidecar wake checkpoint");
    checkpoint["pending_runtime_wakes"][wake.wake_id.as_str()]
        .as_object_mut()
        .expect("pending wake object")
        .remove("request_digest");
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&checkpoint).expect("encode legacy checkpoint"),
    )
    .expect("strip request digest from legacy checkpoint");
    (world, wake, context, path)
}

#[test]
fn legacy_sidecar_wake_checkpoint_restores_authoritative_digest_and_persists_migration() {
    let (world, wake, context, path) = legacy_sidecar_wake_fixture();
    let authoritative = world
        .cognition_in_flight_wakes()
        .expect("Runtime in-flight wakes")
        .into_iter()
        .find(|candidate| candidate.wake_id == wake.wake_id)
        .expect("authoritative Runtime wake");
    assert!(!authoritative.request_digest.is_empty());

    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    restored
        .restore_provider_lineage(&world)
        .expect("restore legacy sidecar wake checkpoint");
    assert_eq!(
        restored
            .pending_runtime_wakes
            .get(wake.wake_id.as_str())
            .map(|wake| wake.request_digest.as_str()),
        Some(authoritative.request_digest.as_str()),
        "legacy wake must hydrate from the Runtime continuation"
    );
    assert_eq!(
        restored.pending_runtime_wake_id_for_context(&wake.agent_id, &context),
        Some(wake.wake_id.as_str())
    );
    assert_eq!(
        restored.pending_runtime_wake_id_for_terminal(&wake.agent_id),
        Some(wake.wake_id.as_str())
    );
    let persisted: Value = serde_json::from_slice(
        &std::fs::read(&path).expect("read migrated sidecar wake checkpoint"),
    )
    .expect("decode migrated sidecar wake checkpoint");
    assert_eq!(
        persisted["pending_runtime_wakes"][wake.wake_id.as_str()]["request_digest"],
        authoritative.request_digest
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn legacy_sidecar_wake_checkpoint_uses_complete_terminal_identity_when_runtime_wake_absent() {
    let (mut world, wake, context, path) = legacy_sidecar_wake_fixture();
    world
        .handoff_cognition_wake(
            &wake.wake_id,
            crate::runtime::CognitionWakeDispositionV1::Terminal {
                status: crate::runtime::ContinuationStatusV1::Rejected,
                reason: "legacy_terminal_fallback_test".to_string(),
            },
        )
        .expect("consume Runtime wake before terminal fallback restore");
    assert!(
        world
            .cognition_in_flight_wakes()
            .expect("Runtime in-flight wake readback")
            .is_empty()
    );

    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    restored
        .restore_provider_lineage(&world)
        .expect("restore terminal fallback sidecar wake checkpoint");
    let authoritative_digest = context.request_context.request_digest.to_string();
    assert_eq!(
        restored
            .pending_runtime_wakes
            .get(wake.wake_id.as_str())
            .map(|wake| wake.request_digest.as_str()),
        Some(authoritative_digest.as_str()),
        "legacy wake must hydrate from the complete terminal identity"
    );
    assert_eq!(
        restored.pending_runtime_wake_id_for_terminal(&wake.agent_id),
        Some(wake.wake_id.as_str())
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn legacy_sidecar_wake_recovery_handoff_clears_fence_without_provider_redispatch() {
    let (world, wake, _context, path) = legacy_sidecar_wake_fixture();
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_provider_lineage_store(path.clone()),
    )
    .expect("Runtime live server");
    server.world = world;
    server
        .llm_sidecar
        .restore_provider_lineage(&server.world)
        .expect("restore legacy wake recovery checkpoint");
    server
        .retry_provider_wake_recovery()
        .expect("hydrated wake recovery should complete");
    assert!(
        server
            .world
            .cognition_in_flight_wakes()
            .expect("Runtime in-flight wake readback")
            .is_empty(),
        "terminal recovery must consume the exact Runtime wake"
    );
    assert!(
        !server
            .llm_sidecar
            .pending_runtime_wakes
            .contains_key(wake.wake_id.as_str())
    );
    assert!(
        server
            .llm_sidecar
            .provider_wake_recovery_pending(&wake.agent_id)
            .is_none()
    );
    assert!(
        !server
            .llm_sidecar
            .provider_contexts
            .contains_key(wake.agent_id.as_str())
    );
    assert_eq!(
        server.world.cognition_execution_metrics()["provider_invocation_count"],
        0
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn explicit_sidecar_wake_digest_conflict_fails_closed_during_restore() {
    let (world, wake, _context, path) = legacy_sidecar_wake_fixture();
    let mut checkpoint: Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read legacy sidecar wake checkpoint"))
            .expect("decode legacy sidecar wake checkpoint");
    checkpoint["pending_runtime_wakes"][wake.wake_id.as_str()]
        .as_object_mut()
        .expect("pending wake object")
        .insert(
            "request_digest".to_string(),
            json!(crate::simulator::h_v1(
                "oasis7.test.legacy-sidecar-wake-conflict.v1",
                &wake.wake_id,
            )),
        );
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&checkpoint).expect("encode conflicting checkpoint"),
    )
    .expect("write conflicting checkpoint");
    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    let error = restored
        .restore_provider_lineage(&world)
        .expect_err("contradictory explicit wake digest must fail closed");
    assert!(error.contains("request_digest") || error.contains("identity"));
    let _ = std::fs::remove_file(path);
}
