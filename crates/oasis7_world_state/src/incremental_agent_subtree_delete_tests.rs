use super::*;
use oasis7_formal_protocol::incremental_state::{StateScanBudget, StateScanContext};
#[test]
fn deleted_actor_requires_every_required_and_optional_resource_leaf() {
    for leave in [
        None,
        Some(AgentFieldV1::ResourceAmount),
        Some(AgentFieldV1::PositionX),
    ] {
        let memory = MemoryBudget::new(192 << 20, 8 << 20).unwrap();
        let _metadata = memory.reserve(8 << 20, MemoryClass::Ordinary).unwrap();
        let (pages, roots) = fixture::seed(&memory);
        let proof = fixture::qualify(&roots, &pages, &memory).unwrap();
        let state = fixture::State::new(&pages, &memory);
        let key = AgentFieldKeyV1::new("agent α", AgentFieldV1::Identity, 0).unwrap();
        let target = AgentFieldKeyV1::from_bytes(key.as_bytes()).unwrap().0;
        let mut rows = Vec::with_capacity(128);
        let mut scan = StateScanBudget {
            nodes_remaining: 1_000_000,
            read_bytes_remaining: 64 << 20,
            leaves_remaining: 1000,
        };
        // Test-only independent enumeration of authenticated original keys.
        for root in &roots.state {
            StateScanContext {
                source: &state,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut scan,
            }
            .visit_entries(
                root,
                |entry| {
                    let parsed = parse_agent_subtree_key_v1(entry.namespace, &entry.key)?;
                    let selected = match parsed {
                        Some(AgentSubtreeKeyV1::Field { owner, field, .. }) => {
                            owner == target && Some(field) != leave
                        }
                        Some(
                            AgentSubtreeKeyV1::Body { agent }
                            | AgentSubtreeKeyV1::Mailbox { agent, .. },
                        ) => agent == "agent α",
                        None => false,
                    };
                    if selected {
                        assert!(rows.len() < 128);
                        rows.push((entry.namespace, entry.key.clone()));
                    }
                    Ok::<(), ValueTreeError>(())
                },
                || false,
            )
            .unwrap();
        }
        let parent = parent(&roots);
        let live = parent.fence().unwrap();
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [15; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 128,
                max_writes: 64,
                ..limits()
            },
        )
        .unwrap();
        for (namespace, key) in rows {
            tx.write(namespace, &key, None).unwrap();
        }
        let guard = tx.guard();
        let sealed = tx.seal(&guard, &live, 1).unwrap();
        let verifier = TransitionVerifier {
            parent: &parent,
            operation: [15; 32],
            source: &state,
            memory: &memory,
            class: MemoryClass::Ordinary,
            max_pending_node_bytes: 8 << 20,
        };
        verifier.verify(&sealed.record).unwrap();
        let accepted = accept_agent_subtree_transition_v1(
            &proof,
            &verifier,
            &sealed.record,
            &mut crate::incremental_field_read::WorldFieldReadContextV1 {
                root: &sealed.record.after.state[0],
                state: &state,
                values: ValueReadContext {
                    source: &pages,
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
                cells_remaining: 5000,
                max_scalar_bytes: 1 << 20,
            },
        );
        if leave.is_none() {
            let accepted = accepted.unwrap();
            accepted.validate_binding(&sealed.record.after).unwrap();
            assert_eq!(accepted.agent_count(), 1);
        } else {
            assert!(matches!(
                accepted,
                Err(WorldFieldOverlayErrorV1::Value(ValueTreeError::Invalid(_)))
            ));
        }
    }
}

