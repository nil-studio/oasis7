use super::pos;
use crate::{
    Action, AgentLocationAuthorityV1, DomainEvent, FactoryConstructionPowerMode,
    FactoryConstructionPowerProfileV1, FactoryProfileV1, FactorySiteAuthorityV1, LocationAnchorV1,
    MaterialLedgerId, RejectReason, World, WorldError, WorldEventBody,
};
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_wasm_abi::{FactoryModuleSpec, MaterialStack, RecipeExecutionPlan};

pub(super) fn factory_spec(
    factory_id: &str,
    build_time_ticks: u32,
    recipe_slots: u16,
    maintenance_per_tick: i64,
) -> FactoryModuleSpec {
    FactoryModuleSpec {
        factory_id: factory_id.to_string(),
        display_name: "Lifecycle Factory".to_string(),
        tier: 1,
        tags: vec!["lifecycle".to_string()],
        build_cost: vec![
            MaterialStack::new("steel_plate", 10),
            MaterialStack::new("circuit_board", 2),
        ],
        build_time_ticks,
        base_power_draw: 5,
        recipe_slots,
        throughput_bps: 10_000,
        maintenance_per_tick,
    }
}

pub(super) fn register_builder(world: &mut World, agent_id: &str) {
    world.submit_action(Action::RegisterAgent {
        agent_id: agent_id.to_string(),
        pos: pos(0, 0),
    });
    world.step().expect("register builder");
}

pub(super) fn install_factory_authority(
    world: &mut World,
    builder_agent_id: &str,
    site_id: &str,
    factory_id: &str,
    construction_power: i64,
) {
    let location_id = format!("location-{site_id}");
    if !world.state().location_anchors.contains_key(&location_id) {
        world
            .set_location_anchor(LocationAnchorV1 {
                location_id: location_id.clone(),
                active: true,
                authority_revision: 1,
                effective_at: 0,
            })
            .expect("install location anchor");
    }
    let location_revision = world
        .state()
        .agent_location_authorities
        .get(builder_agent_id)
        .map_or(1, |authority| {
            authority.authority_revision.saturating_add(1)
        });
    world
        .set_agent_location_authority(AgentLocationAuthorityV1 {
            agent_id: builder_agent_id.to_string(),
            location_id: location_id.clone(),
            active: true,
            authority_revision: location_revision,
            effective_at: 0,
        })
        .expect("install agent location authority");
    let site_revision = world
        .state()
        .factory_site_authorities
        .get(site_id)
        .map_or(1, |authority| {
            authority.authority_revision.saturating_add(1)
        });
    world
        .set_factory_site_authority(FactorySiteAuthorityV1 {
            site_id: site_id.to_string(),
            location_id,
            owner_agent_id: builder_agent_id.to_string(),
            authorized_agent_ids: Vec::new(),
            chunk_ready: true,
            active: true,
            authority_revision: site_revision,
            registered_at: 0,
        })
        .expect("install factory site authority");
    world
        .set_factory_construction_power_profile(FactoryConstructionPowerProfileV1 {
            factory_id: factory_id.to_string(),
            factory_kind: "lifecycle".to_string(),
            source_module_id: None,
            electricity_amount: construction_power,
            mode: FactoryConstructionPowerMode::StartOnlySink,
            authority_revision: 1,
            active: true,
        })
        .expect("install construction power profile");
    world
        .upsert_factory_profile(FactoryProfileV1 {
            factory_id: factory_id.to_string(),
            tier: 1,
            recipe_slots: 1,
            tags: vec!["lifecycle".to_string()],
        })
        .expect("install factory capability profile");
}

fn build_factory_ready(
    world: &mut World,
    builder_agent_id: &str,
    site_id: &str,
    spec: FactoryModuleSpec,
) {
    let builder_ledger = MaterialLedgerId::agent(builder_agent_id);
    for stack in &spec.build_cost {
        world
            .set_ledger_material_balance(builder_ledger.clone(), stack.kind.as_str(), stack.amount)
            .expect("seed builder construction material");
    }
    const CONSTRUCTION_POWER: i64 = 10;
    world
        .set_agent_resource_balance(
            builder_agent_id,
            ResourceKind::Electricity,
            CONSTRUCTION_POWER,
        )
        .expect("seed builder construction power");
    install_factory_authority(
        world,
        builder_agent_id,
        site_id,
        spec.factory_id.as_str(),
        CONSTRUCTION_POWER,
    );
    world
        .upsert_factory_profile(FactoryProfileV1 {
            factory_id: spec.factory_id.clone(),
            tier: spec.tier,
            recipe_slots: spec.recipe_slots,
            tags: spec.tags.clone(),
        })
        .expect("install exact factory capability profile");
    world.submit_action(Action::BuildFactory {
        builder_agent_id: builder_agent_id.to_string(),
        site_id: site_id.to_string(),
        spec,
    });
    world.step().expect("start build");
    world.step().expect("complete build");
}

