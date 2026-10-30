use super::super::tests::{Fixture, logical_io, physical_io};
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionError, ExecutionParent, ExecutionTransaction, RootSetV1,
        TransactionLimits, TransitionVerifier,
    },
    incremental_history::{CatalogQueryV1, CatalogRootV1, GenesisConfigV1, HeaderPreparation},
    incremental_state::StateNamespaceV1,
    incremental_value::write_value,
};

fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 32,
        max_writes: 8,
        max_facts: 32,
        max_pending_node_bytes: 8 << 20,
    }
}
fn authority() -> AuthorityBinding {
    AuthorityBinding {
        token: [5; 32],
        epoch: 1,
    }
}
fn value(store: &IncrementalPageStore, memory: &MemoryBudget, bytes: &[u8]) -> [u8; 32] {
    let mut sink = store.value_session(physical_io());
    write_value(
        &mut &bytes[..],
        bytes.len() as u64,
        &mut sink,
        memory,
        &mut logical_io(),
        || false,
    )
    .unwrap()
    .digest()
    .unwrap()
}
fn first_transition(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
) -> (ExecutionParent, ReservedValue<PreparedTransition>) {
    let source = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap();
    let parent = ExecutionParent {
        commit: [6; 32],
        roots: RootSetV1::empty("page-backed".into(), [3; 32]).unwrap(),
        authority: authority(),
    };
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [7; 32], &source, memory, limits()).unwrap();
    txn.write(
        StateNamespaceV1::Business,
        b"alice",
        Some(value(store, memory, b"first value")),
    )
    .unwrap();
    let guard = txn.guard();
    let prepared = txn.seal(&guard, &parent.fence().unwrap(), 1).unwrap();
    (parent, prepared)
}

#[test]
fn actual_catalog_pages_reopen_and_prove_old_height_and_commit_queries() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let mut source = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap();
    let config = GenesisConfigV1 {
        world_id: "catalog-pages".into(),
        format_profile: [1; 32],
        initial_content: [2; 32],
        activation_config: [3; 32],
        execution_environment: [4; 32],
    };
    let scope = config.scope().unwrap();
    let roots = RootSetV1::empty(scope.world_id.clone(), scope.genesis_config).unwrap();
    let catalog = CatalogRootV1::empty(scope).unwrap();
    let header = HeaderPreparation {
        config: &config,
        catalog: &catalog,
        roots: &roots,
        parent: None,
        parent_history: None,
        transition: None,
        genesis_facts: &[],
        operation: [6; 32],
        authority: authority(),
        execution_environment: config.execution_environment,
        checkpoint: true,
    }
    .prepare(&memory)
    .unwrap();
    let delta = catalog
        .extend(&header.body, &source, &memory, 8 << 20)
        .unwrap();
    let foreign = MemoryBudget::new(32 << 20, 0).unwrap();
    assert!(matches!(
        catalog.extend(&header.body, &source, &foreign, 8 << 20),
        Err(ExecutionError::Invalid("Merkle source memory pool/class"))
    ));
    assert!(matches!(
        delta
            .after
            .prove(CatalogQueryV1::Height(0), &source, &foreign),
        Err(ExecutionError::Invalid("Merkle source memory pool/class"))
    ));
    assert_eq!(foreign.used(), 0);
    source.persist_catalog_pages(&delta).unwrap();
    let after = delta.after.clone();
    drop(source);
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
    assert!(matches!(
        after.prove(CatalogQueryV1::Height(0), &source, &memory),
        Err(ExecutionError::Invalid("Merkle source memory pool/class"))
    ));
    // Proof production is a live Ordinary phase, even after a Recovery open.
    let proof_source = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap();
    for query in [
        CatalogQueryV1::Height(0),
        CatalogQueryV1::Commit(header.body.digest().unwrap()),
    ] {
        let proof = after.prove(query, &proof_source, &memory).unwrap();
        assert_eq!(
            *proof.verify(&after, query, &memory).unwrap(),
            Some(delta.entry.clone())
        );
    }
    assert_eq!(
        after.lookup(CatalogQueryV1::Height(1), &source).unwrap(),
        None
    );
}

