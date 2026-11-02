use super::*;
use crate::incremental_event_fields::MailboxFrontierV1 as Frontier;
#[test]
fn provisional_mailbox_fifo_references_keep_absolute_frontiers_and_same_tx_replacements() {
    let memory = MemoryBudget::new(96 << 20, 8 << 20).unwrap();
    let _metadata = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
    let (pages, roots) = fixture::seed(&memory);
    let proof = fixture::qualify(&roots, &pages, &memory).unwrap();
    let extra = RefCell::new(Pages::default());
    let values = Values {
        base: &pages,
        extra: &extra,
    };
    let state = fixture::State::new(&pages, &memory);
    let parent = parent(&roots);
    let live = parent.fence().unwrap();
    let mut tx =
        ExecutionTransaction::begin(parent.clone(), [18; 32], &state, &memory, limits()).unwrap();
    let mut sink = Sink(&extra);
    let mut read_io = io();
    let mut write_io = io();
    let mut keys = crate::incremental_semantic_key::SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let query = AgentSubtreeMailboxQueryV1 {
        agent: "agent α",
        qualification: &proof,
    };
    use AgentSubtreeMailboxMutationV1 as O;
    use AgentSubtreeMailboxResultV1 as V;
    macro_rules! get {
        () => {{
            let guard = tx.guard();
            read_agent_subtree_mailbox_head_overlay_v1(
                query,
                &mut overlay!(
                    &mut tx,
                    &state,
                    &values,
                    &memory,
                    &mut read_io,
                    &mut keys,
                    || false
                ),
                &guard,
                &live,
            )
            .unwrap()
            .unwrap()
        }};
    }
    macro_rules! change {
        ($op:expr) => {{
            let operation = $op;
            let guard = tx.guard();
            change_agent_subtree_mailbox_reference_v1(
                query,
                operation,
                &mut overlay!(
                    &mut tx,
                    &state,
                    &values,
                    &memory,
                    &mut read_io,
                    &mut keys,
                    || false
                ),
                &mut ValueMutationContextV1 {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut write_io,
                    cancelled: || false,
                },
                &guard,
                &live,
            )
            .unwrap()
        }};
    }
    assert_eq!(
        get!(),
        AgentSubtreeMailboxHeadV1 {
            frontier: Frontier { head: 0, tail: 0 },
            original: None
        }
    );
    // Opaque IDs are intentionally unproven; only structure receipts are tested.
    assert_eq!(
        change!(O::Append {
            expected: get!().frontier,
            original: [7; 32]
        }),
        V::Appended(0)
    );
    assert_eq!(
        change!(O::Append {
            expected: get!().frontier,
            original: [7; 32]
        }),
        V::Appended(1)
    );
    assert_eq!(
        change!(O::Pop { expected: get!() }),
        V::Popped(Some([7; 32]))
    );
    assert_eq!(
        change!(O::Append {
            expected: get!().frontier,
            original: [8; 32]
        }),
        V::Appended(2)
    );
    assert_eq!(
        change!(O::Replace {
            expected: get!().frontier,
            ordinal: 1,
            original: [7; 32],
            after: [9; 32]
        }),
        V::Replaced(true)
    );
    assert_eq!(get!().original, Some([9; 32]));
    assert_eq!(
        change!(O::Pop { expected: get!() }),
        V::Popped(Some([9; 32]))
    );
    assert_eq!(
        change!(O::Pop { expected: get!() }),
        V::Popped(Some([8; 32]))
    );
    assert_eq!(change!(O::Pop { expected: get!() }), V::Popped(None));
    assert_eq!(get!().frontier, Frontier { head: 3, tail: 3 });
    assert_eq!(
        change!(O::Append {
            expected: get!().frontier,
            original: [10; 32]
        }),
        V::Appended(3)
    );
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    let accepted = accept_agent_subtree_transition_v1(
        &proof,
        &TransitionVerifier {
            parent: &parent,
            operation: [18; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        },
        &sealed.record,
        &mut crate::incremental_field_read::WorldFieldReadContextV1 {
            root: &sealed.record.after.state[0],
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io(),
            },
            key_work: &mut crate::incremental_semantic_key::SemanticKeyBudget {
                bytes_remaining: 1 << 20,
            },
            semantic_bytes: 1024,
            record_bytes: 1 << 20,
            take_state_error: || state.take(),
            cancelled: || false,
        },
        &mut AgentSubtreeTransitionWorkV1 {
            cells_remaining: 100,
            max_scalar_bytes: 1 << 20,
        },
    )
    .unwrap();
    accepted.validate_binding(&sealed.record.after).unwrap();
    assert_eq!(accepted.mailbox_reference_count(), 1);
    let verified = TransitionVerifier {
        parent: &parent,
        operation: [18; 32],
        source: &state,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    }
    .verify(&sealed.record)
    .unwrap();
    let mut after = Pages(pages.0.clone());
    for (id, page) in extra.borrow().0.iter() {
        after.0.insert(*id, page.clone());
    }
    for node in &verified.new_nodes {
        let retained = memory
            .reserve(node.bytes.capacity(), MemoryClass::Ordinary)
            .unwrap();
        let id = *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes();
        after
            .put_node(id, retained.retain(node.bytes.clone()))
            .unwrap();
    }
    source_tests::assert_only_live_external_dependencies(&sealed.record.after, &after, &memory);
    // Burned/consumed transient ordinals need no final leaves; live ordinal3 does.
    assert!(
        sealed
            .record
            .writes
            .iter()
            .filter_map(
                |write| parse_agent_subtree_key_v1(write.namespace, &write.mutation.key).unwrap()
            )
            .all(|key| !matches!(
                key,
                AgentSubtreeKeyV1::Mailbox {
                    ordinal: Some(0..=2),
                    ..
                }
            ))
    );
}

