#![cfg(all(unix, not(target_arch = "wasm32")))]
use oasis7_distfs::incremental_page_store::{IncrementalPageStore, PageCompression};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::{
        StateDeltaV1, StateNamespaceV1, StatePageWriter, StateRootV1, StateWriteV1,
    },
    incremental_value::{ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    WorldState, incremental_field_member::*, incremental_field_read::*,
    incremental_world_id_sets::*,
};
#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical, physical};
#[test]
fn original_set_ids_and_empty_field_presence_survive_cold_reads() {
    use WorldIdSetFieldV1 as Field;
    use WorldIdSetKeyRefV1 as Key;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let dir = Directory::new();
        let path = dir.0.join("pages");
        let producer = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let source = WorldState {
            completed_logistics_route_ids: ["".into(), "route:一".into()].into(),
            retired_factory_ids: Default::default(),
            settled_logistics_transit_ids: [0, u64::MAX].into(),
            settled_factory_build_ids: [5, u64::MAX].into(),
            settled_recipe_job_ids: [0, 17].into(),
            ..WorldState::default()
        };
        let root_bytes = {
            let store = IncrementalPageStore::create(&path, producer.clone(), compression).unwrap();
            let mut values = store.value_session(physical());
            let _batch = producer.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
            let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(13);
            let mut remaining = 13;
            assert_eq!(
                prepare_world_id_sets_genesis_v1(
                    &source,
                    &mut MemberRecordWriteContext {
                        sink: &mut values,
                        memory: &producer,
                        io: &mut logical(),
                        key_work: &mut SemanticKeyBudget {
                            bytes_remaining: 4096
                        },
                        limits: MemberRecordLimitsV1 {
                            semantic_bytes: 1024,
                            payload_bytes: 0,
                            record_bytes: 4096
                        },
                        cancelled: || false
                    },
                    &mut remaining,
                    &mut |write| {
                        assert!(rows.len() < 13);
                        assert_eq!(write.namespace, StateNamespaceV1::Authorization);
                        rows.push(write);
                        Ok(())
                    }
                )
                .unwrap(),
                8
            );
            assert_eq!(remaining, 0);
            // Independent source-to-field oracle, using literal field names
            // and distinct IDs rather than the producer's enum routing.
            for (name, id) in [
                ("settled_logistics_transit_ids", u64::MAX),
                ("settled_factory_build_ids", 5),
                ("settled_recipe_job_ids", 17),
            ] {
                let mut hash = blake3::Hasher::new();
                hash.update(b"oasis7.iws.semantic-key.v1\0");
                hash.update(&[2]);
                hash.update(&id.to_be_bytes());
                let field = FieldDescriptor {
                    owner: FieldOwner::WorldState,
                    name,
                    destination: FieldDestination::State(StateNamespaceV1::Authorization),
                    shape: FieldShape::Set,
                };
                let expected = WorldFieldKeyV1::new(
                    field,
                    WorldFieldRecordKindV1::Entry,
                    Some(*hash.finalize().as_bytes()),
                )
                .unwrap();
                assert!(
                    rows.iter()
                        .any(|row| row.mutation.key == expected.as_bytes()),
                    "missing original {name} ID {id}"
                );
            }

            let mut prefix = 0;
            let failure = prepare_world_id_sets_genesis_v1(
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
                        payload_bytes: 0,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                },
                &mut 4,
                &mut |_| {
                    prefix += 1;
                    Ok(())
                },
            );
            assert!(matches!(
                failure,
                Err(MemberRecordError::Value(ValueTreeError::ResourceLimited(
                    "world ID set records"
                )))
            ));
            assert_eq!(prefix, 4);
            let cancelled = prepare_world_id_sets_genesis_v1(
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
                        payload_bytes: 0,
                        record_bytes: 4096,
                    },
                    cancelled: || true,
                },
                &mut 13,
                &mut |_| panic!("cancelled producer must not output writes"),
            );
            assert!(matches!(
                cancelled,
                Err(MemberRecordError::Value(ValueTreeError::Cancelled))
            ));

            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let workspace = producer.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
            let mut changes: Vec<_> = rows
                .iter()
                .map(|row| StateWriteV1 {
                    key: row.mutation.key.clone(),
                    before: None,
                    after: row.mutation.after,
                })
                .collect();
            changes.sort_by(|a, b| a.key.cmp(&b.key));
            let delta = workspace.retain(
                StateDeltaV1::build(
                    StateRootV1::empty(StateNamespaceV1::Authorization).unwrap(),
                    changes,
                    &state,
                    2 << 20,
                )
                .unwrap(),
            );
            assert_eq!(delta.after.leaf_count, 13);
            values.put_state_pages(&delta, &producer).unwrap();
            delta.after.canonical_bytes().unwrap()
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
            for (field, key, expected) in [
                (Field::CompletedLogisticsRoutes, Key::Text(""), true),
                (Field::CompletedLogisticsRoutes, Key::Text("route:一"), true),
                (Field::CompletedLogisticsRoutes, Key::Text("route"), false),
                (Field::RetiredFactories, Key::Text(""), false),
                (Field::SettledLogisticsTransits, Key::Action(0), true),
                (Field::SettledLogisticsTransits, Key::Action(u64::MAX), true),
                (Field::SettledFactoryBuilds, Key::Action(u64::MAX), true),
                (Field::SettledFactoryBuilds, Key::Action(5), true),
                (Field::SettledFactoryBuilds, Key::Action(0), false),
                (Field::SettledRecipeJobs, Key::Action(17), true),
                (Field::SettledRecipeJobs, Key::Action(u64::MAX), false),
                (Field::SettledRecipeJobs, Key::Action(0), true),
                (Field::SettledRecipeJobs, Key::Action(1), false),
            ] {
                let state = store.state_session(physical(), class).unwrap();
                let values = store.value_session(physical());
                let mut context = WorldFieldReadContextV1 {
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
                    take_state_error: || {
                        state
                            .take_read_error()
                            .map(|_| ValueTreeError::Invalid("state page failure"))
                    },
                    cancelled: || false,
                };
                assert_eq!(
                    world_id_set_contains_v1(field, key, &mut context).unwrap(),
                    expected
                );
                let wrong = match key {
                    Key::Text(_) => Key::Action(0),
                    Key::Action(_) => Key::Text("0"),
                };
                assert!(matches!(
                    world_id_set_contains_v1(field, wrong, &mut context),
                    Err(ValueTreeError::Invalid("world ID set key type"))
                ));
                let empty = StateRootV1::empty(StateNamespaceV1::Authorization).unwrap();
                context.root = &empty;
                assert!(matches!(
                    world_id_set_contains_v1(field, key, &mut context),
                    Err(ValueTreeError::Invalid("world ID set missing metadata"))
                ));
            }
        }
        let next = {
            use oasis7_formal_protocol::incremental_execution::{
                AuthorityBinding, ExecutionParent, ExecutionTransaction, RootSetV1,
                TransactionLimits, TransitionVerifier,
            };
            use oasis7_formal_protocol::incremental_value::ValueNodeWriter;
            use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
            let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
            let store = IncrementalPageStore::open(
                &path,
                memory.clone(),
                compression,
                MemoryClass::Ordinary,
            )
            .unwrap();
            let mut roots = RootSetV1::empty("world-id-sets".into(), [3; 32]).unwrap();
            *roots
                .state
                .iter_mut()
                .find(|root| root.namespace == StateNamespaceV1::Authorization)
                .unwrap() = StateRootV1::from_canonical_bytes(&root_bytes).unwrap();
            let parent = ExecutionParent {
                commit: [4; 32],
                roots,
                authority: AuthorityBinding {
                    token: [5; 32],
                    epoch: 7,
                },
            };
            let live = parent.fence().unwrap();
            let limits = TransactionLimits {
                max_reads: 16,
                max_writes: 8,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            };
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let mut transaction =
                ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits)
                    .unwrap();
            let values = store.value_session(physical());
            let mut writer = store.value_session(physical());
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
                    take_state_error: || {
                        state
                            .take_read_error()
                            .map(|_| ValueTreeError::Invalid("state page failure"))
                    },
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
                        payload_bytes: 0,
                        record_bytes: 4096,
                    },
                    cancelled: || false,
                };
                for (field, key, expected, after, changed) in [
                    (Field::SettledRecipeJobs, Key::Action(0), true, false, true),
                    (Field::SettledRecipeJobs, Key::Action(0), false, true, true),
                    (Field::SettledRecipeJobs, Key::Action(1), false, true, true),
                    (
                        Field::RetiredFactories,
                        Key::Text("factory:一"),
                        false,
                        true,
                        true,
                    ),
                    (Field::SettledRecipeJobs, Key::Action(1), true, true, false),
                ] {
                    let guard = read.transaction.guard();
                    assert_eq!(
                        change_world_id_set_overlay_v1(
                            WorldIdSetChangeV1 {
                                field,
                                key,
                                expected,
                                after
                            },
                            &mut read,
                            &mut write,
                            &guard,
                            &live
                        )
                        .unwrap(),
                        changed
                    );
                    assert_eq!(
                        world_id_set_contains_overlay_v1(field, key, &mut read).unwrap(),
                        after
                    );
                }
            }
            let guard = transaction.guard();
            let sealed = transaction.seal(&guard, &live, 1).unwrap();
            assert_eq!(sealed.record.reads.len(), 5);
            assert_eq!(sealed.record.writes.len(), 2);
            let verifier = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let verified = TransitionVerifier {
                parent: &parent,
                operation: [6; 32],
                source: &verifier,
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
            for wrong_key in [true, false] {
                let state = store
                    .state_session(physical(), MemoryClass::Ordinary)
                    .unwrap();
                let mut transaction =
                    ExecutionTransaction::begin(parent.clone(), [6; 32], &state, &memory, limits)
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
                        take_state_error: || {
                            state
                                .take_read_error()
                                .map(|_| ValueTreeError::Invalid("state page failure"))
                        },
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
                            payload_bytes: 0,
                            record_bytes: 4096,
                        },
                        cancelled: || false,
                    };
                    let guard = read.transaction.guard();
                    change_world_id_set_overlay_v1(
                        WorldIdSetChangeV1 {
                            field: Field::RetiredFactories,
                            key: Key::Text("factory:一"),
                            expected: false,
                            after: true,
                        },
                        &mut read,
                        &mut write,
                        &guard,
                        &live,
                    )
                    .unwrap();
                    let guard = read.transaction.guard();
                    let result = change_world_id_set_overlay_v1(
                        WorldIdSetChangeV1 {
                            field: Field::SettledRecipeJobs,
                            key: if wrong_key {
                                Key::Text("1")
                            } else {
                                Key::Action(1)
                            },
                            expected: true,
                            after: false,
                        },
                        &mut read,
                        &mut write,
                        &guard,
                        &live,
                    );
                    use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1 as Error;
                    assert!(
                        matches!(result,Err(Error::Value(ValueTreeError::Invalid(reason))) if reason==if wrong_key{"world ID set key type"}else{"world ID set expected value"})
                    );
                }
                let guard = transaction.guard();
                assert!(transaction.seal(&guard, &live, 1).is_err());
            }
            verified
                .record
                .after
                .state
                .iter()
                .find(|root| root.namespace == StateNamespaceV1::Authorization)
                .unwrap()
                .canonical_bytes()
                .unwrap()
        };
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
            for (bytes, added) in [(&root_bytes, false), (&next, true)] {
                let root = StateRootV1::from_canonical_bytes(bytes).unwrap();
                assert_eq!(root.leaf_count, if added { 15 } else { 13 });
                for (field, key, expected) in [
                    (Field::RetiredFactories, Key::Text("factory:一"), added),
                    (Field::SettledRecipeJobs, Key::Action(1), added),
                    (Field::SettledRecipeJobs, Key::Action(0), true),
                ] {
                    let state = store.state_session(physical(), class).unwrap();
                    let values = store.value_session(physical());
                    let mut read = WorldFieldReadContextV1 {
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
                        take_state_error: || {
                            state
                                .take_read_error()
                                .map(|_| ValueTreeError::Invalid("state page failure"))
                        },
                        cancelled: || false,
                    };
                    assert_eq!(
                        world_id_set_contains_v1(field, key, &mut read).unwrap(),
                        expected
                    );
                }
            }
        }
    }
}
