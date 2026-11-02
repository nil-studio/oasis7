//! Matching ordinary World signature fixture, not actual Runtime/CF.
use super::*;
fn attest<Q: serde::Serialize, T: serde::Serialize>(
    domain: &str,
    q: &Q,
    payload: T,
) -> WorldContextAttestationV2<T> {
    let s = ws::sign_service_response(
        domain,
        ws::request_digest(domain, q).unwrap(),
        payload,
        &"3f".repeat(32),
    )
    .unwrap();
    WorldContextAttestationV2 {
        request_digest: s.request_digest,
        payload: s.payload,
        signature_hex: s.signature_hex,
    }
}
pub(super) fn delivery(
    f: &SessionFixture,
    q: &NativeExecutionPrepareRequestV2,
) -> WorldContextAttestationV2<TaskContextPreparedDeliveryV2> {
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
    let fence = &q.original.access.fence;
    assert!(oasis7_agent_api::game::GameTaskFenceV1::from_control(&snapshot.control) == *fence);
    snapshot.runtime_binding.world_id = fence.world_id.clone();
    snapshot.runtime_binding.branch_id = fence.branch_id.clone();
    snapshot.runtime_binding.reorg_epoch = fence.reorg_epoch.0;
    snapshot.captured_base_binding.world_id = fence.world_id.clone();
    snapshot.captured_base_binding.branch_id = fence.branch_id.clone();
    snapshot.captured_base_binding.reorg_epoch = fence.reorg_epoch.0;
    let plan = f.state.test_session_v2_header(&f.session_id).unwrap().0;
    let native = snapshot.native_authority.as_mut().unwrap();
    native.fence = fence.clone();
    native.signer_delegation = plan.termination.original_delegation.clone();
    snapshot.validate().unwrap();
    let until = context_deadline(
        snapshot.captured_base_binding.base_tick,
        snapshot.authority_valid_until_tick().unwrap(),
    )
    .unwrap();
    let mut prepared = PreparedTaskContextV1 {
        schema_revision: 2,
        prepare_proof: q.original.request.original.clone(),
        snapshot,
        reference_salt: "e9".repeat(32),
        valid_until_tick: until,
        identity: TaskContextIdentityV1 {
            authority_mode: Some(TaskContextAuthorityModeV1::NativeExecutorV1),
            native_authorization_root: None,
            schema_revision: 2,
            fence: fence.clone(),
            valid_until_tick: until,
            agent_session_id: "read-history-public-session".into(),
            agent_turn_id: "read-history-public-turn".into(),
            decision_request_id: "read-history-public-decision".into(),
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
    prepared
        .validate_against(&q.original.request.original)
        .unwrap();
    let private_body = ws::sign_service_response(
        TASK_CONTEXT_PREPARE_PATH,
        ws::request_digest(TASK_CONTEXT_PREPARE_PATH, &q.original.request.original).unwrap(),
        prepared,
        &"3f".repeat(32),
    )
    .unwrap();
    let public = derive_prepared_projection_v2(
        &q.original.request,
        &private_body,
        &q.proof.certificate.world.service_public_key,
    )
    .unwrap();
    let projection = attest(PREPARED_PROJECTION_DOMAIN_V2, &q.original.request, public);
    let result = attest(
        CONTEXT_PREPARE_ATTESTED_PATH_V2,
        &q.original.request,
        TaskContextPreparedDeliveryV2 {
            private_body,
            projection,
        },
    );
    verify_prepared_delivery_v2(
        &q.original.request,
        &result,
        &q.proof.certificate.world.service_public_key,
    )
    .unwrap();
    result
}