#[test]
fn page_carriers_remain_charged_and_typed_storage_errors_poison_transactions() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(16 << 20, 4 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let (_, prepared) = first_transition(&store, &memory);
    let mut source = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap();
    source.persist_transition_pages(&prepared).unwrap();
    let root = &prepared.record.after.state[0].node_root;
    let baseline = memory.used();
    let page = source
        .read_node(root, STATE_MAX_PAGE_BYTES)
        .unwrap()
        .unwrap();
    assert!(memory.used() > baseline);
    assert!(!page.as_ref().is_empty());
    drop(page);
    assert_eq!(memory.used(), baseline);
    let parent = ExecutionParent {
        commit: [8; 32],
        roots: prepared.record.after.clone(),
        authority: authority(),
    };
    let foreign = MemoryBudget::new(16 << 20, 0).unwrap();
    assert!(matches!(
        ExecutionTransaction::begin(parent.clone(), [9; 32], &source, &foreign, limits()),
        Err(ExecutionError::Invalid("Merkle source memory pool/class"))
    ));
    let original_parent = ExecutionParent {
        commit: prepared.record.parent_commit,
        roots: prepared.record.before.clone(),
        authority: prepared.record.authority,
    };
    assert!(matches!(
        TransitionVerifier {
            parent: &original_parent,
            operation: prepared.record.operation,
            source: &source,
            memory: &foreign,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20
        }
        .verify(&prepared.record),
        Err(ExecutionError::Invalid("Merkle source memory pool/class"))
    ));
    assert_eq!(foreign.used(), 0);
    // The original typed failure remains available even though the shared
    // Merkle consumer returns its legacy String-based execution error.
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [9; 32], &source, &memory, limits()).unwrap();
    let ordinary = memory
        .reserve(
            (12 << 20) - memory.used() - (1 << 20),
            MemoryClass::Ordinary,
        )
        .unwrap();
    assert!(txn.read(StateNamespaceV1::Business, b"alice").is_err());
    assert!(matches!(
        source.take_read_error(),
        Some(PageStoreError::Memory(_))
    ));
    drop(ordinary);
    let guard = txn.guard();
    assert!(matches!(
        txn.seal(&guard, &parent.fence().unwrap(), 2),
        Err(ExecutionError::Aborted)
    ));
    let mut no_reads = physical_io();
    no_reads.reads_remaining = 0;
    let limited = store
        .state_session(no_reads, MemoryClass::Ordinary)
        .unwrap();
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [9; 32], &limited, &memory, limits()).unwrap();
    assert!(txn.read(StateNamespaceV1::Business, b"alice").is_err());
    assert!(matches!(
        limited.take_read_error(),
        Some(PageStoreError::ResourceLimited(_))
    ));
    drop(txn);
    source.cancelled.store(true, Ordering::Release);
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [9; 32], &source, &memory, limits()).unwrap();
    assert!(txn.read(StateNamespaceV1::Business, b"alice").is_err());
    assert!(matches!(
        source.take_read_error(),
        Some(PageStoreError::Cancelled)
    ));
    let guard = txn.guard();
    assert!(matches!(
        txn.seal(&guard, &parent.fence().unwrap(), 2),
        Err(ExecutionError::Aborted)
    ));
    source.cancelled.store(false, Ordering::Release);
    let recipe = fixture.store().join("recipes").join(root);
    fs::remove_file(&recipe).unwrap();
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [9; 32], &source, &memory, limits()).unwrap();
    assert!(txn.read(StateNamespaceV1::Business, b"alice").is_err());
    assert!(matches!(
        source.take_read_error(),
        Some(PageStoreError::Missing(_))
    ));
}

#[test]
fn recovery_session_can_install_original_pages_while_ordinary_admission_is_full() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(12 << 20, 4 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let (_, prepared) = first_transition(&store, &memory);
    let mut recovery = store
        .state_session(physical_io(), MemoryClass::Recovery)
        .unwrap();
    let ordinary = memory
        .reserve((8 << 20) - memory.used(), MemoryClass::Ordinary)
        .unwrap();
    assert!(
        store
            .state_session(physical_io(), MemoryClass::Ordinary)
            .is_err()
    );
    recovery.persist_transition_pages(&prepared).unwrap();
    let root = &prepared.record.after.state[0].node_root;
    assert!(
        recovery
            .read_node(root, STATE_MAX_PAGE_BYTES)
            .unwrap()
            .is_some()
    );
    drop(ordinary);
}

#[test]
fn full_six_namespace_state_and_value_scan_after_real_store_reopen() {
    use oasis7_formal_protocol::{
        incremental_state::{StateScanBudget, StateScanContext},
        incremental_value::ValueReadContext,
    };
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(16 << 20, 3 << 20).unwrap();
    let store =
        IncrementalPageStore::create(&fixture.store(), memory.clone(), PageCompression::Raw)
            .unwrap();
    let id = value(&store, &memory, b"shared current value");
    let mut pages = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap();
    let parent = ExecutionParent {
        commit: [6; 32],
        roots: RootSetV1::empty("full-state".into(), [3; 32]).unwrap(),
        authority: authority(),
    };
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [7; 32], &pages, &memory, limits()).unwrap();
    for root in &parent.roots.state {
        txn.write(root.namespace, b"current", Some(id)).unwrap();
    }
    let guard = txn.guard();
    let prepared = txn.seal(&guard, &parent.fence().unwrap(), 1).unwrap();
    pages.persist_transition_pages(&prepared).unwrap();
    let roots = prepared.record.after.clone();
    drop(prepared);
    drop(pages);
    drop(store);
    let store = IncrementalPageStore::open(
        &fixture.store(),
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Recovery,
    )
    .unwrap();
    let state = store
        .state_session(physical_io(), MemoryClass::Recovery)
        .unwrap();
    let values = store.value_session(physical_io());
    let mut logical = logical_io();
    logical.write_bytes_remaining = 0;
    let mut read = ValueReadContext {
        source: &values,
        memory: &memory,
        class: MemoryClass::Recovery,
        io: &mut logical,
    };
    let mut quota = StateScanBudget {
        nodes_remaining: 6 * 257,
        read_bytes_remaining: 1 << 24,
        leaves_remaining: 6,
    };
    let before = memory.used();
    let mut total = 0;
    for root in &roots.state {
        total += StateScanContext {
            source: &state,
            memory: &memory,
            class: MemoryClass::Recovery,
            io: &mut quota,
        }
        .visit_entries(
            root,
            |entry| {
                assert_eq!(entry.value, id);
                let summary = read.verify_closure(&entry.value, 100, || false)?;
                assert_eq!(summary.root().0.logical_len, 20);
                Ok::<_, oasis7_formal_protocol::incremental_value::ValueTreeError>(())
            },
            || false,
        )
        .unwrap();
    }
    assert_eq!(total, 6);
    assert_eq!((quota.nodes_remaining, quota.leaves_remaining), (0, 0));
    assert_eq!(logical.nodes_remaining, 100000 - 12);
    assert_eq!(logical.write_bytes_remaining, 0);
    assert_eq!(memory.used(), before);
}
