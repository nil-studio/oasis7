//! Component physical proof. Borrowed setup DTO is outside the cold operation.
use super::*;
use oasis7_distfs::incremental_page_store::*;
use oasis7_formal_protocol::{
    incremental_execution::*, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_world_state::{
    incremental_field_member::*, incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1, incremental_json_records::*,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "oasis7-artifact-config-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 2_000_000,
        read_bytes_remaining: 4 << 30,
        writes_remaining: 2_000_000,
        write_bytes_remaining: 4 << 30,
    }
}
fn delta_io() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 65_536,
        read_bytes_remaining: 64 << 20,
        writes_remaining: 65_536,
        write_bytes_remaining: 64 << 20,
    }
}
fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 2_000_000,
        read_bytes_remaining: 4 << 30,
        write_bytes_remaining: 4 << 30,
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 100_000,
    }
}
fn read_work() -> ArtifactConfigReadWorkV1 {
    ArtifactConfigReadWorkV1 {
        cells_remaining: 100_000,
        max_payload_bytes: 128 << 20,
        json: JsonReadWorkV1 {
            cells_remaining: 100_000,
            json_nodes_remaining: 1000,
            max_depth: 16,
            max_payload_bytes: 128 << 20,
        },
    }
}
fn write_work() -> ArtifactConfigWriteWorkV1 {
    ArtifactConfigWriteWorkV1 {
        cells_remaining: 100_000,
        max_pending_node_bytes: 1 << 20,
        json: JsonWriteWorkV1 {
            cells_remaining: 100_000,
            json_nodes_remaining: 1000,
            max_json_depth: 16,
            max_pending_node_bytes: 1 << 20,
        },
    }
}
fn state_error(e: PageStoreError) -> ValueTreeError {
    match e {
        PageStoreError::Memory(e) => ValueTreeError::Memory(e),
        PageStoreError::ResourceLimited(s) => ValueTreeError::ResourceLimited(s),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        PageStoreError::Io(e) => ValueTreeError::Io(e),
        _ => ValueTreeError::Invalid("artifact config physical"),
    }
}
fn address(column: &'static str) -> ArtifactConfigAddressV1<'static> {
    ArtifactConfigAddressV1 {
        section: "manifest",
        ordinal: None,
        inner_ordinal: None,
        member: None,
        column,
    }
}

fn bootstrap(
    store: &IncrementalPageStore,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    class: MemoryClass,
) -> (
    ReservedValue<ArtifactConfigFamilyQualificationV1>,
    ReservedValue<ArtifactConfigBodyQualificationV1>,
    [u8; 32],
) {
    let state = store.state_session(physical(), class).unwrap();
    let values = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 8 << 20,
    };
    let mut ctx = WorldFieldReadContextV1 {
        root: &roots.state[5],
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory,
            class,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 128 << 20,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let family = qualify_artifact_config_family_v1(
        roots,
        &mut ctx,
        &mut scan(),
        &mut scan(),
        &mut read_work(),
    )
    .unwrap();
    let body = open_qualified_artifact_config_body_v1(
        ArtifactConfigFieldV1::Manifest,
        ArtifactConfigRecordKeyV1::Global,
        roots,
        &family,
        &mut ctx,
    )
    .unwrap()
    .unwrap();
    let pointer = inspect_artifact_config_cell_v1(
        ArtifactConfigBodyKindV1::Manifest,
        address("content"),
        165,
        &mut WorldFieldReadContextV1 {
            root: body.root(),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 128 << 20,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        },
        &mut std::io::sink(),
    )
    .unwrap();
    let ArtifactConfigValueV1::JsonRoot(root) = &*pointer else {
        panic!("JSON pointer")
    };
    let forbidden = *blake3::Hash::from_hex(&root.node_root).unwrap().as_bytes();
    eprintln!(
        "artifact config full bootstrap class={class:?} physical={:?} logical={io:?}",
        state.remaining_io().unwrap()
    );
    (family, body, forbidden)
}
struct Forbid<'a, R> {
    source: &'a R,
    id: [u8; 32],
}
impl<R: ValueNodeReader> ValueNodeReader for Forbid<'_, R> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        assert_ne!(
            id, &self.id,
            "ordinary scalar delta scanned unchanged JSON graph"
        );
        self.source.read_node(id, max, memory, class)
    }
}

