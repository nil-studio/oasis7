#![cfg(all(unix, not(target_arch = "wasm32")))]

use oasis7_agent_api::{
    world_action::MaterialLedgerId,
    world_resource::{CHAIN_RESOURCE_MANIFEST_SCHEMA_V1, ChainResourceManifest},
};
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageStoreError,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_record_root::RecordRootRefV1,
    incremental_state::{
        StateDeltaV1, StateNamespaceV1, StatePageWriter, StateRootV1, StateWriteV1,
    },
    incremental_value::{ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::incremental_runtime_artifact_resources::{
    BuiltinReleaseManifestSourceV1, BuiltinReleaseModuleSetSourceV1,
    RUNTIME_ARTIFACT_RESOURCES_SCAN_DEPTH_V1, RuntimeArtifactChildRootV1, RuntimeArtifactFieldV1,
    RuntimeArtifactMemberProfileV1, RuntimeArtifactRecordV1, RuntimeArtifactResourcesRefV1,
    RuntimeArtifactResourcesWorkV1, RuntimeArtifactRootBindingV1, RuntimeArtifactValueV1,
    inspect_runtime_artifact_record_root_v1, prepare_runtime_artifact_resources_genesis_v1,
    read_runtime_artifact_member_v1, read_runtime_artifact_record_reference_v1,
};
use oasis7_runtime::{
    World,
    incremental_field_member::*,
    incremental_field_presence::{
        PresenceRecordLimitsV1, PresenceRecordWriteContext, write_field_group_v1,
        write_field_metadata_v1,
    },
    incremental_field_read::*,
};

#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical, physical};

#[path = "support/incremental_runtime_artifact_resources_qualification.rs"]
mod qualification;

struct EmptyModuleSet;
impl BuiltinReleaseModuleSetSourceV1 for EmptyModuleSet {
    fn len(&self) -> usize {
        0
    }

    fn for_each_module<'a, E, F>(&'a self, _visit: F) -> Result<(), E>
    where
        F: FnMut(
            &'a str,
            oasis7_runtime::incremental_runtime_artifact_resources::BuiltinReleaseEntryRefV1<'a>,
        ) -> Result<(), E>,
    {
        Ok(())
    }
}

struct EmptyBuiltinManifest;
impl BuiltinReleaseManifestSourceV1 for EmptyBuiltinManifest {
    type ModuleSet<'a>
        = EmptyModuleSet
    where
        Self: 'a;

    fn len(&self) -> usize {
        0
    }

    fn for_each_module_set<'a, E, F>(&'a self, _visit: F) -> Result<(), E>
    where
        F: FnMut(&'a str, Self::ModuleSet<'a>) -> Result<(), E>,
    {
        Ok(())
    }
}

fn typed(error: PageStoreError) -> ValueTreeError {
    match error {
        PageStoreError::Memory(error) => ValueTreeError::Memory(error),
        PageStoreError::ResourceLimited(reason) => ValueTreeError::ResourceLimited(reason),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        PageStoreError::Io(error) => ValueTreeError::Io(error),
        _ => ValueTreeError::Invalid("artifact state page failure"),
    }
}

fn roots(
    path: &std::path::Path,
    compression: PageCompression,
    manifest_override: Option<&ChainResourceManifest>,
) -> (
    [u8; oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES],
    MemoryBudget,
) {
    let memory = MemoryBudget::new(64 << 20, 16 << 20).unwrap();
    let store = IncrementalPageStore::create(path, memory.clone(), compression).unwrap();
    let world = World::new();
    let builtin = EmptyBuiltinManifest;
    let mut values = store.value_session(physical());
    let _rows_charge = memory
        .reserve(
            32_768 * std::mem::size_of::<ReservedValue<ExecutionWriteV1>>(),
            MemoryClass::Ordinary,
        )
        .unwrap();
    let mut rows: Vec<ReservedValue<ExecutionWriteV1>> = Vec::with_capacity(32_768);
    let mut work = RuntimeArtifactResourcesWorkV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 2 << 20,
    };
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
        assert!(rows.len() < 32_768);
        rows.push(write);
        Ok(())
    };
    let emitted = match manifest_override {
        Some(manifest) => prepare_runtime_artifact_resources_genesis_v1(
            &RuntimeArtifactResourcesRefV1 {
                chain_resource_manifest: manifest,
                latest_chain_resource_delta: None,
                builtin_release_manifest: &builtin,
            },
            &mut context,
            &mut work,
            &mut sink,
        ),
        None => world.prepare_incremental_artifact_resources_genesis_v1(
            &mut context,
            &mut work,
            &mut sink,
        ),
    }
    .unwrap();
    assert!(emitted > 0);
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let mut changes: Vec<_> = rows
        .iter()
        .map(|row| StateWriteV1 {
            key: row.mutation.key.clone(),
            before: row.mutation.before,
            after: row.mutation.after,
        })
        .collect();
    changes.sort_by(|left, right| left.key.cmp(&right.key));
    let delta_space = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
    let delta = delta_space.retain(
        StateDeltaV1::build(
            StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap(),
            changes,
            &state,
            2 << 20,
        )
        .unwrap(),
    );
    assert_eq!(delta.after.leaf_count, rows.len() as u64);
    values.put_state_pages(&delta, &memory).unwrap();
    (
        delta.after.canonical_bytes().unwrap().try_into().unwrap(),
        memory,
    )
}