#[test]
fn build_factory_duplicate_material_stacks_reject_aggregate_cost_atomically() {
    const CONSTRUCTION_POWER: i64 = 10;
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    let mut spec = factory_spec("factory.duplicate-build-cost", 1, 1, 1);
    spec.build_cost = vec![
        MaterialStack::new("steel_plate", 6),
        MaterialStack::new("steel_plate", 6),
    ];
    let builder_ledger = MaterialLedgerId::agent("builder-a");
    world
        .set_ledger_material_balance(builder_ledger.clone(), "steel_plate", 10)
        .expect("seed aggregate construction material");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, CONSTRUCTION_POWER)
        .expect("seed construction power");
    install_factory_authority(
        &mut world,
        "builder-a",
        "site-duplicate-build-cost",
        spec.factory_id.as_str(),
        CONSTRUCTION_POWER,
    );
    let journal_start = world.journal().events.len();

    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-duplicate-build-cost".to_string(),
        spec,
    });
    world
        .step()
        .expect("duplicate construction stacks should become a structured rejection");

    assert_eq!(world.pending_factory_builds_len(), 0);
    assert!(!world.has_factory("factory.duplicate-build-cost"));
    assert_eq!(
        world.ledger_material_balance(&builder_ledger, "steel_plate"),
        10,
        "aggregate rejection must not consume construction material"
    );
    assert!(world.journal().events[journal_start..].iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::InsufficientMaterial {
                    material_kind,
                    requested,
                    available,
                },
                ..
            }) if material_kind == "steel_plate" && *requested == 12 && *available == 10
        )
    }));
}

#[path = "economy_factory_lifecycle/depreciation_and_maintenance.rs"]
mod depreciation_and_maintenance;

#[path = "economy_factory_lifecycle/site_material_authority.rs"]
mod site_material_authority;

#[path = "economy_factory_lifecycle/receipts.rs"]
mod receipts;

#[test]
fn build_factory_rejects_site_unknown_without_material_or_power_sink() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    world
        .set_ledger_material_balance(MaterialLedgerId::agent("builder-a"), "steel_plate", 20)
        .expect("seed builder steel");
    world
        .set_ledger_material_balance(MaterialLedgerId::agent("builder-a"), "circuit_board", 4)
        .expect("seed builder circuits");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 10)
        .expect("seed builder construction power");
    world.set_resource_balance(ResourceKind::Electricity, 100);

    let spec = factory_spec("factory.site-unknown", 1, 1, 1);
    let steel_before =
        world.ledger_material_balance(&MaterialLedgerId::agent("builder-a"), "steel_plate");
    let circuits_before =
        world.ledger_material_balance(&MaterialLedgerId::agent("builder-a"), "circuit_board");
    let owner_power_before = world
        .agent_resource_balance("builder-a", ResourceKind::Electricity)
        .expect("builder power before unknown site");
    let world_power_before = world.resource_balance(ResourceKind::Electricity);
    let journal_start = world.journal().events.len();

    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-not-registered".to_string(),
        spec,
    });
    world
        .step()
        .expect("unknown site should become a structured rejection");

    assert_eq!(world.pending_factory_builds_len(), 0);
    assert!(!world.has_factory("factory.site-unknown"));
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::agent("builder-a"), "steel_plate"),
        steel_before,
        "site_unknown must not consume construction steel"
    );
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::agent("builder-a"), "circuit_board"),
        circuits_before,
        "site_unknown must not consume construction circuits"
    );
    assert_eq!(
        world
            .agent_resource_balance("builder-a", ResourceKind::Electricity)
            .expect("builder power after unknown site"),
        owner_power_before,
        "site_unknown must not consume owner-held construction power"
    );
    assert_eq!(
        world.resource_balance(ResourceKind::Electricity),
        world_power_before,
        "site_unknown must not fall back to world electricity"
    );
    assert!(world.journal().events[journal_start..].iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::RuleDenied { notes },
                ..
            }) if notes.iter().any(|note| note.contains("site_unknown") || note.contains("site"))
        )
    }));
}

