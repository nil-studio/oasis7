#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_agent_api::world_action::ResourceKind;
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_state::{
        MerkleNodeReader, STATE_ROOT_BYTES, StateDeltaV1, StateNamespaceV1, StatePageWriter,
        StateRootV1, StateWriteV1,
    },
    incremental_value::{ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::{
    incremental_field_member::*, incremental_inventory_read::*, types::MaterialLedgerId,
};
use std::cell::Cell;
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::*;
fn typed(error: PageStoreError) -> ValueTreeError {
    match error {
        PageStoreError::Memory(e) => ValueTreeError::Memory(e),
        PageStoreError::ResourceLimited(reason) => ValueTreeError::ResourceLimited(reason),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        PageStoreError::Io(e) => ValueTreeError::Io(e),
        PageStoreError::Missing(_) => ValueTreeError::Invalid("missing state page"),
        PageStoreError::Corrupt(reason) => ValueTreeError::Invalid(reason),
        PageStoreError::UnsupportedPlatform => {
            ValueTreeError::Invalid("unsupported state platform")
        }
    }
}
#[test]
fn typed_point_reads_preserve_quantities_presence_and_read_class_after_reopen() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let producer = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let root_bytes = {
            let _source = producer.reserve(1 << 20, MemoryClass::Ordinary).unwrap();
            let store = IncrementalPageStore::create(&path, producer.clone(), compression).unwrap();
            build(&source(), &store, &producer).1
        };
        assert_eq!(producer.used(), 0);
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let memory = MemoryBudget::new(
                4 << 20,
                if class == MemoryClass::Recovery {
                    3 << 20
                } else {
                    1 << 20
                },
            )
            .unwrap();
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            let _filler = if class == MemoryClass::Recovery {
                Some(
                    memory
                        .reserve((1 << 20) - memory.used(), MemoryClass::Ordinary)
                        .unwrap(),
                )
            } else {
                None
            };
            let root = StateRootV1::from_canonical_bytes(&root_bytes).unwrap();
            // Each point operation gets the same original 100,000-read and
            // 256MiB allowance; this is not one unbounded query bundle.
            for case in 0..10 {
                let state = store.state_session(physical(), class).unwrap();
                let values = store.value_session(physical());
                let mut io = logical();
                let mut work = SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                };
                let mut reader = InventoryReadContext {
                    root: &root,
                    state: &state,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class,
                        io: &mut io,
                    },
                    key_work: &mut work,
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || state.take_read_error().map(typed),
                    cancelled: || false,
                };
                match case {
                    0 => assert_eq!(
                        reader.resource(ResourceKind::Electricity).unwrap(),
                        Some(i64::MIN)
                    ),
                    1 => assert_eq!(reader.resource(ResourceKind::Data).unwrap(), Some(9)),
                    2 => assert_eq!(reader.material("").unwrap(), Some(0)),
                    3 => assert_eq!(reader.material("x:y").unwrap(), Some(i64::MAX)),
                    4 => assert_eq!(reader.material("missing").unwrap(), None),
                    5 => assert_eq!(
                        reader.ledger(&MaterialLedgerId::World, "missing").unwrap(),
                        LedgerQuantityV1::MaterialAbsent
                    ),
                    6 => assert_eq!(
                        reader
                            .ledger(&MaterialLedgerId::Agent("a".into()), "x:y")
                            .unwrap(),
                        LedgerQuantityV1::Value(-3)
                    ),
                    7 => assert_eq!(
                        reader
                            .ledger(&MaterialLedgerId::Factory("a".into()), "")
                            .unwrap(),
                        LedgerQuantityV1::Value(42)
                    ),
                    8 => assert_eq!(
                        reader
                            .ledger(&MaterialLedgerId::Site("a".into()), "x:y")
                            .unwrap(),
                        LedgerQuantityV1::LedgerAbsent
                    ),
                    _ => assert_eq!(
                        reader
                            .ledger(&MaterialLedgerId::Agent("a".into()), "missing")
                            .unwrap(),
                        LedgerQuantityV1::MaterialAbsent
                    ),
                }
            }
        }
    }
}
#[test]
fn metadata_or_group_inconsistency_and_wrong_payload_profile_are_invalid() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let _fixture = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let (_, root_bytes) = build(&source(), &store, &memory);
    let root = StateRootV1::from_canonical_bytes(&root_bytes).unwrap();
    let (group, _) = key(
        "material_ledgers",
        FieldShape::NestedMap,
        WorldFieldRecordKindV1::Group,
        Some(&ledger_token(1, Some("a"))),
    );
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let before = {
        let _q = memory.reserve(512 << 10, MemoryClass::Ordinary).unwrap();
        root.get(group.as_bytes(), &state).unwrap().unwrap()
    };
    let orphan = patch(
        &root,
        vec![StateWriteV1 {
            key: group.as_bytes().to_vec(),
            before: Some(before),
            after: None,
        }],
        &store,
        &memory,
    );
    let orphan = StateRootV1::from_canonical_bytes(&orphan).unwrap();
    for current in [
        &orphan,
        &StateRootV1::empty(StateNamespaceV1::Business).unwrap(),
    ] {
        let values = store.value_session(physical());
        let mut io = logical();
        let mut work = SemanticKeyBudget {
            bytes_remaining: 4096,
        };
        let mut reader = InventoryReadContext {
            root: current,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut work,
            semantic_bytes: 1024,
            record_bytes: 1024,
            take_state_error: || state.take_read_error().map(typed),
            cancelled: || false,
        };
        assert!(matches!(
            reader.ledger(&MaterialLedgerId::Agent("a".into()), "x:y"),
            Err(ValueTreeError::Invalid(_))
        ));
    }
    // A valid generic Text record is invalid for this owner's I64 profile,
    // even when its payload exceeds the caller's record limit.
    let field = FieldDescriptor {
        owner: FieldOwner::WorldState,
        name: "materials",
        destination: FieldDestination::State(StateNamespaceV1::Business),
        shape: FieldShape::Map,
    };
    let mut values = store.value_session(physical());
    let text = "x".repeat(4096);
    let bad = write_member_record_v1(
        field,
        WorldFieldRecordKindV1::Entry,
        "x:y",
        MemberPayloadRefV1::Text(&text),
        &mut MemberRecordWriteContext {
            sink: &mut values,
            memory: &memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 4096,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 8192,
                record_bytes: 8192,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    let before = {
        let _q = memory.reserve(512 << 10, MemoryClass::Ordinary).unwrap();
        root.get(bad.field_key.as_bytes(), &state).unwrap().unwrap()
    };
    let wrong = patch(
        &root,
        vec![StateWriteV1 {
            key: bad.field_key.as_bytes().to_vec(),
            before: Some(before),
            after: Some(bad.value_root),
        }],
        &store,
        &memory,
    );
    let wrong = StateRootV1::from_canonical_bytes(&wrong).unwrap();
    let mut io = logical();
    let mut work = SemanticKeyBudget {
        bytes_remaining: 4096,
    };
    let mut reader = InventoryReadContext {
        root: &wrong,
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut work,
        semantic_bytes: 1024,
        record_bytes: 1024,
        take_state_error: || state.take_read_error().map(typed),
        cancelled: || false,
    };
    assert!(matches!(
        reader.material("x:y"),
        Err(ValueTreeError::Invalid("member expected profile"))
    ));
    // Keep the pair preimage independently exercised when constructing the
    // known member key used to detect an orphan.
    let (member, _) = key(
        "material_ledgers",
        FieldShape::NestedMap,
        WorldFieldRecordKindV1::Entry,
        Some(&pair_token(&ledger_token(1, Some("a")), &text_token("x:y"))),
    );
    let _q = memory.reserve(512 << 10, MemoryClass::Ordinary).unwrap();
    assert!(orphan.get(member.as_bytes(), &state).unwrap().is_some());
}
#[test]
fn exhausted_io_work_memory_and_wrong_reader_class_do_not_report_absence() {
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
    for mode in 0..7 {
        let mut quota = physical();
        if mode == 0 {
            quota.reads_remaining = 0;
        }
        let state = store
            .state_session(
                quota,
                if mode == 4 {
                    MemoryClass::Recovery
                } else {
                    MemoryClass::Ordinary
                },
            )
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        if mode == 2 {
            io.nodes_remaining = 0;
        }
        let mut work = SemanticKeyBudget {
            bytes_remaining: if mode == 1 { 0 } else { 4096 },
        };
        let other = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let pool = if mode == 5 { &other } else { &memory };
        let _filler = if mode == 6 {
            Some(
                memory
                    .reserve((31 << 20) - memory.used(), MemoryClass::Ordinary)
                    .unwrap(),
            )
        } else {
            None
        };
        let mut reader = InventoryReadContext {
            root: &root,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: pool,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut work,
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || state.take_read_error().map(typed),
            cancelled: || mode == 3,
        };
        let result = reader.material("missing");
        match mode {
            0..=2 => assert!(
                matches!(result, Err(ValueTreeError::ResourceLimited(_))),
                "mode{mode}:{result:?}"
            ),
            3 => assert!(matches!(result, Err(ValueTreeError::Cancelled))),
            4 | 5 => assert!(matches!(
                result,
                Err(ValueTreeError::Invalid("inventory state pool/class"))
            )),
            _ => assert!(matches!(result, Err(ValueTreeError::Memory(_)))),
        }
    }
}
#[test]
fn cancellation_between_merkle_nodes_stops_before_value_reads() {
    struct CancelSource<'a, M> {
        inner: &'a M,
        flag: &'a Cell<bool>,
        calls: Cell<usize>,
    }
    impl<M: MerkleNodeReader> MerkleNodeReader for CancelSource<'_, M> {
        type Node = M::Node;
        fn allocation_class(&self) -> Option<MemoryClass> {
            self.inner.allocation_class()
        }
        fn allocation_budget(&self) -> Option<&MemoryBudget> {
            self.inner.allocation_budget()
        }
        fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
            self.calls.set(self.calls.get() + 1);
            let result = self.inner.read_node(id, max);
            self.flag.set(true);
            result
        }
    }
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
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let flag = Cell::new(false);
    let checked = CancelSource {
        inner: &state,
        flag: &flag,
        calls: Cell::new(0),
    };
    let values = store.value_session(physical());
    let mut io = logical();
    let mut work = SemanticKeyBudget {
        bytes_remaining: 4096,
    };
    let mut reader = InventoryReadContext {
        root: &root,
        state: &checked,
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
        cancelled: || flag.get(),
    };
    assert!(matches!(
        reader.material("x:y"),
        Err(ValueTreeError::Cancelled)
    ));
    assert_eq!(checked.calls.get(), 1);
    assert_eq!(io.nodes_remaining, 100_000);
}

