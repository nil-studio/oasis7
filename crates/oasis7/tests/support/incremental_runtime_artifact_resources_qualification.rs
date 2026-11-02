use super::*;
use oasis7_agent_api::{
    world_action::{MaterialLedgerId, ResourceKind},
    world_geometry::ChunkCoord,
    world_resource::{
        ChainChunkResourceManifestEntry, ChainChunkResourceStatus, ChainFragmentResourceRef,
        ChainResourceCommitRef, ChainResourceDelta, ChainResourceDeltaEntry,
        ChainResourceDeltaSource, ChainResourceManifest, ChainResourceOrderingKey,
        ChainResourceReplayStatus, FragmentElementKind,
    },
};
use oasis7_distfs::incremental_page_store::PageIoBudget;
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, RootSetV1, TransactionLimits,
        TransitionVerifier,
    },
    incremental_state::{StateScanBudget, StateWriteV1},
};
use oasis7_runtime::BuiltinReleaseManifestEntry;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
use oasis7_runtime::incremental_runtime_artifact_resources::{
    BuiltinReleaseEntryRefV1, BuiltinReleaseManifestSourceV1, BuiltinReleaseModuleSetSourceV1,
    RuntimeArtifactDeltaTickChangeV1, RuntimeArtifactResourcesChangeV1,
    RuntimeArtifactResourcesReadContextV1, change_runtime_artifact_delta_tick_overlay_v1,
    qualify_runtime_artifact_resources_v1, verify_runtime_artifact_resources_change_transition_v1,
};
use oasis7_runtime::incremental_runtime_controls::{
    RuntimeControlsRefV1, prepare_runtime_controls_genesis_v1,
};
use std::collections::BTreeMap;

const QUALIFICATION_STATE_READS: u64 = 1_000_000;

fn generation_physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 1_000_000,
        read_bytes_remaining: 64 << 20,
        writes_remaining: 1_000_000,
        write_bytes_remaining: 256 << 20,
    }
}

struct ModuleSetSource<'a>(&'a BTreeMap<String, BuiltinReleaseManifestEntry>);
impl BuiltinReleaseModuleSetSourceV1 for ModuleSetSource<'_> {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn for_each_module<'a, E, F>(&'a self, mut visit: F) -> Result<(), E>
    where
        F: FnMut(&'a str, BuiltinReleaseEntryRefV1<'a>) -> Result<(), E>,
    {
        for (module, entry) in self.0 {
            visit(
                module,
                BuiltinReleaseEntryRefV1 {
                    hash_tokens: &entry.hash_tokens,
                    artifact_identities: &entry.artifact_identities,
                },
            )?;
        }
        Ok(())
    }
}

struct BuiltinSource<'a>(&'a BTreeMap<String, BTreeMap<String, BuiltinReleaseManifestEntry>>);
impl BuiltinReleaseManifestSourceV1 for BuiltinSource<'_> {
    type ModuleSet<'a>
        = ModuleSetSource<'a>
    where
        Self: 'a;

    fn len(&self) -> usize {
        self.0.len()
    }

    fn for_each_module_set<'a, E, F>(&'a self, mut visit: F) -> Result<(), E>
    where
        F: FnMut(&'a str, Self::ModuleSet<'a>) -> Result<(), E>,
    {
        for (set, modules) in self.0 {
            visit(set, ModuleSetSource(modules))?;
        }
        Ok(())
    }
}

fn bounded_root_set(root: StateRootV1) -> RootSetV1 {
    let mut roots = RootSetV1::empty("artifact-family-test".into(), [0x39; 32]).unwrap();
    assert_eq!(root.namespace, StateNamespaceV1::Artifacts);
    roots.state[5] = root;
    roots
}

fn qualification_state_physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: QUALIFICATION_STATE_READS,
        ..physical()
    }
}

