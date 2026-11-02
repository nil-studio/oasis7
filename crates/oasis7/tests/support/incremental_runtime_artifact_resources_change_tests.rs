use super::*;
use oasis7_runtime::incremental_runtime_artifact_resources::{
    RuntimeArtifactBodyChangeV1, RuntimeArtifactResourcesChangeV1, RuntimeBuiltinEntryChangeV1,
    change_runtime_artifact_body_overlay_v1, change_runtime_builtin_entry_overlay_v1,
    read_runtime_artifact_record_reference_v1,
};
use std::cell::Cell;

struct CancelAfterFirstPut<'a, W> {
    inner: &'a mut W,
    writes: &'a Cell<usize>,
    cancelled: &'a Cell<bool>,
}

impl<W: oasis7_formal_protocol::incremental_value::ValueNodeWriter>
    oasis7_formal_protocol::incremental_value::ValueNodeWriter for CancelAfterFirstPut<'_, W>
{
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.inner.put_node(id, bytes)?;
        self.writes.set(self.writes.get() + 1);
        self.cancelled.set(true);
        Ok(())
    }
}

impl<W: oasis7_formal_protocol::incremental_value::ValueNodeReader>
    oasis7_formal_protocol::incremental_value::ValueNodeReader for CancelAfterFirstPut<'_, W>
{
    fn read_node(
        &self,
        id: &[u8; 32],
        max_bytes: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.inner.read_node(id, max_bytes, memory, class)
    }
}

impl<W: oasis7_formal_protocol::incremental_value::ValueNodeWriter>
    oasis7_formal_protocol::incremental_state::StatePageWriter for CancelAfterFirstPut<'_, W>
{
}

struct FailedChange {
    error: oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1,
    aborted: bool,
    value_puts: usize,
    cancelled_after_put: bool,
}

fn failed_change(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    qualification: &ReservedValue<
        oasis7_runtime::incremental_runtime_artifact_resources::RuntimeArtifactResourcesQualificationV1,
    >,
    operation: [u8; 32],
    change: RuntimeArtifactResourcesChangeV1<'_>,
) -> FailedChange {
    let parent = ExecutionParent {
        commit: [operation[0].wrapping_add(1); 32],
        roots: roots.clone(),
        authority: AuthorityBinding {
            token: [0x9c; 32],
            epoch: 4,
        },
    };
    let live = parent.fence().unwrap();
    let state = store
        .state_session(qualification_state_physical(), MemoryClass::Ordinary)
        .unwrap();
    let mut transaction = ExecutionTransaction::begin(
        parent,
        operation,
        &state,
        memory,
        TransactionLimits {
            max_reads: 64,
            max_writes: 8,
            max_facts: 0,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    let guard = transaction.guard();
    let reader = store.value_session(generation_physical());
    let mut reader_io = logical();
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut output = store.value_session(generation_physical());
    let value_puts = Cell::new(0);
    let cancelled = Cell::new(false);
    let mut sink = CancelAfterFirstPut {
        inner: &mut output,
        writes: &value_puts,
        cancelled: &cancelled,
    };
    let mut writer_io = logical();
    let mut writer_key_work = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut work = RuntimeArtifactResourcesWorkV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 4 << 20,
    };
    let result = {
        let mut write = MemberRecordWriteContext {
            sink: &mut sink,
            memory,
            io: &mut writer_io,
            key_work: &mut writer_key_work,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 8 << 20,
                payload_bytes: 16 << 20,
                record_bytes: 32 << 20,
            },
            cancelled: || cancelled.get(),
        };
        let mut context = WorldFieldOverlayContextV1 {
            transaction: &mut transaction,
            values: ValueReadContext {
                source: &reader,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut reader_io,
            },
            key_work: &mut key_work,
            semantic_bytes: 8 << 20,
            record_bytes: 32 << 20,
            take_state_error: || state.take_read_error().map(super::super::typed),
            cancelled: || false,
        };
        match change {
            RuntimeArtifactResourcesChangeV1::DeltaTick(change) => {
                oasis7_runtime::incremental_runtime_artifact_resources::change_runtime_artifact_delta_tick_overlay_v1(
                    change,
                    qualification,
                    &mut context,
                    &mut write,
                    &mut work,
                    &guard,
                    &live,
                )
            }
            RuntimeArtifactResourcesChangeV1::Body(change) => {
                change_runtime_artifact_body_overlay_v1(
                    change,
                    qualification,
                    &mut context,
                    &mut write,
                    &mut work,
                    &guard,
                    &live,
                )
            }
            RuntimeArtifactResourcesChangeV1::BuiltinEntry(change) => {
                change_runtime_builtin_entry_overlay_v1(
                    change,
                    qualification,
                    &mut context,
                    &mut write,
                    &mut work,
                    &guard,
                    &live,
                )
            }
        }
    };
    let error = result.expect_err("failure case unexpectedly prepared a transition");
    let aborted = matches!(
        transaction.seal(&guard, &live, 0),
        Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
    );
    FailedChange {
        error,
        aborted,
        value_puts: value_puts.get(),
        cancelled_after_put: cancelled.get(),
    }
}

