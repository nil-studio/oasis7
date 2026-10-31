use super::*;
use oasis7_runtime::incremental_inventory_overlay_read::{
    InventoryOverlayReadContext, InventoryOverlayReadError,
};

#[test]
fn typed_reads_see_prior_contributions_without_replacing_the_parent() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let _fixture = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
        let store =
            IncrementalPageStore::create(&directory.0.join("pages"), memory.clone(), compression)
                .unwrap();
        let (_, bytes) = build(&source(), &store, &memory);
        let root = StateRootV1::from_canonical_bytes(&bytes).unwrap();
        let parent = parent(&root);
        let live = parent.fence().unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let mut transaction =
            ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits())
                .unwrap();
        let no = Cell::new(false);
        let agent = MaterialLedgerId::Agent("a".into());
        let site = MaterialLedgerId::Site("new".into());
        for change in [
            InventoryChangeRefV1::Resource {
                key: ResourceKind::Data,
                expected: Some(9),
                after: Some(10),
            },
            InventoryChangeRefV1::Material {
                key: "",
                expected: Some(0),
                after: None,
            },
            InventoryChangeRefV1::LedgerMaterial {
                ledger: &agent,
                material: "x:y",
                expected: LedgerQuantityV1::Value(-3),
                after: None,
                create_ledger: false,
            },
            InventoryChangeRefV1::LedgerMaterial {
                ledger: &site,
                material: "ore",
                expected: LedgerQuantityV1::LedgerAbsent,
                after: Some(7),
                create_ledger: true,
            },
        ] {
            let mut writes = Writes::new(&memory);
            let (prepared, _) = prepare(change, &root, &store, &memory, &mut writes, &no, &no);
            let guard = transaction.guard();
            contribute_inventory_change_v1(
                &prepared.unwrap(),
                &writes.rows.borrow(),
                &mut transaction,
                &guard,
                &live,
                &memory,
                || false,
            )
            .unwrap();
        }
        let values = store.value_session(physical());
        let mut io = logical();
        let mut work = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        {
            let mut read = InventoryOverlayReadContext {
                transaction: &mut transaction,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io,
                },
                key_work: &mut work,
                semantic_bytes: 1024,
                record_bytes: 4096,
                take_state_error: || state.take_read_error().map(typed),
                cancelled: || false,
            };
            assert_eq!(read.resource(ResourceKind::Data).unwrap(), Some(10));
            assert_eq!(read.material("").unwrap(), None);
            assert_eq!(read.material("x:y").unwrap(), Some(i64::MAX));
            assert_eq!(
                read.ledger(&agent, "x:y").unwrap(),
                LedgerQuantityV1::MaterialAbsent
            );
            assert_eq!(
                read.ledger(&site, "ore").unwrap(),
                LedgerQuantityV1::Value(7)
            );
            assert_eq!(
                read.ledger(&MaterialLedgerId::Factory("a".into()), "")
                    .unwrap(),
                LedgerQuantityV1::Value(42)
            );
            assert_eq!(
                read.ledger(&MaterialLedgerId::World, "none").unwrap(),
                LedgerQuantityV1::MaterialAbsent
            );
            assert_eq!(
                read.ledger(&MaterialLedgerId::Site("missing".into()), "ore")
                    .unwrap(),
                LedgerQuantityV1::LedgerAbsent
            );
        }
        assert_eq!(
            quantity(&root, &store, &memory, 0),
            LedgerQuantityV1::Value(9)
        );
        assert_eq!(
            quantity(&root, &store, &memory, 2),
            LedgerQuantityV1::Value(-3)
        );
        let guard = transaction.guard();
        let sealed = transaction.seal(&guard, &live, 1).unwrap();
        assert_eq!(sealed.record.reads.len(), 16);
        assert_eq!(sealed.record.writes.len(), 5);
        // Independent verification is a separate bounded operation, with the
        // same allowance, rather than the transaction/seal reader's spent quota.
        let verification_state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let verified = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &verification_state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        }
        .verify(&sealed.record)
        .unwrap();
        assert_eq!(verified.record.after, sealed.record.after);
    }
}

