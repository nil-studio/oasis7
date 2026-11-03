#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{
        StateDeltaV1, StateNamespaceV1, StatePageWriter, StateRootV1, StateWriteV1,
    },
    incremental_value::{ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    WorldState, incremental_field_member::*, incremental_field_read::*, incremental_world_u64::*,
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical, physical};
#[path = "support/incremental_control_noop_cancel.rs"]
mod noop_cancel;
fn typed(error: PageStoreError) -> ValueTreeError {
    match error {
        PageStoreError::Memory(e) => ValueTreeError::Memory(e),
        PageStoreError::ResourceLimited(r) => ValueTreeError::ResourceLimited(r),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        PageStoreError::Io(e) => ValueTreeError::Io(e),
        _ => ValueTreeError::Invalid("state page failure"),
    }
}
struct CancelOnWrite<'a, W> {
    inner: &'a mut W,
    cancelled: &'a std::cell::Cell<bool>,
    calls: &'a std::cell::Cell<usize>,
    arm: bool,
}
impl<W: oasis7_formal_protocol::incremental_value::ValueNodeWriter>
    oasis7_formal_protocol::incremental_value::ValueNodeWriter for CancelOnWrite<'_, W>
{
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.calls.set(self.calls.get() + 1);
        self.inner.put_node(id, bytes)?;
        if self.arm {
            self.cancelled.set(true);
        }
        Ok(())
    }
}
#[test]
fn required_unsigned_fields_reopen_in_their_original_namespaces_and_read_classes() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let producer = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let source = WorldState {
            time: 0,
            next_industry_settlement_order: u64::MAX,
            next_module_release_request_id: 2,
            next_module_instance_id: 3,
            next_module_market_order_id: 4,
            next_module_market_sale_id: 5,
            agent_claim_last_processed_epoch: 6,
            ..WorldState::default()
        };
        let roots = {
            let store = IncrementalPageStore::create(&path, producer.clone(), compression).unwrap();
            let mut values = store.value_session(physical());
            let _batch = producer.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
            let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(14);
            prepare_world_u64_genesis_v1(
                &source,
                &mut MemberRecordWriteContext {
                    sink: &mut values,
                    memory: &producer,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 8,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                },
                &mut 14,
                &mut |write| {
                    assert!(rows.len() < 14);
                    rows.push(write);
                    Ok(())
                },
            )
            .unwrap();
            let wrong_codec = write_member_record_v1(
                WorldU64FieldV1::Time.descriptor().unwrap(),
                WorldFieldRecordKindV1::Entry,
                &(),
                MemberPayloadRefV1::I64(0),
                &mut MemberRecordWriteContext {
                    sink: &mut values,
                    memory: &producer,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 8,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                },
            )
            .unwrap();
            [0, 1, 2, 3].map(|variant| {
                // Each independent fixture root uses the same finite physical
                // lookup allowance; this is not one four-root operation.
                let state = store
                    .state_session(physical(), MemoryClass::Ordinary)
                    .unwrap();
                let namespace = if variant == 1 {
                    StateNamespaceV1::Scheduler
                } else {
                    StateNamespaceV1::Allocator
                };
                let workspace = producer.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
                let mut changes: Vec<_> = rows
                    .iter()
                    .enumerate()
                    .filter(|(index, row)| {
                        row.namespace == namespace && (variant != 2 || index % 2 == 0)
                    })
                    .map(|(index, row)| StateWriteV1 {
                        key: row.mutation.key.clone(),
                        before: None,
                        after: if variant == 3 && index == 1 {
                            Some(wrong_codec.value_root)
                        } else {
                            row.mutation.after
                        },
                    })
                    .collect();
                changes.sort_by(|a, b| a.key.cmp(&b.key));
                let delta = workspace.retain(
                    StateDeltaV1::build(
                        StateRootV1::empty(namespace).unwrap(),
                        changes,
                        &state,
                        2 << 20,
                    )
                    .unwrap(),
                );
                assert_eq!(
                    delta.after.leaf_count,
                    match variant {
                        1 => 2,
                        2 => 6,
                        _ => 12,
                    }
                );
                store
                    .value_session(physical())
                    .put_state_pages(&delta, &producer)
                    .unwrap();
                delta.after.canonical_bytes().unwrap()
            })
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
            for (index, field) in WorldU64FieldV1::ALL.into_iter().enumerate() {
                let root_index =
                    usize::from(field == WorldU64FieldV1::AgentClaimLastProcessedEpoch);
                let root = StateRootV1::from_canonical_bytes(&roots[root_index]).unwrap();
                let state = store.state_session(physical(), class).unwrap();
                let values = store.value_session(physical());
                let missing = StateRootV1::from_canonical_bytes(&roots[2]).unwrap();
                let signed = StateRootV1::from_canonical_bytes(&roots[3]).unwrap();
                let mut reader = WorldFieldReadContextV1 {
                    root: &root,
                    state: &state,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || state.take_read_error().map(typed),
                    cancelled: || false,
                };
                let expected = [0, u64::MAX, 2, 3, 4, 5, 6][index];
                assert_eq!(read_world_u64_v1(field, &mut reader).unwrap(), expected);
                if field == WorldU64FieldV1::Time {
                    reader.root = &missing;
                    assert!(matches!(
                        read_world_u64_v1(field, &mut reader),
                        Err(ValueTreeError::Invalid("world u64 missing entry"))
                    ));
                    reader.root = &signed;
                    assert!(matches!(
                        read_world_u64_v1(field, &mut reader),
                        Err(ValueTreeError::Invalid("member expected profile"))
                    ));
                }
                let wrong = StateRootV1::from_canonical_bytes(&roots[1 - root_index]).unwrap();
                reader.root = &wrong;
                assert!(matches!(
                    read_world_u64_v1(field, &mut reader),
                    Err(ValueTreeError::Invalid("field namespace"))
                ));
                let empty = StateRootV1::empty(field.namespace()).unwrap();
                reader.root = &empty;
                assert!(matches!(
                    read_world_u64_v1(field, &mut reader),
                    Err(ValueTreeError::Invalid("world u64 missing metadata"))
                ));
            }
        }
        // A fresh shared transaction reads both original namespace roots and
        // then sees a prior owner contribution without replacing its parent.
        let committed_root = {
            use oasis7_formal_protocol::incremental_execution::{
                AuthorityBinding, ExecutionParent, ExecutionTransaction, RootSetV1,
                TransactionLimits,
            };
            use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
            let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
            let store = IncrementalPageStore::open(
                &path,
                memory.clone(),
                compression,
                MemoryClass::Ordinary,
            )
            .unwrap();
            let mut root_set = RootSetV1::empty("world-u64".into(), [3; 32]).unwrap();
            for bytes in &roots[..2] {
                let root = StateRootV1::from_canonical_bytes(bytes).unwrap();
                let namespace = root.namespace;
                *root_set
                    .state
                    .iter_mut()
                    .find(|r| r.namespace == namespace)
                    .unwrap() = root;
            }
            let parent = ExecutionParent {
                commit: [4; 32],
                roots: root_set,
                authority: AuthorityBinding {
                    token: [5; 32],
                    epoch: 7,
                },
            };
            let live = parent.fence().unwrap();
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let mut transaction = ExecutionTransaction::begin(
                parent.clone(),
                [6; 32],
                &state,
                &memory,
                TransactionLimits {
                    max_reads: 32,
                    max_writes: 8,
                    max_facts: 8,
                    max_pending_node_bytes: 4 << 20,
                },
            )
            .unwrap();
            let values = store.value_session(physical());
            {
                let mut read = WorldFieldOverlayContextV1 {
                    transaction: &mut transaction,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || state.take_read_error().map(typed),
                    cancelled: || false,
                };
                for (field, expected) in
                    WorldU64FieldV1::ALL
                        .into_iter()
                        .zip([0, u64::MAX, 2, 3, 4, 5, 6])
                {
                    assert_eq!(
                        read_world_u64_overlay_v1(field, &mut read).unwrap(),
                        expected
                    );
                }
            }
            {
                let mut writer = store.value_session(physical());
                let mut read = WorldFieldOverlayContextV1 {
                    transaction: &mut transaction,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || state.take_read_error().map(typed),
                    cancelled: || false,
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut writer,
                    memory: &memory,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 8,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                };
                for (expected, after, changed) in [(0, 10, true), (10, 13, true), (13, 13, false)] {
                    let guard = read.transaction.guard();
                    assert_eq!(
                        change_world_u64_overlay_v1(
                            WorldU64FieldV1::Time,
                            expected,
                            after,
                            &mut read,
                            &mut write,
                            &guard,
                            &live
                        )
                        .unwrap(),
                        changed
                    );
                    assert_eq!(
                        read_world_u64_overlay_v1(WorldU64FieldV1::Time, &mut read).unwrap(),
                        after
                    );
                }
            }
            let mut writer = store.value_session(physical());
            let record = write_member_record_v1(
                WorldU64FieldV1::Time.descriptor().unwrap(),
                WorldFieldRecordKindV1::Entry,
                &(),
                MemberPayloadRefV1::U64(42),
                &mut MemberRecordWriteContext {
                    sink: &mut writer,
                    memory: &memory,
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 8,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                },
            )
            .unwrap();
            transaction
                .write(
                    StateNamespaceV1::Allocator,
                    record.field_key.as_bytes(),
                    Some(record.value_root),
                )
                .unwrap();
            {
                let mut read = WorldFieldOverlayContextV1 {
                    transaction: &mut transaction,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || state.take_read_error().map(typed),
                    cancelled: || false,
                };
                assert_eq!(
                    read_world_u64_overlay_v1(WorldU64FieldV1::Time, &mut read).unwrap(),
                    42
                );
            }
            let guard = transaction.guard();
            let sealed = transaction.seal(&guard, &live, 1).unwrap();
            assert_eq!(sealed.record.reads.len(), 14);
            assert_eq!(sealed.record.writes.len(), 1);
            let verifier_state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let verified = oasis7_formal_protocol::incremental_execution::TransitionVerifier {
                parent: &parent,
                operation: [6; 32],
                source: &verifier_state,
                memory: &memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 4 << 20,
            }
            .verify(&sealed.record)
            .unwrap();
            assert_eq!(verified.record.after, sealed.record.after);
            let mut pages = store.value_session(physical());
            for node in &verified.new_nodes {
                use oasis7_formal_protocol::incremental_value::ValueNodeWriter;
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
            let committed_root = verified
                .record
                .after
                .state
                .iter()
                .find(|root| root.namespace == StateNamespaceV1::Allocator)
                .unwrap()
                .canonical_bytes()
                .unwrap();
            let failure_state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let mut transaction = ExecutionTransaction::begin(
                parent.clone(),
                [6; 32],
                &failure_state,
                &memory,
                TransactionLimits {
                    max_reads: 32,
                    max_writes: 8,
                    max_facts: 8,
                    max_pending_node_bytes: 4 << 20,
                },
            )
            .unwrap();
            transaction
                .write(
                    StateNamespaceV1::Allocator,
                    record.field_key.as_bytes(),
                    None,
                )
                .unwrap();
            {
                let mut read = WorldFieldOverlayContextV1 {
                    transaction: &mut transaction,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || state.take_read_error().map(typed),
                    cancelled: || false,
                };
                assert!(read_world_u64_overlay_v1(WorldU64FieldV1::Time, &mut read).is_err());
            }
            let guard = transaction.guard();
            assert!(transaction.seal(&guard, &live, 1).is_err());
            // A failed second change cannot publish an earlier valid value.
            for case in 0..6 {
                let state = store
                    .state_session(physical(), MemoryClass::Ordinary)
                    .unwrap();
                let mut transaction = ExecutionTransaction::begin(
                    parent.clone(),
                    [6; 32],
                    &state,
                    &memory,
                    TransactionLimits {
                        max_reads: 32,
                        max_writes: 8,
                        max_facts: 8,
                        max_pending_node_bytes: 4 << 20,
                    },
                )
                .unwrap();
                let stale = transaction.guard();
                transaction
                    .write(
                        StateNamespaceV1::Allocator,
                        record.field_key.as_bytes(),
                        Some(record.value_root),
                    )
                    .unwrap();
                let guard = if case == 3 {
                    stale
                } else {
                    transaction.guard()
                };
                let other_pool = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
                let write_cancel = std::cell::Cell::new(false);
                let write_calls = std::cell::Cell::new(0);
                let mut session = store.value_session(physical());
                let mut writer = CancelOnWrite {
                    inner: &mut session,
                    cancelled: &write_cancel,
                    calls: &write_calls,
                    arm: case == 2,
                };
                let mut read = WorldFieldOverlayContextV1 {
                    transaction: &mut transaction,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: if case == 5 { 0 } else { 4096 },
                    },
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || state.take_read_error().map(typed),
                    cancelled: || case == 1,
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut writer,
                    memory: if case == 4 { &other_pool } else { &memory },
                    io: &mut logical(),
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 1024,
                        payload_bytes: 8,
                        record_bytes: 4096,
                    },
                    cancelled: || write_cancel.get(),
                };
                let result = change_world_u64_overlay_v1(
                    WorldU64FieldV1::Time,
                    if case == 0 { 99 } else { 42 },
                    43,
                    &mut read,
                    &mut write,
                    &guard,
                    &live,
                );
                use oasis7_formal_protocol::incremental_execution::ExecutionError;
                use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1 as Error;
                match (case, result) {
                    (0, Err(Error::Value(ValueTreeError::Invalid("world u64 expected value"))))
                    | (1, Err(Error::Value(ValueTreeError::Cancelled)))
                    | (
                        2,
                        Err(Error::Member(MemberRecordError::Value(ValueTreeError::Cancelled))),
                    )
                    | (3, Err(Error::Execution(ExecutionError::StaleGuard)))
                    | (
                        4,
                        Err(Error::Value(ValueTreeError::Invalid("world u64 overlay pool/class"))),
                    )
                    | (5, Err(Error::Value(ValueTreeError::ResourceLimited(_)))) => {}
                    (case, result) => panic!("unexpected failure {case}: {result:?}"),
                }
                if case == 2 {
                    assert!(
                        write_calls.get() > 0,
                        "must reach the value writer before cancellation"
                    );
                }
                let guard = transaction.guard();
                assert!(transaction.seal(&guard, &live, 1).is_err());
            }
            committed_root
        };
        // Transaction/store/value sessions and their pool have been dropped.
        // The initial producer handle remains empty (checked above). Cold reads
        // use an independent pool and authenticate persisted new and old roots.
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
            for (bytes, expected) in [(&roots[0], 0), (&committed_root, 42)] {
                let root = StateRootV1::from_canonical_bytes(bytes).unwrap();
                let state = store.state_session(physical(), class).unwrap();
                let values = store.value_session(physical());
                let mut reader = WorldFieldReadContextV1 {
                    root: &root,
                    state: &state,
                    values: ValueReadContext {
                        source: &values,
                        memory: &memory,
                        class,
                        io: &mut logical(),
                    },
                    key_work: &mut SemanticKeyBudget {
                        bytes_remaining: 4096,
                    },
                    semantic_bytes: 1024,
                    record_bytes: 4096,
                    take_state_error: || state.take_read_error().map(typed),
                    cancelled: || false,
                };
                assert_eq!(
                    read_world_u64_v1(WorldU64FieldV1::Time, &mut reader).unwrap(),
                    expected
                );
            }
        }
    }
}
