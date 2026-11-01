use super::*;
use oasis7_proto::memory_budget::ReservedValue;
use std::sync::atomic::AtomicBool;
struct Mirror<'a, A, B> {
    first: &'a mut A,
    second: &'a mut B,
    memory: &'a MemoryBudget,
}
impl<A: ValueNodeWriter, B: ValueNodeWriter> ValueNodeWriter for Mirror<'_, A, B> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        let copy = self
            .memory
            .reserve(bytes.len(), MemoryClass::Ordinary)?
            .retain(bytes.to_vec());
        self.first.put_node(id, bytes)?;
        self.second.put_node(id, copy)
    }
}
fn transfer_pair(
    paths: &[std::path::PathBuf; 2],
    total_bytes: usize,
    expect_memory_limit: bool,
    cancel_final: bool,
) -> Option<RootSetV1> {
    let roots = {
        let memory = MemoryBudget::new(total_bytes, 4 << 20).unwrap();
        // GEN uses a separate larger source pool which is destroyed before
        // the continuous bounded transfer starts.
        let source_path = paths[0].with_extension("source");
        let roots = {
            let producer = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
            let store =
                IncrementalPageStore::create(&source_path, producer.clone(), PageCompression::Raw)
                    .unwrap();
            seed(&store, &producer)
        };
        let source = IncrementalPageStore::open(
            &source_path,
            memory.clone(),
            PageCompression::Raw,
            MemoryClass::Ordinary,
        )
        .unwrap();
        let first =
            IncrementalPageStore::create(&paths[0], memory.clone(), PageCompression::Raw).unwrap();
        let second =
            IncrementalPageStore::create(&paths[1], memory.clone(), PageCompression::Zstd).unwrap();
        let state = source
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = source.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut context = WorldFieldReadContextV1 {
            root: &roots.state[0],
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let stopped = AtomicBool::new(false);
        let mut never1 = || false;
        let final_phase = std::cell::Cell::new(false);
        let before_recipes = std::cell::Cell::new(0usize);
        let recipe_path = second.root().join("recipes");
        let recipe_count = || std::fs::read_dir(&recipe_path).unwrap().count();
        let mut never2 = || final_phase.get() && recipe_count() > before_recipes.get();
        let mut first_io = physical();
        let mut second_io = physical();
        let mut first_writer = first
            .buffered_page_writer(&mut first_io, &stopped, &mut never1)
            .unwrap();
        let mut second_writer = second
            .buffered_page_writer(&mut second_io, &stopped, &mut never2)
            .unwrap();
        let limited = mark_runtime_capability_family_pages_v1(
            &roots,
            &mut context,
            &mut RuntimeCapabilityPagesWorkV1 {
                outer_scan: &mut scan(),
                inner_scan: &mut scan(),
                cells: &mut 8192,
                max_payload_bytes: 165,
                max_pages: 0,
            },
        );
        assert!(matches!(limited, Err(ValueTreeError::ResourceLimited(_))));
        let result = transfer_runtime_capability_family_pages_v1(
            &roots,
            &mut context,
            &mut RuntimeCapabilityPagesWorkV1 {
                outer_scan: &mut scan(),
                inner_scan: &mut scan(),
                cells: &mut 8192,
                max_payload_bytes: 165,
                // 72 logical leaves: <=257 sparse-state pages each, plus one
                // Value descriptor + leaf each (every member frame <4096 <64KiB).
                max_pages: 72 * (257 + 2),
            },
            &mut Mirror {
                first: &mut first_writer,
                second: &mut second_writer,
                memory: &memory,
            },
        );
        if expect_memory_limit {
            assert!(matches!(result, Err(ValueTreeError::Memory(_))));
            return None;
        }
        let result = result.unwrap();
        result
            .qualification()
            .validate_binding(&roots.digest().unwrap())
            .unwrap();
        assert!(result.page_count() > 0);
        eprintln!(
            "capability pages actual={} structural bound={}",
            result.page_count(),
            72 * (257 + 2)
        );
        // A completed typed traversal is still provisional while either
        // writer retains pages. Both final flushes are mandatory.
        assert!(first_writer.pending_reserved_bytes() > 0);
        assert!(second_writer.pending_reserved_bytes() > 0);
        first_writer.finish().unwrap();
        before_recipes.set(recipe_count());
        final_phase.set(cancel_final);
        let finished = second_writer.finish();
        if cancel_final {
            assert!(matches!(
                finished,
                Err(oasis7_distfs::incremental_page_store::PageStoreError::Cancelled)
            ));
            assert!(recipe_count() > before_recipes.get());
        } else {
            finished.unwrap();
        }
        let source_pages = source.value_session(physical());
        let first_pages = first.value_session(physical());
        let second_pages = second.value_session(physical());
        result
            .visit_pages(|id| -> Result<(), ValueTreeError> {
                let original = source_pages.read_node(
                    id,
                    VALUE_MAX_NODE_BYTES,
                    &memory,
                    MemoryClass::Ordinary,
                )?;
                assert_eq!(blake3::hash(&original).as_bytes(), id);
                for target in [&first_pages, &second_pages] {
                    let got = target.read_node(
                        id,
                        VALUE_MAX_NODE_BYTES,
                        &memory,
                        MemoryClass::Ordinary,
                    )?;
                    assert_eq!(got.as_slice(), original.as_slice());
                }
                Ok(())
            })
            .unwrap();
        roots
    };
    Some(roots)
}
#[test]
fn typed_runtime_capability_pages_transfer_to_two_buffered_formats_then_cold_recover() {
    let directory = Directory::new();
    let paths = [
        directory.0.join("raw-target"),
        directory.0.join("zstd-target"),
    ];
    let negative_paths = [
        directory.0.join("limited-raw"),
        directory.0.join("limited-zstd"),
    ];
    // This is a generic bounded-memory negative, not an encoder-stage claim.
    assert!(transfer_pair(&negative_paths, 8 << 20, true, false).is_none());
    let roots = transfer_pair(&paths, 32 << 20, false, false).unwrap();
    let cancelled_paths = [
        directory.0.join("cancelled-raw"),
        directory.0.join("cancelled-zstd"),
    ];
    let cancelled_roots = transfer_pair(&cancelled_paths, 32 << 20, false, true).unwrap();
    assert_eq!(roots, cancelled_roots);
    // The cancelled destination has no completion result. A fresh receiver can
    // independently resume by proving its already durable complete root closure.
    // Original World, both transfer writers, source handle and all original
    // pools are gone. Each persisted format receives a new root-only pool.
    for (path, compression) in paths.iter().chain(cancelled_paths.iter()).zip([
        PageCompression::Raw,
        PageCompression::Zstd,
        PageCompression::Raw,
        PageCompression::Zstd,
    ]) {
        let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let store =
            IncrementalPageStore::open(path, memory.clone(), compression, MemoryClass::Recovery)
                .unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Recovery)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut context = WorldFieldReadContextV1 {
            root: &roots.state[0],
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let proof = qualify_runtime_capability_family_v1(
            &roots,
            &mut context,
            &mut scan(),
            &mut scan(),
            &mut 8192,
            165,
        )
        .unwrap();
        proof.validate_binding(&roots.digest().unwrap()).unwrap();
        context.root = roots
            .state
            .iter()
            .find(|r| r.namespace == StateNamespaceV1::Authorization)
            .unwrap();
        let key = RuntimeCapabilityRecordKeyV1 {
            kind: RuntimeCapabilityRecordKindV1::Budget,
            id: "agent-key",
        };
        let record = open_qualified_runtime_capability_record_v1(key, &roots, &proof, &mut context)
            .unwrap()
            .unwrap();
        let value = read_qualified_runtime_capability_cell_v1(
            RuntimeCapabilityCellRequestV1 {
                key,
                column: RuntimeCapabilityColumnV1::RemainingUnits,
                qualification: &record,
            },
            &roots,
            &mut context,
            165,
            &mut std::io::sink(),
        )
        .unwrap();
        assert_eq!(value, RuntimeCapabilityValueV1::Signed(i64::MIN));
    }
}
