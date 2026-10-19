use super::*;

#[test]
fn runtime_continuation_hydration_reads_typed_active_projection() {
    let mut world = crate::runtime_host::world::new();
    let mut continuation: crate::runtime::AgentContinuation =
        serde_json::from_value(serde_json::json!({
            "schema_version": "agent-continuation.v1",
            "continuation_id": "continuation-typed-readback",
            "wake_id": "wake-typed-readback",
            "world_id": "world-typed-readback",
            "branch_id": "main",
            "finality_epoch": 0,
            "finality_block_hash": null,
            "finality_status": "pending",
            "reorg_epoch": 0,
            "runtime_manifest_hash": "manifest-typed-readback",
            "agent_id": "agent-typed-readback",
            "agent_session_id": "session-typed-readback",
            "agent_turn_id": "turn-typed-readback",
            "decision_request_id": "request-typed-readback",
            "origin_turn_id": "turn-typed-readback",
            "origin_request_digest": "origin-typed-readback",
            "continuation_proposal_id": "proposal-typed-readback",
            "proposal_digest": "proposal-digest-typed-readback",
            "action_or_envelope_digest": null,
            "wake_conditions": [{
                "schema_version": "wake-condition.v1",
                "kind": "at_or_after_tick",
                "logical_tick": 0
            }],
            "next_wake_tick": 0,
            "remaining_budget": {"unit": "steps", "value": 1},
            "valid_until_tick": 10,
            "precondition_digest": "precondition-typed-readback",
            "wake_seq": 1,
            "logical_tick": 0,
            "status": "scheduled",
            "terminal_disposition": null
        }))
        .expect("typed continuation fixture");
    continuation.refresh_status_digest();
    continuation
        .validate_authoritative()
        .expect("typed continuation fixture is authoritative");
    let mut snapshot = world.snapshot();
    snapshot.cognition["continuations"] = serde_json::to_value([continuation.clone()]).unwrap();
    world = crate::runtime_host::world::from_snapshot(snapshot, world.journal().clone())
        .expect("restore typed continuation fixture through public checkpoint boundary");
    let wake: crate::runtime::SchedulerWakeV1 = serde_json::from_value(serde_json::json!({
        "schema_version": "scheduler-wake.v1",
        "wake_id": "wake-typed-readback",
        "continuation_id": "continuation-typed-readback",
        "world_id": "world-typed-readback",
        "branch_id": "main",
        "finality_epoch": 0,
        "finality_block_hash": "genesis",
        "finality_status": "pending",
        "reorg_epoch": 0,
        "runtime_manifest_hash": "manifest-typed-readback",
        "agent_id": "agent-typed-readback",
        "agent_session_id": "session-typed-readback",
        "agent_turn_id": "turn-typed-readback",
        "decision_request_id": "request-typed-readback",
        "next_wake_tick": 0,
        "eligible_since_tick": 0,
        "starvation_deadline_tick": 1,
        "initial_priority": 0,
        "wake_seq": 1,
        "retry_seq": 0,
        "status": "pending",
        "pending_reason": "capacity_available"
    }))
    .expect("typed wake fixture");

    assert_eq!(
        active_runtime_continuation_for_wake(&world, &wake)
            .expect("typed active continuation")
            .continuation_id,
        continuation.continuation_id
    );
}

