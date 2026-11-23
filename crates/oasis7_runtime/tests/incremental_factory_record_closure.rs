#![cfg(all(unix, not(target_arch = "wasm32")))]

use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::{AuthorityBinding, ExecutionParent, RootSetV1, TransactionLimits},
    incremental_record_root::{RECORD_ROOT_BYTES, RecordRootRefV1},
    incremental_state::{
        StateDeltaV1, StateNamespaceV1, StatePageWriter, StateRootV1, StateScanBudget, StateWriteV1,
    },
    incremental_value::{ValueIoBudget, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    WorldState,
    incremental_factory_records::{
        FactoryCellReadV1, FactoryCellV1, FactoryColumnV1, FactoryFamilyQualificationV1,
        FactoryFamilyReadContextV1, FactoryRecordKeyV1, FactoryValueV1, FactoryWorkBudgetV1,
        qualify_factory_family_v1, read_factory_cell_v1,
    },
    incremental_field_member::*,
    incremental_field_read::WorldFieldReadContextV1,
};
use std::io;

#[allow(dead_code)]
#[path = "support/incremental_factory_record_closure_fixture.rs"]
mod fixture;

fn physical() -> oasis7_distfs::incremental_page_store::PageIoBudget {
    fixture::physical()
}
fn logical() -> ValueIoBudget {
    fixture::logical()
}
fn field(name: &'static str, namespace: StateNamespaceV1) -> FieldDescriptor {
    FieldDescriptor {
        owner: FieldOwner::WorldState,
        name,
        destination: FieldDestination::State(namespace),
        shape: FieldShape::Map,
    }
}
fn text_token(text: &str) -> Vec<u8> {
    let mut token = vec![1];
    token.extend_from_slice(&(text.len() as u64).to_be_bytes());
    token.extend_from_slice(text.as_bytes());
    token
}
fn pair_token(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut token = vec![5];
    token.extend_from_slice(left);
    token.extend_from_slice(right);
    token
}
fn semantic_digest(token: &[u8]) -> [u8; 32] {
    let mut bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    bytes.extend_from_slice(token);
    *blake3::hash(&bytes).as_bytes()
}

/// Literal field-key profile, independent from WorldFieldKeyV1::new.
fn literal_field_key(name: &str, namespace: u16, kind: u8, token: Option<&[u8]>) -> Vec<u8> {
    let mut bytes = b"oasis7.iws.world-field-key.v1\0".to_vec();
    bytes.push(1); // WorldState owner
    bytes.extend_from_slice(&namespace.to_be_bytes());
    bytes.push(2); // Map shape
    bytes.push(kind);
    bytes.extend_from_slice(&(name.len() as u16).to_be_bytes());
    bytes.extend_from_slice(name.as_bytes());
    bytes.extend_from_slice(&token.map(semantic_digest).unwrap_or([0; 32]));
    bytes
}
fn member_key(
    name: &'static str,
    namespace: StateNamespaceV1,
    kind: WorldFieldRecordKindV1,
    token: Option<&[u8]>,
) -> (WorldFieldKeyV1, Vec<u8>, u64) {
    let digest = token.map(semantic_digest);
    let key = WorldFieldKeyV1::new(field(name, namespace), kind, digest).unwrap();
    let literal = literal_field_key(name, namespace as u16, kind as u8, token);
    assert_eq!(key.as_bytes(), literal);
    (
        key,
        literal,
        token.map_or(0, |value| {
            (b"oasis7.iws.semantic-key.v1\0".len() + value.len()) as u64
        }),
    )
}
fn limits() -> MemberRecordLimitsV1 {
    MemberRecordLimitsV1 {
        semantic_bytes: 8 << 20,
        payload_bytes: 8 << 20,
        record_bytes: 16 << 20,
    }
}
fn factory_work() -> FactoryWorkBudgetV1 {
    FactoryWorkBudgetV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 16 << 20,
        scan: StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 512 << 20,
            leaves_remaining: 100_000,
        },
    }
}
fn qualify_factory_roots(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    class: MemoryClass,
) -> ReservedValue<FactoryFamilyQualificationV1> {
    let state = store.state_session(physical(), class).unwrap();
    let values = store.value_session(physical());
    let mut value_io = logical();
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut outer_scan = StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 512 << 20,
        leaves_remaining: 100_000,
    };
    let mut work = factory_work();
    qualify_factory_family_v1(
        roots,
        &mut FactoryFamilyReadContextV1 {
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class,
                io: &mut value_io,
            },
            key_work: &mut key_work,
            semantic_bytes: 8 << 20,
            record_bytes: 16 << 20,
            max_payload_bytes: 8 << 20,
            outer_scan: &mut outer_scan,
            take_state_error: || state.take_read_error().map(typed),
            cancelled: || false,
        },
        &mut work,
    )
    .unwrap()
}
fn execution_parent(roots: RootSetV1) -> ExecutionParent {
    ExecutionParent {
        commit: [0x34; 32],
        roots,
        authority: AuthorityBinding {
            token: [0x56; 32],
            epoch: 7,
        },
    }
}
fn transaction_limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 128,
        max_writes: 16,
        max_facts: 8,
        max_pending_node_bytes: 16 << 20,
    }
}
fn typed(error: PageStoreError) -> ValueTreeError {
    match error {
        PageStoreError::Memory(error) => ValueTreeError::Memory(error),
        PageStoreError::ResourceLimited(reason) => ValueTreeError::ResourceLimited(reason),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        PageStoreError::Io(error) => ValueTreeError::Io(error),
        _ => ValueTreeError::Invalid("factory closure fixture page"),
    }
}