#[test]
fn malformed_overlay_metadata_orphan_or_value_never_reports_successful_absence() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
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
    let agent_token = ledger_token(1, Some("a"));
    let member_token = pair_token(&agent_token, &text_token("x:y"));
    for case in 0..4 {
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let mut transaction =
            ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits())
                .unwrap();
        let key = match case {
            0 => {
                fixture::key(
                    "material_ledgers",
                    FieldShape::NestedMap,
                    WorldFieldRecordKindV1::Metadata,
                    None,
                )
                .0
            }
            1 => {
                fixture::key(
                    "material_ledgers",
                    FieldShape::NestedMap,
                    WorldFieldRecordKindV1::Group,
                    Some(&agent_token),
                )
                .0
            }
            _ => {
                fixture::key(
                    "material_ledgers",
                    FieldShape::NestedMap,
                    WorldFieldRecordKindV1::Entry,
                    Some(&member_token),
                )
                .0
            }
        };
        let after = if case < 2 {
            None
        } else if case == 2 {
            Some([77; 32])
        } else {
            let resource = fixture::key(
                "resources",
                FieldShape::Map,
                WorldFieldRecordKindV1::Entry,
                Some(&[3, 2]),
            )
            .0;
            transaction
                .read(StateNamespaceV1::Business, resource.as_bytes())
                .unwrap()
        };
        transaction
            .write(StateNamespaceV1::Business, key.as_bytes(), after)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut work = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut read = InventoryOverlayReadContext {
            transaction: &mut transaction,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut work,
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || state.take_read_error().map(typed),
            cancelled: || false,
        };
        let result = read.ledger(&MaterialLedgerId::Agent("a".into()), "x:y");
        if case == 2 {
            assert!(
                matches!(result, Err(InventoryOverlayReadError::Value(ValueTreeError::Io(ref error)))
                if error.kind() == std::io::ErrorKind::NotFound),
                "case={case}: {result:?}"
            );
        } else {
            assert!(
                matches!(
                    result,
                    Err(InventoryOverlayReadError::Value(ValueTreeError::Invalid(_)))
                ),
                "case={case}: {result:?}"
            );
        }
        let guard = transaction.guard();
        assert!(matches!(
            transaction.seal(&guard, &live, 1),
            Err(ExecutionError::Aborted)
        ));
    }
}

#[test]
fn limits_cancel_pool_and_original_state_errors_poison_the_entire_transaction() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
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
    let foreign = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    for case in 0..11 {
        let mut state_io = physical();
        if case == 9 {
            state_io.reads_remaining = 0;
        }
        let state = store
            .state_session(state_io, MemoryClass::Ordinary)
            .unwrap();
        let mut bound = limits();
        if case == 0 {
            bound.max_reads = 0;
        }
        let mut transaction =
            ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, bound).unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        if case == 1 {
            io.nodes_remaining = 0;
        }
        let mut work = SemanticKeyBudget {
            bytes_remaining: if case == 2 { 0 } else { 1 << 20 },
        };
        let cancelled = Cell::new(case == 6);
        let _fill = if case == 10 {
            Some(
                memory
                    .reserve(
                        (31 << 20) - memory.used() - (256 << 10),
                        MemoryClass::Ordinary,
                    )
                    .unwrap(),
            )
        } else {
            None
        };
        let mut read = InventoryOverlayReadContext {
            transaction: &mut transaction,
            values: ValueReadContext {
                source: &values,
                memory: if case == 5 { &foreign } else { &memory },
                class: if case == 4 {
                    MemoryClass::Recovery
                } else {
                    MemoryClass::Ordinary
                },
                io: &mut io,
            },
            key_work: &mut work,
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || {
                if case == 7 {
                    cancelled.set(true);
                }
                if case == 3 {
                    return Some(ValueTreeError::ResourceLimited("outstanding state error"));
                }
                if case == 8 {
                    return Some(ValueTreeError::Io(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "state source",
                    )));
                }
                state.take_read_error().map(typed)
            },
            cancelled: || cancelled.get(),
        };
        let result = read.resource(ResourceKind::Data);
        match case {
            0 => assert!(matches!(
                result,
                Err(InventoryOverlayReadError::Execution(
                    ExecutionError::ResourceLimited(_)
                ))
            )),
            1 | 2 | 3 | 9 => assert!(
                matches!(
                    result,
                    Err(InventoryOverlayReadError::Value(
                        ValueTreeError::ResourceLimited(_)
                    ))
                ),
                "case={case}: {result:?}"
            ),
            4 | 5 => assert!(matches!(
                result,
                Err(InventoryOverlayReadError::Value(ValueTreeError::Invalid(_)))
            )),
            6 | 7 => assert!(matches!(
                result,
                Err(InventoryOverlayReadError::Value(ValueTreeError::Cancelled))
            )),
            8 => assert!(matches!(
                result,
                Err(InventoryOverlayReadError::Value(ValueTreeError::Io(_)))
            )),
            _ => assert!(matches!(
                result,
                Err(InventoryOverlayReadError::Value(ValueTreeError::Memory(_)))
            )),
        }
        if case == 6 || case == 7 {
            assert_eq!(io.nodes_remaining, 100_000);
        }
        let guard = transaction.guard();
        assert!(matches!(
            transaction.seal(&guard, &live, 1),
            Err(ExecutionError::Aborted)
        ));
    }
}
