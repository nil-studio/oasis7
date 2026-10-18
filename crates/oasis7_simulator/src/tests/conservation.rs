use super::*;

fn assert_budget_bounds(model: &WorldModel) {
    for location in model.locations.values() {
        let Some(fragment_budget) = &location.fragment_budget else {
            continue;
        };
        for (element, total) in &fragment_budget.total_by_element_g {
            let remaining = fragment_budget.get_remaining(*element);
            assert!(
                remaining >= 0,
                "fragment remaining should be non-negative: location={} element={:?} remaining={}",
                location.id,
                element,
                remaining
            );
            assert!(
                remaining <= *total,
                "fragment remaining should not exceed total: location={} element={:?} remaining={} total={}",
                location.id,
                element,
                remaining,
                total
            );
        }
    }

    for (coord, chunk_budget) in &model.chunk_resource_budgets {
        for (element, total) in &chunk_budget.total_by_element_g {
            let remaining = chunk_budget.get_remaining(*element);
            assert!(
                remaining >= 0,
                "chunk remaining should be non-negative: coord={:?} element={:?} remaining={}",
                coord,
                element,
                remaining
            );
            assert!(
                remaining <= *total,
                "chunk remaining should not exceed total: coord={:?} element={:?} remaining={} total={}",
                coord,
                element,
                remaining,
                total
            );
        }
    }
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn fragment_and_chunk_budgets_stay_within_total_bounds_after_consumption() {
    let mut config = WorldConfig::default();
    config.space = SpaceConfig {
        width_cm: 200_000,
        depth_cm: 200_000,
        height_cm: 200_000,
    };
    config.asteroid_fragment.base_density_per_km3 = 5.0;
    config.asteroid_fragment.voxel_size_km = 1;
    config.asteroid_fragment.cluster_noise = 0.0;
    config.asteroid_fragment.layer_scale_height_km = 0.0;
    config.asteroid_fragment.min_fragment_spacing_cm = 0;
    config.asteroid_fragment.radius_min_cm = 120;
    config.asteroid_fragment.radius_max_cm = 120;

    let mut init = WorldInitConfig::default();
    init.seed = 31;
    init.agents.count = 0;

    let (mut kernel, _) = initialize_kernel(config.clone(), init).expect("initialize kernel");
    assert_budget_bounds(kernel.model());

    let fragment = kernel
        .model()
        .locations
        .values()
        .find(|location| location.id.starts_with("frag-"))
        .cloned()
        .expect("fragment exists");
    let element = fragment
        .fragment_budget
        .as_ref()
        .and_then(|budget| budget.remaining_by_element_g.keys().next().copied())
        .expect("fragment has tracked element");
    let remaining_before = fragment
        .fragment_budget
        .as_ref()
        .expect("fragment budget")
        .get_remaining(element);
    let consume_amount = remaining_before.clamp(1, 40);

    kernel
        .consume_fragment_resource(&fragment.id, element, consume_amount)
        .expect("consume fragment resource");

    assert_budget_bounds(kernel.model());

    let overdraw = kernel.consume_fragment_resource(&fragment.id, element, remaining_before);
    assert!(matches!(
        overdraw,
        Err(FragmentResourceError::Budget(
            ElementBudgetError::Insufficient { .. }
        ))
    ));
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn harvest_and_refine_follow_resource_ledger_without_free_gain() {
    let mut config = WorldConfig::default();
    config.economy.refine_electricity_cost_per_kg = 3;
    config.economy.refine_hardware_yield_ppm = 2_000;
    config.physics.radiation_floor = 0;
    config.physics.radiation_floor_cap_per_tick = 0;
    config.physics.max_harvest_per_tick = 100;

    let mut kernel = WorldKernel::with_config(config.clone());
    let mut profile = LocationProfile::default();
    profile.radiation_emission_per_tick = 80;

    kernel.submit_action(Action::RegisterLocation {
        location_id: "site".to_string(),
        name: "site".to_string(),
        pos: pos(0, 0),
        profile,
    });
    kernel.submit_action(Action::RegisterAgent {
        agent_id: "agent-1".to_string(),
        location_id: "site".to_string(),
    });
    kernel.step_until_empty();

    kernel.submit_action(Action::HarvestRadiation {
        agent_id: "agent-1".to_string(),
        max_amount: 30,
    });
    let harvested = match kernel.step().expect("harvest event").kind {
        WorldEventKind::RadiationHarvested { amount, .. } => amount,
        other => panic!("unexpected event: {other:?}"),
    };

    let refine_mass_g = 2_000;
    seed_owner_resource(
        &mut kernel,
        ResourceOwner::Agent {
            agent_id: "agent-1".to_string(),
        },
        ResourceKind::Data,
        refine_mass_g,
    );
    kernel.submit_action(Action::RefineCompound {
        owner: ResourceOwner::Agent {
            agent_id: "agent-1".to_string(),
        },
        compound_mass_g: refine_mass_g,
    });
    let (electricity_cost, hardware_output) = match kernel.step().expect("refine event").kind {
        WorldEventKind::CompoundRefined {
            electricity_cost,
            hardware_output,
            ..
        } => (electricity_cost, hardware_output),
        other => panic!("unexpected event: {other:?}"),
    };

    let expected_cost =
        ((refine_mass_g + 999) / 1000) * config.economy.refine_electricity_cost_per_kg;
    let expected_hardware = refine_mass_g
        .saturating_mul(config.economy.refine_hardware_yield_ppm)
        .saturating_div(PPM_BASE);

    assert_eq!(electricity_cost, expected_cost);
    assert_eq!(hardware_output, expected_hardware);

    let agent = kernel.model().agents.get("agent-1").expect("agent exists");
    let electricity_after = agent.resources.get(ResourceKind::Electricity);
    let hardware_after = agent.resources.get(ResourceKind::Data);

    assert_eq!(electricity_after, harvested - electricity_cost);
    assert_eq!(hardware_after, hardware_output);
    assert!(electricity_after >= 0);
    assert!(hardware_after >= 0);
}
