use super::*;
use crate::incremental_field_overlay::WorldFieldOverlayContextV1;
use oasis7_formal_protocol::incremental_execution::*;
use std::cell::Cell;

struct FailPut;
impl ValueNodeWriter for FailPut {
    fn put_node(&mut self, _: [u8; 32], _: ReservedValue<Vec<u8>>) -> Result<(), ValueTreeError> {
        Err(ValueTreeError::Invalid("injected actual put"))
    }
}

// Count an uncancelled execution first, then cancel at its final poll. This
// exercises the post-comparison poll rather than guessing internal call counts.
#[test]
fn getter_and_noop_final_poll_and_actual_put_poison_successful_prefix() {
    let memory = MemoryBudget::new(64 << 20, 4 << 20).unwrap();
    let (pages, roots) = seed(&memory);
    for (getter, write_channel) in [(false, false), (false, true), (true, false)] {
        let mut final_poll = 0;
        for cancel_final in [false, true] {
            let parent = ExecutionParent {
                roots: roots.clone(),
                commit: [71; 32],
                authority: AuthorityBinding {
                    token: [72; 32],
                    epoch: 3,
                },
            };
            let live = parent.fence().unwrap();
            let mut tx = ExecutionTransaction::begin(
                parent,
                [73; 32],
                &pages,
                &memory,
                TransactionLimits {
                    max_reads: 16,
                    max_writes: 8,
                    max_facts: 4,
                    max_pending_node_bytes: 8 << 20,
                },
            )
            .unwrap();
            tx.write(
                StateNamespaceV1::Business,
                b"successful-prefix",
                Some([74; 32]),
            )
            .unwrap();
            let polls = Cell::new(0);
            let mut reads = io();
            let mut keys = SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            };
            let mut read = WorldFieldOverlayContextV1 {
                transaction: &mut tx,
                values: ValueReadContext {
                    source: &pages,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut reads,
                },
                key_work: &mut keys,
                semantic_bytes: 4096,
                record_bytes: 8192,
                take_state_error: || None,
                cancelled: || {
                    if write_channel {
                        return false;
                    }
                    polls.set(polls.get() + 1);
                    cancel_final && polls.get() == final_poll
                },
            };
            let result = if getter {
                read_runtime_artifact_content_overlay_v1(
                    RuntimeArtifactContentFieldV1::Registered,
                    "",
                    &mut read,
                )
                .map(|v| {
                    assert_eq!(v, RuntimeArtifactContentValueV1::Registered(true));
                    false
                })
            } else {
                let mut sink = FailPut;
                let mut writes = io();
                let mut keys = SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                };
                let mut write = MemberRecordWriteContext {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut writes,
                    key_work: &mut keys,
                    limits: MemberRecordLimitsV1 {
                        semantic_bytes: 4096,
                        payload_bytes: 4096,
                        record_bytes: 8192,
                    },
                    cancelled: || {
                        if !write_channel {
                            return false;
                        }
                        polls.set(polls.get() + 1);
                        cancel_final && polls.get() == final_poll
                    },
                };
                let guard = read.transaction.guard();
                change_runtime_artifact_content_overlay_v1(
                    RuntimeArtifactContentChangeV1::Registered {
                        key: "",
                        expected: true,
                        after: true,
                    },
                    &mut read,
                    &mut write,
                    &mut RuntimeArtifactOriginalWorkV1 {
                        objects_remaining: 0,
                        bytes_remaining: 0,
                        max_object_bytes: 0,
                    },
                    &mut |_, _| panic!("NoOp persisted an original"),
                    &guard,
                    &live,
                )
            };
            if cancel_final {
                assert!(result.is_err());
                let guard = tx.guard();
                assert!(matches!(
                    tx.seal(&guard, &live, 1),
                    Err(ExecutionError::Aborted)
                ));
            } else {
                assert!(!result.unwrap());
                final_poll = polls.get();
                assert!(final_poll > 1);
                let guard = tx.guard();
                assert_eq!(tx.seal(&guard, &live, 1).unwrap().record.writes.len(), 1);
            }
        }
    }
    let parent = ExecutionParent {
        roots,
        commit: [71; 32],
        authority: AuthorityBinding {
            token: [72; 32],
            epoch: 3,
        },
    };
    let live = parent.fence().unwrap();
    let mut tx = ExecutionTransaction::begin(
        parent,
        [73; 32],
        &pages,
        &memory,
        TransactionLimits {
            max_reads: 16,
            max_writes: 8,
            max_facts: 4,
            max_pending_node_bytes: 8 << 20,
        },
    )
    .unwrap();
    tx.write(
        StateNamespaceV1::Business,
        b"successful-prefix",
        Some([74; 32]),
    )
    .unwrap();
    let mut reads = io();
    let mut writes = io();
    let mut read_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut write_keys = SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut sink = FailPut;
    let guard = tx.guard();
    let result = change_runtime_artifact_content_overlay_v1(
        RuntimeArtifactContentChangeV1::Registered {
            key: "new",
            expected: false,
            after: true,
        },
        &mut WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &pages,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut reads,
            },
            key_work: &mut read_keys,
            semantic_bytes: 4096,
            record_bytes: 8192,
            take_state_error: || None,
            cancelled: || false,
        },
        &mut MemberRecordWriteContext {
            sink: &mut sink,
            memory: &memory,
            io: &mut writes,
            key_work: &mut write_keys,
            limits: MemberRecordLimitsV1 {
                semantic_bytes: 4096,
                payload_bytes: 4096,
                record_bytes: 8192,
            },
            cancelled: || false,
        },
        &mut RuntimeArtifactOriginalWorkV1 {
            objects_remaining: 0,
            bytes_remaining: 0,
            max_object_bytes: 0,
        },
        &mut |_, _| panic!("registration persisted an original"),
        &guard,
        &live,
    );
    assert!(format!("{result:?}").contains("injected actual put"));
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}
