use super::*;
use crate::world_service::{
    client::{WorldServiceAgentSignerConfig, WorldServiceClientConfig},
    projection::WorldServiceProjection,
    wire::{FeedbackAckIntentV1, SchedulerOperationV1, WorldServicePayloadV1},
};
use oasis7_client_api::world_service::WorldIdentity;

fn service_cognition() -> RuntimeProviderActionContext {
    let mut request = super::super::super::tests::test_provider_context(
        "agent-handoff-test",
        "turn-handoff-test",
        "request-handoff-test",
        1,
    );
    let request_context = &mut request.request_context;
    let digest = |domain: &str| crate::simulator::h_v1(domain, &"handoff-fixture");
    request_context.runtime_binding.base_world_hash = digest("handoff-fixture-world");
    request_context.runtime_binding.runtime_manifest_hash = digest("handoff-fixture-manifest");
    request_context.observation_digest = digest("handoff-fixture-observation");
    request_context.capability_catalog_digest = digest("handoff-fixture-catalog");
    request_context.capability_invocation_context_digest = digest("handoff-fixture-invocation");
    request_context.memory_snapshot_digest = digest("handoff-fixture-memory");
    request_context.goal_snapshot_digest = digest("handoff-fixture-goal");
    request_context.continuation_digest = digest("handoff-fixture-continuation");
    request_context.request_digest = request_context.request_digest();
    request.turn_context.request_digest = request_context.request_digest.clone();
    let response_context = request.request_context.clone();
    let response = crate::simulator::DecisionResponse::wait("handoff-test-provider");
    RuntimeProviderActionContext {
        request,
        response: crate::simulator::ContinuousAgentResponseContextV1 {
            response_digest: crate::simulator::cognition_response_digest(&response),
            base_decision_response: response,
            context_discriminator: crate::simulator::CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR
                .to_string(),
            context_version: crate::simulator::CONTINUOUS_AGENT_CONTEXT_VERSION,
            agent_session_id: response_context.agent_session_id,
            agent_turn_id: response_context.agent_turn_id,
            decision_request_id: response_context.decision_request_id,
            retry_seq: response_context.retry_seq,
            transport_attempt: response_context.transport_attempt,
            request_digest: response_context.request_digest,
        },
        cognition_lease: None,
        memory_write_intents: Vec::new(),
    }
}

fn wake_for(
    cognition: &RuntimeProviderActionContext,
    wake_id: &str,
) -> crate::runtime::SchedulerWakeV1 {
    let request = &cognition.request.request_context;
    let binding = &request.runtime_binding;
    crate::runtime::SchedulerWakeV1 {
        schema_version: crate::runtime::SchedulerWakeV1::SCHEMA_VERSION.to_string(),
        wake_id: wake_id.to_string(),
        continuation_id: format!("continuation-{wake_id}"),
        world_id: binding.world_id.clone(),
        branch_id: binding.branch_id.clone(),
        finality_epoch: binding.finality_epoch,
        finality_block_hash: binding
            .finality_block_hash
            .as_ref()
            .map(ToString::to_string),
        finality_status: binding.finality_status.clone(),
        reorg_epoch: binding.reorg_epoch,
        runtime_manifest_hash: binding.runtime_manifest_hash.to_string(),
        agent_id: request.agent_subject.clone(),
        agent_session_id: request.agent_session_id.clone(),
        agent_turn_id: request.agent_turn_id.clone(),
        decision_request_id: request.decision_request_id.clone(),
        request_digest: request.request_digest.to_string(),
        next_wake_tick: 12,
        eligible_since_tick: 12,
        starvation_deadline_tick: 20,
        initial_priority: 0,
        wake_seq: 1,
        retry_seq: 0,
        status: "pending".into(),
        pending_reason: "capacity_available".into(),
    }
}

