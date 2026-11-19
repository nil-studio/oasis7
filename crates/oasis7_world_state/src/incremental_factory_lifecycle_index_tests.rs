use super::*;
use crate::{
    WorldState,
    incremental_field_member::*,
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_test_support::{Pages, Writer, io},
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, TransactionLimits,
    },
    incremental_state::{StateDeltaV1, StateWriteV1},
    incremental_value::ValueNodeWriter,
};
use serde_json::json;
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 100000,
    }
}
fn fw() -> FactoryWorkBudgetV1 {
    FactoryWorkBudgetV1 {
        cells_remaining: 1_000_000,
        max_pending_node_bytes: 8 << 20,
        scan: scan(),
    }
}
fn iw() -> IndustryWorkBudgetV1 {
    IndustryWorkBudgetV1 {
        cells_remaining: 1_000_000,
        max_pending_node_bytes: 8 << 20,
        scan: scan(),
    }
}
fn work() -> FactoryLifecycleIndexWorkV1 {
    FactoryLifecycleIndexWorkV1 {
        cells_remaining: 1_000_000,
        comparison_bytes_remaining: 64 << 20,
        pending_entries_remaining: 10000,
        max_factories: 16,
        max_recipe_jobs: 16,
        max_text_bytes: 4096,
        max_pending_records: 32,
    }
}
struct Fixture {
    memory: MemoryBudget,
    pages: Pages,
    roots: RootSetV1,
}
fn source() -> WorldState {
    // Initial GEN fixture values only: no World restore or execution fallback.
    let mut source = WorldState::default();
    for (outer, completed, stable) in [("a", 5, false), ("b", 8, true)] {
        let mut factory:crate::FactoryState=serde_json::from_value(json!({"factory_id":"embedded","site_id":"site","builder_agent_id":"builder",
            "spec":{"factory_id":"embedded","display_name":"test","tier":1,"tags":[],"build_cost":[],"build_time_ticks":1,"base_power_draw":1,
            "recipe_slots":1,"throughput_bps":1,"maintenance_per_tick":1},"built_at":1})).unwrap();
        factory.production.completed_jobs = completed;
        if stable {
            factory.production.same_recipe_repeat_count = 3;
            factory.production.last_completed_recipe_id = Some("r".into());
            factory.production.last_completed_canonical_snapshot =
                Some(serde_json::from_value(json!({"recipe_id":"r"})).unwrap());
        }
        source.factories.insert(outer.into(), factory);
    }
    for (outer, factory) in [(2, "a"), (9, "a"), (73, "b")] {
        let job=serde_json::from_value(json!({"job_id":999,"requester_agent_id":"builder","factory_id":factory,"recipe_id":"r","accepted_batches":1,
            "consume":[],"produce":[],"byproducts":[],"power_required":0,"duration_ticks":1,"consume_ledger":"world","output_ledger":"world",
            "bottleneck_tags":[],"logistics_route_ids":[],"logistics_path_ids":[],"ready_at":1})).unwrap();
        source.pending_recipe_jobs.insert(outer, job);
    }
    source.industry_progress.completed_recipe_jobs = 999;
    source
}
fn fixture(source: &WorldState) -> Fixture {
    let memory = MemoryBudget::new(512 << 20, 64 << 20).unwrap();
    let pages = Pages::with_limit(&memory, 65536);
    let f = Fixture {
        memory,
        pages,
        roots: RootSetV1::empty("joint-lifecycle".into(), [19; 32]).unwrap(),
    };
    let _phase = f.memory.reserve(48 << 20, MemoryClass::Ordinary).unwrap();
    let mut writer = Writer(&f.pages);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut ctx = MemberRecordWriteContext {
        sink: &mut writer,
        memory: &f.memory,
        io: &mut quota,
        key_work: &mut keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1 << 20,
            payload_bytes: 2 << 20,
            record_bytes: 3 << 20,
        },
        cancelled: || false,
    };
    let mut rows = Vec::new();
    let mut records = 100000;
    prepare_factory_records_genesis_v1(source, &mut ctx, &mut records, &mut fw(), &mut |row| {
        rows.push(row);
        Ok(())
    })
    .unwrap();
    prepare_industry_jobs_genesis_v1(source, &mut ctx, &mut records, &mut iw(), &mut |row| {
        rows.push(row);
        Ok(())
    })
    .unwrap();
    let mut roots = f.roots.clone();
    for root in &mut roots.state {
        let mut writes: Vec<StateWriteV1> = rows
            .iter()
            .filter(|r| r.namespace == root.namespace)
            .map(|r| r.mutation.clone())
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        if writes.is_empty() {
            continue;
        }
        let delta = StateDeltaV1::build(root.clone(), writes, &f.pages, 8 << 20).unwrap();
        for node in &delta.new_nodes {
            Writer(&f.pages)
                .put_node(
                    *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                    f.memory
                        .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
                        .unwrap()
                        .retain(node.bytes.clone()),
                )
                .unwrap();
        }
        *root = delta.after;
    }
    Fixture { roots, ..f }
}
fn qualify(
    f: &Fixture,
) -> (
    ReservedValue<FactoryFamilyQualificationV1>,
    ReservedValue<IndustryFamilyQualificationV1>,
) {
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut outer = scan();
    let fq = {
        let mut ctx = FactoryFamilyReadContextV1 {
            state: &f.pages,
            values: ValueReadContext {
                source: &f.pages,
                memory: &f.memory,
                class: MemoryClass::Ordinary,
                io: &mut quota,
            },
            key_work: &mut keys,
            semantic_bytes: 1 << 20,
            record_bytes: 3 << 20,
            max_payload_bytes: 2 << 20,
            outer_scan: &mut outer,
            take_state_error: || None,
            cancelled: || false,
        };
        qualify_factory_family_v1(&f.roots, &mut ctx, &mut fw()).unwrap()
    };
    let iq = {
        let mut ctx = IndustryFamilyReadContextV1 {
            state: &f.pages,
            values: ValueReadContext {
                source: &f.pages,
                memory: &f.memory,
                class: MemoryClass::Ordinary,
                io: &mut quota,
            },
            key_work: &mut keys,
            semantic_bytes: 1 << 20,
            record_bytes: 3 << 20,
            max_payload_bytes: 2 << 20,
            outer_scan: &mut outer,
            take_state_error: || None,
            cancelled: || false,
        };
        qualify_industry_family_v1(&f.roots, &mut ctx, &mut iw()).unwrap()
    };
    (fq, iq)
}
fn context<'a>(
    f: &'a Fixture,
    quota: &'a mut oasis7_formal_protocol::incremental_value::ValueIoBudget,
    keys: &'a mut SemanticKeyBudget,
) -> WorldFieldReadContextV1<
    'a,
    Pages,
    Pages,
    impl FnMut() -> Option<ValueTreeError>,
    impl FnMut() -> bool,
