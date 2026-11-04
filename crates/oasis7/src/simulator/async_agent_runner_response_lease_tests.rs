use super::*;
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use std::io::Cursor;

#[test]
fn decoded_provider_charge_lives_through_async_outcome_and_releases_on_drop() {
    let base_decision_response = super::super::DecisionResponse::wait("lease-test-provider");
    let response = ContinuousAgentResponseContextV1 {
        response_digest: Digest32(
            "blake3:0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        ),
        base_decision_response,
        context_discriminator: super::super::CONTINUOUS_AGENT_CONTEXT_DISCRIMINATOR.to_string(),
        context_version: super::super::CONTINUOUS_AGENT_CONTEXT_VERSION,
        agent_session_id: "session.lease-test".to_string(),
        agent_turn_id: "turn.lease-test".to_string(),
        decision_request_id: "request.lease-test".to_string(),
        retry_seq: 1,
        transport_attempt: 1,
        request_digest: Digest32(
            "blake3:1111111111111111111111111111111111111111111111111111111111111111".to_string(),
        ),
    };
    let bytes = serde_json::to_vec(&response).expect("serialize typed response fixture");
    let mut reader = Cursor::new(bytes.as_slice());
    let memory = MemoryBudget::new(8 * 1024 * 1024, 0).expect("lease test pool");

    let (decoded, lease) =
        super::super::provider_json_admission::decode_response_context_with_lease(
            &mut reader,
            bytes.len() as u64,
            &memory,
            MemoryClass::Ordinary,
            || false,
        )
        .expect("bounded response decode");
    let reserved_bytes = lease.reserved_bytes();
    assert!(reserved_bytes > 0);
    assert_eq!(memory.used(), reserved_bytes);

    let outcome = outcome::outcome_from_completion(ActorCompletion {
        turn_id: AsyncTurnId(1),
        agent_id: "agent-lease-test".to_string(),
        decision: Some(AgentDecision::Wait),
        decision_trace: None,
        prepared_context: None,
        prepared_request_context: None,
        prepared_response_context: Some(decoded),
        provider_original_material_refs: None,
        provider_response_context_lease: Some(lease),
        memory_write_intents: Vec::new(),
        panicked: false,
    });

    assert!(outcome.prepared_response_context.is_some());
    assert_eq!(
        outcome
            .provider_response_context_lease
            .as_ref()
            .expect("outcome retains provider decode charge")
            .reserved_bytes(),
        reserved_bytes
    );
    assert_eq!(memory.used(), reserved_bytes);
    drop(outcome);
    assert_eq!(memory.used(), 0);
}
