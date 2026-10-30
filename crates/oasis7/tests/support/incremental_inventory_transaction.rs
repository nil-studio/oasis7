use super::*;
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionError, ExecutionParent, ExecutionTransaction, RootSetV1,
        TransactionLimits, TransitionVerifier,
    },
    incremental_state::StateNamespaceV1,
    incremental_value::ValueNodeWriter,
};
use oasis7_runtime::incremental_field_member::{WorldFieldKeyV1, WorldFieldRecordKindV1};
use oasis7_runtime::incremental_inventory_transaction::*;

fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 32,
        max_writes: 8,
        max_facts: 8,
        max_pending_node_bytes: 4 << 20,
    }
}
fn parent(root: &StateRootV1) -> ExecutionParent {
    let mut roots = RootSetV1::empty("inventory-transaction".into(), [3; 32]).unwrap();
    roots.state[0] = root.clone();
    ExecutionParent {
        commit: [4; 32],
        roots,
        authority: AuthorityBinding {
            token: [5; 32],
            epoch: 7,
        },
    }
}

#[test]
fn complete_point_dependencies_seal_to_an_independently_verified_cold_root() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let _source_charge = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
        let (old, next) = {
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            let (_, bytes) = build(&source(), &store, &memory);
            let root = StateRootV1::from_canonical_bytes(&bytes).unwrap();
            let ledger = MaterialLedgerId::Site("new".into());
            let no = Cell::new(false);
            let mut writes = Writes::new(&memory);
            let (prepared, _) = prepare(
                InventoryChangeRefV1::LedgerMaterial {
                    ledger: &ledger,
                    material: "ore",
                    expected: LedgerQuantityV1::LedgerAbsent,
                    after: Some(7),
                    create_ledger: true,
                },
                &root,
                &store,
                &memory,
                &mut writes,
                &no,
                &no,
            );
            let prepared = prepared.unwrap();
            let parent = parent(&root);
            let live = parent.fence().unwrap();
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let mut transaction =
                ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits())
                    .unwrap();
            let old_guard = transaction.guard();
            contribute_inventory_change_v1(
                &prepared,
                &writes.rows.borrow(),
                &mut transaction,
                &old_guard,
                &live,
                &memory,
                || false,
            )
            .unwrap();
            let retained = memory.used();
            drop(prepared);
            assert!(
                retained - memory.used() >= std::mem::size_of::<PreparedInventoryChangeV1>(),
                "retained inventory dependencies must retain their admission charge"
            );
            assert!(matches!(
                transaction.validate_guard(&old_guard, &live),
                Err(ExecutionError::StaleGuard)
            ));
            let guard = transaction.guard();
            let sealed = transaction.seal(&guard, &live, 1).unwrap();
            assert_eq!(sealed.record.reads.len(), 3);
            assert_eq!(sealed.record.writes.len(), 2);
            let mut kinds: Vec<_> = sealed
                .record
                .reads
                .iter()
                .map(|read| {
                    assert_eq!(read.namespace, StateNamespaceV1::Business);
                    WorldFieldKeyV1::from_bytes(&read.key).unwrap().kind as u8
                })
                .collect();
            kinds.sort();
            assert_eq!(
                kinds,
                vec![
                    WorldFieldRecordKindV1::Metadata as u8,
                    WorldFieldRecordKindV1::Group as u8,
                    WorldFieldRecordKindV1::Entry as u8
                ]
            );
            let verified = TransitionVerifier {
                parent: &parent,
                operation: [6; 32],
                source: &state,
                memory: &memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 4 << 20,
            }
            .verify(&sealed.record)
            .unwrap();
            assert_eq!(verified.record.after, sealed.record.after);
            let mut pages = store.value_session(physical());
            for node in &verified.new_nodes {
                let charge = memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap();
                pages
                    .put_node(
                        *blake3::hash(&node.bytes).as_bytes(),
                        charge.retain(node.bytes.clone()),
                    )
                    .unwrap();
            }
            (
                bytes,
                verified.record.after.state[0].canonical_bytes().unwrap(),
            )
        };
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        assert_eq!(
            quantity(
                &StateRootV1::from_canonical_bytes(&old).unwrap(),
                &store,
                &memory,
                3
            ),
            LedgerQuantityV1::LedgerAbsent
        );
        assert_eq!(
            quantity(
                &StateRootV1::from_canonical_bytes(&next).unwrap(),
                &store,
                &memory,
                3
            ),
            LedgerQuantityV1::Value(7)
        );
    }
}

