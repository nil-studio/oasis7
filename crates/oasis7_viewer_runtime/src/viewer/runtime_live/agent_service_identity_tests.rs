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
        provider_original_material_refs: None,
        provider_response_context_lease: None,
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

fn completion_fixture() -> (
    ViewerRuntimeLiveServer,
    crate::viewer::runtime_live::agent_service_io::AgentServiceIoResult,
) {
    use crate::viewer::runtime_live::agent_service_io::{
        AgentServiceIoResponse, AgentServiceIoResult,
    };
    use oasis7_client_api::world_service::{IntentOutcome, IntentResponse};
    let (mut server, prepared) = capsule();
    let job = server
        .finish_hosted_identity(prepared.compute().unwrap())
        .unwrap();
    let HostedServicePhase::Act { pending, .. } =
        server.llm_sidecar.hosted_service_phase.as_ref().unwrap()
    else {
        panic!("act fixture");
    };
    let result = AgentServiceIoResult {
        token: job.token,
        response: Ok(AgentServiceIoResponse::Intent(IntentResponse {
            formal_profile: None,
            execution_evidence_scope: None,
            contract_version: WORLD_SERVICE_CONTRACT_VERSION,
            correlation: pending.correlation.clone(),
            outcome: IntentOutcome::Pending,
        })),
    };
    (server, result)
}

#[test]
fn completion_identity_computation_releases_viewer_mutex() {
    const CHILD: &str = "OASIS7_COMPLETION_IDENTITY_LOCK_PROBE";
    if std::env::var_os(CHILD).is_none() {
        let name = format!(
            "{}::completion_identity_computation_releases_viewer_mutex",
            module_path!()
        );
        let name = name.split_once("::").unwrap().1;
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--test-threads=1"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "isolated lock probe failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        return;
    }
    use crate::viewer::runtime_live::agent_service_result::prepare_result_phase;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    for mode in 0..6 {
        let (mut server, result) = completion_fixture();
        if mode == 5 {
            server.agent_service_pump_error =
                Some("Agent service result could not be applied; original work remains pending");
            let error = "Transport(previous lookup disconnect)".to_string();
            server
                .agent_service_io_diagnostics
                .finish_apply((Some(error.clone()), 1), &Err(error));
        }
        let shared = Arc::new(Mutex::new(server));
        ViewerRuntimeLiveServer::test_arm_hosted_identity_barrier();
        let worker_shared = shared.clone();
        let worker = std::thread::spawn(move || {
            let mut guard = worker_shared.lock().unwrap();
            if (1..=3).contains(&mode) || mode == 5 {
                if mode == 5 {
                    assert!(guard.apply_agent_service_completion(result, None).is_ok());
                }
                use crate::viewer::runtime_live::agent_service_result::{
                    RetryIdentity, prepare_retry_identity,
                };
                let (guard, identity) = prepare_retry_identity(&worker_shared, guard).unwrap();
                if mode == 5 {
                    assert_eq!(
                        guard.agent_service_pump_error,
                        Some("newer control failure")
                    );
                }
                if mode == 1 || mode == 5 {
                    assert!(matches!(identity, RetryIdentity::Current(Some(Ok(_)))));
                } else {
                    assert!(matches!(identity, RetryIdentity::Changed));
                }
            } else {
                let (mut guard, prepared) =
                    prepare_result_phase(&worker_shared, guard, &result).unwrap();
                let applied = guard.apply_prepared_agent_result(result, prepared);
                if mode == 4 {
                    assert!(applied.is_err());
                    assert!(guard.llm_sidecar.hosted_service_phase.is_none());
                } else {
                    assert!(matches!(applied.unwrap(), crate::viewer::runtime_live::agent_service_io::AgentServiceProgress::Advanced));
                }
            }
        });
        let entered =
            ViewerRuntimeLiveServer::test_wait_hosted_identity_barrier(Duration::from_secs(2));
        let observer_can_lock = match shared.try_lock() {
            Ok(mut guard) => {
                if mode == 5 {
                    assert!(
                        guard.agent_service_pump_error.is_none(),
                        "successful completion must clear the old pump error before retry identity unlocks"
                    );
                    assert_eq!(
                        guard.agent_service_io_diagnostics.snapshot()["last_apply_error_class"],
                        "absent"
                    );
                    guard.agent_service_pump_error = Some("newer control failure");
                }
                if mode == 2 || mode == 4 {
                    guard.llm_sidecar.hosted_service_phase = None;
                } else if mode == 3 {
                    guard.llm_sidecar.hosted_service_generation += 1;
                }
                true
            }
            Err(_) => false,
        };
        ViewerRuntimeLiveServer::test_release_hosted_identity_barrier();
        worker.join().unwrap();
        assert!(
            entered,
            "actual identity must reach the computation barrier"
        );
        assert!(
            observer_can_lock,
            "observer must acquire Viewer while identity is blocked, mode={mode}"
        );
    }
}

