use super::*;
use oasis7_agent_api::game::{TaskContextAuthorityModeV1, TaskContextIdentityV1};
use oasis7_client_api::world_service_wire::{CanonicalIntentResultV1, WorldServicePayloadV1};

pub(super) fn exec_source(
    f: &mut SessionFixture,
    id: &str,
    nonce: &str,
    now: u64,
) -> SignedManagedNativeSessionLifetimeV1 {
    funded_source(
        f,
        SessionSourceFundingV1::Execution,
        SessionPurposeV1::Execution,
        id,
        &nonce.repeat(32),
        now,
    )
}
pub(super) fn live<'a, 'b>(
    p: &'a ProvisionApprovalContext<'b>,
    ns: &'a NativeStageScope<'b>,
    life: &'a SignedManagedNativeSessionLifetimeV1,
    source: &'a str,
    now: u64,
) -> TestSessionLiveCurrent<'a, 'b> {
    TestSessionLiveCurrent {
        provision: p,
        native_scope: ns,
        lifetime: life,
        private_sid_generation: DecimalU64(1),
        source_ticket: 2,
        source_operation_id: source,
        observed_at: UnixMillis::new(now),
    }
}
pub(super) fn prepared_artifact(
    f: &SessionFixture,
) -> (
    ws::SignedReadRequest<TaskContextPrepareV1>,
    ws::SignedServiceResponse<PreparedTaskContextV1>,
) {
    let s = &f.state.test_sessions()[ID];
    let OwnerSetupResultV1::FinalObservation { owner, .. } = f
        .state
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::FinalObservation)
        .unwrap()
        .result
        .as_ref()
        .unwrap()
    else {
        unreachable!()
    };
    let OwnerSetupResultV1::PreparedGoal(goal) = f
        .state
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::PrepareGoal)
        .unwrap()
        .result
        .as_ref()
        .unwrap()
    else {
        unreachable!()
    };
    let OwnerSetupResultV1::Canonical {
        response: published,
        ..
    } = f
        .state
        .owner_setup_outbox("setup-one", OwnerSetupStageV1::PublishGoal)
        .unwrap()
        .result
        .as_ref()
        .unwrap()
    else {
        unreachable!()
    };
    let ws::IntentOutcome::Committed {
        commit: activation, ..
    } = &published.payload.outcome
    else {
        unreachable!()
    };
    // Complete unsigned snapshot is assembled from actual approved fixture facts
    // before the Native service signs it; no approved Account field is changed.
    let mut snapshot: GoalExecutionSnapshotV1 = serde_json::from_str(include_str!(
        "../../../oasis7_game_client/testdata/task-context/native-snapshot.json"
    ))
    .unwrap();
    snapshot.control = owner
        .payload
        .control
        .view
        .game_task_control
        .clone()
        .unwrap();
    snapshot.active_goal = goal.payload.clone();
    snapshot.read_commit = owner.payload.control.version.commit.clone();
    snapshot.activation_commit = (**activation).clone();
    let fence = &s.termination.initial_fence;
    assert_eq!(
        oasis7_agent_api::game::GameTaskFenceV1::from_control(&snapshot.control),
        *fence
    );
    snapshot.runtime_binding.world_id = fence.world_id.clone();
    snapshot.runtime_binding.branch_id = fence.branch_id.clone();
    snapshot.runtime_binding.reorg_epoch = fence.reorg_epoch.0;
    snapshot.captured_base_binding.world_id = fence.world_id.clone();
    snapshot.captured_base_binding.branch_id = fence.branch_id.clone();
    snapshot.captured_base_binding.reorg_epoch = fence.reorg_epoch.0;
    let native = snapshot.native_authority.as_mut().unwrap();
    native.fence = fence.clone();
    native.signer_delegation = s.termination.original_delegation.clone();
    snapshot.validate().unwrap();
    let original = ws::sign_read_request(
        TASK_CONTEXT_PREPARE_PATH,
        TaskContextPrepareV1 {
            authority_mode: Some(TaskContextAuthorityModeV1::NativeExecutorV1),
            schema_revision: 2,
            fence: fence.clone(),
            client_operation_id: "context-operation-one".into(),
            client_nonce: "e1".repeat(32),
        },
        &"41".repeat(32),
    )
    .unwrap();
    verify_prepare(&original).unwrap();
    assert_eq!(
        original.subject_public_key,
        fence.executor_binding.as_ref().unwrap().executor_public_key
    );
    let until = context_deadline(
        snapshot.captured_base_binding.base_tick,
        snapshot.authority_valid_until_tick().unwrap(),
    )
    .unwrap();
    let mut prepared = PreparedTaskContextV1 {
        schema_revision: 2,
        prepare_proof: original.clone(),
        snapshot,
        reference_salt: "e2".repeat(32),
        valid_until_tick: until,
        identity: TaskContextIdentityV1 {
            authority_mode: Some(TaskContextAuthorityModeV1::NativeExecutorV1),
            native_authorization_root: None,
            schema_revision: 2,
            fence: fence.clone(),
            valid_until_tick: until,
            agent_session_id: "context-private-session".into(),
            agent_turn_id: "context-private-turn".into(),
            decision_request_id: "context-private-decision".into(),
            retry_seq: 1,
            transport_attempt: 1,
            request_commitment: format!("blake3:{}", "0".repeat(64)),
            context_commitment: format!("blake3:{}", "0".repeat(64)),
            provider_invocation_commitment: format!("blake3:{}", "0".repeat(64)),
        },
    };
    prepared.identity.native_authorization_root = Some(
        prepared
            .snapshot
            .native_authority
            .as_ref()
            .unwrap()
            .native_authorization_root
            .clone(),
    );
    prepared.identity.request_commitment = prepared
        .commitment("oasis7/private-task-request/v1")
        .unwrap();
    prepared.identity.context_commitment = prepared
        .commitment("oasis7/private-task-context/v1")
        .unwrap();
    prepared.identity.provider_invocation_commitment = prepared
        .commitment("oasis7/private-task-invocation/v1")
        .unwrap();
    prepared.validate_against(&original).unwrap();
    let response = ws::sign_service_response(
        TASK_CONTEXT_PREPARE_PATH,
        ws::request_digest(TASK_CONTEXT_PREPARE_PATH, &original).unwrap(),
        prepared,
        &"3f".repeat(32),
    )
    .unwrap();
    verify_private_reply(
        TASK_CONTEXT_PREPARE_PATH,
        &original,
        &response,
        &f.state.draft().world_identity.service_public_key,
    )
    .unwrap();
    (original, response)
}
pub(super) fn captured_artifact(
    f: &SessionFixture,
    prepared: &PreparedTaskContextV1,
    original: &ws::SignedReadRequest<TaskContextPrepareV1>,
) -> (
    ws::SignedReadRequest<TaskContextGetV1>,
    ContextPrefixProofV1,
    ws::SignedServiceResponse<CapturedTaskContextV1>,
) {
    let get = ws::sign_read_request(
        TASK_CONTEXT_READ_PATH,
        TaskContextGetV1 {
            schema_revision: 1,
            context: prepared.identity.clone(),
            prepare_operation_id: original.request.client_operation_id.clone(),
        },
        &"41".repeat(32),
    )
    .unwrap();
    let prefix = ws::sign_read_request(
        "scheduler",
        public_prefix(prepared).unwrap(),
        &"41".repeat(32),
    )
    .unwrap();
    let payload: WorldServicePayloadV1 = WorldServicePayloadV1::Scheduler(prefix);
    let world = prepared.snapshot.read_commit.world.clone();
    let correlation = oasis7_client_api::world_service_correlation::derive_correlation_with_legacy(
        world,
        &payload,
        |_| Err("not legacy".into()),
    )
    .unwrap();
    let submit = ws::SubmitIntentRequest {
        contract_version: 1,
        correlation: correlation.clone(),
        deadline_unix_ms: Some(
            f.state.test_sessions()[ID].roots[&SessionPurposeV1::Execution]
                .deadline
                .get(),
        ),
        signed_payload: payload,
    };
    let commit = prepared.snapshot.read_commit.clone();
    let receipt = serde_json::json!({"fixture":"committed-prefix"});
    let response = ws::sign_service_response(
        ws::SUBMIT_PATH,
        ws::request_digest(ws::SUBMIT_PATH, &submit).unwrap(),
        ws::IntentResponse {
            formal_profile: None,
            execution_evidence_scope: None,
            contract_version: 1,
            correlation,
            outcome: ws::IntentOutcome::Committed {
                commit: Box::new(commit.clone()),
                receipt: receipt.clone(),
            },
        },
        &"3f".repeat(32),
    )
    .unwrap();
    let captured: CapturedTaskContextV1 = CapturedTaskContextV1 {
        schema_revision: 1,
        prepared: prepared.clone(),
        capture_commit: commit.clone(),
        capture_proof: CanonicalIntentResultV1 {
            request: submit.clone(),
            action_id: 1,
            committed_height: commit.position,
            receipt,
            rejected: None,
        },
        current_public_context: TaskPublicContextV1 {
            read_commit: commit,
            captured_base_binding: prepared.snapshot.captured_base_binding.clone(),
            provider_authority: None,
            native_authority: prepared.snapshot.native_authority.clone(),
            actor_observation: prepared.snapshot.actor_observation.clone(),
        },
    };
    captured.validate_against(&get).unwrap();
    let result = ws::sign_service_response(
        TASK_CONTEXT_READ_PATH,
        ws::request_digest(TASK_CONTEXT_READ_PATH, &get).unwrap(),
        captured,
        &"3f".repeat(32),
    )
    .unwrap();
    (
        get,
        ContextPrefixProofV1 {
            original: submit,
            response,
        },
        result,
    )
}
pub(super) fn oversized_captured(
    get: &ws::SignedReadRequest<TaskContextGetV1>,
    captured: &ws::SignedServiceResponse<CapturedTaskContextV1>,
) -> ws::SignedServiceResponse<CapturedTaskContextV1> {
    let mut payload = captured.payload.clone();
    payload.current_public_context.actor_observation["bounded-context-test-padding"] =
        serde_json::Value::String("x".repeat(CONTEXT_BODY_MAX_BYTES));
    payload.validate_against(get).unwrap();
    ws::sign_service_response(
        TASK_CONTEXT_READ_PATH,
        captured.request_digest.clone(),
        payload,
        &"3f".repeat(32),
    )
    .unwrap()
}
pub(super) fn envelope(scope: &ContextBodyScopeV1, plaintext_bytes: usize) -> ContextBodyRecordV1 {
    // Structural envelope only; SDK body authority is tested separately. Worker
    // AEAD tests use real WebCrypto and never infer plaintext from these bytes.
    ContextBodyRecordV1 {
        scope: scope.clone(),
        envelope: KeyEnvelopeV1 {
            context: scope.envelope_context().unwrap(),
            ciphertext: "ab".repeat(plaintext_bytes + AES_GCM_TAG_BYTES),
            value_nonce: "01".repeat(12),
            wrapped_dek: "cd".repeat(48),
            wrap_nonce: "02".repeat(12),
            wrapping_key_version: DecimalU64(1),
        },
    }
}
