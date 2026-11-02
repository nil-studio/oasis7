use super::*;
use oasis7_distfs::incremental_page_store::{
    objects::{IncrementalObjectStore, ObjectIo, ObjectStreamWriteError},
    *,
};
use oasis7_formal_protocol::{
    incremental_execution::RootSetV1, incremental_state::*, incremental_value::*,
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_field_member::*, incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_artifact_content::*,
};
use std::{
    io::Cursor,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "oasis7-artifact-content-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
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
        reads_remaining: 1_000_000,
        read_bytes_remaining: 4 << 30,
        writes_remaining: 1_000_000,
        write_bytes_remaining: 4 << 30,
    }
}
fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 4 << 30,
        write_bytes_remaining: 4 << 30,
    }
}
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100_000,
        leaves_remaining: 1000,
        read_bytes_remaining: 64 << 20,
    }
}
fn raw(length: u64) -> RuntimeArtifactOriginalWorkV1 {
    RuntimeArtifactOriginalWorkV1 {
        objects_remaining: 2,
        bytes_remaining: length,
        max_object_bytes: length,
    }
}
fn state_error(error: PageStoreError) -> ValueTreeError {
    match error {
        PageStoreError::Memory(e) => ValueTreeError::Memory(e),
        PageStoreError::ResourceLimited(s) => ValueTreeError::ResourceLimited(s),
        PageStoreError::Cancelled => ValueTreeError::Cancelled,
        PageStoreError::Io(e) => ValueTreeError::Io(e),
        _ => ValueTreeError::Invalid("artifact physical source"),
    }
}
struct Dual<'a, A, B>(&'a mut A, &'a mut B, &'a MemoryBudget);
impl<A: ValueNodeWriter, B: ValueNodeWriter> ValueNodeWriter for Dual<'_, A, B> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        let copy = self
            .2
            .reserve(bytes.len(), MemoryClass::Ordinary)?
            .retain(bytes.to_vec());
        self.0.put_node(id, bytes)?;
        self.1.put_node(id, copy)
    }
}
#[test]
fn world_genesis_source_only_raw_zstd_mirrors_drop_and_recovery_stream_large_originals() {
    const LENGTH: usize = 32 << 20;
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let paths = [
            directory.0.join("source"),
            directory.0.join("raw"),
            directory.0.join("zstd"),
        ];
        let memory = MemoryBudget::new(128 << 20, 4 << 20).unwrap();
        let stopped = AtomicBool::new(false);
        let roots = {
            let source =
                IncrementalPageStore::create(&paths[0], memory.clone(), compression).unwrap();
            let mut source_physical = physical();
            let mut source_logical = logical();
            let objects =
                IncrementalObjectStore::create(&source, &mut source_physical, &stopped).unwrap();
            let mut source_io = ObjectIo {
                physical: &mut source_physical,
                logical: &mut source_logical,
                class: MemoryClass::Ordinary,
                cancelled: &stopped,
            };
            let roots = {
                // Caller-owned functional World input is charged separately
                // from temporary storage work; this is not a C<T benchmark.
                let _input = memory
                    .reserve(2 * LENGTH + (8 << 20), MemoryClass::Ordinary)
                    .unwrap();
                let mut world = World::new();
                world.module_artifacts.clear();
                world.module_artifact_bytes.clear();
                world.module_artifacts.insert(String::new());
                world
                    .module_artifacts
                    .insert(" arbitrary registered 界 ".into());
                let bytes: Arc<[u8]> = Arc::from(
                    (0..LENGTH)
                        .map(|i| ((i.wrapping_mul(131) ^ i.wrapping_shr(7)) & 255) as u8)
                        .collect::<Vec<_>>(),
                );
                world
                    .module_artifact_bytes
                    .insert("raw-bytes".into(), bytes);
                world
                    .module_artifact_bytes
                    .insert(String::new(), Arc::from(&b""[..]));
                let _batch = memory.reserve(128 << 10, MemoryClass::Ordinary).unwrap();
                let mut rows = Vec::with_capacity(6);
                let mut values = source.value_session(physical());
                world
                    .prepare_iws_runtime_artifact_content_genesis_v1(
                        &mut MemberRecordWriteContext {
                            sink: &mut values,
                            memory: &memory,
                            io: &mut logical(),
                            key_work: &mut SemanticKeyBudget {
                                bytes_remaining: 1 << 20,
                            },
                            limits: MemberRecordLimitsV1 {
                                semantic_bytes: 4096,
                                payload_bytes: 4096,
                                record_bytes: 8192,
                            },
                            cancelled: || false,
                        },
                        &mut 6,
                        &mut raw(LENGTH as u64),
                        &mut |original, bytes| {
                            objects
                                .write_object(
                                    &mut Cursor::new(bytes),
                                    original.byte_len,
                                    Some(original.id),
                                    &mut source_io,
                                )
                                .map_err(state_error)?;
                            Ok(())
                        },
                        &mut |row| {
                            assert!(rows.len() < 6);
                            rows.push(row);
                            Ok(())
                        },
                    )
                    .unwrap();
                assert_eq!(rows.len(), 6);
                let workspace = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
                let mut writes: Vec<_> = rows.iter().map(|r| r.mutation.clone()).collect();
                writes.sort_by(|a, b| a.key.cmp(&b.key));
                let state = source
                    .state_session(physical(), MemoryClass::Ordinary)
                    .unwrap();
                let delta = workspace.retain(
                    StateDeltaV1::build(
                        StateRootV1::empty(StateNamespaceV1::Artifacts).unwrap(),
                        writes,
                        &state,
                        8 << 20,
                    )
                    .unwrap(),
                );
                values.put_state_pages(&delta, &memory).unwrap();
                let mut roots =
                    RootSetV1::empty("actual-artifact-content".into(), [49; 32]).unwrap();
                roots.state[5] = delta.after.clone();
                roots
            }; // Original World and all borrowed source bytes are now gone.
            let targets = [
                IncrementalPageStore::create(&paths[1], memory.clone(), PageCompression::Raw)
                    .unwrap(),
                IncrementalPageStore::create(&paths[2], memory.clone(), PageCompression::Zstd)
                    .unwrap(),
            ];
            let mut a_io = physical();
            let mut b_io = physical();
            let a_objects =
                IncrementalObjectStore::create(&targets[0], &mut a_io, &stopped).unwrap();
            let b_objects =
                IncrementalObjectStore::create(&targets[1], &mut b_io, &stopped).unwrap();
            let state = source
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let values = source.value_session(physical());
            let mut reads = logical();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut ctx = WorldFieldReadContextV1 {
                root: &roots.state[0],
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut reads,
                },
                key_work: &mut keys,
                semantic_bytes: 4096,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            };
            let marked = {
                let mut never_a = || false;
                let mut never_b = || false;
                let mut a = targets[0]
                    .buffered_page_writer(&mut a_io, &stopped, &mut never_a)
                    .unwrap();
                let mut b = targets[1]
                    .buffered_page_writer(&mut b_io, &stopped, &mut never_b)
                    .unwrap();
                let result = transfer_runtime_artifact_content_state_pages_v1(
                    &roots,
                    &mut ctx,
                    &mut objects.original_reader(&mut source_io),
                    &mut RuntimeArtifactContentPagesWorkV1 {
                        outer_scan: &mut scan(),
                        raw: &mut raw(LENGTH as u64),
                        max_pages: 6 * (257 + 2),
                        max_originals: 2,
                    },
                    &mut Dual(&mut a, &mut b, &memory),
                )
                .unwrap();
                a.finish().unwrap();
                b.finish().unwrap();
                result
            };
            assert_eq!(marked.original_count(), 2);
            let mut a_logical = logical();
            let mut b_logical = logical();
            marked
                .visit_originals::<ObjectStreamWriteError<PageStoreError>>(|original| {
                    for (target, physical, logical) in [
                        (&a_objects, &mut a_io, &mut a_logical),
                        (&b_objects, &mut b_io, &mut b_logical),
                    ] {
                        target.write_object_from_stream(
                            original.byte_len,
                            original.id,
                            &mut ObjectIo {
                                physical,
                                logical,
                                class: MemoryClass::Ordinary,
                                cancelled: &stopped,
                            },
                            &mut || false,
                            |mut output| {
                                objects
                                    .read_object_into(
                                        &original.id,
                                        original.byte_len,
                                        &mut output,
                                        &mut source_io,
                                    )
                                    .map(|_| ())
                            },
                        )?;
                    }
                    Ok(())
                })
                .unwrap();
            marked
                .visit_pages::<ValueTreeError>(|id| {
                    for target in &targets {
                        let value = target.value_session(physical()).read_node(
                            id,
                            8192,
                            &memory,
                            MemoryClass::Ordinary,
                        )?;
                        assert_eq!(blake3::hash(&value).as_bytes(), id);
                    }
                    Ok(())
                })
                .unwrap();
            roots
        };
        assert_eq!(memory.used(), 0);
        drop(memory);
        for (path, compression) in [
            (&paths[1], PageCompression::Raw),
            (&paths[2], PageCompression::Zstd),
        ] {
            let cold = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
            let store =
                IncrementalPageStore::open(path, cold.clone(), compression, MemoryClass::Recovery)
                    .unwrap();
            let mut object_physical = physical();
            let mut object_logical = logical();
            let objects = IncrementalObjectStore::open(
                &store,
                MemoryClass::Recovery,
                &mut object_physical,
                &stopped,
            )
            .unwrap();
            let _ordinary = cold
                .reserve((4 << 20) - cold.used(), MemoryClass::Ordinary)
                .unwrap();
            assert!(cold.reserve(1, MemoryClass::Ordinary).is_err());
            {
                let state = store
                    .state_session(physical(), MemoryClass::Recovery)
                    .unwrap();
                let values = store.value_session(physical());
                let mut reads = logical();
                let mut keys = SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                };
                qualify_runtime_artifact_content_v1(
                    &roots,
                    &mut WorldFieldReadContextV1 {
                        root: &roots.state[0],
                        state: &state,
                        values: ValueReadContext {
                            source: &values,
                            memory: &cold,
                            class: MemoryClass::Recovery,
                            io: &mut reads,
                        },
                        key_work: &mut keys,
                        semantic_bytes: 4096,
                        record_bytes: 8192,
                        take_state_error: || state.take_read_error().map(state_error),
                        cancelled: || false,
                    },
                    &mut scan(),
                    &mut objects.original_reader(&mut ObjectIo {
                        physical: &mut object_physical,
                        logical: &mut object_logical,
                        class: MemoryClass::Recovery,
                        cancelled: &stopped,
                    }),
                    &mut raw(LENGTH as u64),
                )
                .unwrap()
                .validate_binding(&roots)
                .unwrap();
            }
            drop(_ordinary);
            continue_storage_after_recovery(
                &store,
                &objects,
                &cold,
                &roots,
                &stopped,
                LENGTH as u64,
            );
        }
    }
}

