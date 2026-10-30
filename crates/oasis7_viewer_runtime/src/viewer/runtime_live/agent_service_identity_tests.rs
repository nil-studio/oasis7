use crate::viewer::ViewerRuntimeLiveServer;
use crate::viewer::ViewerRuntimeLiveServerConfig;
use crate::viewer::runtime_live::agent_service_io::{
    AgentServiceIoOperation, HostedIdentityReservation, PreparedHostedIdentity,
};
use crate::viewer::runtime_live::agent_service_phase::HostedServicePhase;
use crate::viewer::runtime_live::control_plane::llm_sidecar::RuntimeProviderActionContext;
use crate::world_service::client::RemoteWorldServiceClient;
use oasis7_client_api::world_service::{LookupIntentRequest, WORLD_SERVICE_CONTRACT_VERSION};

use crate::viewer::runtime_live::control_plane::llm_sidecar::lineage_persistence::PendingProviderServiceIntent;
use crate::world_service::client::{WorldServiceAgentSignerConfig, WorldServiceClientConfig};
use crate::world_service::wire::{FeedbackAckIntentV1, WorldServicePayloadV1};
use oasis7_client_api::world_service::WorldIdentity;
use oasis7_simulator::WorldScenario;

fn service_cognition() -> RuntimeProviderActionContext {
    let mut request = super::tests::test_provider_context(
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

fn pending_with_feedback_ack(
    cognition: &RuntimeProviderActionContext,
) -> PendingProviderServiceIntent {
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
    let ack = crate::viewer::runtime_live::control_plane::llm_sidecar::service_feedback_ack::PendingFeedbackAck {
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
    PendingProviderServiceIntent {
        correlation,
        payload,
        cognition: cognition.clone(),
        action: oasis7_simulator::Action::RemoveModuleVisualEntity {
            entity_id: "issued-checkpoint-test".into(),
        },
        feedback_ack: Some(ack),
    }
}

fn capsule() -> (ViewerRuntimeLiveServer, PreparedHostedIdentity) {
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("server");
    let cognition = service_cognition();
    let pending = pending_with_feedback_ack(&cognition);
    let config = WorldServiceClientConfig {
        endpoint: "http://127.0.0.1:1".into(),
        trusted_service_public_key: hex::encode([12; 32]),
        expected_world: pending.correlation.key.world.clone(),
        scope_id: "public".into(),
        read_private_key_hex: hex::encode([13; 32]),
        timeout: std::time::Duration::from_millis(10),
        formal_read_trust: None,
        max_response_bytes: 4096,
    };
    let client = RemoteWorldServiceClient::new(config.clone()).expect("client");
    server.config.world_service = Some(config);
    server.config.world_service_agent_signer = Some(WorldServiceAgentSignerConfig {
        private_key_hex: hex::encode([14; 32]),
        delegation_generation: 3,
    });
    let mut phase = HostedServicePhase::Act {
        pending: pending.clone(),
        submit: true,
    };
    let digest_before = phase.identity().expect("identity");
    PreparedHostedIdentity::reserve_submit(Some(&mut phase));
    assert!(phase.identity().expect("reserved identity") == digest_before);
    let prepared = PreparedHostedIdentity::new(
        1,
        server.hosted_service_config_digest().expect("config"),
        phase.clone(),
        client,
        AgentServiceIoOperation::Lookup {
            request: LookupIntentRequest {
                contract_version: WORLD_SERVICE_CONTRACT_VERSION,
                key: pending.correlation.key.clone(),
            },
            original: pending.payload.clone(),
        },
    );
    server.llm_sidecar.provider_service_pending.insert(
        pending
            .cognition
            .request
            .request_context
            .agent_subject
            .clone(),
        pending.clone(),
    );
    server.llm_sidecar.hosted_service_generation = 1;
    server.llm_sidecar.hosted_service_phase = Some(phase);
    server.llm_sidecar.hosted_identity_reservation = Some(prepared.reservation());
    (server, prepared)
}

#[test]
fn deferred_identity_same_phase_dispatches_once_with_exact_identity() {
    let (mut server, prepared) = capsule();
    let expected = server
        .llm_sidecar
        .hosted_service_phase
        .as_ref()
        .unwrap()
        .identity()
        .unwrap();
    let ready = prepared.compute().expect("outside computation");
    let job = server.finish_hosted_identity(ready).expect("same phase");
    assert!(job.token.phase_digest == expected);
    assert!(server.llm_sidecar.hosted_identity_reservation.is_none());
    assert!(server.llm_sidecar.hosted_service_inflight.as_ref() == Some(&job.token));
    assert!(matches!(
        job.operation,
        AgentServiceIoOperation::Lookup { .. }
    ));
}

#[test]
fn deferred_identity_config_or_full_phase_changes_release_no_job() {
    for config_change in [false, true] {
        let (mut server, prepared) = capsule();
        let reservation = prepared.reservation();
        let ready = prepared.compute().unwrap();
        if config_change {
            server.config.world_service.as_mut().unwrap().scope_id = "changed".into();
        } else {
            // Submit is deliberately excluded from the old identity, but full
            // typed phase equality must still reject a concurrent reset.
            PreparedHostedIdentity::reserve_submit(
                server.llm_sidecar.hosted_service_phase.as_mut(),
            );
            let Some(HostedServicePhase::Act { submit, .. }) =
                server.llm_sidecar.hosted_service_phase.as_mut()
            else {
                panic!("act")
            };
            *submit = true;
        }
        assert!(server.finish_hosted_identity(ready).is_err());
        assert!(server.llm_sidecar.hosted_service_inflight.is_none());
        server.abandon_hosted_identity(&reservation);
        assert!(server.llm_sidecar.hosted_identity_reservation.is_none());
    }
}

#[test]
fn deferred_identity_abandoned_or_stale_reservation_cannot_revive_submit() {
    let (mut server, prepared) = capsule();
    let old = prepared.reservation();
    drop(prepared);
    server.abandon_hosted_identity(&old);
    assert!(server.llm_sidecar.hosted_service_inflight.is_none());
    assert!(matches!(
        server.llm_sidecar.hosted_service_phase.as_ref(),
        Some(HostedServicePhase::Act { submit: false, .. })
    ));
    let newer = HostedIdentityReservation {
        generation: 2,
        config_digest: old.config_digest.clone(),
    };
    server.llm_sidecar.hosted_identity_reservation = Some(newer.clone());
    server.abandon_hosted_identity(&old);
    assert!(server.llm_sidecar.hosted_identity_reservation.as_ref() == Some(&newer));
}

#[test]
fn deferred_identity_queue_failure_keeps_exact_original_and_lookup_recovery() {
    let (mut server, prepared) = capsule();
    let before = serde_json::to_vec(&server.llm_sidecar.provider_service_pending).unwrap();
    let job = server
        .finish_hosted_identity(prepared.compute().unwrap())
        .unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    drop(receive);
    assert!(send.try_send(job).is_err());
    server.agent_service_io_worker_failed();
    assert!(server.llm_sidecar.hosted_service_inflight.is_none());
    assert!(server.llm_sidecar.hosted_identity_reservation.is_none());
    // Existing worker-loss recovery may clear the transient phase, but it must
    // not restore a Submit. Durable maps are not touched by the reservation.
    assert!(server.llm_sidecar.hosted_service_phase.is_none());
    assert!(serde_json::to_vec(&server.llm_sidecar.provider_service_pending).unwrap() == before);
}

#[test]
fn deferred_identity_reservation_blocks_other_preparation_and_stale_finish() {
    let (mut server, prepared) = capsule();
    assert!(matches!(
        server.prepare_agent_service_io(false).unwrap(),
        crate::viewer::runtime_live::agent_service_io::AgentServiceProgress::Idle
    ));
    let reservation = prepared.reservation();
    let ready = prepared.compute().unwrap();
    server.llm_sidecar.hosted_service_generation += 1;
    assert!(server.finish_hosted_identity(ready).is_err());
    assert!(server.llm_sidecar.hosted_service_inflight.is_none());
    server.abandon_hosted_identity(&reservation);
    assert!(matches!(
        server.llm_sidecar.hosted_service_phase.as_ref(),
        Some(HostedServicePhase::Act { submit: false, .. })
    ));
}
