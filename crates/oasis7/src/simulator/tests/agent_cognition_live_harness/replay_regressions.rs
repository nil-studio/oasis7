use super::*;

#[test]
fn compatibility_receipt_lineage_keeps_committed_lifecycle_without_memory_authority() {
    let mut store = MemoryWriteStore::default();
    for status in ["pending", "rejected", "failed"] {
        assert_non_committed_status_does_not_write(status);
    }

    let mut runner = provider_backed_runner();
    runner
        .start_turn_with_context(AGENT_ID, host_context())
        .unwrap();
    let rejected_outcome = completed_turn(&mut runner);
    let mut rejected = rejected_outcome
        .feedback_for_runtime_status("rejected", None)
        .unwrap();
    rejected.provenance = "runtime_authoritative".into();
    rejected.reject_reason = Some("stale_base".into());
    let mut wrong_terminal = rejected.clone();
    wrong_terminal.decision_request_id = "wrong-terminal-request".into();
    assert!(
        runner
            .consume_runtime_feedback_with_lineage(AGENT_ID, wrong_terminal, None, &mut store)
            .is_err()
    );
    runner
        .consume_runtime_feedback_with_lineage(AGENT_ID, rejected.clone(), None, &mut store)
        .unwrap();
    runner
        .consume_runtime_feedback_with_lineage(AGENT_ID, rejected.clone(), None, &mut store)
        .unwrap();
    let mut rejected_collision = rejected;
    rejected_collision.reject_reason = Some("different terminal reason".into());
    let collision = runner
        .consume_runtime_feedback_with_lineage(AGENT_ID, rejected_collision, None, &mut store)
        .expect_err("released feedback ID cannot be replayed with a changed envelope");
    assert!(collision.to_string().contains("feedback_id_conflict"));
    assert!(
        store.entries().is_empty(),
        "terminal feedback cannot write memory"
    );
    let mut replacement_context = host_context();
    replacement_context.agent_turn_id = "replacement-turn".into();
    replacement_context.decision_request_id = "replacement-request".into();
    let turn_id = runner
        .start_turn_with_context(AGENT_ID, replacement_context)
        .expect("open provider-backed target turn");
    let outcome = completed_turn(&mut runner);
    assert_eq!(outcome.turn_id, turn_id);
    assert_eq!(outcome.memory_write_intents.len(), 1);

    let mut committed = outcome
        .feedback_for_runtime_status("committed", Some("receipt-live-harness-1"))
        .expect("build correlated committed feedback");
    committed.candidate_action_id = Some(7);
    committed.feedback_seq = 2;
    committed.provenance = "runtime_authoritative".to_string();
    let receipt = runtime_receipt_for_feedback(&committed);
    runner
        .consume_runtime_feedback_with_lineage(
            AGENT_ID,
            committed.clone(),
            Some(&receipt),
            &mut store,
        )
        .expect("matching Runtime lineage preserves compatibility lifecycle");
    runner
        .consume_runtime_feedback_with_lineage(
            AGENT_ID,
            committed.clone(),
            Some(&receipt),
            &mut store,
        )
        .expect("same receipt replay is exactly-once");
    let mut mismatched_receipt = receipt.clone();
    mismatched_receipt.receipt_digest =
        "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd".into();
    assert!(
        runner
            .consume_runtime_feedback_with_lineage(
                AGENT_ID,
                committed,
                Some(&mismatched_receipt),
                &mut store,
            )
            .expect_err("same feedback envelope with a different receipt lineage is rejected")
            .to_string()
            .contains("replay receipt lineage mismatch")
    );
    assert!(
        store.entries().is_empty(),
        "a compatibility receipt DTO cannot authorize memory projection"
    );
}

#[test]
fn compatibility_lineage_feedback_keeps_lifecycle_without_projecting_memory() {
    let mut runner = provider_backed_runner();
    runner
        .start_turn_with_context(AGENT_ID, host_context())
        .expect("open reduced compatibility turn");
    let outcome = completed_turn(&mut runner);
    let mut feedback = outcome
        .feedback_for_runtime_status("committed", Some("receipt-compat-lineage-only"))
        .expect("build committed compatibility feedback");
    feedback.candidate_action_id = Some(7);
    feedback.provenance = "runtime_authoritative".into();
    let receipt = runtime_receipt_for_feedback(&feedback);
    let mut store = MemoryWriteStore::default();

    runner
        .consume_runtime_feedback_with_lineage(
            AGENT_ID,
            feedback.clone(),
            Some(&receipt),
            &mut store,
        )
        .expect("correlated lineage remains accepted for compatibility lifecycle");
    assert!(
        store.entries().is_empty(),
        "receipt lineage DTO alone is not Runtime World readback authority"
    );
    runner
        .consume_runtime_feedback_with_lineage(
            AGENT_ID,
            feedback.clone(),
            Some(&receipt),
            &mut store,
        )
        .expect("exact compatibility feedback replay remains idempotent");
    let mut changed_receipt = receipt;
    changed_receipt.receipt_digest =
        "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd".into();
    assert!(
        runner
            .consume_runtime_feedback_with_lineage(
                AGENT_ID,
                feedback,
                Some(&changed_receipt),
                &mut store
            )
            .expect_err("compatibility replay still fences conflicting receipt metadata")
            .to_string()
            .contains("replay receipt lineage mismatch")
    );
    assert!(store.entries().is_empty());
}

