use super::*;

fn model_v2_options(now: u64) -> Value {
    let mut request = options(now);
    request["approval_request_id"] = json!("model-summary-old");
    request["intent"] = json!({"kind":"owner_setup_v2","plan":{
        "api_version":2,"setup_id":"model-setup","provision_id":"provision","participant_receipt_digest":"ab".repeat(32),
        "world_identity":{"world_id":"world","genesis_digest":"genesis","branch_id":"main","reorg_epoch":"0","service_public_key":"cd".repeat(32)},
        "owner_key_ref":"owner","executor_key_ref":"executor","executor_public_key":"de".repeat(32),"bootstrap_config_version":"1",
        "goal_content":{"goal_text":"private model budget goal","soft_constraints":[]},
        "create_operation_id":"create","prepare_operation_id":"prepare","prepare_nonce":"ef".repeat(32),"publish_operation_id":"publish",
        "delegation_operation_id":"delegate","bind_operation_id":"bind","executor_id":"executor","delegation_generation":"1","delegation_nonce":"1",
        "selection":{"kind":"build","site":"site","factory":"factory","factory_kind":"smelter"},
        "deadline":(now+100_000).to_string(),"request_limit":"6","transport_limit":"584","write_limit":"4",
        "model_budget":{"api_version":1,"approval_contract":"native_custody_model_budget_v1","native_account_id":"native-a","native_public_key":"11".repeat(32),"native_instance_public_key":"22".repeat(32),
          "ledger_id":"ledger","preset_id":"fixed-preset","preset_revision":"1","model":"fixed-model","runtime_digest":"33".repeat(32),"skill_digest":"44".repeat(32),"tools_digest":"55".repeat(32),"context_policy_digest":"66".repeat(32),
          "physical_graph_digest":"77".repeat(32),"probe_physical_http":["10","10"],"inference_physical_http":"10","max_calls":"3","max_input_tokens":"100","max_output_tokens":"50","max_total_tokens":"450","max_concurrent":"1","deadline":(now+90_000).to_string(),
          "price_policy":{"currency":"USD","input_micro_units_per_million":"1000000","output_micro_units_per_million":"2000000","max_total_micro_units":"600","quote_digest":"88".repeat(32),"valid_until":(now+100_000).to_string()}}
    }});
    request
}
#[test]
fn native_summary_contract_is_server_owned_and_old_pending_records_never_upgrade() {
    let (mut f, key) = fixture();
    let now = now_ms();
    let old = model_v2_options(now);
    let start = f
        .service
        .custody_options(Some(&f.token), &old, "source", now)
        .unwrap();
    assert!(start.get("model_budget_summary_contract").is_none());
    assert_eq!(
        start["model_budget_summary"],
        old["intent"]["plan"]["model_budget"]
    );
    f.service
        .custody
        .as_mut()
        .unwrap()
        .model_budget_summary_contract_enabled = true;
    let retried = f
        .service
        .custody_options(Some(&f.token), &old, "source", now + 1)
        .unwrap();
    assert_eq!(retried["approval_id"], start["approval_id"]);
    assert!(retried.get("model_budget_summary_contract").is_none());
    let mut fresh = old;
    fresh["approval_request_id"] = json!("model-summary-new");
    let typed: ApprovalOptionsRequestV1 = serde_json::from_value(fresh.clone()).unwrap();
    let expected = serde_json::to_value(
        typed
            .expected_model_budget_summary_contract()
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let fresh_start = f
        .service
        .custody_options(Some(&f.token), &fresh, "source", now + 2)
        .unwrap();
    assert_eq!(fresh_start["model_budget_summary_contract"], expected);
    let result = f
        .service
        .custody_complete(
            Some(&f.token),
            &complete_payload(&fresh_start, &key, 0x05),
            "source",
            now + 3,
        )
        .unwrap();
    assert_eq!(
        result["approval"]["proof"]["model_budget_summary_contract"],
        expected
    );
    let signed: SignedCustodyApprovalV1 =
        serde_json::from_value(result["approval"].clone()).unwrap();
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
    // Software UV verifies the actual Native ceremony/signature path; this unit
    // test does not claim browser rendering, Account admission or N capability.
}