fn qualify(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
) -> Result<ReservedValue<oasis7_runtime::incremental_runtime_artifact_resources::RuntimeArtifactResourcesQualificationV1>, ValueTreeError>{
    let state = store
        .state_session(qualification_state_physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut value_io = logical();
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let mut scans = std::array::from_fn(|_| StateScanBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 100_000,
    });
    let mut work = RuntimeArtifactResourcesWorkV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 2 << 20,
    };
    let result = qualify_runtime_artifact_resources_v1(
        roots,
        &mut RuntimeArtifactResourcesReadContextV1 {
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut value_io,
            },
            key_work: &mut key_work,
            semantic_bytes: 8 << 20,
            record_bytes: 16 << 20,
            max_payload_bytes: 8 << 20,
            scans: &mut scans,
            take_state_error: || state.take_read_error().map(super::typed),
            cancelled: || false,
        },
        &mut work,
    );
    if result.is_err() {
        let state_remaining = state.remaining_io().unwrap();
        let value_remaining = values.remaining_io().unwrap();
        eprintln!(
            "Artifact qualification failed: state remaining reads={} bytes={} writes={} write_bytes={}; value remaining reads={} bytes={} writes={} write_bytes={}; scan remaining={:?}; cells remaining={}",
            state_remaining.reads_remaining,
            state_remaining.read_bytes_remaining,
            state_remaining.writes_remaining,
            state_remaining.write_bytes_remaining,
            value_remaining.reads_remaining,
            value_remaining.read_bytes_remaining,
            value_remaining.writes_remaining,
            value_remaining.write_bytes_remaining,
            scans.map(|scan| (
                scan.nodes_remaining,
                scan.read_bytes_remaining,
                scan.leaves_remaining
            )),
            work.cells_remaining,
        );
    } else {
        let state_remaining = state.remaining_io().unwrap();
        let value_remaining = values.remaining_io().unwrap();
        eprintln!(
            "Artifact qualification consumed state {} reads / {} bytes and value {} reads / {} bytes; cells {}",
            QUALIFICATION_STATE_READS - state_remaining.reads_remaining,
            (256 << 20) - state_remaining.read_bytes_remaining,
            100_000 - value_remaining.reads_remaining,
            (256 << 20) - value_remaining.read_bytes_remaining,
            100_000 - work.cells_remaining,
        );
    }
    result
}

fn install_governance_control_roots(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &mut RootSetV1,
) {
    let mut values = store.value_session(generation_physical());
    let row_capacity = 6;
    let rows_charge = memory
        .reserve(
            row_capacity * std::mem::size_of::<ReservedValue<ExecutionWriteV1>>(),
            MemoryClass::Ordinary,
        )
        .unwrap();
    let mut rows = Vec::with_capacity(row_capacity);
    let mut records_remaining = row_capacity as u64;
    {
        let mut context = MemberRecordWriteContext {
            sink: &mut values,
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        };
        prepare_runtime_controls_genesis_v1(
            &RuntimeControlsRefV1 {
                tick_consensus_authority_source: "authority.resources-qualification",
                next_governance_identity_penalty_id: 17,
                governance_emergency_brake_until_tick: Some(0),
            },
            &mut context,
            &mut records_remaining,
            &mut |row| {
                rows.push(row);
                Ok(())
            },
        )
        .unwrap();
    }
    assert_eq!(records_remaining, 0);
    assert_eq!(rows.len(), row_capacity);
    let _rows_charge = rows_charge;

    for namespace in [StateNamespaceV1::Allocator, StateNamespaceV1::Authorization] {
        let mut writes: Vec<_> = rows
            .iter()
            .filter(|row| row.namespace == namespace)
            .map(|row| row.mutation.clone())
            .collect();
        writes.sort_by(|left, right| left.key.cmp(&right.key));
        assert!(writes.windows(2).all(|pair| pair[0].key != pair[1].key));
        assert!(!writes.is_empty());
        let state = store
            .state_session(generation_physical(), MemoryClass::Ordinary)
            .unwrap();
        let delta_charge = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
        let delta = delta_charge.retain(
            StateDeltaV1::build(
                StateRootV1::empty(namespace).unwrap(),
                writes,
                &state,
                1 << 20,
            )
            .unwrap(),
        );
        values.put_state_pages(&delta, memory).unwrap();
        *roots
            .state
            .iter_mut()
            .find(|root| root.namespace == namespace)
            .unwrap() = delta.after.clone();
    }
}