#[test]
fn recovered_committed_replay_requires_exact_authenticated_receipt_lineage() {
    let mut runner = provider_backed_runner();
    let feedback = FeedbackEnvelopeV1 {
        feedback_id: "recovered-feedback-1".into(),
        feedback_seq: 1,
        agent_subject: AGENT_ID.into(),
        agent_session_id: SESSION_ID.into(),
        agent_turn_id: "recovered-turn-1".into(),
        decision_request_id: "recovered-request-1".into(),
        candidate_action_id: Some(7),
        runtime_receipt_id: Some("recovered-receipt-1".into()),
        status: "committed".into(),
        request_digest: crate::simulator::Digest32(format!("blake3:{}", "a".repeat(64))),
        reject_reason: None,
        provenance: "runtime_authoritative".into(),
    };
    let mut receipt = runtime_receipt_for_feedback(&feedback);
    receipt.action_id = "action:7".into();
    let projection = oasis7_agent_harness::FeedbackHistoryProjection {
        feedback: crate::simulator::agent_api_compat::feedback_to_api(&feedback),
        acknowledged: true,
        lineage: Some(oasis7_agent_harness::FeedbackHistoryLineage {
            original_envelope_digest: format!("blake3:{}", "b".repeat(64)),
            delivery_state: "acked".into(),
            receipt_lineage: Some(oasis7_agent_api::RuntimeReceiptLineageV1::from(&receipt)),
        }),
    };
    runner
        .restore_preverified_scoped_feedback_history(&[projection])
        .expect("load the host-authenticated receipt projection");

    let mut store = MemoryWriteStore::default();
    runner
        .consume_runtime_feedback_with_lineage(
            AGENT_ID,
            feedback.clone(),
            Some(&receipt),
            &mut store,
        )
        .expect("recovered exact committed replay remains idempotent");
    assert!(
        runner
            .consume_runtime_feedback_with_lineage(AGENT_ID, feedback.clone(), None, &mut store,)
            .expect_err("missing recovered receipt lineage must fail closed")
            .to_string()
            .contains("replay receipt lineage mismatch")
    );
    let mut conflicting_receipt = receipt;
    conflicting_receipt.receipt_digest = format!("blake3:{}", "c".repeat(64));
    assert!(
        runner
            .consume_runtime_feedback_with_lineage(
                AGENT_ID,
                feedback,
                Some(&conflicting_receipt),
                &mut store,
            )
            .expect_err("changed recovered receipt lineage must fail closed")
            .to_string()
            .contains("replay receipt lineage mismatch")
    );
    assert!(store.entries().is_empty());
}

pub(super) struct VerifiedWorldReadback;

impl RuntimeReceiptReadbackVerifier for VerifiedWorldReadback {
    fn verify_world_readback(
        &self,
        _context: &ContinuousAgentTurnContextV1,
        _feedback: &FeedbackEnvelopeV1,
        receipt: &RuntimeReceiptLineageV1,
        response_identity: &ResponseArtifactIdentityV1,
    ) -> Result<RuntimeReceiptReadbackHandleV1, crate::simulator::AsyncAgentRunnerError> {
        Ok(RuntimeReceiptReadbackHandleV1::from_durable_world_readback(
            receipt,
            response_identity,
        ))
    }
}

struct StaleWorldReadback;

impl RuntimeReceiptReadbackVerifier for StaleWorldReadback {
    fn verify_world_readback(
        &self,
        _context: &ContinuousAgentTurnContextV1,
        _feedback: &FeedbackEnvelopeV1,
        receipt: &RuntimeReceiptLineageV1,
        response_identity: &ResponseArtifactIdentityV1,
    ) -> Result<RuntimeReceiptReadbackHandleV1, crate::simulator::AsyncAgentRunnerError> {
        let mut stale_lineage = receipt.clone();
        stale_lineage.feedback_id.push_str("-stale");
        Ok(RuntimeReceiptReadbackHandleV1::from_durable_world_readback(
            &stale_lineage,
            response_identity,
        ))
    }
}