fn digest(root: &StateRootV1) -> [u8; 32] {
    *blake3::Hash::from_hex(root.digest().unwrap())
        .expect("state root digest")
        .as_bytes()
}

fn execute_change(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    qualification: &ReservedValue<
        oasis7_runtime::incremental_runtime_artifact_resources::RuntimeArtifactResourcesQualificationV1,
    >,
    operation: [u8; 32],
    change: RuntimeArtifactResourcesChangeV1<'_>,
    work: &mut RuntimeArtifactResourcesWorkV1,
) -> oasis7_runtime::incremental_runtime_artifact_resources::RuntimeArtifactResourcesPreparedTransitionV1
{
    let parent = ExecutionParent {
        commit: [operation[0].wrapping_add(1); 32],
        roots: roots.clone(),
        authority: AuthorityBinding {
            token: [0x9c; 32],
            epoch: 4,
        },
    };
    let live = parent.fence().unwrap();
    let state = store
        .state_session(qualification_state_physical(), MemoryClass::Ordinary)
        .unwrap();
    let mut transaction = ExecutionTransaction::begin(
        parent.clone(),
        operation,
        &state,
        memory,
        TransactionLimits {
            max_reads: 64,
            max_writes: 8,
            max_facts: 0,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    let guard = transaction.guard();
    let reader = store.value_session(generation_physical());
    let mut reader_io = logical();
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut writer = store.value_session(generation_physical());
    let mut writer_io = logical();
    let mut writer_key_work = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    {
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory,
            io: &mut writer_io,
            key_work: &mut writer_key_work,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 8 << 20,
                payload_bytes: 16 << 20,
                record_bytes: 32 << 20,
            },
            cancelled: || false,
        };
        let mut context = WorldFieldOverlayContextV1 {
            transaction: &mut transaction,
            values: ValueReadContext {
                source: &reader,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut reader_io,
            },
            key_work: &mut key_work,
            semantic_bytes: 8 << 20,
            record_bytes: 32 << 20,
            take_state_error: || state.take_read_error().map(super::super::typed),
            cancelled: || false,
        };
        let changed = match change {
            RuntimeArtifactResourcesChangeV1::DeltaTick(change) => {
                change_runtime_artifact_delta_tick_overlay_v1(
                    change,
                    qualification,
                    &mut context,
                    &mut write,
                    work,
                    &guard,
                    &live,
                )
            }
            RuntimeArtifactResourcesChangeV1::Body(change) => {
                change_runtime_artifact_body_overlay_v1(
                    change,
                    qualification,
                    &mut context,
                    &mut write,
                    work,
                    &guard,
                    &live,
                )
            }
            RuntimeArtifactResourcesChangeV1::BuiltinEntry(change) => {
                change_runtime_builtin_entry_overlay_v1(
                    change,
                    qualification,
                    &mut context,
                    &mut write,
                    work,
                    &guard,
                    &live,
                )
            }
        }
        .unwrap_or_else(|error| {
            let state_io = state.remaining_io().unwrap();
            let reader_io = reader.remaining_io().unwrap();
            let writer_io = writer.remaining_io().unwrap();
            eprintln!(
                "Artifact change failed: {error:?}; state reads={} bytes={}; reader reads={} bytes={}; writer reads={} bytes={}; cells={}",
                state_io.reads_remaining,
                state_io.read_bytes_remaining,
                reader_io.reads_remaining,
                reader_io.read_bytes_remaining,
                writer_io.reads_remaining,
                writer_io.read_bytes_remaining,
                work.cells_remaining,
            );
            panic!("Artifact typed change failed")
        });
        assert!(changed);
    }
    let guard = transaction.guard();
    let sealed = transaction.seal(&guard, &live, 0).unwrap();
    assert_eq!(sealed.record.writes.len(), 1);
    assert_eq!(
        sealed.record.writes[0].namespace,
        StateNamespaceV1::Artifacts
    );

    let verifier_state = store
        .state_session(qualification_state_physical(), MemoryClass::Ordinary)
        .unwrap();
    let generic = TransitionVerifier {
        parent: &parent,
        operation,
        source: &verifier_state,
        memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 4 << 20,
    }
    .verify(&sealed.record)
    .unwrap();
    let mutations = generic
        .record
        .writes
        .iter()
        .map(|write| write.mutation.clone())
        .collect();
    let delta_charge = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let state_delta = delta_charge.retain(
        StateDeltaV1::build(
            parent.roots.state_root(StateNamespaceV1::Artifacts).clone(),
            mutations,
            &verifier_state,
            4 << 20,
        )
        .unwrap(),
    );
    assert_eq!(
        state_delta.after,
        *generic.record.after.state_root(StateNamespaceV1::Artifacts)
    );
    writer.put_state_pages(&state_delta, memory).unwrap();
    let after_roots = generic.record.after.clone();

    let receiver_state = store
        .state_session(qualification_state_physical(), MemoryClass::Ordinary)
        .unwrap();
    let receiver_values = store.value_session(generation_physical());
    let mut receiver_io = logical();
    let mut receiver_key_work = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut receiver_context = WorldFieldReadContextV1 {
        root: after_roots.state_root(StateNamespaceV1::Artifacts),
        state: &receiver_state,
        values: ValueReadContext {
            source: &receiver_values,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut receiver_io,
        },
        key_work: &mut receiver_key_work,
        semantic_bytes: 8 << 20,
        record_bytes: 32 << 20,
        take_state_error: || receiver_state.take_read_error().map(super::super::typed),
        cancelled: || false,
    };
    let mut receiver_writer = store.value_session(generation_physical());
    let mut receiver_writer_io = logical();
    let mut receiver_writer_key_work = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut receiver_write = MemberRecordWriteContext {
        sink: &mut receiver_writer,
        memory,
        io: &mut receiver_writer_io,
        key_work: &mut receiver_writer_key_work,
        limits: MemberRecordLimitsV1 {
            semantic_bytes: 8 << 20,
            payload_bytes: 16 << 20,
            record_bytes: 32 << 20,
        },
        cancelled: || false,
    };
    let received = verify_runtime_artifact_resources_change_transition_v1(
        change,
        qualification,
        &TransitionVerifier {
            parent: &parent,
            operation,
            source: &receiver_state,
            memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        },
        &generic.record,
        &mut receiver_context,
        &mut receiver_write,
        work,
        &live,
    )
    .unwrap();
    assert_eq!(received.transition().record, generic.record);
    received
        .qualification()
        .validate_binding(&after_roots.digest().unwrap(), MemoryClass::Ordinary)
        .unwrap();
    received
}