#[test]
fn normal_whole_cell_delete_preserves_real_sparse_controls_and_current_expected_values() {
    let memory = MemoryBudget::new(192 << 20, 8 << 20).unwrap();
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
    let mut tx = ExecutionTransaction::begin(
        parent.clone(),
        [16; 32],
        &state,
        &memory,
        TransactionLimits {
            max_reads: 128,
            max_writes: 64,
            ..limits()
        },
    )
    .unwrap();
    let mut sink = Sink(&extra);
    let mut read_io = io();
    let mut write_io = io();
    let mut keys = crate::incremental_semantic_key::SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut expected = fixture::source_cell(0, "agent α");
    let guard = tx.guard();
    assert!(
        change_agent_subtree_scalar_v1(
            AgentSubtreeCellChangeV1 {
                query: AgentSubtreeQualifiedRequestV1 {
                    cell: request("agent α", AgentFieldV1::PositionX, 0),
                    qualification: &proof
                },
                expected: Some(AgentScalarRefV1::I64(i64::MIN)),
                after: Some(AgentScalarRefV1::I64(7)),
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
    expected.state.pos.x_cm = 7;
    let slot = crate::agent_state::BodyModuleSlot {
        slot_id: " repeated original ".into(),
        slot_type: crate::agent_state::BodySlotType::Universal,
        installed_module: Some("".into()),
        locked: true,
    };
    for (ordinal, count, next, before, after) in [
        (2, 2, 2, None, Some(&slot)),
        (2, 3, 3, Some(&slot), None),
        (3, 2, 3, None, Some(&slot)),
    ] {
        let guard = tx.guard();
        assert!(
            change_agent_subtree_row_v1(
                AgentSubtreeRowChangeV1 {
                    agent: "agent α",
                    qualification: &proof,
                    ordinal,
                    expected_count: count,
                    expected_next: next,
                    expected: before.map(AgentSubtreeRowRefV1::Slot),
                    after: after.map(AgentSubtreeRowRefV1::Slot),
                    max_scalar_bytes: 1 << 20,
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
    }
    expected.state.body_state.slots.push(slot);
    let guard = tx.guard();
    let deleted = delete_agent_subtree_v1(
        AgentSubtreeDeleteV1 {
            agent: "agent α",
            expected: &expected,
            qualification: &proof,
            slot_ordinals: &[0, 1, 3],
            slot_next: 4,
            cargo_ordinals: &[0],
            cargo_next: 1,
            mailbox: crate::incremental_event_fields::MailboxFrontierV1 { head: 0, tail: 0 },
            max_scalar_bytes: 1 << 20,
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
            cancelled: || false,
        },
        &mut |_, _| panic!("empty original mailbox"),
        &guard,
        &live,
    )
    .unwrap();
    assert!(deleted > 20);
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    let accepted = accept_agent_subtree_transition_v1(
        &proof,
        &TransitionVerifier {
            parent: &parent,
            operation: [16; 32],
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
            cells_remaining: 5000,
            max_scalar_bytes: 1 << 20,
        },
    )
    .unwrap();
    accepted.validate_binding(&sealed.record.after).unwrap();
    assert_eq!(accepted.agent_count(), 1);
}

#[test]
fn whole_cell_delete_rejects_incomplete_resource_or_vec_expectations_and_aborts_prefix() {
    for omit_resource in [false, true] {
        let memory = MemoryBudget::new(128 << 20, 8 << 20).unwrap();
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
        let mut tx = ExecutionTransaction::begin(
            parent,
            [17; 32],
            &state,
            &memory,
            TransactionLimits {
                max_reads: 128,
                max_writes: 64,
                ..limits()
            },
        )
        .unwrap();
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
        let mut expected = fixture::source_cell(0, "agent α");
        if omit_resource {
            expected.state.resources.amounts.clear();
        } else {
            expected.state.body_state.slots.truncate(1);
        }
        let guard = tx.guard();
        let result = delete_agent_subtree_v1(
            AgentSubtreeDeleteV1 {
                agent: "agent α",
                expected: &expected,
                qualification: &proof,
                slot_ordinals: if omit_resource { &[0, 1] } else { &[0] },
                slot_next: 2,
                cargo_ordinals: &[0],
                cargo_next: 1,
                mailbox: crate::incremental_event_fields::MailboxFrontierV1 { head: 0, tail: 0 },
                max_scalar_bytes: 1 << 20,
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
                cancelled: || false,
            },
            &mut |_, _| panic!("empty original mailbox"),
            &guard,
            &live,
        );
        assert!(matches!(
            result,
            Err(crate::incremental_agent_mutation::BodyMutationError::Invalid(_))
        ));
        let guard = tx.guard();
        assert!(matches!(
            tx.seal(&guard, &live, 1),
            Err(ExecutionError::Aborted)
        ));
    }
}
