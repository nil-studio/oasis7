//! Storage-owner integration through genuine GEN; this does not approve recycling.
use super::*;
use oasis7_world_state::{
    FactoryRecycleReceiptV1, MaterialLedgerId, MaterialStack,
    events::{IndustryStage, MaterialMarketQuote},
    incremental_field_member::{MemberRecordLimitsV1, MemberRecordWriteContext},
    incremental_field_read::WorldFieldReadContextV1,
    incremental_world_native::{
        WorldNativeSelectedReadWorkV1 as TypedRecordReadWorkV1,
        change_current_factory_industry_stage_v1, recover_current_factory_industry_progress_v1,
        recover_current_factory_recycle_receipt_v1, write_current_factory_recycle_receipt_v1,
    },
    incremental_world_state_genesis::TypedRecordWriteWorkV1,
};

#[test]
fn current_factory_native_records_preserve_unrelated_progress_and_receipts() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = industrial_eligibility::producing_world();
    world.state.industry_progress.stage = IndustryStage::Bootstrap;
    world.state.industry_progress.stage_updated_at = 7;
    world.state.industry_progress.completed_recipe_jobs = 93;
    world.state.industry_progress.completed_material_transits = 19;
    world.state.industry_progress.latest_market_quotes.insert(
        "retained-quote".into(),
        MaterialMarketQuote {
            kind: "hardware_part".into(),
            requested_amount: 3,
            local_available_amount: 4,
            world_available_amount: 5,
            local_deficit_amount: 6,
            transit_loss_bps: 7,
            governance_tax_bps: 8,
            effective_cost_index_ppm: 900_001,
        },
    );
    let receipt = FactoryRecycleReceiptV1 {
        operator_agent_id: "owner".into(),
        factory_id: "retired-before-gen".into(),
        recycle_ledger: MaterialLedgerId::World,
        recovered: vec![MaterialStack {
            kind: "iron".into(),
            amount: 13,
        }],
        durability_ppm: 700_000,
    };
    world
        .state
        .factory_recycle_receipts
        .insert("outer-receipt".into(), receipt.clone());
    let mut expected_progress = world.state.industry_progress.clone();
    expected_progress.stage = IndustryStage::Governance;
    expected_progress.stage_updated_at = 17;
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [149; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: EXECUTION_MAX_READS,
            max_writes: 1024,
            max_facts: 64,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let live = tx.parent_fence();
    let mut io = genesis::io();
    let mut keys = genesis::keys();
    let mut read = WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Authorization),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut scan = genesis::scan();
    let mut typed = TypedRecordReadWorkV1 {
        json: genesis::json(),
        max_retained_trees: 4096,
        max_tree_heap: 16 << 20,
    };
    let guard = tx.guard();
    let prior = recover_current_factory_recycle_receipt_v1(
        "outer-receipt",
        fixture.current.native().native(),
        &fixture.roots,
        &mut tx,
        &mut read,
        &mut scan,
        &mut typed,
        &guard,
        &live,
    )
    .unwrap();
    assert_eq!(prior.value(), Some(&receipt));
    drop(prior);
    let guard = tx.guard();
    let absent = recover_current_factory_recycle_receipt_v1(
        "new-receipt",
        fixture.current.native().native(),
        &fixture.roots,
        &mut tx,
        &mut read,
        &mut scan,
        &mut typed,
        &guard,
        &live,
    )
    .unwrap();
    assert_eq!(absent.value(), None);
    let mut sink = fixture.cas.clone();
    let mut output_io = genesis::io();
    let mut output_keys = genesis::keys();
    let mut writer = MemberRecordWriteContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut output_io,
        key_work: &mut output_keys,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 16 << 20,
            record_bytes: 16 << 20,
        },
        cancelled: || false,
    };
    let mut write_work = TypedRecordWriteWorkV1 {
        cells_remaining: 1_000_000,
        max_pending_node_bytes: 16 << 20,
        json: oasis7_world_state::incremental_json_records::JsonWriteWorkV1 {
            cells_remaining: 1_000_000,
            json_nodes_remaining: 1_000_000,
            max_json_depth: 64,
            max_pending_node_bytes: 16 << 20,
        },
    };
    let mut records = 16;
    write_current_factory_recycle_receipt_v1(
        &absent,
        "new-receipt",
        &receipt,
        &mut tx,
        &mut read,
        &mut writer,
        &mut records,
        &mut write_work,
        &live,
    )
    .unwrap();
    let guard = tx.guard();
    let current = recover_current_factory_recycle_receipt_v1(
        "new-receipt",
        fixture.current.native().native(),
        &fixture.roots,
        &mut tx,
        &mut read,
        &mut scan,
        &mut typed,
        &guard,
        &live,
    )
    .unwrap();
    assert_eq!(current.value(), Some(&receipt));
    drop(current);
    read.root = fixture.roots.state_root(StateNamespaceV1::Business);
    let guard = tx.guard();
    let progress = recover_current_factory_industry_progress_v1(
        fixture.current.native().native(),
        &fixture.roots,
        &mut tx,
        &mut read,
        &mut scan,
        &mut typed,
        &guard,
        &live,
    )
    .unwrap();
    assert!(
        change_current_factory_industry_stage_v1(
            &progress,
            IndustryStage::Governance,
            17,
            &mut tx,
            &mut read,
            &mut writer,
            &mut scan,
            &mut typed,
            &mut write_work,
            &live,
        )
        .unwrap()
    );
    let guard = tx.guard();
    let progress = recover_current_factory_industry_progress_v1(
        fixture.current.native().native(),
        &fixture.roots,
        &mut tx,
        &mut read,
        &mut scan,
        &mut typed,
        &guard,
        &live,
    )
    .unwrap();
    assert_eq!(progress.value(), Some(&expected_progress));
    // A no-op must preserve the previous transition time, regardless of caller time.
    assert!(
        !change_current_factory_industry_stage_v1(
            &progress,
            IndustryStage::Governance,
            999,
            &mut tx,
            &mut read,
            &mut writer,
            &mut scan,
            &mut typed,
            &mut write_work,
            &live,
        )
        .unwrap()
    );
    let guard = tx.guard();
    let progress = recover_current_factory_industry_progress_v1(
        fixture.current.native().native(),
        &fixture.roots,
        &mut tx,
        &mut read,
        &mut scan,
        &mut typed,
        &guard,
        &live,
    )
    .unwrap();
    assert_eq!(progress.value(), Some(&expected_progress));
    read.root = fixture.roots.state_root(StateNamespaceV1::Authorization);
    let guard = tx.guard();
    let prior = recover_current_factory_recycle_receipt_v1(
        "outer-receipt",
        fixture.current.native().native(),
        &fixture.roots,
        &mut tx,
        &mut read,
        &mut scan,
        &mut typed,
        &guard,
        &live,
    )
    .unwrap();
    assert_eq!(prior.value(), Some(&receipt));
    drop(prior);
    read.root = fixture.roots.state_root(StateNamespaceV1::Business);
    let mut factories = oasis7_world_state::incremental_factory_records::FactoryWorkBudgetV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 16 << 20,
        scan: genesis::scan(),
    };
    let guard = tx.guard();
    let deletion =
        oasis7_world_state::incremental_factory_records::prepare_current_factory_delete_v1(
            "assembler",
            fixture.current.native().factory(),
            &fixture.roots,
            &mut tx,
            &mut read,
            &mut factories,
            1 << 20,
            &guard,
            &live,
        )
        .unwrap();
    assert_eq!(deletion.factory_id(), "assembler");
    oasis7_world_state::incremental_factory_records::delete_prepared_current_factory_v1(
        &deletion, &mut tx, &mut read, &live,
    )
    .unwrap();
    let mut industry = oasis7_world_state::incremental_industry_jobs::IndustryWorkBudgetV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 16 << 20,
        scan: genesis::scan(),
    };
    let mut index =
        oasis7_world_state::incremental_factory_lifecycle_index::FactoryLifecycleIndexWorkV1 {
            cells_remaining: 100_000,
            comparison_bytes_remaining: 64 << 20,
            pending_entries_remaining: 100_000,
            max_factories: 8,
            max_recipe_jobs: 8,
            max_text_bytes: 4096,
            max_pending_records: 32,
        };
    let guard = tx.guard();
    let observation = oasis7_world_state::incremental_factory_lifecycle_index::observe_current_factory_lifecycle_v1(
        "assembler", fixture.current.native().factory_lifecycle_index(),
        fixture.current.native().factory(), fixture.current.native().industry(),
        &fixture.roots, &mut tx, &mut read, &mut factories, &mut industry,
        &mut index, 1 << 20, &guard, &live,
    ).unwrap();
    assert!(!observation.factory_exists());
    assert_eq!(observation.active_jobs(), 0);
    observation.validate(&mut tx, &memory, &live).unwrap();
    assert_eq!(tx.ordered_fact_count(), 0);
    let guard = tx.guard();
    tx.seal(&guard, &live, 1).unwrap();
}