fn read_display_name(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    outer_key: &'static str,
    expected: &str,
    class: MemoryClass,
) -> (u64, u64, u64) {
    let state = store.state_session(physical(), class).unwrap();
    let values = store.value_session(physical());
    let before = values.remaining_io().unwrap();
    let mut value_io = logical();
    let initial_logical_bytes = value_io.read_bytes_remaining;
    let _output_charge = memory.reserve(expected.len().max(1), class).unwrap();
    let mut output = vec![0; expected.len()];
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let result = read_factory_cell_v1(
        FactoryRecordKeyV1::Factory(outer_key),
        FactoryCellV1::Column(FactoryColumnV1::SpecDisplayName),
        &mut WorldFieldReadContextV1 {
            root: &roots.state[0],
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class,
                io: &mut value_io,
            },
            key_work: &mut key_work,
            semantic_bytes: 8 << 20,
            record_bytes: 16 << 20,
            take_state_error: || state.take_read_error().map(typed),
            cancelled: || false,
        },
        8 << 20,
        &mut &mut output[..],
    )
    .unwrap();
    assert_eq!(
        result,
        FactoryCellReadV1::Value(FactoryValueV1::Text {
            utf8_bytes: expected.len() as u64,
        })
    );
    assert_eq!(output.as_slice(), expected.as_bytes());
    let after = values.remaining_io().unwrap();
    (
        before.reads_remaining - after.reads_remaining,
        before.read_bytes_remaining - after.read_bytes_remaining,
        initial_logical_bytes - value_io.read_bytes_remaining,
    )
}

