//! Retry admission checks the original signed bytes before any network operation.
use super::*;
use crate::world_service::{authority::sign_read_request, wire::*};
use oasis7_client_api::world_service::WorldIdentity;
fn checkpoint() -> (
    WorldIdentity,
    lineage_persistence::PendingProviderSchedulerIntent,
) {
    let world = WorldIdentity {
        world_id: "world".into(),
        genesis_digest: "0".repeat(64),
    };
    let payload = WorldServicePayloadV1::Scheduler(
        sign_read_request(
            "scheduler",
            SchedulerIntentV1 {
                agent_id: "agent-1".into(),
                request_id: "original-reject-predecessor".into(),
                delegation_generation: 7,
                captured_base_binding: crate::runtime::RuntimeCognitionBaseBindingV1 {
                    world_id: "world".into(),
                    branch_id: "main".into(),
                    finality_epoch: 0,
                    finality_block_hash: None,
                    finality_status: "pending".into(),
                    base_tick: 12,
                    base_world_hash: "0".repeat(64),
                    reorg_epoch: 0,
                    runtime_manifest_hash: "0".repeat(64),
                },
                operation: SchedulerOperationV1::TransitionContinuation {
                    continuation_id: "predecessor-1".into(),
                    to: crate::runtime::ContinuationStatusV1::Rejected,
                    logical_tick: 12,
                },
            },
            &"11".repeat(32),
        )
        .unwrap(),
    );
    let correlation = crate::world_service::derive_correlation(world.clone(), &payload).unwrap();
    (
        world,
        lineage_persistence::PendingProviderSchedulerIntent {
            resume_context: None,
            resume_current_context: None,
            correlation,
            payload,
        },
    )
}
#[test]
fn scheduler_retry_accepts_exact_signed_checkpoint_without_replacing_bytes() {
    let (world, pending) = checkpoint();
    let before = serde_json::to_vec(&pending).unwrap();
    RuntimeLlmSidecar::validate_scheduler_checkpoint_integrity(
        &world,
        "original-reject-predecessor",
        &pending,
    )
    .unwrap();
    assert_eq!(serde_json::to_vec(&pending).unwrap(), before);
}
#[test]
fn scheduler_retry_fences_signature_tick_correlation_and_world_tampering() {
    for case in 0..5 {
        let (mut world, mut pending) = checkpoint();
        match case {
            0 => {
                let WorldServicePayloadV1::Scheduler(signed) = &mut pending.payload else {
                    unreachable!()
                };
                signed.signature_hex = "00".repeat(64);
            }
            1 => {
                let WorldServicePayloadV1::Scheduler(signed) = &mut pending.payload else {
                    unreachable!()
                };
                let SchedulerOperationV1::TransitionContinuation { logical_tick, .. } =
                    &mut signed.request.operation
                else {
                    unreachable!()
                };
                *logical_tick += 1;
            }
            2 => {
                pending.correlation.key.world.genesis_digest = "1".repeat(64);
            }
            3 => {
                world.world_id = "other-world".into();
            }
            _ => {
                let WorldServicePayloadV1::Scheduler(signed) = &pending.payload else {
                    unreachable!()
                };
                let mut request = signed.request.clone();
                request.request_id = "different-valid-signed-request".into();
                pending.payload = WorldServicePayloadV1::Scheduler(
                    sign_read_request("scheduler", request, &"11".repeat(32)).unwrap(),
                );
                pending.correlation =
                    crate::world_service::derive_correlation(world.clone(), &pending.payload)
                        .unwrap();
            }
        }
        let before = serde_json::to_vec(&pending).unwrap();
        assert!(
            RuntimeLlmSidecar::validate_scheduler_checkpoint_integrity(
                &world,
                "original-reject-predecessor",
                &pending
            )
            .is_err(),
            "case {case}"
        );
        assert_eq!(serde_json::to_vec(&pending).unwrap(), before, "case {case}");
    }
}

