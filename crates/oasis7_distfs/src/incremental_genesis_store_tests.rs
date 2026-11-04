use super::*;
use crate::incremental_page_store::tests::{Fixture, physical_io};
use oasis7_formal_protocol::incremental_state::StateNamespaceV1;

fn config() -> GenesisConfigV1 {
    GenesisConfigV1 {
        world_id: "streamed-genesis".into(),
        format_profile: [1; 32],
        initial_content: [2; 32],
        activation_config: [3; 32],
        execution_environment: [4; 32],
    }
}
fn work() -> GenesisIndexWorkV1 {
    GenesisIndexWorkV1 {
        rows_remaining: 256,
        new_nodes_remaining: 100_000,
        new_bytes_remaining: 64 << 20,
        max_pending_node_bytes: 16 << 20,
    }
}
fn row(memory: &MemoryBudget, n: usize) -> ReservedValue<ExecutionWriteV1> {
    let key = format!("row-{n:04}").into_bytes();
    memory
        .reserve(512, MemoryClass::Ordinary)
        .unwrap()
        .retain(ExecutionWriteV1 {
            namespace: StateNamespaceV1::Business,
            mutation: StateWriteV1 {
                key,
                before: None,
                after: Some([7; 32]),
            },
        })
}

#[test]
fn streamed_genesis_crosses_batch_boundary_and_survives_cold_reopen() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let cancel = AtomicBool::new(false);
    let mut builder = store
        .genesis_state_index_builder(&config(), work(), physical_io(), physical_io(), &cancel)
        .unwrap();
    builder
        .contribute::<ValueTreeError>(|sink| {
            for n in 0..65 {
                sink(row(&memory, n))?;
            }
            Ok(())
        })
        .unwrap();
    let roots = builder.finish().unwrap();
    assert_eq!(roots.state.iter().map(|r| r.leaf_count).sum::<u64>(), 65);
    drop(store);
    let store = IncrementalPageStore::open(
        &fixture.store(),
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Recovery,
    )
    .unwrap();
    let source = store
        .state_session(physical_io(), MemoryClass::Recovery)
        .unwrap();
    let root = roots
        .state
        .iter()
        .find(|r| r.namespace == StateNamespaceV1::Business)
        .unwrap();
    for n in [0, 63, 64] {
        assert_eq!(
            root.get(format!("row-{n:04}").as_bytes(), &source).unwrap(),
            Some([7; 32])
        );
    }
    assert_eq!(root.get(b"missing", &source).unwrap(), None);
    // These are state-index pages only: the value closure and accepted head
    // deliberately remain separate obligations, not implied by this fixture.
}

#[test]
fn duplicate_across_flushed_batches_aborts_genesis() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let cancel = AtomicBool::new(false);
    let mut builder = store
        .genesis_state_index_builder(&config(), work(), physical_io(), physical_io(), &cancel)
        .unwrap();
    builder
        .contribute::<ValueTreeError>(|sink| {
            for n in 0..65 {
                sink(row(&memory, n))?;
            }
            Ok(())
        })
        .unwrap();
    builder
        .contribute::<ValueTreeError>(|sink| sink(row(&memory, 0)))
        .unwrap();
    assert!(builder.finish().is_err());
}

#[test]
fn swallowed_sink_failure_and_cancellation_cannot_finish() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let cancel = AtomicBool::new(false);
    let mut quota = work();
    quota.rows_remaining = 0;
    let mut builder = store
        .genesis_state_index_builder(&config(), quota, physical_io(), physical_io(), &cancel)
        .unwrap();
    assert!(
        builder
            .contribute::<ValueTreeError>(|sink| {
                let _ = sink(row(&memory, 0));
                Ok(())
            })
            .is_err()
    );
    assert!(builder.finish().is_err());
    let builder = store
        .genesis_state_index_builder(&config(), work(), physical_io(), physical_io(), &cancel)
        .unwrap();
    cancel.store(true, Ordering::Release);
    assert!(matches!(builder.finish(), Err(ValueTreeError::Cancelled)));
}
