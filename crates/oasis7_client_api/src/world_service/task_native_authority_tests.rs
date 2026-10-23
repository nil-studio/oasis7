use super::*;
fn fence() -> GameTaskFenceV1 {
    serde_json::from_value(serde_json::json!({"world_id":"w","genesis_digest":"g","branch_id":"b","reorg_epoch":"0","agent_id":"a","owner_binding":"o","owner_generation":"1","owner_public_key":"ab".repeat(32),"task_id":"t","task_revision":"2","task_status":"active","latest_goal_revision":"1","active_goal_revision":"1","active_goal_reference":{"goal_revision":"1","goal_ref":"cd".repeat(32),"prepare_operation_id":"prepare","publish_operation_id":"publish","activation_committed_height":"2"},"executor_epoch":"1","executor_binding":{"executor_id":"e","executor_public_key":sign_read_request("key",(),&"17".repeat(32)).unwrap().subject_public_key,"delegation_generation":"1","executor_epoch":"1"}})).unwrap()
}
#[test]
fn original_provider_prepare_preserves_typed_cbor_digest_and_signature() {
    #[derive(Serialize)]
    struct Legacy {
        schema_revision: u32,
        fence: GameTaskFenceV1,
        client_operation_id: String,
        client_nonce: String,
    }
    let old = Legacy {
        schema_revision: 1,
        fence: fence(),
        client_operation_id: "operation".into(),
        client_nonce: "aa".repeat(32),
    };
    let current = TaskContextPrepareV1 {
        authority_mode: None,
        schema_revision: 1,
        fence: old.fence.clone(),
        client_operation_id: old.client_operation_id.clone(),
        client_nonce: old.client_nonce.clone(),
    };
    assert_eq!(
        serde_json::to_vec(&old).unwrap(),
        serde_json::to_vec(&current).unwrap()
    );
    assert_eq!(
        oasis7_wasm_abi::encode_canonical_cbor(&old).unwrap(),
        oasis7_wasm_abi::encode_canonical_cbor(&current).unwrap()
    );
    assert_eq!(
        request_digest(TASK_CONTEXT_PREPARE_PATH, &old).unwrap(),
        request_digest(TASK_CONTEXT_PREPARE_PATH, &current).unwrap()
    );
    let a = sign_read_request(TASK_CONTEXT_PREPARE_PATH, old, &"17".repeat(32)).unwrap();
    let b = sign_read_request(TASK_CONTEXT_PREPARE_PATH, current, &"17".repeat(32)).unwrap();
    assert_eq!(a.signature_hex, b.signature_hex);
    verify_prepare(&b).unwrap();
}
#[test]
fn native_mode_is_signed_and_unknown_or_wrong_revision_is_rejected() {
    let native = TaskContextPrepareV1 {
        authority_mode: Some(TaskContextAuthorityModeV1::NativeExecutorV1),
        schema_revision: 2,
        fence: fence(),
        client_operation_id: "operation".into(),
        client_nonce: "aa".repeat(32),
    };
    let signed =
        sign_read_request(TASK_CONTEXT_PREPARE_PATH, native.clone(), &"17".repeat(32)).unwrap();
    verify_prepare(&signed).unwrap();
    let mut changed = signed.clone();
    changed.request.authority_mode = None;
    changed.request.schema_revision = 1;
    assert!(verify_prepare(&changed).is_err());
    let mut wrong = native.clone();
    wrong.schema_revision = 1;
    assert!(
        verify_prepare(
            &sign_read_request(TASK_CONTEXT_PREPARE_PATH, wrong, &"17".repeat(32)).unwrap()
        )
        .is_err()
    );
    let mut unknown = serde_json::to_value(native).unwrap();
    unknown["authority_mode"] = serde_json::json!("unknown");
    assert!(serde_json::from_value::<TaskContextPrepareV1>(unknown).is_err());
    assert!(TaskAuthorityRef::select(None, None).is_err());
}