#[test]
fn new_prefix_checkpoint_binds_verified_post_reserve_view_and_replays_original() {
    let context = super::super::tests::test_provider_context("agent-a", "turn-a", "request-a", 1);
    let mut context = context;
    let request = &mut context.request_context;
    request.observation_digest = crate::simulator::h_v1(
        "oasis7.cognition.test.observation.v1",
        &request.base_decision_request.observation,
    );
    request.capability_catalog_digest =
        crate::simulator::h_v1("oasis7.cognition.test.catalog.v1", &Option::<()>::None);
    request.capability_invocation_context_digest =
        crate::simulator::h_v1("oasis7.cognition.test.invocation.v1", &Option::<()>::None);
    request.memory_snapshot_digest = crate::simulator::h_v1(
        "oasis7.cognition.test.memory.v1",
        &context.turn_context.memory_snapshot,
    );
    request.goal_snapshot_digest = crate::simulator::h_v1(
        "oasis7.cognition.test.goal.v1",
        &context.turn_context.goal_snapshot,
    );
    request.continuation_digest =
        crate::simulator::h_v1("oasis7.cognition.test.continuation.v1", &Option::<()>::None);
    request.runtime_binding.base_world_hash =
        crate::simulator::h_v1("oasis7.cognition.test.world.v1", &"checkpoint");
    request.runtime_binding.runtime_manifest_hash =
        crate::simulator::h_v1("oasis7.cognition.test.manifest.v1", &"checkpoint");
    request.request_digest = request.request_digest();
    context.turn_context.request_digest = request.request_digest.clone();
    let request = context.request_context;
    let mut current = request.runtime_binding.clone();
    current.base_tick += 1;
    current.base_world_hash = crate::simulator::h_v1("post-reserve-world", &1);
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    let mut projection = crate::world_service::projection::WorldServiceProjection::from_world(
        &crate::runtime_host::world::new(),
        None,
    )
    .unwrap();
    projection.runtime_binding = Some(current.clone());
    sidecar.provider_service_projection = Some(projection);
    sidecar.provider_service_config =
        Some(crate::world_service::client::WorldServiceClientConfig {
            formal_read_trust: None,
            endpoint: "http://127.0.0.1:1".into(),
            trusted_service_public_key: "11".repeat(32),
            expected_world: WorldIdentity {
                world_id: current.world_id.clone(),
                genesis_digest: "0".repeat(64),
            },
            scope_id: "agent:agent-a".into(),
            read_private_key_hex: "11".repeat(32),
            timeout: std::time::Duration::from_secs(2),
            max_response_bytes: 4096,
        });
    sidecar.provider_service_signer = Some(
        crate::world_service::client::WorldServiceAgentSignerConfig {
            private_key_hex: "11".repeat(32),
            delegation_generation: 1,
        },
    );
    let path = std::env::temp_dir().join(format!(
        "pre2-prefix-checkpoint-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    sidecar.provider_lineage_store = Some(path.clone());
    sidecar.provider_service_lineage_store_explicit = true;
    sidecar.provider_service_required = true;
    let operation = SchedulerOperationV1::ProviderPrefix {
        request: crate::simulator::agent_api_compat::request_context_to_api(&request),
        context_digest: async_support::runtime_provider_context_digest(&request),
    };
    let (original, existed) = sidecar
        .prepare_service_scheduler_checkpoint(&request, "prefix:1", operation.clone(), None)
        .unwrap();
    assert!(!existed);
    let WorldServicePayloadV1::Scheduler(signed) = &original.payload else {
        unreachable!()
    };
    assert_eq!(
        signed.request.captured_base_binding.base_tick,
        current.base_tick
    );
    assert_eq!(
        signed.request.captured_base_binding.base_world_hash,
        current.base_world_hash.to_string()
    );
    assert_eq!(signed.request.operation, operation);
    let bytes = serde_json::to_vec(&original).unwrap();
    sidecar
        .provider_service_projection
        .as_mut()
        .unwrap()
        .runtime_binding
        .as_mut()
        .unwrap()
        .base_tick += 1;
    let (replayed, existed) = sidecar
        .prepare_service_scheduler_checkpoint(&request, "prefix:1", operation, None)
        .unwrap();
    assert!(existed);
    assert_eq!(serde_json::to_vec(&replayed).unwrap(), bytes);
    std::fs::remove_file(path).unwrap();
}