#[test]
fn build_factory_site_available_start_only_rejects_insufficient_construction_power_atomically() {
    const CONSTRUCTION_POWER: i64 = 10;

    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    world
        .set_ledger_material_balance(MaterialLedgerId::agent("builder-a"), "steel_plate", 20)
        .expect("seed builder steel");
    world
        .set_ledger_material_balance(MaterialLedgerId::agent("builder-a"), "circuit_board", 4)
        .expect("seed builder circuits");
    world
        .set_agent_resource_balance(
            "builder-a",
            ResourceKind::Electricity,
            CONSTRUCTION_POWER - 1,
        )
        .expect("seed insufficient builder construction power");
    world.set_resource_balance(ResourceKind::Electricity, CONSTRUCTION_POWER + 100);

    let spec = factory_spec("factory.site-power-shortage", 1, 1, 1);
    install_factory_authority(
        &mut world,
        "builder-a",
        "site-1",
        spec.factory_id.as_str(),
        CONSTRUCTION_POWER,
    );
    let steel_before =
        world.ledger_material_balance(&MaterialLedgerId::agent("builder-a"), "steel_plate");
    let circuits_before =
        world.ledger_material_balance(&MaterialLedgerId::agent("builder-a"), "circuit_board");
    let owner_power_before = world
        .agent_resource_balance("builder-a", ResourceKind::Electricity)
        .expect("builder power before shortage");
    let world_power_before = world.resource_balance(ResourceKind::Electricity);
    let journal_start = world.journal().events.len();

    // The explicit authority events above make `site-1` the canonical
    // site_available/authorized/colocated fixture for this admission test.
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec,
    });
    world
        .step()
        .expect("insufficient construction power should reject before sink");

    assert_eq!(world.pending_factory_builds_len(), 0);
    assert!(!world.has_factory("factory.site-power-shortage"));
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::agent("builder-a"), "steel_plate"),
        steel_before,
        "construction power shortage must not consume steel"
    );
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::agent("builder-a"), "circuit_board"),
        circuits_before,
        "construction power shortage must not consume circuits"
    );
    assert_eq!(
        world
            .agent_resource_balance("builder-a", ResourceKind::Electricity)
            .expect("builder power after shortage"),
        owner_power_before,
        "construction power shortage must not partially debit owner power"
    );
    assert_eq!(
        world.resource_balance(ResourceKind::Electricity),
        world_power_before,
        "construction power shortage must not debit world electricity"
    );
    assert!(world.journal().events[journal_start..].iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::InsufficientResource {
                    kind: ResourceKind::Electricity,
                    ..
                },
                ..
            })
        )
    }));
}