fn native_fixture() -> (NativeTaskAuthorityV1, RuntimeCognitionBaseBindingV1) {
    let f = fence();
    let root = format!("blake3:{}", "42".repeat(32));
    let base = RuntimeCognitionBaseBindingV1 {
        world_id: f.world_id.clone(),
        branch_id: f.branch_id.clone(),
        finality_epoch: 0,
        finality_block_hash: None,
        finality_status: "pending".into(),
        base_tick: 10,
        base_world_hash: format!("blake3:{}", "43".repeat(32)),
        reorg_epoch: 0,
        runtime_manifest_hash: format!("blake3:{}", "44".repeat(32)),
    };
    let native = NativeTaskAuthorityV1 {
        schema_revision: 1,
        authority_mode: TaskContextAuthorityModeV1::NativeExecutorV1,
        signer_delegation: AgentSignerDelegationChangeV1 {
            world_id: f.world_id.clone(),
            branch_id: f.branch_id.clone(),
            agent_id: f.agent_id.clone(),
            owner_binding: f.owner_binding.clone(),
            agent_identity_generation: f.owner_generation.0,
            generation: 1,
            delegate_public_key: f
                .executor_binding
                .as_ref()
                .unwrap()
                .executor_public_key
                .clone(),
            revoked: false,
            nonce: 1,
        },
        capability_snapshot_hash: oasis7_agent_api::h_v1("oasis7.runtime.manifest.v1", &root)
            .to_string(),
        authority_context_hash: oasis7_agent_api::h_v1(
            "oasis7.runtime.authority-context.v1",
            &root,
        )
        .to_string(),
        capability_authorization_root: root,
        native_authorization_root: format!("blake3:{}", "45".repeat(32)),
        native_delegation_authorizations: vec![],
        logical_tick: 10,
        valid_until_tick: 26,
        fence: f,
    };
    (native, base)
}
// A serialization-only provider shape. It is never installed or admitted as a grant.
fn provider_shape() -> WorldServiceAgentContext {
    let f = fence();
    let subject = serde_json::json!({"kind":"agent","agent_id":f.agent_id,"owner_binding":f.owner_binding,"generation":f.owner_generation.0});
    let presenter = serde_json::json!({"presenter_id":"provider","presenter_kind":"provider","session_id":"test-session"});
    let audience = serde_json::json!({"world_id":"w","branch_id":"b","finality_epoch":0,"target_kind":"world"});
    serde_json::from_value(serde_json::json!({"agent_id":"a","capability_authorization_root":"root","capability_snapshot_hash":"hash","authority_context_hash":"hash","payer_binding":{},"capability_catalog":{"snapshot_id":"catalog","world_id":"w","world_head":0,"branch_id":"b","finality_epoch":0,"logical_tick":10,"module_registry_hash":"modules","policy_hash":"policy","revocation_epoch":0,"subject":subject,"presenter":presenter,"audience":audience,"entries":[],"valid_until_tick":30},"capability_invocation_context":{"grant_id":"grant","subject":subject,"presenter":presenter,"audience":audience,"catalog_snapshot_id":"catalog","module_id":"test","module_version":"1","response_nonce":"test"}})).unwrap()
}
#[test]
fn native_authority_codec_requires_exactly_one_and_never_serializes_provider_placeholder() {
    let (native, base) = native_fixture();
    native.validate_for(&native.fence, &base).unwrap();
    let provider = provider_shape();
    assert!(TaskAuthorityRef::select(Some(&provider), Some(&native)).is_err());
    assert!(TaskAuthorityRef::select(None, None).is_err());
    assert!(matches!(
        TaskAuthorityRef::select(None, Some(&native)).unwrap(),
        TaskAuthorityRef::Native(_)
    ));
    let json = serde_json::to_value(&native).unwrap();
    assert!(json.get("capability_catalog").is_none());
    assert!(json.get("capability_invocation_context").is_none());
}
#[test]
fn native_authority_codec_rejects_wrong_world_fence_signer_root_and_lease() {
    for case in 0..10 {
        let (mut native, base) = native_fixture();
        let fence = native.fence.clone();
        match case {
            0 => native.schema_revision = 2,
            1 => native.fence.task_revision.0 += 1,
            2 => native.signer_delegation.world_id = "other".into(),
            3 => native.signer_delegation.generation += 1,
            4 => native.signer_delegation.revoked = true,
            5 => native.capability_authorization_root = format!("blake3:{}", "46".repeat(32)),
            6 => native.native_authorization_root = "bad".into(),
            7 => native.valid_until_tick += 1,
            8 => native.logical_tick += 1,
            _ => native.signer_delegation.owner_binding = "other".into(),
        }
        assert!(
            native.validate_for(&fence, &base).is_err(),
            "accepted {case}"
        );
    }
    let (native, mut base) = native_fixture();
    base.world_id = "other".into();
    assert!(native.validate_for(&native.fence, &base).is_err());
}

