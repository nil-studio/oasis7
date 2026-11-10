//! Actual Current depreciation batch against the independent old World phase.
use super::*;
use crate::world::{
    incremental_factory_depreciation::*, incremental_factory_directory_orders::*,
    incremental_factory_directory_records::*,
    incremental_industrial_eligibility::incremental_industrial_eligibility_factory::IndustrialEligibilityReadWorkV1,
    incremental_industrial_eligibility::*,
};
use oasis7_world_state::{
    incremental_factory_records::*, incremental_industry_jobs::*,
    incremental_world_rule_composite_family::bind_current_agent_orders_v1,
    incremental_world_rule_receipts::qualify_world_rule_agent_orders_v1,
};
fn typed() -> oasis7_world_state::incremental_runtime_revocation::RevocationReadWorkV1 {
    oasis7_world_state::incremental_runtime_revocation::RevocationReadWorkV1 {
        json: oasis7_world_state::incremental_json_records::JsonReadWorkV1 {
            cells_remaining: 1000000,
            json_nodes_remaining: 1000000,
            max_depth: 64,
            max_payload_bytes: 1 << 20,
        },
        max_retained_trees: 4096,
        max_tree_heap: 1 << 20,
    }
}
fn run_current_depreciation(mut world: World, blocked: bool) {
    let now = world.state.time;
    if blocked {
        world.state.retired_factory_ids.insert("assembler".into());
    }
    let original = world.state.factories["assembler"].clone();
    let original_maintenance = original.spec.maintenance_per_tick;
    let mut first = original.clone();
    first.durability_ppm = 900_000;
    let mut last = original.clone();
    last.durability_ppm = 700_000;
    world.state.factories.insert("a-outer".into(), first);
    world.state.factories.insert("z-outer".into(), last);
    let recipe=serde_json::from_value(serde_json::json!({"job_id":73,"requester_agent_id":"owner","factory_id":"assembler","recipe_id":"recipe","accepted_batches":1,"consume":[],"produce":[],"byproducts":[],"power_required":0,"duration_ticks":1,"consume_ledger":"world","output_ledger":"world","bottleneck_tags":[],"logistics_route_ids":[],"logistics_path_ids":[],"ready_at":u64::MAX})).unwrap();
    world.state.pending_recipe_jobs.insert(73, recipe);
    let oracle = world.prepare_factory_depreciation_event_bodies_for_test();
    assert_eq!(
        oracle.len(),
        3,
        "duplicate embedded factory IDs must not be deduplicated"
    );
    let cancelled = std::cell::Cell::new(false);
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut ri = genesis::io();
    let mut rk = genesis::keys();
    let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Authorization),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut ri,
        },
        key_work: &mut rk,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || cancelled.get(),
    };
    let mut scan = genesis::scan();
    let mut order_typed = typed();
    let proof =
        qualify_world_rule_agent_orders_v1(&fixture.roots, &mut read, &mut scan, &mut order_typed)
            .unwrap();
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [91; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 4096,
            max_writes: 1024,
            max_facts: 32,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let live = tx.parent_fence();
    let mut oi = genesis::io();
    let mut ok = genesis::keys();
    let mut ctx = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut oi,
        },
        key_work: &mut ok,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || cancelled.get(),
    };
    let guard = ctx.transaction.guard();
    let source = bind_current_agent_orders_v1(
        &proof,
        &fixture.roots,
        &reader,
        &mut ctx,
        &mut scan,
        &mut order_typed,
        &guard,
        &live,
    )
    .unwrap();
    let mut directory_work = FactoryOrderDirectoryWorkV1 {
        ids:
            oasis7_world_state::incremental_world_rule_composite_family::AgentOrderDirectoryWorkV1 {
                max_ids: 32,
                max_id_bytes: 256,
                comparison_bytes_remaining: 16 << 20,
            },
        pending_entries_remaining: 10000,
        merge_comparison_bytes_remaining: 16 << 20,
        max_index_text_bytes: 256,
    };
    let guard = ctx.transaction.guard();
    let accepted = build_accepted_factory_order_directory_v1(
        &fixture.runtime,
        &fixture.head,
        &source,
        &reader,
        &mut ctx,
        &mut scan,
        &mut order_typed,
        &mut directory_work,
        &guard,
        &live,
    )
    .unwrap();
    let guard = ctx.transaction.guard();
    let directory = open_current_factory_order_directory_v1(
        &accepted,
        &source,
        &reader,
        &mut ctx,
        &mut scan,
        &mut order_typed,
        &mut directory_work,
        &guard,
        &live,
    )
    .unwrap();
    let native = fixture.current.native();
    let runtime = fixture.current.runtime();
    let sources = IndustrialEligibilitySourcesV1 {
        roots: &fixture.roots,
        orders: &directory,
        agents: fixture.current.raw().agents(),
        native: native.native(),
        factories: native.factory(),
        sites: native.factory_authority(),
        anchors: native.agent_authority(),
        revocation: runtime.revocation(),
        artifacts: runtime.resources(),
    };

    let mut work = IndustrialEligibilityWorkV1 {
        read: IndustrialEligibilityReadWorkV1 {
            cells_remaining: 1000000,
            comparison_bytes_remaining: 64 << 20,
            max_text_bytes: 4096,
        },
        typed: typed(),
        scan: genesis::scan(),
        revocation: typed(),
        pending_entries_remaining: 100000,
    };
    let mut fi = genesis::io();
    let mut fk = genesis::keys();
    let mut fs = genesis::scan();
    let mut rfi = genesis::io();
    let mut rfk = genesis::keys();
    let mut rfs = genesis::scan();
    let mut factory = FactoryFamilyReadContextV1 {
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut fi,
        },
        key_work: &mut fk,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        max_payload_bytes: 1 << 20,
        outer_scan: &mut fs,
        take_state_error: || reader.cas.error(),
        cancelled: || cancelled.get(),
    };
    let mut recipe = IndustryFamilyReadContextV1 {
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut rfi,
        },
        key_work: &mut rfk,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        max_payload_bytes: 1 << 20,
        outer_scan: &mut rfs,
        take_state_error: || reader.cas.error(),
        cancelled: || cancelled.get(),
    };
    let mut fw = FactoryWorkBudgetV1 {
        cells_remaining: 1000000,
        max_pending_node_bytes: 16 << 20,
        scan: genesis::scan(),
    };
    let mut rw = IndustryWorkBudgetV1 {
        cells_remaining: 1000000,
        max_pending_node_bytes: 16 << 20,
        scan: genesis::scan(),
    };
    let mut dw = FactoryRecipeDirectoryWorkV1 {
        max_factories: 32,
        max_recipe_jobs: 32,
        max_factory_id_bytes: 256,
        max_outer_changes: 64,
        comparisons_remaining: 64 << 20,
        pending_entries_remaining: 100000,
    };
    let guard = ctx.transaction.guard();
    let parent = build_accepted_factory_recipe_directory_v1(
        &fixture.runtime,
        &fixture.head,
        &fixture.roots,
        native.factory(),
        native.industry(),
        ctx.transaction,
        &mut factory,
        &mut recipe,
        &mut fw,
        &mut rw,
        &mut dw,
        &guard,
        &live,
    )
    .unwrap();
    let mut fci = genesis::io();
    let mut fck = genesis::keys();
    let mut rci = genesis::io();
    let mut rck = genesis::keys();
    let mut fc = FactoryTransitionReadContextV1 {
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut fci,
        },
        key_work: &mut fck,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        max_payload_bytes: 1 << 20,
        cancelled: || cancelled.get(),
    };
    let mut rc = IndustryTransitionReadContextV1 {
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut rci,
        },
        key_work: &mut rck,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        max_payload_bytes: 1 << 20,
        cancelled: || cancelled.get(),
    };
    let guard = ctx.transaction.guard();
    let records = open_current_factory_recipe_directory_v1(
        &parent,
        native.factory(),
        native.industry(),
        ctx.transaction,
        &mut fc,
        &mut rc,
        &mut fw,
        &mut rw,
        &mut dw,
        &guard,
        &live,
    )
    .unwrap();
    let mut depreciation = FactoryDepreciationWorkV1 {
        max_factories: 32,
        max_workload_entries: 64,
        max_text_bytes: 4096,
        cells_remaining: 1000000,
        comparison_bytes_remaining: 64 << 20,
    };
    let guard = ctx.transaction.guard();
    let batch = prepare_current_factory_depreciation_v1(
        &records,
        &sources,
        now,
        false,
        &reader,
        &mut ctx,
        &mut read,
        &mut fc,
        &mut rc,
        &mut fw,
        &mut rw,
        &mut dw,
        &mut work,
        &mut depreciation,
        &guard,
        &live,
    )
    .unwrap();
    assert_eq!(batch.bodies(), oracle.as_slice());
    assert_eq!(batch.time(), now);
    let guard = ctx.transaction.guard();
    batch
        .validate_fresh_start_v1(&mut ctx, now, &guard, &live)
        .unwrap();
    let guard = ctx.transaction.guard();
    batch
        .validate(ctx.transaction, &memory, now, &guard, &live)
        .unwrap();
    assert!(
        batch
            .validate(
                ctx.transaction,
                &memory,
                now.saturating_add(1),
                &guard,
                &live
            )
            .is_err()
    );
    let foreign = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [124; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 4096,
            max_writes: 1024,
            max_facts: 32,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let foreign_guard = foreign.guard();
    assert!(
        batch
            .validate(&foreign, &memory, now, &foreign_guard, &live)
            .is_err()
    );

    // Resume skips the complete phase, including all scalar reads and business eligibility.
    depreciation.cells_remaining = 0;
    work.read.cells_remaining = 0;
    fw.cells_remaining = 0;
    rw.cells_remaining = 0;
    let guard = ctx.transaction.guard();
    let resumed = prepare_current_factory_depreciation_v1(
        &records,
        &sources,
        now,
        true,
        &reader,
        &mut ctx,
        &mut read,
        &mut fc,
        &mut rc,
        &mut fw,
        &mut rw,
        &mut dw,
        &mut work,
        &mut depreciation,
        &guard,
        &live,
    )
    .unwrap();
    assert!(resumed.bodies().is_empty());
    if blocked {
        let mut cas = fixture.cas.clone();
        let mut write_io = genesis::io();
        let mut write_keys = genesis::keys();
        let mut mutation_work = FactoryWorkBudgetV1 {
            cells_remaining: 1000000,
            max_pending_node_bytes: 16 << 20,
            scan: genesis::scan(),
        };
        let guard = ctx.transaction.guard();
        change_factory_cell_overlay_v1(
            FactoryCellChangeV1 {
                key: FactoryRecordKeyV1::Factory("assembler"),
                cell: FactoryCellV1::Column(FactoryColumnV1::SpecMaintenancePerTick),
                expected: Some(FactoryValueRefV1::Signed(original_maintenance)),
                after: Some(FactoryValueRefV1::Signed(
                    original_maintenance.saturating_add(1),
                )),
                max_current_payload_bytes: 4096,
            },
            &mut ctx,
            &mut oasis7_world_state::incremental_field_member::MemberRecordWriteContext {
                sink: &mut cas,
                memory: &memory,
                io: &mut write_io,
                key_work: &mut write_keys,
                limits: oasis7_world_state::incremental_field_member::MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: 4096,
                    record_bytes: 16 << 20,
                },
                cancelled: || false,
            },
            &mut mutation_work,
            &guard,
            &live,
        )
        .unwrap();
        let fresh_guard = ctx.transaction.guard();
        // Identity-only validation still succeeds; exact-start rejects the
        // actual maintenance mutation even with the caller's new guard.
        batch
            .validate(ctx.transaction, &memory, now, &fresh_guard, &live)
            .unwrap();
        assert!(matches!(
            batch.validate_fresh_start_v1(&mut ctx, now, &fresh_guard, &live),
            Err(FactoryDepreciationErrorV1::Execution(
                ExecutionError::StaleGuard
            ))
        ));
    } else {
        cancelled.set(true);
        let guard = ctx.transaction.guard();
        assert!(
            prepare_current_factory_depreciation_v1(
                &records,
                &sources,
                now,
                false,
                &reader,
                &mut ctx,
                &mut read,
                &mut fc,
                &mut rc,
                &mut fw,
                &mut rw,
                &mut dw,
                &mut work,
                &mut depreciation,
                &guard,
                &live
            )
            .is_err()
        );
    }
    let guard = ctx.transaction.guard();
    assert!(tx.seal(&guard, &live, 1).is_err());
}
#[test]
fn current_depreciation_preserves_duplicate_embedded_order_and_actual_recipe_industrial_load() {
    run_current_depreciation(super::industrial_eligibility::producing_world(), false);
}
#[test]
fn current_depreciation_excludes_real_blocked_producing_order_load() {
    run_current_depreciation(super::industrial_eligibility::producing_world(), true);
}
