use super::*;

#[expect(
    clippy::field_reassign_with_default,
    reason = "Scenario fixture mutates only the fields under test."
)]
fn kernel_for_fragment_refill_preview(
    interval_ticks: i64,
    current_tick: u64,
    fragment_count: usize,
    max_fragments_per_chunk: i64,
) -> (WorldKernel, ChunkCoord) {
    let chunk = ChunkCoord { x: 0, y: 0, z: 0 };
    let mut config = WorldConfig::default();
    config.space = SpaceConfig {
        width_cm: 200_000,
        depth_cm: 200_000,
        height_cm: 200_000,
    };
    config.asteroid_fragment.max_fragments_per_chunk = max_fragments_per_chunk;
    config.asteroid_fragment.replenish_interval_ticks = interval_ticks;
    config.asteroid_fragment.replenish_percent_ppm = 10_000;

    let mut model = WorldModel::default();
    model.chunks.insert(chunk, ChunkState::Generated);
    let mut budget = ChunkResourceBudget::default();
    budget
        .remaining_by_element_g
        .insert(FragmentElementKind::Iron, 15);
    model.chunk_resource_budgets.insert(chunk, budget);
    for index in 0..fragment_count {
        let id = format!("frag-preview-{index}");
        model.locations.insert(
            id.clone(),
            Location::new_with_profile(
                id,
                "preview fragment".to_string(),
                GeoPos {
                    x_cm: 10_000 + index as i64,
                    y_cm: 10_000,
                    z_cm: 10_000,
                },
                LocationProfile::default(),
            ),
        );
    }

    let mut kernel = WorldKernel::with_model_and_chunk_runtime(
        config,
        model,
        ChunkRuntimeConfig {
            world_seed: 7,
            asteroid_fragment_enabled: true,
            asteroid_fragment_seed_offset: 1,
            min_fragment_spacing_cm: None,
        },
    );
    let mut snapshot = kernel.kernel_snapshot();
    snapshot.time = current_tick;
    kernel = WorldKernel::from_kernel_snapshot(snapshot, kernel.kernel_journal_snapshot())
        .expect("restore preview fixture at requested tick");
    (kernel, chunk)
}

#[test]
fn fragment_refill_preview_explains_disabled_interval_without_promising_replenishment() {
    let (kernel, chunk) = kernel_for_fragment_refill_preview(0, 20, 0, 100);

    let quote = kernel
        .quote_fragment_refill_preview(chunk)
        .expect("disabled interval still has a player-readable preview");

    assert!(!quote.replenishment_enabled);
    assert_eq!(quote.next_replenish_tick, None);
    assert_eq!(quote.estimated_replenished_frag_count, 0);
    assert_eq!(
        quote.recommended_resource_action,
        "move_or_switch_material_route"
    );
}

#[test]
fn fragment_refill_preview_reports_wait_cost_when_replenishment_is_not_yet_due() {
    let (kernel, chunk) = kernel_for_fragment_refill_preview(100, 40, 0, 100);

    let quote = kernel
        .quote_fragment_refill_preview(chunk)
        .expect("not-yet-due preview");

    assert!(quote.replenishment_enabled);
    assert!(!quote.replenishment_due);
    assert_eq!(quote.next_replenish_tick, Some(100));
    assert_eq!(quote.wait_cost_ticks, 60);
    assert_eq!(quote.estimated_replenished_frag_count, 1);
    assert_eq!(quote.recommended_resource_action, "wait_current_chunk");
}

#[test]
fn fragment_refill_preview_quantifies_a_replenishable_deficit_and_resource_state() {
    let (kernel, chunk) = kernel_for_fragment_refill_preview(100, 100, 99, 100);

    let quote = kernel
        .quote_fragment_refill_preview(chunk)
        .expect("next replenish preview at a completed boundary");

    assert!(!quote.replenishment_due);
    assert_eq!(quote.next_replenish_tick, Some(200));
    assert_eq!(quote.wait_cost_ticks, 100);
    assert_eq!(quote.estimated_replenished_frag_count, 1);
    assert_eq!(
        quote.remaining_by_element_g.get(&FragmentElementKind::Iron),
        Some(&15)
    );
    assert!(!quote.next_industrial_goal_relevance.is_empty());
    assert_eq!(quote.recommended_resource_action, "wait_current_chunk");
}

#[test]
fn fragment_refill_preview_does_not_recommend_waiting_for_a_full_chunk() {
    let (kernel, chunk) = kernel_for_fragment_refill_preview(100, 100, 100, 100);

    let quote = kernel
        .quote_fragment_refill_preview(chunk)
        .expect("full chunk preview");

    assert_eq!(quote.estimated_replenished_frag_count, 0);
    assert_eq!(
        quote.recommended_resource_action,
        "move_or_switch_material_route"
    );
}

#[test]
fn fragment_refill_preview_is_deterministic_and_does_not_mutate_kernel_state() {
    let (kernel, chunk) = kernel_for_fragment_refill_preview(100, 100, 99, 100);
    let snapshot_before = kernel.kernel_snapshot();
    let journal_before = kernel.kernel_journal_snapshot();

    let first = kernel
        .quote_fragment_refill_preview(chunk)
        .expect("first preview");
    let second = kernel
        .quote_fragment_refill_preview(chunk)
        .expect("repeat preview");

    assert_eq!(second, first);
    assert_eq!(kernel.kernel_snapshot(), snapshot_before);
    assert_eq!(kernel.kernel_journal_snapshot(), journal_before);
}

