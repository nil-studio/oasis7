//! True GEN parent and ordinary Current candidate selection; no caller ID source.
use super::*;
use crate::world::incremental_factory_directory_records::*;
use oasis7_world_state::incremental_factory_records::*;
use oasis7_world_state::incremental_industry_jobs::*;

fn directory_work() -> FactoryRecipeDirectoryWorkV1 {
    FactoryRecipeDirectoryWorkV1 {
        max_factories: 16,
        max_recipe_jobs: 16,
        max_factory_id_bytes: 128,
        max_outer_changes: 32,
        comparisons_remaining: 16 << 20,
        pending_entries_remaining: 10000,
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1000000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 100000,
    }
}
fn factory_work() -> FactoryWorkBudgetV1 {
    FactoryWorkBudgetV1 {
        cells_remaining: 1000000,
        max_pending_node_bytes: 8 << 20,
        scan: scan(),
    }
}
fn recipe_work() -> IndustryWorkBudgetV1 {
    IndustryWorkBudgetV1 {
        cells_remaining: 1000000,
        max_pending_node_bytes: 8 << 20,
        scan: scan(),
    }
}
fn records_source() -> oasis7_world_state::WorldState {
    let mut source = oasis7_world_state::WorldState::default();
    let factory = serde_json::from_value(serde_json::json!({
        "factory_id":"embedded-factory", "site_id":"site", "builder_agent_id":"builder",
        "spec":{"factory_id":"embedded-factory","display_name":"factory","tier":1,
            "tags":[],"build_cost":[],"build_time_ticks":1,"base_power_draw":1,
            "recipe_slots":2,"throughput_bps":1,"maintenance_per_tick":1},"built_at":1
    }))
    .unwrap();
    source.factories.insert("z-parent".into(), factory);
    let recipe = serde_json::from_value(serde_json::json!({
        "job_id":73,"requester_agent_id":"requester","factory_id":"embedded-factory",
        "recipe_id":"recipe","accepted_batches":1,"consume":[],"produce":[],"byproducts":[],
        "power_required":0,"duration_ticks":1,"consume_ledger":"world","output_ledger":"world",
        "bottleneck_tags":[],"logistics_route_ids":[],"logistics_path_ids":[],"ready_at":1
    }))
    .unwrap();
    source.pending_recipe_jobs.insert(73, recipe);
    source
}