fn builtin_entry_digest(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    root: &StateRootV1,
) -> [u8; 32] {
    let manifest = field_root(
        store,
        memory,
        root,
        RuntimeArtifactFieldV1::BuiltinReleaseManifest,
    )
    .expect("required builtin manifest");
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 4096,
    };
    let module_set = read_runtime_artifact_record_reference_v1(
        RuntimeArtifactRecordV1::BuiltinReleaseManifest,
        RuntimeArtifactRecordV1::BuiltinModuleSet,
        &manifest,
        &("module_sets", "set\0key"),
        &mut WorldFieldReadContextV1 {
            root: manifest.root(),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut key_work,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || state.take_read_error().map(super::super::typed),
            cancelled: || false,
        },
    )
    .unwrap()
    .expect("module set");
    let entry = read_runtime_artifact_record_reference_v1(
        RuntimeArtifactRecordV1::BuiltinModuleSet,
        RuntimeArtifactRecordV1::BuiltinReleaseEntry,
        &module_set,
        &("modules", "module-1"),
        &mut WorldFieldReadContextV1 {
            root: module_set.root(),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut key_work,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || state.take_read_error().map(super::super::typed),
            cancelled: || false,
        },
    )
    .unwrap()
    .expect("builtin entry");
    digest(entry.root())
}

