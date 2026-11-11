//! Additional genuine FIFO boundary fixtures; expectations come from the old World.
use super::*;
fn world_with_factory() -> World {
    let mut world = World::new();
    for id in ["owner", "other"] {
        let mut agent = oasis7_world_state::agent_state::AgentState::new(
            id,
            oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
        );
        agent
            .resources
            .set(
                oasis7_agent_api::world_action::ResourceKind::Electricity,
                10,
            )
            .unwrap();
        world
            .state
            .agents
            .insert(id.into(), crate::AgentCell::new(agent, 0));
    }
    let source = industrial_eligibility::producing_world();
    let mut factory = source.state.factories["assembler"].clone();
    factory.output_ledger = oasis7_world_state::MaterialLedgerId::factory("salvage");
    world.state.factories.insert("assembler".into(), factory);
    world
}
fn maintain(actor: &str, id: &str, parts: i64) -> crate::Action {
    crate::Action::MaintainFactory {
        operator_agent_id: actor.into(),
        factory_id: id.into(),
        parts,
    }
}
fn recycle(actor: &str, id: &str) -> crate::Action {
    crate::Action::RecycleFactory {
        operator_agent_id: actor.into(),
        factory_id: id.into(),
    }
}
#[test]
fn queued_factory_rejection_priority_and_future_pending_recipe_match_legacy() {
    let mut world = world_with_factory();
    // Pending jobs are busy even when they are not currently producing/ready.
    let job=serde_json::from_value(serde_json::json!({"job_id":73,"requester_agent_id":"owner","factory_id":"assembler","recipe_id":"recipe","accepted_batches":1,"consume":[],"produce":[],"byproducts":[],"power_required":0,"duration_ticks":1,"consume_ledger":"world","output_ledger":"world","bottleneck_tags":[],"logistics_route_ids":[],"logistics_path_ids":[],"ready_at":u64::MAX})).unwrap();
    world.state.pending_recipe_jobs.insert(73, job);
    for action in [
        maintain("missing", "absent", 0),
        maintain("missing", "absent", 1),
        maintain("owner", "absent", 1),
        maintain("other", "assembler", 1),
        maintain("owner", "assembler", 1),
        recycle("missing", "absent"),
        recycle("owner", "absent"),
        recycle("other", "assembler"),
        recycle("owner", "assembler"),
    ] {
        world.submit_action(action);
    }
    assert_factory_fifo(world);
}
#[test]
fn queued_factory_requested_fallback_and_actual_consumption_match_legacy() {
    let mut world = world_with_factory();
    let preferred = oasis7_world_state::MaterialLedgerId::agent("owner");
    let factory = world.state.factories.get_mut("assembler").unwrap();
    factory.durability_ppm = 999_999;
    factory.input_ledger = preferred.clone();
    world
        .state
        .material_ledgers
        .entry(preferred)
        .or_default()
        .insert("hardware_part".into(), 1);
    world.state.materials.insert("hardware_part".into(), 2);
    world
        .state
        .material_ledgers
        .entry(oasis7_world_state::MaterialLedgerId::World)
        .or_default()
        .insert("hardware_part".into(), 2);
    // Request two cannot use preferred=1, so World pays only one consumed part.
    world.submit_action(maintain("owner", "assembler", 2));
    world.submit_action(maintain("owner", "assembler", 1));
    assert_factory_fifo(world);
}
#[test]
fn queued_factory_duplicate_salvage_keeps_stack_order_and_flooring() {
    let mut world = world_with_factory();
    let factory = world.state.factories.get_mut("assembler").unwrap();
    factory.spec.build_cost = vec![
        oasis7_world_state::MaterialStack::new("ore", 1),
        oasis7_world_state::MaterialStack::new("water", 3),
        oasis7_world_state::MaterialStack::new("ore", 1),
        oasis7_world_state::MaterialStack::new("water", 4),
    ];
    world.submit_action(recycle("owner", "assembler"));
    assert_factory_fifo(world);
}

#[test]
fn queued_factory_matching_retired_receipt_replays_without_touch_or_output_group() {
    let mut world = world_with_factory();
    world
        .state
        .factories
        .get_mut("assembler")
        .unwrap()
        .spec
        .build_cost
        .clear();
    world.state.retired_factory_ids.insert("assembler".into());
    world.state.factory_recycle_receipts.insert(
        "assembler".into(),
        oasis7_world_state::FactoryRecycleReceiptV1 {
            operator_agent_id: "owner".into(),
            factory_id: "assembler".into(),
            recycle_ledger: oasis7_world_state::MaterialLedgerId::factory("salvage"),
            recovered: vec![],
            durability_ppm: 1_000_000,
        },
    );
    world.submit_action(recycle("owner", "assembler"));
    world.submit_action(recycle("owner", "assembler"));
    assert_factory_fifo(world);
}
