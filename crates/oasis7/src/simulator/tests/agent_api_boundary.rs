//! B2 compatibility fixtures for the portable Agent API boundary.
//!
//! These tests prove data and identity equivalence only. They intentionally
//! leave provider execution, Runtime admission, durable receipts, and World
//! readback authority in their existing Oasis7 owners.

use crate::runtime::{CognitionLeaseStatusV1, RuntimeReceiptLineageV1};
use crate::simulator::{
    BudgetContractV1, CognitionError, ContinuationBudgetV1, ContinuationProposalV1,
    ContinuousAgentResponseContextV1, DecisionResponse, Digest32, FeedbackEnvelopeV1,
    GoalSnapshotV1, MemoryContextSnapshotV1, MemoryWriteIntentV1, NormalizedMemoryWriteIntentV1,
    ProviderExecutionMode, ResponseArtifactIdentityV1, RuntimeBindingV1, WakeConditionV1,
};
use serde_json::json;

#[test]
fn unaffected_shared_types_are_api_type_identities() {
    fn same_type<T>(_: Option<T>, _: Option<T>) {}

    same_type(
        None::<crate::capability_invocation_context::CapabilityInvocationContext>,
        None::<oasis7_agent_api::CapabilityInvocationContext>,
    );
    same_type(
        None::<BudgetContractV1>,
        None::<oasis7_agent_api::BudgetContractV1>,
    );
    same_type(
        None::<ContinuationBudgetV1>,
        None::<oasis7_agent_api::ContinuationBudgetV1>,
    );
    same_type(
        None::<ContinuationProposalV1>,
        None::<oasis7_agent_api::ContinuationProposalV1>,
    );
    same_type(
        None::<WakeConditionV1>,
        None::<oasis7_agent_api::WakeConditionV1>,
    );
    same_type(
        None::<ProviderExecutionMode>,
        None::<oasis7_agent_api::ProviderExecutionMode>,
    );
    let _: CognitionError = oasis7_agent_api::CognitionError::new("code", "message");
    let _: Digest32 = oasis7_agent_api::Digest32::from("blake3:identity");
    let _: RuntimeBindingV1 = oasis7_agent_api::RuntimeBindingV1 {
        world_id: "world".into(),
        branch_id: "branch".into(),
        finality_epoch: 1,
        finality_block_hash: None,
        finality_status: "pending".into(),
        base_tick: 0,
        base_world_hash: Digest32::default(),
        reorg_epoch: 0,
        runtime_manifest_hash: Digest32::default(),
    };
    let _: GoalSnapshotV1 = oasis7_agent_api::GoalSnapshotV1::empty();
    let _: MemoryContextSnapshotV1 =
        oasis7_agent_api::MemoryContextSnapshotV1::empty("turn_private");
    let _: CognitionLeaseStatusV1 = oasis7_agent_api::CognitionLeaseStatusV1::Reserved;
    let _: ResponseArtifactIdentityV1 = oasis7_agent_api::ResponseArtifactIdentityV1 {
        schema_version: 1,
        context_discriminator: "context".into(),
        context_version: 1,
        agent_session_id: "session".into(),
        agent_turn_id: "turn".into(),
        decision_request_id: "request".into(),
        retry_seq: 1,
        transport_attempt: 1,
        request_digest: Digest32::default(),
        response_digest: Digest32::default(),
        artifact_digest: Digest32::default(),
    };
}

