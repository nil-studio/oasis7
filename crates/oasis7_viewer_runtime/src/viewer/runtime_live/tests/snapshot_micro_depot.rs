use super::*;

const SNAPSHOT_PLAYER_ID: &str = "player-snapshot";

#[test]
fn compat_snapshot_exposes_micro_depot_facility_state_and_evidence() {
    let _guard = runtime_provider_env_lock().lock().expect("env lock");
    let mut server =
        ViewerRuntimeLiveServer::new(ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal))
            .expect("runtime server");
    let mut model = WorldModel::default();
    model.regional_infrastructure.insert(
        "depot-compat".to_string(),
        oasis7_simulator::RegionalInfrastructure {
            facility_id: "depot-compat".to_string(),
            kind: "micro_depot".to_string(),
            location_id: "loc-compat".to_string(),
            owner: oasis7_simulator::ResourceOwner::Agent {
                agent_id: "agent-0".to_string(),
            },
            owner_claim_id: "claim-compat".to_string(),
            regional_blocker_receipt_id: "blocker-compat".to_string(),
            status: "active".to_string(),
            module_id: "regional.micro_depot".to_string(),
            module_version: "0.2.0".to_string(),
            wasm_hash: "sha256:micro-depot-compat".to_string(),
            entrypoint: "evaluate_quote".to_string(),
            service_radius_cm: 250_000,
            supported_resource_kinds: vec!["data".to_string()],
            measured_supply_schema_version: 2,
            inventory_revision: 7,
            available_units_by_kind: [("data".to_string(), 5)].into_iter().collect(),
            throughput_limit_units_per_epoch: 16,
            throughput_epoch: 11,
            throughput_remaining_units: 13,
            upkeep_paid: true,
            last_proposal_hash: Some("sha256:proposal-compat".to_string()),
            last_receipt_id: Some("receipt-compat".to_string()),
            last_serviced_target_id: None,
        },
    );
    server.seed_model = Some(model);

    let snapshot = server.compat_snapshot(Some(SNAPSHOT_PLAYER_ID));
    let facility = snapshot
        .player_gameplay
        .as_ref()
        .expect("player gameplay snapshot")
        .micro_depot_facilities
        .first()
        .expect("micro-depot facility in canonical gameplay snapshot");
    assert_eq!(facility.facility_id, "depot-compat");
    assert_eq!(facility.available_units_by_kind.get("data"), Some(&5));
    assert_eq!(facility.module_id, "regional.micro_depot");
    assert_eq!(facility.module_version, "0.2.0");
    assert_eq!(facility.wasm_hash, "sha256:micro-depot-compat");
    assert_eq!(facility.last_receipt_id.as_deref(), Some("receipt-compat"));
    assert_eq!(
        facility.last_proposal_hash.as_deref(),
        Some("sha256:proposal-compat")
    );
}
