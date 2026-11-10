//! Child of module_execution_tests; real Current points and old WorldState oracle.
use super::*;
use crate::world::incremental_material_normalization::*;
use oasis7_world_state::{
    MaterialLedgerId, incremental_inventory::*, incremental_inventory_change::InventoryChangeRefV1,
    incremental_inventory_read::LedgerQuantityV1,
};
#[test]
fn current_material_normalization_matches_core_policy_for_empty_fallback_signed_and_conflicting_maps()
 {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    world.state.materials.clear();
    world.state.material_ledgers.clear();
    let other = MaterialLedgerId::Agent("other".into());
    world
        .state
        .material_ledgers
        .insert(other.clone(), [("untouched".into(), -7)].into());
    let event = crate::DomainEvent::ActionRejected {
        action_id: 1,
        reason: crate::RejectReason::AgentNotFound {
            agent_id: "missing".into(),
        },
    };
    let mut oracle = world.state.clone();
    let mut expected = Vec::new();
    for phase in 0..3 {
        if phase == 1 {
            oracle.materials = [
                ("zero".into(), 0),
                ("negative".into(), -9),
                ("legacy_only".into(), 4),
            ]
            .into();
        }
        if phase == 2 {
            oracle.material_ledgers.insert(
                MaterialLedgerId::World,
                [
                    ("zero".into(), 0),
                    ("negative".into(), -11),
                    ("world_only".into(), 13),
                    ("conflict".into(), -1),
                ]
                .into(),
            );
        }
        oracle.apply_domain_event(&event, 0).unwrap();
        assert_eq!(
            oracle.materials,
            oracle.material_ledgers[&MaterialLedgerId::World]
        );
        assert_eq!(oracle.material_ledgers[&other]["untouched"], -7);
        expected.push(oracle.materials.clone());
    }
    drop(oracle);
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut parent_io = genesis::io();
    let mut parent_keys = genesis::keys();
    let mut parent_read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Business),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut parent_io,
        },
        key_work: &mut parent_keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut family = InventoryFamilyWorkV1 {
        records_remaining: 1000000,
        index_steps_remaining: 1000000,
        max_semantic_bytes: 4096,
    };
    // The three fixture states contain at most four entries per selected map;
    // `legacy_only` is the longest material key. Current opening prepays
    // (2 * max_entries)^2 * max_key_bytes * 16 on EACH of seven openings, so
    // these declarations describe the actual fixture rather than a large
    // unrelated protocol capacity. The continuous comparison quota stays fixed.
    const MAX_ENTRIES_PER_MAP: usize = 4;
    const MAX_MATERIAL_BYTES: usize = "legacy_only".len();
    let mut scope_work = InventoryMaterialScopeWorkV1 {
        max_entries_per_map: MAX_ENTRIES_PER_MAP,
        max_material_bytes: MAX_MATERIAL_BYTES,
        pending_entries_remaining: 100000,
        comparison_bytes_remaining: 64 << 20,
    };
    let parent = build_parent_inventory_material_scope_v1(
        fixture.current.native().inventory(),
        &fixture.roots,
        &mut parent_read,
        &mut genesis::scan(),
        &mut family,
        &mut scope_work,
    )
    .unwrap();
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [132; 32],
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
    let mut io = genesis::io();
    let mut keys = genesis::keys();
    let mut ctx = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
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
    let mut write_io = genesis::io();
    let mut write_keys = genesis::keys();
    let mut sink = fixture.cas.clone();
    let mut write = genesis::member(&mut sink, &memory, &mut write_io, &mut write_keys, || false);
    let mut group_work = InventoryWorldGroupWorkV1 {
        steps_remaining: 100000,
    };
    let mut cells = 100000;
    let mut comparisons = 64 << 20;
    for (phase, expected) in expected.iter().enumerate() {
        if phase == 1 {
            for (key, quantity) in [("zero", 0), ("negative", -9), ("legacy_only", 4)] {
                let guard = ctx.transaction.guard();
                ctx.change(
                    InventoryChangeRefV1::Material {
                        key,
                        expected: None,
                        after: Some(quantity),
                    },
                    &mut write,
                    &guard,
                    &live,
                )
                .unwrap();
            }
        }
        if phase == 2 {
            for (material, before, after) in [
                ("legacy_only", Some(4), None),
                ("negative", Some(-9), Some(-11)),
                ("world_only", None, Some(13)),
                ("conflict", None, Some(-1)),
            ] {
                let guard = ctx.transaction.guard();
                ctx.change(
                    InventoryChangeRefV1::LedgerMaterial {
                        ledger: &MaterialLedgerId::World,
                        material,
                        expected: before
                            .map_or(LedgerQuantityV1::MaterialAbsent, LedgerQuantityV1::Value),
                        after,
                        create_ledger: false,
                    },
                    &mut write,
                    &guard,
                    &live,
                )
                .unwrap();
            }
        }
        let guard = ctx.transaction.guard();
        let source = open_current_inventory_material_scope_v1(
            &parent,
            &fixture.roots,
            &reader,
            &mut ctx,
            &mut family,
            &mut scope_work,
            &guard,
            &live,
        )
        .unwrap();
        let guard = ctx.transaction.guard();
        normalize_current_materials_v1(
            &source,
            &mut ctx,
            &mut write,
            &mut group_work,
            &mut MaterialNormalizationWorkV1 {
                max_union_entries: 2 * MAX_ENTRIES_PER_MAP,
                cells_remaining: &mut cells,
                comparison_bytes_remaining: &mut comparisons,
            },
            &guard,
            &live,
        )
        .unwrap();
        for key in ["zero", "negative", "legacy_only", "world_only", "conflict"] {
            assert_eq!(ctx.material(key).unwrap(), expected.get(key).copied());
            assert_eq!(
                ctx.ledger(&MaterialLedgerId::World, key).unwrap(),
                expected
                    .get(key)
                    .copied()
                    .map_or(LedgerQuantityV1::MaterialAbsent, LedgerQuantityV1::Value)
            );
        }
        assert_eq!(
            ctx.ledger(&other, "untouched").unwrap(),
            LedgerQuantityV1::Value(-7)
        );
        let guard = ctx.transaction.guard();
        let after = open_current_inventory_material_scope_v1(
            &parent,
            &fixture.roots,
            &reader,
            &mut ctx,
            &mut family,
            &mut scope_work,
            &guard,
            &live,
        )
        .unwrap();
        assert!(
            after.world_ledger_present(),
            "empty normalization must create the actual World group"
        );
        assert_eq!(after.legacy_materials().len(), expected.len());
        assert_eq!(after.world_materials().len(), expected.len());
    }
    // A source-dependent entrypoint preserves zero work as a host failure and
    // poisons the complete transaction rather than keeping provisional changes.
    let guard = ctx.transaction.guard();
    let source = open_current_inventory_material_scope_v1(
        &parent,
        &fixture.roots,
        &reader,
        &mut ctx,
        &mut family,
        &mut scope_work,
        &guard,
        &live,
    )
    .unwrap();
    let guard = ctx.transaction.guard();
    cells = 0;
    assert!(matches!(
        normalize_current_materials_v1(
            &source,
            &mut ctx,
            &mut write,
            &mut group_work,
            &mut MaterialNormalizationWorkV1 {
                max_union_entries: 2 * MAX_ENTRIES_PER_MAP,
                cells_remaining: &mut cells,
                comparison_bytes_remaining: &mut comparisons
            },
            &guard,
            &live
        ),
        Err(MaterialNormalizationErrorV1::Value(
            ValueTreeError::ResourceLimited(_)
        ))
    ));
    let guard = ctx.transaction.guard();
    assert!(matches!(
        tx.seal(&guard, &live, fixture.roots.height + 1),
        Err(ExecutionError::Aborted)
    ));
}
