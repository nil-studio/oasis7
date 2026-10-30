use super::*;

#[test]
fn failed_second_changes_cannot_publish_the_first_change_or_a_group_prefix() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let foreign = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let _fixture = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let (_, bytes) = build(&source(), &store, &memory);
    let root = StateRootV1::from_canonical_bytes(&bytes).unwrap();
    let parent = parent(&root);
    let live = parent.fence().unwrap();
    let ledger = MaterialLedgerId::Site("new".into());
    for case in 0..14 {
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let mut bound = limits();
        if case == 10 {
            bound.max_writes = 2;
        }
        let mut transaction =
            ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, bound).unwrap();
        let values = store.value_session(physical());
        let mut output = store.value_session(physical());
        let mut input_io = logical();
        let mut output_io = logical();
        let mut input_work = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut output_work = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let read_cancel = Cell::new(false);
        let write_cancel = Cell::new(false);
        let armed = Cell::new(false);
        let mut read = InventoryOverlayReadContext {
            transaction: &mut transaction,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut input_io,
            },
            key_work: &mut input_work,
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || {
                if armed.get() && case == 13 {
                    write_cancel.set(true);
                }
                state.take_read_error().map(typed)
            },
            cancelled: || read_cancel.get(),
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut output,
            memory: &memory,
            io: &mut output_io,
            key_work: &mut output_work,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 8,
                record_bytes: 4096,
            },
            cancelled: || write_cancel.get(),
        };
        let initial_guard = read.transaction.guard();
        assert_eq!(
            read.change(
                InventoryChangeRefV1::Resource {
                    key: ResourceKind::Data,
                    expected: Some(9),
                    after: Some(10)
                },
                &mut write,
                &initial_guard,
                &live
            )
            .unwrap(),
            1
        );
        assert_eq!(read.resource(ResourceKind::Data).unwrap(), Some(10));
        let output_nodes = write.io.nodes_remaining;
        let input_nodes = read.values.io.nodes_remaining;
        match case {
            3 => write.io.nodes_remaining = 0,
            4 => write.key_work.bytes_remaining = 0,
            5 => read.values.io.nodes_remaining = 0,
            6 => write.memory = &foreign,
            7 => read.values.memory = &foreign,
            8 => read_cancel.set(true),
            9 => write_cancel.set(true),
            12 => {
                let metadata = fixture::key(
                    "resources",
                    FieldShape::Map,
                    WorldFieldRecordKindV1::Metadata,
                    None,
                )
                .0;
                read.transaction
                    .write(StateNamespaceV1::Business, metadata.as_bytes(), None)
                    .unwrap();
            }
            _ => {}
        }
        armed.set(true);
        let current_guard = read.transaction.guard();
        let guard = if case == 1 {
            &initial_guard
        } else {
            &current_guard
        };
        let mut fence = live;
        if case == 2 {
            fence.authority.epoch += 1;
        }
        let change = if case == 10 || case == 11 {
            InventoryChangeRefV1::LedgerMaterial {
                ledger: &ledger,
                material: "ore",
                expected: LedgerQuantityV1::LedgerAbsent,
                after: Some(1),
                create_ledger: case == 10,
            }
        } else {
            InventoryChangeRefV1::Resource {
                key: ResourceKind::Data,
                expected: Some(if case == 0 { 9 } else { 10 }),
                after: Some(13),
            }
        };
        let result = read.change(change, &mut write, guard, &fence);
        match case {
            0 => assert!(
                matches!(
                    result,
                    Err(InventoryOverlayChangeError::Change(
                        InventoryChangeError::Precondition
                    ))
                ),
                "case={case}: {result:?}"
            ),
            1 => assert!(
                matches!(
                    result,
                    Err(InventoryOverlayChangeError::Execution(
                        ExecutionError::StaleGuard
                    ))
                ),
                "case={case}: {result:?}"
            ),
            2 => assert!(
                matches!(
                    result,
                    Err(InventoryOverlayChangeError::Execution(
                        ExecutionError::ParentConflict
                    ))
                ),
                "case={case}: {result:?}"
            ),
            3 => assert!(
                matches!(
                    result,
                    Err(InventoryOverlayChangeError::Change(
                        InventoryChangeError::Record(MemberRecordError::Value(
                            ValueTreeError::ResourceLimited(_)
                        ))
                    ))
                ),
                "case={case}: {result:?}"
            ),
            4 => assert!(
                matches!(
                    result,
                    Err(InventoryOverlayChangeError::Change(
                        InventoryChangeError::Record(MemberRecordError::Key(_))
                    ))
                ),
                "case={case}: {result:?}"
            ),
            5 => assert!(
                matches!(
                    result,
                    Err(InventoryOverlayChangeError::Read(
                        InventoryOverlayReadError::Value(ValueTreeError::ResourceLimited(_))
                    ))
                ),
                "case={case}: {result:?}"
            ),
            6 | 7 | 11 => assert!(
                matches!(
                    result,
                    Err(InventoryOverlayChangeError::Change(
                        InventoryChangeError::Invalid(_)
                    ))
                ),
                "case={case}: {result:?}"
            ),
            8 | 9 => assert!(
                matches!(
                    result,
                    Err(InventoryOverlayChangeError::Change(
                        InventoryChangeError::Value(ValueTreeError::Cancelled)
                    ))
                ),
                "case={case}: {result:?}"
            ),
            10 => assert!(
                matches!(
                    result,
                    Err(InventoryOverlayChangeError::Contribution(
                        InventoryContributionError::Execution(ExecutionError::ResourceLimited(_))
                    ))
                ),
                "case={case}: {result:?}"
            ),
            12 => assert!(
                matches!(
                    result,
                    Err(InventoryOverlayChangeError::Read(
                        InventoryOverlayReadError::Value(ValueTreeError::Invalid(_))
                    ))
                ),
                "case={case}: {result:?}"
            ),
            _ => assert!(
                matches!(
                    result,
                    Err(InventoryOverlayChangeError::Read(
                        InventoryOverlayReadError::Value(ValueTreeError::Cancelled)
                    ))
                ),
                "case={case}: {result:?}"
            ),
        }
        if case == 0 || case == 1 || case == 2 || case == 11 || case == 13 {
            assert_eq!(write.io.nodes_remaining, output_nodes);
        }
        if case == 13 {
            assert_eq!(read.values.io.nodes_remaining, input_nodes);
        }
        let guard = transaction.guard();
        assert!(matches!(
            transaction.seal(&guard, &live, 1),
            Err(ExecutionError::Aborted)
        ));
    }
    assert_eq!(
        quantity(&root, &store, &memory, 0),
        LedgerQuantityV1::Value(9)
    );
    assert_eq!(
        quantity(&root, &store, &memory, 3),
        LedgerQuantityV1::LedgerAbsent
    );
}
