use super::*;
use crate::viewer::runtime_live::WorldScenario;
use crate::viewer::{ViewerRuntimeLiveServer, ViewerRuntimeLiveServerConfig};

#[path = "llm_sidecar_recovery_fixture.rs"]
mod recovery_fixture;
use recovery_fixture::{
    restart_wake_handoff_recovery_fixture, setup_wake_handoff_recovery_fixture,
};

fn provider_wake_stack_diag(message: &str) {
    use std::io::Write as _;

    if std::env::var_os("CI").is_none() && std::env::var_os("OASIS7_STACK_DIAG").is_none() {
        return;
    }

    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "[stack-diag:provider-wake] {message}");
    let _ = stderr.flush();
}

#[test]
fn provider_terminal_marker_requires_complete_identity() {
    let agent_id = "agent-terminal-identity";
    let mut context = test_provider_context(
        agent_id,
        "turn-terminal-identity",
        "request-terminal-identity",
        1,
    );
    let identity_digest =
        crate::simulator::h_v1("oasis7.test.terminal-identity.v1", &"terminal-identity");
    context.request_context.request_digest = identity_digest.clone();
    context.turn_context.request_digest = identity_digest;
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar
        .provider_contexts
        .insert(agent_id.to_string(), context.clone());
    sidecar.record_provider_terminal_state(
        agent_id,
        &context,
        "failed",
        Some("failed_provider".to_string()),
        Some("feedback-terminal-identity".to_string()),
    );
    let decision = async_support::RuntimeLlmDecision {
        agent_id: agent_id.to_string(),
        decision: AgentDecision::Wait,
        decision_trace: None,
        cognition: None,
        memory_write_intents: Vec::new(),
        continuation_admitted: false,
    };
    assert!(sidecar.provider_decision_is_terminalized(&decision));

    let mut later_context = context;
    later_context.request_context.agent_session_id = "session-terminal-identity-later".to_string();
    later_context.turn_context.agent_session_id = "session-terminal-identity-later".to_string();
    later_context.request_context.request_digest =
        crate::simulator::h_v1("oasis7.test.terminal-identity-later.v1", &agent_id);
    later_context.turn_context.request_digest =
        later_context.request_context.request_digest.clone();
    sidecar
        .provider_contexts
        .insert(agent_id.to_string(), later_context);
    assert!(
        !sidecar.provider_decision_is_terminalized(&decision),
        "same turn/request values from a later session or digest must not match a terminal marker"
    );
}

#[test]
fn restored_active_context_session_only_mismatch_is_quarantined() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-provider-session-mismatch-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let agent_id = "agent-session-mismatch";
    let marker = test_provider_context(
        agent_id,
        "turn-session-mismatch",
        "request-session-mismatch",
        1,
    );
    let mut conflicting = marker.clone();
    conflicting.request_context.agent_session_id = "session-conflicting-only".to_string();
    conflicting.turn_context.agent_session_id = "session-conflicting-only".to_string();

    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    first.provider_agent_ids.insert(agent_id.to_string());
    first
        .provider_active_turns
        .insert(agent_id.to_string(), marker.clone());
    first
        .provider_contexts
        .insert(agent_id.to_string(), conflicting);
    first
        .persist_provider_lineage()
        .expect("persist session-only identity mismatch");

    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    restored
        .restore_provider_lineage(&RuntimeWorld::default())
        .expect("restore session-only identity mismatch");
    assert_eq!(
        restored
            .provider_recovery_pending
            .get(agent_id)
            .map(|pending| pending.active.request_context.agent_session_id.as_str()),
        Some("session-test"),
        "Runtime marker must remain the quarantined identity"
    );
    assert!(
        restored.provider_transport_exhausted.contains(agent_id),
        "session-only mismatch must remain fenced until Runtime terminalization"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn restored_active_context_digest_only_mismatch_is_quarantined() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-provider-digest-mismatch-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let agent_id = "agent-digest-mismatch";
    let marker = test_provider_context(
        agent_id,
        "turn-digest-mismatch",
        "request-digest-mismatch",
        1,
    );
    let mut conflicting = marker.clone();
    let digest =
        crate::simulator::h_v1("oasis7.test.digest-only-mismatch.v1", &"conflicting-digest");
    conflicting.request_context.request_digest = digest.clone();
    conflicting.turn_context.request_digest = digest;

    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    first.provider_agent_ids.insert(agent_id.to_string());
    first
        .provider_active_turns
        .insert(agent_id.to_string(), marker.clone());
    first
        .provider_contexts
        .insert(agent_id.to_string(), conflicting);
    first
        .persist_provider_lineage()
        .expect("persist digest-only identity mismatch");

    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    restored
        .restore_provider_lineage(&RuntimeWorld::default())
        .expect("restore digest-only identity mismatch");
    assert_eq!(
        restored
            .provider_recovery_pending
            .get(agent_id)
            .map(|pending| pending.active.request_context.request_digest.to_string()),
        Some(marker.request_context.request_digest.to_string()),
        "Runtime marker digest must remain the quarantined identity"
    );
    assert!(restored.provider_transport_exhausted.contains(agent_id));
    let _ = std::fs::remove_file(path);
}

