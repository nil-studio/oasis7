//! Actual encrypted-cache + authenticated HTTP adapter, not a mock host trait.
use super::*;

pub(super) fn action_payload(
    context: &TaskContextIdentityV1,
    base: oasis7_agent_api::world_cognition::RuntimeCognitionBaseBindingV1,
) -> WorldServicePayloadV1 {
    use oasis7_agent_api::world_cognition::*;
    let request = RuntimeCognitionCommitRequestV1 {
        agent_id: "a".into(),
        agent_session_id: context.agent_session_id.clone(),
        agent_turn_id: context.agent_turn_id.clone(),
        decision_request_id: context.decision_request_id.clone(),
        retry_seq: 1,
        transport_attempt: 1,
        request_digest: context.request_commitment.clone(),
        observation_digest: format!("blake3:{}", "44".repeat(32)),
        context_digest: context.context_commitment.clone(),
        capability_snapshot_hash: format!("blake3:{}", "55".repeat(32)),
        authority_context_hash: format!("blake3:{}", "66".repeat(32)),
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
        response_digest: format!("blake3:{}", "77".repeat(32)),
        artifact_digest: String::new(),
    };
    artifact.refresh_artifact_digest();
    let intent = CognitionIntentV1 {
        request,
        action: crate_action::Action::MoveAgent {
            agent_id: "a".into(),
            to: crate_action::GeoPos::new(0, 0, 0),
        },
        response_artifact: artifact,
        delegation_generation: 1,
        causal_proposal: None,
        task_fence: Some(context.fence.clone()),
    };
    WorldServicePayloadV1::Cognition(
        authority::sign_read_request("cognition", intent, &hex::encode([7; 32])).unwrap(),
    )
}

pub(super) fn status_response(
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
    status: &str,
) -> IntentResponse<Value> {
    let mut response = committed(original);
    response.outcome = match status {
        "received" => IntentOutcome::Received {
            durability: AdmissionDurability::Volatile,
        },
        "pending" => IntentOutcome::Pending,
        "unknown" => IntentOutcome::Unknown,
        "history_unavailable" => IntentOutcome::HistoryUnavailable,
        "rejected" => IntentOutcome::Rejected {
            reason: WorldServiceErrorKind::Conflict,
        },
        "expired" => IntentOutcome::Expired,
        _ => panic!("unknown fixture status"),
    };
    response
}
fn cache(probe: &Probe, kind: OriginalKind) -> ClientStateKind {
    let lane = match kind {
        OriginalKind::Prefix => ClientStateKind::Prefix,
        OriginalKind::Action => ClientStateKind::Action,
        OriginalKind::Outcome => ClientStateKind::Outcome,
    };
    assert_eq!(
        probe
            .client
            .cache_intent(lane, "slot", probe.original.signed_payload.clone())
            .unwrap(),
        probe.original
    );
    lane
}

#[test]
fn lifecycle_separates_committed_prefix_action_and_no_effect_termination_without_submit() {
    for kind in [
        OriginalKind::Prefix,
        OriginalKind::Action,
        OriginalKind::Outcome,
    ] {
        let probe = Probe::new_kind(Fault::DropFirst, kind);
        let lane = cache(&probe, kind);
        let result = probe.client.recover_operation("slot").unwrap();
        assert_eq!(
            result.disposition,
            match kind {
                OriginalKind::Prefix => OperationDisposition::ContextReady,
                OriginalKind::Action => OperationDisposition::ActionCommitted,
                OriginalKind::Outcome => OperationDisposition::ContextTerminated {
                    reason: "executor_aborted".into()
                },
            }
        );
        assert_eq!(result.originals[0].kind, kind);
        assert_eq!(result.originals[0].correlation, probe.original.correlation);
        assert_eq!(result.logical_operation, "slot");
        assert_eq!(
            result.delivery_phase,
            crate::operation_lifecycle::DeliveryPhase::Unknown
        );
        assert_eq!(
            result.next_action,
            crate::operation_lifecycle::RecoveryAction::ReadTerminal
        );
        assert_eq!(
            result.response.unwrap().correlation,
            probe.original.correlation
        );
        assert_eq!(
            probe.client.cached_intent(lane, "slot").unwrap().unwrap(),
            probe.original
        );
        assert_eq!(probe.submits.load(Ordering::SeqCst), 0);
        assert_eq!(probe.lookups.load(Ordering::SeqCst), 2);
    }
}