fn builtin_module_set_root(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    root: &StateRootV1,
    module_set: &str,
) -> Option<ReservedValue<RuntimeArtifactChildRootV1>> {
    let manifest = field_root(
        store,
        memory,
        root,
        RuntimeArtifactFieldV1::BuiltinReleaseManifest,
    )
    .expect("required builtin manifest");
    let state = store
        .state_session(qualification_state_physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(generation_physical());
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 4096,
    };
    read_runtime_artifact_record_reference_v1(
        RuntimeArtifactRecordV1::BuiltinReleaseManifest,
        RuntimeArtifactRecordV1::BuiltinModuleSet,
        &manifest,
        &("module_sets", module_set),
        &mut WorldFieldReadContextV1 {
            root: manifest.root(),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut key_work,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || state.take_read_error().map(super::super::typed),
            cancelled: || false,
        },
    )
    .unwrap()
}

fn assert_builtin_hash_token(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    root: &StateRootV1,
    expected: &str,
) {
    let manifest = field_root(
        store,
        memory,
        root,
        RuntimeArtifactFieldV1::BuiltinReleaseManifest,
    )
    .expect("required builtin manifest");
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 4096,
    };
    let module_set = read_runtime_artifact_record_reference_v1(
        RuntimeArtifactRecordV1::BuiltinReleaseManifest,
        RuntimeArtifactRecordV1::BuiltinModuleSet,
        &manifest,
        &("module_sets", "set\0key"),
        &mut WorldFieldReadContextV1 {
            root: manifest.root(),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut key_work,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || state.take_read_error().map(super::super::typed),
            cancelled: || false,
        },
    )
    .unwrap()
    .expect("module set after recreate");
    let entry = read_runtime_artifact_record_reference_v1(
        RuntimeArtifactRecordV1::BuiltinModuleSet,
        RuntimeArtifactRecordV1::BuiltinReleaseEntry,
        &module_set,
        &("modules", "module-1"),
        &mut WorldFieldReadContextV1 {
            root: module_set.root(),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut key_work,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || state.take_read_error().map(super::super::typed),
            cancelled: || false,
        },
    )
    .unwrap()
    .expect("entry after recreate");
    let mut token = Vec::new();
    let read = read_runtime_artifact_member_v1(
        RuntimeArtifactRecordV1::BuiltinReleaseEntry,
        &entry,
        &("hash_tokens", 0u64),
        RuntimeArtifactMemberProfileV1::Text,
        &mut WorldFieldReadContextV1 {
            root: entry.root(),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut key_work,
            semantic_bytes: 1 << 20,
            record_bytes: 2 << 20,
            take_state_error: || state.take_read_error().map(super::super::typed),
            cancelled: || false,
        },
        1024,
        &mut token,
    )
    .unwrap()
    .expect("hash token");
    assert_eq!(
        read,
        oasis7_runtime::incremental_runtime_artifact_resources::RuntimeArtifactValueV1::Text {
            utf8_bytes: expected.len() as u64
        }
    );
    assert_eq!(token, expected.as_bytes());
}