fn configured_sidecar(
    cognition: &RuntimeProviderActionContext,
    wakes: Vec<crate::runtime::SchedulerWakeV1>,
    lineage_path: std::path::PathBuf,
) -> RuntimeLlmSidecar {
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    let binding = cognition.request.request_context.runtime_binding.clone();
    let mut projection =
        WorldServiceProjection::from_world(&crate::runtime_host::world::new(), None)
            .expect("projection");
    projection.runtime_binding = Some(binding.clone());
    projection.scheduler_wakes = wakes;
    sidecar.provider_service_projection = Some(projection);
    sidecar.provider_service_config = Some(WorldServiceClientConfig {
        endpoint: "http://127.0.0.1:1".into(),
        trusted_service_public_key: hex::encode([12; 32]),
        expected_world: WorldIdentity {
            world_id: binding.world_id,
            genesis_digest: "0".repeat(64),
        },
        scope_id: "public".into(),
        read_private_key_hex: hex::encode([13; 32]),
        timeout: std::time::Duration::from_millis(10),
        formal_read_trust: None,
        max_response_bytes: 4096,
    });
    sidecar.provider_service_signer = Some(WorldServiceAgentSignerConfig {
        private_key_hex: hex::encode([14; 32]),
        delegation_generation: 3,
    });
    sidecar.provider_lineage_store = Some(lineage_path);
    sidecar.provider_service_lineage_store_explicit = true;
    sidecar.provider_service_required = true;
    sidecar
}

fn pending_with_feedback_ack(
    cognition: &RuntimeProviderActionContext,
) -> lineage_persistence::PendingProviderServiceIntent {
    let request = &cognition.request.request_context;
    let signer = hex::encode([14u8; 32]);
    let intent = FeedbackAckIntentV1 {
        agent_id: request.agent_subject.clone(),
        agent_session_id: request.agent_session_id.clone(),
        agent_turn_id: request.agent_turn_id.clone(),
        decision_request_id: request.decision_request_id.clone(),
        request_digest: request.request_digest.to_string(),
        feedback_id: "feedback-issued-checkpoint-test".into(),
        feedback_seq: 1,
        original_envelope_digest: crate::simulator::h_v1("feedback", &"original").to_string(),
        private_acceptance_digest: crate::simulator::h_v1("acceptance", &"private").to_string(),
        runtime_receipt_id: Some("receipt-issued-checkpoint-test".into()),
        delegation_generation: 3,
    };
    let signed =
        crate::world_service::authority::sign_read_request("feedback_ack", intent, &signer)
            .expect("signed feedback ACK fixture");
    let payload = WorldServicePayloadV1::FeedbackAck(signed);
    let world = WorldIdentity {
        world_id: request.runtime_binding.world_id.clone(),
        genesis_digest: "0".repeat(64),
    };
    let correlation = crate::world_service::derive_correlation(world, &payload)
        .expect("feedback ACK correlation");
    let ack = super::super::service_feedback_ack::PendingFeedbackAck {
        correlation: correlation.clone(),
        payload: payload.clone(),
        original_feedback: serde_json::json!({"feedback_id":"feedback-issued-checkpoint-test"}),
        receipt_lineage: crate::runtime::RuntimeReceiptLineageV1 {
            schema_version: "oasis7.runtime.receipt_lineage.v1".into(),
            status: "committed".into(),
            receipt_id: "receipt-issued-checkpoint-test".into(),
            receipt_digest: crate::simulator::h_v1("receipt", &"digest").to_string(),
            envelope_digest: crate::simulator::h_v1("feedback", &"original").to_string(),
            action_id: "action:1".into(),
            agent_id: request.agent_subject.clone(),
            agent_session_id: request.agent_session_id.clone(),
            agent_turn_id: request.agent_turn_id.clone(),
            decision_request_id: request.decision_request_id.clone(),
            request_digest: request.request_digest.to_string(),
            feedback_id: "feedback-issued-checkpoint-test".into(),
        },
        delivered: false,
        issued: false,
    };
    lineage_persistence::PendingProviderServiceIntent {
        correlation,
        payload,
        cognition: cognition.clone(),
        action: oasis7_simulator::Action::RemoveModuleVisualEntity {
            entity_id: "issued-checkpoint-test".into(),
        },
        feedback_ack: Some(ack),
    }
}

