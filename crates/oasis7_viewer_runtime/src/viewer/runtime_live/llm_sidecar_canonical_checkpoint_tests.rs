use super::*;

#[test]
fn canonical_scheduler_checkpoint_retains_signed_request_and_rejects_tampering() {
    use crate::world_service::{authority::sign_read_request, wire::*};
    use oasis7_client_api::world_service::WorldIdentity;
    let path = std::env::temp_dir().join(format!(
        "oasis7-canonical-scheduler-checkpoint-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let request_id = "original-release";
    let payload = WorldServicePayloadV1::Scheduler(
        sign_read_request(
            "scheduler",
            SchedulerIntentV1 {
                agent_id: "agent-1".into(),
                request_id: request_id.into(),
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
                operation: SchedulerOperationV1::ReleaseLease {
                    lease_id: "lease-original".into(),
                },
            },
            &"11".repeat(32),
        )
        .expect("sign original scheduler request"),
    );
    let world_identity = WorldIdentity {
        world_id: "world".into(),
        genesis_digest: "0".repeat(64),
    };
    let correlation =
        crate::world_service::derive_correlation(world_identity, &payload).expect("correlation");
    let mut sidecar = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    sidecar.configure_provider_lineage_store(path.clone());
    sidecar.provider_scheduler_pending.insert(
        request_id.into(),
        lineage_persistence::PendingProviderSchedulerIntent {
            resume_context: None,
            resume_current_context: None,
            correlation: correlation.clone(),
            payload: payload.clone(),
        },
    );
    sidecar
        .persist_provider_lineage()
        .expect("persist original signature");
    let mut restored = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    restored.configure_provider_lineage_store(path.clone());
    restored
        .restore_provider_lineage(&RuntimeWorld::default())
        .expect("restore original scheduler request");
    let actual = &restored.provider_scheduler_pending[request_id];
    assert_eq!(actual.correlation, correlation);
    assert_eq!(
        serde_json::to_value(&actual.payload).unwrap(),
        serde_json::to_value(&payload).unwrap()
    );
    let mut compatible = serde_json::to_value(actual).unwrap();
    compatible.as_object_mut().unwrap().remove("resume_context");
    compatible
        .as_object_mut()
        .unwrap()
        .remove("resume_current_context");
    let compatible: lineage_persistence::PendingProviderSchedulerIntent =
        serde_json::from_value(compatible).expect("legacy non-resume checkpoint defaults");
    assert!(compatible.resume_context.is_none());
    assert!(compatible.resume_current_context.is_none());
    let original_context = test_provider_context("agent-1", "original-turn", "original-request", 1);
    restored
        .provider_scheduler_pending
        .get_mut(request_id)
        .unwrap()
        .resume_context = Some(original_context.clone());
    restored
        .persist_provider_lineage()
        .expect("persist complete prepared context");
    let mut checkpoint = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    checkpoint.configure_provider_lineage_store(path.clone());
    checkpoint
        .restore_provider_lineage(&RuntimeWorld::default())
        .expect("restore complete context checkpoint");
    assert_eq!(
        serde_json::to_value(
            checkpoint.provider_scheduler_pending[request_id]
                .resume_context
                .as_ref()
                .unwrap()
        )
        .unwrap(),
        serde_json::to_value(&original_context).unwrap(),
    );
    // Mutate the actual signed request independent of enum serialization layout.
    let pending = restored
        .provider_scheduler_pending
        .get_mut(request_id)
        .unwrap();
    if let WorldServicePayloadV1::Scheduler(signed) = &mut pending.payload {
        signed.request.delegation_generation += 1;
    }
    restored
        .persist_provider_lineage()
        .expect("write tampered checkpoint fixture");
    let mut tampered = RuntimeLlmSidecar::new(ViewerLiveDecisionMode::Llm);
    tampered.configure_provider_lineage_store(path.clone());
    assert!(
        tampered
            .restore_provider_lineage(&RuntimeWorld::default())
            .is_err()
    );
    assert!(tampered.provider_scheduler_pending.is_empty());
    let _ = std::fs::remove_file(path);
}
