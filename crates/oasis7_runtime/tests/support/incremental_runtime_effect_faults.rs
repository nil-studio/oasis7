use super::*;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_runtime::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1,
};
use std::{cell::Cell, sync::Arc};
struct Observe<'a, W> {
    inner: W,
    armed: &'a Cell<bool>,
    puts: usize,
    signal: &'a Cell<bool>,
    mode: usize,
}
impl<W: ValueNodeWriter> ValueNodeWriter for Observe<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        data: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.inner.put_node(id, data)?;
        if self.armed.get() {
            self.puts += 1;
            if self.puts == 2 {
                if self.mode == 3 {
                    return Err(ValueTreeError::Io(std::io::Error::other(
                        "after real complete effect RootRef put",
                    )));
                }
                if matches!(self.mode, 4 | 5) {
                    self.signal.set(true);
                }
            }
        }
        Ok(())
    }
}
impl<W: ValueNodeReader> ValueNodeReader for Observe<'_, W> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.inner.read_node(id, max, memory, class)
    }
}
impl<W: StatePageWriter> StatePageWriter for Observe<'_, W> {
    fn put_state_pages_cancelled(
        &mut self,
        delta: &ReservedValue<StateDeltaV1>,
        memory: &MemoryBudget,
        cancel: &mut dyn FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        self.inner.put_state_pages_cancelled(delta, memory, cancel)
    }
}
fn value(error: WorldFieldOverlayErrorV1) -> ValueTreeError {
    match error {
        WorldFieldOverlayErrorV1::Value(e)
        | WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(e)) => e,
        other => panic!("unexpected effect error: {other:?}"),
    }
}
#[test]
fn typed_prefix_is_poisoned_by_effect_noop_and_real_put_failures() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("faults"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let intent = EffectIntent {
        birth_era: None,
        intent_id: "id".into(),
        kind: "kind".into(),
        params: serde_json::Value::Bool(true),
        cap_ref: "cap".into(),
        origin: EffectOrigin::System,
    };
    let roots = super::scaling::seed(&store, &memory, intent.clone(), 165);
    let state = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let source = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
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
    let family = qualify_runtime_effect_records_v1(
        &roots,
        &mut ctx,
        &mut StateScanBudget {
            nodes_remaining: 100000,
            leaves_remaining: 1000,
            read_bytes_remaining: 64 << 20,
        },
        &mut RuntimeEffectJsonWorkV1 {
            cells_remaining: 29,
            json_nodes_remaining: 1,
            max_depth: 0,
            max_payload_bytes: 165,
        },
    )
    .unwrap();
    let proof = open_runtime_effect_record_v1(
        RuntimeEffectRecordKeyV1::Pending(0),
        &roots,
        &family,
        &mut ctx,
    )
    .unwrap()
    .unwrap();
    // Frozen public trace at fd855afb9, probe84296: C=W=13037, with
    // all 32 direct value puts complete. This is a fixed last-callback oracle,
    // not a self-calibrating or independent getter N+1 claim. The positive mode
    // must keep that exact trace, so callback-layout changes require review.
    const LAST_CALLBACK: usize = 13037;
    for mode in 0..8 {
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let source = store.value_session(physical());
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
            parent,
            [4; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 64,
                max_writes: 32,
                max_facts: 0,
                max_pending_node_bytes: 2 << 20,
            },
        )
        .unwrap();
        let armed = Cell::new(false);
        let signal = Cell::new(false);
        let c = Cell::new(0usize);
        let w = Cell::new(0usize);
        let mut writer = Observe {
            inner: store.value_session(physical()),
            armed: &armed,
            puts: 0,
            signal: &signal,
            mode,
        };
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut write_io = logical();
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
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
            cancelled: || {
                if !armed.get() {
                    return false;
                }
                c.set(c.get() + 1);
                (mode == 1 && c.get() >= LAST_CALLBACK) || (mode == 4 && signal.get())
            },
        };
        let mut write = MemberRecordWriteContext {
            sink: &mut writer,
            memory: &memory,
            io: &mut write_io,
            key_work: &mut write_keys,
            limits: limits(),
            cancelled: || {
                if !armed.get() {
                    return false;
                }
                w.set(w.get() + 1);
                (mode == 2 && w.get() >= LAST_CALLBACK) || (mode == 5 && signal.get())
            },
        };
        let mut work = RuntimeEffectRecordsWorkV1 {
            cells_remaining: 512,
            json_nodes_remaining: 16,
            max_json_depth: 1,
            max_pending_node_bytes: 2 << 20,
        };
        for change in [
            RuntimeEffectMembershipChangeV1::Handoff {
                ordinal: 0,
                expected_body: proof.root(),
                inflight_key: "existing",
                expected_inflight: None,
            },
            RuntimeEffectMembershipChangeV1::AppendPending { intent: &intent },
        ] {
            let guard = overlay.transaction.guard();
            change_runtime_effect_membership_v1(
                change,
                &family,
                &mut overlay,
                &mut write,
                &mut work,
                &guard,
                &live,
            )
            .unwrap();
        }
        armed.set(true);
        let fault_key = format!("fault-{mode}");
        let wrong = StateRootV1::empty(StateNamespaceV1::Authorization).unwrap();
        let change = match mode {
            0..=2 => RuntimeEffectMembershipChangeV1::PutInflight {
                key: "existing",
                expected_body: Some(proof.root()),
                intent: &intent,
            },
            6 => RuntimeEffectMembershipChangeV1::AppendPending { intent: &intent },
            7 => RuntimeEffectMembershipChangeV1::RemovePending {
                ordinal: 1,
                expected_body: &wrong,
            },
            _ => RuntimeEffectMembershipChangeV1::Handoff {
                ordinal: 1,
                expected_body: proof.root(),
                inflight_key: &fault_key,
                expected_inflight: None,
            },
        };
        if mode == 6 {
            work.json_nodes_remaining = 0;
        }
        let guard = overlay.transaction.guard();
        let result = change_runtime_effect_membership_v1(
            change,
            &family,
            &mut overlay,
            &mut write,
            &mut work,
            &guard,
            &live,
        );
        if mode == 0 {
            result.unwrap();
            assert_eq!(writer.puts, 32);
            assert_eq!(c.get(), LAST_CALLBACK);
            assert_eq!(w.get(), LAST_CALLBACK);
            eprintln!(
                "effect NoOp frozen trace C={} W={} direct_value_puts={}",
                c.get(),
                w.get(),
                writer.puts
            );
            let guard = tx.guard();
            let sealed = tx.seal(&guard, &live, 1).unwrap();
            assert_eq!(sealed.record.writes.len(), 6);
        } else {
            let error = value(result.unwrap_err());
            match mode {
                1 | 2 | 4 | 5 => assert!(matches!(error, ValueTreeError::Cancelled), "{error:?}"),
                3 => assert!(matches!(error, ValueTreeError::Io(_)), "{error:?}"),
                6 => assert!(
                    matches!(error, ValueTreeError::ResourceLimited("JSON nodes")),
                    "{error:?}"
                ),
                7 => assert!(matches!(error, ValueTreeError::Invalid(_)), "{error:?}"),
                _ => unreachable!(),
            };
            if matches!(mode, 3..=5) {
                assert_eq!(writer.puts, 2);
            }
            if matches!(mode, 1 | 2) {
                assert_eq!(writer.puts, 32);
            }
            let guard = tx.guard();
            assert!(matches!(
                tx.seal(&guard, &live, 1),
                Err(ExecutionError::Aborted)
            ));
        }
    }
}

#[test]
fn real_world_effect_genesis_borrows_original_empty_collections() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(8 << 20, 4 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &directory.0.join("world-gen"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let world = oasis7_runtime::World::new();
    assert_eq!(world.pending_effects_len(), 0);
    let mut writer = store.value_session(physical());
    let mut io = logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut work = RuntimeEffectRecordsWorkV1 {
        cells_remaining: 8,
        json_nodes_remaining: 0,
        max_json_depth: 0,
        max_pending_node_bytes: 2 << 20,
    };
    let mut rows = 0;
    assert_eq!(
        world
            .prepare_incremental_effect_records_genesis_v1(
                &mut MemberRecordWriteContext {
                    sink: &mut writer,
                    memory: &memory,
                    io: &mut io,
                    key_work: &mut keys,
                    limits: limits(),
                    cancelled: || false
                },
                &mut work,
                &mut |row| {
                    assert_eq!(row.namespace, StateNamespaceV1::Effects);
                    rows += 1;
                    Ok(())
                }
            )
            .unwrap(),
        0
    );
    assert_eq!(rows, 8);
    assert_eq!(work.cells_remaining, 0);
    assert_eq!(world.pending_effects_len(), 0);
}
