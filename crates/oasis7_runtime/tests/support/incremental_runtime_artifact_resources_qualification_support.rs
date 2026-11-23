use super::*;

pub(super) fn qualification_state_physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: QUALIFICATION_STATE_READS,
        ..physical()
    }
}

pub(super) fn qualify(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
) -> Result<ReservedValue<oasis7_runtime::incremental_runtime_artifact_resources::RuntimeArtifactResourcesQualificationV1>, ValueTreeError>{
    qualify_with_class(store, memory, roots, MemoryClass::Ordinary)
}

pub(super) fn qualify_with_class(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    roots: &RootSetV1,
    class: MemoryClass,
) -> Result<ReservedValue<oasis7_runtime::incremental_runtime_artifact_resources::RuntimeArtifactResourcesQualificationV1>, ValueTreeError>{
    let state = store
        .state_session(qualification_state_physical(), class)
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
                class,
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
