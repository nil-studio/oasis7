//! RED/GREEN coverage for the World-owned provider feedback outbox and
//! capability context execution transaction.

use super::super::*;
use super::capability_grant_v2::*;
use oasis7_wasm_abi::{AgentCommandResponse, CapabilityPresenter};
use serde_json::json;

fn feedback(seq: u64, feedback_id: &str) -> oasis7_agent_api::FeedbackEnvelopeV1 {
    let request_id = format!("request-{seq}");
    oasis7_agent_api::FeedbackEnvelopeV1 {
        feedback_id: feedback_id.to_string(),
        feedback_seq: seq,
        agent_subject: SUBJECT_ID.to_string(),
        agent_session_id: "provider-session-1".to_string(),
        agent_turn_id: format!("turn-{seq}"),
        decision_request_id: request_id.clone(),
        candidate_action_id: Some(seq),
        runtime_receipt_id: Some(format!("receipt-{seq}")),
        status: "committed".to_string(),
        request_digest: oasis7_agent_api::h_v1("oasis7.cognition.request.v1", &request_id),
        reject_reason: None,
        provenance: "runtime_authoritative".to_string(),
    }
}

#[test]
fn runtime_feedback_outbox_is_durable_ordered_retryable_and_recoverable() {
    let mut world = World::new();
    let first = feedback(1, "feedback-1");
    let second = feedback(2, "feedback-2");

    let queued = world
        .enqueue_runtime_feedback(first.clone())
        .expect("Runtime should durably queue feedback");
    assert_eq!(queued.state, "pending");
    assert_eq!(queued.attempt, 0);
    world
        .enqueue_runtime_feedback(first)
        .expect("same feedback retry is idempotent");
    world
        .enqueue_runtime_feedback(second)
        .expect("second feedback should queue");

    let claimed = world
        .claim_runtime_feedback("feedback-1")
        .expect("Runtime should claim one outbox item");
    assert_eq!(claimed.state, "in_flight");
    assert_eq!(claimed.attempt, 1);
    world
        .retry_runtime_feedback("feedback-1", "provider unavailable")
        .expect("failed delivery should return to pending");
    assert_eq!(
        world
            .pending_runtime_feedback()
            .expect("read pending outbox")
            .len(),
        2
    );

    let snapshot = world.snapshot();
    let mut restored = World::from_snapshot(snapshot, world.journal().clone())
        .expect("restore feedback outbox snapshot");
    restored
        .recover_cognition()
        .expect("recover feedback outbox");
    assert_eq!(
        restored
            .pending_runtime_feedback()
            .expect("read restored pending outbox")
            .iter()
            .map(|record| record.feedback_id.as_str())
            .collect::<Vec<_>>(),
        vec!["feedback-1", "feedback-2"]
    );
    let claimed = restored
        .claim_runtime_feedback("feedback-1")
        .expect("claim after restore");
    assert_eq!(claimed.attempt, 2);
    restored
        .ack_runtime_feedback("feedback-1")
        .expect("acknowledge delivered feedback");
    assert_eq!(
        restored
            .pending_runtime_feedback()
            .expect("read pending after ack")
            .iter()
            .map(|record| record.feedback_id.as_str())
            .collect::<Vec<_>>(),
        vec!["feedback-2"]
    );
}

