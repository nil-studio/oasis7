use super::*;
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionError, ExecutionParent, ExecutionTransaction, TransactionLimits,
    },
    incremental_value::{ValueNodeReader, ValueNodeWriter},
};
use oasis7_runtime::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1,
};
use std::{cell::Cell, sync::Arc};
struct Writer<'a, W> {
    source: &'a mut W,
    enabled: &'a Cell<bool>,
    written: &'a Cell<usize>,
    cancel: &'a Cell<bool>,
    mode: usize,
}
impl<W: ValueNodeReader> ValueNodeReader for Writer<'_, W> {
    fn read_node(
        &self,
        id: &[u8; 32],
        max: usize,
        memory: &MemoryBudget,
        class: MemoryClass,
    ) -> Result<Arc<ReservedValue<Vec<u8>>>, ValueTreeError> {
        self.source.read_node(id, max, memory, class)
    }
}
impl<W: ValueNodeWriter> ValueNodeWriter for Writer<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        if self.enabled.get() && self.mode == 2 && self.written.get() == 1 {
            return Err(ValueTreeError::Io(std::io::Error::other(
                "diplomacy injected put",
            )));
        }
        self.source.put_node(id, bytes)?;
        if self.enabled.get() {
            self.written.set(self.written.get() + 1);
            if self.mode == 1 {
                self.cancel.set(true);
            }
        }
        Ok(())
    }
}
impl<W: StatePageWriter> StatePageWriter for Writer<'_, W> {
    fn put_state_pages_cancelled(
        &mut self,
        delta: &ReservedValue<StateDeltaV1>,
        memory: &MemoryBudget,
        cancel: &mut dyn FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        self.source.put_state_pages_cancelled(delta, memory, cancel)
    }
}
#[test]
fn final_noop_and_real_put_failures_poison_a_successful_typed_prefix() {
    let final_noop = Cell::new(0usize);
    // Baseline measures the public callback boundary; subsequent identical NoOp
    // cancels exactly at its observed final check (no production test hook).
    for mode in 0..7 {
        let directory = Directory::new();
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store = IncrementalPageStore::create(
            &directory.0.join("fault"),
            memory.clone(),
            PageCompression::Raw,
        )
        .unwrap();
        let before = build(
            &store,
            &memory,
            &produce(&store, &memory, &WorldState::default()),
        );
        let parent = ExecutionParent {
            roots: roots(&before),
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
        let mut read = fixed!(&before, &state, &values, &memory, &mut io, &mut keys);
        let family = qualify_diplomacy_family_v1(
            &parent.roots,
            &mut read,
            &mut scan(),
            &mut scan(),
            &mut 8192,
            165,
        )
        .unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [6; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 160,
                max_writes: 96,
                max_facts: 4,
                max_pending_node_bytes: 4 << 20,
            },
        )
        .unwrap();
        let active = Cell::new(false);
        let callbacks = Cell::new(0usize);
        let cancelled = Cell::new(false);
        let written = Cell::new(0usize);
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
            record_bytes: 8192,
            take_state_error: || state.take_read_error().map(state_error),
            cancelled: || {
                if active.get() {
                    callbacks.set(callbacks.get() + 1);
                    if mode == 3 && callbacks.get() >= final_noop.get() {
                        return true;
                    }
                }
                cancelled.get()
            },
        };
        let mut backend = store.value_session(physical());
        let mut writer = Writer {
            source: &mut backend,
            enabled: &active,
            written: &written,
            cancel: &cancelled,
            mode,
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
            cancelled: || false,
        };
        let mut alliance = source().alliances.remove(KEY).unwrap();
        alliance.members.clear();
        let guard = overlay.transaction.guard();
        let proof = create_qualified_diplomacy_empty_v1(
            DiplomacyEmptyCreateV1 {
                id: KEY,
                source: DiplomacySourceV1::Alliance(&alliance),
                family: &family,
                max_pending_node_bytes: 4 << 20,
            },
            &mut overlay,
            &mut write,
            &guard,
            &live,
            &mut 128,
        )
        .unwrap();
        let prefix_recipes = std::fs::read_dir(store.root().join("recipes"))
            .unwrap()
            .count();
        assert!(prefix_recipes > 0);
        let mut after = alliance.clone();
        if matches!(mode, 1 | 2) {
            after.charter = "new-value".into();
        }
        active.set(true);
        callbacks.set(0);
        written.set(0);
        if mode == 4 {
            overlay.values.io.nodes_remaining = 0;
        }
        if mode == 5 {
            overlay.key_work.bytes_remaining = 0;
        }
        let guard = overlay.transaction.guard();
        let result = if mode == 6 {
            change_qualified_diplomacy_member_v1(
                DiplomacyMemberChangeV1 {
                    field: DiplomacyFieldV1::Alliances,
                    id: KEY,
                    qualification: &proof,
                    expected_length: 1,
                    ordinal: 1,
                    expected: None,
                    after: Some(DiplomacyMemberV1::Alliance("hole")),
                    max_current_payload_bytes: 165,
                    max_pending_node_bytes: 4 << 20,
                },
                &mut overlay,
                &mut write,
                &guard,
                &live,
            )
        } else {
            change_qualified_diplomacy_scalars_v1(
                DiplomacyScalarsChangeV1 {
                    id: KEY,
                    qualification: &proof,
                    expected: DiplomacySourceV1::Alliance(&alliance),
                    after: DiplomacySourceV1::Alliance(&after),
                    max_current_payload_bytes: 165,
                    max_pending_node_bytes: 4 << 20,
                },
                &mut overlay,
                &mut write,
                &guard,
                &live,
            )
        };
        match mode {
            0 => {
                assert!(result.unwrap().is_none());
                assert_eq!(written.get(), 0);
                assert!(callbacks.get() > 1);
                final_noop.set(callbacks.get());
                eprintln!("63a29 public final NoOp callback={}", callbacks.get());
                continue;
            }
            1 | 3 => assert!(
                matches!(
                    result,
                    Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled)
                        | WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                            ValueTreeError::Cancelled
                        )))
                ),
                "mode {mode}: {result:?}, puts={}",
                written.get()
            ),
            2 => assert!(
                matches!(
                    result,
                    Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Io(_))
                        | WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                            ValueTreeError::Io(_)
                        )))
                ),
                "mode {mode}: {result:?}, puts={}",
                written.get()
            ),
            4 | 5 => assert!(
                matches!(
                    result,
                    Err(WorldFieldOverlayErrorV1::Value(
                        ValueTreeError::ResourceLimited(_)
                    ))
                ),
                "mode {mode}: {result:?}"
            ),
            6 => assert!(matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
            )),
            _ => unreachable!(),
        }
        if matches!(mode, 1 | 2) {
            assert_eq!(written.get(), 1);
            assert!(
                std::fs::read_dir(store.root().join("recipes"))
                    .unwrap()
                    .count()
                    > prefix_recipes,
                "new real put after successful prefix"
            );
        }
        if mode == 3 {
            assert_eq!(callbacks.get(), final_noop.get());
            assert_eq!(written.get(), 0);
        }
        let guard = overlay.transaction.guard();
        assert!(matches!(
            tx.seal(&guard, &live, 1),
            Err(ExecutionError::Aborted)
        ));
    }
}
