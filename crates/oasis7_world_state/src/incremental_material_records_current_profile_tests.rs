use super::*;
use crate::{
    WorldState,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_material_records::MaterialWorkBudgetV1,
    incremental_semantic_key::SemanticKeyBudget,
    incremental_typed_record_test_support::{Pages, Writer, io},
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionError, ExecutionParent, ExecutionTransaction, RootSetV1,
        TransactionLimits,
    },
    incremental_state::{StateDeltaV1, StateNamespaceV1, StateScanBudget, StateWriteV1},
    incremental_value::{ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};

fn no_error() -> Option<ValueTreeError> {
    None
}

fn not_cancelled() -> bool {
    false
}

struct Fixture {
    memory: MemoryBudget,
    pages: Pages,
    roots: RootSetV1,
    family: ReservedValue<MaterialFamilyQualificationV1>,
    _input: oasis7_proto::memory_budget::MemoryReservation,
}

fn fixture() -> Fixture {
    fixture_with_profile(true)
}

fn fixture_with_profile(profile_present: bool) -> Fixture {
    let memory = MemoryBudget::new(128 << 20, 16 << 20).unwrap();
    let input = memory.reserve(24 << 20, MemoryClass::Ordinary).unwrap();
    let pages = Pages::new(&memory);
    let mut source = WorldState::default();
    source.material_profiles.clear();
    source.direct_material_transfer_receipts.clear();
    if profile_present {
        source.material_profiles.insert(
            "ore".into(),
            MaterialProfileV1 {
                kind: "ore".into(),
                tier: 1,
                category: "raw".into(),
                stack_limit: 64,
                transport_loss_class: oasis7_wasm_abi::MaterialTransportLossClass::Low,
                decay_bps_per_tick: 0,
                default_priority: MaterialDefaultPriority::Standard,
            },
        );
    }

    let mut writes = Vec::<
        ReservedValue<oasis7_formal_protocol::incremental_execution::ExecutionWriteV1>,
    >::new();
    let mut writer = Writer(&pages);
    let mut write_io = io();
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 4 << 20,
    };
    prepare_material_records_genesis_v1(
        &source,
        &mut MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 2 << 20,
                payload_bytes: 2 << 20,
                record_bytes: 4 << 20,
            },
            cancelled: not_cancelled,
        },
        &mut 10_000,
        &mut MaterialWorkBudgetV1 {
            cells_remaining: 10_000,
            max_pending_node_bytes: 8 << 20,
            scan: StateScanBudget {
                nodes_remaining: 100_000,
                leaves_remaining: 10_000,
                read_bytes_remaining: 64 << 20,
            },
        },
        &mut |write| {
            writes.push(write);
            Ok(())
        },
    )
    .unwrap();

    let mut roots = RootSetV1::empty("current-material-profile".into(), [0x27; 32]).unwrap();
    for namespace in [StateNamespaceV1::Business, StateNamespaceV1::Authorization] {
        let mut batch: Vec<StateWriteV1> = writes
            .iter()
            .filter(|write| write.namespace == namespace)
            .map(|write| write.mutation.clone())
            .collect();
        batch.sort_by(|left, right| left.key.cmp(&right.key));
        let delta =
            StateDeltaV1::build(roots.state_root(namespace).clone(), batch, &pages, 8 << 20)
                .unwrap();
        for node in &delta.new_nodes {
            let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
            writer
                .put_node(
                    id,
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
            .find(|root| root.namespace == namespace)
            .unwrap() = delta.after;
    }

    let mut outer_scan = scan();
    let mut inner_scan = scan();
    let mut cells_remaining = 100_000;
    let family = qualify_material_family_v1(
        &roots,
        &mut WorldFieldReadContextV1 {
            root: roots.state_root(StateNamespaceV1::Business),
            state: &pages,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            semantic_bytes: 2 << 20,
            record_bytes: 4 << 20,
            take_state_error: no_error,
            cancelled: not_cancelled,
        },
        &mut outer_scan,
        &mut inner_scan,
        &mut cells_remaining,
        2 << 20,
    )
    .unwrap();
    Fixture {
        memory,
        pages,
        roots,
        family,
        _input: input,
    }
}

fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 200_000,
        leaves_remaining: 20_000,
        read_bytes_remaining: 64 << 20,
    }
}

