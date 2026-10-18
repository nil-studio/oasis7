use super::kernel_wasm_sandbox_bridge::DynamicMicroDepotSandbox;
use super::*;
use oasis7_wasm_abi::ModuleLimits;
use std::sync::{Arc, Mutex};

pub(super) fn installed_micro_depot_with_measured_supply(
    inventory_units: i64,
    throughput_units: i64,
    service_units: i64,
) -> WorldKernel {
    let mut kernel = WorldKernel::new();
    kernel.submit_action(Action::RegisterLocation {
        location_id: "loc-measured".to_string(),
        name: "Measured Hub".to_string(),
        pos: pos(0, 0),
        profile: LocationProfile::default(),
    });
    kernel.step().expect("register measured-supply location");
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-measured".to_string(),
        location_id: "loc-measured".to_string(),
    });
    kernel.step().expect("register measured-supply owner");
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-measured".to_string(),
        },
        ResourceKind::Data,
        20,
    );

    kernel.submit_action(Action::InstallMicroDepot {
        installer_agent_id: "agent-measured".to_string(),
        facility_id: "depot-measured".to_string(),
        location_id: "loc-measured".to_string(),
        owner_claim_id: "claim-measured".to_string(),
        regional_blocker_receipt_id: "repair-logistics:loc-measured:1".to_string(),
        module_id: "regional.micro_depot".to_string(),
        module_version: "0.2.0".to_string(),
        wasm_hash: "hash-measured".to_string(),
        entrypoint: "evaluate_quote".to_string(),
        service_radius_cm: 250_000,
        supported_resource_kinds: vec!["data".to_string()],
    });
    kernel.step().expect("install measured-supply depot");

    // Canonical actions do not expose arbitrary inventory mutation. Build a deterministic
    // persisted-state fixture, then restore the process-local evaluator hook.
    let mut encoded = serde_json::to_value(&kernel).expect("serialize measured-supply fixture");
    let depot = &mut encoded["model"]["regional_infrastructure"]["depot-measured"];
    depot["inventory_revision"] = serde_json::json!(1);
    depot["available_units_by_kind"] = serde_json::json!({ "data": inventory_units });
    depot["throughput_limit_units_per_epoch"] = serde_json::json!(throughput_units);
    depot["throughput_epoch"] = serde_json::json!(0);
    depot["throughput_remaining_units"] = serde_json::json!(throughput_units);
    let mut kernel: WorldKernel =
        serde_json::from_value(encoded).expect("restore measured-supply fixture");
    let sandbox = Arc::new(Mutex::new(DynamicMicroDepotSandbox {
        resource_debits: Some(vec![MicroDepotProposalResourceDebit {
            resource_kind: ResourceKind::Data,
            units: service_units,
        }]),
        ..DynamicMicroDepotSandbox::default()
    }));
    kernel.set_micro_depot_wasm_module_evaluator(
        "regional.micro_depot",
        "hash-measured",
        "evaluate_quote",
        vec![0x00, 0x61, 0x73, 0x6d],
        ModuleLimits::unbounded(),
        sandbox,
    );
    kernel
}

pub(super) fn submit_measured_service(kernel: &mut WorldKernel) -> WorldEventKind {
    kernel.submit_action(Action::ServiceMicroDepotRepair {
        agent_id: "agent-measured".to_string(),
        facility_id: "depot-measured".to_string(),
        target_id: "loc-measured".to_string(),
        base_cost_class: MicroDepotPressureClass::Medium,
        base_risk_class: MicroDepotPressureClass::Low,
        blocker_type: Some("supply_missing".to_string()),
    });
    kernel.step().expect("execute measured service").kind
}

#[test]
fn micro_depot_service_debits_exact_facility_inventory_and_epoch_throughput() {
    let mut kernel = installed_micro_depot_with_measured_supply(5, 4, 2);
    let event = submit_measured_service(&mut kernel);
    assert!(matches!(
        event,
        WorldEventKind::MicroDepotServiceApplied { .. }
    ));

    let depot = kernel
        .model()
        .regional_infrastructure
        .get("depot-measured")
        .expect("measured depot remains");
    assert_eq!(depot.available_units_by_kind.get("data"), Some(&3));
    assert_eq!(depot.throughput_remaining_units, 2);
    assert_eq!(depot.inventory_revision, 2);
}

#[test]
fn micro_depot_service_receipt_reconciles_inventory_and_throughput_before_after() {
    let mut kernel = installed_micro_depot_with_measured_supply(5, 4, 2);
    let event = submit_measured_service(&mut kernel);
    let receipt = serde_json::to_value(event).expect("serialize measured service receipt");
    let data = receipt.get("data").expect("service receipt data");

    assert_eq!(data["evaluated_epoch"], 0);
    assert_eq!(data["inventory_revision_before"], 1);
    assert_eq!(data["inventory_revision_after"], 2);
    assert_eq!(data["resource_debits"][0]["resource_kind"], "data");
    assert_eq!(data["resource_debits"][0]["units"], 2);
    assert_eq!(data["inventory_units_before_by_kind"]["data"], 5);
    assert_eq!(data["inventory_units_after_by_kind"]["data"], 3);
    assert_eq!(data["throughput_units_before"], 4);
    assert_eq!(data["throughput_units_after"], 2);
}

#[test]
fn micro_depot_service_rejects_insufficient_facility_inventory_without_partial_debit() {
    let mut kernel = installed_micro_depot_with_measured_supply(1, 4, 2);
    let before = kernel
        .model()
        .regional_infrastructure
        .get("depot-measured")
        .expect("measured depot")
        .clone();

    let event = submit_measured_service(&mut kernel);
    let rejection = serde_json::to_string(&event).expect("serialize inventory rejection");
    assert!(matches!(event, WorldEventKind::ActionRejected { .. }));
    assert!(rejection.contains("DEPOT_INVENTORY_INSUFFICIENT"));
    assert!(rejection.contains("resource_kind=data"));
    assert!(rejection.contains("required_units=2"));
    assert!(rejection.contains("available_units=1"));
    assert_eq!(
        kernel
            .model()
            .regional_infrastructure
            .get("depot-measured")
            .expect("measured depot after rejection"),
        &before
    );
}

#[test]
fn micro_depot_service_defense_in_depth_rejects_synthetic_exhausted_throughput_state() {
    // Commissioning creates 8 inventory units and 16 throughput units, so inventory is the
    // reachable gameplay blocker. This synthetic persisted-state fixture protects the lower-level
    // throughput invariant against corrupted, migrated, or future independently refilled state.
    let mut kernel = installed_micro_depot_with_measured_supply(5, 1, 2);
    let before = kernel
        .model()
        .regional_infrastructure
        .get("depot-measured")
        .expect("measured depot")
        .clone();

    let event = submit_measured_service(&mut kernel);
    let rejection = serde_json::to_string(&event).expect("serialize throughput rejection");
    assert!(matches!(event, WorldEventKind::ActionRejected { .. }));
    assert!(rejection.contains("DEPOT_THROUGHPUT_EXHAUSTED"));
    assert!(rejection.contains("required_units=2"));
    assert!(rejection.contains("remaining_units=1"));
    assert_eq!(
        kernel
            .model()
            .regional_infrastructure
            .get("depot-measured")
            .expect("measured depot after rejection"),
        &before
    );
}