#[test]
fn runtime_feedback_allocator_owns_sequence_and_exact_replay_identity() {
    let mut world = World::new();
    let request_digest =
        oasis7_agent_api::h_v1("oasis7.cognition.request.v1", &"runtime-feedback-request")
            .to_string();
    let request = RuntimeFeedbackRequestV1 {
        feedback_id: None,
        agent_subject: SUBJECT_ID.to_string(),
        agent_session_id: "runtime-feedback-session".to_string(),
        agent_turn_id: "runtime-feedback-turn".to_string(),
        decision_request_id: "runtime-feedback-decision".to_string(),
        candidate_action_id: None,
        runtime_receipt_id: None,
        status: "pending".to_string(),
        request_digest: request_digest.clone(),
        reject_reason: None,
    };
    let first = world
        .allocate_runtime_feedback(request.clone())
        .expect("Runtime allocates a complete feedback envelope");
    assert_eq!(first.feedback_seq, 1);
    assert_eq!(first.state, "pending");

    let replay = world
        .allocate_runtime_feedback(request)
        .expect("exact disposition replay is idempotent");
    assert_eq!(replay.feedback_id, first.feedback_id);
    assert_eq!(replay.feedback_seq, first.feedback_seq);
    assert_eq!(world.runtime_feedback_outbox().expect("outbox").len(), 1);

    let second = world
        .allocate_runtime_feedback(RuntimeFeedbackRequestV1 {
            feedback_id: None,
            agent_subject: SUBJECT_ID.to_string(),
            agent_session_id: "runtime-feedback-session".to_string(),
            agent_turn_id: "runtime-feedback-turn-2".to_string(),
            decision_request_id: "runtime-feedback-decision-2".to_string(),
            candidate_action_id: None,
            runtime_receipt_id: None,
            status: "rejected".to_string(),
            request_digest,
            reject_reason: Some("stale_base".to_string()),
        })
        .expect("next disposition receives the next Runtime sequence");
    assert_eq!(second.feedback_seq, 2);
    assert_ne!(second.feedback_id, first.feedback_id);
}

#[test]
fn runtime_feedback_disposition_mapping_rejects_contradictions_and_bounds_projection() {
    let mut world = World::new();
    let request_digest =
        oasis7_agent_api::h_v1("oasis7.cognition.request.v1", &"feedback-disposition");
    let contradictory = RuntimeFeedbackRequestV1 {
        feedback_id: None,
        agent_subject: SUBJECT_ID.to_string(),
        agent_session_id: "feedback-disposition-session".to_string(),
        agent_turn_id: "feedback-disposition-turn".to_string(),
        decision_request_id: "feedback-disposition-request".to_string(),
        candidate_action_id: Some(1),
        runtime_receipt_id: Some("receipt-contradictory".to_string()),
        status: "rejected".to_string(),
        request_digest: request_digest.to_string(),
        reject_reason: None,
    };
    assert!(
        world.allocate_runtime_feedback(contradictory).is_err(),
        "rejected feedback cannot carry a committed action/receipt"
    );

    let unknown_reason = RuntimeFeedbackRequestV1 {
        candidate_action_id: None,
        runtime_receipt_id: None,
        status: "rejected".to_string(),
        reject_reason: Some("provider_free_text".to_string()),
        agent_subject: SUBJECT_ID.to_string(),
        agent_session_id: "feedback-disposition-session".to_string(),
        agent_turn_id: "feedback-disposition-turn-2".to_string(),
        decision_request_id: "feedback-disposition-request-2".to_string(),
        request_digest: request_digest.to_string(),
        feedback_id: None,
    };
    assert!(
        world.allocate_runtime_feedback(unknown_reason).is_err(),
        "rejected feedback requires a stable Runtime reason"
    );

    let projected = world
        .allocate_runtime_feedback_with_projection(
            RuntimeFeedbackRequestV1 {
                feedback_id: None,
                agent_subject: SUBJECT_ID.to_string(),
                agent_session_id: "feedback-disposition-session".to_string(),
                agent_turn_id: "feedback-disposition-turn-projected".to_string(),
                decision_request_id: "feedback-disposition-request-projected".to_string(),
                candidate_action_id: None,
                runtime_receipt_id: None,
                status: "pending".to_string(),
                request_digest: request_digest.to_string(),
                reject_reason: None,
            },
            RuntimeFeedbackProjectionV1 {
                envelope_digest: Some(
                    oasis7_agent_api::h_v1("oasis7.cognition.envelope.v1", &"projected-envelope")
                        .to_string(),
                ),
                emitted_events: vec![json!({"kind": "action_committed"})],
                committed_event_summary: Some("one bounded event".to_string()),
                world_delta_summary: Some("state root advanced".to_string()),
            },
        )
        .expect("valid Runtime projection should be durable");
    let projected_payload = projected
        .transport_payload()
        .expect("transport payload is revalidated");
    assert_eq!(
        projected_payload["emitted_events"][0]["kind"],
        json!("action_committed")
    );
    assert_eq!(
        projected_payload["committed_event_summary"],
        json!("one bounded event")
    );

    let oversize = RuntimeFeedbackProjectionV1 {
        envelope_digest: Some(
            oasis7_agent_api::h_v1("oasis7.cognition.envelope.v1", &"feedback-envelope")
                .to_string(),
        ),
        emitted_events: Vec::new(),
        committed_event_summary: Some("x".repeat(513)),
        world_delta_summary: None,
    };
    let accepted = world
        .allocate_runtime_feedback_with_projection(
            RuntimeFeedbackRequestV1 {
                feedback_id: None,
                agent_subject: SUBJECT_ID.to_string(),
                agent_session_id: "feedback-disposition-session".to_string(),
                agent_turn_id: "feedback-disposition-turn-3".to_string(),
                decision_request_id: "feedback-disposition-request-3".to_string(),
                candidate_action_id: None,
                runtime_receipt_id: None,
                status: "pending".to_string(),
                request_digest: request_digest.to_string(),
                reject_reason: None,
            },
            oversize,
        )
        .expect_err("oversize projection must not cross Runtime feedback boundary");
    assert!(format!("{accepted:?}").contains("feedback_projection_too_large"));

    let too_many_events = RuntimeFeedbackProjectionV1 {
        envelope_digest: None,
        emitted_events: vec![json!("event"); 33],
        committed_event_summary: None,
        world_delta_summary: None,
    };
    assert!(
        world
            .allocate_runtime_feedback_with_projection(
                RuntimeFeedbackRequestV1 {
                    feedback_id: None,
                    agent_subject: SUBJECT_ID.to_string(),
                    agent_session_id: "feedback-disposition-session".to_string(),
                    agent_turn_id: "feedback-disposition-turn-too-many".to_string(),
                    decision_request_id: "feedback-disposition-request-too-many".to_string(),
                    candidate_action_id: Some(4),
                    runtime_receipt_id: Some("receipt-too-many".to_string()),
                    status: "committed".to_string(),
                    request_digest: request_digest.to_string(),
                    reject_reason: None,
                },
                too_many_events,
            )
            .is_err(),
        "feedback projection event count is bounded"
    );

    let sensitive_event = RuntimeFeedbackProjectionV1 {
        envelope_digest: None,
        emitted_events: vec![json!({"token": "must-not-cross"})],
        committed_event_summary: None,
        world_delta_summary: None,
    };
    assert!(
        world
            .allocate_runtime_feedback_with_projection(
                RuntimeFeedbackRequestV1 {
                    feedback_id: None,
                    agent_subject: SUBJECT_ID.to_string(),
                    agent_session_id: "feedback-disposition-session".to_string(),
                    agent_turn_id: "feedback-disposition-turn-sensitive".to_string(),
                    decision_request_id: "feedback-disposition-request-sensitive".to_string(),
                    candidate_action_id: Some(5),
                    runtime_receipt_id: Some("receipt-sensitive".to_string()),
                    status: "committed".to_string(),
                    request_digest: request_digest.to_string(),
                    reject_reason: None,
                },
                sensitive_event,
            )
            .is_err(),
        "sensitive feedback projection is rejected before transport"
    );
}