fn install_wrong_namespace_artifact_alias(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &mut RootSetV1,
    namespace: StateNamespaceV1,
) {
    let field = FieldDescriptor {
        owner: FieldOwner::Runtime,
        name: RuntimeArtifactFieldV1::ChainResourceManifest.name(),
        destination: FieldDestination::State(namespace),
        shape: FieldShape::Value,
    };
    let mut values = store.value_session(generation_physical());
    let mut io = logical();
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 4096,
    };
    let metadata = write_field_metadata_v1(
        field,
        &mut PresenceRecordWriteContext {
            sink: &mut values,
            memory,
            io: &mut io,
            key_work: &mut key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: 4096,
                record_bytes: 8192,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    let entry = write_member_record_v1(
        field,
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::Unit,
        &mut MemberRecordWriteContext {
            sink: &mut values,
            memory,
            io: &mut io,
            key_work: &mut key_work,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 0,
                record_bytes: 8192,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    assert!(values.remaining_io().unwrap().writes_remaining < 1_000_000);

    let namespace_root = roots
        .state
        .iter()
        .find(|root| root.namespace == namespace)
        .unwrap()
        .clone();
    let replacement = persist_delta(
        store,
        memory,
        namespace_root,
        vec![
            StateWriteV1 {
                key: metadata.field_key.as_bytes().to_vec(),
                before: None,
                after: Some(metadata.value_root),
            },
            StateWriteV1 {
                key: entry.field_key.as_bytes().to_vec(),
                before: None,
                after: Some(entry.value_root),
            },
        ],
    );
    let auth_root = roots
        .state
        .iter_mut()
        .find(|root| root.namespace == namespace)
        .unwrap();
    *auth_root = replacement;

    // Confirm this is a generic-valid field-member pair before asking the
    // Artifact owner to reject its cross-namespace registration.
    let state = store
        .state_session(generation_physical(), MemoryClass::Ordinary)
        .unwrap();
    let namespace_root = roots.state_root(namespace);
    let metadata_value = namespace_root
        .get(metadata.field_key.as_bytes(), &state)
        .unwrap()
        .unwrap();
    let entry_value = namespace_root
        .get(entry.field_key.as_bytes(), &state)
        .unwrap()
        .unwrap();
    let reader = store.value_session(physical());
    let mut read_io = logical();
    let mut context = ValueReadContext {
        source: &reader,
        memory,
        class: MemoryClass::Ordinary,
        io: &mut read_io,
    };
    oasis7_runtime::incremental_field_presence::read_field_presence_v1(
        &metadata_value,
        &metadata.field_key,
        &mut context,
        PresenceRecordLimitsV1 {
            semantic_bytes: 4096,
            record_bytes: 8192,
        },
        &mut std::io::sink(),
        || false,
    )
    .unwrap();
    let info = read_member_record_profile_v1(
        &entry_value,
        &entry.field_key,
        &mut context,
        MemberReadProfileV1 {
            codec: MemberPayloadCodecV1::Unit,
            semantic_bytes: entry.info.semantic_bytes,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 0,
                record_bytes: 8192,
            },
        },
        &mut std::io::sink(),
        &mut std::io::sink(),
        || false,
    )
    .unwrap();
    assert_eq!(info.codec, MemberPayloadCodecV1::Unit);
    assert_eq!(info.payload_bytes, 0);
    assert_eq!(
        WorldFieldKeyV1::from_bytes(entry.field_key.as_bytes())
            .unwrap()
            .namespace,
        namespace
    );
}

fn persist_sources<B: BuiltinReleaseManifestSourceV1>(
    path: &std::path::Path,
    compression: PageCompression,
    source: &RuntimeArtifactResourcesRefV1<'_, B>,
) -> (
    [u8; oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES],
    MemoryBudget,
) {
    let memory = MemoryBudget::new(64 << 20, 16 << 20).unwrap();
    let store = IncrementalPageStore::create(path, memory.clone(), compression).unwrap();
    let mut values = store.value_session(generation_physical());
    let row_capacity = 32_768;
    let _rows_charge = memory
        .reserve(
            row_capacity * std::mem::size_of::<ReservedValue<ExecutionWriteV1>>(),
            MemoryClass::Ordinary,
        )
        .unwrap();
    let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(row_capacity);
    let mut work = RuntimeArtifactResourcesWorkV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 2 << 20,
    };
    let emitted = {
        let mut context = MemberRecordWriteContext {
            sink: &mut values,
            memory: &memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 8 << 20,
                record_bytes: 16 << 20,
            },
            cancelled: || false,
        };
        let mut sink = |write| {
            assert!(rows.len() < row_capacity);
            rows.push(write);
            Ok(())
        };
        match prepare_runtime_artifact_resources_genesis_v1(
            source,
            &mut context,
            &mut work,
            &mut sink,
        ) {
            Ok(emitted) => emitted,
            Err(error) => panic!(
                "artifact source fixture failed with {:?}; remaining page IO: {:?}",
                error,
                values.remaining_io()
            ),
        }
    };
    assert!(emitted > 0);
    let value_generation_io = values.remaining_io().unwrap();
    eprintln!(
        "Artifact GEN value session consumed {} reads / {} bytes and {} writes / {} bytes",
        1_000_000 - value_generation_io.reads_remaining,
        (64 << 20) - value_generation_io.read_bytes_remaining,
        1_000_000 - value_generation_io.writes_remaining,
        (256 << 20) - value_generation_io.write_bytes_remaining,
    );
    let state = store
        .state_session(generation_physical(), MemoryClass::Ordinary)
        .unwrap();
    let mut writes: Vec<_> = rows
        .iter()
        .map(|row| StateWriteV1 {
            key: row.mutation.key.clone(),
            before: row.mutation.before,
            after: row.mutation.after,
        })
        .collect();
    writes.sort_by(|left, right| left.key.cmp(&right.key));
    let delta_space = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let delta = delta_space.retain(
        StateDeltaV1::build(
            StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap(),
            writes,
            &state,
            2 << 20,
        )
        .unwrap(),
    );
    assert_eq!(delta.after.leaf_count, rows.len() as u64);
    values.put_state_pages(&delta, &memory).unwrap();
    let state_generation_io = state.remaining_io().unwrap();
    eprintln!(
        "Artifact GEN state build consumed {} reads / {} bytes",
        1_000_000 - state_generation_io.reads_remaining,
        (64 << 20) - state_generation_io.read_bytes_remaining,
    );
    (
        delta.after.canonical_bytes().unwrap().try_into().unwrap(),
        memory,
    )
}

