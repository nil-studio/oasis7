use super::super::events::Action;
use super::super::{ModuleRuntimeChargeEvent, WorldEventBody};
use super::{World, WorldError};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_wasm_abi::ModuleStateUpdate;

fn charge_for(payer: &str, compute: i64, electricity: i64) -> ModuleRuntimeChargeEvent {
    ModuleRuntimeChargeEvent {
        module_id: "m.compat".into(),
        trace_id: "trace-compat".into(),
        payer_agent_id: payer.into(),
        compute_fee_kind: ResourceKind::Data,
        compute_fee_amount: compute,
        electricity_fee_kind: ResourceKind::Electricity,
        electricity_fee_amount: electricity,
        input_bytes: 1,
        output_bytes: 1,
        effect_count: 0,
        emit_count: 0,
    }
}

fn world_with_payer(agent_id: &str) -> World {
    let mut world = World::new();
    world.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: crate::tests::pos(0, 0),
    });
    world.step().expect("register module payer");
    world
        .set_agent_resource_balance(agent_id, ResourceKind::Data, 10)
        .expect("seed payer data");
    world
        .set_agent_resource_balance(agent_id, ResourceKind::Electricity, 10)
        .expect("seed payer electricity");
    world
}

fn assert_publication_unchanged(
    world: &World,
    snapshot_before: &super::super::Snapshot,
    journal_before: &super::super::Journal,
    consensus_before: &[super::super::TickConsensusRecord],
    backpressure_before: &super::WorldRuntimeBackpressureStats,
    event_allocator_before: (u64, u64),
) {
    assert_eq!(world.snapshot(), *snapshot_before);
    assert_eq!(world.journal(), journal_before);
    assert_eq!(world.tick_consensus_records(), consensus_before);
    assert_eq!(world.runtime_backpressure_stats(), backpressure_before);
    assert_eq!(
        (world.next_event_id, world.next_event_id_era),
        event_allocator_before
    );
}

#[test]
fn module_runtime_charge_is_atomic_when_electricity_debit_is_insufficient() {
    let payer = "module-charge-electricity-payer";
    let mut world = world_with_payer(payer);
    world
        .set_agent_resource_balance(payer, ResourceKind::Electricity, 1)
        .expect("set insufficient electricity");
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let backpressure_before = world.runtime_backpressure_stats().clone();
    let event_allocator_before = (world.next_event_id, world.next_event_id_era);

    let error = world
        .apply_module_runtime_charge_event(
            &ModuleRuntimeChargeEvent {
                module_id: "m.atomic-charge".to_string(),
                trace_id: "trace-electricity-shortfall".to_string(),
                payer_agent_id: payer.to_string(),
                compute_fee_kind: ResourceKind::Data,
                compute_fee_amount: 3,
                electricity_fee_kind: ResourceKind::Electricity,
                electricity_fee_amount: 2,
                input_bytes: 1,
                output_bytes: 1,
                effect_count: 0,
                emit_count: 0,
            },
            world.state.time.saturating_add(7),
        )
        .expect_err("insufficient electricity must reject before any debit is visible");
    assert!(matches!(error, WorldError::ResourceBalanceInvalid { .. }));
    assert_publication_unchanged(
        &world,
        &snapshot_before,
        &journal_before,
        consensus_before.as_slice(),
        &backpressure_before,
        event_allocator_before,
    );
}

#[test]
fn module_runtime_charge_is_atomic_when_shared_fee_kind_is_insufficient_in_aggregate() {
    let payer = "module-charge-shared-kind-payer";
    let mut world = world_with_payer(payer);
    world
        .set_agent_resource_balance(payer, ResourceKind::Data, 5)
        .expect("set shared-kind balance");
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let backpressure_before = world.runtime_backpressure_stats().clone();
    let event_allocator_before = (world.next_event_id, world.next_event_id_era);

    let error = world
        .apply_module_runtime_charge_event(
            &ModuleRuntimeChargeEvent {
                module_id: "m.atomic-shared-kind".to_string(),
                trace_id: "trace-shared-kind-shortfall".to_string(),
                payer_agent_id: payer.to_string(),
                compute_fee_kind: ResourceKind::Data,
                compute_fee_amount: 3,
                electricity_fee_kind: ResourceKind::Data,
                electricity_fee_amount: 4,
                input_bytes: 1,
                output_bytes: 1,
                effect_count: 0,
                emit_count: 0,
            },
            world.state.time.saturating_add(11),
        )
        .expect_err("shared fee kind aggregate must reject before any debit is visible");
    assert!(matches!(error, WorldError::ResourceBalanceInvalid { .. }));
    assert_publication_unchanged(
        &world,
        &snapshot_before,
        &journal_before,
        consensus_before.as_slice(),
        &backpressure_before,
        event_allocator_before,
    );
}