#[test]
fn committed_feedback_can_cross_only_the_world_readback_identity_seam() {
    let mut runner = provider_backed_runner();
    let _turn_id = runner
        .start_turn_with_context(AGENT_ID, host_context())
        .expect("open provider-backed target turn");
    let outcome = completed_turn(&mut runner);
    let mut committed = outcome
        .feedback_for_runtime_status("committed", Some("receipt-readback-1"))
        .expect("build correlated committed feedback");
    committed.candidate_action_id = Some(7);
    committed.provenance = "runtime_authoritative".to_string();
    let receipt = runtime_receipt_for_feedback(&committed);
    let response_identity = outcome
        .prepared_response_context
        .as_ref()
        .expect("response context retained")
        .response_artifact_identity();
    let mut store = MemoryWriteStore::default();
    runner
        .consume_runtime_feedback_with_world_readback(
            AGENT_ID,
            committed.clone(),
            &receipt,
            &response_identity,
            &VerifiedWorldReadback,
            &mut store,
        )
        .expect("Runtime readback verifier admits committed memory");
    assert_eq!(store.entries().len(), 1);
    let accepted_checkpoint = serde_json::to_value(&store).expect("checkpoint serializes");
    assert_eq!(accepted_checkpoint["revision"], 1);
    assert_eq!(
        accepted_checkpoint["entries"][0]["scope"],
        "session_private"
    );
    assert_eq!(
        accepted_checkpoint["entries"][0]["summary"],
        "candidate observation awaiting Runtime receipt"
    );
    assert_eq!(
        accepted_checkpoint["entries"][0]["receipt_id"],
        "receipt-readback-1"
    );
    assert_eq!(
        accepted_checkpoint["entries"][0]["provenance"],
        "runtime_authoritative"
    );
    assert_eq!(accepted_checkpoint["entries"][0]["agent_id"], AGENT_ID);
    assert_eq!(
        accepted_checkpoint["entries"][0]["agent_session_id"],
        SESSION_ID
    );
    runner
        .consume_runtime_feedback_with_lineage(
            AGENT_ID,
            committed.clone(),
            Some(&receipt),
            &mut store,
        )
        .expect("legacy lineaged replay remains idempotent after world-readback completion");
    assert_eq!(
        serde_json::to_value(&store).expect("replayed checkpoint serializes"),
        accepted_checkpoint,
        "accepted replay must preserve the exact persisted memory checkpoint"
    );
    let mut changed_receipt = receipt.clone();
    changed_receipt.envelope_digest =
        "blake3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd".into();
    assert!(
        runner
            .consume_runtime_feedback_with_lineage(
                AGENT_ID,
                committed,
                Some(&changed_receipt),
                &mut store,
            )
            .expect_err("post-release replay must retain the previously accepted receipt lineage")
            .to_string()
            .contains("replay receipt lineage mismatch")
    );
    assert_eq!(store.entries().len(), 1);
    assert_eq!(
        serde_json::to_value(&store).expect("rejected replay checkpoint serializes"),
        accepted_checkpoint,
        "rejected replay must not mutate the accepted checkpoint"
    );
}

#[test]
fn mismatched_full_readback_lineage_cannot_authorize_memory_write() {
    let mut runner = provider_backed_runner();
    runner
        .start_turn_with_context(AGENT_ID, host_context())
        .expect("open provider-backed target turn");
    let outcome = completed_turn(&mut runner);
    let mut committed = outcome
        .feedback_for_runtime_status("committed", Some("receipt-readback-stale"))
        .expect("build correlated committed feedback");
    committed.candidate_action_id = Some(7);
    committed.provenance = "runtime_authoritative".to_string();
    let receipt = runtime_receipt_for_feedback(&committed);
    let response_identity = outcome
        .prepared_response_context
        .as_ref()
        .expect("response context retained")
        .response_artifact_identity();
    let mut store = MemoryWriteStore::default();

    let error = runner
        .consume_runtime_feedback_with_world_readback(
            AGENT_ID,
            committed,
            &receipt,
            &response_identity,
            &StaleWorldReadback,
            &mut store,
        )
        .expect_err("a full-lineage mismatch must fail closed");
    assert!(
        error
            .to_string()
            .contains("readback verifier returned mismatched identity"),
        "unexpected readback error: {error}"
    );
    assert!(store.entries().is_empty(), "mismatched proof wrote memory");
    let checkpoint = serde_json::to_value(&store).expect("memory checkpoint serializes");
    assert_eq!(
        checkpoint["revision"], 0,
        "mismatched proof advanced revision"
    );
}