#[test]
fn runtime_feedback_outbox_rejects_identity_collision_and_tampered_restore() {
    let mut world = World::new();
    world
        .enqueue_runtime_feedback(feedback(1, "feedback-collision"))
        .expect("queue feedback");
    let mut conflicting = feedback(1, "feedback-collision");
    conflicting.agent_turn_id = "different-turn".to_string();
    assert!(
        world.enqueue_runtime_feedback(conflicting).is_err(),
        "feedback identity collision must fail closed"
    );
    assert!(
        world
            .enqueue_runtime_feedback(feedback(1, "other-feedback"))
            .is_err(),
        "feedback sequence collision must fail closed"
    );

    let mut snapshot = world.snapshot();
    snapshot.cognition["feedback_outbox"]["feedback-collision"]["envelope_digest"] =
        json!("blake3:tampered");
    let mut restored = World::from_snapshot(snapshot, world.journal().clone())
        .expect("snapshot structure should decode");
    assert!(
        restored.recover_cognition().is_err(),
        "tampered feedback projection must fail closed"
    );

    let mut invalid_status = feedback(2, "invalid-status");
    invalid_status.status = "provider_owned".to_string();
    assert!(
        world.enqueue_runtime_feedback(invalid_status).is_err(),
        "feedback status outside the typed Runtime contract must fail closed"
    );

    let mut missing_receipt = feedback(2, "missing-receipt");
    missing_receipt.runtime_receipt_id = None;
    assert!(
        world.enqueue_runtime_feedback(missing_receipt).is_err(),
        "committed feedback without a Runtime receipt must fail closed"
    );
}

