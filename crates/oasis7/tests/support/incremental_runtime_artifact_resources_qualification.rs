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
    incremental_execution::RootSetV1,
    incremental_state::{StateScanBudget, StateWriteV1},
};
use oasis7_runtime::BuiltinReleaseManifestEntry;
use oasis7_runtime::incremental_runtime_artifact_resources::{
    BuiltinReleaseEntryRefV1, BuiltinReleaseManifestSourceV1, BuiltinReleaseModuleSetSourceV1,
    RuntimeArtifactResourcesReadContextV1, qualify_runtime_artifact_resources_v1,
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