#[test]
fn request_wrapper_round_trip_preserves_wire_bytes_and_request_identities() {
    let request = super::agent_cognition_identity::request_from_value(
        super::agent_cognition_identity::production_request_fixture(1, 60_000),
    );
    request
        .validate()
        .expect("legacy request wrapper validates");

    let api = super::super::agent_api_compat::request_context_to_api(&request);
    assert_eq!(
        serde_json::to_value(&api).expect("encode API request"),
        serde_json::to_value(&request).expect("encode Oasis7 request")
    );
    assert_eq!(
        request
            .canonical_request_bytes()
            .expect("Oasis7 canonical bytes"),
        api.canonical_request_bytes().expect("API canonical bytes")
    );
    assert_eq!(request.request_digest(), api.request_digest());
    assert_eq!(
        request.provider_invocation_key(),
        api.provider_invocation_key()
    );

    let round_trip = super::super::agent_api_compat::request_context_from_api(api);
    assert_eq!(round_trip, request);
    assert_eq!(round_trip.validate(), request.validate());
}

#[test]
fn response_wrapper_round_trip_preserves_artifact_identity_and_wire_shape() {
    let response = ContinuousAgentResponseContextV1 {
        base_decision_response: DecisionResponse::wait("boundary-fixture-provider"),
        context_discriminator: crate::simulator::CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR.into(),
        context_version: crate::simulator::CONTINUOUS_AGENT_CONTEXT_VERSION,
        agent_session_id: "session-boundary".into(),
        agent_turn_id: "turn-boundary".into(),
        decision_request_id: "request-boundary".into(),
        retry_seq: 2,
        transport_attempt: 3,
        request_digest: Digest32::from(
            "blake3:1111111111111111111111111111111111111111111111111111111111111111",
        ),
        response_digest: Digest32::from(
            "blake3:2222222222222222222222222222222222222222222222222222222222222222",
        ),
    };
    let api = super::super::agent_api_compat::response_context_to_api(&response);
    assert_eq!(
        serde_json::to_value(&api).expect("encode API response"),
        serde_json::to_value(&response).expect("encode Oasis7 response")
    );
    assert_eq!(
        response.response_artifact_identity(),
        api.response_artifact_identity()
    );
    assert_eq!(
        response
            .response_artifact_identity_payload()
            .expect("Oasis7 identity payload"),
        api.response_artifact_identity_payload()
            .expect("API identity payload")
    );
    let round_trip = super::super::agent_api_compat::response_context_from_api(api);
    assert_eq!(round_trip, response);
}

#[test]
fn feedback_and_memory_intent_adapters_preserve_serialized_fields() {
    let feedback = FeedbackEnvelopeV1 {
        feedback_id: "feedback-boundary".into(),
        feedback_seq: 1,
        agent_subject: "agent-boundary".into(),
        agent_session_id: "session-boundary".into(),
        agent_turn_id: "turn-boundary".into(),
        decision_request_id: "request-boundary".into(),
        candidate_action_id: Some(17),
        runtime_receipt_id: Some("receipt-boundary".into()),
        status: "committed".into(),
        request_digest: Digest32::from(
            "blake3:3333333333333333333333333333333333333333333333333333333333333333",
        ),
        reject_reason: None,
        provenance: "runtime_authoritative".into(),
    };
    let api_feedback = super::super::agent_api_compat::feedback_to_api(&feedback);
    assert_eq!(
        serde_json::to_value(api_feedback).expect("encode API feedback"),
        serde_json::to_value(&feedback).expect("encode Oasis7 feedback")
    );

    let normalized = NormalizedMemoryWriteIntentV1 {
        schema_version: 1,
        scope: "turn_private".into(),
        summary: Some("remember this".into()),
        tags: vec!["bounded".into(), "private".into()],
        compatibility_reason: None,
    };
    let local: MemoryWriteIntentV1 = normalized.into();
    let api = super::super::agent_api_compat::memory_write_intent_to_api(&local);
    assert_eq!(
        serde_json::to_value(api).expect("encode API memory intent"),
        serde_json::to_value(local).expect("encode Oasis7 memory intent")
    );
}