#[test]
fn fresh_resume_leaves_past_schedule_tick_for_runtime_derivation() {
    let world = crate::runtime_host::world::new();
    let continuation: crate::runtime::AgentContinuation = serde_json::from_value(serde_json::json!({
        "schema_version": "agent-continuation.v1",
        "continuation_id": "continuation-past", "wake_id": "wake-past",
        "world_id": "world", "branch_id": "main", "finality_epoch": 0,
        "finality_block_hash": null, "finality_status": "pending", "reorg_epoch": 0,
        "runtime_manifest_hash": "manifest", "agent_id": "agent-a",
        "agent_session_id": "session-old", "agent_turn_id": "turn-old",
        "decision_request_id": "request-old", "origin_turn_id": "turn-old",
        "origin_request_digest": "origin-old", "continuation_proposal_id": "proposal-old",
        "proposal_digest": "digest-old", "action_or_envelope_digest": null,
        "wake_conditions": [{"schema_version": "wake-condition.v1", "kind": "at_or_after_tick", "logical_tick": 5}],
        "next_wake_tick": 5, "remaining_budget": {"unit": "steps", "value": 2},
        "valid_until_tick": 21, "precondition_digest": "precondition-old",
        "wake_seq": 1, "logical_tick": 5, "status": "waking", "terminal_disposition": null
    })).unwrap();
    let wake: crate::runtime::SchedulerWakeV1 = serde_json::from_value(serde_json::json!({
        "schema_version": "scheduler-wake.v1", "wake_id": "wake-past",
        "continuation_id": "continuation-past", "world_id": "world", "branch_id": "main",
        "finality_epoch": 0, "finality_block_hash": null, "finality_status": "pending",
        "reorg_epoch": 0, "runtime_manifest_hash": "manifest", "agent_id": "agent-a",
        "agent_session_id": "session-old", "agent_turn_id": "turn-old", "decision_request_id": "request-old",
        "next_wake_tick": 5, "eligible_since_tick": 5, "starvation_deadline_tick": 21,
        "initial_priority": 0, "wake_seq": 1, "retry_seq": 0, "status": "pending",
        "pending_reason": "capacity_available"
    })).unwrap();
    let mut projection = crate::world_service::projection::WorldServiceProjection {
        formal_commit_header: None,
        canonical_agent_owner: None,
        canonical_agent_chat: None,
        game_task_control: None,
        native_delegation_authorizations: Vec::new(),
        state: world.state().clone(),
        events: vec![],
        runtime_binding: None,
        agent_context: None,
        feedback_history: None,
        scheduler_wakes: vec![wake.clone()],
        continuations: vec![continuation.clone()],
        cognition_leases: vec![],
        continuation_contexts: BTreeMap::new(),
    };
    projection.state.time = 9;
    projection.continuation_contexts.insert(
        continuation.continuation_id.clone(),
        crate::world_service::projection::WorldServiceContinuationContext {
            baseline_observation_digest: "observation-old".into(),
            goal_digest: "goal-old".into(),
            policy_digest: "policy-old".into(),
            policy_revision: 1,
            precondition_summary: "precondition".into(),
            precondition_digest: "precondition-old".into(),
        },
    );
    let (_, proposal) = runtime_continuation_for_wake_with_identity(
        &world,
        Some(&projection),
        &wake,
        "session-new",
        2,
    )
    .unwrap();
    proposal.validate().unwrap();
    assert_eq!(proposal.next_wake_tick, None);
    let derived = crate::runtime::WakeConditionValidator::next_wake_tick_at(
        &proposal.wake_conditions,
        projection.state.time,
    )
    .unwrap();
    assert_eq!(derived, Some(9));
    assert_ne!(continuation.next_wake_tick, derived);
    assert_eq!(proposal.wake_conditions, continuation.wake_conditions);
    assert_eq!(proposal.valid_until_tick, continuation.valid_until_tick);
    assert_eq!(proposal.remaining_budget.value, 1);
    assert_eq!(
        proposal.origin_request_digest,
        continuation.origin_request_digest
    );
    assert_eq!(proposal.baseline_observation_digest, "observation-old");
    let mut admitted = proposal.clone();
    admitted.next_wake_tick = derived;
    assert_eq!(admitted.proposal_digest(), proposal.proposal_digest);

    // Losing the selected wake must not turn an incomplete original
    // checkpoint into a fresh request or an inferred committed handoff.
    let payload = crate::world_service::wire::WorldServicePayloadV1::Scheduler(
        crate::world_service::authority::sign_read_request(
            "scheduler",
            crate::world_service::wire::SchedulerIntentV1 {
                agent_id: "agent-a".into(),
                request_id: "original:resume:wake-past".into(),
                delegation_generation: 1,
                captured_base_binding: crate::runtime::RuntimeCognitionBaseBindingV1 {
                    world_id: "world".into(),
                    branch_id: "main".into(),
                    finality_epoch: 0,
                    finality_block_hash: None,
                    finality_status: "pending".into(),
                    base_tick: 9,
                    base_world_hash: "0".repeat(64),
                    reorg_epoch: 0,
                    runtime_manifest_hash: "0".repeat(64),
                },
                operation: crate::world_service::wire::SchedulerOperationV1::ResumeWake {
                    wake_id: wake.wake_id.clone(),
                    proposal: proposal.clone(),
                    budget_spent: 1,
                    resume: crate::runtime::CognitionContinuationResumeRequestV1 {
                        agent_session_id: proposal.agent_session_id.clone(),
                        agent_turn_id: proposal.agent_turn_id.clone(),
                        decision_request_id: proposal.decision_request_id.clone(),
                        request_digest: "0".repeat(64),
                        context_digest: "0".repeat(64),
                    },
                    current_context: crate::runtime::CognitionContextDigestsV1::from_proposal(
                        &proposal,
                    ),
                },
            },
            &"11".repeat(32),
        )
        .unwrap(),
    );
    let correlation = crate::world_service::derive_correlation(
        oasis7_client_api::world_service::WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "0".repeat(64),
        },
        &payload,
    )
    .unwrap();
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.provider_service_required = true;
    sidecar.provider_scheduler_pending.insert(
        "original:resume:wake-past".into(),
        lineage_persistence::PendingProviderSchedulerIntent {
            correlation,
            payload,
            resume_context: None,
            resume_current_context: None,
        },
    );
    let before = serde_json::to_value(&sidecar.provider_scheduler_pending).unwrap();
    let binding = crate::simulator::RuntimeBindingV1 {
        world_id: "world".into(),
        branch_id: "main".into(),
        finality_epoch: 0,
        finality_block_hash: None,
        finality_status: "pending".into(),
        base_tick: 9,
        base_world_hash: h_v1("test-world", &"world"),
        reorg_epoch: 0,
        runtime_manifest_hash: h_v1("test-manifest", &"world"),
    };
    assert!(
        sidecar
            .recover_consumed_service_resumes(&binding)
            .unwrap_err()
            .contains("original context missing")
    );
    assert_eq!(
        serde_json::to_value(&sidecar.provider_scheduler_pending).unwrap(),
        before
    );
    assert!(sidecar.provider_contexts.is_empty());
    assert!(sidecar.pending_runtime_wakes.is_empty());
}