#[test]
fn feedback_ack_issued_persistence_failure_retains_both_checkpoint_views() {
    let cognition = service_cognition();
    let pending = pending_with_feedback_ack(&cognition);
    let agent = cognition.request.request_context.agent_subject.clone();
    let path = lineage_path("ack-issued-write-failure");
    let mut sidecar = configured_sidecar(&cognition, Vec::new(), path.clone());
    sidecar
        .provider_service_pending
        .insert(agent.clone(), pending.clone());
    sidecar
        .persist_provider_lineage()
        .expect("persist original non-issued ACK checkpoint");
    let original_bytes = std::fs::read(&path).expect("original checkpoint bytes");
    let original_pending = serde_json::to_vec(&pending).expect("original pending bytes");
    let mut phase_pending = pending.clone();
    let backup = path.with_extension("original");
    std::fs::rename(&path, &backup).expect("retain exact original checkpoint");
    std::fs::create_dir(&path).expect("create real rename failure at checkpoint target");

    let error = sidecar
        .persist_feedback_ack_issued(&agent, &mut phase_pending)
        .expect_err("failed durable commit must not issue or advance the owned phase");
    assert!(
        error.contains("provider lineage checkpoint commit failed"),
        "expected an actual atomic checkpoint commit failure, got {error}"
    );
    assert_eq!(
        serde_json::to_vec(&sidecar.provider_service_pending[&agent]).unwrap(),
        original_pending,
        "durable pending intent must roll back byte-for-byte"
    );
    assert_eq!(
        serde_json::to_vec(&phase_pending).unwrap(),
        original_pending,
        "phase identity must not advance before the durable commit"
    );
    assert!(sidecar.hosted_service_phase.is_none());

    std::fs::remove_dir(&path).expect("remove persistence blocker");
    std::fs::rename(&backup, &path).expect("restore original checkpoint");
    assert_eq!(
        std::fs::read(&path).expect("restored checkpoint bytes"),
        original_bytes,
        "restore must retain the exact original signed checkpoint"
    );
    sidecar
        .persist_feedback_ack_issued(&agent, &mut phase_pending)
        .expect("retry exact issuance after restoring persistence");
    assert!(
        sidecar.provider_service_pending[&agent]
            .feedback_ack
            .as_ref()
            .unwrap()
            .issued
    );
    assert!(phase_pending.feedback_ack.as_ref().unwrap().issued);
    let persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        persisted["provider_service_pending"][&agent]["feedback_ack"]["issued"],
        true
    );
    std::fs::remove_file(path).expect("remove fixture checkpoint");
}

fn handoff_operation(wake_id: &str) -> SchedulerOperationV1 {
    let digest = |name: &str| crate::simulator::h_v1("oasis7.test.handoff.v1", &name).to_string();
    SchedulerOperationV1::HandoffWake {
        wake_id: wake_id.to_string(),
        disposition: crate::runtime::CognitionWakeDispositionV1::Terminal {
            status: crate::runtime::ContinuationStatusV1::Completed,
            reason: "provider_action_committed".into(),
        },
        current_context: crate::runtime::CognitionContextDigestsV1 {
            baseline_observation_digest: digest("observation"),
            goal_digest: digest("goal"),
            policy_digest: digest("policy"),
            precondition_digest: digest("precondition"),
        },
    }
}

fn persist_recovery_handoff(
    sidecar: &mut RuntimeLlmSidecar,
    cognition: &RuntimeProviderActionContext,
    wake: &crate::runtime::SchedulerWakeV1,
    operation: SchedulerOperationV1,
) -> lineage_persistence::PendingProviderSchedulerIntent {
    sidecar
        .pending_runtime_wakes
        .insert(wake.wake_id.clone(), wake.clone());
    sidecar.retain_provider_wake_recovery_pending(
        &wake.agent_id,
        &cognition.request,
        crate::runtime::ContinuationStatusV1::Completed,
        "provider_action_committed",
    );
    sidecar
        .prepare_service_scheduler_checkpoint(
            &cognition.request.request_context,
            &format!("handoff:{}", wake.wake_id),
            operation,
            None,
        )
        .expect("persist exact signed handoff checkpoint")
        .0
}

fn lineage_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "oasis7-service-wake-{label}-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ))
}

