use super::*;
use oasis7_formal_protocol::incremental_execution::RootSetV1;
use oasis7_runtime::{incremental_cognition_family::*, incremental_json_records::JsonReadWorkV1};
use std::{path::Path, sync::atomic::AtomicBool};
pub fn transfer_and_reopen(source_path: &Path, target_path: &Path, roots: &RootSetV1) {
    let cancelled_path = target_path.with_extension("cancelled-zstd");
    {
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let source = IncrementalPageStore::open(
            source_path,
            memory.clone(),
            PageCompression::Raw,
            MemoryClass::Ordinary,
        )
        .unwrap();
        let target =
            IncrementalPageStore::create(target_path, memory.clone(), PageCompression::Raw)
                .unwrap();
        let second =
            IncrementalPageStore::create(&cancelled_path, memory.clone(), PageCompression::Zstd)
                .unwrap();
        let state = source
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = source.value_session(physical());
        let mut io = fixture::logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
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
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let mut scan = StateScanBudget {
            nodes_remaining: 100_000,
            read_bytes_remaining: 64 << 20,
            leaves_remaining: 1000,
        };
        let mut json = JsonReadWorkV1 {
            cells_remaining: 4096,
            json_nodes_remaining: 512,
            max_depth: 4,
            max_payload_bytes: 4096,
        };
        let mut cells = 1000;
        let cancelled = AtomicBool::new(false);
        let mut output_io = physical();
        let mut external = || false;
        let limited = mark_cognition_current_pages_v1(
            roots,
            &mut context,
            &mut CognitionCurrentPagesWorkV1 {
                outer_scan: &mut scan,
                json: &mut json,
                cells: &mut cells,
                max_pages: 0,
            },
        );
        assert!(matches!(
            limited,
            Err(ValueTreeError::ResourceLimited(
                "cognition current marked pages"
            ))
        ));
        let final_phase = std::cell::Cell::new(false);
        let baseline = std::cell::Cell::new(0usize);
        let recipes = second.root().join("recipes");
        let recipe_count = || std::fs::read_dir(&recipes).unwrap().count();
        let mut stop_after_real_publish = || final_phase.get() && recipe_count() > baseline.get();
        let mut second_io = physical();
        let mut second_sink = second
            .buffered_page_writer(&mut second_io, &cancelled, &mut stop_after_real_publish)
            .unwrap();
        let mut sink = target
            .buffered_page_writer(&mut output_io, &cancelled, &mut external)
            .unwrap();
        let pages = transfer_cognition_current_pages_v1(
            roots,
            &mut context,
            &mut CognitionCurrentPagesWorkV1 {
                outer_scan: &mut scan,
                json: &mut json,
                cells: &mut cells,
                max_pages: 65_536,
            },
            &mut Mirror {
                first: &mut sink,
                second: &mut second_sink,
                memory: &memory,
            },
        )
        .unwrap();
        assert!(sink.pending_reserved_bytes() > 0);
        assert!(second_sink.pending_reserved_bytes() > 0);
        // A collected set is provisional until the actual final pack is durable.
        sink.finish().unwrap();
        assert!(pages.page_count() > 1000);
        pages
            .qualification()
            .validate_binding(&roots.digest().unwrap())
            .unwrap();
        let id = roots.digest().unwrap();
        let feedback = pages
            .qualification()
            .json_entry(CognitionJsonFieldV1::FeedbackOutbox, &id)
            .unwrap()
            .unwrap();
        cognition_missing_child::exercise(&source, &memory, roots, &feedback.body_root().node_root);
        let target_values = target.value_session(physical());
        pages
            .visit_pages(|id| {
                let bytes = target_values
                    .read_node(id, BUFFERED_PAGE_MAX_BYTES, &memory, MemoryClass::Ordinary)
                    .unwrap();
                assert_eq!(blake3::hash(&bytes).as_bytes(), id);
                Ok::<_, ()>(())
            })
            .unwrap();
        eprintln!(
            "Cognition typed transfer unique_pages={} output_remaining={output_io:?}",
            pages.page_count()
        );
        baseline.set(recipe_count());
        final_phase.set(true);
        // A real new final recipe is published before cancellation. No
        // qualification escapes the combined buffered completion operation.
        let completed = second_sink.finish().map(|()| pages);
        assert!(matches!(completed, Err(PageStoreError::Cancelled)));
        assert!(recipe_count() > baseline.get());
    }
    for (target_path, compression) in [
        (target_path, PageCompression::Raw),
        (cancelled_path.as_path(), PageCompression::Zstd),
    ] {
        let cold = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
        let target = IncrementalPageStore::open(
            target_path,
            cold.clone(),
            compression,
            MemoryClass::Recovery,
        )
        .unwrap();
        let _root_charge = cold.reserve(32 << 10, MemoryClass::Recovery).unwrap();
        let state = target
            .state_session(physical(), MemoryClass::Recovery)
            .unwrap();
        let values = target.value_session(physical());
        let mut io = fixture::logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let mut context = WorldFieldReadContextV1 {
            root: &roots.state[0],
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &cold,
                class: MemoryClass::Recovery,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        };
        let family = qualify_cognition_current_state_v1(
            roots,
            &mut context,
            &mut StateScanBudget {
                nodes_remaining: 100_000,
                read_bytes_remaining: 64 << 20,
                leaves_remaining: 1000,
            },
            &mut JsonReadWorkV1 {
                cells_remaining: 4096,
                json_nodes_remaining: 512,
                max_depth: 4,
                max_payload_bytes: 4096,
            },
            &mut 1000,
        )
        .unwrap();
        family.validate_binding(&roots.digest().unwrap()).unwrap();
        assert!(
            family
                .json_entry(
                    CognitionJsonFieldV1::FeedbackOutbox,
                    &roots.digest().unwrap()
                )
                .unwrap()
                .is_some()
        );
    }
}

struct Mirror<'a, A, B> {
    first: &'a mut A,
    second: &'a mut B,
    memory: &'a MemoryBudget,
}
impl<A: ValueNodeWriter, B: ValueNodeWriter> ValueNodeWriter for Mirror<'_, A, B> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: oasis7_proto::memory_budget::ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        let copy = self
            .memory
            .reserve(bytes.len(), MemoryClass::Ordinary)?
            .retain(bytes.to_vec());
        self.first.put_node(id, bytes)?;
        self.second.put_node(id, copy)
    }
}
