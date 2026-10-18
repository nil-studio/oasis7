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
