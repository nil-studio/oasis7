use super::*;

#[test]
fn conflicting_factory_recycle_replay_rejects_without_mutation_after_receipt() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    build_factory_ready(
        &mut world,
        "builder-a",
        "site-recycle-conflict",
        factory_spec("factory.recycle-conflict", 1, 1, 1),
    );
    world.submit_action(Action::RecycleFactory {
        operator_agent_id: "builder-a".to_string(),
        factory_id: "factory.recycle-conflict".to_string(),
    });
    world.step().expect("recycle factory");
    let recycled = world
        .journal()
        .events
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(event @ DomainEvent::FactoryRecycled { .. }) => {
                Some(event.clone())
            }
            _ => None,
        })
        .expect("recycle event");
    let mut conflicting = recycled;
    if let DomainEvent::FactoryRecycled { recovered, .. } = &mut conflicting {
        recovered.push(MaterialStack::new("tampered", 1));
    }
    let mut replay = world.state().clone();
    let before = serde_json::to_vec(&replay).expect("serialize retired state");
    let result = replay.apply_domain_event(&conflicting, replay.time);
    assert!(
        matches!(result, Err(WorldError::ResourceBalanceInvalid { .. })),
        "same-factory conflicting recycle must be rejected: {result:?}"
    );
    assert_eq!(
        serde_json::to_vec(&replay).expect("serialize after conflicting recycle"),
        before,
        "conflicting recycle must not mutate retired state"
    );
}

#[test]
fn construction_receipt_persists_electricity_and_material_before_after_facts() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    let spec = factory_spec("factory.construction-receipt", 2, 1, 1);
    let builder_ledger = MaterialLedgerId::agent("builder-a");
    for stack in &spec.build_cost {
        world
            .set_ledger_material_balance(builder_ledger.clone(), stack.kind.as_str(), stack.amount)
            .expect("seed construction material");
    }
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 10)
        .expect("seed construction electricity");
    install_factory_authority(
        &mut world,
        "builder-a",
        "site-construction-receipt",
        spec.factory_id.as_str(),
        10,
    );
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-construction-receipt".to_string(),
        spec,
    });
    world.step().expect("start construction");
    let build = world
        .state()
        .pending_factory_builds
        .values()
        .next()
        .expect("pending construction");
    let obligation = build
        .construction_power_obligation
        .as_ref()
        .expect("construction receipt");
    assert_eq!(obligation.electricity_before, Some(10));
    assert_eq!(obligation.electricity_after, Some(0));
    assert_eq!(
        obligation
            .material_balances_before
            .as_ref()
            .and_then(|balances| balances.get("steel_plate")),
        Some(&10)
    );
    assert_eq!(
        obligation
            .material_balances_after
            .as_ref()
            .and_then(|balances| balances.get("steel_plate")),
        Some(&0)
    );
    world.step().expect("advance construction");
    world.step().expect("settle construction");
    assert_eq!(
        world
            .state()
            .factory_construction_receipts
            .get("factory.construction-receipt")
            .and_then(|receipt| receipt.electricity_before),
        Some(10)
    );
}