#[test]
fn lifecycle_preserves_all_verified_dispositions_and_unknown_history_is_not_rejected() {
    for status in [
        "received",
        "pending",
        "unknown",
        "history_unavailable",
        "rejected",
        "expired",
    ] {
        let probe = Probe::new(Fault::Status(status));
        cache(&probe, OriginalKind::Prefix);
        let result = probe.client.recover_operation("slot").unwrap();
        assert_eq!(
            serde_json::to_value(result.disposition).unwrap()["status"],
            status
        );
        assert_eq!(probe.submits.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn lifecycle_conflicts_wrong_lane_bad_signature_and_correlation_never_produce_confirmed_facts() {
    for wrong_lane in [ClientStateKind::Action, ClientStateKind::Outcome] {
        let probe = Probe::new(Fault::DropFirst);
        assert!(
            probe
                .client
                .cache_intent(wrong_lane, "slot", probe.original.signed_payload.clone())
                .unwrap_err()
                .contains("kind mismatch")
        );
        assert_eq!(
            probe.client.recover_operation("slot").unwrap().disposition,
            OperationDisposition::Vacant
        );
        assert_eq!(probe.lookups.load(Ordering::SeqCst), 0);
    }
    let probe = Probe::new(Fault::DropFirst);
    let mut corrupt = probe.original.clone();
    corrupt.correlation.payload_digest = format!("blake3:{}", "ff".repeat(32));
    assert!(
        probe
            .client
            .state
            .put_client_state(
                ClientStateKind::Prefix,
                "slot",
                &ClientStateV1::PublicIntent(Box::new(corrupt))
            )
            .is_err()
    );
    assert_eq!(
        probe.client.recover_operation("slot").unwrap().disposition,
        OperationDisposition::Vacant
    );
    assert_eq!(probe.lookups.load(Ordering::SeqCst), 0);
    let probe = Probe::new(Fault::WrongSignature);
    cache(&probe, OriginalKind::Prefix);
    assert!(
        probe
            .client
            .recover_operation("slot")
            .unwrap_err()
            .contains("response not verified")
    );
    assert_eq!(probe.submits.load(Ordering::SeqCst), 0);
}

#[test]
fn lifecycle_missing_and_conflicting_originals_never_query_or_write() {
    let probe = Probe::new(Fault::DropFirst);
    assert_eq!(
        probe.client.recover_operation("slot").unwrap().disposition,
        OperationDisposition::Vacant
    );
    for (lane, kind) in [
        (ClientStateKind::Action, OriginalKind::Action),
        (ClientStateKind::Outcome, OriginalKind::Outcome),
    ] {
        let source = Probe::new_kind(Fault::DropFirst, kind);
        probe
            .client
            .cache_intent(lane, "slot", source.original.signed_payload.clone())
            .unwrap();
    }
    assert!(
        probe
            .client
            .recover_operation("slot")
            .unwrap_err()
            .contains("conflicting")
    );
    assert_eq!(probe.lookups.load(Ordering::SeqCst), 0);
    assert_eq!(probe.submits.load(Ordering::SeqCst), 0);
}

#[test]
fn lifecycle_rejects_original_signature_tampering_and_opaque_cache_corruption_before_io() {
    let probe = Probe::new(Fault::DropFirst);
    let mut tampered = probe.original.signed_payload.clone();
    let WorldServicePayloadV1::Scheduler(signed) = &mut tampered else {
        unreachable!()
    };
    signed.signature_hex = "00".repeat(64);
    assert!(
        probe
            .client
            .cache_intent(ClientStateKind::Prefix, "slot", tampered)
            .is_err()
    );
    cache(&probe, OriginalKind::Prefix);
    let name = crate::operation_store::client_state_name(&[7; 32], ClientStateKind::Prefix, "slot")
        .unwrap();
    std::fs::write(probe.root.join(name), b"opaque corrupt encrypted record").unwrap();
    assert!(probe.client.recover_operation("slot").is_err());
    assert_eq!(probe.lookups.load(Ordering::SeqCst), 0);
    assert_eq!(probe.submits.load(Ordering::SeqCst), 0);
}