#[test]
fn prepared_completion_rejects_phase_generation_token_and_config_changes() {
    use crate::viewer::runtime_live::agent_service_result::prepare_result_phase;
    for changed in 0..7 {
        let (server, mut result) = completion_fixture();
        let shared = std::sync::Mutex::new(server);
        let guard = shared.lock().unwrap();
        let (mut guard, prepared) = prepare_result_phase(&shared, guard, &result).unwrap();
        match changed {
            0 => {
                let Some(HostedServicePhase::Act { submit, .. }) =
                    guard.llm_sidecar.hosted_service_phase.as_mut()
                else {
                    panic!("act");
                };
                *submit = true; // Excluded from the digest: full phase comparison must still reject it.
            }
            1 => {
                guard.llm_sidecar.hosted_service_generation += 1;
                guard
                    .llm_sidecar
                    .hosted_service_inflight
                    .as_mut()
                    .unwrap()
                    .generation += 1;
            }
            2 => result.token.generation += 1,
            3 => guard.config.world_service.as_mut().unwrap().scope_id = "new-scope".into(),
            4 => guard.llm_sidecar.hosted_service_phase = None,
            5 => guard.llm_sidecar.hosted_service_generation += 1,
            6 => guard.llm_sidecar.hosted_service_inflight = None,
            _ => unreachable!(),
        }
        let pending = guard.llm_sidecar.provider_service_pending.clone();
        let phase = guard.llm_sidecar.hosted_service_phase.clone();
        let inflight = guard.llm_sidecar.hosted_service_inflight.clone();
        assert!(guard.apply_prepared_agent_result(result, prepared).is_err());
        assert!(guard.llm_sidecar.provider_service_pending == pending);
        assert!(guard.llm_sidecar.hosted_service_phase == phase);
        if matches!(changed, 1 | 2 | 6) {
            assert!(guard.llm_sidecar.hosted_service_inflight == inflight);
        } else {
            assert!(guard.llm_sidecar.hosted_service_inflight.is_none());
        }
    }
}

#[test]
fn prepared_completion_preserves_pending_and_checks_original_phase_digest() {
    use crate::viewer::runtime_live::agent_service_io::AgentServiceProgress;
    use crate::viewer::runtime_live::agent_service_result::prepare_result_phase;
    for invalid_digest in [false, true] {
        let (mut server, mut result) = completion_fixture();
        if invalid_digest {
            result.token.phase_digest.push_str("-tampered");
            server.llm_sidecar.hosted_service_inflight = Some(result.token.clone());
        }
        let shared = std::sync::Mutex::new(server);
        let guard = shared.lock().unwrap();
        let (mut guard, prepared) = prepare_result_phase(&shared, guard, &result).unwrap();
        assert!(prepared.is_some());
        let pending = guard.llm_sidecar.provider_service_pending.clone();
        let phase = guard.llm_sidecar.hosted_service_phase.clone();
        let applied = guard.apply_prepared_agent_result(result, prepared);
        if invalid_digest {
            assert!(
                matches!(applied, Err(reason) if reason == "hosted service phase identity mismatch")
            );
        } else {
            assert!(matches!(applied, Ok(AgentServiceProgress::Advanced)));
        }
        assert!(guard.llm_sidecar.hosted_service_inflight.is_none());
        assert!(guard.llm_sidecar.hosted_service_phase == phase);
        assert!(guard.llm_sidecar.provider_service_pending == pending);
    }
}

#[test]
fn prepared_completion_releases_consumed_marker_after_phase_change() {
    use crate::viewer::runtime_live::agent_service_result::prepare_result_phase;
    let (server, result) = completion_fixture();
    let shared = std::sync::Mutex::new(server);
    let guard = shared.lock().unwrap();
    let (mut guard, prepared) = prepare_result_phase(&shared, guard, &result).unwrap();
    guard.llm_sidecar.hosted_service_phase = None;
    let pending = guard.llm_sidecar.provider_service_pending.clone();
    assert!(guard.apply_prepared_agent_result(result, prepared).is_err());
    assert!(guard.llm_sidecar.hosted_service_phase.is_none());
    assert!(guard.llm_sidecar.provider_service_pending == pending);
    assert!(
        guard.llm_sidecar.hosted_service_inflight.is_none(),
        "a consumed old completion must not leave a marker that blocks all future preparation"
    );
}

#[test]
fn stale_completion_allows_original_lookup_recovery_without_resubmit() {
    use crate::viewer::runtime_live::agent_service_io::AgentServiceProgress;
    use crate::viewer::runtime_live::agent_service_result::prepare_result_phase;
    let (server, result) = completion_fixture();
    let shared = std::sync::Mutex::new(server);
    let guard = shared.lock().unwrap();
    let (mut guard, prepared) = prepare_result_phase(&shared, guard, &result).unwrap();
    let Some(HostedServicePhase::Act {
        pending,
        submit: false,
    }) = guard.llm_sidecar.hosted_service_phase.clone()
    else {
        panic!("reserved original");
    };
    guard.llm_sidecar.hosted_service_generation += 1;
    assert!(guard.apply_prepared_agent_result(result, prepared).is_err());
    let progress = guard.prepare_agent_service_io_deferred(false).unwrap();
    let AgentServiceProgress::NeedsIdentity(prepared) = progress else {
        panic!("original lookup must be prepared again");
    };
    drop(guard);
    let ready = prepared.compute().unwrap();
    let mut guard = shared.lock().unwrap();
    let job = guard.finish_hosted_identity(ready).unwrap();
    match job.operation {
        AgentServiceIoOperation::Lookup { request, original } => {
            assert_eq!(request.key, pending.correlation.key);
            assert_eq!(original, pending.payload);
        }
        _ => panic!("stale completion must not restore a Submit"),
    }
    assert!(matches!(
        guard.llm_sidecar.hosted_service_phase,
        Some(HostedServicePhase::Act { submit: false, .. })
    ));
}