> {
    WorldFieldReadContextV1 {
        root: f.roots.state_root(StateNamespaceV1::Business),
        state: &f.pages,
        values: ValueReadContext {
            source: &f.pages,
            memory: &f.memory,
            class: MemoryClass::Ordinary,
            io: quota,
        },
        key_work: keys,
        semantic_bytes: 1 << 20,
        record_bytes: 3 << 20,
        take_state_error: || None,
        cancelled: || false,
    }
}
fn build(
    f: &Fixture,
    fq: &ReservedValue<FactoryFamilyQualificationV1>,
    iq: &ReservedValue<IndustryFamilyQualificationV1>,
) -> ReservedValue<FactoryLifecycleIndexV1> {
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut ctx = context(f, &mut quota, &mut keys);
    build_factory_lifecycle_index_v1(
        fq,
        iq,
        &f.roots,
        &mut ctx,
        &mut scan(),
        &mut fw(),
        &mut iw(),
        &mut work(),
        2 << 20,
    )
    .unwrap()
}
fn parent(f: &Fixture) -> ExecutionParent {
    ExecutionParent {
        roots: f.roots.clone(),
        commit: [31; 32],
        authority: AuthorityBinding {
            token: [32; 32],
            epoch: 0,
        },
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 4096,
        max_writes: 128,
        max_facts: 8,
        max_pending_node_bytes: 8 << 20,
    }
}
#[test]
fn joint_index_genuine_genesis_counts_every_job_and_recomputes_without_factory() {
    let source = source();
    let expected_a = source
        .pending_recipe_jobs
        .iter()
        .filter(|(_, j)| j.factory_id == "a")
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    let f = fixture(&source);
    let (fq, iq) = qualify(&f);
    let index = build(&f, &fq, &iq);
    drop(source);
    assert_eq!(index.factory_records(), 2);
    assert_eq!(index.recipe_records(), 3);
    let mut tx =
        ExecutionTransaction::begin(parent(&f), [41; 32], &f.pages, &f.memory, limits()).unwrap();
    let live = tx.parent_fence();
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut ctx = context(&f, &mut quota, &mut keys);
    let mut work = work();
    let mut factory = fw();
    let mut industry = iw();
    for (id, expected_jobs, expected_total, stable, exists) in [
        ("a", expected_a, 8, true, true),
        ("b", vec![73], 5, false, true),
        ("missing", vec![], 13, true, false),
    ] {
        let guard = tx.guard();
        let observation = observe_current_factory_lifecycle_v1(
            id,
            &index,
            &fq,
            &iq,
            &f.roots,
            &mut tx,
            &mut ctx,
            &mut factory,
            &mut industry,
            &mut work,
            2 << 20,
            &guard,
            &live,
        )
        .unwrap();
        assert_eq!(
            observation.recipe_job_ids().collect::<Vec<_>>(),
            expected_jobs
        );
        assert_eq!(
            observation.completed_jobs_without_factory().unwrap(),
            expected_total
        );
        assert_eq!(observation.stable_without_factory(), stable);
        assert!(observation.has_stable_production_line());
        assert_eq!(observation.stage_completed_jobs_total(), 6);
        assert_eq!(observation.factory_exists(), exists);
        observation.validate(&mut tx, &f.memory, &live).unwrap();
    }
}
#[test]
fn joint_index_heap_sort_cancellation_and_comparison_cap_are_prepaid() {
    let mut values = [9, 3, 8, 0, 3, 7, 1];
    let mut w = work();
    sort(
        &mut values,
        &mut w,
        || false,
        |a, b, w| {
            w.cell()?;
            Ok(a.cmp(b))
        },
    )
    .unwrap();
    assert_eq!(values, [0, 1, 3, 3, 7, 8, 9]);
    let mut values = ["long", "a"];
    let mut w = work();
    w.comparison_bytes_remaining = 0;
    assert!(sort(&mut values, &mut w, || false, |a, b, w| w.comparison(a, b)).is_err());
    let mut empty: [u8; 0] = [];
    assert!(matches!(
        sort(&mut empty, &mut work(), || true, |a, b, _| Ok(a.cmp(b))),
        Err(ValueTreeError::Cancelled)
    ));
}