fn assert_literal_child_value(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    outer_key: &str,
    expected: &str,
) -> StateRootV1 {
    let outer_token = text_token(outer_key);
    let (outer_field, outer_bytes, outer_semantic_bytes) = member_key(
        "factories",
        StateNamespaceV1::Business,
        WorldFieldRecordKindV1::Entry,
        Some(&outer_token),
    );
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let outer_value_root = roots.state[0].get(&outer_bytes, &state).unwrap().unwrap();

    let mut root_bytes = [0; RECORD_ROOT_BYTES];
    let mut value_io = logical();
    let info = read_member_record_profile_v1(
        &outer_value_root,
        &outer_field,
        &mut ValueReadContext {
            source: &values,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut value_io,
        },
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::RecordRoot,
            semantic_bytes: outer_semantic_bytes,
            limits: limits(),
        },
        &mut io::sink(),
        &mut &mut root_bytes[..],
        || false,
    )
    .unwrap();
    assert_eq!(info.codec, MemberPayloadCodecV1::RecordRoot);
    let reference = RecordRootRefV1::from_canonical_bytes(&root_bytes).unwrap();
    assert_eq!(
        reference.schema,
        FactoryRecordKeyV1::Factory(outer_key).schema()
    );
    assert_eq!(reference.owner_key, semantic_digest(&outer_token));
    let child = reference
        .validate_binding(
            &FactoryRecordKeyV1::Factory(outer_key).schema(),
            &semantic_digest(&outer_token),
            StateNamespaceV1::Business,
        )
        .unwrap()
        .clone();

    let child_token = pair_token(
        &pair_token(&[0], &text_token("spec")),
        &text_token("display_name"),
    );
    let (child_field, child_bytes, child_semantic_bytes) = member_key(
        "factories",
        StateNamespaceV1::Business,
        WorldFieldRecordKindV1::Entry,
        Some(&child_token),
    );
    let child_state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let child_value_root = child.get(&child_bytes, &child_state).unwrap().unwrap();
    let _output_charge = memory
        .reserve(expected.len().max(1), MemoryClass::Ordinary)
        .unwrap();
    let mut output = vec![0; expected.len()];
    let info = read_member_record_profile_v1(
        &child_value_root,
        &child_field,
        &mut ValueReadContext {
            source: &values,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut logical(),
        },
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::Text,
            semantic_bytes: child_semantic_bytes,
            limits: limits(),
        },
        &mut io::sink(),
        &mut &mut output[..],
        || false,
    )
    .unwrap();
    assert_eq!(info.codec, MemberPayloadCodecV1::Text);
    assert_eq!(output.as_slice(), expected.as_bytes());
    child
}

#[derive(Clone, Copy)]
enum BadBinding {
    Schema,
    OwnerKey,
    Namespace,
}

fn with_bad_binding(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    outer_key: &str,
    child: &StateRootV1,
    bad: BadBinding,
) -> RootSetV1 {
    let token = text_token(outer_key);
    let (_, key_bytes, semantic_bytes) = member_key(
        "factories",
        StateNamespaceV1::Business,
        WorldFieldRecordKindV1::Entry,
        Some(&token),
    );
    let schema = FactoryRecordKeyV1::Factory(outer_key).schema();
    let owner = semantic_digest(&token);
    let (schema, owner, child) = match bad {
        BadBinding::Schema => ([0x5a; 32], owner, child.clone()),
        BadBinding::OwnerKey => (schema, [0xa5; 32], child.clone()),
        BadBinding::Namespace => {
            let mut wrong = child.clone();
            wrong.namespace = StateNamespaceV1::Scheduler;
            (schema, owner, wrong)
        }
    };
    let reference = RecordRootRefV1::new(schema, owner, child).unwrap();
    let mut values = store.value_session(physical());
    let value = write_member_record_v1(
        field("factories", StateNamespaceV1::Business),
        WorldFieldRecordKindV1::Entry,
        outer_key,
        MemberPayloadRefV1::RecordRoot(&reference),
        &mut MemberRecordWriteContext {
            sink: &mut values,
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 64 << 20,
            },
            limits: limits(),
            cancelled: || false,
        },
    )
    .unwrap();
    assert_eq!(value.info.codec, MemberPayloadCodecV1::RecordRoot);
    assert_eq!(value.info.semantic_bytes, semantic_bytes);

    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let before = roots.state[0].get(&key_bytes, &state).unwrap().unwrap();
    let workspace = memory
        .reserve(512 * 1024 + 128 * 1024, MemoryClass::Ordinary)
        .unwrap();
    let delta = workspace.retain(
        StateDeltaV1::build(
            roots.state[0].clone(),
            vec![StateWriteV1 {
                key: key_bytes,
                before: Some(before),
                after: Some(value.value_root),
            }],
            &state,
            8 << 20,
        )
        .unwrap(),
    );
    values.put_state_pages(&delta, memory).unwrap();
    let mut result = roots.clone();
    result.state[0] = delta.after.clone();
    result
}