#[test]
fn wake_digest_only_collision_is_not_selected_for_handoff() {
    let agent_id = "agent-wake-digest-collision";
    let context = test_provider_context(
        agent_id,
        "turn-wake-digest-collision",
        "request-wake-digest-collision",
        1,
    );
    let conflicting_digest = crate::simulator::h_v1(
        "oasis7.test.wake-digest-collision.v1",
        &"conflicting-wake-digest",
    );
    let wake: crate::runtime::SchedulerWakeV1 = serde_json::from_value(serde_json::json!({
        "schema_version": "scheduler-wake.v1",
        "wake_id": "wake-digest-collision",
        "continuation_id": "continuation-digest-collision",
        "world_id": "world",
        "branch_id": "main",
        "finality_epoch": 0,
        "finality_block_hash": null,
        "finality_status": "pending",
        "reorg_epoch": 0,
        "runtime_manifest_hash": "manifest",
        "agent_id": agent_id,
        "agent_session_id": context.request_context.agent_session_id,
        "agent_turn_id": context.request_context.agent_turn_id,
        "decision_request_id": context.request_context.decision_request_id,
        "request_digest": conflicting_digest,
        "next_wake_tick": 0,
        "eligible_since_tick": 0,
        "starvation_deadline_tick": 1,
        "initial_priority": 0,
        "wake_seq": 1,
        "retry_seq": 0,
        "status": "pending",
        "pending_reason": "capacity_available"
    }))
    .expect("wake digest collision fixture");
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar
        .pending_runtime_wakes
        .insert(wake.wake_id.clone(), wake);
    sidecar.record_provider_terminal_state(
        agent_id,
        &context,
        "failed",
        None,
        Some("terminal-wake-digest-collision".to_string()),
    );
    assert_eq!(
        sidecar.pending_runtime_wake_id_for_context(agent_id, &context),
        None,
        "same session/turn/request with another digest must not select the wake"
    );
    assert_eq!(
        sidecar.pending_runtime_wake_id_for_terminal(agent_id),
        None,
        "terminal wake lookup must reject a digest-only collision"
    );
}

#[test]
fn wake_recovery_cleanup_checkpoint_failure_retains_identity_for_retry() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-provider-wake-cleanup-failure-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let agent_id = "agent-wake-cleanup-failure";
    let context = test_provider_context(
        agent_id,
        "turn-wake-cleanup-failure",
        "request-wake-cleanup-failure",
        1,
    );
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.configure_provider_lineage_store(path.clone());
    sidecar.provider_agent_ids.insert(agent_id.to_string());
    sidecar
        .provider_contexts
        .insert(agent_id.to_string(), context.clone());
    sidecar
        .provider_active_turns
        .insert(agent_id.to_string(), context.clone());
    sidecar
        .provider_transport_exhausted
        .insert(agent_id.to_string());
    sidecar.provider_wake_recovery_pending.insert(
        agent_id.to_string(),
        lineage_persistence::ProviderWakeRecoveryPending {
            active: context.clone(),
            status: crate::runtime::ContinuationStatusV1::Completed,
            reason: "wake_cleanup_checkpoint_failure_test".to_string(),
        },
    );
    sidecar
        .persist_provider_lineage()
        .expect("persist wake cleanup recovery fixture");
    sidecar
        .install_test_provider_lineage_checkpoint_blocker()
        .expect("install checkpoint blocker");
    assert!(
        sidecar.complete_provider_wake_recovery(agent_id).is_err(),
        "checkpoint failure must be surfaced"
    );
    assert!(sidecar.provider_wake_recovery_pending(agent_id).is_some());
    assert!(sidecar.provider_contexts.contains_key(agent_id));
    assert!(sidecar.provider_active_turns.contains_key(agent_id));

    let backup = path.with_extension(format!("blocked-backup-{}", std::process::id()));
    std::fs::remove_dir(&path).expect("remove checkpoint blocker");
    std::fs::rename(&backup, &path).expect("restore checkpoint after blocker");
    sidecar
        .complete_provider_wake_recovery(agent_id)
        .expect("wake cleanup retries after checkpoint recovery");
    let checkpoint: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&path).expect("read retried wake cleanup checkpoint"),
    )
    .expect("decode retried wake cleanup checkpoint");
    assert!(
        checkpoint["provider_wake_recovery_pending"]
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
    );
    assert!(!sidecar.provider_contexts.contains_key(agent_id));
    assert!(!sidecar.provider_active_turns.contains_key(agent_id));
    let _ = std::fs::remove_file(path);
}