fn transaction(fixture: &Fixture) -> ExecutionTransaction<'_, Pages> {
    ExecutionTransaction::begin(
        ExecutionParent {
            roots: fixture.roots.clone(),
            commit: [0x41; 32],
            authority: AuthorityBinding {
                token: [0x52; 32],
                epoch: 1,
            },
        },
        [0x63; 32],
        &fixture.pages,
        &fixture.memory,
        TransactionLimits {
            max_reads: 512,
            max_writes: 64,
            max_facts: 8,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap()
}

fn work() -> MaterialProfileCurrentReadWorkV1 {
    MaterialProfileCurrentReadWorkV1 {
        scan: scan(),
        cells_remaining: 100_000,
        max_payload_bytes: 2 << 20,
    }
}

fn read_current_priority_in_fresh_transaction(
    fixture: &Fixture,
) -> Option<MaterialDefaultPriority> {
    let mut tx = transaction(fixture);
    let live = tx.parent_fence();
    let mut current_io = io();
    let mut current_keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut current = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &fixture.pages,
            memory: &fixture.memory,
            class: MemoryClass::Ordinary,
            io: &mut current_io,
        },
        key_work: &mut current_keys,
        semantic_bytes: 2 << 20,
        record_bytes: 4 << 20,
        take_state_error: no_error,
        cancelled: not_cancelled,
    };
    let guard = current.transaction.guard();
    read_current_material_default_priority_overlay_v1(
        "ore",
        &fixture.family,
        &fixture.pages,
        &mut current,
        &mut work(),
        &guard,
        &live,
    )
    .unwrap()
}

#[test]
fn fresh_transactions_read_some_and_absent_current_profiles_without_prewarming() {
    assert_eq!(
        read_current_priority_in_fresh_transaction(&fixture()),
        Some(MaterialDefaultPriority::Standard)
    );
    assert_eq!(
        read_current_priority_in_fresh_transaction(&fixture_with_profile(false)),
        None
    );
}