#[test]
fn latest_delta_body_none_some_changes_replay_and_cold_qualify() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let (manifest, initial_delta, builtins) = rich_sources();
        let builtin_source = BuiltinSource(&builtins);
        let source = RuntimeArtifactResourcesRefV1 {
            chain_resource_manifest: &manifest,
            latest_chain_resource_delta: Some(&initial_delta),
            builtin_release_manifest: &builtin_source,
        };
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let (canonical, _producer_memory) = persist_sources(&path, compression, &source);
        let memory = MemoryBudget::new(64 << 20, 16 << 20).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let roots = bounded_root_set(StateRootV1::from_canonical_bytes(&canonical).unwrap());
        let qualified = qualify(&store, &memory, &roots).unwrap();
        let initial_child = field_root(
            &store,
            &memory,
            roots.state_root(StateNamespaceV1::Artifacts),
            RuntimeArtifactFieldV1::LatestChainResourceDelta,
        )
        .expect("initial Some delta");
        let initial_child_digest = digest(initial_child.root());
        let mut work = RuntimeArtifactResourcesWorkV1 {
            cells_remaining: 100_000,
            max_pending_node_bytes: 4 << 20,
        };
        let removed = execute_change(
            &store,
            &memory,
            &roots,
            &qualified,
            [0x81; 32],
            RuntimeArtifactResourcesChangeV1::Body(
                RuntimeArtifactBodyChangeV1::ReplaceLatestDelta {
                    expected_root: Some(initial_child_digest),
                    after: None,
                },
            ),
            &mut work,
        );
        assert_eq!(removed.qualification().record_counts(), [1, 0, 1]);
        let removed_roots = removed.transition().record.after.clone();
        assert!(
            field_root(
                &store,
                &memory,
                removed_roots.state_root(StateNamespaceV1::Artifacts),
                RuntimeArtifactFieldV1::LatestChainResourceDelta,
            )
            .is_none()
        );

        let mut replacement = initial_delta.clone();
        replacement.tick = replacement.tick.checked_add(11).unwrap();
        replacement.delta_id = "resource-delta-after-none".into();
        let inserted = execute_change(
            &store,
            &memory,
            &removed_roots,
            removed.qualification(),
            [0x82; 32],
            RuntimeArtifactResourcesChangeV1::Body(
                RuntimeArtifactBodyChangeV1::ReplaceLatestDelta {
                    expected_root: None,
                    after: Some(&replacement),
                },
            ),
            &mut work,
        );
        assert_eq!(inserted.qualification().record_counts(), [1, 1, 1]);
        let after_roots = inserted.transition().record.after.clone();
        let after_digest = after_roots.digest().unwrap();
        drop(inserted);
        drop(removed);
        drop(qualified);
        drop(store);

        let cold_memory = MemoryBudget::new(16 << 20, 4 << 20).unwrap();
        let cold_store = IncrementalPageStore::open(
            &path,
            cold_memory.clone(),
            compression,
            MemoryClass::Ordinary,
        )
        .unwrap();
        let cold_qualification = qualify(&cold_store, &cold_memory, &after_roots).unwrap();
        cold_qualification
            .validate_binding(&after_digest, MemoryClass::Ordinary)
            .unwrap();
        assert_eq!(cold_qualification.record_counts(), [1, 1, 1]);
        let delta_root = field_root(
            &cold_store,
            &cold_memory,
            after_roots.state_root(StateNamespaceV1::Artifacts),
            RuntimeArtifactFieldV1::LatestChainResourceDelta,
        )
        .expect("recreated Some delta");
        assert_eq!(
            unsigned_cell(
                &cold_store,
                &cold_memory,
                &delta_root,
                RuntimeArtifactRecordV1::ChainResourceDelta,
                "tick",
            ),
            RuntimeArtifactValueV1::Unsigned(replacement.tick)
        );
    }
}

