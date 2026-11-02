#![cfg(all(unix, not(target_arch = "wasm32")))]

use oasis7_agent_api::world_resource::CHAIN_RESOURCE_MANIFEST_SCHEMA_V1;
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
use oasis7_runtime::incremental_runtime_artifact_resources::{
    RuntimeArtifactChildRootV1, RuntimeArtifactFieldV1, RuntimeArtifactMemberProfileV1,
    RuntimeArtifactRecordV1, RuntimeArtifactResourcesWorkV1, RuntimeArtifactRootBindingV1,
    inspect_runtime_artifact_record_root_v1, read_runtime_artifact_member_v1,
};
use oasis7_runtime::{World, incremental_field_member::*, incremental_field_read::*};

#[allow(dead_code)]
#[path = "support/incremental_inventory_fixture.rs"]
mod fixture;
use fixture::{Directory, logical, physical};

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
) -> (
    [u8; oasis7_formal_protocol::incremental_state::STATE_ROOT_BYTES],
    MemoryBudget,
) {
    let memory = MemoryBudget::new(64 << 20, 16 << 20).unwrap();
    let store = IncrementalPageStore::create(path, memory.clone(), compression).unwrap();
    let world = World::new();
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
    let emitted = world
        .prepare_incremental_artifact_resources_genesis_v1(
            &mut MemberRecordWriteContext {
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
            },
            &mut work,
            &mut |write| {
                assert!(rows.len() < 32_768);
                rows.push(write);
                Ok(())
            },
        )
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

#[test]
fn artifact_roots_and_typed_member_read_survive_raw_and_zstd_reopen() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let path = directory.0.join("pages");
        let (canonical, _producer_memory) = roots(&path, compression);
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