fn field_root(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    root: &StateRootV1,
    field: RuntimeArtifactFieldV1,
) -> Option<ReservedValue<RuntimeArtifactChildRootV1>> {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 4096,
    };
    inspect_runtime_artifact_record_root_v1(
        field,
        &mut WorldFieldReadContextV1 {
            root,
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut key_work,
            semantic_bytes: 1024,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(typed),
            cancelled: || false,
        },
    )
    .unwrap()
}

fn record_root(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    parent: &ReservedValue<RuntimeArtifactChildRootV1>,
    ledger_id: &MaterialLedgerId,
) -> ReservedValue<RuntimeArtifactChildRootV1> {
    record_root_result(store, memory, parent, ledger_id)
        .unwrap()
        .expect("nested record root")
}

fn record_root_result(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    parent: &ReservedValue<RuntimeArtifactChildRootV1>,
    ledger_id: &MaterialLedgerId,
) -> Result<Option<ReservedValue<RuntimeArtifactChildRootV1>>, ValueTreeError> {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 4096,
    };
    let key = ("material_ledgers", ledger_id);
    read_runtime_artifact_record_reference_v1(
        RuntimeArtifactRecordV1::ChainResourceManifest,
        RuntimeArtifactRecordV1::MaterialLedger,
        parent,
        &key,
        &mut WorldFieldReadContextV1 {
            root: parent.root(),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut key_work,
            semantic_bytes: 1024,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(typed),
            cancelled: || false,
        },
    )
}

fn unit_semantic_digest() -> [u8; 32] {
    let mut bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    bytes.push(0);
    *blake3::hash(&bytes).as_bytes()
}

fn material_ledger_outer_semantic_digest(agent_id: &str) -> [u8; 32] {
    let mut bytes = b"oasis7.iws.semantic-key.v1\0".to_vec();
    bytes.push(5); // tuple: original "material_ledgers" map key + ledger ID
    bytes.push(1);
    bytes.extend_from_slice(&("material_ledgers".len() as u64).to_be_bytes());
    bytes.extend_from_slice(b"material_ledgers");
    bytes.extend_from_slice(&[4, 1, 1]); // MaterialLedgerId::Agent + text key
    bytes.extend_from_slice(&(agent_id.len() as u64).to_be_bytes());
    bytes.extend_from_slice(agent_id.as_bytes());
    *blake3::hash(&bytes).as_bytes()
}

fn persist_artifact_root(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    mut writes: Vec<StateWriteV1>,
) -> StateRootV1 {
    writes.sort_by(|left, right| left.key.cmp(&right.key));
    assert!(writes.windows(2).all(|pair| pair[0].key != pair[1].key));
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let workspace = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let delta = workspace.retain(
        StateDeltaV1::build(
            StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap(),
            writes,
            &state,
            1 << 20,
        )
        .unwrap(),
    );
    let mut values = store.value_session(physical());
    values.put_state_pages(&delta, memory).unwrap();
    delta.after.clone()
}