#[test]
fn builtin_last_entry_remove_prunes_set_and_create_rebuilds_it_cold() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let (manifest, delta, builtins) = rich_sources();
        let builtin_source = BuiltinSource(&builtins);
        let source = RuntimeArtifactResourcesRefV1 {
            chain_resource_manifest: &manifest,
            latest_chain_resource_delta: Some(&delta),
            builtin_release_manifest: &builtin_source,
        };
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let (canonical, _producer_memory) = persist_sources(&path, compression, &source);
        let memory = MemoryBudget::new(64 << 20, 16 << 20).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let roots = bounded_root_set(StateRootV1::from_canonical_bytes(&canonical).unwrap());
        let qualified = qualify(&store, &memory, &roots).unwrap();
        let entry_digest = builtin_entry_digest(
            &store,
            &memory,
            roots.state_root(StateNamespaceV1::Artifacts),
        );
        let mut work = RuntimeArtifactResourcesWorkV1 {
            cells_remaining: 100_000,
            max_pending_node_bytes: 4 << 20,
        };
        let removed = execute_change(
            &store,
            &memory,
            &roots,
            &qualified,
            [0x91; 32],
            RuntimeArtifactResourcesChangeV1::BuiltinEntry(RuntimeBuiltinEntryChangeV1 {
                module_set: "set\0key",
                module_id: "module-1",
                expected_entry_root: Some(entry_digest),
                after: None,
            }),
            &mut work,
        );
        assert_eq!(removed.qualification().record_counts(), [1, 1, 1]);
        let removed_roots = removed.transition().record.after.clone();
        assert!(
            builtin_module_set_root(
                &store,
                &memory,
                removed_roots.state_root(StateNamespaceV1::Artifacts),
                "set\0key",
            )
            .is_none(),
            "removing the last module must prune its empty module set"
        );

        let new_tokens = vec![String::from("fresh-token"), String::new()];
        let new_identities = BTreeMap::from([(
            String::from("artifact\0new"),
            oasis7_runtime::ModuleArtifactIdentity {
                source_hash: "source-new".into(),
                build_manifest_hash: "build-new".into(),
                signer_node_id: "signer-new".into(),
                signature_scheme: "scheme-new".into(),
                artifact_signature: "signature-new".into(),
            },
        )]);
        let inserted = execute_change(
            &store,
            &memory,
            &removed_roots,
            removed.qualification(),
            [0x92; 32],
            RuntimeArtifactResourcesChangeV1::BuiltinEntry(RuntimeBuiltinEntryChangeV1 {
                module_set: "set\0key",
                module_id: "module-1",
                expected_entry_root: None,
                after: Some(BuiltinReleaseEntryRefV1 {
                    hash_tokens: &new_tokens,
                    artifact_identities: &new_identities,
                }),
            }),
            &mut work,
        );
        assert_eq!(inserted.qualification().record_counts(), [1, 1, 1]);
        let after_roots = inserted.transition().record.after.clone();
        let after_digest = after_roots.digest().unwrap();
        drop(inserted);
        drop(removed);
        drop(qualified);
        drop(store);

        let cold_memory = MemoryBudget::new(16 << 20, 4 << 20).unwrap();
        let cold_store = IncrementalPageStore::open(
            &path,
            cold_memory.clone(),
            compression,
            MemoryClass::Ordinary,
        )
        .unwrap();
        let cold_qualification = qualify(&cold_store, &cold_memory, &after_roots).unwrap();
        cold_qualification
            .validate_binding(&after_digest, MemoryClass::Ordinary)
            .unwrap();
        assert_builtin_hash_token(
            &cold_store,
            &cold_memory,
            after_roots.state_root(StateNamespaceV1::Artifacts),
            "fresh-token",
        );
    }
}

