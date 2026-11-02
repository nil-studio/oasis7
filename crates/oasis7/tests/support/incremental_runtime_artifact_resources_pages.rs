use super::*;
use oasis7_formal_protocol::incremental_state::StateScanBudget;
use oasis7_runtime::incremental_runtime_artifact_resources::{
    RuntimeArtifactResourcesPagesWorkV1, RuntimeArtifactResourcesReadContextV1,
    mark_runtime_artifact_resources_pages_v1, transfer_runtime_artifact_resources_pages_v1,
};
use std::sync::atomic::AtomicBool;

fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 256 << 20,
        leaves_remaining: 100_000,
    }
}

fn context<'a>(
    memory: &'a MemoryBudget,
    state: &'a oasis7_distfs::incremental_page_store::StatePageSession<'a>,
    values: &'a oasis7_distfs::incremental_page_store::ValuePageSession<'a>,
    io: &'a mut oasis7_formal_protocol::incremental_value::ValueIoBudget,
    key_work: &'a mut SemanticKeyBudget,
    scans: &'a mut [StateScanBudget; RUNTIME_ARTIFACT_RESOURCES_SCAN_DEPTH_V1],
) -> RuntimeArtifactResourcesReadContextV1<
    'a,
    oasis7_distfs::incremental_page_store::StatePageSession<'a>,
    oasis7_distfs::incremental_page_store::ValuePageSession<'a>,
    impl FnMut() -> Option<ValueTreeError> + 'a,
    impl FnMut() -> bool + 'a,
> {
    RuntimeArtifactResourcesReadContextV1 {
        state,
        values: ValueReadContext {
            source: values,
            memory,
            class: MemoryClass::Ordinary,
            io,
        },
        key_work,
        semantic_bytes: 64 << 20,
        record_bytes: 16 << 20,
        max_payload_bytes: 8 << 20,
        scans,
        take_state_error: || state.take_read_error().map(typed),
        cancelled: || false,
    }
}

fn counts() -> [StateScanBudget; RUNTIME_ARTIFACT_RESOURCES_SCAN_DEPTH_V1] {
    std::array::from_fn(|_| scan())
}

fn closure_physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 1_000_000,
        writes_remaining: 1_000_000,
        ..physical()
    }
}

