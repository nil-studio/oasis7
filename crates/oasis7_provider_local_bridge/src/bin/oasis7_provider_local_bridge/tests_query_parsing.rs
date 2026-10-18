use super::*;
use oasis7::simulator::ActionCatalogEntry;
use oasis7_simulator::{Action, AgentQuery};

#[test]
fn parse_model_decision_maps_quote_micro_depot_install_from_serialized_action() {
    let mut request = sample_request();
    request
        .observation
        .action_catalog
        .push(ActionCatalogEntry::new(
            "quote_micro_depot_install",
            "preview a micro-depot install without submitting it",
        ));
    let install = Action::InstallMicroDepot {
        installer_agent_id: "agent-1".to_string(),
        facility_id: "depot-quote".to_string(),
        location_id: "loc-1".to_string(),
        owner_claim_id: "claim-quote".to_string(),
        regional_blocker_receipt_id: "repair-logistics:loc-1:1".to_string(),
        module_id: "regional.micro_depot".to_string(),
        module_version: "0.2.0".to_string(),
        wasm_hash: "quote-hash".to_string(),
        entrypoint: "evaluate_quote".to_string(),
        service_radius_cm: 250_000,
        supported_resource_kinds: vec!["data".to_string()],
    };
    let raw = serde_json::json!({
        "decision": "query",
        "action_ref": "quote_micro_depot_install",
        "args": install,
    })
    .to_string();

    let (decision, repairs) =
        parse_model_decision("agent-1", &request, raw.as_str()).expect("parse quote query");

    assert_eq!(repairs, 0);
    assert_eq!(
        decision,
        ProviderDecision::Query {
            query_ref: "quote_micro_depot_install".to_string(),
            query: AgentQuery::QuoteMicroDepotInstall(install),
        }
    );
}

#[test]
fn parse_model_decision_rejects_non_install_quote_payload() {
    let mut request = sample_request();
    request
        .observation
        .action_catalog
        .push(ActionCatalogEntry::new(
            "quote_micro_depot_install",
            "preview a micro-depot install without submitting it",
        ));
    let raw = serde_json::json!({
        "decision": "query",
        "action_ref": "quote_micro_depot_install",
        "args": Action::MoveAgent {
            agent_id: "agent-1".to_string(),
            to: "loc-1".to_string(),
        },
    })
    .to_string();

    let error = parse_model_decision("agent-1", &request, raw.as_str())
        .expect_err("non-install quote payload must be rejected");

    assert!(error.contains("quote_micro_depot_install requires an InstallMicroDepot action"));
}