#[test]
fn actual_factory_recipe_directory_gen_current_add_delete_and_cancel_poison() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    let source = records_source();
    world.state.factories = source.factories;
    world.state.pending_recipe_jobs = source.pending_recipe_jobs;
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let cancelled = std::cell::Cell::new(false);
    let mut fi = genesis::io();
    let mut fk = genesis::keys();
    let mut fs = scan();
    let mut ri = genesis::io();
    let mut rk = genesis::keys();
    let mut rs = scan();
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
        record_bytes: 2 << 20,
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
            io: &mut ri,
        },
        key_work: &mut rk,
        semantic_bytes: 4096,
        record_bytes: 2 << 20,
        max_payload_bytes: 1 << 20,
        outer_scan: &mut rs,
        take_state_error: || reader.cas.error(),
        cancelled: || cancelled.get(),
    };
    let mut fw = factory_work();
    let mut rw = recipe_work();
    let fq = qualify_factory_family_v1(&fixture.roots, &mut factory, &mut fw).unwrap();
    let rq = qualify_industry_family_v1(&fixture.roots, &mut recipe, &mut rw).unwrap();
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [123; 32],
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
    let guard = tx.guard();
    let mut work = directory_work();
    let parent = build_accepted_factory_recipe_directory_v1(
        &fixture.runtime,
        &fixture.head,
        &fixture.roots,
        &fq,
        &rq,
        &mut tx,
        &mut factory,
        &mut recipe,
        &mut fw,
        &mut rw,
        &mut work,
        &guard,
        &live,
    )
    .unwrap();
    // New actual generated outer roots, plus genuine deletion of the former
    // factory map key. Embedded factory_id is deliberately different.
    let mut changed = records_source();
    let value = changed.factories.remove("z-parent").unwrap();
    changed.factories.insert("a-current".into(), value);
    let mut value = changed.pending_recipe_jobs.remove(&73).unwrap();
    value.job_id = 19;
    changed.pending_recipe_jobs.insert(19, value);
    let mut sink = fixture.cas.clone();
    let mut wi = genesis::io();
    let mut wk = genesis::keys();
    let mut write = oasis7_world_state::incremental_field_member::MemberRecordWriteContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut wi,
        key_work: &mut wk,
        limits: oasis7_world_state::incremental_field_member::MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 1 << 20,
            record_bytes: 2 << 20,
        },
        cancelled: || false,
    };
    let mut records = 1000000;
    prepare_factory_records_genesis_v1(&changed, &mut write, &mut records, &mut fw, &mut |row| {
        tx.write(row.namespace, &row.mutation.key, row.mutation.after)
            .unwrap();
        Ok(())
    })
    .unwrap();
    prepare_industry_jobs_genesis_v1(&changed, &mut write, &mut records, &mut rw, &mut |row| {
        tx.write(row.namespace, &row.mutation.key, row.mutation.after)
            .unwrap();
        Ok(())
    })
    .unwrap();
    // Use the original semantic key grammar, not a fabricated diff/ID list.
    let mut delete = |field_name: &str, digest: [u8; 32]| {
        let descriptor = *oasis7_world_state::incremental_projection::WORLD_STATE_FIELDS
            .iter()
            .find(|field| field.name == field_name)
            .unwrap();
        let key = oasis7_world_state::incremental_world_field_key::WorldFieldKeyV1::new(
            descriptor,
            oasis7_world_state::incremental_world_field_key::WorldFieldRecordKindV1::Entry,
            Some(digest),
        )
        .unwrap();
        let oasis7_world_state::incremental_projection::FieldDestination::State(namespace) =
            descriptor.destination
        else {
            panic!("actual state field")
        };
        tx.write(namespace, key.as_bytes(), None).unwrap();
    };
    let mut keys = genesis::keys();
    delete(
        "factories",
        oasis7_world_state::incremental_semantic_key::hash_semantic_key_v1(
            &"z-parent",
            &memory,
            MemoryClass::Ordinary,
            &mut keys,
            &mut || false,
        )
        .unwrap()
        .digest,
    );
    delete(
        "pending_recipe_jobs",
        oasis7_world_state::incremental_semantic_key::hash_semantic_key_v1(
            &73u64,
            &memory,
            MemoryClass::Ordinary,
            &mut keys,
            &mut || false,
        )
        .unwrap()
        .digest,
    );
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
        record_bytes: 2 << 20,
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
        record_bytes: 2 << 20,
        max_payload_bytes: 1 << 20,
        cancelled: || cancelled.get(),
    };
    let guard = tx.guard();
    let current = open_current_factory_recipe_directory_v1(
        &parent, &fq, &rq, &mut tx, &mut fc, &mut rc, &mut fw, &mut rw, &mut work, &guard, &live,
    )
    .unwrap();
    assert_eq!(
        current.iter_factory_ids().collect::<Vec<_>>(),
        vec!["a-current"]
    );
    assert_eq!(current.iter_recipe_job_ids().collect::<Vec<_>>(), vec![19]);
    let guard = tx.guard();
    current
        .validate_current(
            &mut tx, &mut fc, &mut rc, &mut fw, &mut rw, &mut work, &guard, &live,
        )
        .unwrap();
    cancelled.set(true);
    let guard = tx.guard();
    assert!(
        current
            .validate_current(
                &mut tx, &mut fc, &mut rc, &mut fw, &mut rw, &mut work, &guard, &live
            )
            .is_err()
    );
    let guard = tx.guard();
    assert!(tx.seal(&guard, &live, 1).is_err());
}