#[test]
fn conflicts_in_metadata_group_or_member_poison_the_shared_transaction() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let _source_charge = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let (_, bytes) = build(&source(), &store, &memory);
    let root = StateRootV1::from_canonical_bytes(&bytes).unwrap();
    let ledger = MaterialLedgerId::Agent("a".into());
    let no = Cell::new(false);
    let mut writes = Writes::new(&memory);
    let (prepared, _) = prepare(
        InventoryChangeRefV1::LedgerMaterial {
            ledger: &ledger,
            material: "x:y",
            expected: LedgerQuantityV1::Value(-3),
            after: Some(4),
            create_ledger: false,
        },
        &root,
        &store,
        &memory,
        &mut writes,
        &no,
        &no,
    );
    let prepared = prepared.unwrap();
    let parent = parent(&root);
    let live = parent.fence().unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let mut successful =
        ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits()).unwrap();
    let guard = successful.guard();
    contribute_inventory_change_v1(
        &prepared,
        &writes.rows.borrow(),
        &mut successful,
        &guard,
        &live,
        &memory,
        || false,
    )
    .unwrap();
    let guard = successful.guard();
    let sealed = successful.seal(&guard, &live, 0).unwrap();
    assert_eq!(sealed.record.reads.len(), 3);
    for dependency in &sealed.record.reads {
        let mut transaction =
            ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits())
                .unwrap();
        transaction
            .write(StateNamespaceV1::Business, &dependency.key, Some([44; 32]))
            .unwrap();
        let guard = transaction.guard();
        assert!(matches!(
            contribute_inventory_change_v1(
                &prepared,
                &writes.rows.borrow(),
                &mut transaction,
                &guard,
                &live,
                &memory,
                || false
            ),
            Err(InventoryContributionError::DependencyConflict)
        ));
        let guard = transaction.guard();
        assert!(matches!(
            transaction.seal(&guard, &live, 0),
            Err(ExecutionError::Aborted)
        ));
    }
    // Even a no-op contributes the authenticated read dependencies.
    let mut none = Writes::new(&memory);
    let (noop, _) = prepare(
        InventoryChangeRefV1::Resource {
            key: ResourceKind::Data,
            expected: Some(9),
            after: Some(9),
        },
        &root,
        &store,
        &memory,
        &mut none,
        &no,
        &no,
    );
    let mut transaction =
        ExecutionTransaction::begin(parent, [6; 32], &state, &memory, limits()).unwrap();
    let guard = transaction.guard();
    let noop = noop.unwrap();
    contribute_inventory_change_v1(&noop, &[], &mut transaction, &guard, &live, &memory, || {
        false
    })
    .unwrap();
    let guard = transaction.guard();
    contribute_inventory_change_v1(&noop, &[], &mut transaction, &guard, &live, &memory, || {
        false
    })
    .unwrap();
    // Re-reading already registered dependencies for a no-op changes no revision.
    transaction.validate_guard(&guard, &live).unwrap();
    let sealed = transaction.seal(&guard, &live, 0).unwrap();
    assert_eq!(sealed.record.reads.len(), 2);
    assert!(sealed.record.writes.is_empty());
}

