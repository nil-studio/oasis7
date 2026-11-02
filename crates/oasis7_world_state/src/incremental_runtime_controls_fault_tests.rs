use super::*;
use crate::incremental_field_overlay::{WorldFieldOverlayContextV1, WorldFieldOverlayErrorV1};
use oasis7_formal_protocol::incremental_execution::*;
use std::cell::Cell;
struct FaultSink<'a> {
    extra: &'a RefCell<Pages>,
    enabled: &'a Cell<bool>,
    mode: u8,
    published: &'a Cell<usize>,
    cancelled: &'a Cell<bool>,
}
impl ValueNodeWriter for FaultSink<'_> {
    fn put_node(
        &mut self,
        id: [u8; 32],
        bytes: ReservedValue<Vec<u8>>,
    ) -> Result<(), ValueTreeError> {
        self.extra.borrow_mut().put_node(id, bytes)?;
        self.published.set(self.published.get() + 1);
        if self.enabled.get() {
            if self.mode == 1 {
                return Err(ValueTreeError::Io(std::io::ErrorKind::Other.into()));
            }
            self.cancelled.set(true);
        }
        Ok(())
    }
}
#[test]
fn expected_and_post_publication_failures_abort_the_successful_transaction_prefix() {
    use RuntimeControlFieldV1 as F;
    use RuntimeControlValueRefV1 as V;
    for mode in 0..4 {
        let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
        let (pages, roots) = seed(&memory, "source", None);
        let extra = RefCell::new(Pages::new(&memory));
        let values = Combined {
            base: &pages,
            extra: &extra,
        };
        let parent = ExecutionParent {
            roots,
            commit: [4; 32],
            authority: AuthorityBinding {
                token: [5; 32],
                epoch: 7,
            },
        };
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent,
            [6; 32],
            &pages,
            &memory,
            TransactionLimits {
                max_reads: 64,
                max_writes: 16,
                max_facts: 8,
                max_pending_node_bytes: 8 << 20,
            },
        )
        .unwrap();
        let enabled = Cell::new(false);
        let published = Cell::new(0);
        let cancelled = Cell::new(false);
        let mut sink = FaultSink {
            extra: &extra,
            enabled: &enabled,
            mode,
            published: &published,
            cancelled: &cancelled,
        };
        let mut reads = io();
        let mut writes = io();
        let mut read_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let mut write_keys = SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        {
            let mut read = WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut reads,
                },
                key_work: &mut read_keys,
                semantic_bytes: 1024,
                record_bytes: 2 << 20,
                take_state_error: || None,
                cancelled: || mode == 2 && cancelled.get(),
            };
            let mut write = MemberRecordWriteContext {
                sink: &mut sink,
                memory: &memory,
                io: &mut writes,
                key_work: &mut write_keys,
                limits: MemberRecordLimitsV1 {
                    semantic_bytes: 1024,
                    payload_bytes: 1 << 20,
                    record_bytes: 2 << 20,
                },
                cancelled: || mode == 3 && cancelled.get(),
            };
            let guard = read.transaction.guard();
            assert!(
                change_runtime_control_overlay_v1(
                    RuntimeControlChangeV1 {
                        field: F::NextGovernanceIdentityPenaltyId,
                        expected: V::Unsigned(u64::MAX),
                        after: V::Unsigned(7),
                        max_text_bytes: 1024
                    },
                    &mut read,
                    &mut write,
                    &guard,
                    &live
                )
                .unwrap()
            );
            let prefix = published.get();
            assert!(prefix > 0);
            enabled.set(true);
            let guard = read.transaction.guard();
            let result = change_runtime_control_overlay_v1(
                RuntimeControlChangeV1 {
                    field: F::TickConsensusAuthoritySource,
                    expected: V::Text(if mode == 0 { "wrong!" } else { "source" }),
                    after: V::Text("new source"),
                    max_text_bytes: 1024,
                },
                &mut read,
                &mut write,
                &guard,
                &live,
            );
            match mode {
                0 => {
                    assert!(matches!(
                        result,
                        Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
                    ));
                    assert_eq!(published.get(), prefix);
                }
                1 => {
                    assert!(
                        matches!(
                            result,
                            Err(WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                                ValueTreeError::Io(_)
                            )))
                        ) || matches!(
                            result,
                            Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Io(_)))
                        )
                    );
                    assert!(published.get() > prefix);
                }
                _ => {
                    assert!(
                        matches!(
                            result,
                            Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Cancelled))
                        ) || matches!(
                            result,
                            Err(WorldFieldOverlayErrorV1::Member(MemberRecordError::Value(
                                ValueTreeError::Cancelled
                            )))
                        )
                    );
                    assert!(published.get() > prefix);
                }
            }
        }
        let guard = tx.guard();
        assert!(matches!(
            tx.seal(&guard, &live, 1),
            Err(ExecutionError::Aborted)
        ));
    }
}