fn synthetic_manifest_root(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    reported_count: u64,
    ledger_edge: Option<(&MaterialLedgerId, &StateRootV1, [u8; 32])>,
) -> StateRootV1 {
    let descriptor = RuntimeArtifactRecordV1::ChainResourceManifest.descriptor();
    let mut values = store.value_session(physical());
    let mut io = logical();
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let presence_limits = PresenceRecordLimitsV1 {
        semantic_bytes: 1 << 20,
        record_bytes: 1 << 20,
    };
    let metadata = write_field_metadata_v1(
        descriptor,
        &mut PresenceRecordWriteContext {
            sink: &mut values,
            memory,
            io: &mut io,
            key_work: &mut key_work,
            limits: presence_limits,
            cancelled: || false,
        },
    )
    .unwrap();
    let group = write_field_group_v1(
        descriptor,
        &("material_ledgers", ()),
        &mut PresenceRecordWriteContext {
            sink: &mut values,
            memory,
            io: &mut io,
            key_work: &mut key_work,
            limits: presence_limits,
            cancelled: || false,
        },
    )
    .unwrap();
    let count = write_member_record_v1(
        descriptor,
        WorldFieldRecordKindV1::Entry,
        &"material_ledgers.count",
        MemberPayloadRefV1::U64(reported_count),
        &mut MemberRecordWriteContext {
            sink: &mut values,
            memory,
            io: &mut io,
            key_work: &mut key_work,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 8,
                record_bytes: 1 << 20,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    let mut writes = vec![
        StateWriteV1 {
            key: metadata.field_key.as_bytes().to_vec(),
            before: None,
            after: Some(metadata.value_root),
        },
        StateWriteV1 {
            key: group.field_key.as_bytes().to_vec(),
            before: None,
            after: Some(group.value_root),
        },
        StateWriteV1 {
            key: count.field_key.as_bytes().to_vec(),
            before: None,
            after: Some(count.value_root),
        },
    ];
    if let Some((ledger_id, child, encoded_owner)) = ledger_edge {
        let reference = RecordRootRefV1::new(
            RuntimeArtifactRecordV1::MaterialLedger.schema(),
            encoded_owner,
            child.clone(),
        )
        .unwrap();
        let edge = write_member_record_v1(
            descriptor,
            WorldFieldRecordKindV1::Entry,
            &("material_ledgers", ledger_id),
            MemberPayloadRefV1::RecordRoot(&reference),
            &mut MemberRecordWriteContext {
                sink: &mut values,
                memory,
                io: &mut io,
                key_work: &mut key_work,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1 << 20,
                    payload_bytes: 1 << 20,
                    record_bytes: 2 << 20,
                },
                cancelled: || false,
            },
        )
        .unwrap();
        writes.push(StateWriteV1 {
            key: edge.field_key.as_bytes().to_vec(),
            before: None,
            after: Some(edge.value_root),
        });
    }
    drop(values);
    persist_artifact_root(store, memory, writes)
}

fn wrap_manifest_child(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    manifest: &StateRootV1,
) -> StateRootV1 {
    let field = RuntimeArtifactFieldV1::ChainResourceManifest;
    let reference =
        RecordRootRefV1::new(field.schema(), unit_semantic_digest(), manifest.clone()).unwrap();
    let mut values = store.value_session(physical());
    let mut io = logical();
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let metadata = write_field_metadata_v1(
        field.descriptor(),
        &mut PresenceRecordWriteContext {
            sink: &mut values,
            memory,
            io: &mut io,
            key_work: &mut key_work,
            limits: PresenceRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                record_bytes: 1 << 20,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    let entry = write_member_record_v1(
        field.descriptor(),
        WorldFieldRecordKindV1::Entry,
        &(),
        MemberPayloadRefV1::RecordRoot(&reference),
        &mut MemberRecordWriteContext {
            sink: &mut values,
            memory,
            io: &mut io,
            key_work: &mut key_work,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 1 << 20,
                payload_bytes: 1 << 20,
                record_bytes: 2 << 20,
            },
            cancelled: || false,
        },
    )
    .unwrap();
    let writes = vec![
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
    ];
    drop(values);
    persist_artifact_root(store, memory, writes)
}

fn material_amount_bytes(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    child: &ReservedValue<RuntimeArtifactChildRootV1>,
    material: &String,
) -> RuntimeArtifactValueV1 {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 4096,
    };
    let key = ("materials", material);
    let result = read_runtime_artifact_member_v1(
        RuntimeArtifactRecordV1::MaterialLedger,
        child,
        &key,
        RuntimeArtifactMemberProfileV1::Signed,
        &mut WorldFieldReadContextV1 {
            root: child.root(),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut key_work,
            semantic_bytes: 1024,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(typed),
            cancelled: || false,
        },
        8,
        &mut std::io::sink(),
    )
    .unwrap();
    assert!(result.is_some(), "required typed member");
    result.unwrap()
}

fn unsigned_cell(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    child: &ReservedValue<RuntimeArtifactChildRootV1>,
    record: RuntimeArtifactRecordV1,
    key: &str,
) -> RuntimeArtifactValueV1 {
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut key_work = SemanticKeyBudget {
        bytes_remaining: 4096,
    };
    let result = read_runtime_artifact_member_v1(
        record,
        child,
        key,
        RuntimeArtifactMemberProfileV1::Unsigned,
        &mut WorldFieldReadContextV1 {
            root: child.root(),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut logical(),
            },
            key_work: &mut key_work,
            semantic_bytes: 1024,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(typed),
            cancelled: || false,
        },
        8,
        &mut std::io::sink(),
    )
    .unwrap();
    assert!(result.is_some(), "required material count");
    result.unwrap()
}

