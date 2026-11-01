//! Real new approval/Book/consume with fixture Ed25519 roots; no model IO or live-price qualification.
use super::*;
use oasis7_custody_core::approval::{
    NativeCustodySummaryContractV1, NativeFullSessionProxyProfileV2,
    NativeFullSessionProxySummaryContractV2, NativeModelProxyBasisV2,
    NativeModelProxyConnectionBindingV2,
};
pub(super) fn summary_contract(
    input: &ApprovalAdmissionInput,
    current: &ApprovalAdmissionContext,
) -> Option<NativeCustodySummaryContractV1> {
    let expected = input
        .request
        .expected_model_budget_summary_contract()
        .unwrap();
    let CustodyIntentV1::TestSessionV2 { plan, .. } = &input.request.intent else {
        return expected;
    };
    let Some(budget) = plan.model_budget.as_ref().filter(|b| {
        b.approval_contract == ModelBudgetApprovalContractV1::NativeCustodyModelProxyV2
    }) else {
        return expected;
    };
    let Some(NativeCustodySummaryContractV1::FullTestSessionV2(full)) = expected else {
        panic!("full fixture");
    };
    let basis = NativeModelProxyBasisV2::for_request(
        &input.request,
        &NativeModelProxyConnectionBindingV2 {
            native_account_id: current.source.manager_account_id.clone(),
            native_instance_public_key: budget.native_instance_public_key.clone(),
            connection_ref_digest: "61".repeat(32),
            connection_generation: DecimalU64(1),
            endpoint_policy_digest: "62".repeat(32),
        },
        8000,
        9000,
    )
    .unwrap();
    Some(NativeCustodySummaryContractV1::FullTestSessionProxyV2(
        Box::new(NativeFullSessionProxySummaryContractV2 {
            api_version: 2,
            contract: NativeFullSessionProxyProfileV2::FullTestSessionProxyV2,
            full_session: Box::new(full),
            basis_digest: basis.digest().unwrap(),
            basis: Box::new(basis),
        }),
    ))
}
fn budget(proxy: bool) -> ModelInferenceBudgetV1 {
    ModelInferenceBudgetV1 {
        api_version: 1,
        approval_contract: if proxy {
            ModelBudgetApprovalContractV1::NativeCustodyModelProxyV2
        } else {
            ModelBudgetApprovalContractV1::NativeCustodyModelBudgetV1
        },
        native_account_id: "replaced-by-actual-manager".into(),
        native_public_key: "11".repeat(32),
        native_instance_public_key: ed25519_dalek::SigningKey::from_bytes(&[91; 32])
            .verifying_key()
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        ledger_id: "model-ledger".into(),
        preset_id: "fixed-preset".into(),
        preset_revision: DecimalU64(1),
        model: "fixed-model".into(),
        runtime_digest: "33".repeat(32),
        skill_digest: "44".repeat(32),
        tools_digest: "55".repeat(32),
        context_policy_digest: "66".repeat(32),
        physical_graph_digest: "77".repeat(32),
        probe_physical_http: [DecimalU64(10), DecimalU64(10)],
        inference_physical_http: DecimalU64(10),
        max_calls: DecimalU64(3),
        max_input_tokens: DecimalU64(100),
        max_output_tokens: DecimalU64(50),
        max_total_tokens: DecimalU64(450),
        max_concurrent: DecimalU64(1),
        deadline: DecimalU64(30000),
        price_policy: ModelPricePolicyV1 {
            currency: "USD".into(),
            input_micro_units_per_million: DecimalU64(1000000),
            output_micro_units_per_million: DecimalU64(2000000),
            max_total_micro_units: DecimalU64(600),
            quote_digest: "88".repeat(32),
            valid_until: DecimalU64(30000),
        },
    }
}
#[test]
fn session_v2_native_model_budget_arms_only_actual_consumed_new_proxy_contract() {
    for mode in 0..3 {
        let f = pending_fixture_with_model_budget(80, (mode > 0).then(|| budget(mode == 2)));
        let original = serde_json::to_value(&f.state).unwrap();
        let session = &original["test_sessions_v2"][&f.session_id];
        assert!(session["roots"].as_object().unwrap().is_empty());
        assert_eq!(session["consumed_at"], serde_json::json!("8004"));
        assert!(session.get("native_model_budget").is_some());
        if mode < 2 {
            assert!(session["native_model_budget"].is_null());
        } else {
            let model = &session["native_model_budget"];
            assert_eq!(model["calls_reserved"], 0);
            assert_eq!(model["tokens_reserved"], 0);
            assert_eq!(model["micro_units_reserved"], 0);
            assert!(
                model["budget_digest"]
                    .as_str()
                    .is_some_and(|s| s.len() == 64)
            );
        }
        let restored: AccountProvisionApprovalState =
            serde_json::from_value(original.clone()).unwrap();
        assert_eq!(restored, f.state);
        let mut missing = original.clone();
        missing["test_sessions_v2"][&f.session_id]
            .as_object_mut()
            .unwrap()
            .remove("native_model_budget");
        assert!(serde_json::from_value::<AccountProvisionApprovalState>(missing).is_err());
        if mode == 2 {
            for field in ["approved_input_digest", "basis_digest", "budget_digest"] {
                let mut changed = original.clone();
                changed["test_sessions_v2"][&f.session_id]["native_model_budget"][field] =
                    serde_json::json!("ff".repeat(32));
                assert!(serde_json::from_value::<AccountProvisionApprovalState>(changed).is_err());
            }
        }
    }
}
