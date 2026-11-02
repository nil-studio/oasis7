use super::*;
use oasis7_distfs::incremental_page_store::{
    PageIoBudget, PageStoreError, objects::ObjectStreamWriteError,
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionError, ExecutionTransaction, TransactionLimits},
    incremental_value::{ValueIoBudget, ValueTreeError},
};
use oasis7_runtime::{
    AgentEventContext, AgentEventOriginalStreamV1, AgentEventOriginalV1, BodyMutationContext,
    BodyMutationError, DomainEvent, WorldEventBody,
};
use std::{cell::Cell, io::Write, sync::atomic::Ordering};
fn whole_physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 4_000_000,
        read_bytes_remaining: 512 << 20,
        writes_remaining: 1_000_000,
        write_bytes_remaining: 512 << 20,
    }
}
fn whole_logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 1_000_000,
        read_bytes_remaining: 512 << 20,
        write_bytes_remaining: 512 << 20,
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 16,
        max_writes: 16,
        max_facts: 4,
        max_pending_node_bytes: 4 << 20,
    }
}
struct HashOutput {
    hash: blake3::Hasher,
    length: u64,
}
impl Write for HashOutput {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.hash.update(b);
        self.length += b.len() as u64;
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[test]
fn actual_public_agent_event_streams_large_originals_then_reopens_in_small_recovery_pool() {
    for length in [32 << 20, 64 << 20] {
        let directory = Directory::new();
        let root = directory.0.join("original-large");
        let (id, encoded) = {
            let input = MemoryBudget::new(128 << 20, 4 << 20).unwrap();
            let _input = input
                .reserve(length + (2 << 20), MemoryClass::Ordinary)
                .unwrap();
            let (world, parent) = {
                let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
                let store =
                    IncrementalPageStore::create(&root, memory.clone(), PageCompression::Raw)
                        .unwrap();
                fixture(&store, &memory)
            };
            let mut event = event(&world, 1, false);
            let WorldEventBody::Domain(DomainEvent::BodyAttributesUpdated { reason, .. }) =
                &mut event.body
            else {
                unreachable!()
            };
            *reason = "r".repeat(length);
            let memory = MemoryBudget::new(16 << 20, 4 << 20).unwrap();
            let store = IncrementalPageStore::open(
                &root,
                memory.clone(),
                PageCompression::Raw,
                MemoryClass::Ordinary,
            )
            .unwrap();
            let source = store
                .state_session(whole_physical(), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(whole_physical());
            let mut sink = store.value_session(whole_physical());
            let cancelled = AtomicBool::new(false);
            let mut physical = whole_physical();
            let mut logical = whole_logical();
            let objects = IncrementalObjectStore::open(
                &store,
                MemoryClass::Ordinary,
                &mut physical,
                &cancelled,
            )
            .unwrap();
            let live = parent.fence().unwrap();
            let mut tx =
                ExecutionTransaction::begin(parent.clone(), [4; 32], &source, &memory, limits())
                    .unwrap();
            let mut work = original_work();
            work.bytes_remaining = length as u64 * 3 + (1 << 20);
            let original_length = Cell::new(0);
            let before = world.state().agents["alice"].state.body_view.clone();
            world
                .contribute_iws_agent_event(
                    &event,
                    &mut tx,
                    &mut AgentEventContext {
                        values: &values,
                        mutation: BodyMutationContext {
                            sink: &mut sink,
                            memory: &memory,
                            io: &mut support::logical(),
                            cancelled: || false,
                        },
                        persist_original: |original: &mut dyn AgentEventOriginalV1| {
                            original_length.set(original.original_bytes());
                            support::persist_original(
                                original,
                                &objects,
                                &mut ObjectIo {
                                    physical: &mut physical,
                                    logical: &mut logical,
                                    class: MemoryClass::Ordinary,
                                    cancelled: &cancelled,
                                },
                            )
                        },
                        max_event_bytes: length + 4096,
                        original_work: work,
                    },
                )
                .unwrap();
            let guard = tx.guard();
            let prepared = tx.seal(&guard, &live, 0).unwrap();
            TransitionVerifier {
                parent: &parent,
                operation: [4; 32],
                source: &source,
                memory: &memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 4 << 20,
            }
            .verify(&prepared.record)
            .unwrap();
            assert_eq!(world.state().agents["alice"].state.body_view, before);
            assert_eq!(prepared.record.facts.len(), 1);
            let id = prepared.record.facts[0].id;
            eprintln!(
                "actual Agent original input={length} encoded={} C=16MiB physical={physical:?} logical={logical:?}",
                original_length.get()
            );
            (id, original_length.get())
        };
        // No borrowed DTO, source World, previous page handle or operation pool
        // survives into this original-ID-only cold read.
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let store = IncrementalPageStore::open(
            &root,
            memory.clone(),
            PageCompression::Raw,
            MemoryClass::Recovery,
        )
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let mut physical = whole_physical();
        let mut logical = whole_logical();
        let objects =
            IncrementalObjectStore::open(&store, MemoryClass::Recovery, &mut physical, &cancelled)
                .unwrap();
        let _output = memory.reserve(8 << 10, MemoryClass::Recovery).unwrap();
        let mut output = HashOutput {
            hash: blake3::Hasher::new(),
            length: 0,
        };
        let read = objects
            .read_object_into(
                &id,
                encoded,
                &mut output,
                &mut ObjectIo {
                    physical: &mut physical,
                    logical: &mut logical,
                    class: MemoryClass::Recovery,
                    cancelled: &cancelled,
                },
            )
            .unwrap();
        assert_eq!(read, encoded);
        assert_eq!(output.length, encoded);
        assert_eq!(*output.hash.finalize().as_bytes(), id);
    }
}
fn recipes(path: &std::path::Path) -> usize {
    std::fs::read_dir(path.join("recipes")).unwrap().count()
}
#[test]
fn actual_original_last_flush_cancellation_and_io_quota_abort_all_outer_state() {
    for mode in 0..3 {
        let directory = Directory::new();
        let root = directory.0.join("original-final-flush");
        let (world, parent) = {
            let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
            let store =
                IncrementalPageStore::create(&root, memory.clone(), PageCompression::Raw).unwrap();
            fixture(&store, &memory)
        };
        let memory = MemoryBudget::new(16 << 20, 4 << 20).unwrap();
        let store = IncrementalPageStore::open(
            &root,
            memory.clone(),
            PageCompression::Raw,
            MemoryClass::Ordinary,
        )
        .unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut sink = store.value_session(physical());
        let atomic = AtomicBool::new(false);
        let phase = Cell::new(false);
        let baseline = Cell::new(usize::MAX);
        let mut physical_io = physical();
        let mut logical_io = logical();
        let objects =
            IncrementalObjectStore::open(&store, MemoryClass::Ordinary, &mut physical_io, &atomic)
                .unwrap();
        if mode == 2 {
            physical_io.writes_remaining = 0;
        }
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent, [4; 32], &state, &memory, limits()).unwrap();
        tx.write(Ns::Business, b"successful-outer-prefix", Some([9; 32]))
            .unwrap();
        let expected = Cell::new([0; 32]);
        let result = world.contribute_iws_agent_event(
            &event(&world, 1, false),
            &mut tx,
            &mut AgentEventContext {
                values: &values,
                mutation: BodyMutationContext {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut support::logical(),
                    cancelled: || mode == 0 && phase.get() && recipes(&root) > baseline.get(),
                },
                persist_original: |original: &mut dyn AgentEventOriginalV1| {
                    expected.set(original.original_id());
                    original.persist_to(
                        &mut |stream: &mut dyn AgentEventOriginalStreamV1,
                              cancelled: &mut dyn FnMut() -> bool| {
                            let mut combined = || {
                                let stop = cancelled();
                                if mode == 1 && phase.get() && recipes(&root) > baseline.get() {
                                    atomic.store(true, Ordering::Release);
                                }
                                stop
                            };
                            objects
                                .write_object_from_stream(
                                    stream.original_bytes(),
                                    stream.original_id(),
                                    &mut ObjectIo {
                                        physical: &mut physical_io,
                                        logical: &mut logical_io,
                                        class: MemoryClass::Ordinary,
                                        cancelled: &atomic,
                                    },
                                    &mut combined,
                                    |out| {
                                        stream.write_to(out)?;
                                        baseline.set(recipes(&root));
                                        phase.set(true);
                                        Ok(())
                                    },
                                )
                                .map(|_| ())
                                .map_err(|e| match e {
                                    ObjectStreamWriteError::Producer(e) => e,
                                    ObjectStreamWriteError::Store(PageStoreError::Cancelled) => {
                                        ValueTreeError::Cancelled.into()
                                    }
                                    ObjectStreamWriteError::Store(
                                        PageStoreError::ResourceLimited(s),
                                    ) => ExecutionError::ResourceLimited(s).into(),
                                    ObjectStreamWriteError::Store(e) => {
                                        BodyMutationError::Original(Box::new(e))
                                    }
                                })
                        },
                    )
                },
                max_event_bytes: 4096,
                original_work: original_work(),
            },
        );
        if mode < 2 {
            assert!(
                phase.get() && recipes(&root) > baseline.get(),
                "must cancel after this attempt really published a new recipe"
            );
            assert!(
                matches!(
                    result,
                    Err(BodyMutationError::Value(ValueTreeError::Cancelled))
                ),
                "mode={mode} result={result:?}"
            );
            assert!(
                !root
                    .join("objects")
                    .join(hex::encode(expected.get()))
                    .exists(),
                "cancelled final pack must not publish an original locator"
            );
        } else {
            assert!(
                matches!(
                    result,
                    Err(BodyMutationError::Execution(
                        ExecutionError::ResourceLimited(_)
                    ))
                ),
                "result={result:?}"
            );
        }
        let guard = tx.guard();
        assert!(matches!(
            tx.seal(&guard, &live, 0),
            Err(ExecutionError::Aborted)
        ));
    }
}

#[test]
fn fake_completed_sink_does_not_create_a_readable_fact_original() {
    use oasis7_formal_protocol::incremental_material::OriginalObjectStreamReader;
    let directory = Directory::new();
    let root = directory.0.join("fake-original-sink");
    let (world, parent) = {
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let store =
            IncrementalPageStore::create(&root, memory.clone(), PageCompression::Raw).unwrap();
        fixture(&store, &memory)
    };
    let memory = MemoryBudget::new(16 << 20, 4 << 20).unwrap();
    let store = IncrementalPageStore::open(
        &root,
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Ordinary,
    )
    .unwrap();
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical());
    let mut sink = store.value_session(physical());
    let live = parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(parent, [4; 32], &state, &memory, limits()).unwrap();
    world
        .contribute_iws_agent_event(
            &event(&world, 1, false),
            &mut tx,
            &mut AgentEventContext {
                values: &values,
                mutation: BodyMutationContext {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut support::logical(),
                    cancelled: || false,
                },
                persist_original: |producer: &mut dyn AgentEventOriginalV1| {
                    producer.persist_to(
                        &mut |stream: &mut dyn AgentEventOriginalStreamV1,
                              _: &mut dyn FnMut() -> bool| {
                            stream.write_to(&mut std::io::sink())
                        },
                    )
                },
                max_event_bytes: 4096,
                original_work: original_work(),
            },
        )
        .unwrap();
    let guard = tx.guard();
    let provisional = tx.seal(&guard, &live, 0).unwrap();
    assert_eq!(provisional.record.facts.len(), 1);
    let id = provisional.record.facts[0].id;
    let cancelled = AtomicBool::new(false);
    let mut physical = whole_physical();
    let mut logical = whole_logical();
    let objects =
        IncrementalObjectStore::open(&store, MemoryClass::Ordinary, &mut physical, &cancelled)
            .unwrap();
    let mut io = ObjectIo {
        physical: &mut physical,
        logical: &mut logical,
        class: MemoryClass::Ordinary,
        cancelled: &cancelled,
    };
    let result =
        objects
            .original_reader(&mut io)
            .stream_original(&id, 4096, &memory, &mut std::io::sink());
    assert!(matches!(result,Err(PageStoreError::Missing(missing)) if missing==id));
    assert!(!root.join("objects").join(hex::encode(id)).exists());
}