#[test]
fn artifact_roots_and_typed_member_read_survive_raw_and_zstd_reopen() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let (canonical, _producer_memory) = roots(&path, compression, None);
        let memory = MemoryBudget::new(16 << 20, 2 << 20).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let root = StateRootV1::from_canonical_bytes(&canonical).unwrap();
        let manifest = field_root(
            &store,
            &memory,
            &root,
            RuntimeArtifactFieldV1::ChainResourceManifest,
        )
        .expect("required manifest root");
        let delta = field_root(
            &store,
            &memory,
            &root,
            RuntimeArtifactFieldV1::LatestChainResourceDelta,
        );
        assert!(delta.is_none(), "None remains a typed Unit member");
        let builtin = field_root(
            &store,
            &memory,
            &root,
            RuntimeArtifactFieldV1::BuiltinReleaseManifest,
        );
        assert!(builtin.is_some(), "required builtin root");

        assert_eq!(
            manifest.binding(),
            RuntimeArtifactRootBindingV1::Field(RuntimeArtifactFieldV1::ChainResourceManifest)
        );
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut key_work = SemanticKeyBudget {
            bytes_remaining: 4096,
        };
        let mut output = [0; 64];
        let result = read_runtime_artifact_member_v1(
            RuntimeArtifactRecordV1::ChainResourceManifest,
            &manifest,
            &"schema_version",
            RuntimeArtifactMemberProfileV1::Text,
            &mut WorldFieldReadContextV1 {
                root: manifest.root(),
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                },
                key_work: &mut key_work,
                semantic_bytes: 1024,
                record_bytes: 4096,
                take_state_error: || state.take_read_error().map(typed),
                cancelled: || false,
            },
            64,
            &mut &mut output[..],
        )
        .unwrap();
        assert!(result.is_some(), "schema_version is a typed text member");
        assert_eq!(
            &output[..CHAIN_RESOURCE_MANIFEST_SCHEMA_V1.len()],
            CHAIN_RESOURCE_MANIFEST_SCHEMA_V1.as_bytes()
        );

        let mut key_work = SemanticKeyBudget {
            bytes_remaining: 4096,
        };
        let mut output = [0; 64];
        let mismatched_record = read_runtime_artifact_member_v1(
            RuntimeArtifactRecordV1::BuiltinReleaseManifest,
            &manifest,
            &"schema_version",
            RuntimeArtifactMemberProfileV1::Text,
            &mut WorldFieldReadContextV1 {
                root: manifest.root(),
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut logical(),
                },
                key_work: &mut key_work,
                semantic_bytes: 1024,
                record_bytes: 4096,
                take_state_error: || state.take_read_error().map(typed),
                cancelled: || false,
            },
            64,
            &mut &mut output[..],
        );
        assert!(mismatched_record.is_err(), "child binding cannot be reused");
    }
}