fn prefix_fixture() -> TaskSchedulerIntentV1 {
    let (native, base) = native_fixture();
    let identity = TaskContextIdentityV1 {
        authority_mode: Some(TaskContextAuthorityModeV1::NativeExecutorV1),
        native_authorization_root: Some(native.native_authorization_root),
        schema_revision: 2,
        fence: native.fence,
        agent_session_id: "session".into(),
        agent_turn_id: "turn".into(),
        decision_request_id: "decision".into(),
        retry_seq: 1,
        transport_attempt: 1,
        request_commitment: format!("blake3:{}", "a1".repeat(32)),
        context_commitment: format!("blake3:{}", "a2".repeat(32)),
        provider_invocation_commitment: format!("blake3:{}", "a3".repeat(32)),
        valid_until_tick: DecimalU64(26),
    };
    SchedulerIntentV1 {
        agent_id: "a".into(),
        request_id: "decision".into(),
        delegation_generation: 1,
        task_prefix_binding_revision: 0,
        captured_base_binding: base,
        operation: SchedulerOperationV1::TaskProviderPrefix { context: identity },
    }
}
#[test]
fn historical_task_prefix_bytes_and_signature_survive_revision_default() {
    #[derive(Serialize)]
    struct Historical<'a> {
        agent_id: &'a str,
        request_id: &'a str,
        delegation_generation: u64,
        captured_base_binding: &'a RuntimeCognitionBaseBindingV1,
        operation: &'a TaskSchedulerOperationV1,
    }
    let prefix = prefix_fixture();
    let old = Historical {
        agent_id: &prefix.agent_id,
        request_id: &prefix.request_id,
        delegation_generation: prefix.delegation_generation,
        captured_base_binding: &prefix.captured_base_binding,
        operation: &prefix.operation,
    };
    let old_json = serde_json::to_vec(&old).unwrap();
    let decoded: TaskSchedulerIntentV1 = serde_json::from_slice(&old_json).unwrap();
    assert_eq!(decoded.task_prefix_binding_revision, 0);
    assert_eq!(old_json, serde_json::to_vec(&decoded).unwrap());
    assert_eq!(
        oasis7_wasm_abi::encode_canonical_cbor(&old).unwrap(),
        oasis7_wasm_abi::encode_canonical_cbor(&decoded).unwrap()
    );
    let signed_old = sign_read_request("scheduler", old, &"17".repeat(32)).unwrap();
    let signed_new = sign_read_request("scheduler", decoded, &"17".repeat(32)).unwrap();
    assert_eq!(signed_old.signature_hex, signed_new.signature_hex);
    let mut changed = signed_new.clone();
    changed.request.task_prefix_binding_revision = 1;
    assert!(
        verify_owner_signature_strict(
            "scheduler",
            &changed.request,
            &changed.subject_public_key,
            &changed.signature_hex
        )
        .is_err()
    );
}
#[test]
fn scheduler_revision_is_signed_and_only_known_task_prefix_revision_is_valid() {
    let world = WorldIdentity {
        world_id: "w".into(),
        genesis_digest: "g".into(),
    };
    for revision in [0, 1, 2, u16::MAX] {
        let mut prefix = prefix_fixture();
        prefix.task_prefix_binding_revision = revision;
        for is_prefix in [true, false] {
            let mut request = prefix.clone();
            if !is_prefix {
                let SchedulerOperationV1::TaskProviderPrefix { context } = request.operation else {
                    unreachable!()
                };
                request.operation = SchedulerOperationV1::TaskProviderFailure {
                    context,
                    reason: "provider_failed".into(),
                };
            }
            let payload: WorldServicePayloadV1 = WorldServicePayloadV1::Scheduler(
                sign_read_request("scheduler", request, &"17".repeat(32)).unwrap(),
            );
            let result =
                crate::world_service_correlation::derive_correlation(world.clone(), &payload);
            assert_eq!(
                result.is_ok(),
                revision == 0 || (revision == 1 && is_prefix),
                "revision {revision}, prefix {is_prefix}"
            );
        }
    }
}
