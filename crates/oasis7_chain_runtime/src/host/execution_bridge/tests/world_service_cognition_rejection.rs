//! Registered canonical execution retains only authenticated stale rejection metadata.
use super::*;
use oasis7_runtime::{
    RuntimeCognitionCommitRequestV1, RuntimeCognitionResponseArtifactV1, RuntimeFeedbackRequestV1,
};

fn fixture() -> (
    RuntimeWorld,
    WorldIdentity,
    SignedReadRequest<CognitionIntentV1>,
) {
    let key = hex::encode([7; 32]);
    let public = sign_read_request("key", (), &key)
        .unwrap()
        .subject_public_key;
    let world = live_fixture_world("w1", &public);
    let mut base = world.current_cognition_base_binding().unwrap();
    base.base_tick = base.base_tick.saturating_sub(1);
    let digest = |b: &str| format!("blake3:{}", b.repeat(64));
    let request = RuntimeCognitionCommitRequestV1 {
        agent_id: "agent-a".into(),
        agent_session_id: "reject-session".into(),
        agent_turn_id: "reject-turn".into(),
        decision_request_id: "reject-decision".into(),
        retry_seq: 1,
        transport_attempt: 1,
        request_digest: digest("1"),
        observation_digest: digest("2"),
        context_digest: digest("3"),
        capability_snapshot_hash: digest("4"),
        authority_context_hash: digest("5"),
        captured_base_binding: base,
    };
    let mut artifact = RuntimeCognitionResponseArtifactV1 {
        schema_version: 1,
        context_discriminator: RuntimeCognitionResponseArtifactV1::CONTEXT_DISCRIMINATOR.into(),
        context_version: 1,
        agent_session_id: request.agent_session_id.clone(),
        agent_turn_id: request.agent_turn_id.clone(),
        decision_request_id: request.decision_request_id.clone(),
        retry_seq: 1,
        transport_attempt: 1,
        request_digest: request.request_digest.clone(),
        response_digest: digest("6"),
        artifact_digest: String::new(),
    };
    artifact.refresh_artifact_digest();
    let signed = sign_read_request(
        "cognition",
        CognitionIntentV1 {
            request,
            action: Action::MoveAgent {
                agent_id: "agent-a".into(),
                to: oasis7_agent_api::world_action::GeoPos::new(10, 0, 0),
            },
            response_artifact: artifact,
            delegation_generation: 0,
            causal_proposal: None,
            task_fence: None,
        },
        &key,
    )
    .unwrap();
    (
        world,
        WorldIdentity {
            world_id: "w1".into(),
            genesis_digest: "fixture-genesis-v1".into(),
        },
        signed,
    )
}
fn context() -> NodeExecutionCommitContext {
    NodeExecutionCommitContext {
        world_id: "w1".into(),
        node_id: "node-a".into(),
        proposer_id: "node-a".into(),
        height: 9,
        slot: 9,
        epoch: 0,
        node_block_hash: "node-h9".into(),
        action_root: "root9".into(),
        committed_actions: vec![],
        committed_at_unix_ms: 9000,
    }
}
fn apply(
    world: &mut RuntimeWorld,
    identity: &WorldIdentity,
    signed: SignedReadRequest<CognitionIntentV1>,
) -> Result<(), String> {
    let staged = super::super::super::world_service_execution::apply_intents(
        world,
        &context(),
        Some(identity),
        vec![(
            9,
            request(identity, WorldServicePayloadV1::Cognition(signed)),
        )],
    )?;
    super::super::super::world_service_execution::finalize_intents(world, staged)
}
#[test]
fn authenticated_stale_rejection_publishes_exact_outbox_without_business_effect_and_replays() {
    let (mut world, identity, signed) = fixture();
    let state = serde_json::to_value(world.state()).unwrap();
    let pending = serde_json::to_value(world.snapshot().pending_actions).unwrap();
    let receipts = world.cognition()["receipt_registry"].clone();
    let request_identity = signed.request.request.clone();
    let mut core_reference = world.clone();
    assert!(
        core_reference
            .commit_cognition_action(
                signed.request.request.clone(),
                signed.request.action.clone(),
                signed.request.response_artifact.clone()
            )
            .is_err()
    );
    let terminal_events = |world: &RuntimeWorld| {
        world.cognition()["cognition_journal"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| {
                matches!(
                    event["kind"].as_str(),
                    Some("DecisionRejected" | "CognitionTurnCompleted")
                )
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    let expected_terminal = terminal_events(&core_reference);
    assert_eq!(expected_terminal.len(), 2);
    let original = request(&identity, WorldServicePayloadV1::Cognition(signed.clone()));
    apply(&mut world, &identity, signed.clone()).unwrap();
    assert_eq!(serde_json::to_value(world.state()).unwrap(), state);
    assert_eq!(
        serde_json::to_value(world.snapshot().pending_actions).unwrap(),
        pending
    );
    assert_eq!(world.cognition()["receipt_registry"], receipts);
    assert_eq!(
        terminal_events(&world),
        expected_terminal,
        "publish the exact existing Runtime stale terminal metadata"
    );
    assert_eq!(
        world.cognition()["idempotency_index"],
        core_reference.cognition()["idempotency_index"]
    );
    assert!(world.cognition().get("service_causal_proposals").is_none());
    let records = world.runtime_feedback_outbox().unwrap();
    assert_eq!(records.len(), 1);
    let record = &records[0];
    record.validate().unwrap();
    assert_eq!(record.agent_subject, request_identity.agent_id);
    assert_eq!(record.agent_session_id, request_identity.agent_session_id);
    assert_eq!(record.agent_turn_id, request_identity.agent_turn_id);
    assert_eq!(
        record.decision_request_id,
        request_identity.decision_request_id
    );
    assert_eq!(record.request_digest, request_identity.request_digest);
    assert_eq!(record.payload["status"], "rejected");
    assert_eq!(record.payload["reject_reason"], "stale_base");
    assert!(
        record
            .payload
            .get("candidate_action_id")
            .is_none_or(serde_json::Value::is_null)
    );
    assert!(
        record
            .payload
            .get("runtime_receipt_id")
            .is_none_or(serde_json::Value::is_null)
    );
    let key = correlation::key_digest(&original.correlation.key).unwrap();
    let canonical: CanonicalIntentResultV1 = serde_json::from_value(
        world.capability_revocation_state().world_service_results[&key].clone(),
    )
    .unwrap();
    assert!(canonical.rejected.is_some());
    assert!(canonical.receipt.is_null());
    assert_eq!(canonical.request, original);
    let history =
        oasis7::world_service::projection::feedback_history_from_world(&world, "agent-a").unwrap();
    assert_eq!(history.records.len(), 1);
    assert!(history.records[0].receipt_lineage.is_none());
    let before = serde_json::to_value(world.snapshot()).unwrap();
    apply(&mut world, &identity, signed).unwrap();
    assert_eq!(
        serde_json::to_value(world.snapshot()).unwrap(),
        before,
        "replay cannot allocate a second feedback sequence or mutate the original"
    );
}
#[test]
fn prevalidation_failures_and_unrelated_actor_never_publish_stale_feedback() {
    for kind in [
        "signature",
        "artifact",
        "signer",
        "actor",
        "owner_generation",
        "task_fence",
    ] {
        let (mut world, identity, mut signed) = fixture();
        match kind {
            "signature" => signed.signature_hex = "00".repeat(64),
            "artifact" => {
                signed.request.response_artifact.request_digest =
                    format!("blake3:{}", "a".repeat(64));
                signed =
                    sign_read_request("cognition", signed.request, &hex::encode([7; 32])).unwrap();
            }
            "signer" => {
                signed =
                    sign_read_request("cognition", signed.request, &hex::encode([8; 32])).unwrap()
            }
            "actor" => {
                signed.request.action = Action::MoveAgent {
                    agent_id: "another-agent".into(),
                    to: oasis7_agent_api::world_action::GeoPos::new(10, 0, 0),
                };
                signed =
                    sign_read_request("cognition", signed.request, &hex::encode([7; 32])).unwrap();
            }
            "task_fence" => {
                let public = signed.subject_public_key.clone();
                signed.request.task_fence = Some(serde_json::from_value(serde_json::json!({
                    "world_id":"w1","genesis_digest":"fixture-genesis-v1","branch_id":"main","reorg_epoch":"0",
                    "agent_id":"agent-a","owner_binding":"owner-a","owner_generation":"1","owner_public_key":public,
                    "task_id":"nonexistent-task","task_revision":"1","task_status":"active","latest_goal_revision":"1","active_goal_revision":"1",
                    "active_goal_reference":{"goal_revision":"1","goal_ref":"c".repeat(64),"prepare_operation_id":"prepare","publish_operation_id":"publish","activation_committed_height":"1"},
                    "executor_epoch":"1","executor_binding":{"executor_id":"executor","executor_public_key":signed.subject_public_key,"delegation_generation":"1","executor_epoch":"1"}
                })).unwrap());
                signed =
                    sign_read_request("cognition", signed.request, &hex::encode([7; 32])).unwrap();
            }
            "owner_generation" => {
                signed.request.delegation_generation = 1;
                signed =
                    sign_read_request("cognition", signed.request, &hex::encode([7; 32])).unwrap();
            }
            _ => unreachable!(),
        }
        let state = serde_json::to_value(world.state()).unwrap();
        if matches!(kind, "signature" | "artifact") {
            // These are rejected even before canonical correlation can be
            // derived; exercise the authenticated service boundary directly.
            assert!(
                world
                    .commit_authenticated_cognition_disposition_staged(signed, &[])
                    .is_err()
            );
        } else {
            apply(&mut world, &identity, signed).unwrap();
        }
        assert!(
            world.runtime_feedback_outbox().unwrap().is_empty(),
            "{kind}"
        );
        assert_eq!(
            serde_json::to_value(world.state()).unwrap(),
            state,
            "{kind}"
        );
        assert!(
            world
                .capability_revocation_state()
                .world_service_results
                .values()
                .all(|r| r["receipt"].is_null())
        );
    }
}
#[test]
fn rejected_outbox_collision_aborts_execution_without_adopting_failed_candidate() {
    let (mut first, identity, signed) = fixture();
    apply(&mut first, &identity, signed.clone()).unwrap();
    let feedback_id = first.runtime_feedback_outbox().unwrap()[0]
        .feedback_id
        .clone();
    let (mut world, _, _) = fixture();
    let r = &signed.request.request;
    world
        .allocate_runtime_feedback_with_projection(
            RuntimeFeedbackRequestV1 {
                feedback_id: Some(feedback_id),
                agent_subject: r.agent_id.clone(),
                agent_session_id: r.agent_session_id.clone(),
                agent_turn_id: r.agent_turn_id.clone(),
                decision_request_id: r.decision_request_id.clone(),
                candidate_action_id: None,
                runtime_receipt_id: None,
                status: "rejected".into(),
                request_digest: format!("blake3:{}", "f".repeat(64)),
                reject_reason: Some("stale_base".into()),
            },
            Default::default(),
        )
        .unwrap();
    let before = serde_json::to_value(world.snapshot()).unwrap();
    assert!(apply(&mut world, &identity, signed).is_err());
    assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
    assert!(
        world
            .capability_revocation_state()
            .world_service_results
            .is_empty()
    );
}
