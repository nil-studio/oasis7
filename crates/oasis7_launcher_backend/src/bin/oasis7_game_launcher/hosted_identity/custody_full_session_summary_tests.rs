use super::*;
use oasis7_custody_core::provisioning::*;
fn session_options(now: u64) -> Value {
    let mut request = options(now);
    request["approval_request_id"] = json!("full-summary-old");
    let fence: oasis7_agent_api::game::GameTaskFenceV1=serde_json::from_value(json!({"world_id":"world","genesis_digest":"genesis","branch_id":"main","reorg_epoch":"0","agent_id":"agent","owner_binding":"owner","owner_generation":"1","owner_public_key":"bb".repeat(32),"task_id":"task","task_revision":"4","task_status":"active","latest_goal_revision":"1","active_goal_revision":"1","active_goal_reference":{"goal_revision":"1","goal_ref":"11".repeat(32),"prepare_operation_id":"prepare","publish_operation_id":"publish","activation_committed_height":"1"},"executor_epoch":"1","executor_binding":{"executor_id":"executor","executor_public_key":"cc".repeat(32),"delegation_generation":"1","executor_epoch":"1"}})).unwrap();
    let world:ApprovedWorldV1=serde_json::from_value(json!({"world_id":"world","genesis_digest":"genesis","branch_id":"main","reorg_epoch":"0","service_public_key":"cd".repeat(32)})).unwrap();
    let mut term = TerminationPlanV2 {
        api_version: 2,
        session_id: "session".into(),
        custody_account_id: "custody-a".into(),
        provision_id: "provision".into(),
        setup_id: "setup".into(),
        prepared_result_digest: "ab".repeat(32),
        world: world.clone(),
        owner_key_ref: "owner".into(),
        executor_key_ref: "executor".into(),
        initial_fence: fence.clone(),
        original_delegation: oasis7_client_api::world_service_wire::AgentSignerDelegationChangeV1 {
            world_id: "world".into(),
            branch_id: "main".into(),
            agent_id: "agent".into(),
            owner_binding: "owner".into(),
            agent_identity_generation: 1,
            generation: 1,
            delegate_public_key: "cc".repeat(32),
            revoked: false,
            nonce: 1,
        },
        stop_operation_id: String::new(),
        close_operation_id: String::new(),
        cleanup_until: oasis7_custody_core::time_units::UnixMillis::new(now + 120_000),
        request_limit: DecimalU64(2),
        transport_limit: DecimalU64(24),
        trigger: TerminationTriggerV1::ClosingOnlyV1,
        nonce_policy: TerminationNoncePolicyV1::AccountAllocatedHighWaterV1,
        close_policy: WorldTaskClosePolicyV2::LiteralPersistentClosedV2,
    };
    term.stop_operation_id = term.expected_stop_operation_id().unwrap();
    term.close_operation_id = term.expected_close_operation_id().unwrap();
    let scope = serde_json::json!({"session_id":"session","native_account_id":"native-a","world_identity":world,"task_fence":fence,"executor_key_ref":"executor","client_binding":{"profile":"native-broker-custody-v1","client_key":"dd".repeat(32)},"client_pairing":{"api_version":1,"target":{"custody_account_id":"custody-a","provision_id":"provision","invite_id":"invite","invite_digest":"aa".repeat(32)},"enrollment_id":"enrollment","enrollment_digest":"ab".repeat(32),"host_instance_id":"fixture-not-os-qualified","descriptor_digest":"ac".repeat(32),"client_key_id":"key","client_public_key":"dd".repeat(32),"connector_public_key":null},"runtime_ref":"runtime","skill_ref":"skill","source_ref":"source","termination_plan_digest":term.digest().unwrap()});
    request["intent"] = json!({"kind":"test_session_v2","scope":scope,"plan":{"api_version":2,"model_budget":null,"termination":term,"request_id":"request","enrollment_id":"enrollment","basis_digest":"aa".repeat(32),"owner_setup_plan_digest":"ab".repeat(32),"prepared_resource_basis_digest":null,"selection":{"kind":"move","position_cm":["1","2","3"]},"actions":[{"tool":"move","position_cm":["1","2","3"]}],"allowed_resource_refs":[],"write_deadline":(now+100_000).to_string(),"read_until":(now+120_000).to_string(),"cleanup_until":(now+120_000).to_string(),"request_limit":"100","transport_limit":"400","business_limit":"0","read_budget":{"requests":"100","transport":"300","business":"0"},"cleanup_budget":{"requests":"2","transport":"24","business":"0"},"control_budget":{"requests":"80","transport_per_hop":"240","identity_transport":"160","transport_hops":1}}});
    request
}
#[test]
fn full_session_summary_is_required_with_no_model_and_old_records_stay_opaque() {
    let (mut f, key) = fixture();
    let now = now_ms();
    let old = session_options(now);
    let start = f
        .service
        .custody_options(Some(&f.token), &old, "source", now)
        .unwrap();
    assert!(start.get("model_budget_summary_contract").is_none());
    assert_eq!(
        start["custody_session_summary_v2"]["plan"],
        old["intent"]["plan"]
    );
    f.service
        .custody
        .as_mut()
        .unwrap()
        .full_session_v2_summary_contract_enabled = true;
    let replay = f
        .service
        .custody_options(Some(&f.token), &old, "source", now + 1)
        .unwrap();
    assert_eq!(replay["ceremony_id"], start["ceremony_id"]);
    assert!(replay.get("model_budget_summary_contract").is_none());
    let mut fresh = old;
    fresh["approval_request_id"] = json!("full-summary-new");
    let typed: ApprovalOptionsRequestV1 = serde_json::from_value(fresh.clone()).unwrap();
    let expected = serde_json::to_value(
        typed
            .expected_model_budget_summary_contract()
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(expected["contract"], json!("full_test_session_v2"));
    assert!(expected["model_budget_contract"].is_null());
    let start = f
        .service
        .custody_options(Some(&f.token), &fresh, "source", now + 2)
        .unwrap();
    assert_eq!(start["model_budget_summary_contract"], expected);
    let completed = f
        .service
        .custody_complete(
            Some(&f.token),
            &complete_payload(&start, &key, 0x05),
            "source",
            now + 3,
        )
        .unwrap();
    assert_eq!(
        completed["approval"]["proof"]["model_budget_summary_contract"],
        expected
    );
    let signed: SignedCustodyApprovalV1 =
        serde_json::from_value(completed["approval"].clone()).unwrap();
    let signature =
        Signature::from_slice(&URL_SAFE_NO_PAD.decode(&signed.signature).unwrap()).unwrap();
    f.service
        .custody
        .as_ref()
        .unwrap()
        .key
        .verifying_key()
        .verify_strict(&signed.proof.signing_bytes().unwrap(), &signature)
        .unwrap();
    // Actual software UV/Native signer, not a browser, World or OS qualification.
}
