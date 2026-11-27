use oasis7_simulator::{
    MicroDepotActionKind, MicroDepotDecision, MicroDepotDeltaClass, MicroDepotEffectPreview,
    MicroDepotPressureClass, MicroDepotProposal, MicroDepotProposalResourceDebit,
    MicroDepotQuotePreview, MicroDepotQuoteRequest,
};

#[test]
fn llm_agent_idle_prompt_omits_last_query_without_losing_recipe_coverage() {
    let behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());

    let prompt = behavior.user_prompt(&make_observation(), &[], 0, 4);

    assert!(prompt.contains("\"recipe_coverage\"") || prompt.contains("...(truncated)"));
    assert!(!prompt.contains("\"last_query\""));
}

#[test]
fn llm_agent_query_feedback_prompt_keeps_recipe_coverage_and_structured_availability() {
    let mut behavior = LlmAgentBehavior::new("agent-1", base_config(), MockClient::default());
    let request = MicroDepotQuoteRequest {
        agent_id: "agent-1".to_string(),
        facility_id: "depot-prompt".to_string(),
        target_id: "loc-prompt".to_string(),
        action_kind: MicroDepotActionKind::Repair,
        base_cost_class: MicroDepotPressureClass::Medium,
        base_risk_class: MicroDepotPressureClass::Low,
        blocker_type: Some("supply_missing".to_string()),
    };
    behavior.on_query_result(&AgentQueryResult::EvaluateMicroDepotQuote {
        request: request.clone(),
        available_units_by_kind: Some(BTreeMap::from([("data".to_string(), 5)])),
        result: Ok(MicroDepotQuotePreview {
            module_id: "regional.micro_depot".to_string(),
            wasm_hash: "hash-prompt".to_string(),
            entrypoint: "evaluate_quote".to_string(),
            proposal: MicroDepotProposal {
                action_id: 0,
                facility_id: request.facility_id.clone(),
                decision: MicroDepotDecision::Applicable,
                cost_delta_class: MicroDepotDeltaClass::MinorDecrease,
                risk_delta_class: MicroDepotDeltaClass::MinorDecrease,
                wait_delta_class: MicroDepotDeltaClass::MinorDecrease,
                blocker_change: Some("supply_gap_reduced".to_string()),
                consumed_resource_classes: Vec::new(),
                resource_debits: vec![MicroDepotProposalResourceDebit {
                    resource_kind: ResourceKind::Data,
                    units: 2,
                }],
                evaluated_inventory_revision: 3,
                evaluated_epoch: 7,
                explanation_code: "micro_depot_quote_available".to_string(),
                proposal_hash: "sha256:prompt".to_string(),
            },
            effect: MicroDepotEffectPreview {
                cost_delta_class: MicroDepotDeltaClass::MinorDecrease,
                risk_delta_class: MicroDepotDeltaClass::MinorDecrease,
                wait_delta_class: MicroDepotDeltaClass::MinorDecrease,
                blocker_change: Some("supply_gap_reduced".to_string()),
                consumed_resource_classes: Vec::new(),
                explanation_code: "micro_depot_quote_available".to_string(),
            },
        }),
    });

    let prompt = behavior.user_prompt(&make_observation(), &[], 0, 4);

    assert!(prompt.contains("\"recipe_coverage\"") || prompt.contains("...(truncated)"));
    assert!(prompt.contains("\"last_query\""));
    assert!(prompt.contains("\"available_units_by_kind\":{\"data\":5}"));
}
