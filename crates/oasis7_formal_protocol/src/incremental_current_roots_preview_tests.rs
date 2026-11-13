use super::*;
use crate::controlled_authority::formal::bounded_context::FormalBoundedWorkV1;
use std::{cell::Cell, collections::BTreeMap};
#[derive(Default)]
struct Pages {
    nodes: BTreeMap<String, Vec<u8>>,
    reads: Cell<usize>,
}
impl MerkleNodeReader for Pages {
    type Node = Vec<u8>;
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Vec<u8>>, String> {
        self.reads.set(self.reads.get() + 1);
        self.nodes
            .get(id)
            .map(|bytes| {
                if bytes.len() > max {
                    return Err("fixture source bound".into());
                }
                Ok(bytes.clone())
            })
            .transpose()
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 4096,
        max_writes: 1024,
        max_facts: 0,
        max_pending_node_bytes: 32 << 20,
    }
}
fn parent() -> ExecutionParent {
    ExecutionParent {
        commit: [7; 32],
        roots: RootSetV1::empty("actual-preview-fixture".into(), [8; 32]).unwrap(),
        authority: AuthorityBinding {
            token: [9; 32],
            epoch: 3,
        },
    }
}
fn memory() -> MemoryBudget {
    MemoryBudget::new(128 << 20, 2 << 20).unwrap()
}
fn with_real_parent(memory: &MemoryBudget) -> (Pages, ExecutionParent) {
    let empty = Pages::default();
    let mut tx = ExecutionTransaction::begin(parent(), [1; 32], &empty, memory, limits()).unwrap();
    tx.write(StateNamespaceV1::Business, b"old", Some([2; 32]))
        .unwrap();
    let guard = tx.guard();
    let live = tx.parent_fence();
    let sealed = tx.seal(&guard, &live, 4).unwrap();
    let mut pages = Pages::default();
    for node in &sealed.new_nodes {
        pages.nodes.insert(node.digest.clone(), node.bytes.clone());
    }
    let mut parent = parent();
    parent.roots = sealed.record.after.clone();
    (pages, parent)
}
#[test]
fn actual_preview_equals_the_original_seal_all_namespaces_and_does_not_advance() {
    let memory = memory();
    let (pages, parent) = with_real_parent(&memory);
    let mut tx = ExecutionTransaction::begin(parent, [3; 32], &pages, &memory, limits()).unwrap();
    for (namespace, key) in NAMESPACES.into_iter().zip([
        b"business".as_slice(),
        b"alloc",
        b"sched",
        b"auth",
        b"effects",
        b"artifacts",
    ]) {
        tx.write(namespace, key, Some([namespace as u8; 32]))
            .unwrap();
    }
    tx.write(StateNamespaceV1::Business, b"cancelled", Some([1; 32]))
        .unwrap();
    tx.write(StateNamespaceV1::Business, b"cancelled", None)
        .unwrap();
    let guard = tx.guard();
    let live = tx.parent_fence();
    let work = FormalBoundedWorkV1::new(2_000_000, 256 << 20).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let preview = tx.preview_current_roots_v1(5, &mut common, &live).unwrap();
    preview.validate(&tx, &mut common, &live).unwrap();
    tx.validate_guard(&guard, &live).unwrap();
    assert_eq!(tx.ordered_fact_count(), 0);
    assert!(work.remaining_steps() < 2_000_000);
    assert!(work.remaining_read_bytes() < 256 << 20);
    let sealed = tx.seal(&guard, &live, 5).unwrap();
    assert_eq!(preview.roots(), &sealed.record.after);
    assert_eq!(preview.roots().height, 2);
    assert_eq!(preview.roots().simulation_tick, 5);
}
#[test]
fn new_read_invalidates_preview_and_repreview_uses_the_actual_new_guard() {
    let memory = memory();
    let (pages, parent) = with_real_parent(&memory);
    let mut tx = ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
    let live = tx.parent_fence();
    let work = FormalBoundedWorkV1::new(2_000_000, 256 << 20).unwrap();
    let mut stop = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
    let first = tx.preview_current_roots_v1(4, &mut common, &live).unwrap();
    assert_eq!(
        tx.read(StateNamespaceV1::Business, b"old").unwrap(),
        Some([2; 32])
    );
    assert!(matches!(
        first.validate(&tx, &mut common, &live),
        Err(CurrentRootsPreviewErrorV1::Execution(
            ExecutionError::StaleGuard
        ))
    ));
    let current = tx.preview_current_roots_v1(4, &mut common, &live).unwrap();
    current.validate(&tx, &mut common, &live).unwrap();
    assert_eq!(first.roots(), current.roots());
    let foreign = FormalBoundedWorkV1::new(100, 1024).unwrap();
    let mut foreign_stop = || false;
    let mut foreign_common = FormalBoundedContextV1::new(&memory, &foreign, &mut foreign_stop);
    assert!(current.validate(&tx, &mut foreign_common, &live).is_err());
}
#[test]
fn common_exhaustion_and_mid_node_cancel_keep_typed_first_cause_and_poison() {
    let memory = memory();
    let (pages, parent) = with_real_parent(&memory);
    for cancellation in [false, true] {
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [5; 32], &pages, &memory, limits())
                .unwrap();
        tx.write(StateNamespaceV1::Business, b"old", Some([3; 32]))
            .unwrap();
        let live = tx.parent_fence();
        let work =
            FormalBoundedWorkV1::new(if cancellation { 2_000_000 } else { 0 }, 256 << 20).unwrap();
        let polls = Cell::new(0usize);
        let mut stop = || {
            polls.set(polls.get() + 1);
            cancellation && polls.get() > 600
        };
        let mut common = FormalBoundedContextV1::new(&memory, &work, &mut stop);
        assert!(matches!(
            tx.preview_current_roots_v1(5, &mut common, &live),
            Err(CurrentRootsPreviewErrorV1::Common(_))
        ));
        if cancellation {
            assert!(polls.get() > 600);
        }
        assert!(matches!(
            tx.read(StateNamespaceV1::Business, b"old"),
            Err(ExecutionError::Aborted)
        ));
    }
}