#[test]
fn raw_zstd_cold_config_and_scalar_delta_do_not_scan_growing_json() {
    let mut baseline = [None, None];
    for length in [64, 1 << 20, 32 << 20, 64 << 20] {
        for (index, compression) in [PageCompression::Raw, PageCompression::Zstd]
            .into_iter()
            .enumerate()
        {
            let actual = run(length, compression);
            if let Some(expected) = baseline[index] {
                assert_eq!(
                    actual, expected,
                    "logical delta and physical writes grew with unchanged JSON S={length}"
                );
            } else {
                baseline[index] = Some(actual);
            }
        }
    }
}
fn run(length: usize, compression: PageCompression) -> [u64; 8] {
    let directory = Directory::new();
    let source_path = directory.0.join("source");
    let target_path = directory.0.join("target");
    let memory = MemoryBudget::new(192 << 20, 4 << 20).unwrap();
    let stopped = AtomicBool::new(false);
    let roots = {
        let source =
            IncrementalPageStore::create(&source_path, memory.clone(), compression).unwrap();
        let target =
            IncrementalPageStore::create(&target_path, memory.clone(), compression).unwrap();
        let roots = {
            let _input = memory
                .reserve(length + 8 * (1 << 20), MemoryClass::Ordinary)
                .unwrap();
            let mut world = World::new();
            world.manifest.version = 0;
            world.manifest.content = serde_json::Value::String("x".repeat(length));
            world.module_registry.records.clear();
            world.module_registry.active.clear();
            let _rows = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
            let mut rows = Vec::with_capacity(16);
            let mut values = source.value_session(physical());
            let mut io = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 8 << 20,
            };
            world
                .prepare_incremental_artifact_config_genesis_v1(
                    &mut MemberRecordWriteContext {
                        sink: &mut values,
                        memory: &memory,
                        io: &mut io,
                        key_work: &mut keys,
                        limits: MemberRecordLimitsV1 {
                            semantic_bytes: 4096,
                            payload_bytes: 128 << 20,
                            record_bytes: 128 << 20,
                        },
                        cancelled: || false,
                    },
                    &mut write_work(),
                    &mut |row| {
                        assert!(rows.len() < 16);
                        rows.push(row);
                        Ok(())
                    },
                )
                .unwrap();
            let charge = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
            let mut writes: Vec<_> = rows.iter().map(|row| row.mutation.clone()).collect();
            writes.sort_by(|a, b| a.key.cmp(&b.key));
            let state = source
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let delta = charge.retain(
                StateDeltaV1::build(
                    StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap(),
                    writes,
                    &state,
                    8 << 20,
                )
                .unwrap(),
            );
            values.put_state_pages(&delta, &memory).unwrap();
            let mut roots = RootSetV1::empty("artifact config actual".into(), [12; 32]).unwrap();
            roots.state[5] = delta.after.clone();
            roots
        }; // World and its large original JsonValue have been dropped.
        let state = source
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = source.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 8 << 20,
        };
        let mut target_io = physical();
        let mut never = || false;
        let mut writer = target
            .buffered_page_writer(&mut target_io, &stopped, &mut never)
            .unwrap();
        let pages = transfer_artifact_config_pages_v1(
            &roots,
            &mut WorldFieldReadContextV1 {
                root: &roots.state[5],
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io,
                },
                key_work: &mut keys,
                semantic_bytes: 4096,
                record_bytes: 128 << 20,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            },
            &mut ArtifactConfigPagesWorkV1 {
                outer_scan: &mut scan(),
                inner_scan: &mut scan(),
                body: &mut read_work(),
                max_pages: 32_768,
            },
            &mut writer,
        )
        .unwrap();
        writer.finish().unwrap();
        let target_values = target.value_session(physical());
        pages
            .visit_pages::<ValueTreeError>(|id| {
                let source = values.read_node(id, 128 << 20, &memory, MemoryClass::Ordinary)?;
                let target =
                    target_values.read_node(id, 128 << 20, &memory, MemoryClass::Ordinary)?;
                assert_eq!(blake3::hash(&target).as_bytes(), id);
                assert_eq!(&**source, &**target);
                Ok(())
            })
            .unwrap();
        roots
    };
    assert_eq!(memory.used(), 0);
    drop(memory);
    // New source-independent pool, Ordinary exhausted, 4MiB Recovery retained.
    let cold = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    {
        let store = IncrementalPageStore::open(
            &target_path,
            cold.clone(),
            compression,
            MemoryClass::Ordinary,
        )
        .unwrap();
        let hold = cold
            .reserve((4 << 20) - cold.used(), MemoryClass::Ordinary)
            .unwrap();
        assert!(cold.reserve(1, MemoryClass::Ordinary).is_err());
        let (family, body, _) = bootstrap(&store, &roots, &cold, MemoryClass::Recovery);
        assert_eq!(family.allocation_class(), MemoryClass::Recovery);
        assert_eq!(body.allocation_class(), MemoryClass::Recovery);
        drop(hold);
    }
    assert_eq!(cold.used(), 0);
    drop(cold);
    // Fixed envelope across all S. Zstd's existing 16MiB encoder admission is
    // respected; 32MiB is not claimed to be a 4MiB ordinary write operation.
    let total = if matches!(compression, PageCompression::Raw) {
        8 << 20
    } else {
        32 << 20
    };
    let memory = MemoryBudget::new(total, 4 << 20).unwrap();
    let store = IncrementalPageStore::open(
        &target_path,
        memory.clone(),
        compression,
        MemoryClass::Ordinary,
    )
    .unwrap();
    let (family, body, forbidden) = bootstrap(&store, &roots, &memory, MemoryClass::Ordinary);
    delta(&store, &roots, &memory, &family, &body, forbidden, length)
}

