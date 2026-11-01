use super::*;
use oasis7_runtime::incremental_field_overlay::{
    WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1,
};
use std::cell::Cell;
use std::sync::Arc;
#[test]
fn reward_genesis_limits_and_final_emit_cancellation_never_complete() {
    for mode in 0..7 {
        let directory = Directory::new();
        let memory = MemoryBudget::new(if mode == 4 { 8192 } else { 64 << 20 }, 0).unwrap();
        let store = IncrementalPageStore::create(
            &directory.0.join("producer"),
            memory.clone(),
            PageCompression::Raw,
        )
        .unwrap();
        let mut backend = store.value_session(physical());
        let mut io = logical();
        if mode == 3 {
            io.nodes_remaining = 0;
        }
        let mut keys = SemanticKeyBudget {
            bytes_remaining: if mode == 2 { 0 } else { 1 << 20 },
        };
        let emitted = Cell::new(0usize);
        let stop = Cell::new(mode == 0);
        let mut work = RewardRecordsWorkV1 {
            cells_remaining: if mode == 1 { 0 } else { 55 },
            max_pending_node_bytes: 2 << 20,
        };
        let result = prepare_reward_records_genesis_v1(
            &source(),
            &mut MemberRecordWriteContext {
                sink: &mut backend,
                memory: &memory,
                io: &mut io,
                key_work: &mut keys,
                limits: limits(),
                cancelled: || stop.get(),
            },
            &mut work,
            &mut |_| {
                emitted.set(emitted.get() + 1);
                if mode == 5 {
                    return Err(ValueTreeError::Io(std::io::Error::other(
                        "provisional emit",
                    )));
                }
                if mode == 6 && emitted.get() == 10 {
                    stop.set(true);
                }
                Ok(())
            },
        );
        match mode {
            0 | 6 => assert!(matches!(
                result,
                Err(MemberRecordError::Value(ValueTreeError::Cancelled))
            )),
            1 | 3 => assert!(matches!(
                result,
                Err(MemberRecordError::Value(ValueTreeError::ResourceLimited(_)))
            )),
            2 => assert!(matches!(
                result,
                Err(MemberRecordError::Key(SemanticKeyError::ResourceLimited))
            )),
            4 => assert!(matches!(
                result,
                Err(MemberRecordError::Value(ValueTreeError::Memory(_)))
            )),
            5 => assert!(matches!(
                result,
                Err(MemberRecordError::Value(ValueTreeError::Io(_)))
            )),
            _ => unreachable!(),
        }
        assert_eq!(
            emitted.get(),
            if mode == 5 || mode == 2 {
                1
            } else if mode == 6 {
                10
            } else {
                0
            }
        );
    }
}

