//! Account signatures here are explicit component fixtures, never CF payment.
use super::*;
use oasis7_client_api::world_service::{
    entry_executor::EntryExecutorAccessV1,
    task_context::{TASK_CONTEXT_PREPARE_PATH, TaskContextPrepareV1},
    task_context_projection::TaskContextPrepareAttestationRequestV2,
};
use oasis7_custody_core::provisioning::ExecutionPrepareSignedOriginalV2;

pub(super) fn request(ready: &ReadyNativeRegistryV2) -> NativeExecutionPrepareRequestV2 {
    let pending = &ready.pending.certificate;
    let CustodyIntentV1::TestSessionV2 { plan, scope } = &pending.input.request.intent else {
        unreachable!()
    };
    let fence = plan.termination.initial_fence.clone();
    let executor = hex::encode([63; 32]);
    assert_eq!(
        fence.executor_binding.as_ref().unwrap().executor_public_key,
        ws::sign_read_request("fixture-key", (), &executor)
            .unwrap()
            .subject_public_key
    );
    let now = now_ms();
    let expires = (now + 4000)
        .min(plan.write_deadline.0)
        .min(pending.input.approval.proof.expires_at.0);
    let read: ws::ReadWorldViewRequest = serde_json::from_value(json!({
        "contract_version":1, "world":{"world_id":fence.world_id,"genesis_digest":fence.genesis_digest},
        "scope_id":format!("agent:{}",fence.agent_id),"min_commit":null,"fixed_commit":null,
        "deadline_unix_ms":expires,"observation_nonce":"e1".repeat(32)
    })).unwrap();
    read.validate().unwrap();
    let prepare = TaskContextPrepareV1 {
        authority_mode: Some(oasis7_agent_api::game::TaskContextAuthorityModeV1::NativeExecutorV1),
        schema_revision: 2,
        fence: fence.clone(),
        client_operation_id: "native-tls-component-prepare".into(),
        client_nonce: "e2".repeat(32),
    };
    let original = ExecutionPrepareSignedOriginalV2 {
        access: EntryExecutorAccessV1 {
            fence,
            read: ws::sign_read_request(ws::VIEW_JSON_PATH, read, &executor).unwrap(),
        },
        request: TaskContextPrepareAttestationRequestV2 {
            original: ws::sign_read_request(TASK_CONTEXT_PREPARE_PATH, prepare, &executor).unwrap(),
            observation_nonce: "e3".repeat(32),
        },
    };
    ws::task_context_projection::validate_prepare_attestation_request_v2(&original.request)
        .unwrap();
    let edges = [
        Edge::AccountToIdentityService,
        Edge::IdentityServiceToNativeActor,
        Edge::SidCipherReadBefore,
        Edge::SidCipherReadAfter,
        Edge::CsrfCipherReadBefore,
        Edge::CsrfCipherReadAfter,
        Edge::IdentityActorToNative,
        Edge::NativeToWorldCurrent,
        Edge::NativeToWorldPrepare,
        Edge::IdentityActorToContextBody,
    ];
    let certificate = AccountNativeExecutionPrepareV2 {
        api_version: 2,
        issuer: pending.issuer.clone(),
        session_id: ready.active.certificate.session_id.clone(),
        approved_input_digest: pending.approved_input_digest.clone(),
        session_intent_digest: pending.intent_digest.clone(),
        execution_deadline: DecimalU64(plan.write_deadline.0),
        action_request_id: "native-tls-component-action".into(),
        stage_operation_id: original
            .request
            .original
            .request
            .client_operation_id
            .clone(),
        source_operation_id: "native-tls-component-source".into(),
        source_nonce: "e4".repeat(32),
        source_record_digest: bytes_digest(b"explicit fixture Source original, not CF charge"),
        source_observed_at: DecimalU64(now),
        source_expires_at: DecimalU64(expires),
        credential_generation: DecimalU64(1),
        accepted_ordinal: DecimalU64(100),
        factory_record_key: format!("explicit-component-fixture:{}", "a".repeat(256)),
        factory_ports_digest: bytes_digest(b"fixture factory metadata, not paid CF prefix"),
        private_current_proof_digest: bytes_digest(
            b"fixture private Current commitment, not actual Identity actor",
        ),
        opaque_registration_ref: ready.active.certificate.opaque_registration_ref.clone(),
        active_certificate_digest: ready.active.digest().unwrap(),
        native_binding: pending.native_lifetime.lifetime.status.receipt.clone(),
        native_session_ref: pending.participant_session.session_ref.clone(),
        private_sid_generation: pending.participant_session.private_sid_generation,
        native_binding_generation: pending.participant_session.native_binding_generation,
        session: pending.participant_session.clone(),
        world: scope.world_identity.clone(),
        original_digest: original.commitment_digest().unwrap(),
        issued_at: DecimalU64(now),
        expires_at: DecimalU64(expires),
        ports: std::array::from_fn(|i| ExecutionPreparePhysicalPortV2 {
            edge: edges[i],
            attempt_id: bytes_digest(format!("native-tls-physical-{i}").as_bytes()),
        }),
    };
    certificate.validate_shape().unwrap();
    let proof = SignedAccountNativeExecutionPrepareV2 {
        signature_hex: hex::encode(
            SigningKey::from_bytes(&[81; 32])
                .sign(&certificate.signing_bytes().unwrap())
                .to_bytes(),
        ),
        certificate,
    };
    let request = NativeExecutionPrepareRequestV2 {
        proof,
        original: Box::new(original),
    };
    request.wire_bytes().unwrap();
    request
}

pub(super) fn claim(
    request: &NativeExecutionPrepareRequestV2,
    edge: Edge,
    body: &[u8],
    ordinal: u64,
) -> SignedExecutionDispatchClaimV2 {
    let c = &request.proof.certificate;
    let claim = ExecutionDispatchClaimV2 {
        api_version: 2,
        issuer: c.issuer.clone(),
        native_proof_digest: request.proof.digest().unwrap(),
        reservation_ordinal: DecimalU64(c.accepted_ordinal.0 + 1),
        claim_ordinal: DecimalU64(ordinal),
        session_id: c.session_id.clone(),
        approved_input_digest: c.approved_input_digest.clone(),
        source_record_digest: c.source_record_digest.clone(),
        leaf: c.ports.iter().find(|p| p.edge == edge).unwrap().clone(),
        body_digest: bytes_digest(body),
        callbacks: execution_reentry_inventory(request)
            .unwrap()
            .into_iter()
            .filter(|p| p.edge == edge)
            .map(|p| p.port)
            .collect(),
        issued_at: DecimalU64(now_ms()),
        expires_at: c.expires_at,
    };
    SignedExecutionDispatchClaimV2 {
        signature_hex: hex::encode(
            SigningKey::from_bytes(&[81; 32])
                .sign(&claim.signing_bytes(request).unwrap())
                .to_bytes(),
        ),
        claim,
    }
}
