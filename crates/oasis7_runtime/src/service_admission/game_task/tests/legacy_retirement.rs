use super::*;
use crate::{CognitionEconomyStateV1, CognitionLeaseQuoteV1, CognitionLeaseRequestV1};
use oasis7_agent_api::ContinuousAgentRequestContextV1;

fn legacy_context(world: &World, agent: &str, decision: &str) -> ContinuousAgentRequestContextV1 {
    // Runtime-owned protocol fixture: no host provider or Simulator execution is involved.
    let mut base: oasis7_agent_api::DecisionRequest = serde_json::from_value(serde_json::json!({
        "observation": {"agent_id":"agent-1", "world_time":7,
            "observation": oasis7_agent_api::ProviderObservation::default(),
            "action_catalog":[{"action_ref":"wait", "summary":"Wait"}], "timeout_budget_ms":60000},
        "timeout_budget_ms":60000
    })).unwrap();
    base.observation.agent_id = agent.into();
    let digest = format!("blake3:{}", "11".repeat(32));
    let mut request: ContinuousAgentRequestContextV1 = serde_json::from_value(serde_json::json!({
        "base_decision_request":base,"context_discriminator":"oasis7.continuous-agent-context","context_version":1,
        "protocol_version":"continuous-agent-v1","agent_session_id":format!("session-{decision}"),
        "agent_turn_id":format!("turn-{decision}"),"decision_request_id":decision,"retry_seq":1,"transport_attempt":1,
        "agent_subject":agent,"runtime_binding":world.current_cognition_runtime_binding().unwrap(),
        "observation_digest":digest,"capability_catalog_digest":digest,"capability_invocation_context_digest":digest,
        "memory_snapshot_digest":digest,"goal_snapshot_digest":digest,"continuation_digest":digest,
        "adapter_protocol_version":"loopback-http-v1","budget_contract":{"max_latency_ms":60000,"max_repair_attempts":2,"max_model_calls":4,"max_tool_calls":3},
        "request_digest":digest
    })).unwrap();
    request.request_digest = request.request_digest();
    request.validate().unwrap();
    request
}
fn prefix(
    world: &mut World,
    context: &ContinuousAgentRequestContextV1,
    key: &str,
    generation: u64,
) {
    let mut logical = context.clone();
    logical.transport_attempt = 1;
    let payload = WorldServicePayloadV1::Scheduler(
        sign_read_request(
            "scheduler",
            SchedulerIntentV1 {
                agent_id: context.agent_subject.clone(),
                request_id: format!("prefix-{}", context.decision_request_id),
                delegation_generation: generation,
                captured_base_binding: world.current_cognition_base_binding().unwrap(),
                operation: SchedulerOperationV1::ProviderPrefix {
                    request: context.clone(),
                    context_digest: oasis7_agent_api::h_v1("oasis7.cognition.context.v1", &logical)
                        .to_string(),
                },
            },
            key,
        )
        .unwrap(),
    );
    let WorldServicePayloadV1::Scheduler(signed) = &payload else {
        unreachable!()
    };
    let receipt = world.apply_authenticated_scheduler(signed.clone()).unwrap();
    let request = SubmitIntentRequest {
        contract_version: 1,
        correlation: derive_correlation(
            WorldIdentity {
                world_id: "world".into(),
                genesis_digest: "fixture".into(),
            },
            &payload,
        )
        .unwrap(),
        signed_payload: payload,
        deadline_unix_ms: None,
    };
    world
        .record_world_service_result(CanonicalIntentResultV1 {
            request,
            receipt,
            action_id: 10,
            committed_height: 3,
            rejected: None,
        })
        .unwrap();
}
fn is_active(world: &World, context: &ContinuousAgentRequestContextV1) -> bool {
    let mut logical = context.clone();
    logical.transport_attempt = 1;
    world
        .validate_active_cognition_context(
            (
                &context.agent_subject,
                &context.agent_session_id,
                &context.agent_turn_id,
                &context.decision_request_id,
                context.request_digest.as_str(),
            ),
            &oasis7_agent_api::h_v1("oasis7.cognition.context.v1", &logical).to_string(),
            context.provider_invocation_key().as_str(),
        )
        .is_ok()
}
#[test]
fn game_task_first_bind_retires_only_agent_legacy_pending_turn_and_reserved_lease() {
    let (mut world, owner, delegate, control) = boot();
    let other_owner = hex::encode([75; 32]);
    world.submit_action(Action::RegisterAgent {
        agent_id: "other".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    // Explicit fixture funding permits a second canonical owner claim after
    // the first starter issuance; the default empty treasury cannot fund it.
    let funding = world.state().starter_oc_claims["agent"].amount;
    let mut supply = world.main_token_supply().clone();
    supply.total_supply += funding;
    supply.total_issued += funding;
    world.set_main_token_supply(supply);
    world
        .set_main_token_treasury_balance(
            crate::MAIN_TOKEN_TREASURY_BUCKET_ECOSYSTEM_POOL,
            world.state().starter_oc_claims["agent"].amount,
        )
        .unwrap();
    world.submit_action(Action::ClaimStarterOc {
        agent_id: "other".into(),
        player_id: "other-owner".into(),
        public_key: Some(
            sign_read_request("key", (), &other_owner)
                .unwrap()
                .subject_public_key,
        ),
    });
    world.step().unwrap();
    world
        .install_capability_agent_identity("other", "other-owner", 1)
        .unwrap();
    let completed = legacy_context(&world, "agent", "completed");
    prefix(&mut world, &completed, &delegate, 1);
    world
        .fail_cognition_turn(
            "agent",
            &completed.agent_session_id,
            &completed.agent_turn_id,
            &completed.decision_request_id,
            completed.request_digest.as_str(),
            "provider_failure",
            1,
            1,
        )
        .unwrap();
    let completed_projection = world.cognition().clone();
    let pending = legacy_context(&world, "agent", "pending");
    prefix(&mut world, &pending, &delegate, 1);
    let other = legacy_context(&world, "other", "other-pending");
    prefix(&mut world, &other, &other_owner, 0);
    // Install valid typed accounting through the fixture snapshot, then verify
    // real World retirement/refund and recovery rather than fabricating balances.
    let mut economy = CognitionEconomyStateV1::default();
    economy
        .set_resource_balance("owner", "cognition_units", 10)
        .unwrap();
    economy
        .set_resource_balance("other-owner", "cognition_units", 10)
        .unwrap();
    let mut reserve = |context: &ContinuousAgentRequestContextV1| {
        economy
            .reserve(
                CognitionLeaseRequestV1::new(
                    format!("lease-{}", context.decision_request_id),
                    if context.agent_subject == "agent" {
                        "owner"
                    } else {
                        "other-owner"
                    },
                    &context.agent_subject,
                    &context.agent_session_id,
                    &context.agent_turn_id,
                    &context.decision_request_id,
                    context.request_digest.as_str(),
                    CognitionLeaseQuoteV1::new(
                        format!("quote-{}", context.decision_request_id),
                        "cognition_units",
                        2,
                    ),
                ),
                world.state().time,
            )
            .unwrap()
    };
    let pending_lease = reserve(&pending);
    let other_lease = reserve(&other);
    let mut checkpoint = goal_snapshot(&world);
    checkpoint.cognition["cognition_economy"] = economy.snapshot_json().unwrap();
    world = World::from_snapshot(checkpoint, world.journal().clone()).unwrap();
    let request = executor_request(&owner, &control, "bind", bind(&delegate, 1, "executor"));
    let mut candidate = world.clone();
    let receipt = prepare_executor_control(&candidate, &request, []).unwrap();
    retire_task_contexts(&mut candidate, "agent", &[]).unwrap();
    candidate
        .record_world_service_result(CanonicalIntentResultV1 {
            request,
            receipt,
            action_id: 20,
            committed_height: 4,
            rejected: None,
        })
        .unwrap();
    assert!(
        is_active(&world, &pending),
        "candidate mutation must not affect original world"
    );
    assert!(!is_active(&candidate, &pending));
    assert!(is_active(&candidate, &other));
    let economy = candidate.cognition_economy().unwrap();
    assert_eq!(
        economy.leases[&pending_lease.lease_id].status,
        oasis7_agent_api::CognitionLeaseStatusV1::Released
    );
    assert_eq!(economy.leases[&other_lease.lease_id], other_lease);
    let old_events = completed_projection["cognition_journal"]["events"]
        .as_array()
        .unwrap();
    for event in old_events
        .iter()
        .filter(|e| e["decision_request_id"] == completed.decision_request_id)
    {
        assert!(
            candidate.cognition()["cognition_journal"]["events"]
                .as_array()
                .unwrap()
                .contains(event)
        );
    }
    let restored =
        World::from_snapshot(goal_snapshot(&candidate), candidate.journal().clone()).unwrap();
    assert!(!is_active(&restored, &pending));
    assert!(is_active(&restored, &other));
    assert_eq!(restored.cognition_economy().unwrap(), economy);
}