#[test]
fn fragment_refill_preview_rejects_unknown_budget_instead_of_reporting_zero_resources() {
    let (kernel, chunk) = kernel_for_fragment_refill_preview(100, 40, 0, 100);
    let mut snapshot = kernel.kernel_snapshot();
    snapshot.model.chunk_resource_budgets.remove(&chunk);
    let kernel = WorldKernel::from_kernel_snapshot(snapshot, kernel.kernel_journal_snapshot())
        .expect("restore fixture with unavailable budget");

    let reason = kernel
        .quote_fragment_refill_preview(chunk)
        .expect_err("missing budget is unknown, not zero");
    assert!(reason.contains("chunk resource budget is unavailable"));
}

#[test]
fn fragment_refill_preview_rejects_unrepresentable_future_tick() {
    let (kernel, chunk) = kernel_for_fragment_refill_preview(100, u64::MAX, 0, 100);

    let reason = kernel
        .quote_fragment_refill_preview(chunk)
        .expect_err("overflow cannot promise a refill at the current tick");
    assert!(reason.contains("next replenishment tick is unavailable"));
}

#[test]
#[expect(clippy::field_reassign_with_default, reason = "fixture defaults")]
fn fragment_refill_preview_tracks_real_step_replenishment_and_restored_boundaries() {
    let mut config = WorldConfig::default();
    config.asteroid_fragment.base_density_per_km3 = 0.0;
    config.asteroid_fragment.min_fragments_per_chunk = 0;
    config.asteroid_fragment.max_fragments_per_chunk = 100;
    config.asteroid_fragment.replenish_interval_ticks = 100;
    config.asteroid_fragment.replenish_percent_ppm = 10_000;
    let chunk = ChunkCoord { x: 0, y: 0, z: 0 };
    let mut init = WorldInitConfig::default();
    init.seed = 123;
    init.agents.count = 0;
    init.asteroid_fragment.bootstrap_chunks = vec![chunk];
    let (mut kernel, _) = initialize_kernel(config, init).expect("initialize real fragment world");
    let initial_snapshot = kernel.kernel_snapshot();

    for tick in 0..100 {
        if tick == 40 {
            let quote = kernel
                .quote_fragment_refill_preview(chunk)
                .expect("future preview");
            assert_eq!(quote.next_replenish_tick, Some(100));
            assert_eq!(quote.wait_cost_ticks, 60);
            assert_eq!(quote.estimated_replenished_frag_count, 1);
        }
        kernel.submit_action(Action::RegisterLocation {
            location_id: format!("refill-quote-tick-{tick}"),
            name: format!("refill-quote-tick-{tick}"),
            pos: GeoPos {
                x_cm: 1000 + tick,
                y_cm: 2000,
                z_cm: 3000,
            },
            profile: LocationProfile::default(),
        });
        kernel.step().expect("advance real kernel tick");
    }

    let quote = kernel
        .quote_fragment_refill_preview(chunk)
        .expect("post-step preview");
    assert_eq!(quote.next_replenish_tick, Some(200));
    assert_eq!(quote.wait_cost_ticks, 100);
    assert!(!quote.replenishment_due);
    assert_eq!(quote.estimated_replenished_frag_count, 1);
    let replenished: Vec<_> = kernel
        .journal()
        .iter()
        .filter_map(|event| match &event.kind {
            WorldEventKind::FragmentsReplenished { entries } => Some(entries),
            _ => None,
        })
        .collect();
    assert_eq!(replenished.len(), 1);
    // The default origin also initializes its own chunk. A refill event may
    // cover multiple chunks, whereas this quote describes only the requested one.
    assert!(replenished[0].iter().any(|entry| entry.coord != chunk));
    let replenished_in_chunk: Vec<_> = replenished[0]
        .iter()
        .filter(|entry| entry.coord == chunk)
        .collect();
    assert_eq!(replenished_in_chunk.len(), 1);
    let fragment = &replenished_in_chunk[0].location;
    assert!(kernel.model().locations.contains_key(&fragment.id));
    assert_eq!(
        quote.remaining_by_element_g,
        fragment
            .fragment_budget
            .as_ref()
            .expect("committed budget")
            .remaining_by_element_g
    );
    let restored = WorldKernel::from_kernel_snapshot(
        kernel.kernel_snapshot(),
        kernel.kernel_journal_snapshot(),
    )
    .expect("restore completed boundary");
    let replayed = WorldKernel::replay_from_kernel_snapshot(
        initial_snapshot,
        kernel.kernel_journal_snapshot(),
    )
    .expect("replay committed refill once");
    assert_eq!(
        restored
            .quote_fragment_refill_preview(chunk)
            .expect("restored quote"),
        quote
    );
    assert_eq!(
        replayed
            .quote_fragment_refill_preview(chunk)
            .expect("replayed quote"),
        quote
    );
    assert_eq!(replayed.model(), kernel.model());
}