fn assert_binding_rejected(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    outer_key: &'static str,
) {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let result = read_factory_cell_v1(
        FactoryRecordKeyV1::Factory(outer_key),
        FactoryCellV1::Column(FactoryColumnV1::SpecDisplayName),
        &mut WorldFieldReadContextV1 {
            root: &roots.state[0],
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut key_work,
            semantic_bytes: 8 << 20,
            record_bytes: 16 << 20,
            take_state_error: || state.take_read_error().map(typed),
            cancelled: || false,
        },
        8 << 20,
        &mut io::sink(),
    );
    assert!(matches!(
        result,
        Err(ValueTreeError::Invalid("factory record root binding"))
    ));
}

#[test]
fn factory_record_root_binding_and_child_closure_survive_cold_reopen() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let source = fixture::source();
        let source_dir = fixture::Directory::new("source");
        let source_path = source_dir.0.join("pages");
        let (source_roots, bad_roots) = {
            let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
            let _fixture = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
            let store =
                IncrementalPageStore::create(&source_path, memory.clone(), compression).unwrap();
            let roots = fixture::seed_factory_roots(&store, &memory, &source);
            let child = assert_literal_child_value(
                &store,
                &memory,
                &roots,
                "outer:large/Δ",
                &source.factories["outer:large/Δ"].spec.display_name,
            );
            // Mutate test roots in the producer pool. Cold pools below only
            // read/reject them and never pay for a compression workspace.
            let bad = [
                BadBinding::Schema,
                BadBinding::OwnerKey,
                BadBinding::Namespace,
            ]
            .map(|bad| with_bad_binding(&store, &memory, &roots, "outer:large/Δ", &child, bad));
            (roots, bad)
        };

        let generic_dir = fixture::Directory::new("generic-only");
        let generic_path = generic_dir.0.join("pages");
        let generic_roots = {
            let source_memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
            let target_memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
            let source_store = IncrementalPageStore::open(
                &source_path,
                source_memory.clone(),
                compression,
                MemoryClass::Ordinary,
            )
            .unwrap();
            let target_store =
                IncrementalPageStore::create(&generic_path, target_memory.clone(), compression)
                    .unwrap();
            fixture::copy_generic_roots(
                &source_store,
                &target_store,
                &source_roots,
                &source_memory,
                &target_memory,
                MemoryClass::Ordinary,
            )
        };
        assert_eq!(generic_roots, source_roots);

        // Reopen the complete source under both ordinary and recovery classes.
        for class in [MemoryClass::Ordinary, MemoryClass::Recovery] {
            let memory = MemoryBudget::new(16 << 20, 8 << 20).unwrap();
            let _caller = memory.reserve(128 << 10, class).unwrap();
            let store =
                IncrementalPageStore::open(&source_path, memory.clone(), compression, class)
                    .unwrap();
            let small = &source.factories["outer:small/Δ"].spec.display_name;
            let large = &source.factories["outer:large/Δ"].spec.display_name;
            let (small_reads, small_bytes, _small_logical_bytes) = read_display_name(
                &store,
                &memory,
                &source_roots,
                "outer:small/Δ",
                small,
                class,
            );
            let (large_reads, large_bytes, large_logical_bytes) = read_display_name(
                &store,
                &memory,
                &source_roots,
                "outer:large/Δ",
                large,
                class,
            );
            assert!(large_reads > small_reads, "{large_reads} <= {small_reads}");
            assert!(large_bytes > small_bytes, "{large_bytes} <= {small_bytes}");
            assert!(large_logical_bytes >= large.len() as u64);
            if class == MemoryClass::Ordinary {
                assert_literal_child_value(&store, &memory, &source_roots, "outer:large/Δ", large);
                for bad in &bad_roots {
                    assert_binding_rejected(&store, &memory, bad, "outer:large/Δ");
                }
            }
        }

        // The copied store has exact generic roots and complete top-level
        // ValueRoot closures, but no factory-local child state pages.
        let generic_memory = MemoryBudget::new(24 << 20, 8 << 20).unwrap();
        let _caller = generic_memory
            .reserve(128 << 10, MemoryClass::Ordinary)
            .unwrap();
        let generic_store = IncrementalPageStore::open(
            &generic_path,
            generic_memory.clone(),
            compression,
            MemoryClass::Ordinary,
        )
        .unwrap();
        fixture::verify_generic_root_values(
            &generic_store,
            &generic_roots,
            &generic_memory,
            MemoryClass::Ordinary,
        );
        let state = generic_store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = generic_store.value_session(physical());
        let mut key_work = SemanticKeyBudget {
            bytes_remaining: 64 << 20,
        };
        let result = read_factory_cell_v1(
            FactoryRecordKeyV1::Factory("outer:large/Δ"),
            FactoryCellV1::Column(FactoryColumnV1::SpecDisplayName),
            &mut WorldFieldReadContextV1 {
                root: &generic_roots.state[0],
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &generic_memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                },
                key_work: &mut key_work,
                semantic_bytes: 8 << 20,
                record_bytes: 16 << 20,
                take_state_error: || state.take_read_error().map(typed),
                cancelled: || false,
            },
            8 << 20,
            &mut io::sink(),
        );
        assert!(result.is_err());
        assert!(!matches!(result, Ok(FactoryCellReadV1::RecordAbsent)));
    }
}