#[test]
fn stale_entry_guard_is_rejected_and_poisons_transaction() {
    let fixture = fixture();
    let mut tx = transaction(&fixture);
    let live = tx.parent_fence();
    let stale_guard = tx.guard();
    tx.write(StateNamespaceV1::Business, b"guard-probe", None)
        .unwrap();

    let result = {
        let mut current_io = io();
        let mut current_keys = SemanticKeyBudget {
            bytes_remaining: 8 << 20,
        };
        let mut current = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &fixture.pages,
                memory: &fixture.memory,
                class: MemoryClass::Ordinary,
                io: &mut current_io,
            },
            key_work: &mut current_keys,
            semantic_bytes: 2 << 20,
            record_bytes: 4 << 20,
            take_state_error: no_error,
            cancelled: not_cancelled,
        };
        read_current_material_default_priority_overlay_v1(
            "ore",
            &fixture.family,
            &fixture.pages,
            &mut current,
            &mut work(),
            &stale_guard,
            &live,
        )
    };
    assert!(matches!(
        result,
        Err(WorldFieldOverlayErrorV1::Execution(_))
    ));
    let aborted_guard = tx.guard();
    assert!(matches!(
        tx.seal(&aborted_guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}

#[test]
fn current_profile_uses_pending_child_and_distinguishes_absent_from_corrupt() {
    let fixture = fixture();

    // Update only the Current child. The original family qualification remains
    // parent-bound, while the reader must close the newly referenced child.
    let mut tx = transaction(&fixture);
    let live = tx.parent_fence();
    let parent_proof = open_qualified_material_record_v1(
        MaterialRecordKeyV1::Profile("ore"),
        &fixture.roots,
        &fixture.family,
        &mut WorldFieldReadContextV1 {
            root: fixture.roots.state_root(StateNamespaceV1::Business),
            state: &fixture.pages,
            values: ValueReadContext {
                source: &fixture.pages,
                memory: &fixture.memory,
                class: MemoryClass::Ordinary,
                io: &mut io(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            semantic_bytes: 2 << 20,
            record_bytes: 4 << 20,
            take_state_error: no_error,
            cancelled: not_cancelled,
        },
    )
    .unwrap()
    .unwrap();
    let mut write_io = io();
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut writer = Writer(&fixture.pages);
    let guard = tx.guard();
    change_qualified_material_cell_v1(
        MaterialCellChangeV1 {
            key: MaterialRecordKeyV1::Profile("ore"),
            cell: MaterialCellV1::DefaultPriority,
            qualification: &parent_proof,
            expected: Some(MaterialCellValueRefV1::U64(1)),
            after: Some(MaterialCellValueRefV1::U64(0)),
            max_current_payload_bytes: 8,
            max_pending_node_bytes: 8 << 20,
        },
        &mut WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &fixture.pages,
                memory: &fixture.memory,
                class: MemoryClass::Ordinary,
                io: &mut io(),
            },
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            semantic_bytes: 2 << 20,
            record_bytes: 4 << 20,
            take_state_error: no_error,
            cancelled: not_cancelled,
        },
        &mut MemberRecordWriteContext {
            sink: &mut writer,
            memory: &fixture.memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 2 << 20,
                payload_bytes: 2 << 20,
                record_bytes: 4 << 20,
            },
            cancelled: not_cancelled,
        },
        &guard,
        &live,
    )
    .unwrap();

    let mut current_io = io();
    let mut current_keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut current = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &fixture.pages,
            memory: &fixture.memory,
            class: MemoryClass::Ordinary,
            io: &mut current_io,
        },
        key_work: &mut current_keys,
        semantic_bytes: 2 << 20,
        record_bytes: 4 << 20,
        take_state_error: no_error,
        cancelled: not_cancelled,
    };
    let guard = current.transaction.guard();
    assert_eq!(
        read_current_material_default_priority_overlay_v1(
            "ore",
            &fixture.family,
            &fixture.pages,
            &mut current,
            &mut work(),
            &guard,
            &live,
        )
        .unwrap(),
        Some(MaterialDefaultPriority::Urgent)
    );

    // An absent outer value is the only successful None. A present pointer to
    // an unavailable value tree remains an integrity error.
    let mut absent_tx = transaction(&fixture);
    let absent_live = absent_tx.parent_fence();
    let mut outer_values = ValueReadContext {
        source: &fixture.pages,
        memory: &fixture.memory,
        class: MemoryClass::Ordinary,
        io: &mut io(),
    };
    let mut outer_keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut cancelled = not_cancelled;
    let outer = read::outer_key(
        MaterialRecordKeyV1::Profile("ore"),
        &mut InventoryValueReader {
            values: &mut outer_values,
            key_work: &mut outer_keys,
            semantic_bytes: 2 << 20,
            record_bytes: 4 << 20,
            cancelled: &mut cancelled,
        },
    )
    .unwrap()
    .0;
    absent_tx
        .write(
            MaterialRecordKindV1::Profile.namespace(),
            outer.as_bytes(),
            None,
        )
        .unwrap();
    let mut absent_io = io();
    let mut absent_keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut absent_context = WorldFieldOverlayContextV1 {
        transaction: &mut absent_tx,
        values: ValueReadContext {
            source: &fixture.pages,
            memory: &fixture.memory,
            class: MemoryClass::Ordinary,
            io: &mut absent_io,
        },
        key_work: &mut absent_keys,
        semantic_bytes: 2 << 20,
        record_bytes: 4 << 20,
        take_state_error: no_error,
        cancelled: not_cancelled,
    };
    let absent_guard = absent_context.transaction.guard();
    assert_eq!(
        read_current_material_default_priority_overlay_v1(
            "ore",
            &fixture.family,
            &fixture.pages,
            &mut absent_context,
            &mut work(),
            &absent_guard,
            &absent_live,
        )
        .unwrap(),
        None
    );

    let mut corrupt_tx = transaction(&fixture);
    let corrupt_live = corrupt_tx.parent_fence();
    let mut corrupt_values = ValueReadContext {
        source: &fixture.pages,
        memory: &fixture.memory,
        class: MemoryClass::Ordinary,
        io: &mut io(),
    };
    let mut corrupt_keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut corrupt_cancelled = not_cancelled;
    let corrupt_outer = read::outer_key(
        MaterialRecordKeyV1::Profile("ore"),
        &mut InventoryValueReader {
            values: &mut corrupt_values,
            key_work: &mut corrupt_keys,
            semantic_bytes: 2 << 20,
            record_bytes: 4 << 20,
            cancelled: &mut corrupt_cancelled,
        },
    )
    .unwrap()
    .0;
    corrupt_tx
        .write(
            MaterialRecordKindV1::Profile.namespace(),
            corrupt_outer.as_bytes(),
            Some([0xff; 32]),
        )
        .unwrap();
    let mut corrupt_io = io();
    let mut corrupt_key_work = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut corrupt_context = WorldFieldOverlayContextV1 {
        transaction: &mut corrupt_tx,
        values: ValueReadContext {
            source: &fixture.pages,
            memory: &fixture.memory,
            class: MemoryClass::Ordinary,
            io: &mut corrupt_io,
        },
        key_work: &mut corrupt_key_work,
        semantic_bytes: 2 << 20,
        record_bytes: 4 << 20,
        take_state_error: no_error,
        cancelled: not_cancelled,
    };
    let corrupt_guard = corrupt_context.transaction.guard();
    assert!(
        read_current_material_default_priority_overlay_v1(
            "ore",
            &fixture.family,
            &fixture.pages,
            &mut corrupt_context,
            &mut work(),
            &corrupt_guard,
            &corrupt_live,
        )
        .is_err()
    );
}