#[test]
fn material_ledger_identity_and_empty_group_survive_nested_cold_read() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let mut manifest = ChainResourceManifest::empty_at_height("world", 9, 0);
        let ledger_id = MaterialLedgerId::agent("agent-1");
        let material = String::from("ore\0λ");
        manifest.material_ledgers.insert(
            ledger_id.clone(),
            [(material.clone(), -17)].into_iter().collect(),
        );
        let empty_ledger = MaterialLedgerId::site("empty");
        manifest
            .material_ledgers
            .insert(empty_ledger.clone(), Default::default());
        manifest.refresh_hashes();

        let directory = Directory::new();
        let path = directory.0.join("pages");
        let (canonical, _producer_memory) = roots(&path, compression, Some(&manifest));
        let memory = MemoryBudget::new(16 << 20, 2 << 20).unwrap();
        let store =
            IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
                .unwrap();
        let root = StateRootV1::from_canonical_bytes(&canonical).unwrap();
        let manifest_root = field_root(
            &store,
            &memory,
            &root,
            RuntimeArtifactFieldV1::ChainResourceManifest,
        )
        .expect("manifest root");
        let ledger_root = record_root(&store, &memory, &manifest_root, &ledger_id);
        assert_eq!(
            ledger_root.binding(),
            RuntimeArtifactRootBindingV1::Record(RuntimeArtifactRecordV1::MaterialLedger)
        );
        let amount = material_amount_bytes(&store, &memory, &ledger_root, &material);
        assert_eq!(amount, RuntimeArtifactValueV1::Signed(-17));

        let empty_root = record_root(&store, &memory, &manifest_root, &empty_ledger);
        let count = unsigned_cell(
            &store,
            &memory,
            &empty_root,
            RuntimeArtifactRecordV1::MaterialLedger,
            "materials.count",
        );
        assert_eq!(count, RuntimeArtifactValueV1::Unsigned(0));
    }
}

#[test]
fn material_ledger_orphan_and_same_count_substitution_remain_distinct() {
    let directory = Directory::new();
    let path = directory.0.join("pages");
    let mut manifest = ChainResourceManifest::empty_at_height("world", 9, 0);
    let expected = MaterialLedgerId::agent("expected");
    let substitute = MaterialLedgerId::agent("substitute");
    manifest.material_ledgers.insert(
        expected.clone(),
        [(String::from("ore"), -3)].into_iter().collect(),
    );
    manifest.material_ledgers.insert(
        substitute.clone(),
        [(String::from("ore"), 41)].into_iter().collect(),
    );
    manifest.refresh_hashes();
    let (canonical, _producer_memory) = roots(&path, PageCompression::Raw, Some(&manifest));
    let memory = MemoryBudget::new(16 << 20, 2 << 20).unwrap();
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
    let expected_child = record_root(&store, &memory, &manifest_root, &expected);
    assert_ne!(
        material_ledger_outer_semantic_digest("expected"),
        material_ledger_outer_semantic_digest("substitute")
    );
    assert_eq!(
        unsigned_cell(
            &store,
            &memory,
            &expected_child,
            RuntimeArtifactRecordV1::MaterialLedger,
            "materials.count",
        ),
        RuntimeArtifactValueV1::Unsigned(1)
    );
    assert_eq!(
        unsigned_cell(
            &store,
            &memory,
            &substitute_child,
            RuntimeArtifactRecordV1::MaterialLedger,
            "materials.count",
        ),
        RuntimeArtifactValueV1::Unsigned(1),
        "replacement child has the same member count"
    );

    let orphan_manifest = synthetic_manifest_root(&store, &memory, 1, None);
    let orphan_top = wrap_manifest_child(&store, &memory, &orphan_manifest);
    let orphan_parent = field_root(
        &store,
        &memory,
        &orphan_top,
        RuntimeArtifactFieldV1::ChainResourceManifest,
    )
    .unwrap();
    assert_eq!(
        unsigned_cell(
            &store,
            &memory,
            &orphan_parent,
            RuntimeArtifactRecordV1::ChainResourceManifest,
            "material_ledgers.count",
        ),
        RuntimeArtifactValueV1::Unsigned(1)
    );
    assert!(
        record_root_result(&store, &memory, &orphan_parent, &expected)
            .unwrap()
            .is_none(),
        "a count marker cannot make an orphaned ledger edge present"
    );

    let substituted_manifest = synthetic_manifest_root(
        &store,
        &memory,
        1,
        Some((
            &expected,
            substitute_child.root(),
            material_ledger_outer_semantic_digest("substitute"),
        )),
    );
    let substituted_top = wrap_manifest_child(&store, &memory, &substituted_manifest);
    let substituted_parent = field_root(
        &store,
        &memory,
        &substituted_top,
        RuntimeArtifactFieldV1::ChainResourceManifest,
    )
    .unwrap();
    assert!(
        record_root_result(&store, &memory, &substituted_parent, &expected).is_err(),
        "a same-count child bound to another MaterialLedgerId is invalid"
    );
}