#[test]
fn service_handoff_selects_exact_request_and_reuses_signed_checkpoint() {
    let cognition = service_cognition();
    let target = wake_for(&cognition, "wake-exact-handoff");
    let mut unrelated = target.clone();
    unrelated.wake_id = "wake-other-turn".into();
    unrelated.continuation_id = "continuation-other-turn".into();
    unrelated.agent_turn_id = "turn-other".into();
    unrelated.request_digest = crate::simulator::h_v1("other-request", &"different").to_string();
    let path = lineage_path("exact-checkpoint");
    let mut sidecar = configured_sidecar(&cognition, vec![unrelated, target.clone()], path.clone());
    let request = &cognition.request.request_context;
    let phase = format!("handoff:{}", target.wake_id);
    let operation = handoff_operation(&target.wake_id);
    let (checkpoint, existed) = sidecar
        .prepare_service_scheduler_checkpoint(request, &phase, operation.clone(), None)
        .expect("persist exact signed handoff checkpoint");
    assert!(!existed);
    let checkpoint_bytes = serde_json::to_vec(&checkpoint).expect("checkpoint bytes");

    let (selected_wake, selected_operation) = sidecar
        .committed_service_wake_handoff(&cognition)
        .expect("select exact wake")
        .expect("matching wake exists");
    assert_eq!(selected_wake, target);
    assert_eq!(selected_operation, operation);
    let (retried, existed) = sidecar
        .prepare_service_scheduler_checkpoint(request, &phase, selected_operation, None)
        .expect("reuse exact signed handoff checkpoint");
    assert!(existed);
    assert_eq!(
        serde_json::to_vec(&retried).expect("retried bytes"),
        checkpoint_bytes,
        "recovery must not replace the signed request payload"
    );
    let persisted = std::fs::read(&path).expect("durable lineage checkpoint");
    assert!(
        String::from_utf8_lossy(&persisted).contains(&target.wake_id),
        "the selected handoff request remains persisted"
    );
    std::fs::remove_file(path).expect("remove fixture checkpoint");
}

#[test]
fn service_handoff_accepts_verified_same_lineage_tick_advance() {
    let cognition = service_cognition();
    let target = wake_for(&cognition, "wake-after-verified-commit");
    let path = lineage_path("verified-tick-advance");
    let mut sidecar = configured_sidecar(&cognition, vec![target.clone()], path.clone());
    let current_binding = sidecar
        .provider_service_projection
        .as_mut()
        .expect("projection")
        .runtime_binding
        .as_mut()
        .expect("runtime binding");
    current_binding.base_tick += 1;
    current_binding.base_world_hash =
        crate::simulator::h_v1("handoff-fixture-world-after-commit", &"verified");

    let request = &cognition.request.request_context;
    let (checkpoint, existed) = sidecar
        .prepare_service_scheduler_checkpoint(
            request,
            &format!("handoff:{}", target.wake_id),
            handoff_operation(&target.wake_id),
            None,
        )
        .expect("sign handoff against the verified post-commit binding");
    assert!(!existed);
    let (selected_wake, operation) = sidecar
        .committed_service_wake_handoff(&cognition)
        .expect("verified action advances the same runtime lineage")
        .expect("exact target wake remains available");
    assert_eq!(selected_wake, target);
    assert_eq!(operation, handoff_operation(&selected_wake.wake_id));
    let crate::world_service::wire::WorldServicePayloadV1::Scheduler(signed) = checkpoint.payload
    else {
        panic!("handoff checkpoint payload");
    };
    assert_eq!(
        signed.request.captured_base_binding.base_tick,
        request.runtime_binding.base_tick + 1
    );
    assert_eq!(
        signed.request.captured_base_binding.base_world_hash,
        crate::simulator::h_v1("handoff-fixture-world-after-commit", &"verified").to_string()
    );
    std::fs::remove_file(path).expect("remove fixture checkpoint");
}

fn assert_handoff_fenced_after_binding_mutation(
    label: &str,
    mutate: impl FnOnce(&mut crate::simulator::RuntimeBindingV1),
) {
    let cognition = service_cognition();
    let target = wake_for(&cognition, &format!("wake-invalid-binding-{label}"));
    let path = lineage_path(label);
    let mut sidecar = configured_sidecar(&cognition, vec![target], path.clone());
    mutate(
        sidecar
            .provider_service_projection
            .as_mut()
            .expect("projection")
            .runtime_binding
            .as_mut()
            .expect("runtime binding"),
    );
    let error = sidecar
        .committed_service_wake_handoff(&cognition)
        .expect_err("changed lineage or regressed binding must fail closed");
    assert!(error.contains("changed lineage or regressed"), "{error}");
    assert!(sidecar.provider_scheduler_pending.is_empty());
    assert!(
        !path.exists(),
        "invalid runtime binding cannot sign a handoff"
    );
}

