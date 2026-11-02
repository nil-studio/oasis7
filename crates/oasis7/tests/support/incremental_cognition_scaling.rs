//! Fixed-delta measurement includes helper, generic replay and typed receiver.
//! Logical node calls are separate from actual PageStore IO counters.
use super::*;
use oasis7_formal_protocol::incremental_execution::*;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_runtime::{
    incremental_cognition_family::*, incremental_cognition_json_overlay::*,
    incremental_field_overlay::WorldFieldOverlayContextV1, incremental_json_records::*,
};
use std::{
    cell::{Cell, RefCell},
    sync::Arc,
};
#[derive(Debug, PartialEq, Eq)]
pub struct Metrics {
    logical: [(u64, u64); 3],
    physical: [(u64, u64); 3],
    writes: (u64, u64),
    logical_writes: (u64, u64),
    locator: [(u64, u64); 3],
}
impl Metrics {
    pub fn assert_fixed(&self, other: &Self) {
        assert_eq!(self.logical, other.logical);
        assert_eq!(self.logical_writes, other.logical_writes);
        assert_eq!(self.writes, other.writes);
        let normalize = |value: &Self| {
            std::array::from_fn::<_, 3, _>(|i| {
                let (packs, probes) = value.locator[i];
                assert!(probes <= 9 * packs); // v1 packs admit at most 256 entries.
                (
                    value.physical[i].0.checked_sub(probes).unwrap(),
                    value.physical[i].1.checked_sub(44 * probes).unwrap(),
                )
            })
        };
        assert_eq!(normalize(self), normalize(other));
    }
}
struct Trace {
    rows: RefCell<Vec<(usize, [u8; 32])>>,
}
impl Trace {
    fn push(&self, channel: usize, id: [u8; 32]) {
        let mut rows = self.rows.borrow_mut();
        assert!(rows.len() < rows.capacity());
        rows.push((channel, id));
    }
}

