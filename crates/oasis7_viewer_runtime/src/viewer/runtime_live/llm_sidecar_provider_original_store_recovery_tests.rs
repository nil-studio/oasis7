//! Receipt fixture tests only local readback consistency and owner fencing;
//! it does not manufacture a remote service authentication capability.
use super::*;
use crate::viewer::runtime_live::agent_service_phase::HostedServicePhase;
use crate::viewer::runtime_live::control_plane::llm_sidecar::{
    RuntimeProviderActionContext, lineage_persistence::PendingProviderServiceIntent,
};
use crate::viewer::runtime_live::control_plane::provider_action_commit::{
    ProviderServiceCognitionReceipt, provider_cognition_commit_inputs,
    validate_provider_service_receipt,
};
use oasis7_client_api::world_service::{CommitRef, ExecutionBinding, WorldIdentity};

fn service_receipt(
    cognition: RuntimeProviderActionContext,
    world: &crate::runtime::World,
) -> (
    PendingProviderServiceIntent,
    ProviderServiceCognitionReceipt,
) {
    let (mut request, response_artifact) =
        provider_cognition_commit_inputs(world, &cognition).unwrap();
    let raw_manifest = "fixture-raw-manifest";
    let raw_parent = "fixture-raw-parent";
    let binding = &mut request.captured_base_binding;
    binding.runtime_manifest_hash = oasis7_agent_api::world_cognition::cognition_digest_v1(
        "oasis7.runtime.manifest.v1",
        &raw_manifest,
    );
    binding.base_world_hash = oasis7_world_state::cognition::world_state_binding_digest_v1(
        &binding.world_id,
        &binding.branch_id,
        binding.finality_epoch,
        binding.finality_block_hash.as_deref(),
        &binding.finality_status,
        binding.base_tick,
        raw_parent,
        binding.reorg_epoch,
        raw_manifest,
    );
    let base = request.captured_base_binding.clone();
    let mut artifact = serde_json::to_value(&response_artifact).unwrap();
    artifact["outer_lineage"] = serde_json::json!({
        "agent_id": request.agent_id, "world_id": base.world_id,
        "branch_id": base.branch_id, "finality_epoch": base.finality_epoch,
        "finality_block_hash": base.finality_block_hash, "finality_status": base.finality_status,
        "base_tick": base.base_tick, "base_world_hash": base.base_world_hash,
        "reorg_epoch": base.reorg_epoch, "runtime_manifest_hash": base.runtime_manifest_hash,
    });
    let digest = oasis7_agent_api::world_cognition::cognition_digest_v1(
        "oasis7.cognition.response.v1",
        &artifact,
    );
    let marker: crate::runtime::WorldCommitRecordV1 = serde_json::from_value(serde_json::json!({
        "schema_version":"world-commit-record.v1", "commit_id":"fixture-commit",
        "envelope_idempotency_key":"fixture-envelope-key", "envelope_digest":"fixture-envelope",
        "world_id":base.world_id,"branch_id":base.branch_id,"finality_epoch":base.finality_epoch,
        "finality_block_hash":base.finality_block_hash,"finality_status":base.finality_status,
        "finality_binding_digest":"fixture-finality","runtime_manifest_hash":raw_manifest,
        "action_id":"action:8","parent_tick":base.base_tick,"parent_world_hash":raw_parent,
        "staged_event_root":"fixture-events","staged_state_root":"fixture-state",
        "receipt_id":"different-receipt","receipt_digest":"fixture-receipt-digest",
        "reorg_epoch":base.reorg_epoch,"cognition_journal_seq":1,"status":"committed",
        "agent_id":request.agent_id,"agent_session_id":request.agent_session_id,
        "agent_turn_id":request.agent_turn_id,"decision_request_id":request.decision_request_id,
        "request_digest":request.request_digest,"feedback_id":"fixture-feedback",
        "response_artifact_digest":digest,
    }))
    .unwrap();
    let lineage =
        crate::runtime::RuntimeReceiptLineageV1::from_durable_commit_record(&marker).unwrap();
    let feedback = crate::simulator::FeedbackEnvelopeV1 {
        feedback_id: marker.feedback_id.clone(),
        feedback_seq: 1,
        agent_subject: request.agent_id.clone(),
        agent_session_id: request.agent_session_id.clone(),
        agent_turn_id: request.agent_turn_id.clone(),
        decision_request_id: request.decision_request_id.clone(),
        candidate_action_id: Some(8),
        runtime_receipt_id: Some(marker.receipt_id.clone()),
        status: "committed".into(),
        request_digest: cognition.request.request_context.request_digest.clone(),
        reject_reason: None,
        provenance: "runtime".into(),
    };
    let intent = crate::world_service::CognitionIntentV1 {
        request,
        response_artifact,
        action: crate::runtime::Action::QueryObservation {
            agent_id: marker.agent_id.clone(),
        },
        delegation_generation: 1,
        causal_proposal: None,
        task_fence: None,
    };
    let signed = crate::world_service::authority::sign_read_request(
        "cognition",
        intent,
        &hex::encode([14; 32]),
    )
    .unwrap();
    let payload = crate::world_service::WorldServicePayloadV1::Cognition(signed);
    let correlation = crate::world_service::derive_correlation(
        WorldIdentity {
            world_id: base.world_id,
            genesis_digest: "0".repeat(64),
        },
        &payload,
    )
    .unwrap();
    let pending = PendingProviderServiceIntent {
        correlation,
        payload,
        cognition,
        action: oasis7_simulator::Action::RemoveModuleVisualEntity {
            entity_id: "fixture".into(),
        },
        feedback_ack: None,
    };
    let receipt = ProviderServiceCognitionReceipt {
        commit_record: marker,
        lineage,
        raw_feedback: serde_json::to_value(&feedback).unwrap(),
        feedback,
    };
    validate_provider_service_receipt(&pending, &receipt)
        .expect("independent full receipt validator");
    (pending, receipt)
}