fn delta(
    store: &IncrementalPageStore,
    roots: &RootSetV1,
    memory: &MemoryBudget,
    family: &ReservedValue<ArtifactConfigFamilyQualificationV1>,
    body: &ReservedValue<ArtifactConfigBodyQualificationV1>,
    forbidden: [u8; 32],
    length: usize,
) -> [u64; 8] {
    let state = store
        .state_session(delta_io(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(delta_io());
    let mut writer = store.value_session(delta_io());
    let source = Forbid {
        source: &values,
        id: forbidden,
    };
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [41; 32],
        authority: AuthorityBinding {
            token: [42; 32],
            epoch: 3,
        },
    };
    let live = parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [43; 32],
        &state,
        memory,
        TransactionLimits {
            max_reads: 4096,
            max_writes: 8,
            max_facts: 0,
            max_pending_node_bytes: 1 << 20,
        },
    )
    .unwrap();
    let mut reads = ValueIoBudget {
        nodes_remaining: 4096,
        read_bytes_remaining: 512 << 10,
        write_bytes_remaining: 512 << 10,
    };
    let mut writes = reads.clone();
    let mut read_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut ctx = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &source,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut reads,
        },
        key_work: &mut read_keys,
        semantic_bytes: 4096,
        record_bytes: 8192,
        take_state_error: || state.take_read_error().map(state_error),
        cancelled: || false,
    };
    let guard = ctx.transaction.guard();
    change_qualified_artifact_config_cell_v1(
        ArtifactConfigCellChangeV1 {
            request: ArtifactConfigCellRequestV1 {
                field: ArtifactConfigFieldV1::Manifest,
                key: ArtifactConfigRecordKeyV1::Global,
                address: address("version"),
                family,
                qualification: body,
                max_payload_bytes: 165,
            },
            expected: ArtifactConfigScalarRefV1::Unsigned(0),
            after: ArtifactConfigScalarRefV1::Unsigned(1),
            max_pending_node_bytes: 1 << 20,
        },
        &mut ctx,
        &mut MemberRecordWriteContext {
            sink: &mut writer,
            memory,
            io: &mut writes,
            key_work: &mut write_keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 165,
                record_bytes: 8192,
            },
            cancelled: || false,
        },
        &guard,
        &live,
    )
    .unwrap()
    .unwrap();
    let guard = ctx.transaction.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    assert_eq!(prepared.record.writes.len(), 1);
    for node in &prepared.new_nodes {
        writer
            .put_node(
                *blake3::hash(&node.bytes).as_bytes(),
                memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [43; 32],
        source: &state,
        memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 1 << 20,
    };
    verifier.verify(&prepared.record).unwrap();
    accept_artifact_config_transition_v1(
        family,
        &verifier,
        &prepared.record,
        &mut WorldFieldReadContextV1 {
            root: &prepared.record.after.state[5],
            state: &state,
            values: ValueReadContext {
                source: &source,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut reads,
            },
            key_work: &mut read_keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        },
        &mut scan(),
        &mut ArtifactConfigReadWorkV1 {
            cells_remaining: 4096,
            max_payload_bytes: 165,
            json: JsonReadWorkV1 {
                cells_remaining: 0,
                json_nodes_remaining: 0,
                max_depth: 16,
                max_payload_bytes: 165,
            },
        },
    )
    .unwrap()
    .validate_binding(&prepared.record.after.digest().unwrap())
    .unwrap();
    eprintln!(
        "artifact config delta S={length} logical_read={reads:?} logical_write={writes:?} state={:?} value={:?} writer={:?}",
        state.remaining_io().unwrap(),
        values.remaining_io().unwrap(),
        writer.remaining_io().unwrap()
    );
    let physical_writes = writer.remaining_io().unwrap();
    let metrics = [
        reads.nodes_remaining,
        reads.read_bytes_remaining,
        reads.write_bytes_remaining,
        writes.nodes_remaining,
        writes.read_bytes_remaining,
        writes.write_bytes_remaining,
        physical_writes.writes_remaining,
        physical_writes.write_bytes_remaining,
    ];
    let (_, after, _) = bootstrap(store, &prepared.record.after, memory, MemoryClass::Ordinary);
    assert_ne!(after.root(), body.root());
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let observed = inspect_artifact_config_cell_v1(
        ArtifactConfigBodyKindV1::Manifest,
        address("version"),
        165,
        &mut WorldFieldReadContextV1 {
            root: after.root(),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        },
        &mut std::io::sink(),
    )
    .unwrap();
    assert_eq!(*observed, ArtifactConfigValueV1::Unsigned(1));
    metrics
}