#[test]
fn service_handoff_fences_changed_lineage_and_binding_regression() {
    assert_handoff_fenced_after_binding_mutation("branch-change", |binding| {
        binding.branch_id = "other-branch".into();
    });
    assert_handoff_fenced_after_binding_mutation("reorg-change", |binding| {
        binding.reorg_epoch += 1;
    });
    assert_handoff_fenced_after_binding_mutation("tick-regression", |binding| {
        binding.base_tick -= 1;
    });
    assert_handoff_fenced_after_binding_mutation("same-tick-hash-change", |binding| {
        binding.base_world_hash = crate::simulator::h_v1("different-world", &"same-tick");
    });
}

#[test]
fn service_handoff_rejects_checkpoint_ahead_of_verified_projection() {
    let cognition = service_cognition();
    let target = wake_for(&cognition, "wake-future-checkpoint");
    let path = lineage_path("future-checkpoint");
    let mut sidecar = configured_sidecar(&cognition, vec![target.clone()], path.clone());
    let binding = sidecar
        .provider_service_projection
        .as_mut()
        .expect("projection")
        .runtime_binding
        .as_mut()
        .expect("runtime binding");
    binding.base_tick += 3;
    binding.base_world_hash = crate::simulator::h_v1("future-world", &"checkpoint");
    let request = &cognition.request.request_context;
    let (checkpoint, existed) = sidecar
        .prepare_service_scheduler_checkpoint(
            request,
            &format!("handoff:{}", target.wake_id),
            handoff_operation(&target.wake_id),
            None,
        )
        .expect("persist signed checkpoint at the advanced binding");
    assert!(!existed);
    let checkpoint_bytes = serde_json::to_vec(&checkpoint).expect("checkpoint bytes");

    let binding = sidecar
        .provider_service_projection
        .as_mut()
        .expect("projection")
        .runtime_binding
        .as_mut()
        .expect("runtime binding");
    binding.base_tick = request.runtime_binding.base_tick + 1;
    binding.base_world_hash = crate::simulator::h_v1("current-world", &"verified-view");
    let error = sidecar
        .committed_service_wake_handoff(&cognition)
        .expect_err("a signed checkpoint from a future binding is not current authority");
    assert!(error.contains("checkpoint identity conflict"), "{error}");
    assert_eq!(
        serde_json::to_vec(
            &sidecar.provider_scheduler_pending[&format!(
                "{}:handoff:{}",
                request.provider_invocation_key(),
                target.wake_id
            )]
        )
        .expect("stored checkpoint bytes"),
        checkpoint_bytes,
        "rejected checkpoint recovery must not replace signed intent"
    );
    std::fs::remove_file(path).expect("remove fixture checkpoint");
}

#[test]
fn service_handoff_recovers_mirror_only_wake_from_exact_signed_checkpoint() {
    let cognition = service_cognition();
    let target = wake_for(&cognition, "wake-mirror-recovery");
    let path = lineage_path("mirror-recovery");
    let mut sidecar = configured_sidecar(&cognition, Vec::new(), path.clone());
    sidecar
        .pending_runtime_wakes
        .insert(target.wake_id.clone(), target.clone());

    let request = &cognition.request.request_context;
    let phase = format!("handoff:{}", target.wake_id);
    let operation = handoff_operation(&target.wake_id);
    let (checkpoint, existed) = sidecar
        .prepare_service_scheduler_checkpoint(request, &phase, operation.clone(), None)
        .expect("persist original signed handoff checkpoint alongside durable wake mirror");
    assert!(!existed);
    let checkpoint_bytes = serde_json::to_vec(&checkpoint).expect("checkpoint bytes");

    let (selected_wake, selected_operation) = sidecar
        .committed_service_wake_handoff(&cognition)
        .expect("recovered mirror is fenced by its original checkpoint")
        .expect("exact durable mirror is restored");
    assert_eq!(selected_wake, target);
    assert_eq!(selected_operation, operation);
    assert_eq!(
        serde_json::to_vec(
            &sidecar.provider_scheduler_pending[&format!(
                "{}:handoff:{}",
                request.provider_invocation_key(),
                target.wake_id
            )]
        )
        .expect("checkpoint bytes after recovery"),
        checkpoint_bytes,
        "recovery reuses the original signed request without replacement"
    );
    assert_eq!(
        sidecar
            .provider_service_projection
            .as_ref()
            .expect("verified projection")
            .scheduler_wakes,
        Vec::<crate::runtime::SchedulerWakeV1>::new(),
        "mirror recovery does not synthesize a projection wake"
    );
    std::fs::remove_file(path).expect("remove fixture checkpoint");
}

