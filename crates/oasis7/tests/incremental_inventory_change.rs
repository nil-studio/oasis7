#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_state::{
        STATE_ROOT_BYTES, StateDeltaV1, StatePageWriter, StateRootV1, StateWriteV1,
    },
    incremental_value::{ValueIoBudget, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::{
    incremental_field_member::*, incremental_inventory_change::*, incremental_inventory_read::*,
    types::MaterialLedgerId,
};
use std::{cell::Cell, rc::Rc};
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::*;
fn typed(e: PageStoreError) -> ValueTreeError {
    match e {
        PageStoreError::Memory(e) => ValueTreeError::Memory(e),
        PageStoreError::ResourceLimited(r) => ValueTreeError::ResourceLimited(r),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        PageStoreError::Io(e) => ValueTreeError::Io(e),
        PageStoreError::Corrupt(r) => ValueTreeError::Invalid(r),
        PageStoreError::Missing(_) => ValueTreeError::Invalid("missing state page"),
        PageStoreError::UnsupportedPlatform => {
            ValueTreeError::Invalid("unsupported state platform")
        }
    }
}
fn prepare(
    change: InventoryChangeRefV1<'_>,
    root: &StateRootV1,
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    writes: &mut Writes,
    read_cancel: &Cell<bool>,
    write_cancel: &Cell<bool>,
) -> (
    Result<PreparedInventoryChangeV1, InventoryChangeError>,
    ValueIoBudget,
) {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut input_io = logical();
    let mut input_work = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut read = InventoryReadContext {
        root,
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut input_io,
        },
        key_work: &mut input_work,
        semantic_bytes: 1024,
        record_bytes: 4096,
        take_state_error: || state.take_read_error().map(typed),
        cancelled: || read_cancel.get(),
    };
    let mut output = store.value_session(physical());
    let mut output_io = logical();
    let mut output_work = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut write = MemberRecordWriteContext {
        sink: &mut output,
        memory,
        io: &mut output_io,
        key_work: &mut output_work,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 1024,
            payload_bytes: 8,
            record_bytes: 4096,
        },
        cancelled: || write_cancel.get(),
    };
    let result = prepare_inventory_change_v1(change, &mut read, &mut write, writes);
    (result, output_io)
}
fn apply(
    prepared: &PreparedInventoryChangeV1,
    root: &StateRootV1,
    writes: &Writes,
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
) -> [u8; STATE_ROOT_BYTES] {
    assert_eq!(
        prepared.before_root.as_slice(),
        root.canonical_bytes().unwrap()
    );
    let charge = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let rows = writes.rows.borrow();
    assert_eq!(rows.len(), prepared.records as usize);
    let mut changes: Vec<_> = rows
        .iter()
        .map(|row| StateWriteV1 {
            key: row.key.clone(),
            before: row.before,
            after: row.after,
        })
        .collect();
    changes.sort_by(|a, b| a.key.cmp(&b.key));
    let source = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let delta =
        charge.retain(StateDeltaV1::build(root.clone(), changes, &source, 2 << 20).unwrap());
    store
        .value_session(physical())
        .put_state_pages(&delta, memory)
        .unwrap();
    delta.after.canonical_bytes().unwrap().try_into().unwrap()
}
fn quantity(
    root: &StateRootV1,
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    case: u8,
) -> LedgerQuantityV1 {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut io = logical();
    let mut work = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut read = InventoryReadContext {
        root,
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut work,
        semantic_bytes: 1024,
        record_bytes: 4096,
        take_state_error: || state.take_read_error().map(typed),
        cancelled: || false,
    };
    match case {
        0 => read
            .resource(ResourceKind::Data)
            .unwrap()
            .map_or(LedgerQuantityV1::MaterialAbsent, LedgerQuantityV1::Value),
        1 => read
            .material("")
            .unwrap()
            .map_or(LedgerQuantityV1::MaterialAbsent, LedgerQuantityV1::Value),
        2 => read
            .ledger(&MaterialLedgerId::Agent("a".into()), "x:y")
            .unwrap(),
        _ => read
            .ledger(&MaterialLedgerId::Site("new".into()), "ore")
            .unwrap(),
    }
}
#[test]
fn point_changes_preserve_old_root_and_touch_only_their_exact_leaves() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let _fixture = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
        let (original, updated) = {
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let (_, bytes) = build(&source(), &store, &memory);
            let original = StateRootV1::from_canonical_bytes(&bytes).unwrap();
            let mut root = original.clone();
            let no = Cell::new(false);
            for case in 0..4 {
                let ledger = MaterialLedgerId::Site("new".into());
                let change = match case {
                    0 => InventoryChangeRefV1::Resource {
                        key: ResourceKind::Data,
                        expected: Some(9),
                        after: Some(10),
                    },
                    1 => InventoryChangeRefV1::Material {
                        key: "",
                        expected: Some(0),
                        after: None,
                    },
                    2 => InventoryChangeRefV1::LedgerMaterial {
                        ledger: &MaterialLedgerId::Agent("a".into()),
                        material: "x:y",
                        expected: LedgerQuantityV1::Value(-3),
                        after: None,
                        create_ledger: false,
                    },
                    _ => InventoryChangeRefV1::LedgerMaterial {
                        ledger: &ledger,
                        material: "ore",
                        expected: LedgerQuantityV1::LedgerAbsent,
                        after: Some(i64::MAX),
                        create_ledger: true,
                    },
                };
                let mut writes = Writes::new(&memory);
                let prepared = prepare(change, &root, &store, &memory, &mut writes, &no, &no)
                    .0
                    .unwrap();
                assert_eq!(prepared.records, if case == 3 { 2 } else { 1 });
                for row in writes.rows.borrow().iter() {
                    assert!(row.uses_budget(&memory));
                }
                if case == 0 {
                    let (expected, _) = key(
                        "resources",
                        FieldShape::Map,
                        WorldFieldRecordKindV1::Entry,
                        Some(&[3, 2]),
                    );
                    assert_eq!(writes.rows.borrow()[0].key, expected.as_bytes());
                }
                if case == 3 {
                    let expected_pair =
                        pair_token(&ledger_token(2, Some("new")), &text_token("ore"));
                    let (expected, _) = key(
                        "material_ledgers",
                        FieldShape::NestedMap,
                        WorldFieldRecordKindV1::Entry,
                        Some(&expected_pair),
                    );
                    assert!(
                        writes
                            .rows
                            .borrow()
                            .iter()
                            .any(|row| row.key == expected.as_bytes())
                    );
                }
                let next = apply(&prepared, &root, &writes, &store, &memory);
                root = StateRootV1::from_canonical_bytes(&next).unwrap();
            }
            assert_eq!(root.leaf_count, original.leaf_count);
            (bytes, root.canonical_bytes().unwrap())
        };
        for bytes in [&original[..], &updated[..]] {
            let store = IncrementalPageStore::open(
                &path,
                memory.clone(),
                compression,
                MemoryClass::Ordinary,
            )
            .unwrap();
            let root = StateRootV1::from_canonical_bytes(bytes).unwrap();
            let old = bytes == original.as_slice();
            assert_eq!(
                quantity(&root, &store, &memory, 0),
                LedgerQuantityV1::Value(if old { 9 } else { 10 })
            );
            assert_eq!(
                quantity(&root, &store, &memory, 1),
                if old {
                    LedgerQuantityV1::Value(0)
                } else {
                    LedgerQuantityV1::MaterialAbsent
                }
            );
            assert_eq!(
                quantity(&root, &store, &memory, 2),
                if old {
                    LedgerQuantityV1::Value(-3)
                } else {
                    LedgerQuantityV1::MaterialAbsent
                }
            );
            assert_eq!(
                quantity(&root, &store, &memory, 3),
                if old {
                    LedgerQuantityV1::LedgerAbsent
                } else {
                    LedgerQuantityV1::Value(i64::MAX)
                }
            );
        }
    }
}
#[test]
fn preconditions_noops_and_explicit_empty_group_creation_are_distinct() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let (_, bytes) = build(&source(), &store, &memory);
    let root = StateRootV1::from_canonical_bytes(&bytes).unwrap();
    let no = Cell::new(false);
    let mut writes = Writes::new(&memory);
    let (result, io) = prepare(
        InventoryChangeRefV1::Material {
            key: "x:y",
            expected: Some(0),
            after: Some(1),
        },
        &root,
        &store,
        &memory,
        &mut writes,
        &no,
        &no,
    );
    assert!(matches!(result, Err(InventoryChangeError::Precondition)));
    assert!(writes.rows.borrow().is_empty());
    assert_eq!(io.nodes_remaining, 100_000);
    let (result, io) = prepare(
        InventoryChangeRefV1::Material {
            key: "x:y",
            expected: Some(i64::MAX),
            after: Some(i64::MAX),
        },
        &root,
        &store,
        &memory,
        &mut writes,
        &no,
        &no,
    );
    assert_eq!(result.unwrap().records, 0);
    assert!(writes.rows.borrow().is_empty());
    assert_eq!(io.nodes_remaining, 100_000);
    let ledger = MaterialLedgerId::Site("new".into());
    let (result, io) = prepare(
        InventoryChangeRefV1::LedgerMaterial {
            ledger: &ledger,
            material: "ore",
            expected: LedgerQuantityV1::LedgerAbsent,
            after: Some(5),
            create_ledger: false,
        },
        &root,
        &store,
        &memory,
        &mut writes,
        &no,
        &no,
    );
    assert!(matches!(
        result,
        Err(InventoryChangeError::Invalid(
            "inventory ledger creation not allowed"
        ))
    ));
    assert_eq!(io.nodes_remaining, 100_000);
    assert!(writes.rows.borrow().is_empty());
    let prepared = prepare(
        InventoryChangeRefV1::LedgerMaterial {
            ledger: &ledger,
            material: "ore",
            expected: LedgerQuantityV1::LedgerAbsent,
            after: None,
            create_ledger: true,
        },
        &root,
        &store,
        &memory,
        &mut writes,
        &no,
        &no,
    )
    .0
    .unwrap();
    assert_eq!(prepared.records, 1);
    let after = apply(&prepared, &root, &writes, &store, &memory);
    let after = StateRootV1::from_canonical_bytes(&after).unwrap();
    assert_eq!(
        quantity(&after, &store, &memory, 3),
        LedgerQuantityV1::MaterialAbsent
    );
}
#[test]
fn cancellation_after_group_output_stops_before_member_encoding_and_no_success_escapes() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let (_, bytes) = build(&source(), &store, &memory);
    let root = StateRootV1::from_canonical_bytes(&bytes).unwrap();
    let ledger = MaterialLedgerId::Site("cancel".into());
    let no = Cell::new(false);
    let mut baseline = Writes::new(&memory);
    let (complete, expected_io) = prepare(
        InventoryChangeRefV1::LedgerMaterial {
            ledger: &ledger,
            material: "ore",
            expected: LedgerQuantityV1::LedgerAbsent,
            after: None,
            create_ledger: true,
        },
        &root,
        &store,
        &memory,
        &mut baseline,
        &no,
        &no,
    );
    assert_eq!(complete.unwrap().records, 1);
    let flag = Rc::new(Cell::new(false));
    let mut writes = Writes::new(&memory);
    writes.cancel = Some(flag.clone());
    let (result, io) = prepare(
        InventoryChangeRefV1::LedgerMaterial {
            ledger: &ledger,
            material: "ore",
            expected: LedgerQuantityV1::LedgerAbsent,
            after: Some(5),
            create_ledger: true,
        },
        &root,
        &store,
        &memory,
        &mut writes,
        &flag,
        &no,
    );
    assert!(matches!(
        result,
        Err(InventoryChangeError::Value(ValueTreeError::Cancelled))
    ));
    assert_eq!(writes.rows.borrow().len(), 1);
    assert_eq!(io.nodes_remaining, expected_io.nodes_remaining);
    let mut failed = Writes::new(&memory);
    failed.fail_after = 1;
    let (result, _) = prepare(
        InventoryChangeRefV1::LedgerMaterial {
            ledger: &ledger,
            material: "ore",
            expected: LedgerQuantityV1::LedgerAbsent,
            after: Some(5),
            create_ledger: true,
        },
        &root,
        &store,
        &memory,
        &mut failed,
        &no,
        &no,
    );
    assert!(result.is_err());
    assert_eq!(failed.rows.borrow().len(), 1);
    assert_eq!(
        quantity(&root, &store, &memory, 3),
        LedgerQuantityV1::LedgerAbsent
    );
}