#[test]
fn body_and_builtin_failures_poison_after_expected_mismatch_and_real_value_put() {
    use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayErrorV1 as Error;

    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let (manifest, delta, builtins) = rich_sources();
        let builtin_source = BuiltinSource(&builtins);
        let source = RuntimeArtifactResourcesRefV1 {
            chain_resource_manifest: &manifest,
            latest_chain_resource_delta: Some(&delta),
            builtin_release_manifest: &builtin_source,
        };
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let (canonical, _producer_memory) = persist_sources(&path, compression, &source);
        let memory = MemoryBudget::new(64 << 20, 16 << 20).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let roots = bounded_root_set(StateRootV1::from_canonical_bytes(&canonical).unwrap());
        let qualification = qualify(&store, &memory, &roots).unwrap();
        let delta_root = field_root(
            &store,
            &memory,
            roots.state_root(StateNamespaceV1::Artifacts),
            RuntimeArtifactFieldV1::LatestChainResourceDelta,
        )
        .expect("required delta");
        let delta_digest = digest(delta_root.root());
        let mut replacement_delta = delta.clone();
        replacement_delta.tick = replacement_delta.tick.checked_add(17).unwrap();
        replacement_delta.delta_id = "failure-matrix-delta".into();

        let mismatch_body = failed_change(
            &store,
            &memory,
            &roots,
            &qualification,
            [0xa1; 32],
            RuntimeArtifactResourcesChangeV1::Body(
                RuntimeArtifactBodyChangeV1::ReplaceLatestDelta {
                    expected_root: Some([0xee; 32]),
                    after: Some(&replacement_delta),
                },
            ),
        );
        assert!(matches!(
            mismatch_body.error,
            Error::Value(ValueTreeError::Invalid(_))
        ));
        assert_eq!(mismatch_body.value_puts, 0);
        assert!(!mismatch_body.cancelled_after_put);
        assert!(mismatch_body.aborted);

        let cancelled_body = failed_change(
            &store,
            &memory,
            &roots,
            &qualification,
            [0xa2; 32],
            RuntimeArtifactResourcesChangeV1::Body(
                RuntimeArtifactBodyChangeV1::ReplaceLatestDelta {
                    expected_root: Some(delta_digest),
                    after: Some(&replacement_delta),
                },
            ),
        );
        assert!(matches!(
            cancelled_body.error,
            Error::Value(ValueTreeError::Cancelled)
                | Error::Member(MemberRecordError::Value(ValueTreeError::Cancelled))
        ));
        assert!(cancelled_body.value_puts > 0);
        assert!(cancelled_body.cancelled_after_put);
        assert!(cancelled_body.aborted);

        let replacement_tokens = [String::from("replacement-token")];
        let replacement_identities = BTreeMap::from([(
            String::from("replacement-artifact"),
            oasis7_runtime::ModuleArtifactIdentity {
                source_hash: "replacement-source".into(),
                build_manifest_hash: "replacement-build".into(),
                signer_node_id: "replacement-signer".into(),
                signature_scheme: "replacement-scheme".into(),
                artifact_signature: "replacement-signature".into(),
            },
        )]);
        let replacement_entry =
            oasis7_runtime::incremental_runtime_artifact_resources::BuiltinReleaseEntryRefV1 {
                hash_tokens: &replacement_tokens,
                artifact_identities: &replacement_identities,
            };
        let actual_entry_digest = builtin_entry_digest(
            &store,
            &memory,
            roots.state_root(StateNamespaceV1::Artifacts),
        );
        let mismatch_builtin = failed_change(
            &store,
            &memory,
            &roots,
            &qualification,
            [0xa3; 32],
            RuntimeArtifactResourcesChangeV1::BuiltinEntry(RuntimeBuiltinEntryChangeV1 {
                module_set: "set\0key",
                module_id: "module-1",
                expected_entry_root: Some([0xef; 32]),
                after: Some(replacement_entry),
            }),
        );
        assert!(matches!(
            mismatch_builtin.error,
            Error::Value(ValueTreeError::Invalid(_))
        ));
        assert_eq!(mismatch_builtin.value_puts, 0);
        assert!(!mismatch_builtin.cancelled_after_put);
        assert!(mismatch_builtin.aborted);

        let cancelled_builtin = failed_change(
            &store,
            &memory,
            &roots,
            &qualification,
            [0xa4; 32],
            RuntimeArtifactResourcesChangeV1::BuiltinEntry(RuntimeBuiltinEntryChangeV1 {
                module_set: "set\0key",
                module_id: "module-1",
                expected_entry_root: Some(actual_entry_digest),
                after: Some(replacement_entry),
            }),
        );
        assert!(matches!(
            cancelled_builtin.error,
            Error::Value(ValueTreeError::Cancelled)
                | Error::Member(MemberRecordError::Value(ValueTreeError::Cancelled))
        ));
        assert!(cancelled_builtin.value_puts > 0);
        assert!(cancelled_builtin.cancelled_after_put);
        assert!(cancelled_builtin.aborted);
    }
}