#[test]
fn service_handoff_restart_preserves_only_exact_checkpointed_wake_then_cleans_up() {
    let cognition = service_cognition();
    let target = wake_for(&cognition, "wake-postcommit-restart");
    let path = lineage_path("postcommit-restart");
    let mut before_restart = configured_sidecar(&cognition, Vec::new(), path.clone());
    let post_commit_tick = cognition.request.request_context.runtime_binding.base_tick + 1;
    let post_commit_hash =
        crate::simulator::h_v1("handoff-fixture-world-after-commit", &"verified");
    let binding = before_restart
        .provider_service_projection
        .as_mut()
        .expect("projection")
        .runtime_binding
        .as_mut()
        .expect("runtime binding");
    binding.base_tick = post_commit_tick;
    binding.base_world_hash = post_commit_hash.clone();
    let original = persist_recovery_handoff(
        &mut before_restart,
        &cognition,
        &target,
        handoff_operation(&target.wake_id),
    );
    let checkpoint_id = format!(
        "{}:handoff:{}",
        cognition.request.request_context.provider_invocation_key(),
        target.wake_id
    );
    let original_checkpoint = serde_json::to_vec(&original).expect("original checkpoint");

    let mut after_restart = configured_sidecar(&cognition, Vec::new(), path.clone());
    let binding = after_restart
        .provider_service_projection
        .as_mut()
        .expect("projection")
        .runtime_binding
        .as_mut()
        .expect("runtime binding");
    binding.base_tick = post_commit_tick;
    binding.base_world_hash = post_commit_hash.clone();
    after_restart
        .restore_provider_lineage(&crate::runtime_host::world::new())
        .expect("restore exact signed checkpoint and wake mirror");
    after_restart
        .sync_runtime_wakes(&crate::runtime_host::world::new())
        .expect("preserve terminal wake covered by exact checkpoint");
    assert_eq!(
        after_restart.pending_runtime_wakes.get(&target.wake_id),
        Some(&target)
    );
    let (_, reused) = after_restart
        .committed_service_wake_handoff(&cognition)
        .expect("recovered handoff remains usable")
        .expect("exact post-commit wake remains visible");
    assert_eq!(reused, handoff_operation(&target.wake_id));
    assert_eq!(
        serde_json::to_vec(&after_restart.provider_scheduler_pending[&checkpoint_id])
            .expect("reused checkpoint bytes"),
        original_checkpoint,
        "restart must reuse rather than replace the original signed request"
    );
    let crate::world_service::wire::WorldServicePayloadV1::Scheduler(signed) =
        &after_restart.provider_scheduler_pending[&checkpoint_id].payload
    else {
        panic!("handoff checkpoint payload");
    };
    assert_eq!(
        signed.request.captured_base_binding.base_tick,
        post_commit_tick
    );
    assert_eq!(
        signed.request.captured_base_binding.base_world_hash,
        post_commit_hash.to_string()
    );

    after_restart
        .complete_committed_service_wake_handoff(&cognition, &target.wake_id)
        .expect("persist wake mirror and checkpoint cleanup");
    after_restart
        .complete_provider_wake_recovery(&target.agent_id)
        .expect("persist recovery identity cleanup after accepted handoff");
    let persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("cleaned lineage checkpoint"))
            .expect("lineage JSON");
    assert!(
        persisted["pending_runtime_wakes"]
            .get(&target.wake_id)
            .is_none()
    );
    assert!(
        persisted["provider_scheduler_pending"]
            .get(&checkpoint_id)
            .is_none()
    );
    assert!(
        persisted["provider_wake_recovery_pending"]
            .get(&target.agent_id)
            .is_none()
    );
    std::fs::remove_file(path).expect("remove fixture checkpoint");
}