#[test]
fn module_state_update_append_post_prepare_failure_preserves_all_observables() {
    let mut world = World::new();
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let backpressure_before = world.runtime_backpressure_stats().clone();
    let event_allocator_before = (world.next_event_id, world.next_event_id_era);

    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .append_event(
            WorldEventBody::ModuleStateUpdated(ModuleStateUpdate {
                module_id: "m.append-state".to_string(),
                trace_id: "trace-append-state".to_string(),
                state: vec![1, 2, 3],
            }),
            None,
        )
        .expect_err("ModuleStateUpdated append must honor post-prepare failpoint");
    assert!(matches!(error, WorldError::ResourceBalanceInvalid { .. }));
    assert_publication_unchanged(
        &world,
        &snapshot_before,
        &journal_before,
        consensus_before.as_slice(),
        &backpressure_before,
        event_allocator_before,
    );
}

#[test]
fn module_runtime_charge_append_post_prepare_failure_preserves_all_observables() {
    let payer = "module-append-charge-payer";
    let mut world = world_with_payer(payer);
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();
    let consensus_before = world.tick_consensus_records().to_vec();
    let backpressure_before = world.runtime_backpressure_stats().clone();
    let event_allocator_before = (world.next_event_id, world.next_event_id_era);

    world.fail_next_append_after_publication_prepare_for_test();
    let error = world
        .append_event(
            WorldEventBody::ModuleRuntimeCharged(ModuleRuntimeChargeEvent {
                module_id: "m.append-charge".to_string(),
                trace_id: "trace-append-charge".to_string(),
                payer_agent_id: payer.to_string(),
                compute_fee_kind: ResourceKind::Data,
                compute_fee_amount: 2,
                electricity_fee_kind: ResourceKind::Electricity,
                electricity_fee_amount: 3,
                input_bytes: 1,
                output_bytes: 1,
                effect_count: 0,
                emit_count: 0,
            }),
            None,
        )
        .expect_err("ModuleRuntimeCharged append must honor post-prepare failpoint");
    assert!(matches!(error, WorldError::ResourceBalanceInvalid { .. }));
    assert_publication_unchanged(
        &world,
        &snapshot_before,
        &journal_before,
        consensus_before.as_slice(),
        &backpressure_before,
        event_allocator_before,
    );
}

#[test]
fn shared_fee_kind_success_preserves_sequential_debits_and_saturating_treasury() {
    let mut world = world_with_payer("payer");
    world
        .state
        .resources
        .insert(ResourceKind::Data, i64::MAX - 2);
    let mut charge = charge_for("payer", 3, 4);
    charge.electricity_fee_kind = ResourceKind::Data;
    let now = world.state.time + 9;
    world
        .apply_module_runtime_charge_event(&charge, now)
        .unwrap();
    assert_eq!(
        world
            .agent_resource_balance("payer", ResourceKind::Data)
            .unwrap(),
        3
    );
    assert_eq!(world.resource_balance(ResourceKind::Data), i64::MAX);
    assert_eq!(
        world
            .agent_resource_balance("payer", ResourceKind::Electricity)
            .unwrap(),
        10
    );
    assert_eq!(world.state.agents["payer"].last_active, now);
}

#[test]
fn zero_runtime_fees_only_refresh_last_active() {
    let mut world = world_with_payer("payer");
    let resources_before = world.state.agents["payer"].state.resources.clone();
    let treasury_before = world.state.resources.clone();
    let now = world.state.time + 13;
    world
        .apply_module_runtime_charge_event(&charge_for("payer", 0, 0), now)
        .unwrap();
    assert_eq!(
        world.state.agents["payer"].state.resources,
        resources_before
    );
    assert_eq!(world.state.resources, treasury_before);
    assert_eq!(world.state.agents["payer"].last_active, now);
}

