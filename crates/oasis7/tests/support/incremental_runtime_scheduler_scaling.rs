use super::*;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
use std::{cell::Cell, sync::Arc};
// Count the complete logical State source, including generic replay. A sparse
// path is at most257 nodes; each recorded read uses one path, and each write
// performs at most one lookup plus one replace path (cached ancestors only reduce it).
struct CountState<'a, 'b> {
    source: &'a oasis7_distfs::incremental_page_store::StatePageSession<'b>,
    calls: Cell<u64>,
}
impl CountState<'_, '_> {
    fn take_read_error(&self) -> Option<PageStoreError> {
        self.source.take_read_error()
    }
    fn remaining_io(&self) -> Result<PageIoBudget, PageStoreError> {
        self.source.remaining_io()
    }
}
impl MerkleNodeReader for CountState<'_, '_> {
    type Node = oasis7_distfs::incremental_page_store::ChargedMerklePage;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.source.allocation_budget()
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.source.allocation_class()
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        self.calls.set(self.calls.get() + 1);
        self.source.read_node(id, max)
    }
}
struct Forbid<'a, R> {
    source: &'a R,
    old: [u8; 32],
    hits: &'a Cell<u64>,
    max: &'a Cell<usize>,
}
impl<R: ValueNodeReader> ValueNodeReader for Forbid<'_, R> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.max.set(self.max.get().max(max));
        if id == &self.old {
            self.hits.set(self.hits.get() + 1);
            return Err(ValueTreeError::Invalid(
                "unchanged cursor root read forbidden",
            ));
        }
        self.source.read_node(id, max, memory, class)
    }
}
#[test]
fn unchanged_32m_cursor_is_not_read_by_scalar_sender_or_complete_receiver() {
    let mut observed = None;
    for length in [64, 1 << 20, 32 << 20] {
        let directory = Directory::new();
        let path = directory.0.join("scale");
        // Borrow an original source; storage never clones or serializes the whole cursor.
        let original = "c".repeat(length);
        let memory = MemoryBudget::new(24 << 20, 4 << 20).unwrap();
        let store =
            IncrementalPageStore::create(&path, memory.clone(), PageCompression::Raw).unwrap();
        let roots = seed_cursor(&store, &memory, Some(&original));
        drop(original);
        let recovery = store
            .state_session(physical(), MemoryClass::Recovery)
            .unwrap();
        let recovered_values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut bootstrap = read_ctx!(
            roots
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Scheduler)
                .unwrap(),
            &recovery,
            &recovered_values,
            &memory,
            MemoryClass::Recovery,
            &mut io,
            &mut keys
        );
        bootstrap.record_bytes = 4096.max(length as u64 + 4096);
        let family = qualify_runtime_scheduler_controls_v1(
            &roots,
            &mut bootstrap,
            &mut scan(),
            &mut scan(),
            &mut 1000,
            length as u64,
        )
        .unwrap();
        let descriptor = FieldDescriptor {
            owner: FieldOwner::Runtime,
            name: "scheduler_cursor",
            shape: FieldShape::Value,
            destination: FieldDestination::State(StateNamespaceV1::Scheduler),
        };
        let cursor = WorldFieldKeyV1::new(
            descriptor,
            WorldFieldRecordKindV1::Entry,
            Some(*blake3::hash(b"oasis7.iws.semantic-key.v1\0\0").as_bytes()),
        )
        .unwrap();
        let old = roots
            .state
            .iter()
            .find(|r| r.namespace == StateNamespaceV1::Scheduler)
            .unwrap()
            .get(cursor.as_bytes(), &recovery)
            .unwrap()
            .unwrap();
        let parent = ExecutionParent {
            roots: roots.clone(),
            commit: [4; 32],
            authority: AuthorityBinding {
                token: [5; 32],
                epoch: 7,
            },
        };
        let live = parent.fence().unwrap();
        let state_backend = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let state = CountState {
            source: &state_backend,
            calls: Cell::new(0),
        };
        let values = store.value_session(physical());
        let hits = Cell::new(0);
        let max = Cell::new(0);
        let forbid = Forbid {
            source: &values,
            old,
            hits: &hits,
            max: &max,
        };
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [6; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 16,
                max_writes: 4,
                max_facts: 1,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &forbid,
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
        let mut writer = store.value_session(physical());
        let mut write_io = logical();
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: limits(),
            cancelled: || false,
        };
        let guard = overlay.transaction.guard();
        assert!(
            change_qualified_runtime_scheduler_scalar_v1(
                RuntimeSchedulerScalarChangeV1 {
                    field: RuntimeSchedulerScalarV1::MaxPendingActions,
                    qualification: &family,
                    expected: RuntimeSchedulerControlValueRefV1::U64(usize::MAX as u64),
                    after: RuntimeSchedulerControlValueRefV1::U64(0),
                    max_current_payload_bytes: 165
                },
                &mut overlay,
                &mut write,
                &guard,
                &live
            )
            .unwrap()
        );
        let guard = overlay.transaction.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        assert_eq!(sealed.record.writes.len(), 1);
        let state2_backend = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let state2 = CountState {
            source: &state2_backend,
            calls: Cell::new(0),
        };
        let values2 = store.value_session(physical());
        let forbid2 = Forbid {
            source: &values2,
            old,
            hits: &hits,
            max: &max,
        };
        let mut rio = logical();
        let mut rkeys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = read_ctx!(
            sealed
                .record
                .after
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Scheduler)
                .unwrap(),
            &state2,
            &forbid2,
            &memory,
            MemoryClass::Ordinary,
            &mut rio,
            &mut rkeys
        );
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [6; 32],
            source: &state2,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 4 << 20,
        };
        let accepted = accept_runtime_scheduler_controls_transition_v1(
            &family,
            &verifier,
            &sealed.record,
            &mut ctx,
            &mut 128,
            165,
        )
        .unwrap();
        accepted
            .validate_binding(&sealed.record.after.digest().unwrap())
            .unwrap();
        let structural_bound =
            (sealed.record.reads.len() + 2 * sealed.record.writes.len()) as u64 * 257;
        assert!(
            state.calls.get() <= structural_bound,
            "sender logical State paths"
        );
        assert!(
            state2.calls.get() <= structural_bound,
            "full generic+typed State paths"
        );
        eprintln!(
            "scheduler logical State sender={} generic+typed={} structuralbound={structural_bound}",
            state.calls.get(),
            state2.calls.get()
        );
        assert_eq!(hits.get(), 0);
        assert!(max.get() <= VALUE_MAX_NODE_BYTES);
        let sender = (
            logical().nodes_remaining - io.nodes_remaining,
            logical().read_bytes_remaining - io.read_bytes_remaining,
        );
        let owner_receiver = (
            logical().nodes_remaining - rio.nodes_remaining,
            logical().read_bytes_remaining - rio.read_bytes_remaining,
        );
        // State source counters include the actual generic replay inside typed accept.
        // Physical recipe lookup paths depend on content hashes; report them separately
        // rather than asserting byte-for-byte equality of physical index work.
        let sender_state = state.remaining_io().unwrap();
        let receiver_state = state2.remaining_io().unwrap();
        let source_values = values.remaining_io().unwrap();
        let receiver_values = values2.remaining_io().unwrap();
        let counts = (
            sender,
            owner_receiver,
            max.get(),
            physical().reads_remaining - source_values.reads_remaining,
            physical().read_bytes_remaining - source_values.read_bytes_remaining,
            physical().reads_remaining - receiver_values.reads_remaining,
            physical().read_bytes_remaining - receiver_values.read_bytes_remaining,
        );
        eprintln!(
            "scheduler cursor={length} max={}; senderLogical={sender:?} ownerReceiverLogical={owner_receiver:?} senderState={sender_state:?} genericAndTypedState={receiver_state:?} senderValues={source_values:?} receiverValues={receiver_values:?}",
            max.get()
        );
        if let Some(before) = observed {
            assert_eq!(counts, before, "fixed delta source={length}");
        } else {
            observed = Some(counts);
        }
    }
}