#[test]
fn hosted_terminal_receipt_fence_precedes_projection_settlement_and_feedback() {
    let root = capture_root();
    let memory = MemoryBudget::new(128 * 1024 * 1024, 8 * 1024 * 1024).unwrap();
    let binding = ProviderMaterialOwnerBindingV1 {
        world_id: "world".into(),
        genesis_root: [0x71; 32],
        owner_id: [0x72; 32],
    };
    let authority = Arc::new(RecoveryFixtureAuthority {
        base: FixtureProviderAuthority {
            world_id: binding.world_id.clone(),
            owner_id: binding.owner_id,
        },
        enabled: std::sync::atomic::AtomicBool::new(true),
    });
    let owner = ProviderOriginalMaterialOwnerV1::create(
        &root,
        binding,
        memory,
        PageCompression::Raw,
        authority,
    )
    .unwrap();
    let mut context = test_provider_context(
        "provider-owner-agent",
        "provider-owner-turn",
        "phase-fence",
        1,
    );
    context.request_context = interruption_tests::direct_capture_request("phase-fence");
    let (request, response, refs) =
        store_captured_attempt(&owner, &context.request_context, "-receipt");
    context.request_context = request;
    context.turn_context.request_digest = context.request_context.request_digest.clone();
    let cognition = RuntimeProviderActionContext {
        request: context,
        response,
        cognition_lease: None,
        memory_write_intents: Vec::new(),
        provider_original_material_refs: Some(refs.clone()),
        provider_response_context_lease: None,
    };
    let mut server = ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(
        oasis7_simulator::WorldScenario::Minimal,
    ))
    .unwrap();
    let (pending, receipt) = service_receipt(cognition, &server.world);
    // B is independently accepted by this authority, but this attempt has
    // already durably committed A. Finalization must reject B before effects.
    owner
        .verify_commit_disposition(&refs, "different-receipt", 8)
        .unwrap();
    owner
        .mark_committed(&refs, "fixture-runtime-receipt", 7)
        .unwrap();
    server
        .llm_sidecar
        .install_provider_original_material_store(owner.clone())
        .unwrap();
    let mut matching = receipt.clone();
    matching.commit_record.receipt_id = "fixture-runtime-receipt".into();
    matching.commit_record.action_id = "action:7".into();
    matching.lineage.receipt_id = matching.commit_record.receipt_id.clone();
    matching.lineage.action_id = matching.commit_record.action_id.clone();
    matching.feedback.runtime_receipt_id = Some(matching.commit_record.receipt_id.clone());
    validate_provider_service_receipt(&pending, &matching).unwrap();
    server
        .llm_sidecar
        .verify_hosted_original_disposition(&pending, &matching)
        .expect("the original matching receipt remains eligible for finalization");
    server.config.world_service = Some(crate::world_service::client::WorldServiceClientConfig {
        endpoint: "http://127.0.0.1:1".into(),
        trusted_service_public_key: hex::encode([12; 32]),
        expected_world: pending.correlation.key.world.clone(),
        scope_id: "public".into(),
        read_private_key_hex: hex::encode([13; 32]),
        timeout: Duration::from_millis(10),
        formal_read_trust: None,
        max_response_bytes: 4096,
    });
    server.config.world_service_agent_signer = Some(
        crate::world_service::client::WorldServiceAgentSignerConfig {
            private_key_hex: hex::encode([14; 32]),
            delegation_generation: 1,
        },
    );
    let commit = CommitRef {
        world: pending.correlation.key.world.clone(),
        binding: ExecutionBinding {
            provider_world_id: "world".into(),
            branch_id: "fixture".into(),
            finality_ref: "fixture".into(),
            reorg_generation: 0,
            governing_manifest_ref: "fixture".into(),
            authority_generation: 0,
            permission_generation: 0,
        },
        position: 1,
        execution_block_hash: "fixture".into(),
        state_root_ref: "fixture".into(),
    };
    let phases = [
        HostedServicePhase::ActView {
            pending: pending.clone(),
            receipt: receipt.clone(),
            commit: commit.clone(),
            settled: false,
        },
        HostedServicePhase::ActSettle {
            pending: pending.clone(),
            receipt: receipt.clone(),
            submit: true,
            replay_authorized: false,
        },
        HostedServicePhase::Feedback {
            pending: pending.clone(),
            receipt: receipt.clone(),
        },
        HostedServicePhase::FeedbackAck {
            pending: pending.clone(),
            receipt: receipt.clone(),
            submit: true,
        },
        HostedServicePhase::FeedbackAckView {
            pending: pending.clone(),
            receipt: receipt.clone(),
            commit,
        },
        HostedServicePhase::Finalize {
            pending,
            receipt,
            feedback_acked: true,
        },
    ];
    let before_world = serde_json::to_value(server.world.snapshot()).unwrap();
    let before_memory = serde_json::to_value(server.llm_sidecar.provider_memory_store()).unwrap();
    let before_root = owner.pending_protection_root().unwrap();
    for phase in phases {
        server.llm_sidecar.hosted_service_phase = Some(phase.clone());
        let error = server
            .prepare_hosted_service_io(false)
            .err()
            .expect("B must be fenced before I/O or mutation");
        assert!(error.contains("finalization receipt differs"), "{error}");
        assert!(server.llm_sidecar.hosted_service_phase.as_ref() == Some(&phase));
        assert!(server.llm_sidecar.hosted_service_inflight.is_none());
        assert_eq!(
            serde_json::to_value(server.world.snapshot()).unwrap(),
            before_world
        );
        assert_eq!(
            serde_json::to_value(server.llm_sidecar.provider_memory_store()).unwrap(),
            before_memory
        );
        assert_eq!(owner.pending_protection_root().unwrap(), before_root);
    }
    drop(server);
    drop(owner);
    std::fs::remove_dir_all(root).unwrap();
}