#[test]
fn current_factory_stage_threshold_matches_legacy_overflow_and_removal() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = industrial_eligibility::producing_world();
    let prototype = world.state.factories["assembler"].clone();
    world.state.factories.clear();
    world.state.gameplay_policy.electricity_tax_bps = 1;
    for (id, jobs, stable) in [
        ("large", u64::MAX, false),
        ("stable", 1, true),
        ("zero", 0, false),
    ] {
        let mut factory = prototype.clone();
        factory.production.completed_jobs = jobs;
        factory.production.same_recipe_repeat_count = if stable { 3 } else { 0 };
        factory.production.last_completed_recipe_id = Some("r".into());
        factory.production.last_completed_canonical_snapshot =
            Some(serde_json::from_value(serde_json::json!({"recipe_id":"r"})).unwrap());
        world.state.factories.insert(id.into(), factory);
    }
    let mut oracle = world.state.clone();
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [150; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: EXECUTION_MAX_READS,
            max_writes: 1024,
            max_facts: 64,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let live = tx.parent_fence();
    let mut io = genesis::io();
    let mut keys = genesis::keys();
    let mut read = WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Business),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut factories = oasis7_world_state::incremental_factory_records::FactoryWorkBudgetV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 16 << 20,
        scan: genesis::scan(),
    };
    let mut industry = oasis7_world_state::incremental_industry_jobs::IndustryWorkBudgetV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 16 << 20,
        scan: genesis::scan(),
    };
    let mut index =
        oasis7_world_state::incremental_factory_lifecycle_index::FactoryLifecycleIndexWorkV1 {
            cells_remaining: 100_000,
            comparison_bytes_remaining: 64 << 20,
            pending_entries_remaining: 100_000,
            max_factories: 8,
            max_recipe_jobs: 8,
            max_text_bytes: 4096,
            max_pending_records: 32,
        };
    for (id, now, threshold, expected_stage) in [
        ("zero", 11, 6, IndustryStage::Governance),
        ("large", 12, 1, IndustryStage::ScaleOut),
    ] {
        oracle
            .apply_domain_event(
                &crate::DomainEvent::FactoryRecycled {
                    operator_agent_id: "owner".into(),
                    factory_id: id.into(),
                    recycle_ledger: MaterialLedgerId::World,
                    recovered: vec![],
                    durability_ppm: prototype.durability_ppm,
                },
                now,
            )
            .unwrap();
        assert_eq!(oracle.industry_progress.stage, expected_stage);
        // Storage deletion is isolated here: the complete recycle publication
        // (receipt, retired set, balances, mailbox) belongs to the action owner.
        let guard = tx.guard();
        let deletion =
            oasis7_world_state::incremental_factory_records::prepare_current_factory_delete_v1(
                id,
                fixture.current.native().factory(),
                &fixture.roots,
                &mut tx,
                &mut read,
                &mut factories,
                1 << 20,
                &guard,
                &live,
            )
            .unwrap();
        oasis7_world_state::incremental_factory_records::delete_prepared_current_factory_v1(
            &deletion, &mut tx, &mut read, &live,
        )
        .unwrap();
        let guard = tx.guard();
        let observation = oasis7_world_state::incremental_factory_lifecycle_index::observe_current_factory_lifecycle_v1(
            "absent-probe", fixture.current.native().factory_lifecycle_index(),
            fixture.current.native().factory(), fixture.current.native().industry(),
            &fixture.roots, &mut tx, &mut read, &mut factories, &mut industry,
            &mut index, 1 << 20, &guard, &live,
        ).unwrap();
        assert_eq!(
            observation.stage_completed_jobs_without_factory(),
            threshold
        );
        let stage = oasis7_world_state::state::industry_transition::industry_stage_for_progress(
            observation.stable_without_factory(),
            observation.stage_completed_jobs_without_factory(),
            oracle.industry_progress.completed_material_transits,
            true,
        );
        assert_eq!(stage, oracle.industry_progress.stage);
        observation.validate(&mut tx, &memory, &live).unwrap();
    }
    assert_eq!(tx.ordered_fact_count(), 0);
    let guard = tx.guard();
    tx.seal(&guard, &live, 1).unwrap();
}