#[test]
fn restore_reconciles_committed_runtime_record_without_sidecar_terminal_marker() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-provider-committed-recovery-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let agent_id = "agent-committed-recovery";
    let mut world = crate::runtime_host::world::new();
    world
        .bind_cognition_runtime("world", "main", 0, None, "pending", 0)
        .expect("Runtime cognition binding");
    let binding = world
        .current_cognition_runtime_binding()
        .expect("Runtime binding");
    let mut context = test_provider_context(
        agent_id,
        "turn-committed-recovery",
        "request-committed-recovery",
        1,
    );
    context.request_context.runtime_binding = binding.clone();
    let digest = crate::simulator::h_v1(
        "oasis7.test.committed-recovery-context.v1",
        &"committed-recovery",
    );
    context.request_context.observation_digest = digest.clone();
    context.request_context.capability_catalog_digest = digest.clone();
    context.request_context.capability_invocation_context_digest = digest.clone();
    context.request_context.memory_snapshot_digest = digest.clone();
    context.request_context.goal_snapshot_digest = digest.clone();
    context.request_context.continuation_digest = digest;
    context.request_context.request_digest = context.request_context.request_digest();
    context.turn_context.request_digest = context.request_context.request_digest.clone();
    let request = crate::runtime::RuntimeCognitionCommitRequestV1 {
        agent_id: agent_id.to_string(),
        agent_session_id: context.request_context.agent_session_id.clone(),
        agent_turn_id: context.request_context.agent_turn_id.clone(),
        decision_request_id: context.request_context.decision_request_id.clone(),
        retry_seq: context.request_context.retry_seq,
        transport_attempt: context.request_context.transport_attempt,
        request_digest: context.request_context.request_digest.to_string(),
        observation_digest: context.request_context.observation_digest.to_string(),
        context_digest:
            crate::viewer::runtime_live::control_plane::llm_sidecar::runtime_provider_context_digest(
                &context.request_context,
            ),
        capability_snapshot_hash: crate::simulator::h_v1(
            "oasis7.test.committed-recovery-capability.v1",
            &"capability",
        )
        .to_string(),
        authority_context_hash: crate::simulator::h_v1(
            "oasis7.test.committed-recovery-authority.v1",
            &"authority",
        )
        .to_string(),
        captured_base_binding: crate::runtime::RuntimeCognitionBaseBindingV1 {
            world_id: binding.world_id.clone(),
            branch_id: binding.branch_id.clone(),
            finality_epoch: binding.finality_epoch,
            finality_block_hash: binding
                .finality_block_hash
                .as_ref()
                .map(ToString::to_string),
            finality_status: binding.finality_status.clone(),
            base_tick: binding.base_tick,
            base_world_hash: binding.base_world_hash.to_string(),
            reorg_epoch: binding.reorg_epoch,
            runtime_manifest_hash: binding.runtime_manifest_hash.to_string(),
        },
    };
    let mut artifact = crate::runtime::RuntimeCognitionResponseArtifactV1 {
        schema_version: 1,
        context_discriminator:
            crate::runtime::RuntimeCognitionResponseArtifactV1::CONTEXT_DISCRIMINATOR.to_string(),
        context_version: crate::runtime::RuntimeCognitionResponseArtifactV1::CONTEXT_VERSION,
        agent_session_id: request.agent_session_id.clone(),
        agent_turn_id: request.agent_turn_id.clone(),
        decision_request_id: request.decision_request_id.clone(),
        retry_seq: request.retry_seq,
        transport_attempt: request.transport_attempt,
        request_digest: request.request_digest.clone(),
        response_digest: crate::simulator::h_v1(
            "oasis7.test.committed-recovery-response.v1",
            &"response",
        )
        .to_string(),
        artifact_digest: String::new(),
    };
    artifact.refresh_artifact_digest();
    let idempotent_request = request.clone();
    let idempotent_artifact = artifact.clone();
    world
        .commit_cognition_action(
            request,
            RuntimeAction::RegisterAgent {
                agent_id: agent_id.to_string(),
                pos: GeoPos::new(0, 0, 0),
            },
            artifact,
        )
        .expect("Runtime committed response");

    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    first.provider_agent_ids.insert(agent_id.to_string());
    first
        .provider_active_turns
        .insert(agent_id.to_string(), context.clone());
    first
        .provider_contexts
        .insert(agent_id.to_string(), context.clone());
    first
        .provider_completed_decisions
        .push_back(async_support::RuntimeLlmDecision {
            agent_id: "agent-committed-recovery-sibling".to_string(),
            // This placeholder protects queue identity preservation, not Wait admission.
            decision: AgentDecision::Act(oasis7_simulator::Action::MoveAgent {
                agent_id: "agent-committed-recovery-sibling".into(),
                to: "loc-recovery".into(),
            }),
            decision_trace: None,
            cognition: None,
            memory_write_intents: Vec::new(),
            continuation_admitted: false,
        });
    let queued = async_support::RuntimeLlmDecision {
        agent_id: agent_id.to_string(),
        decision: AgentDecision::Act(oasis7_simulator::Action::MoveAgent {
            agent_id: agent_id.into(),
            to: "loc-recovery".into(),
        }),
        decision_trace: None,
        cognition: None,
        memory_write_intents: Vec::new(),
        continuation_admitted: false,
    };
    first.provider_completed_decisions.push_back(queued.clone());
    first
        .provider_held_decisions
        .insert(agent_id.to_string(), queued);
    first
        .persist_provider_lineage()
        .expect("persist pre-track sidecar checkpoint");

    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    restored
        .restore_provider_lineage(&world)
        .expect("restore committed Runtime response");
    world
        .commit_cognition_action(
            idempotent_request,
            RuntimeAction::RegisterAgent {
                agent_id: agent_id.to_string(),
                pos: GeoPos::new(0, 0, 0),
            },
            idempotent_artifact,
        )
        .expect("Runtime commit must be idempotent after restore");
    assert_eq!(
        world.cognition()["commit_records"]
            .as_array()
            .expect("commit records")
            .iter()
            .filter(|record| record["agent_id"] == agent_id)
            .count(),
        1,
        "restore/retry must preserve exactly one Runtime effect"
    );
    assert!(
        restored.provider_terminal_matches_request(agent_id, &context.request_context),
        "Runtime committed record must synthesize an exact sidecar terminal marker"
    );
    assert_eq!(
        restored
            .provider_terminal_states
            .get(agent_id)
            .map(|terminal| terminal.status.as_str()),
        Some("committed")
    );
    assert!(
        !restored.provider_transport_exhausted.contains(agent_id),
        "a committed Runtime response must not become a failed transport fence"
    );
    assert!(
        !restored
            .provider_completed_decisions
            .iter()
            .any(|decision| decision.agent_id == agent_id)
            && !restored.provider_held_decisions.contains_key(agent_id),
        "Runtime commit marker must suppress queued legacy decision replay"
    );
    assert!(
        restored
            .provider_completed_decisions
            .iter()
            .any(|decision| decision.agent_id == "agent-committed-recovery-sibling"),
        "committed recovery for Agent A must preserve sibling B progress"
    );
    assert!(!restored.provider_active_turns.contains_key(agent_id));
    assert!(!restored.provider_contexts.contains_key(agent_id));
    let _ = std::fs::remove_file(path);
}

