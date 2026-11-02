use super::*;
use oasis7_proto::memory_budget::ReservedValue;
use std::cell::Cell;
struct Meter<'a, R> {
    source: &'a R,
    forbidden: [[u8; 32]; 2],
    hits: Cell<u64>,
    nodes: Cell<u64>,
    bytes: Cell<u64>,
    largest: Cell<usize>,
}
impl<R: ValueNodeReader> ValueNodeReader for Meter<'_, R> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        if self.forbidden.contains(id) {
            self.hits.set(self.hits.get() + 1);
            return Err(ValueTreeError::Invalid(
                "unchanged effect body/JSON traversed",
            ));
        }
        let data = self.source.read_node(id, max, memory, class)?;
        self.nodes.set(self.nodes.get() + 1);
        self.bytes.set(self.bytes.get() + data.len() as u64);
        self.largest.set(self.largest.get().max(data.len()));
        Ok(data)
    }
}
fn digest(id: &str) -> [u8; 32] {
    *blake3::Hash::from_hex(id).unwrap().as_bytes()
}
fn bootstrap(size: usize) -> PageIoBudget {
    PageIoBudget {
        read_bytes_remaining: (size as u64) * 12 + (256 << 20),
        ..physical()
    }
}
fn bootstrap_io(size: usize) -> ValueIoBudget {
    ValueIoBudget {
        read_bytes_remaining: (size as u64) * 12 + (256 << 20),
        ..logical()
    }
}
pub(super) fn seed(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    intent: EffectIntent,
    size: usize,
) -> RootSetV1 {
    let pending = VecDeque::from([intent]);
    let inflight = BTreeMap::new();
    let mut writer = store.value_session(bootstrap(size));
    let mut io = bootstrap_io(size);
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut rows = Vec::with_capacity(9);
    let mut work = RuntimeEffectRecordsWorkV1 {
        cells_remaining: 29,
        json_nodes_remaining: 1,
        max_json_depth: 0,
        max_pending_node_bytes: 2 << 20,
    };
    prepare_runtime_effect_records_genesis_v1(
        &RuntimeEffectRecordsRefV1 {
            pending: &pending,
            inflight: &inflight,
        },
        &mut MemberRecordWriteContext {
            sink: &mut writer,
            memory,
            io: &mut io,
            key_work: &mut keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: (size as u64).max(165),
                record_bytes: size as u64 + 8192,
            },
            cancelled: || false,
        },
        &mut work,
        &mut |row| {
            rows.push(row);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(work.cells_remaining, 0);
    assert_eq!(work.json_nodes_remaining, 0);
    assert_eq!(rows.len(), 9);
    let state = store
        .state_session(bootstrap(size), MemoryClass::Ordinary)
        .unwrap();
    let mut writes = rows
        .into_iter()
        .map(|row| StateWriteV1 {
            key: row.mutation.key.clone(),
            before: None,
            after: row.mutation.after,
        })
        .collect::<Vec<_>>();
    writes.sort_by(|a, b| a.key.cmp(&b.key));
    let charge = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let delta = charge.retain(
        StateDeltaV1::build(
            StateRootV1::empty(StateNamespaceV1::Effects).unwrap(),
            writes,
            &state,
            2 << 20,
        )
        .unwrap(),
    );
    writer.put_state_pages(&delta, memory).unwrap();
    let mut roots = RootSetV1::empty("effect-scaling".into(), [0x3c; 32]).unwrap();
    *roots
        .state
        .iter_mut()
        .find(|r| r.namespace == StateNamespaceV1::Effects)
        .unwrap() = delta.after.clone();
    roots
}
#[test]
fn unchanged_params_do_not_enter_handoff_sender_or_complete_typed_receiver() {
    use oasis7_runtime::incremental_field_overlay::WorldFieldOverlayContextV1;
    let mut observations = Vec::new();
    for size in [64usize, 32 << 20, 64 << 20] {
        let directory = Directory::new();
        let path = directory.0.join("source");
        let roots = {
            let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
            let store =
                IncrementalPageStore::create(&path, memory.clone(), PageCompression::Raw).unwrap();
            let mut text = String::with_capacity(size);
            let mut block = 0u64;
            while text.len() < size {
                let prefix = format!("{block:016x}");
                let n = (size - text.len()).min(65536);
                text.push_str(&prefix[..n.min(prefix.len())]);
                for _ in prefix.len().min(n)..n {
                    text.push('x');
                }
                block += 1;
            }
            assert_eq!(text.len(), size);
            seed(
                &store,
                &memory,
                EffectIntent {
                    birth_era: None,
                    intent_id: "id".into(),
                    kind: "kind".into(),
                    params: serde_json::Value::String(text),
                    cap_ref: "cap".into(),
                    origin: EffectOrigin::System,
                },
                size,
            )
        }; // Original DTO, source pool and warm producer are destroyed here.
        let memory = MemoryBudget::new(24 << 20, 4 << 20).unwrap();
        if size != 64 {
            assert!((24usize << 20) < size);
        }
        let store = IncrementalPageStore::open(
            &path,
            memory.clone(),
            PageCompression::Raw,
            MemoryClass::Ordinary,
        )
        .unwrap();
        let family = {
            let state = store
                .state_session(bootstrap(size), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(bootstrap(size));
            let mut io = bootstrap_io(size);
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut ctx = WorldFieldReadContextV1 {
                root: roots.state_root(StateNamespaceV1::Effects),
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io,
                },
                key_work: &mut keys,
                semantic_bytes: 4096,
                record_bytes: size as u64 + 8192,
                take_state_error: || {
                    state
                        .take_read_error()
                        .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
                },
                cancelled: || false,
            };
            let mut work = RuntimeEffectJsonWorkV1 {
                cells_remaining: 29,
                json_nodes_remaining: 1,
                max_depth: 0,
                max_payload_bytes: size as u64,
            };
            let family = qualify_runtime_effect_records_v1(
                &roots,
                &mut ctx,
                &mut StateScanBudget {
                    nodes_remaining: 100000,
                    leaves_remaining: 1000,
                    read_bytes_remaining: 64 << 20,
                },
                &mut work,
            )
            .unwrap();
            assert_eq!(work.cells_remaining, 0);
            assert_eq!(work.json_nodes_remaining, 0);
            family
        };
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let source = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let proof = {
            let mut ctx = WorldFieldReadContextV1 {
                root: roots.state_root(StateNamespaceV1::Effects),
                state: &state,
                values: ValueReadContext {
                    source: &source,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io,
                },
                key_work: &mut keys,
                semantic_bytes: 4096,
                record_bytes: 8192,
                take_state_error: || {
                    state
                        .take_read_error()
                        .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
                },
                cancelled: || false,
            };
            open_runtime_effect_record_v1(
                RuntimeEffectRecordKeyV1::Pending(0),
                &roots,
                &family,
                &mut ctx,
            )
            .unwrap()
            .unwrap()
        };
        let json = {
            let mut ctx = WorldFieldReadContextV1 {
                root: proof.root(),
                state: &state,
                values: ValueReadContext {
                    source: &source,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io,
                },
                key_work: &mut keys,
                semantic_bytes: 4096,
                record_bytes: 8192,
                take_state_error: || {
                    state
                        .take_read_error()
                        .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
                },
                cancelled: || false,
            };
            read_qualified_runtime_effect_params_root_v1(&proof, &mut ctx).unwrap()
        };
        // Fixed operation scope starts after full bootstrap and point resolution.
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let source = store.value_session(physical());
        let meter = Meter {
            source: &source,
            forbidden: [digest(&proof.root().node_root), digest(&json.node_root)],
            hits: Cell::new(0),
            nodes: Cell::new(0),
            bytes: Cell::new(0),
            largest: Cell::new(0),
        };
        let mut io = ValueIoBudget {
            nodes_remaining: 16384,
            read_bytes_remaining: 4 << 20,
            write_bytes_remaining: 4 << 20,
        };
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let parent = ExecutionParent {
            roots: roots.clone(),
            commit: [9; 32],
            authority: AuthorityBinding {
                token: [10; 32],
                epoch: 11,
            },
        };
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [12; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 32,
                max_writes: 16,
                max_facts: 0,
                max_pending_node_bytes: 2 << 20,
            },
        )
        .unwrap();
        let mut writer = store.value_session(physical());
        let mut write_io = logical();
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &meter,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || {
                state
                    .take_read_error()
                    .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
            },
            cancelled: || false,
        };
        let guard = overlay.transaction.guard();
        change_runtime_effect_membership_v1(
            RuntimeEffectMembershipChangeV1::Handoff {
                ordinal: 0,
                expected_body: proof.root(),
                inflight_key: "same-identity",
                expected_inflight: None,
            },
            &family,
            &mut overlay,
            &mut MemberRecordWriteContext {
                sink: &mut writer,
                memory: &memory,
                io: &mut write_io,
                key_work: &mut write_keys,
                limits: limits(),
                cancelled: || false,
            },
            &mut RuntimeEffectRecordsWorkV1 {
                cells_remaining: 8,
                json_nodes_remaining: 0,
                max_json_depth: 0,
                max_pending_node_bytes: 2 << 20,
            },
            &guard,
            &live,
        )
        .unwrap();
        let sender = (meter.nodes.get(), meter.bytes.get());
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        let mut receiver = WorldFieldReadContextV1 {
            root: sealed.record.after.state_root(StateNamespaceV1::Effects),
            state: &state,
            values: ValueReadContext {
                source: &meter,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || {
                state
                    .take_read_error()
                    .map(|e| ValueTreeError::Io(std::io::Error::other(e.to_string())))
            },
            cancelled: || false,
        };
        let after = accept_runtime_effect_transition_v1(
            &family,
            &TransitionVerifier {
                parent: &parent,
                operation: [12; 32],
                source: &state,
                memory: &memory,
                class: MemoryClass::Ordinary,
                max_pending_node_bytes: 2 << 20,
            },
            &sealed.record,
            &mut receiver,
            &mut RuntimeEffectJsonWorkV1 {
                cells_remaining: 64,
                json_nodes_remaining: 0,
                max_depth: 0,
                max_payload_bytes: 165,
            },
        )
        .unwrap();
        assert_eq!(after.record_counts(), [0, 1]);
        assert_eq!(meter.hits.get(), 0);
        assert!(meter.largest.get() <= 8192);
        let receiver_counts = (meter.nodes.get() - sender.0, meter.bytes.get() - sender.1);
        eprintln!(
            "effect unchanged bytes={size} sender={sender:?} typed_owner={receiver_counts:?} logical_total_remaining={io:?} state_physical={:?} values_physical={:?} writer_physical={:?} max_returned={} forbidden_hits={}",
            state.remaining_io().unwrap(),
            source.remaining_io().unwrap(),
            writer.remaining_io().unwrap(),
            meter.largest.get(),
            meter.hits.get()
        );
        observations.push((sender, receiver_counts));
    }
    assert_eq!(observations[0], observations[1]);
    assert_eq!(observations[1], observations[2]);
}