#[test]
fn rejects_legacy_flat_factory_members() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = fixture::Directory::new("legacy-flat-factory");
        let path = directory.0.join("pages");
        let memory = MemoryBudget::new(48 << 20, 8 << 20).unwrap();
        let _caller = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
        let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
        let source = WorldState::default();
        let mut roots = fixture::seed_factory_roots(&store, &memory, &source);

        // This is the pre-closure shape: an outer record ID and nested column
        // path share one flat semantic key in the factories map. The new owner
        // treats every non-marker Entry as a RecordRootRef and must reject it.
        let legacy_semantic = (("outer:legacy", "spec"), "display_name");
        let legacy_token = pair_token(
            &pair_token(&text_token("outer:legacy"), &text_token("spec")),
            &text_token("display_name"),
        );
        let (expected_key, expected_key_bytes, semantic_bytes) = member_key(
            "factories",
            StateNamespaceV1::Business,
            WorldFieldRecordKindV1::Entry,
            Some(&legacy_token),
        );
        let mut values = store.value_session(physical());
        let entry = write_member_record_v1(
            field("factories", StateNamespaceV1::Business),
            WorldFieldRecordKindV1::Entry,
            &legacy_semantic,
            MemberPayloadRefV1::Text("legacy-flat-value"),
            &mut MemberRecordWriteContext {
                sink: &mut values,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 64 << 20,
                },
                limits: limits(),
                cancelled: || false,
            },
        )
        .unwrap();
        assert_eq!(entry.field_key, expected_key);
        assert_eq!(entry.field_key.as_bytes(), expected_key_bytes);
        assert_eq!(entry.info.semantic_bytes, semantic_bytes);

        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let workspace = memory
            .reserve(512 * 1024 + 128 * 1024, MemoryClass::Ordinary)
            .unwrap();
        let delta = workspace.retain(
            StateDeltaV1::build(
                roots.state[0].clone(),
                vec![StateWriteV1 {
                    key: expected_key_bytes,
                    before: None,
                    after: Some(entry.value_root),
                }],
                &state,
                8 << 20,
            )
            .unwrap(),
        );
        values.put_state_pages(&delta, &memory).unwrap();
        roots.state[0] = delta.after.clone();

        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut value_io = logical();
        let mut key_work = SemanticKeyBudget {
            bytes_remaining: 64 << 20,
        };
        let mut outer_scan = StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 512 << 20,
            leaves_remaining: 100_000,
        };
        let mut work = factory_work();
        let result = qualify_factory_family_v1(
            &roots,
            &mut FactoryFamilyReadContextV1 {
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut value_io,
                },
                key_work: &mut key_work,
                semantic_bytes: 8 << 20,
                record_bytes: 16 << 20,
                max_payload_bytes: 8 << 20,
                outer_scan: &mut outer_scan,
                take_state_error: || state.take_read_error().map(typed),
                cancelled: || false,
            },
            &mut work,
        );
        assert!(matches!(result, Err(ValueTreeError::Invalid(_))));
    }
}

#[test]
fn qualified_factory_create_receiver_and_typed_transfer_preserve_cold_child() {
    fixture::qualified_factory_create_receiver_and_typed_transfer_preserve_cold_child();
}