#[test]
fn service_handoff_restart_fences_missing_or_mismatched_checkpoint() {
    let cognition = service_cognition();
    let target = wake_for(&cognition, "wake-uncheckpointed-restart");
    let missing_path = lineage_path("restart-missing-checkpoint");
    let mut before_restart = configured_sidecar(&cognition, Vec::new(), missing_path.clone());
    before_restart
        .pending_runtime_wakes
        .insert(target.wake_id.clone(), target.clone());
    before_restart.retain_provider_wake_recovery_pending(
        &target.agent_id,
        &cognition.request,
        crate::runtime::ContinuationStatusV1::Completed,
        "provider_action_committed",
    );
    let mut after_restart = configured_sidecar(&cognition, Vec::new(), missing_path.clone());
    after_restart
        .restore_provider_lineage(&crate::runtime_host::world::new())
        .expect("restore mirror without handoff checkpoint");
    let error = after_restart
        .sync_runtime_wakes(&crate::runtime_host::world::new())
        .expect_err("missing exact checkpoint fails closed");
    assert!(
        error.contains("lacks its signed handoff checkpoint"),
        "{error}"
    );
    assert_eq!(
        after_restart.pending_runtime_wakes.get(&target.wake_id),
        Some(&target),
        "failed projection reconciliation must not replace the durable mirror"
    );
    assert!(after_restart.provider_scheduler_pending.is_empty());
    std::fs::remove_file(missing_path).expect("remove missing-checkpoint fixture");

    let mismatch_path = lineage_path("restart-mismatched-checkpoint");
    let mut before_restart = configured_sidecar(&cognition, Vec::new(), mismatch_path.clone());
    let wrong_operation = handoff_operation("different-wake");
    persist_recovery_handoff(&mut before_restart, &cognition, &target, wrong_operation);
    let mut after_restart = configured_sidecar(&cognition, Vec::new(), mismatch_path.clone());
    after_restart
        .restore_provider_lineage(&crate::runtime_host::world::new())
        .expect("restore signed but wrong-target checkpoint");
    let error = after_restart
        .sync_runtime_wakes(&crate::runtime_host::world::new())
        .expect_err("wrong signed wake target fails closed");
    assert!(error.contains("checkpoint identity mismatch"), "{error}");
    assert_eq!(
        after_restart.pending_runtime_wakes.get(&target.wake_id),
        Some(&target),
        "wrong signed intent cannot replace the saved wake mirror"
    );
    assert_eq!(after_restart.provider_scheduler_pending.len(), 1);
    std::fs::remove_file(mismatch_path).expect("remove mismatched-checkpoint fixture");
}

#[test]
fn service_handoff_fences_mirror_only_wake_without_signed_checkpoint() {
    let cognition = service_cognition();
    let target = wake_for(&cognition, "wake-mirror-without-checkpoint");
    let path = lineage_path("mirror-without-checkpoint");
    let mut sidecar = configured_sidecar(&cognition, Vec::new(), path.clone());
    sidecar
        .pending_runtime_wakes
        .insert(target.wake_id.clone(), target);

    let error = sidecar
        .committed_service_wake_handoff(&cognition)
        .expect_err("a mirror without the original signed checkpoint fails closed");
    assert!(error.contains("absent from verified projection"), "{error}");
    assert!(sidecar.provider_scheduler_pending.is_empty());
    assert!(
        !path.exists(),
        "the fenced recovery does not sign or persist work"
    );
}