struct NoOldOriginalReads<'a> {
    memory: &'a MemoryBudget,
    calls: usize,
}
impl oasis7_formal_protocol::incremental_material::OriginalObjectReader for NoOldOriginalReads<'_> {
    type Error = PageStoreError;
    fn allocation_budget(&self) -> &MemoryBudget {
        self.memory
    }
    fn allocation_class(&self) -> MemoryClass {
        MemoryClass::Ordinary
    }
    fn read_original(
        &mut self,
        _: &[u8; 32],
        _: usize,
        _: &MemoryBudget,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, PageStoreError> {
        self.calls += 1;
        Err(PageStoreError::Corrupt("unchanged original was read"))
    }
}
impl oasis7_formal_protocol::incremental_material::OriginalObjectStreamReader
    for NoOldOriginalReads<'_>
{
    fn stream_original(
        &mut self,
        _: &[u8; 32],
        _: u64,
        _: &MemoryBudget,
        _: &mut dyn std::io::Write,
    ) -> Result<u64, PageStoreError> {
        self.calls += 1;
        Err(PageStoreError::Corrupt("unchanged original was streamed"))
    }
}
fn continue_storage_after_recovery(
    store: &IncrementalPageStore,
    objects: &IncrementalObjectStore<'_>,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    stopped: &AtomicBool,
    length: u64,
) {
    use oasis7_formal_protocol::incremental_execution::*;
    use oasis7_world_state::incremental_field_overlay::WorldFieldOverlayContextV1;
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut raw_physical = physical();
    let mut raw_logical = logical();
    let mut object_io = ObjectIo {
        physical: &mut raw_physical,
        logical: &mut raw_logical,
        class: MemoryClass::Ordinary,
        cancelled: stopped,
    };
    let parent_proof = {
        let mut reads = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        qualify_runtime_artifact_content_v1(
            roots,
            &mut WorldFieldReadContextV1 {
                root: &roots.state[0],
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut reads,
                },
                key_work: &mut keys,
                semantic_bytes: 4096,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            },
            &mut scan(),
            &mut objects.original_reader(&mut object_io),
            &mut raw(length),
        )
        .unwrap()
    };
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [57; 32],
        authority: AuthorityBinding {
            token: [58; 32],
            epoch: 7,
        },
    };
    let live = parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [59; 32],
        &state,
        memory,
        TransactionLimits {
            max_reads: 16,
            max_writes: 4,
            max_facts: 4,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    let mut writer = store.value_session(physical());
    {
        let mut reads = logical();
        let mut writes = logical();
        let mut read_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let guard = tx.guard();
        assert!(
            change_runtime_artifact_content_overlay_v1(
                RuntimeArtifactContentChangeV1::Registered {
                    key: "after-recovery",
                    expected: false,
                    after: true
                },
                &mut WorldFieldOverlayContextV1 {
                    transaction: &mut tx,
                    values: ValueReadContext {
                        source: &values,
                        memory,
                        class: MemoryClass::Ordinary,
                        io: &mut reads
                    },
                    key_work: &mut read_keys,
                    semantic_bytes: 4096,
                    record_bytes: 8192,
                    take_state_error: || state.take_read_error().map(state_error),
                    cancelled: || false
                },
                &mut MemberRecordWriteContext {
                    sink: &mut writer,
                    memory,
                    io: &mut writes,
                    key_work: &mut write_keys,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 4096,
                        payload_bytes: 4096,
                        record_bytes: 8192
                    },
                    cancelled: || false
                },
                &mut raw(0),
                &mut |_, _| panic!("registration must not import raw content"),
                &guard,
                &live
            )
            .unwrap()
        );
    }
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, 1).unwrap();
    assert_eq!(prepared.record.writes.len(), 1);
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [59; 32],
        source: &state,
        memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let verified = verifier.verify(&prepared.record).unwrap();
    let mut unchanged = NoOldOriginalReads { memory, calls: 0 };
    {
        let mut reads = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        accept_runtime_artifact_content_transition_v1(
            &parent_proof,
            &verifier,
            &prepared.record,
            &mut WorldFieldReadContextV1 {
                root: &prepared.record.after.state[0],
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory,
                    class: MemoryClass::Ordinary,
                    io: &mut reads,
                },
                key_work: &mut keys,
                semantic_bytes: 4096,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            },
            &mut unchanged,
            &mut raw(0),
        )
        .unwrap()
        .validate_binding(&prepared.record.after)
        .unwrap();
    }
    assert_eq!(unchanged.calls, 0);
    for node in &verified.new_nodes {
        let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
        let charge = memory
            .reserve(node.bytes.len(), MemoryClass::Ordinary)
            .unwrap();
        writer
            .put_node(id, charge.retain(node.bytes.clone()))
            .unwrap();
    }
    let reopened = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let mut reads = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    qualify_runtime_artifact_content_v1(
        &prepared.record.after,
        &mut WorldFieldReadContextV1 {
            root: &prepared.record.after.state[0],
            state: &reopened,
            values: ValueReadContext {
                source: &values,
                memory,
                class: MemoryClass::Ordinary,
                io: &mut reads,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || reopened.take_read_error().map(state_error),
            cancelled: || false,
        },
        &mut scan(),
        &mut objects.original_reader(&mut object_io),
        &mut raw(length),
    )
    .unwrap()
    .validate_binding(&prepared.record.after)
    .unwrap();
}
