use super::*;
use oasis7_runtime::incremental_runtime_artifact_resources::{
    RuntimeArtifactBodyChangeV1, RuntimeArtifactResourcesChangeV1, RuntimeBuiltinEntryChangeV1,
    change_runtime_artifact_body_overlay_v1, change_runtime_builtin_entry_overlay_v1,
    read_runtime_artifact_record_reference_v1,
};

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
        .state_session(physical(), MemoryClass::Ordinary)
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
    let reader = store.value_session(physical());
    let mut reader_io = logical();
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut writer = store.value_session(physical());
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
        .unwrap();
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
        .state_session(physical(), MemoryClass::Ordinary)
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
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let receiver_values = store.value_session(physical());
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
    let mut receiver_writer = store.value_session(physical());
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
        .validate_binding(
            &digest(after_roots.state_root(StateNamespaceV1::Artifacts)),
            MemoryClass::Ordinary,
        )
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
        let after_digest = digest(after_roots.state_root(StateNamespaceV1::Artifacts));
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
        let after_digest = digest(after_roots.state_root(StateNamespaceV1::Artifacts));
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