#[test]
fn capability_context_execution_installs_and_executes_atomically() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({
        "grant_nonce": "atomic-provider-command"
    })));
    world
        .register_capability_grant_v2(grant.clone())
        .expect("register command grant");
    install_budget_for_grant(&mut world, &grant, 128);
    let presenter = CapabilityPresenter {
        presenter_id: PRESENTER_ID.to_string(),
        presenter_kind: "provider".to_string(),
        session_id: Some("provider-session-atomic".to_string()),
        attestation_ref: None,
    };
    let (catalog, invocation) = world
        .capability_context_for_agent(SUBJECT_ID, presenter, "atomic-response")
        .expect("derive live provider context");
    let response = prepared_response(
        response_json(json!({
            "response_nonce": "atomic-response",
            "provider_id": PRESENTER_ID,
            "presenter": {"session_id": "provider-session-atomic"}
        })),
        &catalog,
    );
    let mut sandbox = RecordingSandbox::default();
    let before_head = world.journal().len();
    let receipt = world
        .execute_trusted_module_command_with_context(
            grant,
            catalog,
            response,
            invocation.clone(),
            &mut (),
            &mut sandbox,
        )
        .expect("atomic context execution");
    assert_eq!(sandbox.calls, 1);
    assert_eq!(receipt.decision, "accepted");
    assert!(
        world
            .capability_invocation_contexts()
            .values()
            .any(|stored| stored == &invocation)
    );
    assert!(world.journal().len() > before_head);
}

#[test]
fn capability_context_execution_failure_does_not_install_context() {
    let mut world = fixture_world();
    let grant = signed_grant(grant_json(json!({
        "grant_nonce": "atomic-provider-failure"
    })));
    world
        .register_capability_grant_v2(grant.clone())
        .expect("register command grant");
    install_budget_for_grant(&mut world, &grant, 128);
    let presenter = CapabilityPresenter {
        presenter_id: PRESENTER_ID.to_string(),
        presenter_kind: "provider".to_string(),
        session_id: Some("provider-session-failure".to_string()),
        attestation_ref: None,
    };
    let (catalog, invocation) = world
        .capability_context_for_agent(SUBJECT_ID, presenter, "failure-response")
        .expect("derive live provider context");
    let response: AgentCommandResponse = prepared_response(
        response_json(json!({
            "response_nonce": "failure-response",
            "provider_id": PRESENTER_ID,
            "presenter": {"session_id": "provider-session-failure"}
        })),
        &catalog,
    );
    let before_cognition = world.cognition().clone();
    let before_head = world.journal().len();
    let before_state_root = world.current_state_root_hash().expect("state root");
    let mut sandbox = FailingSandbox;
    assert!(
        world
            .execute_trusted_module_command_with_context(
                grant,
                catalog,
                response,
                invocation.clone(),
                &mut (),
                &mut sandbox,
            )
            .is_err()
    );
    assert_eq!(*world.cognition(), before_cognition);
    assert_eq!(world.journal().len(), before_head);
    assert_eq!(
        world.current_state_root_hash().expect("state root"),
        before_state_root
    );
    assert!(
        !world
            .capability_invocation_contexts()
            .values()
            .any(|stored| stored == &invocation)
    );
}

struct FailingSandbox;
impl oasis7_wasm_abi::ModuleSandbox for FailingSandbox {
    fn call(
        &mut self,
        _request: &oasis7_wasm_abi::ModuleCallRequest,
    ) -> Result<oasis7_wasm_abi::ModuleOutput, oasis7_wasm_abi::ModuleCallFailure> {
        Err(oasis7_wasm_abi::ModuleCallFailure {
            code: oasis7_wasm_abi::ModuleCallErrorCode::Trap,
            detail: "fixture failure".to_string(),
            module_id: "module.weather".to_string(),
            trace_id: "fixture-trace".to_string(),
        })
    }
}
