//! Byte-for-byte old provider request/snapshot/core encoding reference.
use super::*;
use serde::Serialize;
#[derive(Serialize)]
struct LegacyRequest<'a> {
    schema_revision: u32,
    fence: &'a GameTaskFenceV1,
    client_operation_id: &'a str,
    client_nonce: &'a str,
}
#[derive(Serialize)]
struct LegacySnapshot<'a> {
    schema_revision: u32,
    control: &'a oasis7_agent_api::game::GameTaskControlV1,
    read_commit: &'a oasis7_client_api::world_service::CommitRef,
    activation_commit: &'a oasis7_client_api::world_service::CommitRef,
    active_goal: &'a oasis7_client_api::world_service::private_goal::PreparedGoalReceiptV1,
    actor_observation_schema: &'a str,
    actor_observation: &'a serde_json::Value,
    provider_authority: &'a oasis7_agent_api::world_context::WorldServiceAgentContext,
    runtime_binding: &'a oasis7_agent_api::RuntimeBindingV1,
    captured_base_binding: &'a oasis7_agent_api::world_cognition::RuntimeCognitionBaseBindingV1,
}
#[derive(Serialize)]
struct LegacyCore<'a> {
    prepare_proof: SignedReadRequest<LegacyRequest<'a>>,
    snapshot: LegacySnapshot<'a>,
    session: &'a str,
    turn: &'a str,
    request: &'a str,
    valid_until_tick: DecimalU64,
}
pub(super) fn verify_original_provider_bytes(record: &PreparedTaskContextV1) {
    let s = &record.snapshot;
    let r = &record.prepare_proof;
    let q = &r.request;
    let legacy_request = LegacyRequest {
        schema_revision: q.schema_revision,
        fence: &q.fence,
        client_operation_id: &q.client_operation_id,
        client_nonce: &q.client_nonce,
    };
    assert_eq!(
        serde_cbor::to_vec(&q).unwrap(),
        serde_cbor::to_vec(&legacy_request).unwrap()
    );
    let snapshot = LegacySnapshot {
        schema_revision: s.schema_revision,
        control: &s.control,
        read_commit: &s.read_commit,
        activation_commit: &s.activation_commit,
        active_goal: &s.active_goal,
        actor_observation_schema: &s.actor_observation_schema,
        actor_observation: &s.actor_observation,
        provider_authority: s.provider_authority.as_ref().unwrap(),
        runtime_binding: &s.runtime_binding,
        captured_base_binding: &s.captured_base_binding,
    };
    assert_eq!(
        serde_cbor::to_vec(s).unwrap(),
        serde_cbor::to_vec(&snapshot).unwrap()
    );
    assert_eq!(
        serde_json::to_vec(s).unwrap(),
        serde_json::to_vec(&snapshot).unwrap()
    );
    let core = LegacyCore {
        prepare_proof: SignedReadRequest {
            request: legacy_request,
            subject_public_key: r.subject_public_key.clone(),
            signature_hex: r.signature_hex.clone(),
        },
        snapshot,
        session: &record.identity.agent_session_id,
        turn: &record.identity.agent_turn_id,
        request: &record.identity.decision_request_id,
        valid_until_tick: record.valid_until_tick,
    };
    for domain in [
        "oasis7/private-task-request/v1",
        "oasis7/private-task-context/v1",
        "oasis7/private-task-invocation/v1",
    ] {
        let bytes =
            oasis7_wasm_abi::encode_canonical_cbor(&(domain, &record.reference_salt, &core))
                .unwrap();
        assert_eq!(
            record.commitment(domain).unwrap(),
            format!("blake3:{}", blake3::hash(&bytes))
        );
    }
}

/// Explicit test-only export of synthetic fixed-key scenario data for SDK HTTP
/// verifier regression fixtures. Never reads a deployed world or user material.
pub(super) fn export_test_snapshot_if_requested(record: &PreparedTaskContextV1) {
    let Ok(root) = std::env::var("OASIS7_TASK_TEST_FIXTURE_DIR") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    assert!(root.is_absolute());
    let name = if record.identity.authority_mode.is_some() {
        "native-snapshot.json"
    } else {
        "provider-snapshot.json"
    };
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(root.join(name))
        .unwrap();
    file.write_all(&serde_json::to_vec_pretty(&record.snapshot).unwrap())
        .unwrap();
    let request = sign_read_request(
        TASK_SNAPSHOT_PATH,
        TaskGoalSnapshotRequestV1 {
            authority_mode: record.identity.authority_mode,
            schema_revision: record.snapshot.schema_revision,
            fence: record.identity.fence.clone(),
        },
        &hex::encode([73; 32]),
    )
    .unwrap();
    assert_eq!(
        request.subject_public_key,
        record
            .identity
            .fence
            .executor_binding
            .as_ref()
            .unwrap()
            .executor_public_key
    );
    let request_name = if record.identity.authority_mode.is_some() {
        "native-snapshot-request.json"
    } else {
        "provider-snapshot-request.json"
    };
    let mut request_file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(root.join(request_name))
        .unwrap();
    request_file
        .write_all(&serde_json::to_vec_pretty(&request).unwrap())
        .unwrap();
}