#[test]
fn build_factory_site_available_start_only_debits_construction_power_once_and_replays_without_duplication()
 {
    const CONSTRUCTION_POWER: i64 = 10;

    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    world
        .set_ledger_material_balance(MaterialLedgerId::agent("builder-a"), "steel_plate", 20)
        .expect("seed builder steel");
    world
        .set_ledger_material_balance(MaterialLedgerId::agent("builder-a"), "circuit_board", 4)
        .expect("seed builder circuits");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, CONSTRUCTION_POWER)
        .expect("seed builder construction power");
    world.set_resource_balance(ResourceKind::Electricity, 100);
    install_factory_authority(
        &mut world,
        "builder-a",
        "site-1",
        "factory.site-power-start-only",
        CONSTRUCTION_POWER,
    );
    let snapshot_before_build = world.snapshot();

    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec: factory_spec("factory.site-power-start-only", 1, 1, 1),
    });
    world.step().expect("start site_available factory build");
    assert_eq!(
        world
            .agent_resource_balance("builder-a", ResourceKind::Electricity)
            .expect("builder power after start"),
        0,
        "start_only construction power must debit the owner exactly once at start"
    );
    assert_eq!(world.resource_balance(ResourceKind::Electricity), 100);
    assert_eq!(world.pending_factory_builds_len(), 1);

    // A start-only commitment must not reinterpret later owner-power drift as
    // a second construction sink at completion.
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 0)
        .expect("clear owner power after start-only debit");
    world.step().expect("complete start-only factory build");
    assert!(world.has_factory("factory.site-power-start-only"));
    assert_eq!(
        world
            .state()
            .factory_construction_receipts
            .get("factory.site-power-start-only")
            .and_then(|obligation| obligation.material_ledger.clone()),
        Some(MaterialLedgerId::agent("builder-a")),
        "settled construction must retain its source ledger"
    );
    assert_eq!(
        world
            .agent_resource_balance("builder-a", ResourceKind::Electricity)
            .expect("builder power after completion"),
        0,
        "completion must not debit start_only construction power again"
    );

    let mut journal = world.journal().clone();
    let mut duplicate_built = journal
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::FactoryBuilt { .. }) => Some(event.clone()),
            _ => None,
        })
        .expect("FactoryBuilt event");
    duplicate_built.id = journal.events.last().expect("journal event").id + 1;
    journal.append(duplicate_built);

    let restored = World::from_snapshot(snapshot_before_build, journal)
        .expect("snapshot/replay must preserve one construction settlement");
    assert_eq!(
        restored.state().factories,
        world.state().factories,
        "replay must preserve the settled factory without duplication"
    );
    assert_eq!(
        restored.state().pending_factory_builds,
        world.state().pending_factory_builds,
        "replay must preserve an empty pending-build set"
    );
    assert_eq!(
        restored.state().settled_factory_build_ids,
        world.state().settled_factory_build_ids,
        "replay must preserve exactly one settled build id"
    );
    let builder_ledger = MaterialLedgerId::agent("builder-a");
    for kind in ["steel_plate", "circuit_board"] {
        assert_eq!(
            restored.ledger_material_balance(&builder_ledger, kind),
            world.ledger_material_balance(&builder_ledger, kind),
            "replay must not duplicate consumed construction material: {kind}"
        );
    }
    assert_eq!(
        restored.agent_resource_balance("builder-a", ResourceKind::Electricity),
        world.agent_resource_balance("builder-a", ResourceKind::Electricity),
        "replay must not duplicate the construction power sink"
    );
    assert_eq!(restored.pending_factory_builds_len(), 0);
    assert!(restored.has_factory("factory.site-power-start-only"));
    assert_eq!(
        restored
            .agent_resource_balance("builder-a", ResourceKind::Electricity)
            .expect("restored builder power"),
        0
    );
}

#[test]
fn factory_depreciation_reduces_durability_each_tick() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    build_factory_ready(
        &mut world,
        "builder-a",
        "site-1",
        factory_spec("factory.alpha", 1, 1, 3),
    );

    let snapshot = world.snapshot();
    let durability_before = snapshot
        .state
        .factories
        .get("factory.alpha")
        .expect("factory exists")
        .durability_ppm;
    assert_eq!(durability_before, 1_000_000);

    world.step().expect("idle tick for depreciation");
    let snapshot = world.snapshot();
    let durability_after = snapshot
        .state
        .factories
        .get("factory.alpha")
        .expect("factory exists")
        .durability_ppm;
    assert_eq!(durability_after, 997_000);

    let last = world.journal().events.last().expect("depreciation event");
    match &last.body {
        WorldEventBody::Domain(DomainEvent::FactoryDurabilityChanged {
            factory_id,
            previous_durability_ppm,
            durability_ppm,
            reason,
        }) => {
            assert_eq!(factory_id, "factory.alpha");
            assert_eq!(*previous_durability_ppm, 1_000_000);
            assert_eq!(*durability_ppm, 997_000);
            assert_eq!(reason, "depreciation_tick");
        }
        other => panic!("expected FactoryDurabilityChanged, got {other:?}"),
    }
}

#[test]
fn factory_depreciation_preparation_is_sorted_and_non_mutating() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    build_factory_ready(
        &mut world,
        "builder-a",
        "site-z",
        factory_spec("factory.zeta", 1, 1, 3),
    );
    build_factory_ready(
        &mut world,
        "builder-a",
        "site-a",
        factory_spec("factory.alpha", 1, 1, 2),
    );
    world
        .step()
        .expect("advance both factories past their build tick");
    let snapshot_before = world.snapshot();
    let journal_before = world.journal().clone();

    let prepared = world.prepare_factory_depreciation_event_bodies_for_test();

    assert_eq!(world.snapshot(), snapshot_before);
    assert_eq!(world.journal(), &journal_before);
    let factory_ids = prepared
        .iter()
        .map(|body| match body {
            WorldEventBody::Domain(DomainEvent::FactoryDurabilityChanged {
                factory_id, ..
            }) => factory_id.as_str(),
            other => panic!("expected prepared depreciation event, got {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(factory_ids, vec!["factory.alpha", "factory.zeta"]);
}

#[test]
fn factory_depreciation_scales_with_active_recipe_load() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    build_factory_ready(
        &mut world,
        "builder-a",
        "site-1",
        factory_spec("factory.load", 1, 2, 3),
    );
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 2)
        .expect("seed recipe input");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 20)
        .expect("seed builder electricity");
    world.set_resource_balance(ResourceKind::Electricity, 20);
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.load".to_string(),
        recipe_id: "recipe.load".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![MaterialStack::new("iron_ingot", 1)],
            vec![MaterialStack::new("control_chip", 1)],
            Vec::new(),
            1,
            3,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start recipe");
    assert_eq!(world.pending_recipe_jobs_len(), 1);

    let durability_before_loaded_tick = world
        .snapshot()
        .state
        .factories
        .get("factory.load")
        .expect("factory exists")
        .durability_ppm;
    assert_eq!(durability_before_loaded_tick, 997_000);

    world.step().expect("depreciation under load");

    let durability_after_loaded_tick = world
        .snapshot()
        .state
        .factories
        .get("factory.load")
        .expect("factory exists")
        .durability_ppm;
    assert_eq!(
        durability_before_loaded_tick - durability_after_loaded_tick,
        4_500
    );
}