fn persist_delta(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    before: StateRootV1,
    mut writes: Vec<StateWriteV1>,
) -> StateRootV1 {
    writes.sort_by(|left, right| left.key.cmp(&right.key));
    assert!(writes.windows(2).all(|pair| pair[0].key != pair[1].key));
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let workspace = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let delta = workspace.retain(StateDeltaV1::build(before, writes, &state, 2 << 20).unwrap());
    let mut values = store.value_session(physical());
    values.put_state_pages(&delta, memory).unwrap();
    delta.after.clone()
}

fn persist_manifest_child(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    top_root: StateRootV1,
    manifest_root: StateRootV1,
) -> StateRootV1 {
    let field = RuntimeArtifactFieldV1::ChainResourceManifest;
    let reference =
        RecordRootRefV1::new(field.schema(), unit_semantic_digest(), manifest_root).unwrap();
    let replacement = write_member_record_v1(
        field.descriptor(),
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::RecordRoot(&reference),
        &mut MemberRecordWriteContext {
            sink: &mut store.value_session(physical()),
            memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 4096,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1024,
                payload_bytes: 4096,
                record_bytes: 8192,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    let before = top_root
        .get(
            replacement.field_key.as_bytes(),
            &store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap(),
        )
        .unwrap()
        .expect("existing manifest root reference");
    persist_delta(
        store,
        memory,
        top_root,
        vec![StateWriteV1 {
            key: replacement.field_key.as_bytes().to_vec(),
            before: Some(before),
            after: Some(replacement.value_root),
        }],
    )
}

fn rich_sources() -> (
    ChainResourceManifest,
    ChainResourceDelta,
    BTreeMap<String, BTreeMap<String, BuiltinReleaseManifestEntry>>,
) {
    let mut manifest = ChainResourceManifest::empty_at_height("artifact-world", 17, 4);
    manifest.genesis_ref = Some("root\0λ".into());
    manifest.created_at_block_hash = Some("block-4".into());
    manifest
        .resource_balances
        .insert(ResourceKind::Electricity, -8);
    manifest.material_ledgers.insert(
        MaterialLedgerId::agent("agent-7"),
        [(String::from("ore\0λ"), -13)].into_iter().collect(),
    );
    manifest
        .material_ledgers
        .insert(MaterialLedgerId::site("empty-site"), BTreeMap::new());
    manifest.generated_chunks.insert(
        "chunk:2:-1:3".into(),
        ChainChunkResourceManifestEntry {
            schema_version: "chunk-schema".into(),
            world_id: "artifact-world".into(),
            chain_id: "chain-4".into(),
            world_seed: 17,
            chunk_generation_schema_version: "chunk-gen".into(),
            coord: ChunkCoord { x: 2, y: -1, z: 3 },
            seed: 19,
            chunk_status: ChainChunkResourceStatus::Exhausted,
            fragment_count: 1,
            block_count: 2,
            fragment_refs: vec![ChainFragmentResourceRef {
                fragment_id: "fragment".into(),
                location_id: "location".into(),
                profile_hash: "profile".into(),
                budget_total_hash: "total".into(),
                budget_remaining_hash: "remaining".into(),
            }],
            chunk_budget_total_hash: "chunk-total".into(),
            chunk_budget_remaining_hash: "chunk-left".into(),
            total_by_element_g: [(FragmentElementKind::Oxygen, 11)].into_iter().collect(),
            remaining_by_element_g: [(FragmentElementKind::Oxygen, 5)].into_iter().collect(),
            commit_ref: ChainResourceCommitRef {
                height: 4,
                block_hash: Some("chunk-block".into()),
                event_id: Some(8),
                action_id: Some(9),
            },
            manifest_hash: String::new(),
        },
    );
    manifest.refresh_hashes();

    let mut delta = ChainResourceDelta::empty_for_manifest("artifact-world", "base", 5, 12);
    delta.chain_id = "chain-4".into();
    delta.action_id = Some(21);
    delta.event_id = Some(22);
    delta.commit_block_hash = Some("delta-block".into());
    delta.source = ChainResourceDeltaSource::Replenish;
    delta.replay_status = ChainResourceReplayStatus::Rejected;
    delta.ordering_key = ChainResourceOrderingKey {
        height: 5,
        event_sequence: 7,
        action_sequence: 3,
    };
    delta.entries = vec![
        ChainResourceDeltaEntry::RuntimeResource {
            kind: ResourceKind::Data,
            delta: 6,
            resulting_balance: 18,
        },
        ChainResourceDeltaEntry::MaterialLedger {
            ledger_id: MaterialLedgerId::factory("factory-4"),
            material_kind: "catalyst".into(),
            delta: -2,
            resulting_balance: 3,
        },
        ChainResourceDeltaEntry::ChunkResource {
            coord: ChunkCoord { x: -2, y: 0, z: 1 },
            element: FragmentElementKind::Iron,
            total_delta_g: -3,
            remaining_delta_g: -1,
            resulting_remaining_g: 4,
            remaining_after_hash: "remaining-hash".into(),
            chunk_remaining_after_hash: "chunk-hash".into(),
        },
    ];

    let identity = oasis7_runtime::ModuleArtifactIdentity {
        source_hash: "source".into(),
        build_manifest_hash: "build".into(),
        signer_node_id: "signer".into(),
        signature_scheme: "scheme".into(),
        artifact_signature: "signature".into(),
    };
    let entry = BuiltinReleaseManifestEntry {
        hash_tokens: vec!["hash-a".into(), String::new(), "hash-c".into()],
        artifact_identities: [("artifact\0key".into(), identity)].into_iter().collect(),
    };
    let builtins = [(
        "set\0key".into(),
        [("module-1".into(), entry)].into_iter().collect(),
    )]
    .into_iter()
    .collect();
    (manifest, delta, builtins)
}

#[test]
fn artifact_qualification_rejects_generic_valid_same_name_in_wrong_root_namespace() {
    let directory = Directory::new();
    let path = directory.0.join("pages");
    let (canonical, _producer_memory) = roots(&path, PageCompression::Raw, None);
    let memory = MemoryBudget::new(64 << 20, 16 << 20).unwrap();
    let store = IncrementalPageStore::open(
        &path,
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Ordinary,
    )
    .unwrap();
    let mut base_roots = bounded_root_set(StateRootV1::from_canonical_bytes(&canonical).unwrap());
    install_governance_control_roots(&store, &memory, &mut base_roots);
    for namespace in [StateNamespaceV1::Authorization, StateNamespaceV1::Allocator] {
        let mut root_set = base_roots.clone();
        install_wrong_namespace_artifact_alias(&store, &memory, &mut root_set, namespace);
        assert!(matches!(
            qualify(&store, &memory, &root_set),
            Err(ValueTreeError::Invalid(_))
        ));
    }
}

#[test]
fn raw_and_zstd_family_qualification_closes_every_runtime_artifact_record() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let (manifest, delta, builtin_entries) = rich_sources();
        let builtin = BuiltinSource(&builtin_entries);
        let source = RuntimeArtifactResourcesRefV1 {
            chain_resource_manifest: &manifest,
            latest_chain_resource_delta: Some(&delta),
            builtin_release_manifest: &builtin,
        };
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let (canonical, _producer_memory) = persist_sources(&path, compression, &source);
        let memory = MemoryBudget::new(64 << 20, 16 << 20).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let artifact_root = StateRootV1::from_canonical_bytes(&canonical).unwrap();
        let mut roots = bounded_root_set(artifact_root);
        install_governance_control_roots(&store, &memory, &mut roots);
        let proof = qualify(&store, &memory, &roots).expect("complete typed family");
        assert_eq!(proof.record_counts(), [1, 1, 1]);
        let digest = roots.digest().unwrap();
        assert!(proof.uses_budget(&memory));
        proof
            .validate_binding(&digest, MemoryClass::Ordinary)
            .unwrap();
        assert!(
            proof
                .validate_binding(&digest, MemoryClass::Recovery)
                .is_err()
        );
        let other_memory = MemoryBudget::new(64 << 20, 16 << 20).unwrap();
        assert!(!proof.uses_budget(&other_memory));
        let mut other_roots = roots.clone();
        other_roots
            .state
            .iter_mut()
            .find(|root| root.namespace == StateNamespaceV1::Authorization)
            .unwrap()
            .clone_from(&StateRootV1::empty(StateNamespaceV1::Authorization).unwrap());
        assert!(
            proof
                .validate_binding(&other_roots.digest().unwrap(), MemoryClass::Ordinary)
                .is_err()
        );
    }
}