struct LengthWriter<'a, W> {
    source: &'a mut W,
    target: &'a WorldFieldKeyV1,
    root_refs: &'a Cell<usize>,
    length_puts: &'a Cell<usize>,
    stop: &'a Cell<bool>,
    cancel: bool,
}
impl<W: ValueNodeReader> ValueNodeReader for LengthWriter<'_, W> {
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
impl<W: StatePageWriter> StatePageWriter for LengthWriter<'_, W> {
    fn put_state_pages_cancelled(
        &mut self,
        delta: &ReservedValue<StateDeltaV1>,
        memory: &MemoryBudget,
        cancel: &mut dyn FnMut() -> bool,
    ) -> Result<(), ValueTreeError> {
        self.source.put_state_pages_cancelled(delta, memory, cancel)
    }
}
impl<W: ValueNodeWriter> ValueNodeWriter for LengthWriter<'_, W> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        let header = bytes
            .windows(FIELD_MEMBER_DOMAIN.len())
            .position(|v| v == FIELD_MEMBER_DOMAIN);
        let is_root =
            header.is_some_and(|p| bytes.get(p + FIELD_MEMBER_DOMAIN.len() + 2) == Some(&10));
        let is_length = header.is_some_and(|p| {
            bytes.get(p + FIELD_MEMBER_DOMAIN.len() + 2) == Some(&2)
                && bytes.get(
                    p + FIELD_MEMBER_HEADER_BYTES
                        ..p + FIELD_MEMBER_HEADER_BYTES + self.target.as_bytes().len(),
                ) == Some(self.target.as_bytes())
        });
        self.source.put_node(id, bytes)?;
        if is_root {
            self.root_refs.set(self.root_refs.get() + 1);
        }
        if is_length {
            assert!(self.root_refs.get() > 0);
            self.length_puts.set(self.length_puts.get() + 1);
            if self.cancel {
                self.stop.set(true);
            } else {
                return Err(ValueTreeError::Io(std::io::Error::other(
                    "after reward length data physical put",
                )));
            }
        }
        Ok(())
    }
}
#[test]
fn append_length_publication_fault_poison_then_original_root_retry_succeeds() {
    for cancel in [false, true] {
        let directory = Directory::new();
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let store = IncrementalPageStore::create(
            &directory.0.join("late-length"),
            memory.clone(),
            PageCompression::Raw,
        )
        .unwrap();
        let roots = seed(&store, &memory, &source());
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
        let mut context = ctx!(
            &roots.state[0],
            &state,
            &values,
            &memory,
            MemoryClass::Ordinary,
            &mut io,
            &mut keys
        );
        let family = qualify_reward_family_v1(
            &roots,
            &mut context,
            &mut scan(),
            &mut scan(),
            &mut 8192,
            165,
        )
        .unwrap();
        context.root = roots
            .state
            .iter()
            .find(|r| r.namespace == StateNamespaceV1::Business)
            .unwrap();
        let key = RewardRecordKeyV1 {
            kind: RewardRecordKindV1::Pool,
            index: 7,
        };
        let base = open_qualified_reward_record_v1(key, &roots, &family, &mut context)
            .unwrap()
            .unwrap();
        let mut preimage = b"oasis7.iws.semantic-key.v1\0".to_vec();
        preimage.push(5);
        for text in ["reward_budget_record_closure_v1", "length"] {
            preimage.push(1);
            preimage.extend_from_slice(&(text.len() as u64).to_be_bytes());
            preimage.extend_from_slice(text.as_bytes());
        }
        let target = WorldFieldKeyV1::new(
            FieldDescriptor {
                owner: FieldOwner::WorldState,
                name: "reward_mint_records",
                shape: FieldShape::Ordered,
                destination: FieldDestination::State(StateNamespaceV1::Authorization),
            },
            WorldFieldRecordKindV1::Entry,
            Some(*blake3::hash(&preimage).as_bytes()),
        )
        .unwrap();
        let minted = NodeRewardMintRecord {
            epoch_index: 9,
            node_id: "late".into(),
            signature: "retry".into(),
            ..Default::default()
        };
        for retry in [false, true] {
            let root_refs = Cell::new(0);
            let length_puts = Cell::new(0);
            let stop = Cell::new(false);
            let state = store
                .state_session(physical(), MemoryClass::Ordinary)
                .unwrap();
            let values = store.value_session(physical());
            let mut tx = ExecutionTransaction::begin(
                parent.clone(),
                [6; 32],
                &state,
                &memory,
                TransactionLimits {
                    max_reads: 64,
                    max_writes: 8,
                    max_facts: 4,
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
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io,
                },
                key_work: &mut keys,
                semantic_bytes: 4096,
                record_bytes: 8192,
                take_state_error: || state.take_read_error().map(state_error),
                cancelled: || stop.get(),
            };
            let mut backend = store.value_session(physical());
            // A different impossible key disables injection for the legal retry. The same
            // original authenticated parent and source record are used after partial puts.
            let inert = WorldFieldKeyV1::new(
                FieldDescriptor {
                    owner: FieldOwner::WorldState,
                    name: "reward_mint_records",
                    shape: FieldShape::Ordered,
                    destination: FieldDestination::State(StateNamespaceV1::Authorization),
                },
                WorldFieldRecordKindV1::Entry,
                Some([0; 32]),
            )
            .unwrap();
            let mut writer = LengthWriter {
                source: &mut backend,
                target: if retry { &inert } else { &target },
                root_refs: &root_refs,
                length_puts: &length_puts,
                stop: &stop,
                cancel,
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
                cancelled: || stop.get(),
            };
            if !retry {
                let guard = overlay.transaction.guard();
                change_qualified_reward_cell_v1(
                    RewardCellChangeV1 {
                        key,
                        cell: RewardCellV1::Column(RewardColumnV1::RemainingCreditBudget),
                        qualification: &base,
                        expected: Some(RewardValueRefV1::U64(u64::MAX)),
                        after: Some(RewardValueRefV1::U64(0)),
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
                root_refs.set(0);
            }
            let before = std::fs::read_dir(store.root().join("recipes"))
                .unwrap()
                .count();
            let guard = overlay.transaction.guard();
            let result = append_qualified_reward_mint_v1(
                RewardMintAppendV1 {
                    expected_length: 3,
                    source: &minted,
                    family: &family,
                },
                &mut overlay,
                &mut write,
                &mut RewardRecordsWorkV1 {
                    cells_remaining: 100,
                    max_pending_node_bytes: 2 << 20,
                },
                &guard,
                &live,
            );
            if retry {
                result.unwrap();
                let guard = overlay.transaction.guard();
                let sealed = tx.seal(&guard, &live, 1).unwrap();
                assert_eq!(sealed.record.writes.len(), 2);
                let verifier = TransitionVerifier {
                    parent: &parent,
                    operation: [6; 32],
                    source: &state,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    max_pending_node_bytes: 4 << 20,
                };
                let mut io = logical();
                let mut keys = SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                };
                let mut context = ctx!(
                    &sealed.record.after.state[0],
                    &state,
                    &values,
                    &memory,
                    MemoryClass::Ordinary,
                    &mut io,
                    &mut keys
                );
                assert_eq!(
                    accept_reward_transition_v1(
                        &family,
                        &verifier,
                        &sealed.record,
                        &mut context,
                        &mut scan(),
                        &mut 8192,
                        165
                    )
                    .unwrap()
                    .mint_length(),
                    4
                );
            } else {
                if cancel {
                    assert!(matches!(
                        result,
                        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled)
                            | WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                                ValueTreeError::Cancelled
                            )))
                    ));
                } else {
                    assert!(matches!(
                        result,
                        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Io(_))
                            | WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                                ValueTreeError::Io(_)
                            )))
                    ));
                }
                assert_eq!(root_refs.get(), 1);
                assert_eq!(length_puts.get(), 1);
                assert!(
                    std::fs::read_dir(store.root().join("recipes"))
                        .unwrap()
                        .count()
                        > before
                );
                let guard = overlay.transaction.guard();
                assert!(matches!(
                    tx.seal(&guard, &live, 1),
                    Err(ExecutionError::Aborted)
                ));
            }
        }
    }
}
