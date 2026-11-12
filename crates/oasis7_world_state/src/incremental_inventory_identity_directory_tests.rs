use super::*;
use crate::{
    WorldState,
    incremental_inventory_genesis::{
        InventoryGenesisContext, InventoryWriteSink, prepare_inventory_genesis_v1,
    },
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_test_support::{Pages, Writer, io},
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, TransactionLimits,
    },
    incremental_state::{StateDeltaV1, StateWriteV1},
    incremental_value::{ValueNodeWriter, ValueReadContext},
};
fn family_work() -> InventoryFamilyWorkV1 {
    InventoryFamilyWorkV1 {
        records_remaining: 100000,
        index_steps_remaining: 1000000,
        max_semantic_bytes: 4096,
    }
}
fn directory_work() -> InventoryIdentityDirectoryWorkV1 {
    InventoryIdentityDirectoryWorkV1 {
        visits_remaining: 1000000,
        comparison_bytes_remaining: 64 << 20,
        pending_entries_remaining: 100000,
        max_identity_bytes: 4096,
        max_output_entries: 1000,
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1000000,
        read_bytes_remaining: 64 << 20,
        leaves_remaining: 100000,
    }
}
struct Rows(Vec<ReservedValue<StateWriteV1>>);
impl InventoryWriteSink for Rows {
    fn put_write(&mut self, row: ReservedValue<StateWriteV1>) -> Result<(), ValueTreeError> {
        self.0.push(row);
        Ok(())
    }
}
fn source() -> WorldState {
    let mut world = WorldState::default();
    world.resources.clear();
    world.materials.clear();
    world.material_ledgers.clear();
    world.materials.insert("legacy-only".into(), 17);
    world.material_ledgers.insert(
        MaterialLedgerId::World,
        [("zero".into(), 0), ("negative/界".into(), -7)].into(),
    );
    world.material_ledgers.insert(
        MaterialLedgerId::Agent("empty/原".into()),
        Default::default(),
    );
    world.material_ledgers.insert(
        MaterialLedgerId::Site("site:raw".into()),
        [("not-required-kind".into(), 23)].into(),
    );
    world.material_ledgers.insert(
        MaterialLedgerId::Factory("factory:raw".into()),
        [("factory-kind".into(), -11)].into(),
    );
    world
}
fn rows(world: &WorldState, memory: &MemoryBudget, pages: &Pages) -> Rows {
    let mut rows = Rows(Vec::new());
    let mut writer = Writer(pages);
    prepare_inventory_genesis_v1(
        world,
        &mut InventoryGenesisContext {
            values: &mut writer,
            writes: &mut rows,
            memory,
            io: &mut io(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 16 << 20,
            },
            semantic_bytes: 4096,
            record_bytes: 2 << 20,
            records_remaining: 100000,
            cancelled: || false,
        },
    )
    .unwrap();
    rows
}
fn install(world: &WorldState, memory: &MemoryBudget, pages: &Pages) -> RootSetV1 {
    let rows = rows(world, memory, pages);
    let mut writes: Vec<_> = rows.0.iter().map(|r| (**r).clone()).collect();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let mut roots = RootSetV1::empty("inventory-identities".into(), [8; 32]).unwrap();
    let delta = StateDeltaV1::build(
        roots.state_root(StateNamespaceV1::Business).clone(),
        writes,
        pages,
        8 << 20,
    )
    .unwrap();
    for node in &delta.new_nodes {
        Writer(pages)
            .put_node(
                *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                memory
                    .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Business)
        .unwrap() = delta.after;
    roots
}
fn read<'a>(
    memory: &'a MemoryBudget,
    pages: &'a Pages,
    roots: &'a RootSetV1,
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
        root: roots.state_root(StateNamespaceV1::Business),
        state: pages,
        values: ValueReadContext {
            source: pages,
            memory,
            class: MemoryClass::Ordinary,
            io: quota,
        },
        key_work: keys,
        semantic_bytes: 4096,
        record_bytes: 2 << 20,
        take_state_error: || None,
        cancelled: || false,
    }
}
fn recover(
    ledger: &MaterialLedgerId,
    directory: &ReservedValue<InventoryIdentityDirectoryV1>,
    family: &ReservedValue<InventoryFamilyQualificationV1>,
    roots: &RootSetV1,
    tx: &mut ExecutionTransaction<'_, Pages>,
    memory: &MemoryBudget,
    pages: &Pages,
    family_work: &mut InventoryFamilyWorkV1,
    work: &mut InventoryIdentityDirectoryWorkV1,
    live: &oasis7_formal_protocol::incremental_execution::ExecutionFence,
) -> ReservedValue<Option<std::collections::BTreeMap<String, i64>>> {
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
    recover_current_inventory_ledger_v1(
        ledger,
        directory,
        family,
        roots,
        &mut current,
        &mut read(memory, pages, roots, &mut parent_io, &mut parent_keys),
        family_work,
        work,
        &guard,
        live,
    )
    .unwrap()
}
#[test]
fn qualified_directory_keeps_every_ledger_and_material_and_folds_real_pending_rows() {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let _fixture = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::with_limit(&memory, 16384);
    let mut world = source();
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
    let absent = MaterialLedgerId::Site("absent".into());
    for ledger in world
        .material_ledgers
        .keys()
        .chain(std::iter::once(&absent))
    {
        let actual = recover(
            ledger, &directory, &family, &roots, &mut tx, &memory, &pages, &mut fw, &mut work,
            &live,
        );
        assert_eq!(actual.as_ref(), world.material_ledgers.get(ledger));
    }
    // Build actual original member rows for a genuinely new material identity;
    // the resident parent directory remains unchanged and learns it only from
    // the transaction's real after-value, not a supplied identity list.
    world
        .material_ledgers
        .get_mut(&MaterialLedgerId::World)
        .unwrap()
        .insert("new-unrequested-kind".into(), -99);
    let desired = rows(&world, &memory, &pages);
    let semantic = crate::incremental_semantic_key::hash_semantic_key_v1(
        &(MaterialLedgerId::World, "new-unrequested-kind"),
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
    let actual = recover(
        &MaterialLedgerId::World,
        &directory,
        &family,
        &roots,
        &mut tx,
        &memory,
        &pages,
        &mut fw,
        &mut work,
        &live,
    );
    assert_eq!(
        actual.as_ref(),
        world.material_ledgers.get(&MaterialLedgerId::World)
    );
    // Deleting the real group of an originally empty ledger yields None,
    // preserving the original get() domain used by the starter hash.
    let empty = MaterialLedgerId::Agent("empty/原".into());
    let semantic = crate::incremental_semantic_key::hash_semantic_key_v1(
        &empty,
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
        WorldFieldRecordKindV1::Group,
        Some(semantic.digest),
    )
    .unwrap();
    tx.write(StateNamespaceV1::Business, key.as_bytes(), None)
        .unwrap();
    assert!(
        recover(
            &empty, &directory, &family, &roots, &mut tx, &memory, &pages, &mut fw, &mut work,
            &live
        )
        .is_none()
    );
}

#[path = "incremental_inventory_identity_directory_lifecycle_tests.rs"]
mod lifecycle;

#[path = "incremental_inventory_identity_directory_total_tests.rs"]
mod total_balance;