#[test]
fn schedule_recipe_uses_factory_builder_electricity_and_debits_owner() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    build_factory_ready(
        &mut world,
        "builder-a",
        "site-owner-power",
        factory_spec("factory.owner-power", 1, 1, 1),
    );
    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-owner-power"), "iron_ingot", 1)
        .expect("seed recipe input");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 5)
        .expect("seed builder electricity");
    world.set_resource_balance(ResourceKind::Electricity, 0);

    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.owner-power".to_string(),
        recipe_id: "recipe.owner-power".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![MaterialStack::new("iron_ingot", 1)],
            vec![MaterialStack::new("control_chip", 1)],
            Vec::new(),
            5,
            2,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world
        .step()
        .expect("schedule recipe with owner electricity");

    assert_eq!(world.pending_recipe_jobs_len(), 1);
    assert_eq!(
        world
            .agent_resource_balance("builder-a", ResourceKind::Electricity)
            .expect("builder electricity"),
        0,
        "recipe power must debit the factory builder ledger"
    );
    assert_eq!(
        world.resource_balance(ResourceKind::Electricity),
        0,
        "world electricity must not be used as an implicit payer"
    );
}

#[test]
fn schedule_recipe_rejects_when_builder_electricity_is_insufficient_even_if_world_has_power() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    build_factory_ready(
        &mut world,
        "builder-a",
        "site-owner-power-reject",
        factory_spec("factory.owner-power-reject", 1, 1, 1),
    );
    world
        .set_ledger_material_balance(
            MaterialLedgerId::site("site-owner-power-reject"),
            "iron_ingot",
            1,
        )
        .expect("seed recipe input");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 0)
        .expect("clear builder electricity");
    world.set_resource_balance(ResourceKind::Electricity, 5);

    let recipe_ledger = MaterialLedgerId::site("site-owner-power-reject");
    let material_before = world.ledger_material_balance(&recipe_ledger, "iron_ingot");
    let journal_start = world.journal().events.len();
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.owner-power-reject".to_string(),
        recipe_id: "recipe.owner-power-reject".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![MaterialStack::new("iron_ingot", 1)],
            vec![MaterialStack::new("control_chip", 1)],
            Vec::new(),
            5,
            2,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world
        .step()
        .expect("insufficient owner electricity should be an action rejection");

    assert_eq!(world.pending_recipe_jobs_len(), 0);
    assert_eq!(
        world.ledger_material_balance(&recipe_ledger, "iron_ingot"),
        material_before
    );
    assert_eq!(
        world
            .agent_resource_balance("builder-a", ResourceKind::Electricity)
            .expect("builder electricity"),
        0
    );
    assert_eq!(world.resource_balance(ResourceKind::Electricity), 5);
    let rejection = world.journal().events[journal_start..]
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(DomainEvent::ActionRejected { reason, .. }) => Some(reason),
            _ => None,
        })
        .expect("rejection event");
    match rejection {
        RejectReason::InsufficientResource {
            agent_id,
            kind: ResourceKind::Electricity,
            requested,
            available,
        } => {
            assert_eq!(agent_id, "builder-a");
            assert_eq!(requested, &5);
            assert_eq!(available, &0);
        }
        other => panic!("expected owner-power rejection, got {other:?}"),
    }
}

