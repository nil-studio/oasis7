use super::*;

#[test]
fn schedule_recipe_does_not_fallback_to_world_material_for_moderate_site_shortage() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    build_factory_ready(
        &mut world,
        "builder-a",
        "site-1",
        factory_spec("factory.scarcity.moderate", 1, 1, 1),
    );
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 6)
        .expect("seed partial local bottleneck");
    world
        .set_material_balance("iron_ingot", 20)
        .expect("seed world bottleneck");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 20)
        .expect("seed builder electricity");
    world.set_resource_balance(ResourceKind::Electricity, 20);
    let site_ledger = MaterialLedgerId::site("site-1");
    let site_material_before = world.ledger_material_balance(&site_ledger, "iron_ingot");
    let world_material_before = world.material_balance("iron_ingot");
    let journal_start = world.journal().events.len();
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.scarcity.moderate".to_string(),
        recipe_id: "recipe.scarcity.moderate".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![MaterialStack::new("iron_ingot", 10)],
            vec![MaterialStack::new("motor_mk1", 1)],
            Vec::new(),
            1,
            3,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world
        .step()
        .expect("moderate site shortage should become a structured rejection");
    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert_eq!(
        world.ledger_material_balance(&site_ledger, "iron_ingot"),
        site_material_before,
        "moderate site shortage must not consume local material"
    );
    assert_eq!(
        world.material_balance("iron_ingot"),
        world_material_before,
        "moderate site shortage must not fall back to world material"
    );
    let rejection = world.journal().events[journal_start..]
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => Some(reason),
            _ => None,
        })
        .expect("moderate site shortage rejection event");
    match rejection {
        RejectReason::InsufficientMaterial {
            material_kind,
            requested,
            available,
        } => {
            assert_eq!(material_kind, "iron_ingot");
            assert_eq!(*requested, 10);
            assert_eq!(*available, 6);
        }
        other => panic!("expected site-bound material rejection, got {other:?}"),
    }
}

#[test]
fn schedule_recipe_does_not_fallback_to_world_material_for_severe_site_shortage() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    build_factory_ready(
        &mut world,
        "builder-a",
        "site-1",
        factory_spec("factory.scarcity.severe", 1, 1, 1),
    );
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 2)
        .expect("seed severe local bottleneck");
    world
        .set_material_balance("iron_ingot", 20)
        .expect("seed world bottleneck");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 20)
        .expect("seed builder electricity");
    world.set_resource_balance(ResourceKind::Electricity, 20);
    let site_ledger = MaterialLedgerId::site("site-1");
    let site_material_before = world.ledger_material_balance(&site_ledger, "iron_ingot");
    let world_material_before = world.material_balance("iron_ingot");
    let journal_start = world.journal().events.len();
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.scarcity.severe".to_string(),
        recipe_id: "recipe.scarcity.severe".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![MaterialStack::new("iron_ingot", 10)],
            vec![MaterialStack::new("motor_mk1", 1)],
            Vec::new(),
            1,
            3,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world
        .step()
        .expect("severe site shortage should become a structured rejection");
    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert_eq!(
        world.ledger_material_balance(&site_ledger, "iron_ingot"),
        site_material_before,
        "severe site shortage must not consume local material"
    );
    assert_eq!(
        world.material_balance("iron_ingot"),
        world_material_before,
        "severe site shortage must not fall back to world material"
    );
    let rejection = world.journal().events[journal_start..]
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => Some(reason),
            _ => None,
        })
        .expect("severe site shortage rejection event");
    match rejection {
        RejectReason::InsufficientMaterial {
            material_kind,
            requested,
            available,
        } => {
            assert_eq!(material_kind, "iron_ingot");
            assert_eq!(*requested, 10);
            assert_eq!(*available, 2);
        }
        other => panic!("expected site-bound material rejection, got {other:?}"),
    }
}