#[test]
fn service_handoff_cleanup_removes_wake_and_checkpoint_atomically() {
    let cognition = service_cognition();
    let target = wake_for(&cognition, "wake-cleanup-atomic");
    let path = lineage_path("atomic-cleanup");
    let mut sidecar = configured_sidecar(&cognition, vec![target.clone()], path.clone());
    sidecar
        .pending_runtime_wakes
        .insert(target.wake_id.clone(), target.clone());
    let request = &cognition.request.request_context;
    let checkpoint_id = format!(
        "{}:handoff:{}",
        request.provider_invocation_key(),
        target.wake_id
    );
    sidecar
        .prepare_service_scheduler_checkpoint(
            request,
            &format!("handoff:{}", target.wake_id),
            handoff_operation(&target.wake_id),
            None,
        )
        .expect("persist exact handoff checkpoint");

    let blocker = path.with_extension("blocker");
    std::fs::write(&blocker, b"not a directory").expect("persistence blocker");
    sidecar.provider_lineage_store = Some(blocker.join("checkpoint.json"));
    assert!(
        sidecar
            .complete_committed_service_wake_handoff(&cognition, &target.wake_id)
            .is_err()
    );
    assert_eq!(
        sidecar.pending_runtime_wakes.get(&target.wake_id),
        Some(&target),
        "failed persistence restores the wake mirror"
    );
    assert!(
        sidecar
            .provider_scheduler_pending
            .contains_key(&checkpoint_id)
    );

    sidecar.provider_lineage_store = Some(path.clone());
    sidecar
        .complete_committed_service_wake_handoff(&cognition, &target.wake_id)
        .expect("persist both durable removals together");
    assert!(!sidecar.pending_runtime_wakes.contains_key(&target.wake_id));
    assert!(
        !sidecar
            .provider_scheduler_pending
            .contains_key(&checkpoint_id)
    );
    let persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("durable lineage"))
            .expect("lineage JSON");
    assert!(
        persisted["pending_runtime_wakes"]
            .get(&target.wake_id)
            .is_none()
    );
    assert!(
        persisted["provider_scheduler_pending"]
            .get(&checkpoint_id)
            .is_none()
    );
    std::fs::remove_file(path).expect("remove fixture checkpoint");
    std::fs::remove_file(blocker).expect("remove fixture blocker");
}

#[test]
fn service_handoff_does_not_consume_an_unrelated_same_agent_wake() {
    let cognition = service_cognition();
    let mut target = wake_for(&cognition, "wake-other-request");
    target.agent_turn_id = "turn-other".into();
    target.request_digest = crate::simulator::h_v1("other-request", &"different").to_string();
    let path = lineage_path("unrelated-wake");
    let mut sidecar = configured_sidecar(&cognition, vec![target.clone()], path.clone());
    assert!(
        sidecar
            .committed_service_wake_handoff(&cognition)
            .expect("unrelated wake is ignored")
            .is_none()
    );
    assert!(sidecar.provider_scheduler_pending.is_empty());
    assert_eq!(
        sidecar
            .provider_service_projection
            .as_ref()
            .expect("projection")
            .scheduler_wakes,
        vec![target]
    );
    assert!(!path.exists(), "no checkpoint is written for another turn");
}

#[test]
fn service_handoff_does_not_consume_request_wake_from_other_runtime_binding() {
    let cognition = service_cognition();
    let mut target = wake_for(&cognition, "wake-other-world-binding");
    target.world_id = "another-world".into();
    let path = lineage_path("other-world-binding");
    let mut sidecar = configured_sidecar(&cognition, vec![target.clone()], path.clone());

    assert!(
        sidecar
            .committed_service_wake_handoff(&cognition)
            .expect("wake with another binding is ignored")
            .is_none()
    );
    assert!(sidecar.provider_scheduler_pending.is_empty());
    assert_eq!(
        sidecar
            .provider_service_projection
            .as_ref()
            .expect("projection")
            .scheduler_wakes,
        vec![target]
    );
    assert!(!path.exists(), "no checkpoint is written for another world");
}

#[test]
fn service_handoff_fences_multiple_exact_wakes_without_selecting_one() {
    let cognition = service_cognition();
    let target = wake_for(&cognition, "wake-duplicate-exact");
    let mut duplicate = target.clone();
    duplicate.wake_id = "wake-duplicate-exact-2".into();
    duplicate.continuation_id = "continuation-wake-duplicate-exact-2".into();
    let path = lineage_path("ambiguous-exact-wakes");
    let mut sidecar = configured_sidecar(&cognition, vec![target, duplicate], path.clone());
    let error = sidecar
        .committed_service_wake_handoff(&cognition)
        .expect_err("ambiguous exact target wakes are fenced");
    assert!(error.contains("ambiguous"), "{error}");
    assert!(sidecar.provider_scheduler_pending.is_empty());
    assert!(
        !path.exists(),
        "ambiguous wake selection has no side effect"
    );
}
