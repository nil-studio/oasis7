//! Ordinary matching synthetic World component delivery. Full SDK verification;
//! no Runtime/CF/Native45 qualification and no padding to claim large-body proof.
use super::*;
use oasis7_agent_api::game::{GameTaskFenceV1, TaskContextAuthorityModeV1, TaskContextIdentityV1};
use oasis7_client_api::world_service::{task_context::*, task_context_projection::*};
fn attest<Q: serde::Serialize, T: serde::Serialize>(
    domain: &str,
    q: &Q,
    payload: T,
    key: &str,
) -> WorldContextAttestationV2<T> {
    let signed =
        ws::sign_service_response(domain, ws::request_digest(domain, q).unwrap(), payload, key)
            .unwrap();
    WorldContextAttestationV2 {
        request_digest: signed.request_digest,
        payload: signed.payload,
        signature_hex: signed.signature_hex,
    }
}
pub(super) fn response(
    data: &identity::participant_entry::resource_fixture::Data,
    input: &[u8],
) -> Vec<u8> {
    let q: TaskContextPrepareAttestationRequestV2 = serde_json::from_slice(input).unwrap();
    validate_prepare_attestation_request_v2(&q).unwrap();
    let fence = GameTaskFenceV1::from_control(&data.state.control);
    assert!(q.original.request.fence == fence);
    let mut snapshot: GoalExecutionSnapshotV1 = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../oasis7_game_client/testdata/task-context/native-snapshot.json"
    )))
    .unwrap();
    snapshot.control = data.state.control.clone();
    snapshot.active_goal = data.prepared.clone();
    snapshot.read_commit = data.state.read_commit.clone().into();
    snapshot.activation_commit = snapshot.read_commit.clone();
    snapshot.activation_commit.position = fence
        .active_goal_reference
        .as_ref()
        .unwrap()
        .activation_committed_height
        .0;
    snapshot.runtime_binding.world_id = fence.world_id.clone();
    snapshot.runtime_binding.branch_id = fence.branch_id.clone();
    snapshot.runtime_binding.reorg_epoch = fence.reorg_epoch.0;
    snapshot.runtime_binding.base_tick = data.state.logical_tick.0;
    snapshot.captured_base_binding.world_id = fence.world_id.clone();
    snapshot.captured_base_binding.branch_id = fence.branch_id.clone();
    snapshot.captured_base_binding.reorg_epoch = fence.reorg_epoch.0;
    snapshot.captured_base_binding.base_tick = data.state.logical_tick.0;
    snapshot.actor_observation = json!({"agent_id":fence.agent_id,"position_cm":[0,0,0]});
    if std::env::var_os("OASIS7_NATIVE_ARCHIVE_RUNTIME_OBSERVATION").is_some() {
        let (tick, observation) = super::super::runtime_observation::build(data);
        assert_eq!(tick, data.state.logical_tick.0);
        snapshot.actor_observation = observation;
    }

    let until = data.state.logical_tick.0 + TASK_CONTEXT_LEASE_TICKS;
    let native = snapshot.native_authority.as_mut().unwrap();
    native.fence = fence.clone();
    native.logical_tick = data.state.logical_tick.0;
    native.valid_until_tick = until;
    native.native_delegation_authorizations.clear();
    let signer = data.state.signer_delegation.as_ref().unwrap();
    native.signer_delegation =
        oasis7_client_api::world_service_wire::AgentSignerDelegationChangeV1 {
            world_id: signer.world_id.clone(),
            branch_id: signer.branch_id.clone(),
            agent_id: signer.agent_id.clone(),
            owner_binding: signer.owner_binding.clone(),
            agent_identity_generation: signer.agent_identity_generation.0,
            generation: signer.generation.0,
            delegate_public_key: signer.delegate_public_key.clone(),
            revoked: signer.revoked,
            nonce: signer.nonce.0,
        };
    snapshot.validate().unwrap();
    let native_root = snapshot
        .native_authority
        .as_ref()
        .unwrap()
        .native_authorization_root
        .clone();
    let mut prepared = PreparedTaskContextV1 {
        schema_revision: 2,
        prepare_proof: q.original.clone(),
        snapshot,
        reference_salt: "e7".repeat(32),
        valid_until_tick: DecimalU64(until),
        identity: TaskContextIdentityV1 {
            authority_mode: Some(TaskContextAuthorityModeV1::NativeExecutorV1),
            native_authorization_root: Some(native_root),
            schema_revision: 2,
            fence,
            valid_until_tick: DecimalU64(until),
            agent_session_id: "native-archive-component-session".into(),
            agent_turn_id: "native-archive-component-turn".into(),
            decision_request_id: "native-archive-component-decision".into(),
            retry_seq: 1,
            transport_attempt: 1,
            request_commitment: format!("blake3:{}", "0".repeat(64)),
            context_commitment: format!("blake3:{}", "0".repeat(64)),
            provider_invocation_commitment: format!("blake3:{}", "0".repeat(64)),
        },
    };
    prepared.identity.request_commitment = prepared
        .commitment("oasis7/private-task-request/v1")
        .unwrap();
    prepared.identity.context_commitment = prepared
        .commitment("oasis7/private-task-context/v1")
        .unwrap();
    prepared.identity.provider_invocation_commitment = prepared
        .commitment("oasis7/private-task-invocation/v1")
        .unwrap();
    prepared.validate_against(&q.original).unwrap();
    let private_body = ws::sign_service_response(
        TASK_CONTEXT_PREPARE_PATH,
        ws::request_digest(TASK_CONTEXT_PREPARE_PATH, &q.original).unwrap(),
        prepared,
        &data.world_key,
    )
    .unwrap();
    let public = derive_prepared_projection_v2(
        &q,
        &private_body,
        &ws::sign_read_request("key", (), &data.world_key)
            .unwrap()
            .subject_public_key,
    )
    .unwrap();
    let projection = attest(PREPARED_PROJECTION_DOMAIN_V2, &q, public, &data.world_key);
    let delivery = attest(
        CONTEXT_PREPARE_ATTESTED_PATH_V2,
        &q,
        TaskContextPreparedDeliveryV2 {
            private_body,
            projection,
        },
        &data.world_key,
    );
    verify_prepared_delivery_v2(
        &q,
        &delivery,
        &ws::sign_read_request("key", (), &data.world_key)
            .unwrap()
            .subject_public_key,
    )
    .unwrap();
    serde_json::to_vec(&delivery).unwrap()
}