#[test]
fn mailbox_public_head_read_n_then_empty_pop_n_plus_one_cancels_and_poisons_prefix() {
    let mut baseline = 0;
    for mode in 0..3 {
        let memory = MemoryBudget::new(64 << 20, 8 << 20).unwrap();
        let _metadata = memory.reserve(5 << 20, MemoryClass::Ordinary).unwrap();
        let (pages, roots) = fixture::seed(&memory);
        let proof = fixture::qualify(&roots, &pages, &memory).unwrap();
        let extra = RefCell::new(Pages::default());
        let values = Values {
            base: &pages,
            extra: &extra,
        };
        let state = fixture::State::new(&pages, &memory);
        let parent = parent(&roots);
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent, [19; 32], &state, &memory, limits()).unwrap();
        let mut sink = Sink(&extra);
        let mut read_io = io();
        let mut write_io = io();
        let mut keys = crate::incremental_semantic_key::SemanticKeyBudget {
            bytes_remaining: 1 << 20,
        };
        let guard = tx.guard();
        assert!(
            change_agent_subtree_scalar_v1(
                AgentSubtreeCellChangeV1 {
                    query: AgentSubtreeQualifiedRequestV1 {
                        cell: request("agent β", AgentFieldV1::IntentEventSeq, 0),
                        qualification: &proof
                    },
                    expected: Some(AgentScalarRefV1::U64(u64::MAX)),
                    after: Some(AgentScalarRefV1::U64(7)),
                    max_scalar_bytes: 64,
                },
                &mut overlay!(
                    &mut tx,
                    &state,
                    &values,
                    &memory,
                    &mut read_io,
                    &mut keys,
                    || false
                ),
                &mut ValueMutationContextV1 {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut write_io,
                    cancelled: || false
                },
                &guard,
                &live
            )
            .unwrap()
        );
        let query = AgentSubtreeMailboxQueryV1 {
            agent: "agent α",
            qualification: &proof,
        };
        let polls = Cell::new(0usize);
        let guard = tx.guard();
        if mode == 0 {
            let value = read_agent_subtree_mailbox_head_overlay_v1(
                query,
                &mut overlay!(
                    &mut tx,
                    &state,
                    &values,
                    &memory,
                    &mut read_io,
                    &mut keys,
                    || {
                        polls.set(polls.get() + 1);
                        false
                    }
                ),
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
            assert_eq!(value.original, None);
            baseline = polls.get();
        } else {
            let result = change_agent_subtree_mailbox_reference_v1(
                query,
                AgentSubtreeMailboxMutationV1::Pop {
                    expected: AgentSubtreeMailboxHeadV1 {
                        frontier: Frontier { head: 0, tail: 0 },
                        original: None,
                    },
                },
                &mut overlay!(
                    &mut tx,
                    &state,
                    &values,
                    &memory,
                    &mut read_io,
                    &mut keys,
                    || {
                        if mode == 1 {
                            polls.set(polls.get() + 1);
                            polls.get() == baseline + 1
                        } else {
                            false
                        }
                    }
                ),
                &mut ValueMutationContextV1 {
                    sink: &mut sink,
                    memory: &memory,
                    io: &mut write_io,
                    cancelled: || {
                        if mode == 2 {
                            polls.set(polls.get() + 1);
                            polls.get() == baseline + 1
                        } else {
                            false
                        }
                    },
                },
                &guard,
                &live,
            );
            assert!(matches!(
                result,
                Err(crate::incremental_agent_mutation::BodyMutationError::Value(
                    ValueTreeError::Cancelled
                ))
            ));
            let guard = tx.guard();
            assert!(matches!(
                tx.seal(&guard, &live, 1),
                Err(ExecutionError::Aborted)
            ));
            eprintln!(
                "agent mailbox public getterN={baseline} emptyPopN+1={} channel={mode} prefixAborted",
                polls.get()
            );
        }
    }
}