#[test]
fn recycle_factory_removes_factory_and_returns_materials() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    build_factory_ready(
        &mut world,
        "builder-a",
        "site-1",
        factory_spec("factory.alpha", 1, 1, 1),
    );
    world.step().expect("depreciate once");

    world.submit_action(Action::RecycleFactory {
        operator_agent_id: "builder-a".to_string(),
        factory_id: "factory.alpha".to_string(),
    });
    world.step().expect("recycle factory");

    assert!(!world.has_factory("factory.alpha"));
    let site_ledger = MaterialLedgerId::site("site-1");
    assert!(world.ledger_material_balance(&site_ledger, "steel_plate") > 0);
    assert!(world.ledger_material_balance(&site_ledger, "circuit_board") > 0);

    let last = world.journal().events.last().expect("recycle event");
    match &last.body {
        WorldEventBody::Domain(DomainEvent::FactoryRecycled {
            factory_id,
            recovered,
            ..
        }) => {
            assert_eq!(factory_id, "factory.alpha");
            assert!(!recovered.is_empty());
        }
        other => panic!("expected FactoryRecycled, got {other:?}"),
    }
}

#[test]
fn industrial_integrity_factory_recycled_wrong_operator_is_byte_stable_rejection() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    build_factory_ready(
        &mut world,
        "builder-a",
        "site-recycle-owner",
        factory_spec("factory.recycle-owner", 1, 1, 1),
    );

    let mut replay = world.state().clone();
    let before = serde_json::to_vec(&replay).expect("serialize state before wrong recycle");
    let event = DomainEvent::FactoryRecycled {
        operator_agent_id: "operator-not-builder".to_string(),
        factory_id: "factory.recycle-owner".to_string(),
        recycle_ledger: MaterialLedgerId::site("site-recycle-owner"),
        recovered: vec![MaterialStack::new("steel_plate", 1)],
        durability_ppm: 1_000_000,
    };

    let result = replay.apply_domain_event(&event, replay.time);
    assert!(
        matches!(result, Err(WorldError::ResourceBalanceInvalid { .. })),
        "recycle by a non-builder must fail closed: {result:?}"
    );
    assert_eq!(
        serde_json::to_vec(&replay).expect("serialize state after wrong recycle"),
        before,
        "wrong-operator recycle must not remove the factory or recover materials"
    );
}

#[test]
fn industrial_integrity_duplicate_factory_recycle_is_noop_without_recovery_duplication() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    build_factory_ready(
        &mut world,
        "builder-a",
        "site-1",
        factory_spec("factory.recycle-replay", 1, 1, 1),
    );
    world.submit_action(Action::RecycleFactory {
        operator_agent_id: "builder-a".to_string(),
        factory_id: "factory.recycle-replay".to_string(),
    });
    world.step().expect("recycle factory for replay test");

    let recycled = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(event @ DomainEvent::FactoryRecycled { .. }) => {
                Some(event.clone())
            }
            _ => None,
        })
        .expect("factory recycle event");
    let mut replay = world.state().clone();
    let before = serde_json::to_vec(&replay).expect("serialize state before recycle replay");

    replay
        .apply_domain_event(&recycled, replay.time)
        .expect("duplicate factory recycle should be idempotent");
    assert!(
        serde_json::to_vec(&replay).expect("serialize state after recycle replay") == before,
        "duplicate factory recycle must not duplicate recovered materials or progress"
    );
}

#[test]
fn industrial_integrity_retired_factory_id_rejects_same_id_rebuild_before_sink() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    let spec = factory_spec("factory.retired", 1, 1, 1);
    build_factory_ready(&mut world, "builder-a", "site-1", spec.clone());

    world.submit_action(Action::RecycleFactory {
        operator_agent_id: "builder-a".to_string(),
        factory_id: spec.factory_id.clone(),
    });
    world.step().expect("recycle factory");
    assert!(!world.has_factory(spec.factory_id.as_str()));

    let steel_before = world.material_balance("steel_plate");
    let circuits_before = world.material_balance("circuit_board");
    let journal_start = world.journal().events.len();
    world.submit_action(Action::BuildFactory {
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-1".to_string(),
        spec,
    });
    world
        .step()
        .expect("same-id rebuild should produce an explicit rejection");

    assert_eq!(world.material_balance("steel_plate"), steel_before);
    assert_eq!(world.material_balance("circuit_board"), circuits_before);
    assert_eq!(world.pending_factory_builds_len(), 0);
    assert!(world.journal().events[journal_start..].iter().any(|event| {
        matches!(
            &event.body,
            WorldEventBody::Domain(DomainEvent::ActionRejected {
                reason: RejectReason::RuleDenied { notes },
                ..
            }) if notes.iter().any(|note| note.contains("retired") || note.contains("factory"))
        )
    }));
    assert!(
        !world.journal().events[journal_start..].iter().any(|event| {
            matches!(
                &event.body,
                WorldEventBody::Domain(DomainEvent::FactoryBuildStarted { .. })
            )
        })
    );
}

