//! Fixed-delta measurement includes helper, generic replay and typed receiver.
//! Logical node calls are separate from actual PageStore IO counters.
use super::*;
use oasis7_formal_protocol::incremental_execution::*;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_runtime::{
    incremental_cognition_family::*, incremental_cognition_json_overlay::*,
    incremental_field_overlay::WorldFieldOverlayContextV1, incremental_json_records::*,
};
use std::{cell::Cell, sync::Arc};
#[derive(Debug, PartialEq, Eq)]
pub struct Metrics {
    logical: [(u64, u64); 3],
    physical: [(u64, u64); 3],
}
struct CountState<'a, S> {
    source: &'a S,
    count: Cell<(u64, u64)>,
}
impl<S: MerkleNodeReader> MerkleNodeReader for CountState<'_, S> {
    type Node = S::Node;
    fn allocation_class(&self) -> Option<MemoryClass> {
        self.source.allocation_class()
    }
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        self.source.allocation_budget()
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Self::Node>, String> {
        let result = self.source.read_node(id, max)?;
        let (calls, bytes) = self.count.get();
        self.count.set((
            calls + 1,
            bytes + result.as_ref().map_or(0, |node| node.as_ref().len() as u64),
        ));
        Ok(result)
    }
}
struct CountValue<'a, R> {
    source: &'a R,
    count: Cell<(u64, u64)>,
    deny: [u8; 32],
}
impl<R: ValueNodeReader> ValueNodeReader for CountValue<'_, R> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        assert_ne!(
            id, &self.deny,
            "ordinary acceptance visited unchanged large JSON subtree"
        );
        let result = self.source.read_node(id, max, memory, class)?;
        let (calls, bytes) = self.count.get();
        self.count.set((calls + 1, bytes + result.len() as u64));
        Ok(result)
    }
}
struct CountWriter<'a, W> {
    source: &'a mut W,
    count: Cell<(u64, u64)>,
    deny: [u8; 32],
}
impl<W: ValueNodeReader> ValueNodeReader for CountWriter<'_, W> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        assert_ne!(
            id, &self.deny,
            "writer visited unchanged large JSON subtree"
        );
        let result = self.source.read_node(id, max, memory, class)?;
        let (calls, bytes) = self.count.get();
        self.count.set((calls + 1, bytes + result.len() as u64));
        Ok(result)
    }
}
impl<W: ValueNodeWriter> ValueNodeWriter for CountWriter<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.source.put_node(id, bytes)
    }
}
impl<W: StatePageWriter> StatePageWriter for CountWriter<'_, W> {
    fn put_state_pages_cancelled(
        &mut self,
        delta: &ReservedValue<StateDeltaV1>,
        memory: &MemoryBudget,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        self.source
            .put_state_pages_cancelled(delta, memory, cancelled)
    }
}
fn setup_io() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 32_000_000,
        read_bytes_remaining: 8 << 30,
        writes_remaining: 1_000_000,
        write_bytes_remaining: 2 << 30,
    }
}
fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 2_000_000,
        read_bytes_remaining: 2 << 30,
        write_bytes_remaining: 2 << 30,
    }
}
pub fn run(size: usize) -> Metrics {
    let directory = fixture::Directory::new();
    let memory = MemoryBudget::new(32 << 20, 8 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("cognition-scaling"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let mut top = RootSetV1::empty("cognition-scaling".into(), [0x63; 32]).unwrap();
    {
        // Caller-owned legacy borrowed DTO is explicit setup input, not an output
        // allocation or a claim that the caller's snapshot fits the engine pool.
        let mut source = super::source(false);
        source["feedback_outbox"] = serde_json::json!({"small":false,"unchanged":"x".repeat(size)});
        let mut writer = store.value_session(setup_io());
        let state = store
            .state_session(setup_io(), MemoryClass::Ordinary)
            .unwrap();
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 16 << 20,
        };
        let _batch = memory.reserve(64 << 10, MemoryClass::Ordinary).unwrap();
        let mut writes = Vec::with_capacity(64);
        let mut remaining = 64;
        prepare_cognition_state_genesis_v1(
            CognitionEnvelopeRefV1::new(&source).unwrap(),
            &mut MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut io,
                key_work: &mut keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: (size as u64).max(4096),
                    record_bytes: (size as u64).max(4096) + 8192,
                },
                cancelled: || false,
            },
            &mut remaining,
            &mut JsonWriteWorkV1 {
                cells_remaining: 4096,
                json_nodes_remaining: 512,
                max_json_depth: 4,
                max_pending_node_bytes: 2 << 20,
            },
            &mut |row| {
                writes.push(row);
                Ok(())
            },
        )
        .unwrap();
        for root in &mut top.state {
            let count = writes
                .iter()
                .filter(|row| row.namespace == root.namespace)
                .count();
            let heap = memory
                .reserve(count * (512 << 10) + (128 << 10), MemoryClass::Ordinary)
                .unwrap();
            let mut changes = writes
                .iter()
                .filter(|row| row.namespace == root.namespace)
                .map(|row| row.mutation.clone())
                .collect::<Vec<_>>();
            changes.sort_by(|a, b| a.key.cmp(&b.key));
            let delta =
                heap.retain(StateDeltaV1::build(root.clone(), changes, &state, 2 << 20).unwrap());
            writer
                .put_state_pages_cancelled(&delta, &memory, &mut || false)
                .unwrap();
            *root = delta.after.clone();
        }
    }
    let state_source = store
        .state_session(setup_io(), MemoryClass::Ordinary)
        .unwrap();
    let values_source = store.value_session(setup_io());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut context = WorldFieldReadContextV1 {
        root: &top.state[0],
        state: &state_source,
        values: ValueReadContext {
            source: &values_source,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: (size as u64).max(4096) + 8192,
        take_state_error: || state_source.take_read_error().map(state_error),
        cancelled: || false,
    };
    let mut rawwork = JsonReadWorkV1 {
        cells_remaining: 4096,
        json_nodes_remaining: 512,
        max_depth: 4,
        max_payload_bytes: (size as u64).max(4096),
    };
    let family = qualify_cognition_current_state_v1(
        &top,
        &mut context,
        &mut StateScanBudget {
            nodes_remaining: 100_000,
            read_bytes_remaining: 64 << 20,
            leaves_remaining: 1000,
        },
        &mut rawwork,
        &mut 1000,
    )
    .unwrap();
    let root_id = top.digest().unwrap();
    let field = CognitionJsonFieldV1::FeedbackOutbox;
    let entry = family.json_entry(field, &root_id).unwrap().unwrap();
    let parent = ExecutionParent {
        roots: top.clone(),
        commit: [0x64; 32],
        authority: AuthorityBinding {
            token: [0x65; 32],
            epoch: 1,
        },
    };
    let large = {
        let mut query = ExecutionTransaction::begin(
            parent.clone(),
            [0x67; 32],
            &state_source,
            &memory,
            TransactionLimits {
                max_reads: 8,
                max_writes: 2,
                max_facts: 0,
                max_pending_node_bytes: 2 << 20,
            },
        )
        .unwrap();
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut query,
            values: ValueReadContext {
                source: &values_source,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || state_source.take_read_error().map(state_error),
            cancelled: || false,
        };
        qualify_cognition_json_path_overlay_v1(
            CognitionJsonPathReadV1 {
                field,
                qualification: entry,
                path: &[JsonPathSegmentV1::Name("unchanged")],
            },
            &mut overlay,
            &mut JsonScalarPathReadWorkV1 {
                cells_remaining: 128,
                json_nodes_remaining: 8,
                max_depth: 4,
            },
        )
        .unwrap()
    };
    let deny = *blake3::Hash::from_hex(&large.root().node_root)
        .unwrap()
        .as_bytes();
    let state_physical = state_source.remaining_io().unwrap();
    let value_physical = values_source.remaining_io().unwrap();
    let state = CountState {
        source: &state_source,
        count: Cell::new((0, 0)),
    };
    let values = CountValue {
        source: &values_source,
        count: Cell::new((0, 0)),
        deny,
    };
    let mut writer_source = store.value_session(setup_io());
    let writer_physical = writer_source.remaining_io().unwrap();
    let mut writer = CountWriter {
        source: &mut writer_source,
        count: Cell::new((0, 0)),
        deny,
    };
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [0x66; 32],
        &state,
        &memory,
        TransactionLimits {
            max_reads: 8,
            max_writes: 2,
            max_facts: 0,
            max_pending_node_bytes: 2 << 20,
        },
    )
    .unwrap();
    let live = parent.fence().unwrap();
    let mut point_io = fixture::logical();
    let mut point_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    let mut write_io = fixture::logical();
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 16 << 20,
    };
    {
        let guard = tx.guard();
        let mut context = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut point_io,
            },
            key_work: &mut point_keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || state_source.take_read_error().map(state_error),
            cancelled: || false,
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 4096,
                record_bytes: 8192,
            },
            cancelled: || false,
        };
        change_cognition_json_scalar_v1(
            CognitionJsonScalarChangeV1 {
                field,
                qualification: entry,
                path: &[JsonPathSegmentV1::Name("small")],
                change: (JsonScalarRefV1::Bool(false), JsonScalarRefV1::Bool(true)),
            },
            &mut context,
            &mut write,
            &mut CognitionJsonPointWorkV1 {
                point: JsonWriteWorkV1 {
                    cells_remaining: 128,
                    json_nodes_remaining: 16,
                    max_json_depth: 4,
                    max_pending_node_bytes: 2 << 20,
                },
                changed: JsonChangeWorkV1 {
                    json: JsonReadWorkV1 {
                        cells_remaining: 256,
                        json_nodes_remaining: 32,
                        max_depth: 4,
                        max_payload_bytes: 4096,
                    },
                    paths_remaining: 4096,
                    max_pending_pairs: 8,
                },
            },
            &guard,
            &live,
        )
        .unwrap()
        .unwrap();
    }
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    assert_eq!(sealed.record.writes.len(), 1);
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [0x66; 32],
        source: &state,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 2 << 20,
    };
    let mut context = WorldFieldReadContextV1 {
        root: &parent.roots.state[0],
        state: &state,
        values: ValueReadContext {
            source: &values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut point_io,
        },
        key_work: &mut point_keys,
        semantic_bytes: 4096,
        record_bytes: 8192,
        take_state_error: || state_source.take_read_error().map(state_error),
        cancelled: || false,
    };
    let accepted = accept_cognition_current_transition_v1(
        &family,
        &verifier,
        &sealed.record,
        &mut context,
        &mut JsonChangeWorkV1 {
            json: JsonReadWorkV1 {
                cells_remaining: 256,
                json_nodes_remaining: 32,
                max_depth: 4,
                max_payload_bytes: 4096,
            },
            paths_remaining: 4096,
            max_pending_pairs: 8,
        },
    )
    .unwrap();
    accepted
        .validate_binding(&sealed.record.after.digest().unwrap())
        .unwrap();
    let after_state = state_source.remaining_io().unwrap();
    let after_value = values_source.remaining_io().unwrap();
    let after_writer = writer.source.remaining_io().unwrap();
    let metrics = Metrics {
        logical: [state.count.get(), values.count.get(), writer.count.get()],
        physical: [
            (
                state_physical.reads_remaining - after_state.reads_remaining,
                state_physical.read_bytes_remaining - after_state.read_bytes_remaining,
            ),
            (
                value_physical.reads_remaining - after_value.reads_remaining,
                value_physical.read_bytes_remaining - after_value.read_bytes_remaining,
            ),
            (
                writer_physical.reads_remaining - after_writer.reads_remaining,
                writer_physical.read_bytes_remaining - after_writer.read_bytes_remaining,
            ),
        ],
    };
    eprintln!("Cognition fixed delta S={size} engine pool32M/8M metrics={metrics:?}");
    assert!(metrics.logical.iter().map(|pair| pair.1).sum::<u64>() < 1 << 20);
    metrics
}