#[test]
fn full_qualification_rejects_orphan_count_and_wrong_owner_child_binding() {
    let directory = Directory::new();
    let path = directory.0.join("pages");
    let mut manifest = ChainResourceManifest::empty_at_height("artifact-world", 17, 4);
    let expected = MaterialLedgerId::agent("expected");
    let substitute = MaterialLedgerId::agent("substitute");
    manifest.material_ledgers.insert(
        expected.clone(),
        [(String::from("ore"), 1)].into_iter().collect(),
    );
    manifest.material_ledgers.insert(
        substitute.clone(),
        [(String::from("ore"), 2)].into_iter().collect(),
    );
    manifest.refresh_hashes();
    let (canonical, _producer_memory) = roots(&path, PageCompression::Raw, Some(&manifest));
    let memory = MemoryBudget::new(64 << 20, 16 << 20).unwrap();
    let store = IncrementalPageStore::open(
        &path,
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Ordinary,
    )
    .unwrap();
    let root = StateRootV1::from_canonical_bytes(&canonical).unwrap();
    let manifest_root = field_root(
        &store,
        &memory,
        &root,
        RuntimeArtifactFieldV1::ChainResourceManifest,
    )
    .unwrap();
    let substitute_child = record_root(&store, &memory, &manifest_root, &substitute);
    let owner_digest = material_ledger_outer_semantic_digest("substitute");
    let wrong_owner = RecordRootRefV1::new(
        RuntimeArtifactRecordV1::MaterialLedger.schema(),
        owner_digest,
        substitute_child.root().clone(),
    )
    .unwrap();
    let key = WorldFieldKeyV1::new(
        RuntimeArtifactRecordV1::ChainResourceManifest.descriptor(),
        WorldFieldRecordKindV1::Entry,
        Some(material_ledger_outer_semantic_digest("expected")),
    )
    .unwrap();
    let mut values = store.value_session(physical());
    let replacement = write_member_record_v1(
        RuntimeArtifactRecordV1::ChainResourceManifest.descriptor(),
        WorldFieldRecordKindV1::Entry,
        &("material_ledgers", &expected),
        MemberPayloadRefV1::RecordRoot(&wrong_owner),
        &mut MemberRecordWriteContext {
            sink: &mut values,
            memory: &memory,
            io: &mut logical(),
            key_work: &mut SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    assert_eq!(replacement.field_key, key);
    let before = manifest_root
        .root()
        .get(
            key.as_bytes(),
            &store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap(),
        )
        .unwrap()
        .unwrap();
    let wrong_binding_root = persist_delta(
        &store,
        &memory,
        manifest_root.root().clone(),
        vec![StateWriteV1 {
            key: key.as_bytes().to_vec(),
            before: Some(before),
            after: Some(replacement.value_root),
        }],
    );
    let wrong_binding_root =
        persist_manifest_child(&store, &memory, root.clone(), wrong_binding_root);
    let roots = bounded_root_set(wrong_binding_root);
    assert!(matches!(
        qualify(&store, &memory, &roots),
        Err(ValueTreeError::Invalid(_))
    ));

    let orphan_key = WorldFieldKeyV1::new(
        RuntimeArtifactRecordV1::ChainResourceManifest.descriptor(),
        WorldFieldRecordKindV1::Entry,
        Some(material_ledger_outer_semantic_digest("expected")),
    )
    .unwrap();
    let orphan_before = manifest_root
        .root()
        .get(
            orphan_key.as_bytes(),
            &store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap(),
        )
        .unwrap()
        .unwrap();
    let orphan_root = persist_delta(
        &store,
        &memory,
        manifest_root.root().clone(),
        vec![StateWriteV1 {
            key: orphan_key.as_bytes().to_vec(),
            before: Some(orphan_before),
            after: None,
        }],
    );
    let orphan_root = persist_manifest_child(&store, &memory, root, orphan_root);
    let roots = bounded_root_set(orphan_root);
    assert!(matches!(
        qualify(&store, &memory, &roots),
        Err(ValueTreeError::Invalid(_))
    ));
}

#[test]
fn delta_tick_overlay_seals_one_outer_write_and_requalifies_cold_root() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let (manifest, delta, builtins) = rich_sources();
        let initial_tick = delta.tick;
        let builtin_source = BuiltinSource(&builtins);
        let source = RuntimeArtifactResourcesRefV1 {
            chain_resource_manifest: &manifest,
            latest_chain_resource_delta: Some(&delta),
            builtin_release_manifest: &builtin_source,
        };
        let (canonical, memory) = persist_sources(&path, compression, &source);
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let artifact_root = StateRootV1::from_canonical_bytes(&canonical).unwrap();
        let roots = bounded_root_set(artifact_root);
        let qualified = qualify(&store, &memory, &roots).unwrap();
        assert_eq!(qualified.record_counts()[1], 1);

        let parent = ExecutionParent {
            commit: [0x6b; 32],
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
            [0x71; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 16,
                max_writes: 4,
                max_facts: 0,
                max_pending_node_bytes: 2 << 20,
            },
        )
        .unwrap();
        let reader = store.value_session(physical());
        let mut reader_io = logical();
        let mut key_work = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut writer = store.value_session(physical());
        let mut writer_io = logical();
        let mut writer_key_work = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut writer_io,
            key_work: &mut writer_key_work,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 8 << 20,
                record_bytes: 16 << 20,
            },
            cancelled: || false,
        };
        let mut work = RuntimeArtifactResourcesWorkV1 {
            cells_remaining: 4096,
            max_pending_node_bytes: 2 << 20,
        };
        let initial_guard = transaction.guard();
        let after_tick = initial_tick + 7;
        let mut context = WorldFieldOverlayContextV1 {
            transaction: &mut transaction,
            values: ValueReadContext {
                source: &reader,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut reader_io,
            },
            key_work: &mut key_work,
            semantic_bytes: 1 << 20,
            record_bytes: 16 << 20,
            take_state_error: || state.take_read_error().map(super::typed),
            cancelled: || false,
        };
        assert!(
            change_runtime_artifact_delta_tick_overlay_v1(
                RuntimeArtifactDeltaTickChangeV1 {
                    expected: initial_tick,
                    after: after_tick,
                },
                &qualified,
                &mut context,
                &mut write,
                &mut work,
                &initial_guard,
                &live,
            )
            .unwrap()
        );
        let second_guard = context.transaction.guard();
        assert!(
            !change_runtime_artifact_delta_tick_overlay_v1(
                RuntimeArtifactDeltaTickChangeV1 {
                    expected: after_tick,
                    after: after_tick,
                },
                &qualified,
                &mut context,
                &mut write,
                &mut work,
                &second_guard,
                &live,
            )
            .unwrap()
        );
        let final_guard = transaction.guard();
        let sealed = transaction.seal(&final_guard, &live, 0).unwrap();
        assert_eq!(sealed.record.reads.len(), 2);
        assert_eq!(sealed.record.writes.len(), 1);
        let expected_outer = WorldFieldKeyV1::new(
            RuntimeArtifactFieldV1::LatestChainResourceDelta.descriptor(),
            WorldFieldRecordKindV1::Entry,
            Some(unit_semantic_digest()),
        )
        .unwrap();
        assert_eq!(
            sealed.record.writes[0].mutation.key,
            expected_outer.as_bytes()
        );
        assert_eq!(
            sealed.record.writes[0].namespace,
            StateNamespaceV1::Artifacts
        );

        let verifier_state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let verified = TransitionVerifier {
            parent: &parent,
            operation: [0x71; 32],
            source: &verifier_state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 2 << 20,
        }
        .verify(&sealed.record)
        .unwrap();
        assert_eq!(verified.record.after, sealed.record.after);
        let outer_mutations = verified
            .record
            .writes
            .iter()
            .filter(|write| write.namespace == StateNamespaceV1::Artifacts)
            .map(|write| write.mutation.clone())
            .collect();
        let delta_charge = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
        let top_delta = delta_charge.retain(
            StateDeltaV1::build(
                parent.roots.state_root(StateNamespaceV1::Artifacts).clone(),
                outer_mutations,
                &verifier_state,
                2 << 20,
            )
            .unwrap(),
        );
        assert_eq!(
            top_delta.after,
            *verified
                .record
                .after
                .state_root(StateNamespaceV1::Artifacts)
        );
        writer.put_state_pages(&top_delta, &memory).unwrap();
        let after_roots = verified.record.after.clone();
        let receiver_state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let receiver_values = store.value_session(physical());
        let mut receiver_io = logical();
        let mut receiver_key_work = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut receiver_context = WorldFieldReadContextV1 {
            root: after_roots.state_root(StateNamespaceV1::Artifacts),
            state: &receiver_state,
            values: ValueReadContext {
                source: &receiver_values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut receiver_io,
            },
            key_work: &mut receiver_key_work,
            semantic_bytes: 1 << 20,
            record_bytes: 16 << 20,
            take_state_error: || receiver_state.take_read_error().map(super::typed),
            cancelled: || false,
        };
        let mut receiver_writer = store.value_session(physical());
        let mut receiver_writer_io = logical();
        let mut receiver_writer_key_work = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut receiver_write = MemberRecordWriteContext {
            sink: &mut receiver_writer,
            memory: &memory,
            io: &mut receiver_writer_io,
            key_work: &mut receiver_writer_key_work,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 8 << 20,
                record_bytes: 16 << 20,
            },
            cancelled: || false,
        };
        let prepared = verify_runtime_artifact_resources_change_transition_v1(
            RuntimeArtifactResourcesChangeV1::DeltaTick(RuntimeArtifactDeltaTickChangeV1 {
                expected: initial_tick,
                after: after_tick,
            }),
            &qualified,
            &TransitionVerifier {
                parent: &parent,
                operation: [0x71; 32],
                source: &receiver_state,
                memory: &memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 2 << 20,
            },
            &sealed.record,
            &mut receiver_context,
            &mut receiver_write,
            &mut work,
            &live,
        )
        .unwrap();
        assert_eq!(prepared.transition().record, sealed.record);
        prepared
            .qualification()
            .validate_binding(&after_roots.digest().unwrap(), MemoryClass::Ordinary)
            .unwrap();
        drop(prepared);
        drop(receiver_values);
        drop(receiver_state);
        drop(receiver_write);
        drop(receiver_writer);
        drop(writer);
        drop(reader);
        drop(state);
        drop(store);
        drop(qualified);

        let cold_memory = MemoryBudget::new(16 << 20, 2 << 20).unwrap();
        let cold_store = IncrementalPageStore::open(
            &path,
            cold_memory.clone(),
            compression,
            MemoryClass::Ordinary,
        )
        .unwrap();
        let requalified = qualify(&cold_store, &cold_memory, &after_roots).unwrap();
        assert_eq!(requalified.record_counts()[1], 1);
        let delta_root = field_root(
            &cold_store,
            &cold_memory,
            after_roots.state_root(StateNamespaceV1::Artifacts),
            RuntimeArtifactFieldV1::LatestChainResourceDelta,
        )
        .unwrap();
        assert_eq!(
            unsigned_cell(
                &cold_store,
                &cold_memory,
                &delta_root,
                RuntimeArtifactRecordV1::ChainResourceDelta,
                "tick",
            ),
            oasis7_runtime::incremental_runtime_artifact_resources::RuntimeArtifactValueV1::Unsigned(
                after_tick
            )
        );
    }
}

#[path = "incremental_runtime_artifact_resources_change_tests.rs"]
mod change_tests;

#[path = "incremental_runtime_artifact_resources_pages.rs"]
mod pages;