struct CountState<'a, S> {
    source: &'a S,
    trace: &'a Trace,
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
        if result.is_some() { self.trace.push(0, *blake3::Hash::from_hex(id).unwrap().as_bytes()); }
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
    trace: &'a Trace,
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
        self.trace.push(1, *id);
        let (calls, bytes) = self.count.get();
        self.count.set((calls + 1, bytes + result.len() as u64));
        Ok(result)
    }
}
struct CountWriter<'a, W> {
    source: &'a mut W,
    writes: Cell<(u64, u64)>,
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
        let length = bytes.len() as u64;
        self.source.put_node(id, bytes)?;
        let (calls, total) = self.writes.get();
        self.writes.set((calls + 1, total + length));
        Ok(())
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
            .put_state_pages_cancelled(delta, memory, cancelled)?;
        let (calls, total) = self.writes.get();
        self.writes.set((
            calls + delta.new_nodes.len() as u64,
            total
                + delta
                    .new_nodes
                    .iter()
                    .map(|node| node.bytes.len() as u64)
                    .sum::<u64>(),
        ));
        Ok(())
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
    let _trace_charge = memory.reserve(2 << 20, MemoryClass::Ordinary).unwrap();
    let trace = Trace {
        rows: RefCell::new(Vec::with_capacity(32768)),
    };
    let state_physical = state_source.remaining_io().unwrap();
    let value_physical = values_source.remaining_io().unwrap();
    let state = CountState {
        source: &state_source,
        trace: &trace,
        count: Cell::new((0, 0)),
    };
    let values = CountValue {
        source: &values_source,
        trace: &trace,
        count: Cell::new((0, 0)),
        deny,
    };
    let mut writer_source = store.value_session(setup_io());
    let writer_physical = writer_source.remaining_io().unwrap();
    let mut writer = CountWriter {
        source: &mut writer_source,
        writes: Cell::new((0, 0)),
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
    // Post-measurement diagnostic oracle only: it reads bounded original pack
    // headers/index rows, never changes production or resets a session budget.
    let locator = audit_locators(&directory.0.join("cognition-scaling"), &trace);
    let metrics = Metrics {
        writes: (
            writer_physical.writes_remaining - after_writer.writes_remaining,
            writer_physical.write_bytes_remaining - after_writer.write_bytes_remaining,
        ),
        logical_writes: writer.writes.get(),
        locator,
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
    // The fixed-delta oracle compares all three sizes exactly in the caller.
    // A guessed absolute cumulative-IO cap is not the resident-memory budget;
    // the engine pool and both continuous IO quotas remain finite and unchanged.

    metrics
}

/// Independent literal v1 pack locator oracle, outside the measured operation.
/// It does not deserialize JSON, read full packs, or authorize any page/root.
fn physical_member(path: &std::path::Path, recipe: bool) -> (u64, u64, Vec<u8>) {
    use std::io::{Read, Seek, SeekFrom};
    const MAGIC: &[u8] = b"oasis7.iws.page-pack.v1\0";
    let mut file = std::fs::File::open(path).unwrap();
    let length = file.metadata().unwrap().len();
    let mut prefix = [0u8; MAGIC.len()];
    assert_eq!(prefix.len(), MAGIC.len());
    file.read_exact(&mut prefix).unwrap();
    if prefix != MAGIC {
        if recipe {
            assert!(length <= 2048);
            file.seek(SeekFrom::Start(0)).unwrap();
            let mut bytes = vec![0; length as usize];
            file.read_exact(&mut bytes).unwrap();
            return (0, 0, bytes);
        }
        return (0, 0, Vec::new());
    }
    let mut header = [0u8; 23];
    file.read_exact(&mut header).unwrap();
    assert_eq!(u32::from_be_bytes(header[..4].try_into().unwrap()), 1);
    let count = u16::from_be_bytes(header[5..7].try_into().unwrap()) as usize;
    assert!((1..=256).contains(&count));
    let index = u64::from_be_bytes(header[7..15].try_into().unwrap());
    let total = u64::from_be_bytes(header[15..23].try_into().unwrap());
    assert_eq!(total, length);
    assert_eq!(index + count as u64 * 44, total);
    let wanted = *blake3::Hash::from_hex(path.file_name().unwrap().to_str().unwrap())
        .unwrap()
        .as_bytes();
    let (mut low, mut high, mut probes) = (0, count, 0u64);
    while low < high {
        let middle = low + (high - low) / 2;
        let mut row = [0u8; 44];
        file.seek(SeekFrom::Start(index + middle as u64 * 44))
            .unwrap();
        file.read_exact(&mut row).unwrap();
        probes += 1;
        match row[..32].cmp(&wanted) {
            std::cmp::Ordering::Less => low = middle + 1,
            std::cmp::Ordering::Greater => high = middle,
            std::cmp::Ordering::Equal => {
                assert!(probes <= u64::from(count.ilog2()) + 1);
                let offset = u64::from_be_bytes(row[32..40].try_into().unwrap());
                let size = u32::from_be_bytes(row[40..44].try_into().unwrap()) as usize;
                assert!(size > 0 && offset + size as u64 <= index);
                if recipe {
                    assert!(size <= 2048);
                    let mut bytes = vec![0; size];
                    file.seek(SeekFrom::Start(offset)).unwrap();
                    file.read_exact(&mut bytes).unwrap();
                    return (1, probes, bytes);
                }
                return (1, probes, Vec::new());
            }
        }
    }
    panic!("actual node has no original pack locator");
}
fn audit_locators(root: &std::path::Path, trace: &Trace) -> [(u64, u64); 3] {
    const MAGIC: &[u8] = b"oasis7.iws.page-recipe.v1\0";
    let mut sums = [(0u64, 0u64); 3];
    for (channel, id) in trace.rows.borrow().iter() {
        let (packs, probes, recipe) = physical_member(
            &root
                .join("recipes")
                .join(blake3::Hash::from(*id).to_hex().as_str()),
            true,
        );
        sums[*channel].0 += packs;
        sums[*channel].1 += probes;
        assert!(recipe.starts_with(MAGIC));
        let header = MAGIC.len() + 32 + 4 + 1;
        let count = recipe[header - 1] as usize;
        assert!(count <= 17);
        assert_eq!(recipe.len(), header + count * 69 + 32);
        assert_eq!(&recipe[MAGIC.len()..MAGIC.len() + 32], id);
        for row in recipe[header..recipe.len() - 32].chunks_exact(69) {
            let stored: [u8; 32] = row[..32].try_into().unwrap();
            let (packs, probes, _) = physical_member(
                &root
                    .join("frames")
                    .join(blake3::Hash::from(stored).to_hex().as_str()),
                false,
            );
            sums[*channel].0 += packs;
            sums[*channel].1 += probes;
        }
    }
    sums
}