#[test]
fn foreign_pool_inner_state_pages_poison_a_successful_typed_prefix() {
    use oasis7_formal_protocol::incremental_execution::ExecutionTransaction;
    use oasis7_formal_protocol::incremental_value::ValueNodeReader;
    use oasis7_runtime::incremental_factory_records::*;
    use oasis7_runtime::incremental_field_overlay::*;
    use std::sync::Arc;
    struct Foreign<'a, R> {
        source: &'a R,
        pool: MemoryBudget,
        undercharged: bool,
    }
    impl<R: ValueNodeReader> ValueNodeReader for Foreign<'_, R> {
        fn read_node(
            &self,
            id: &[u8; 32],
            max: usize,
            memory: &MemoryBudget,
            class: MemoryClass,
        ) -> Result<Arc<oasis7_proto::memory_budget::ReservedValue<Vec<u8>>>, ValueTreeError>
        {
            let page = self.source.read_node(id, max, memory, class)?;
            if page.starts_with(b"oasis7.iws.state-page.v1\0") {
                let charge = if self.undercharged {
                    memory.reserve(1, class)?
                } else {
                    self.pool.reserve(page.len(), MemoryClass::Ordinary)?
                };
                return Ok(Arc::new(charge.retain(page.to_vec())));
            }
            Ok(page)
        }
    }
    let dir = fixture::Directory::new("foreign-inner-pool");
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&dir.0.join("pages"), memory.clone(), PageCompression::Raw)
            .unwrap();
    let mut source = fixture::source();
    source.factories.remove("outer:large/Δ");
    let roots = fixture::seed_factory_roots(&store, &memory, &source);
    let mut all_rejected = true;
    for undercharged in [false, true] {
        let parent = execution_parent(roots.clone());
        let live = parent.fence().unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let good = store.value_session(physical());
        let mut writer = store.value_session(physical());
        let mut tx =
            ExecutionTransaction::begin(parent, [0x7a; 32], &state, &memory, transaction_limits())
                .unwrap();
        let mut work = factory_work();
        {
            let mut read = WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &good,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                semantic_bytes: 8 << 20,
                record_bytes: 16 << 20,
                take_state_error: || state.take_read_error().map(typed),
                cancelled: || false,
            };
            let mut write = MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut logical(),
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                limits: limits(),
                cancelled: || false,
            };
            let guard = read.transaction.guard();
            assert!(
                change_factory_cell_overlay_v1(
                    FactoryCellChangeV1 {
                        key: FactoryRecordKeyV1::Factory("outer:small/Δ"),
                        cell: FactoryCellV1::Column(FactoryColumnV1::FactoryBuiltAt),
                        expected: Some(FactoryValueRefV1::Unsigned(17)),
                        after: Some(FactoryValueRefV1::Unsigned(18)),
                        max_current_payload_bytes: 8
                    },
                    &mut read,
                    &mut write,
                    &mut work,
                    &guard,
                    &live
                )
                .unwrap()
            );
        }
        let foreign = Foreign {
            source: &good,
            pool: MemoryBudget::new(4 << 20, 0).unwrap(),
            undercharged,
        };
        let result = read_factory_cell_overlay_v1(
            FactoryRecordKeyV1::Factory("outer:small/Δ"),
            FactoryCellV1::Column(FactoryColumnV1::FactoryBuiltAt),
            &mut WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &foreign,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                },
                key_work: &mut SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                semantic_bytes: 8 << 20,
                record_bytes: 16 << 20,
                take_state_error: || state.take_read_error().map(typed),
                cancelled: || false,
            },
            8,
            &mut io::sink(),
        );
        let rejected = matches!(
            &result,
            Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
        );
        let guard = tx.guard();
        let seal = tx.seal(&guard, &live, 1);
        let aborted = matches!(
            seal,
            Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
        );
        eprintln!(
            "inner state undercharged={undercharged} result={result:?} rejected={rejected} aborted={aborted}"
        );
        all_rejected &= rejected && aborted;
    }
    assert!(
        all_rejected,
        "foreign/undercharged state pages cannot escape as successful caller-pool reads"
    );
}

#[test]
fn scalar_receiver_cost_does_not_follow_unchanged_text_and_vector_size() {
    fixture::scalar_receiver_cost_does_not_follow_unchanged_text_and_vector_size();
}