#[test]
fn partial_import_and_bad_guards_limits_or_batches_never_remain_sealable() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let _source_charge = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let (_, bytes) = build(&source(), &store, &memory);
    let root = StateRootV1::from_canonical_bytes(&bytes).unwrap();
    let ledger = MaterialLedgerId::Site("new".into());
    let no = Cell::new(false);
    let mut writes = Writes::new(&memory);
    let (prepared, _) = prepare(
        InventoryChangeRefV1::LedgerMaterial {
            ledger: &ledger,
            material: "ore",
            expected: LedgerQuantityV1::LedgerAbsent,
            after: Some(7),
            create_ledger: true,
        },
        &root,
        &store,
        &memory,
        &mut writes,
        &no,
        &no,
    );
    let prepared = prepared.unwrap();
    let parent = parent(&root);
    let live = parent.fence().unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let foreign_memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    for case in 0..16 {
        let mut limit = limits();
        if case == 0 {
            limit.max_writes = 1;
        }
        if case == 1 {
            limit.max_reads = 1;
        }
        let mut transaction_parent = parent.clone();
        if case == 4 {
            transaction_parent.roots.state[0] =
                StateRootV1::empty(StateNamespaceV1::Business).unwrap();
        }
        let mut transaction = ExecutionTransaction::begin(
            transaction_parent.clone(),
            [6; 32],
            &state,
            &memory,
            limit,
        )
        .unwrap();
        let guard = transaction.guard();
        if case == 2 {
            transaction
                .read(StateNamespaceV1::Allocator, b"prior-read")
                .unwrap();
        }
        let mut fence = transaction_parent.fence().unwrap();
        if case == 3 {
            fence.authority.epoch += 1;
        }
        let source = &prepared;
        let rows = writes.rows.borrow();
        let _batch_charge = memory.reserve(4096, MemoryClass::Ordinary).unwrap();
        let mut altered = Vec::new();
        if case >= 9 {
            for index in 0..2 {
                let row = &rows[if case == 9 { 0 } else { index }];
                let mut key = row.key.clone();
                let mut before = row.before;
                if case == 15 {
                    key.reserve(1024);
                }
                if index == 0 {
                    if case == 10 {
                        key = b"unrelated".to_vec();
                    }
                    if case == 11 {
                        key = fixture::key(
                            "material_ledgers",
                            FieldShape::NestedMap,
                            WorldFieldRecordKindV1::Metadata,
                            None,
                        )
                        .0
                        .as_bytes()
                        .to_vec();
                    }
                    if case == 12 {
                        before = Some([77; 32]);
                    }
                }
                let charge = memory
                    .reserve(
                        if case == 14 { 0 } else { 1024 + key.len() },
                        MemoryClass::Ordinary,
                    )
                    .unwrap();
                altered.push(charge.retain(StateWriteV1 {
                    key,
                    before,
                    after: if case == 13 {
                        Some([88; 32])
                    } else {
                        row.after
                    },
                }));
            }
        }
        let batch = if case == 5 {
            &rows[..1]
        } else if case >= 9 {
            &altered[..]
        } else {
            &rows[..]
        };
        let mut calls = 0;
        let result = contribute_inventory_change_v1(
            source,
            batch,
            &mut transaction,
            &guard,
            &fence,
            if case == 6 { &foreign_memory } else { &memory },
            || {
                calls += 1;
                case == 7 || (case == 8 && calls == 6)
            },
        );
        match case {
            0 | 1 => assert!(matches!(
                result,
                Err(InventoryContributionError::Execution(
                    ExecutionError::ResourceLimited(_)
                ))
            )),
            2 => assert!(matches!(
                result,
                Err(InventoryContributionError::Execution(
                    ExecutionError::StaleGuard
                ))
            )),
            3 => assert!(matches!(
                result,
                Err(InventoryContributionError::Execution(
                    ExecutionError::ParentConflict
                ))
            )),
            4 => assert!(matches!(
                result,
                Err(InventoryContributionError::DependencyConflict)
            )),
            5 | 6 | 9..=15 => assert!(
                matches!(result, Err(InventoryContributionError::Invalid(_))),
                "case={case}: {result:?}"
            ),
            _ => assert!(matches!(result, Err(InventoryContributionError::Cancelled))),
        }
        if case == 8 {
            assert_eq!(calls, 6);
        }
        let guard = transaction.guard();
        assert!(matches!(
            transaction.seal(&guard, &live, 0),
            Err(ExecutionError::Aborted)
        ));
    }
}

#[test]
fn retained_noop_preparations_are_admitted_and_release_their_budget() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let _source = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let (_, bytes) = build(&source(), &store, &memory);
    let root = StateRootV1::from_canonical_bytes(&bytes).unwrap();
    let _slots = memory
        .reserve(
            128 * std::mem::size_of::<ReservedValue<PreparedInventoryChangeV1>>(),
            MemoryClass::Ordinary,
        )
        .unwrap();
    let mut retained = Vec::with_capacity(128);
    // Existing read path admits a 2 MiB decode workspace, 512 KiB inventory
    // workspace and bounded frame/session metadata before any retained output.
    let read_headroom = (2 << 20) + (512 << 10) + (128 << 10);
    let _pressure = memory
        .reserve(
            (31 << 20) - memory.used() - read_headroom,
            MemoryClass::Ordinary,
        )
        .unwrap();
    let initial = memory.used();
    let no = Cell::new(false);
    let mut limited = false;
    for _ in 0..128 {
        let mut writes = Writes::new(&memory);
        let (prepared, _) = prepare(
            InventoryChangeRefV1::Material {
                key: "absent-noop-budget",
                expected: None,
                after: None,
            },
            &root,
            &store,
            &memory,
            &mut writes,
            &no,
            &no,
        );
        match prepared {
            Ok(prepared) => {
                assert_eq!(prepared.records(), 0);
                assert!(writes.rows.borrow().is_empty());
                assert!(prepared.uses_budget(&memory));
                retained.push(prepared);
            }
            Err(InventoryChangeError::Value(ValueTreeError::Memory(_)))
            | Err(InventoryChangeError::Record(MemberRecordError::Value(
                ValueTreeError::Memory(_),
            ))) => {
                limited = true;
                break;
            }
            Err(error) => panic!("unexpected preparation failure: {error:?}"),
        }
    }
    assert!(!retained.is_empty());
    assert!(
        limited,
        "retained no-op outputs must exhaust finite admission"
    );
    let charged: usize = retained.iter().map(|value| value.reserved_bytes()).sum();
    assert_eq!(memory.used() - initial, charged);
    drop(retained);
    assert_eq!(memory.used(), initial);
}
#[path = "incremental_inventory_overlay_read.rs"]
mod overlay_read;