#[test]
fn receipt_api_projection_is_lossless_correlation_not_runtime_readback_proof() {
    assert_ne!(
        std::any::TypeId::of::<RuntimeReceiptLineageV1>(),
        std::any::TypeId::of::<oasis7_agent_api::RuntimeReceiptLineageV1>(),
        "portable lineage correlation DTO must not become Runtime proof type"
    );
    let runtime = RuntimeReceiptLineageV1 {
        schema_version: RuntimeReceiptLineageV1::SCHEMA_VERSION.into(),
        status: "committed".into(),
        receipt_id: "receipt-boundary".into(),
        receipt_digest: "blake3:1111111111111111111111111111111111111111111111111111111111111111"
            .into(),
        envelope_digest: "blake3:2222222222222222222222222222222222222222222222222222222222222222"
            .into(),
        action_id: "action-boundary".into(),
        agent_id: "agent-boundary".into(),
        agent_session_id: "session-boundary".into(),
        agent_turn_id: "turn-boundary".into(),
        decision_request_id: "request-boundary".into(),
        request_digest: "blake3:3333333333333333333333333333333333333333333333333333333333333333"
            .into(),
        feedback_id: "feedback-boundary".into(),
    };
    runtime.validate().expect("valid Runtime lineage fixture");
    let api = oasis7_agent_api::RuntimeReceiptLineageV1::from(&runtime);
    api.validate()
        .expect("API projection structural validation");
    assert_eq!(
        serde_json::to_value(api).expect("encode API lineage"),
        serde_json::to_value(&runtime).expect("encode Runtime lineage")
    );
    // Readback and authoritative cognition commit paths continue to accept
    // the Runtime-owned lineage type, not the portable correlation DTO.
    fn runtime_lineage_only(_: &RuntimeReceiptLineageV1) {}
    runtime_lineage_only(&runtime);
}

#[test]
fn typed_provider_decision_alias_keeps_generic_payloads_without_erasure() {
    let decision: oasis7_agent_api::ProviderDecision<
        oasis7_simulator::Action,
        oasis7_simulator::AgentQuery,
    > = oasis7_agent_api::ProviderDecision::Wait;
    let _: crate::simulator::ProviderDecision = decision;
}

#[test]
fn transport_attempt_and_legacy_timeout_stay_outside_request_identity() {
    let mut request = super::agent_cognition_identity::request_from_value(
        super::agent_cognition_identity::production_request_fixture(1, 60_000),
    );
    let original_digest = request.request_digest();
    request.transport_attempt += 5;
    request.base_decision_request.timeout_budget_ms += 120;
    assert_eq!(request.request_digest(), original_digest);
    let api = super::super::agent_api_compat::request_context_to_api(&request);
    assert_eq!(original_digest, api.request_digest());
    assert_eq!(
        serde_json::to_value(api).expect("encode API request")["transport_attempt"],
        json!(6)
    );
}

#[test]
fn scheduler_api_preserves_capability_digest_rejection_diagnostics() {
    for catalog_mismatch in [true, false] {
        let mut request = super::agent_cognition_identity::request_from_value(
            super::agent_cognition_identity::production_request_fixture(1, 60_000),
        );
        request.retry_seq = 1;
        if catalog_mismatch {
            request.capability_catalog_digest = Digest32(format!("blake3:{}", "0".repeat(64)));
        } else {
            request.capability_invocation_context_digest =
                Digest32(format!("blake3:{}", "0".repeat(64)));
        }
        request.request_digest = request.request_digest();
        let api = super::super::agent_api_compat::request_context_to_api(&request);
        let host_error = request
            .validate_production_lane()
            .expect_err("tampered digest");
        let api_error = api.validate_production_lane().expect_err("tampered digest");
        assert_eq!(host_error.code(), "capability_context_digest_mismatch");
        assert_eq!(
            host_error.message(),
            if catalog_mismatch {
                "capability catalog digest does not match the Runtime-bound snapshot"
            } else {
                "capability invocation context digest does not match the Runtime-bound context"
            }
        );
        assert_eq!(api_error.code(), host_error.code());
        assert_eq!(api_error.message(), host_error.message());
    }
}