#[test]
fn industrial_integrity_retired_factory_id_rejects_stale_factory_built_replay() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    let spec = factory_spec("factory.retired.replay", 1, 1, 1);
    build_factory_ready(&mut world, "builder-a", "site-1", spec);
    let stale_built = world
        .journal()
        .events
        .iter()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(event @ DomainEvent::FactoryBuilt { .. }) => Some(event.clone()),
            _ => None,
        })
        .expect("original factory-built event");

    world.submit_action(Action::RecycleFactory {
        operator_agent_id: "builder-a".to_string(),
        factory_id: "factory.retired.replay".to_string(),
    });
    world.step().expect("recycle factory");
    assert!(!world.has_factory("factory.retired.replay"));

    let mut replay = world.state().clone();
    let before = serde_json::to_vec(&replay).expect("serialize state before stale build replay");
    let result = replay.apply_domain_event(&stale_built, replay.time);
    assert!(
        matches!(result, Err(WorldError::ResourceBalanceInvalid { .. })),
        "stale FactoryBuilt must fail closed: {result:?}"
    );
    assert!(
        serde_json::to_vec(&replay).expect("serialize state after stale build replay") == before,
        "stale FactoryBuilt must not mutate retired state"
    );
    assert!(!replay.factories.contains_key("factory.retired.replay"));
}

#[test]
fn industrial_integrity_unknown_factory_built_fails_without_state_mutation() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");

    let unknown_built = DomainEvent::FactoryBuilt {
        job_id: 9_999,
        builder_agent_id: "builder-a".to_string(),
        site_id: "site-unknown".to_string(),
        spec: factory_spec("factory.unknown-built", 1, 1, 1),
    };
    let mut replay = world.state().clone();
    let before = serde_json::to_vec(&replay).expect("serialize state before unknown build");

    let result = replay.apply_domain_event(&unknown_built, replay.time);
    assert!(
        matches!(result, Err(WorldError::ResourceBalanceInvalid { .. })),
        "unknown FactoryBuilt must fail closed: {result:?}"
    );
    assert_eq!(
        serde_json::to_vec(&replay).expect("serialize state after unknown build"),
        before,
        "unknown FactoryBuilt must not mutate state"
    );
}

#[test]
fn industrial_integrity_retired_factory_id_keeps_old_recycle_replay_byte_stable() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    let spec = factory_spec("factory.retired.recycle-replay", 1, 1, 1);
    build_factory_ready(&mut world, "builder-a", "site-1", spec);
    let replacement = world
        .state()
        .factories
        .get("factory.retired.recycle-replay")
        .cloned()
        .expect("factory before recycle");

    world.submit_action(Action::RecycleFactory {
        operator_agent_id: "builder-a".to_string(),
        factory_id: "factory.retired.recycle-replay".to_string(),
    });
    world.step().expect("recycle factory");
    let recycled = world
        .journal()
        .events
        .iter()
        .rev()
        .find_map(|event| match &event.body {
            WorldEventBody::Domain(event @ DomainEvent::FactoryRecycled { .. }) => {
                Some(event.clone())
            }
            _ => None,
        })
        .expect("factory recycle event");

    // Simulate a stale same-ID replacement in the replay target.  A retired
    // identity must prevent the old recycle disposition from deleting it.
    let mut replay = world.state().clone();
    replay
        .factories
        .insert(replacement.factory_id.clone(), replacement);
    let before = serde_json::to_vec(&replay).expect("serialize state before old recycle replay");
    replay
        .apply_domain_event(&recycled, replay.time)
        .expect("old recycle replay should be idempotent");
    assert!(
        serde_json::to_vec(&replay).expect("serialize state after old recycle replay") == before,
        "old FactoryRecycled replay must not mutate a retired identity"
    );
    assert!(
        replay
            .factories
            .contains_key("factory.retired.recycle-replay")
    );
}

