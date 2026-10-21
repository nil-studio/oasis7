use super::*;
use oasis7_agent_api::game::{GameTaskControlV1, TaskContextIdentityV1};

fn context() -> TaskContextIdentityV1 {
    let public = authority::sign_read_request("key", (), &hex::encode([7; 32]))
        .unwrap()
        .subject_public_key;
    let control: GameTaskControlV1 = serde_json::from_value(json!({"schema_revision":1,"task_id":"task","world_id":"world","genesis_digest":"genesis","branch_id":"main","reorg_epoch":"0","agent_id":"a","owner_binding":"owner","owner_generation":"1","owner_public_key":public,"task_revision":"3","task_status":"active","latest_goal_revision":"1","active_goal_revision":"1","active_goal_reference":{"goal_revision":"1","goal_ref":"cd".repeat(32),"prepare_operation_id":"goal","publish_operation_id":"publish","activation_committed_height":"2"},"executor_epoch":"1","executor_id":"executor","executor_binding":{"executor_id":"executor","executor_public_key":public,"delegation_generation":"1","executor_epoch":"1"},"created_operation_id":"create"})).unwrap();
    serde_json::from_value(json!({
        "schema_revision":1,"fence":GameTaskFenceV1::from_control(&control),
        "agent_session_id":"session","agent_turn_id":"turn","decision_request_id":"request",
        "retry_seq":1,"transport_attempt":1,"request_commitment":format!("blake3:{}","11".repeat(32)),
        "context_commitment":format!("blake3:{}","22".repeat(32)),"provider_invocation_commitment":format!("blake3:{}","33".repeat(32)),
        "valid_until_tick":"16"})).unwrap()
}
fn fixture() -> (Client, PathBuf, WorldServicePayloadV1) {
    let mut nonce = [0; 16];
    getrandom::fill(&mut nonce).unwrap();
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("oasis7-outcome-{}", hex::encode(nonce)));
    let config = crate::tests::config("http://127.0.0.1:1".into());
    let client = Client {
        world: config.expected_world.clone(),
        agent: "a".into(),
        public_key: authority::sign_read_request("key", (), &hex::encode([7; 32]))
            .unwrap()
            .subject_public_key,
        key_hex: hex::encode([7; 32]).into(),
        #[cfg(unix)]
        gateway: None,
        remote: crate::WorldServiceTaskClient::new(config).unwrap(),
        state: crate::operation_store::OperationStore::open(&root, [17; 32]).unwrap(),
    };
    let base = serde_json::from_value(json!({"world_id":"world","branch_id":"main",
        "finality_epoch":0,"finality_block_hash":null,"finality_status":"pending",
        "base_tick":0,"base_world_hash":format!("blake3:{}","1".repeat(64)),
        "reorg_epoch":0,"runtime_manifest_hash":format!("blake3:{}","2".repeat(64))}))
    .unwrap();
    let payload = WorldServicePayloadV1::Scheduler(
        authority::sign_read_request(
            "scheduler",
            SchedulerIntentV1 {
                agent_id: "a".into(),
                request_id: "request".into(),
                delegation_generation: 1,
                captured_base_binding: base,
                operation: SchedulerOperationV1::TaskProviderPrefix { context: context() },
            },
            &client.key_hex,
        )
        .unwrap(),
    );
    (client, root, payload)
}
#[test]
fn expired_retirement_requires_strict_expiry_and_current_complete_active_fence() {
    let context = context();
    context.validate().unwrap();
    assert!(validate_expired_retirement(&context, Some(&context.fence), 17).is_ok());
    for tick in [0, 15, 16] {
        assert!(validate_expired_retirement(&context, Some(&context.fence), tick).is_err());
    }
    assert!(validate_expired_retirement(&context, None, 17).is_err());
    for field in [
        "task_revision",
        "executor_epoch",
        "active_goal_revision",
        "task_status",
        "owner_generation",
    ] {
        let mut value = serde_json::to_value(&context.fence).unwrap();
        value[field] = if field == "task_status" {
            json!("suspended")
        } else {
            json!("99")
        };
        let changed: GameTaskFenceV1 = serde_json::from_value(value).unwrap();
        assert!(
            validate_expired_retirement(&context, Some(&changed), 17).is_err(),
            "{field}"
        );
    }
    let mut stopped = context.clone();
    stopped.fence.task_status = oasis7_agent_api::game::GameTaskStatusV1::Suspended;
    assert!(validate_expired_retirement(&stopped, Some(&stopped.fence), 17).is_err());
}
#[test]
fn unresolved_or_rejected_original_prefix_cannot_create_retirement_outcome() {
    for outcome in [
        IntentOutcome::Unknown,
        IntentOutcome::Pending,
        IntentOutcome::HistoryUnavailable,
        IntentOutcome::Expired,
        IntentOutcome::Rejected {
            reason: WorldServiceErrorKind::Conflict,
        },
    ] {
        let (mut client, root, payload) = fixture();
        let original = client
            .cache_intent(ClientStateKind::Prefix, "ctx", payload)
            .unwrap();
        let response = IntentResponse::<Value> {
            contract_version: 1,
            formal_profile: None,
            execution_evidence_scope: None,
            correlation: original.correlation,
            outcome,
        };
        let (endpoint, server) = crate::tests::serve_payload(
            LOOKUP_PATH,
            serde_json::to_value(response).unwrap(),
            |_| {},
        );
        client.remote = crate::WorldServiceTaskClient::new(crate::tests::config(endpoint)).unwrap();
        assert!(
            client
                .outcome("ctx", "context_expired")
                .unwrap_err()
                .contains("prefix is not committed")
        );
        assert!(
            client
                .cached_intent(ClientStateKind::Outcome, "ctx")
                .unwrap()
                .is_none()
        );
        server.join().unwrap();
        drop(client);
        std::fs::remove_dir_all(root).unwrap();
    }
}
#[test]
fn competing_original_action_blocks_retirement_without_network_or_new_outcome() {
    let (client, root, payload) = fixture();
    let WorldServicePayloadV1::Scheduler(prefix) = payload else {
        unreachable!()
    };
    let context = context();
    let request = oasis7_agent_api::world_cognition::RuntimeCognitionCommitRequestV1 {
        agent_id: "a".into(),
        agent_session_id: context.agent_session_id,
        agent_turn_id: context.agent_turn_id,
        decision_request_id: context.decision_request_id,
        retry_seq: 1,
        transport_attempt: 1,
        request_digest: context.request_commitment,
        observation_digest: format!("blake3:{}", "44".repeat(32)),
        context_digest: context.context_commitment,
        capability_snapshot_hash: format!("blake3:{}", "55".repeat(32)),
        authority_context_hash: format!("blake3:{}", "66".repeat(32)),
        captured_base_binding: prefix.request.captured_base_binding,
    };
    let mut artifact = oasis7_agent_api::world_cognition::RuntimeCognitionResponseArtifactV1 {
        schema_version: 1,
        context_discriminator: oasis7_agent_api::world_cognition::RuntimeCognitionResponseArtifactV1::CONTEXT_DISCRIMINATOR.into(),
        context_version: 1,
        agent_session_id: request.agent_session_id.clone(),
        agent_turn_id: request.agent_turn_id.clone(),
        decision_request_id: request.decision_request_id.clone(),
        retry_seq: 1, transport_attempt: 1,
        request_digest: request.request_digest.clone(),
        response_digest: format!("blake3:{}", "77".repeat(32)),
        artifact_digest: String::new(),
    };
    artifact.refresh_artifact_digest();
    let intent = oasis7_client_api::world_service_wire::CognitionIntentV1 {
        request,
        action: oasis7_agent_api::world_action::Action::MoveAgent {
            agent_id: "a".into(),
            to: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
        },
        response_artifact: artifact,
        delegation_generation: 1,
        causal_proposal: None,
        task_fence: Some(context.fence),
    };
    let payload = WorldServicePayloadV1::Cognition(
        authority::sign_read_request("cognition", intent, &client.key_hex).unwrap(),
    );
    let original = client
        .cache_intent(ClientStateKind::Action, "ctx", payload)
        .unwrap();
    assert!(
        client
            .outcome("ctx", "context_expired")
            .unwrap_err()
            .contains("original action")
    );
    assert!(
        client
            .cached_intent(ClientStateKind::Outcome, "ctx")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        serde_json::to_value(
            client
                .cached_intent(ClientStateKind::Action, "ctx")
                .unwrap()
                .unwrap()
        )
        .unwrap(),
        serde_json::to_value(original).unwrap()
    );
    drop(client);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn existing_outcome_cannot_be_replaced_with_a_different_reason() {
    let (client, root, payload) = fixture();
    let WorldServicePayloadV1::Scheduler(prefix) = payload else {
        unreachable!()
    };
    let mut base = prefix.request.captured_base_binding;
    base.base_tick = 17;
    let intent = public_outcome_intent(&context(), base, "context_expired").unwrap();
    let original = client
        .cache_intent(
            ClientStateKind::Outcome,
            "ctx",
            WorldServicePayloadV1::Scheduler(
                authority::sign_read_request("scheduler", intent, &client.key_hex).unwrap(),
            ),
        )
        .unwrap();
    assert!(
        client
            .outcome("ctx", "provider_failed")
            .unwrap_err()
            .contains("different original outcome")
    );
    assert_eq!(
        client
            .cached_intent(ClientStateKind::Outcome, "ctx")
            .unwrap()
            .unwrap(),
        original
    );
    assert!(
        client
            .cached_intent(ClientStateKind::Prefix, "ctx")
            .unwrap()
            .is_none()
    );
    drop(client);
    std::fs::remove_dir_all(root).unwrap();
}
