use super::*;
use oasis7_formal_protocol::incremental_execution::RootSetV1;
use oasis7_runtime::{incremental_cognition_family::*, incremental_json_records::JsonReadWorkV1};
use std::{path::Path, sync::atomic::AtomicBool};
pub fn transfer_and_reopen(source_path: &Path, target_path: &Path, roots: &RootSetV1) {
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
            &mut sink,
        )
        .unwrap();
        assert!(sink.pending_reserved_bytes() > 0);
        // A collected set is provisional until the actual final pack is durable.
        sink.finish().unwrap();
        assert!(pages.page_count() > 1000);
        pages
            .qualification()
            .validate_binding(&roots.digest().unwrap())
            .unwrap();
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
    }
    let cold = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let target = IncrementalPageStore::open(
        target_path,
        cold.clone(),
        PageCompression::Raw,
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
