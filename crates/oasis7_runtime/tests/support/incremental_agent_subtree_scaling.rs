use super::*;
use oasis7_formal_protocol::incremental_execution::*;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_runtime::{
    BodyMutationContext as ValueMutationContextV1,
    incremental_field_overlay::WorldFieldOverlayContextV1,
};
use std::{cell::Cell, sync::Arc};
struct CountState<'a, M> {
    source: &'a M,
    calls: Cell<u64>,
    bytes: Cell<u64>,
}
impl<M: MerkleNodeReader> MerkleNodeReader for CountState<'_, M> {
    type Node = M::Node;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.source.allocation_budget()
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.source.allocation_class()
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let node = self.source.read_node(id, max)?;
        self.calls.set(self.calls.get() + 1);
        if let Some(node) = &node {
            self.bytes
                .set(self.bytes.get() + node.as_ref().len() as u64);
        }
        Ok(node)
    }
}
struct CountValues<'a, R> {
    source: &'a R,
    calls: Cell<u64>,
    bytes: Cell<u64>,
}
impl<R: ValueNodeReader> ValueNodeReader for CountValues<'_, R> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        let node = self.source.read_node(id, max, memory, class)?;
        self.calls.set(self.calls.get() + 1);
        self.bytes.set(self.bytes.get() + node.len() as u64);
        Ok(node)
    }
}
fn run(compression: PageCompression, summary_units: usize, slots: usize) -> [u64; 10] {
    let directory = Directory::new();
    let path = directory.0.join("agent-scaling");
    let roots = {
        let memory = MemoryBudget::new(256 << 20, 16 << 20).unwrap();
        let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
        seed_shape(&store, &memory, summary_units, slots)
    };
    // This scope writes Zstd pages and therefore includes the existing 16MiB
    // encode workspace; the separate cold-reader test remains 8MiB/4MiB.
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let store =
        IncrementalPageStore::open(&path, memory.clone(), compression, MemoryClass::Ordinary)
            .unwrap();
    let state = store
        .state_session(
            PageIoBudget {
                reads_remaining: 4_000_000,
                ..physical()
            },
            MemoryClass::Ordinary,
        )
        .unwrap();
    let values = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let qualification = qualify_agent_subtree_structure_v1(
        &roots,
        &mut WorldFieldReadContextV1 {
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
            record_bytes: 2 << 20,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        },
        &mut StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 256 << 20,
            leaves_remaining: 1000,
        },
        &mut AgentSubtreeScanWorkV1 {
            max_agents: 1,
            cells_remaining: 1000,
            max_scalar_bytes: 2 << 20,
        },
        &mut |_| Ok(()),
    );
    eprintln!(
        "agent bootstrap {compression:?} units={summary_units} slots={slots} result={:?} state={:?} value={:?}",
        qualification.as_ref().map(|_| ()),
        state.remaining_io(),
        values.remaining_io()
    );
    let proof = qualification.unwrap();
    let counted_state = CountState {
        source: &state,
        calls: Cell::new(0),
        bytes: Cell::new(0),
    };
    let counted_values = CountValues {
        source: &values,
        calls: Cell::new(0),
        bytes: Cell::new(0),
    };
    let state_before = state.remaining_io().unwrap();
    let value_before = values.remaining_io().unwrap();
    let mut writer = store.value_session(physical());
    let writer_before = writer.remaining_io().unwrap();
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [1; 32],
        authority: AuthorityBinding {
            token: [2; 32],
            epoch: 3,
        },
    };
    let live = parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [4; 32],
        &counted_state,
        &memory,
        TransactionLimits {
            max_reads: 32,
            max_writes: 4,
            max_facts: 0,
            max_pending_node_bytes: 1 << 20,
        },
    )
    .unwrap();
    let guard = tx.guard();
    assert!(
        change_agent_subtree_scalar_v1(
            AgentSubtreeCellChangeV1 {
                query: AgentSubtreeQualifiedRequestV1 {
                    cell: AgentSubtreeScalarRequestV1 {
                        agent: "agent 界",
                        field: AgentFieldV1::PositionY,
                        ordinal: 0
                    },
                    qualification: &proof
                },
                expected: Some(AgentScalarRefV1::I64(0)),
                after: Some(AgentScalarRefV1::I64(7)),
                max_scalar_bytes: 1024,
            },
            &mut WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &counted_values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io
                },
                key_work: &mut keys,
                semantic_bytes: 1024,
                record_bytes: 2 << 20,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || false,
            },
            &mut ValueMutationContextV1 {
                sink: &mut writer,
                memory: &memory,
                io: &mut logical(),
                cancelled: || false
            },
            &guard,
            &live,
        )
        .unwrap()
    );
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [4; 32],
        source: &counted_state,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 1 << 20,
    };
    let accepted = accept_agent_subtree_transition_v1(
        &proof,
        &verifier,
        &sealed.record,
        &mut WorldFieldReadContextV1 {
            root: &sealed.record.after.state[0],
            state: &counted_state,
            values: ValueReadContext {
                source: &counted_values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 1024,
            record_bytes: 2 << 20,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || false,
        },
        &mut AgentSubtreeTransitionWorkV1 {
            cells_remaining: 1000,
            max_scalar_bytes: 2 << 20,
        },
    )
    .unwrap();
    accepted.validate_binding(&sealed.record.after).unwrap();
    let state_after = state.remaining_io().unwrap();
    let value_after = values.remaining_io().unwrap();
    let writer_after = writer.remaining_io().unwrap();
    [
        counted_state.calls.get(),
        counted_state.bytes.get(),
        counted_values.calls.get(),
        counted_values.bytes.get(),
        state_before.reads_remaining - state_after.reads_remaining,
        state_before.read_bytes_remaining - state_after.read_bytes_remaining,
        value_before.reads_remaining - value_after.reads_remaining,
        value_before.read_bytes_remaining - value_after.read_bytes_remaining,
        writer_before.reads_remaining - writer_after.reads_remaining,
        writer_before.read_bytes_remaining - writer_after.read_bytes_remaining,
    ]
}
#[test]
fn scalar_sender_and_real_receiver_skip_unchanged_agent_summary_and_rows() {
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let small = run(compression, 32, 3);
        let large = run(compression, 350_000, 32);
        eprintln!(
            "agent fixed-delta {compression:?} small={small:?} large={large:?}; logical first4, physical state/value/writer last6"
        );
        for i in [1, 3, 5, 7, 9] {
            assert!(
                large[i] <= small[i] + (64 << 10),
                "unchanged payload growth leaked in byte channel{i}"
            );
        }
        for i in [0, 2, 4, 6, 8] {
            assert!(
                large[i] <= small[i] + 1024,
                "unchanged row growth leaked in operation channel{i}"
            );
        }
    }
}