#[test]
fn typed_artifact_pages_mark_and_transfer_include_child_state_roots() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let directory = Directory::new();
        let source_path = directory.0.join("source");
        let target_path = directory.0.join("target");
        let (manifest, delta, builtins) = rich_sources();
        let builtin_source = BuiltinSource(&builtins);
        let source = RuntimeArtifactResourcesRefV1 {
            chain_resource_manifest: &manifest,
            latest_chain_resource_delta: Some(&delta),
            builtin_release_manifest: &builtin_source,
        };
        let (canonical, memory) = persist_sources(&source_path, compression, &source);
        let source_store = IncrementalPageStore::open(
            &source_path,
            memory.clone(),
            compression,
            MemoryClass::Ordinary,
        )
        .unwrap();
        let root = StateRootV1::from_canonical_bytes(&canonical).unwrap();
        let roots = bounded_root_set(root);
        let delta_child = field_root(
            &source_store,
            &memory,
            roots.state_root(StateNamespaceV1::Artifacts),
            RuntimeArtifactFieldV1::LatestChainResourceDelta,
        )
        .unwrap();
        let child_state_page: [u8; 32] = blake3::Hash::from_hex(&delta_child.root().node_root)
            .unwrap()
            .into();

        let source_state = source_store
            .state_session(closure_physical(), MemoryClass::Ordinary)
            .unwrap();
        let source_values = source_store.value_session(closure_physical());
        let mut source_io = logical();
        let mut source_keys = SemanticKeyBudget {
            bytes_remaining: 64 << 20,
        };
        let mut source_scans = counts();
        let mut source_context = context(
            &memory,
            &source_state,
            &source_values,
            &mut source_io,
            &mut source_keys,
            &mut source_scans,
        );
        let mut source_work = RuntimeArtifactResourcesWorkV1 {
            cells_remaining: 100_000,
            max_pending_node_bytes: 2 << 20,
        };
        let marked = mark_runtime_artifact_resources_pages_v1(
            &roots,
            &mut source_context,
            &mut RuntimeArtifactResourcesPagesWorkV1 {
                work: &mut source_work,
                max_pages: 100_000,
            },
        )
        .unwrap();
        assert!(marked.contains(&child_state_page));
        let marked_count = marked.page_count();
        assert!(marked_count > 0);
        drop(marked);
        drop(source_context);
        drop(source_values);
        drop(source_state);

        let target_store =
            IncrementalPageStore::create(&target_path, memory.clone(), compression).unwrap();
        let mut target_io = closure_physical();
        let cancelled = AtomicBool::new(false);
        let mut no_external_cancel = || false;
        let mut target_values = target_store
            .buffered_page_writer(&mut target_io, &cancelled, &mut no_external_cancel)
            .unwrap();
        let transfer_state = source_store
            .state_session(closure_physical(), MemoryClass::Ordinary)
            .unwrap();
        let transfer_values = source_store.value_session(closure_physical());
        let mut transfer_io = logical();
        let mut transfer_keys = SemanticKeyBudget {
            bytes_remaining: 64 << 20,
        };
        let mut transfer_scans = counts();
        let mut transfer_context = context(
            &memory,
            &transfer_state,
            &transfer_values,
            &mut transfer_io,
            &mut transfer_keys,
            &mut transfer_scans,
        );
        let mut transfer_work = RuntimeArtifactResourcesWorkV1 {
            cells_remaining: 100_000,
            max_pending_node_bytes: 2 << 20,
        };
        let transfer = transfer_runtime_artifact_resources_pages_v1(
            &roots,
            &mut transfer_context,
            &mut RuntimeArtifactResourcesPagesWorkV1 {
                work: &mut transfer_work,
                max_pages: 100_000,
            },
            &mut target_values,
        );
        let transferred = match transfer {
            Ok(transferred) => transferred,
            Err(error) => {
                let state_left = transfer_state.remaining_io().unwrap();
                let values_left = transfer_values.remaining_io().unwrap();
                let state_error = transfer_state.take_read_error();
                drop(transfer_context);
                let value_work_left = transfer_io.clone();
                drop(target_values);
                eprintln!(
                    "Artifact page transfer failed {error:?}; state={state_left:?}; value={values_left:?}; logical={value_work_left:?}; state_error={state_error:?}; target={target_io:?}"
                );
                panic!("artifact page transfer failed");
            }
        };
        assert!(transferred.contains(&child_state_page));
        assert_eq!(transferred.page_count(), marked_count);
        target_values.finish().unwrap();
        drop(transfer_context);
        drop(target_store);
        drop(transfer_values);
        drop(transfer_state);

        let cold_store = IncrementalPageStore::open(
            &target_path,
            memory.clone(),
            compression,
            MemoryClass::Ordinary,
        )
        .unwrap();
        let cold_state = cold_store
            .state_session(closure_physical(), MemoryClass::Ordinary)
            .unwrap();
        let cold_values = cold_store.value_session(closure_physical());
        let mut cold_io = logical();
        let mut cold_keys = SemanticKeyBudget {
            bytes_remaining: 64 << 20,
        };
        let mut cold_scans = counts();
        let mut cold_context = context(
            &memory,
            &cold_state,
            &cold_values,
            &mut cold_io,
            &mut cold_keys,
            &mut cold_scans,
        );
        let mut cold_work = RuntimeArtifactResourcesWorkV1 {
            cells_remaining: 100_000,
            max_pending_node_bytes: 2 << 20,
        };
        let cold = qualify_runtime_artifact_resources_v1(&roots, &mut cold_context, &mut cold_work)
            .unwrap();
        assert_eq!(cold.record_counts(), [1, 1, builtins.len() as u64]);
        drop(cold);
        drop(cold_context);
        drop(cold_values);
        drop(cold_state);
        drop(cold_store);
        drop(transferred);
        drop(source_store);
    }
}
