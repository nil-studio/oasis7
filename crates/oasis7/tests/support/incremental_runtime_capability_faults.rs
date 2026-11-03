use super::*;
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_runtime::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1,
};
use std::cell::Cell;
struct Fault<'a, W> {
    sink: &'a mut W,
    armed: &'a Cell<bool>,
    stopped: &'a Cell<bool>,
    hits: &'a Cell<u64>,
    routing: bool,
    cancel: bool,
}
impl<W: ValueNodeWriter> ValueNodeWriter for Fault<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        let root_ref = bytes
            .windows(FIELD_MEMBER_DOMAIN.len())
            .position(|w| w == FIELD_MEMBER_DOMAIN)
            .is_some_and(|p| bytes.get(p + FIELD_MEMBER_DOMAIN.len() + 2) == Some(&10));
        self.sink.put_node(id, bytes)?;
        if self.armed.get() && (!self.routing || root_ref) {
            self.hits.set(self.hits.get() + 1);
            if self.cancel {
                self.stopped.set(true);
            } else {
                return Err(ValueTreeError::Io(std::io::Error::other(
                    "after real capability typed put",
                )));
            }
        }
        Ok(())
    }
}
impl<W: ValueNodeWriter> StatePageWriter for Fault<'_, W> {}
impl<W: ValueNodeReader> ValueNodeReader for Fault<'_, W> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<std::sync::Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.sink.read_node(id, max, memory, class)
    }
}
#[test]
fn capability_real_put_and_admission_failures_poison_successful_prefix() {
    for mode in 0..6 {
        let compression = PageCompression::Raw;
        let directory = Directory::new();
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store =
            IncrementalPageStore::create(&directory.0.join("ops"), memory.clone(), compression)
                .unwrap();
        let roots = seed(&store, &memory);
        let parent = ExecutionParent {
            roots: roots.clone(),
            commit: [4; 32],
            authority: AuthorityBinding {
                token: [5; 32],
                epoch: 7,
            },
        };
        let live = parent.fence().unwrap();
        let state = store
            .state_session(physical(), MemoryClass::Ordinary)
            .unwrap();
        let values = store.value_session(physical());
        let mut io = logical();
        let mut keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut ctx = read_ctx!(
            roots
                .state
                .iter()
                .find(|r| r.namespace == StateNamespaceV1::Authorization)
                .unwrap(),
            &state,
            &values,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let family = qualify_runtime_capability_family_v1(
            &roots,
            &mut ctx,
            &mut scan(),
            &mut scan(),
            &mut 1000,
            165,
        )
        .unwrap();
        let key = RuntimeCapabilityRecordKeyV1 {
            kind: RuntimeCapabilityRecordKindV1::Budget,
            id: "agent-key",
        };
        let mut budget =
            open_qualified_runtime_capability_record_v1(key, &roots, &family, &mut ctx)
                .unwrap()
                .unwrap();

        let armed = Cell::new(false);
        let stopped = Cell::new(false);
        let hits = Cell::new(0);
        let mut tx = ExecutionTransaction::begin(
            parent,
            [6; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 160,
                max_writes: 16,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        let mut overlay_io = logical();
        let mut overlay_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut overlay_io,
            },
            key_work: &mut overlay_keys,
            semantic_bytes: 1024,
            record_bytes: 4096,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || stopped.get(),
        };
        let mut physical_writer = store.value_session(physical());
        let mut writer = Fault {
            sink: &mut physical_writer,
            armed: &armed,
            stopped: &stopped,
            hits: &hits,
            routing: mode >= 2,
            cancel: mode % 2 == 1,
        };
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
            cancelled: || stopped.get(),
        };
        use RuntimeCapabilityValueRefV1 as V;
        let guard = overlay.transaction.guard();
        budget = change_qualified_runtime_capability_cell_v1(
            RuntimeCapabilityPointChangeV1 {
                request: RuntimeCapabilityPointRequestV1 {
                    key,
                    column: RuntimeCapabilityColumnV1::RemainingUnits,
                    qualification: &budget,
                },
                expected: V::Signed(i64::MIN),
                after: V::Signed(17),
                max_current_payload_bytes: 165,
                max_pending_node_bytes: 4 << 20,
            },
            &mut overlay,
            &mut write,
            &guard,
            &live,
        )
        .unwrap()
        .unwrap();
        armed.set(mode < 4);
        if mode == 5 {
            overlay.values.io.nodes_remaining = 0;
        }
        let guard = overlay.transaction.guard();
        let result = change_qualified_runtime_capability_cell_v1(
            RuntimeCapabilityPointChangeV1 {
                request: RuntimeCapabilityPointRequestV1 {
                    key,
                    column: if mode == 4 {
                        RuntimeCapabilityColumnV1::State
                    } else {
                        RuntimeCapabilityColumnV1::ReservedUnits
                    },
                    qualification: &budget,
                },
                expected: V::Signed(0),
                after: V::Signed(1),
                max_current_payload_bytes: 165,
                max_pending_node_bytes: 4 << 20,
            },
            &mut overlay,
            &mut write,
            &guard,
            &live,
        )
        .map(|_| ());
        if mode >= 4 {
            if mode == 4 {
                assert!(
                    matches!(
                        result,
                        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
                    ),
                    "mode{mode}: {result:?}"
                );
            } else {
                assert!(
                    matches!(
                        result,
                        Err(WorldFieldOverlayErrorV1::Value(
                            ValueTreeError::ResourceLimited(_)
                        ))
                    ),
                    "mode{mode}: {result:?}"
                );
            }
            assert_eq!(hits.get(), 0);
            assert!(matches!(
                tx.seal(&guard, &live, 1),
                Err(ExecutionError::Aborted)
            ));
            continue;
        }
        if mode % 2 == 1 {
            assert!(
                matches!(
                    result,
                    Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
                        | Err(WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                            ValueTreeError::Cancelled
                        )))
                ),
                "mode={mode}: {result:?}"
            );
        } else {
            assert!(
                matches!(
                    result,
                    Err(WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                        ValueTreeError::Io(_)
                    ))) | Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Io(_)))
                ),
                "mode={mode}: {result:?}"
            );
        }
        assert_eq!(
            hits.get(),
            1,
            "must be after a real{} put",
            if mode >= 2 { " RootRef" } else { " scalar" }
        );
        assert!(matches!(
            tx.seal(&guard, &live, 1),
            Err(ExecutionError::Aborted)
        ));
    }
}