#[cfg(not(target_arch = "wasm32"))]
fn assert_terminal_marker_suppresses_queued_provider_decision(decision: AgentDecision) {
    let decision_kind = match &decision {
        AgentDecision::Act(_) => "action",
        AgentDecision::Wait => "wait",
        _ => "other",
    };
    let path = std::env::temp_dir().join(format!(
        "oasis7-viewer-provider-lineage-terminal-queue-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos(),
        decision_kind
    ));
    let agent_id = "agent-terminal-queued";
    let mut context = test_provider_context(
        agent_id,
        "turn-terminal-queued",
        "request-terminal-queued",
        1,
    );
    let terminal_digest =
        crate::simulator::h_v1("oasis7.test.terminal-queued.v1", &"terminal-queued");
    context.request_context.request_digest = terminal_digest.clone();
    context.turn_context.request_digest = terminal_digest;
    let queued = async_support::RuntimeLlmDecision {
        agent_id: agent_id.to_string(),
        decision,
        decision_trace: None,
        cognition: None,
        memory_write_intents: Vec::new(),
        continuation_admitted: false,
    };
    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    first.provider_agent_ids.insert(agent_id.to_string());
    first
        .provider_contexts
        .insert(agent_id.to_string(), context.clone());
    first.provider_terminal_states.insert(
        agent_id.to_string(),
        lineage_persistence::ProviderTerminalState {
            feedback: None,
            agent_id: agent_id.to_string(),
            agent_session_id: "session-test".to_string(),
            agent_turn_id: "turn-terminal-queued".to_string(),
            decision_request_id: "request-terminal-queued".to_string(),
            request_digest: context.request_context.request_digest.to_string(),
            status: "committed".to_string(),
            reject_reason: None,
            feedback_id: Some("feedback-terminal-queued".to_string()),
        },
    );
    first.provider_completed_decisions.push_back(queued.clone());
    first
        .provider_held_decisions
        .insert(agent_id.to_string(), queued);
    first
        .persist_provider_lineage()
        .expect("persist terminal queued provider decision");

    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    restored
        .restore_provider_lineage(&RuntimeWorld::default())
        .expect("restore terminal queued provider decision");
    assert!(
        restored.provider_completed_decisions.is_empty(),
        "terminal marker must dominate queued provider decisions after restore"
    );
    assert!(
        restored.provider_held_decisions.is_empty(),
        "terminal marker must clear held provider decisions after restore"
    );
    assert!(
        restored.provider_contexts.is_empty(),
        "terminal marker cleanup must release the exact old identity"
    );
    let _ = std::fs::remove_file(path);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn provider_lineage_restore_terminal_marker_suppresses_queued_action_replay() {
    assert_terminal_marker_suppresses_queued_provider_decision(AgentDecision::Act(
        oasis7_simulator::Action::MoveAgent {
            agent_id: "agent-terminal-queued".to_string(),
            to: "loc-terminal-queued".to_string(),
        },
    ));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn provider_lineage_restore_terminal_marker_suppresses_queued_wait_replay() {
    assert_terminal_marker_suppresses_queued_provider_decision(AgentDecision::Wait);
}

#[test]
fn provider_transport_exhaustion_closes_recovered_runtime_prefix_before_release() {
    let mut world = crate::runtime_host::world::new();
    world
        .bind_cognition_runtime("world", "recovery-branch", 0, None, "pending", 0)
        .expect("Runtime cognition binding");
    world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "agent-recovery".to_string(),
        pos: GeoPos::new(0, 0, 0),
    });
    world.step().expect("register Runtime agent");
    let mut context =
        test_provider_context("agent-recovery", "turn-recovery", "request-recovery", 1);
    let digest = crate::simulator::h_v1("oasis7.test.recovery-digest.v1", &"recovery");
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
        .expect("persist recovered provider Runtime prefix");

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_decision_mode(ViewerLiveDecisionMode::Llm),
    )
    .expect("Runtime live server");
    server.world = world;
    server
        .llm_sidecar
        .provider_agent_ids
        .insert("agent-recovery".to_string());
    server
        .llm_sidecar
        .provider_contexts
        .insert("agent-recovery".to_string(), context.clone());
    server
        .llm_sidecar
        .provider_active_turns
        .insert("agent-recovery".to_string(), context);
    server
        .llm_sidecar
        .provider_transport_exhausted
        .insert("agent-recovery".to_string());

    let _trace = server
        .enqueue_llm_action_from_sidecar()
        .expect_err("transport exhaustion must return a typed terminal trace");
    let events = server.world.cognition()["cognition_journal"]["events"]
        .as_array()
        .expect("Runtime cognition journal events");
    assert!(
        events.iter().any(|event| {
            event["event_kind"] == "CognitionTurnFailed"
                && event["agent_id"] == "agent-recovery"
                && event["agent_turn_id"] == "turn-recovery"
        }),
        "recovery must close the Runtime turn before sidecar release: {events:?}"
    );
    assert!(
        events.iter().any(|event| {
            event["event_kind"] == "CognitionTurnCompleted"
                && event["status"] == "failed"
                && event["agent_id"] == "agent-recovery"
                && event["agent_turn_id"] == "turn-recovery"
        }),
        "recovery must append a failed Runtime completion: {events:?}"
    );
    assert!(
        server.world.cognition_in_flight_wakes().unwrap().is_empty(),
        "a recovered provider failure must not leave a Runtime wake in flight"
    );
}