#[test]
fn recycle_factory_rejects_when_recipe_job_is_active() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    build_factory_ready(
        &mut world,
        "builder-a",
        "site-1",
        factory_spec("factory.alpha", 1, 1, 1),
    );

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 1)
        .expect("seed recipe input");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 10)
        .expect("seed builder electricity");
    world.set_resource_balance(ResourceKind::Electricity, 10);
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.alpha".to_string(),
        recipe_id: "chip".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![MaterialStack::new("iron_ingot", 1)],
            vec![MaterialStack::new("control_chip", 1)],
            Vec::new(),
            1,
            3,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("schedule recipe");
    assert_eq!(world.pending_recipe_jobs_len(), 1);

    world.submit_action(Action::RecycleFactory {
        operator_agent_id: "builder-a".to_string(),
        factory_id: "factory.alpha".to_string(),
    });
    world.step().expect("recycle attempt");

    assert!(world.has_factory("factory.alpha"));
    let last = world.journal().events.last().expect("recycle reject");
    match &last.body {
        WorldEventBody::Domain(DomainEvent::ActionRejected {
            reason: RejectReason::FactoryBusy { factory_id, .. },
            ..
        }) => assert_eq!(factory_id, "factory.alpha"),
        other => panic!("expected FactoryBusy rejection, got {other:?}"),
    }
}

#[test]
fn industrial_integrity_direct_factory_recycle_with_active_recipe_is_byte_stable_rejection() {
    let mut world = World::new();
    register_builder(&mut world, "builder-a");
    build_factory_ready(
        &mut world,
        "builder-a",
        "site-1",
        factory_spec("factory.active-wip", 1, 1, 1),
    );

    world
        .set_ledger_material_balance(MaterialLedgerId::site("site-1"), "iron_ingot", 1)
        .expect("seed recipe input");
    world
        .set_agent_resource_balance("builder-a", ResourceKind::Electricity, 10)
        .expect("seed builder electricity");
    world.set_resource_balance(ResourceKind::Electricity, 10);
    world.submit_action(Action::ScheduleRecipe {
        requester_agent_id: "builder-a".to_string(),
        factory_id: "factory.active-wip".to_string(),
        recipe_id: "recipe.active-wip".to_string(),
        plan: RecipeExecutionPlan::accepted(
            1,
            vec![MaterialStack::new("iron_ingot", 1)],
            vec![MaterialStack::new("control_chip", 1)],
            Vec::new(),
            1,
            3,
        ),
        logistics_route_ids: Vec::new(),
        logistics_path_ids: Vec::new(),
    });
    world.step().expect("start recipe with active WIP");
    assert_eq!(world.pending_recipe_jobs_len(), 1);
    assert_eq!(
        world.ledger_material_balance(&MaterialLedgerId::site("site-1"), "iron_ingot"),
        0
    );
    assert_eq!(
        world
            .agent_resource_balance("builder-a", ResourceKind::Electricity)
            .expect("builder exists"),
        9
    );

    let mut replay = world.state().clone();
    let pending_before = replay.pending_recipe_jobs.clone();
    let material_ledgers_before = replay.material_ledgers.clone();
    let agent_before = replay
        .agents
        .get("builder-a")
        .cloned()
        .expect("builder state");
    let factory_before = replay
        .factories
        .get("factory.active-wip")
        .cloned()
        .expect("active factory state");
    let before = serde_json::to_vec(&replay).expect("serialize state before active-WIP recycle");
    let event = DomainEvent::FactoryRecycled {
        operator_agent_id: "builder-a".to_string(),
        factory_id: "factory.active-wip".to_string(),
        recycle_ledger: MaterialLedgerId::site("site-1"),
        recovered: vec![MaterialStack::new("steel_plate", 1)],
        durability_ppm: 1_000_000,
    };

    let result = replay.apply_domain_event(&event, replay.time);
    assert!(
        matches!(result, Err(WorldError::ResourceBalanceInvalid { .. })),
        "direct recycle reducer event must reject a factory with active WIP: {result:?}"
    );
    assert_eq!(
        serde_json::to_vec(&replay).expect("serialize state after active-WIP recycle"),
        before,
        "rejected active-WIP recycle must be byte-stable"
    );
    assert_eq!(
        replay.pending_recipe_jobs, pending_before,
        "rejected active-WIP recycle must preserve the pending recipe job"
    );
    assert_eq!(
        replay.material_ledgers, material_ledgers_before,
        "rejected active-WIP recycle must preserve consumed material balances"
    );
    assert_eq!(
        replay.agents.get("builder-a"),
        Some(&agent_before),
        "rejected active-WIP recycle must preserve consumed electricity and activity state"
    );
    assert_eq!(
        replay.factories.get("factory.active-wip"),
        Some(&factory_before),
        "rejected active-WIP recycle must preserve the factory state"
    );
}
