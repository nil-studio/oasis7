use super::*;
fn sum(
    material: &str,
    directory: &ReservedValue<InventoryIdentityDirectoryV1>,
    family: &ReservedValue<InventoryFamilyQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, Pages>,
    memory: &MemoryBudget,
    pages: &Pages,
    fw: &mut InventoryFamilyWorkV1,
    work: &mut InventoryIdentityDirectoryWorkV1,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
) -> i64 {
    let mut current_io = io();
    let mut parent_io = io();
    let mut current_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut parent_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let guard = tx.guard();
    let mut current = crate::incremental_field_overlay::WorldFieldOverlayContextV1 {
        transaction: tx,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut current_io,
        },
        key_work: &mut current_keys,
        semantic_bytes: 4096,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled: || false,
    };
    *sum_current_inventory_material_v1(
        material,
        directory,
        family,
        roots,
        &mut current,
        &mut read(memory, pages, roots, &mut parent_io, &mut parent_keys),
        fw,
        work,
        &guard,
        live,
    )
    .unwrap()
}
#[test]
fn actual_all_ledger_total_preserves_negative_zero_missing_and_pending_members() {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 16384);
    let mut world = source();
    world.materials.insert("shared/kind".into(), 1000); // Original total excludes legacy materials.
    world
        .material_ledgers
        .get_mut(&MaterialLedgerId::World)
        .unwrap()
        .insert("shared/kind".into(), 7);
    world
        .material_ledgers
        .get_mut(&MaterialLedgerId::Site("site:raw".into()))
        .unwrap()
        .insert("shared/kind".into(), -2);
    world
        .material_ledgers
        .get_mut(&MaterialLedgerId::Factory("factory:raw".into()))
        .unwrap()
        .insert("shared/kind".into(), 0);
    let roots = install(&world, &memory, &pages);
    let mut quota = io();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut fw = family_work();
    let mut work = directory_work();
    let family = qualify_inventory_family_v1(
        &roots,
        &mut read(&memory, &pages, &roots, &mut quota, &mut keys),
        &mut scan(),
        &mut fw,
    )
    .unwrap();
    let directory = build_inventory_identity_directory_v1(
        &family,
        &roots,
        &mut read(&memory, &pages, &roots, &mut quota, &mut keys),
        &mut scan(),
        &mut fw,
        &mut work,
    )
    .unwrap();
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [1; 32],
        authority: AuthorityBinding {
            token: [2; 32],
            epoch: 3,
        },
    };
    let live = parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent,
        [3; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 128,
            max_writes: 16,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    assert_eq!(
        sum(
            "shared/kind",
            &directory,
            &family,
            &roots,
            &mut tx,
            &memory,
            &pages,
            &mut fw,
            &mut work,
            &live
        ),
        5
    );
    assert_eq!(
        sum(
            "entirely-absent",
            &directory,
            &family,
            &roots,
            &mut tx,
            &memory,
            &pages,
            &mut fw,
            &mut work,
            &live
        ),
        0
    );
    let agent = MaterialLedgerId::Agent("empty/原".into());
    world
        .material_ledgers
        .get_mut(&MaterialLedgerId::World)
        .unwrap()
        .insert("shared/kind".into(), 9);
    world
        .material_ledgers
        .get_mut(&agent)
        .unwrap()
        .insert("shared/kind".into(), -5);
    let desired = rows(&world, &memory, &pages);
    for ledger in [&MaterialLedgerId::World, &agent] {
        let semantic = crate::incremental_semantic_key::hash_semantic_key_v1(
            &(ledger, "shared/kind"),
            &memory,
            MemoryClass::Ordinary,
            &mut keys,
            || false,
        )
        .unwrap();
        let key = WorldFieldKeyV1::new(
            descriptor(
                "material_ledgers",
                crate::incremental_projection::FieldShape::NestedMap,
            )
            .unwrap(),
            WorldFieldRecordKindV1::Entry,
            Some(semantic.digest),
        )
        .unwrap();
        let row = desired.0.iter().find(|r| r.key == key.as_bytes()).unwrap();
        tx.write(StateNamespaceV1::Business, key.as_bytes(), row.after)
            .unwrap();
    }
    let original_total: i64 = world
        .material_ledgers
        .values()
        .filter_map(|ledger| ledger.get("shared/kind"))
        .copied()
        .sum();
    assert_eq!(original_total, 2);
    assert_eq!(
        sum(
            "shared/kind",
            &directory,
            &family,
            &roots,
            &mut tx,
            &memory,
            &pages,
            &mut fw,
            &mut work,
            &live
        ),
        original_total
    );
}