fn patch(
    root: &StateRootV1,
    mut changes: Vec<StateWriteV1>,
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
) -> [u8; STATE_ROOT_BYTES] {
    let workspace = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    changes.sort_by(|a, b| a.key.cmp(&b.key));
    let source = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let delta =
        workspace.retain(StateDeltaV1::build(root.clone(), changes, &source, 2 << 20).unwrap());
    store
        .value_session(physical())
        .put_state_pages(&delta, memory)
        .unwrap();
    delta.after.canonical_bytes().unwrap().try_into().unwrap()
}

#[test]
fn shared_point_budget_exhaustion_remains_a_typed_error_after_successful_reads() {
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
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut io = logical();
    let mut work = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut reader = InventoryReadContext {
        root: &root,
        state: &state,
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
    let mut successes = 0;
    let mut exhausted = false;
    for _ in 0..20 {
        match reader.ledger(&MaterialLedgerId::Agent("a".into()), "x:y") {
            Ok(value) => {
                assert_eq!(value, LedgerQuantityV1::Value(-3));
                successes += 1;
            }
            Err(ValueTreeError::ResourceLimited(_)) => {
                exhausted = true;
                break;
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }
    assert!(successes > 0);
    assert!(exhausted);
    let remaining = state.remaining_io().unwrap();
    assert_eq!(remaining.reads_remaining, 0);
    assert!(remaining.read_bytes_remaining > 0);
}

#[test]
fn large_typed_ledger_query_keeps_request_charged_and_uses_only_recovery_headroom() {
    use oasis7_runtime::WorldState;
    use std::collections::BTreeMap;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let repetitions = (65 << 20) / 5 + 1;
        let length = repetitions * 5;
        let producer = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
        let bytes = {
            let source = producer
                .reserve(length + (1 << 20), MemoryClass::Ordinary)
                .unwrap()
                .retain(WorldState {
                    resources: BTreeMap::new(),
                    materials: BTreeMap::new(),
                    material_ledgers: BTreeMap::from([(
                        MaterialLedgerId::Agent("ab甲".repeat(repetitions)),
                        BTreeMap::from([("ore".into(), i64::MIN)]),
                    )]),
                    ..WorldState::default()
                });
            let store = IncrementalPageStore::create(&path, producer.clone(), compression).unwrap();
            build(&source, &store, &producer).1
        };
        assert_eq!(producer.used(), 0);
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let memory = MemoryBudget::new(96 << 20, 4 << 20).unwrap();
            // The original query identity is caller-owned and charged before
            // allocation in the very same pool as all read workspaces.
            let request = memory
                .reserve(length + 1024, MemoryClass::Ordinary)
                .unwrap()
                .retain(MaterialLedgerId::Agent("ab甲".repeat(repetitions)));
            let store =
                IncrementalPageStore::open(&path, memory.clone(), compression, class).unwrap();
            let state = store.state_session(physical(), class).unwrap();
            let values = store.value_session(physical());
            let _filler = if class == MemoryClass::Recovery {
                Some(
                    memory
                        .reserve((92 << 20) - memory.used(), MemoryClass::Ordinary)
                        .unwrap(),
                )
            } else {
                None
            };
            if class == MemoryClass::Recovery {
                assert!(memory.reserve(1, MemoryClass::Ordinary).is_err());
            }
            let root = StateRootV1::from_canonical_bytes(&bytes).unwrap();
            let mut io = logical();
            let mut work = SemanticKeyBudget {
                bytes_remaining: 256 << 20,
            };
            let mut reader = InventoryReadContext {
                root: &root,
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class,
                    io: &mut io,
                },
                key_work: &mut work,
                semantic_bytes: 128 << 20,
                record_bytes: 128 << 20,
                take_state_error: || state.take_read_error().map(typed),
                cancelled: || false,
            };
            assert_eq!(
                reader.ledger(&request, "ore").unwrap(),
                LedgerQuantityV1::Value(i64::MIN)
            );
        }
    }
}