#[test]
fn runtime_charge_validation_retains_negative_missing_payer_and_compute_error_priority() {
    let mut world = world_with_payer("payer");
    let before = world.snapshot();
    for charge in [charge_for("missing", -1, 2), charge_for("missing", 2, -1)] {
        let error = world
            .apply_module_runtime_charge_event(&charge, 99)
            .unwrap_err();
        assert!(
            matches!(error, WorldError::ResourceBalanceInvalid { reason } if reason.contains("fee must be >= 0"))
        );
    }
    assert!(
        matches!(world.apply_module_runtime_charge_event(&charge_for("missing", 0, 0), 99), Err(WorldError::AgentNotFound { agent_id }) if agent_id == "missing")
    );
    let error = world
        .apply_module_runtime_charge_event(&charge_for("payer", 11, 11), 99)
        .unwrap_err();
    assert!(
        matches!(error, WorldError::ResourceBalanceInvalid { reason } if reason.contains("compute fee debit failed"))
    );
    assert_eq!(world.snapshot(), before);
}

#[test]
fn module_state_append_inserts_replaces_and_replays_to_same_root() {
    let mut world = World::new();
    let baseline = world.snapshot();
    for (index, state) in [vec![1, 2], vec![3, 4, 5]].into_iter().enumerate() {
        let id = world
            .append_event(
                WorldEventBody::ModuleStateUpdated(ModuleStateUpdate {
                    module_id: "m.compat-state".into(),
                    trace_id: format!("state-{index}"),
                    state: state.clone(),
                }),
                None,
            )
            .unwrap();
        assert_eq!(id, index as u64 + 1);
        assert_eq!(world.state.module_states["m.compat-state"], state);
        assert_eq!(
            world
                .tick_consensus_records()
                .last()
                .unwrap()
                .block
                .header
                .state_root,
            world.current_state_root_hash().unwrap(),
            "published root must match installed module state after update {index}"
        );
    }
    let replayed = World::from_snapshot(baseline, world.journal().clone()).unwrap();
    assert_eq!(replayed.state.module_states, world.state.module_states);
    assert_eq!(
        replayed.current_state_root_hash().unwrap(),
        world.current_state_root_hash().unwrap()
    );
    assert_eq!(replayed.next_event_id, world.next_event_id);
}

#[test]
fn charge_append_retry_publishes_once_and_replays_to_same_root() {
    let mut world = world_with_payer("payer");
    let baseline = world.snapshot();
    let next_id = world.next_event_id;
    let charge = charge_for("payer", 2, 3);
    world.fail_next_append_after_publication_prepare_for_test();
    assert!(
        world
            .append_event(WorldEventBody::ModuleRuntimeCharged(charge.clone()), None)
            .is_err()
    );
    let id = world
        .append_event(WorldEventBody::ModuleRuntimeCharged(charge), None)
        .unwrap();
    assert_eq!(id, next_id);
    assert_eq!(world.journal().events.len(), baseline.journal_len + 1);
    assert_eq!(
        world
            .tick_consensus_records()
            .last()
            .unwrap()
            .block
            .header
            .state_root,
        world.current_state_root_hash().unwrap(),
        "published root must match installed charge after retry"
    );
    assert_eq!(
        world
            .agent_resource_balance("payer", ResourceKind::Data)
            .unwrap(),
        8
    );
    assert_eq!(
        world
            .agent_resource_balance("payer", ResourceKind::Electricity)
            .unwrap(),
        7
    );
    let replayed = World::from_snapshot(baseline, world.journal().clone()).unwrap();
    assert_eq!(replayed.state.agents, world.state.agents);
    assert_eq!(replayed.state.resources, world.state.resources);
    assert_eq!(
        replayed.current_state_root_hash().unwrap(),
        world.current_state_root_hash().unwrap()
    );
    assert_eq!(replayed.next_event_id, world.next_event_id);
}