#[test]
fn restored_mismatched_active_identity_terminalizes_runtime_before_release() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-provider-quarantine-terminal-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let agent_id = "agent-mismatch-recovery";
    let mut runtime_context = test_provider_context(
        agent_id,
        "turn-runtime-recovery",
        "request-runtime-recovery",
        1,
    );
    let digest = crate::simulator::h_v1(
        "oasis7.test.quarantine-terminal-digest.v1",
        &"quarantine-terminal",
    );
    runtime_context.request_context.observation_digest = digest.clone();
    runtime_context.request_context.capability_catalog_digest = digest.clone();
    runtime_context
        .request_context
        .capability_invocation_context_digest = digest.clone();
    runtime_context.request_context.memory_snapshot_digest = digest.clone();
    runtime_context.request_context.goal_snapshot_digest = digest.clone();
    runtime_context.request_context.continuation_digest = digest.clone();
    runtime_context
        .request_context
        .runtime_binding
        .base_world_hash = digest.clone();
    runtime_context
        .request_context
        .runtime_binding
        .runtime_manifest_hash = digest.clone();
    runtime_context.request_context.request_digest = digest.clone();
    runtime_context.turn_context.request_digest = digest;
    let mut active_marker = runtime_context.clone();
    active_marker.request_context.agent_turn_id = "turn-marker-recovery".to_string();
    active_marker.turn_context.agent_turn_id = "turn-marker-recovery".to_string();
    active_marker.request_context.agent_session_id = "session-marker-recovery".to_string();
    active_marker.turn_context.agent_session_id = "session-marker-recovery".to_string();
    let marker_digest = crate::simulator::h_v1(
        "oasis7.test.quarantine-terminal-marker-digest.v1",
        &"quarantine-terminal-marker",
    );
    active_marker.request_context.request_digest = marker_digest.clone();
    active_marker.turn_context.request_digest = marker_digest;

    let mut world = crate::runtime_host::world::new();
    world
        .bind_cognition_runtime("world", "quarantine-branch", 0, None, "pending", 0)
        .expect("Runtime cognition binding");
    world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: GeoPos::new(0, 0, 0),
    });
    world.step().expect("register Runtime agent");
    super::async_support::runtime_provider_prefix(&mut world, &active_marker)
        .expect("persist Runtime prefix for mismatched recovery identity");

    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    first.provider_agent_ids.insert(agent_id.to_string());
    first
        .provider_active_turns
        .insert(agent_id.to_string(), active_marker.clone());
    first
        .provider_contexts
        .insert(agent_id.to_string(), runtime_context);
    first
        .persist_provider_lineage()
        .expect("persist mismatched active identity");

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
        .expect("restore mismatched active identity");
    assert!(
        server
            .llm_sidecar
            .provider_transport_exhausted
            .contains(agent_id),
        "restore must route the quarantine into terminalization"
    );
    let _trace = server
        .enqueue_llm_action_from_sidecar()
        .expect_err("quarantine must emit a terminal recovery trace");
    let events = server.world.cognition()["cognition_journal"]["events"]
        .as_array()
        .expect("Runtime cognition journal events");
    assert!(
        events.iter().any(|event| {
            event["event_kind"] == "CognitionTurnFailed"
                && event["agent_id"] == agent_id
                && event["agent_turn_id"] == "turn-marker-recovery"
        }),
        "restored mismatch must terminalize the correlated Runtime turn: {events:?}"
    );
    assert!(
        events.iter().any(|event| {
            event["event_kind"] == "CognitionTurnCompleted"
                && event["status"] == "failed"
                && event["agent_id"] == agent_id
                && event["agent_turn_id"] == "turn-marker-recovery"
        }),
        "restored mismatch must append failed Runtime completion: {events:?}"
    );
    let feedback = server
        .world
        .runtime_feedback_outbox()
        .expect("Runtime feedback outbox");
    assert!(
        feedback.iter().any(|record| {
            record.agent_subject == agent_id
                && record.agent_session_id == "session-marker-recovery"
                && record.agent_turn_id == "turn-marker-recovery"
                && record.decision_request_id == "request-runtime-recovery"
                && record.request_digest == active_marker.request_context.request_digest.to_string()
        }),
        "quarantine feedback must bind the Runtime-terminalized active marker identity: {feedback:?}"
    );
    assert!(
        feedback.iter().all(|record| {
            record.agent_session_id != "session-test"
                && record.agent_turn_id != "turn-runtime-recovery"
        }),
        "conflicting provider context must not supply feedback identity: {feedback:?}"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn restored_missing_active_identity_terminalizes_runtime_and_feedback() {
    let path = std::env::temp_dir().join(format!(
        "oasis7-provider-missing-terminal-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let agent_id = "agent-missing-terminal";
    let mut active_marker = test_provider_context(
        agent_id,
        "turn-missing-terminal",
        "request-missing-terminal",
        1,
    );
    active_marker.request_context.agent_session_id = "session-missing-terminal".to_string();
    active_marker.turn_context.agent_session_id = "session-missing-terminal".to_string();
    let marker_digest = crate::simulator::h_v1(
        "oasis7.test.missing-terminal-digest.v1",
        &"missing-terminal",
    );
    active_marker.request_context.observation_digest = marker_digest.clone();
    active_marker.request_context.capability_catalog_digest = marker_digest.clone();
    active_marker
        .request_context
        .capability_invocation_context_digest = marker_digest.clone();
    active_marker.request_context.memory_snapshot_digest = marker_digest.clone();
    active_marker.request_context.goal_snapshot_digest = marker_digest.clone();
    active_marker.request_context.continuation_digest = marker_digest.clone();
    active_marker
        .request_context
        .runtime_binding
        .base_world_hash = marker_digest.clone();
    active_marker
        .request_context
        .runtime_binding
        .runtime_manifest_hash = marker_digest.clone();
    active_marker.request_context.request_digest = marker_digest.clone();
    active_marker.turn_context.request_digest = marker_digest;

    let mut world = crate::runtime_host::world::new();
    world
        .bind_cognition_runtime("world", "missing-terminal-branch", 0, None, "pending", 0)
        .expect("Runtime cognition binding");
    world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: GeoPos::new(0, 0, 0),
    });
    world.step().expect("register Runtime agent");
    super::async_support::runtime_provider_prefix(&mut world, &active_marker)
        .expect("persist Runtime prefix for missing identity");

    let mut first = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    first.configure_provider_lineage_store(path.clone());
    first.provider_agent_ids.insert(agent_id.to_string());
    first
        .provider_active_turns
        .insert(agent_id.to_string(), active_marker.clone());
    // Deliberately omit provider_contexts: restore has only the Runtime active
    // marker and must carry it through terminalization and feedback.
    first
        .persist_provider_lineage()
        .expect("persist missing active identity");

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
        .expect("restore missing active identity");
    assert!(
        server
            .llm_sidecar
            .provider_recovery_context(agent_id)
            .is_some(),
        "missing active identity must remain recoverable before the control pass"
    );
    let _trace = server
        .enqueue_llm_action_from_sidecar()
        .expect_err("missing identity must emit a terminal recovery trace");
    let events = server.world.cognition()["cognition_journal"]["events"]
        .as_array()
        .expect("Runtime cognition journal events");
    assert!(
        events.iter().any(|event| {
            event["event_kind"] == "CognitionTurnFailed"
                && event["agent_id"] == agent_id
                && event["agent_session_id"] == "session-missing-terminal"
                && event["agent_turn_id"] == "turn-missing-terminal"
                && event["decision_request_id"] == "request-missing-terminal"
                && event["request_digest"]
                    == active_marker.request_context.request_digest.to_string()
        }),
        "missing identity must produce Runtime terminalization evidence: {events:?}"
    );
    assert!(
        events.iter().any(|event| {
            event["event_kind"] == "CognitionTurnCompleted"
                && event["status"] == "failed"
                && event["agent_id"] == agent_id
                && event["agent_session_id"] == "session-missing-terminal"
                && event["agent_turn_id"] == "turn-missing-terminal"
                && event["decision_request_id"] == "request-missing-terminal"
                && event["request_digest"]
                    == active_marker.request_context.request_digest.to_string()
        }),
        "missing identity must produce a Runtime failed completion: {events:?}"
    );
    let feedback = server
        .world
        .runtime_feedback_outbox()
        .expect("Runtime feedback outbox");
    assert!(
        feedback.iter().any(|record| {
            record.agent_subject == agent_id
                && record.agent_session_id == "session-missing-terminal"
                && record.agent_turn_id == "turn-missing-terminal"
                && record.decision_request_id == "request-missing-terminal"
                && record.request_digest == active_marker.request_context.request_digest.to_string()
        }),
        "missing identity feedback must preserve the Runtime marker identity: {feedback:?}"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn provider_wake_handoff_failure_retains_terminal_identity_for_recovery() {
    // Feedback recovery reads env-backed transport settings; isolate selection.
    let _env_guard = crate::viewer::runtime_live::canonical_runtime_provider_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _provider_env_snapshot = ProviderEnvSnapshot::capture(&[
        VIEWER_AGENT_DECISION_SOURCE_ENV,
        VIEWER_AGENT_PROVIDER_MODE_ENV,
        VIEWER_AGENT_PROVIDER_URL_ENV,
    ]);

    let lineage_path = std::env::temp_dir().join(format!(
        "oasis7-provider-wake-recovery-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    let mut server = setup_wake_handoff_recovery_fixture(&lineage_path);
    provider_wake_stack_diag(&format!(
        "fixture-ready server_size={} world_size={}",
        std::mem::size_of::<ViewerRuntimeLiveServer>(),
        std::mem::size_of::<RuntimeWorld>()
    ));

    let trace = server
        .enqueue_llm_action_from_sidecar()
        .expect_err("wake authority failure must return a terminal trace");
    provider_wake_stack_diag("first-enqueue-returned");
    assert!(
        trace
            .llm_error
            .as_deref()
            .is_some_and(|error| error.contains("Runtime wake handoff failed")),
        "trace must expose the real Runtime wake return error: {trace:?}"
    );
    assert!(
        server
            .llm_sidecar
            .provider_recovery_context("agent-wake-recovery")
            .is_some(),
        "wake handoff failure must retain the terminal identity for retry"
    );
    assert!(
        server
            .llm_sidecar
            .provider_wake_recovery_pending("agent-wake-recovery")
            .is_some(),
        "wake handoff failure must leave a durable wake recovery marker"
    );
    let checkpoint: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&lineage_path).expect("read wake recovery checkpoint"),
    )
    .expect("decode wake recovery checkpoint");
    assert!(
        checkpoint["provider_wake_recovery_pending"]
            .as_object()
            .is_some_and(|values| values.contains_key("agent-wake-recovery")),
        "wake recovery marker must be persisted for restart: {checkpoint:?}"
    );
    provider_wake_stack_diag("first-checkpoint-verified");

    // An unresolved wake belongs only to its own Agent. A completed sibling
    // response in the queue must still make progress in the next control
    // pass while the wake retry continues to fail.
    server.world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "agent-wake-sibling".to_string(),
        pos: GeoPos::new(0, 0, 0),
    });
    server.world.step().expect("register wake recovery sibling");
    server
        .world
        .install_test_provider_capability_fixture("agent-wake-sibling")
        .expect("install sibling Runtime capability and cognition provision");
    provider_wake_stack_diag("sibling-world-ready");
    let sibling_cognition =
        super::lineage_tests::queued_wait_cognition(&mut server.world, "agent-wake-sibling");
    let fenced_provider = crate::simulator::MockDecisionProvider::new("wake-recovery-fenced");
    let fenced_behavior = crate::simulator::ProviderBackedAgentBehavior::new_legacy_compatibility(
        "agent-wake-recovery",
        fenced_provider,
        vec![crate::simulator::ActionCatalogEntry::new("wait", "wait")],
    );
    let sibling_provider = crate::simulator::MockDecisionProvider::new("wake-recovery-sibling");
    let sibling_behavior = crate::simulator::ProviderBackedAgentBehavior::new_legacy_compatibility(
        "agent-wake-sibling",
        sibling_provider,
        vec![crate::simulator::ActionCatalogEntry::new("wait", "wait")],
    );
    let mut sibling_runner = crate::simulator::AsyncAgentRunner::with_default_capacity();
    sibling_runner
        .register(fenced_behavior)
        .expect("register fenced wake recovery provider actor");
    sibling_runner
        .register(sibling_behavior)
        .expect("register wake recovery sibling provider actor");
    server.llm_sidecar.runner = Some(RuntimeDecisionRunner::ProviderBacked(sibling_runner));
    server
        .llm_sidecar
        .provider_agent_ids
        .insert("agent-wake-sibling".to_string());
    server
        .llm_sidecar
        .provider_completed_decisions
        .push_back(async_support::RuntimeLlmDecision {
            agent_id: "agent-wake-sibling".to_string(),
            decision: AgentDecision::Wait,
            decision_trace: None,
            cognition: Some(sibling_cognition),
            memory_write_intents: Vec::new(),
            continuation_admitted: false,
        });
    server.llm_sidecar.request_decision();
    let sibling_result = server.enqueue_llm_action_from_sidecar();
    provider_wake_stack_diag("sibling-enqueue-returned");
    assert!(
        sibling_result.is_ok(),
        "an unresolved Agent A wake must not block sibling B: {sibling_result:?}"
    );
    assert!(
        server
            .llm_sidecar
            .provider_held_decisions
            .contains_key("agent-wake-sibling"),
        "healthy sibling B must be delivered while Agent A wake recovery remains pending"
    );

    let mut restarted = restart_wake_handoff_recovery_fixture(&server, &lineage_path);
    provider_wake_stack_diag("restart-restored");
    assert!(
        restarted
            .llm_sidecar
            .provider_wake_recovery_pending("agent-wake-recovery")
            .is_some(),
        "restart must retain the exact wake recovery identity"
    );
    let retry = restarted.retry_provider_wake_recovery();
    provider_wake_stack_diag("restart-retry-returned");
    assert!(
        retry.is_err(),
        "retry must expose the same Runtime wake authority error"
    );
    assert!(
        restarted
            .llm_sidecar
            .provider_recovery_context("agent-wake-recovery")
            .is_some(),
        "failed retry must retain identity rather than redispatching"
    );
    let _ = std::fs::remove_file(lineage_path);
}