#[test]
fn generic_valid_mailbox_holes_missing_members_outside_ordinals_and_wrong_profiles_are_rejected() {
    use crate::{
        incremental_agent_mutation::{BodyMutationContext, write_field_bytes},
        incremental_event_fields::{MailboxKeyV1, mailbox_event_ref_v1},
    };
    use oasis7_formal_protocol::incremental_state::StateNamespaceV1 as Ns;
    for mode in 0..4 {
        let memory = MemoryBudget::new(96 << 20, 8 << 20).unwrap();
        let _metadata = memory.reserve(5 << 20, MemoryClass::Ordinary).unwrap();
        let (pages, roots) = fixture::seed(&memory);
        let proof = fixture::qualify(&roots, &pages, &memory).unwrap();
        let extra = RefCell::new(Pages::default());
        let values = Values {
            base: &pages,
            extra: &extra,
        };
        let state = fixture::State::new(&pages, &memory);
        let parent = parent(&roots);
        let live = parent.fence().unwrap();
        let mut tx =
            ExecutionTransaction::begin(parent.clone(), [20; 32], &state, &memory, limits())
                .unwrap();
        let mut sink = Sink(&extra);
        let mut write_io = io();
        let header = write_field_bytes(
            &Frontier {
                head: 0,
                tail: if mode == 0 { 2 } else { 1 },
            }
            .canonical_bytes()
            .unwrap(),
            &mut BodyMutationContext {
                sink: &mut sink,
                memory: &memory,
                io: &mut write_io,
                cancelled: || false,
            },
        )
        .unwrap();
        let key = MailboxKeyV1::new("agent α", None).unwrap();
        tx.write(Ns::Scheduler, key.as_bytes(), Some(header))
            .unwrap();
        if mode != 1 {
            let ordinal = if mode <= 2 { 1 } else { 0 };
            let value = if mode == 3 {
                write_agent_scalar_stream_v1(
                    AgentScalarRefV1::U64(1),
                    &mut BodyMutationContext {
                        sink: &mut sink,
                        memory: &memory,
                        io: &mut write_io,
                        cancelled: || false,
                    },
                )
                .unwrap()
                .digest()
                .unwrap()
            } else {
                write_field_bytes(
                    &mailbox_event_ref_v1([7; 32]),
                    &mut BodyMutationContext {
                        sink: &mut sink,
                        memory: &memory,
                        io: &mut write_io,
                        cancelled: || false,
                    },
                )
                .unwrap()
            };
            let key = MailboxKeyV1::new("agent α", Some(ordinal)).unwrap();
            tx.write(Ns::Scheduler, key.as_bytes(), Some(value))
                .unwrap();
        }
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [20; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        };
        verifier.verify(&sealed.record).unwrap();
        let result = accept_agent_subtree_transition_v1(
            &proof,
            &verifier,
            &sealed.record,
            &mut crate::incremental_field_read::WorldFieldReadContextV1 {
                root: &sealed.record.after.state[0],
                state: &state,
                values: ValueReadContext {
                    source: &values,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut io(),
                },
                key_work: &mut crate::incremental_semantic_key::SemanticKeyBudget {
                    bytes_remaining: 1 << 20,
                },
                semantic_bytes: 1024,
                record_bytes: 1 << 20,
                take_state_error: || state.take(),
                cancelled: || false,
            },
            &mut AgentSubtreeTransitionWorkV1 {
                cells_remaining: 100,
                max_scalar_bytes: 1 << 20,
            },
        );
        assert!(
            matches!(
                result,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
            ),
            "mode={mode}"
        );
    }
}
