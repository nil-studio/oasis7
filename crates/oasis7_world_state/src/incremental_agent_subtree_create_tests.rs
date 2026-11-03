use super::*;
fn cell() -> crate::AgentCell {
    let mut cell = crate::AgentCell::new(
        crate::AgentState::new(
            "new agent",
            oasis7_agent_api::world_action::GeoPos::new(7, 8, 9),
        ),
        u64::MAX,
    );
    cell.state.body_state.slots.truncate(1);
    cell
}
#[test]
fn shared_original_producer_creates_one_borrowed_cell_then_reads_and_edits_same_transaction() {
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
        [13; 32],
        &state,
        &memory,
        TransactionLimits {
            max_reads: 128,
            max_writes: 64,
            ..limits()
        },
    )
    .unwrap();
    let mut read_io = io();
    let mut write_io = io();
    let mut keys = crate::incremental_semantic_key::SemanticKeyBudget {
        bytes_remaining: 1 << 20,
    };
    let mut sink = Sink(&extra);
    let cell = cell();
    let guard = tx.guard();
    let emitted = create_agent_subtree_v1(
        AgentSubtreeCreateV1 {
            agent: "new agent",
            cell: &cell,
            qualification: &proof,
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
    assert!(emitted > 20);
    let guard = tx.guard();
    assert!(
        change_agent_subtree_scalar_v1(
            AgentSubtreeCellChangeV1 {
                query: AgentSubtreeQualifiedRequestV1 {
                    cell: request("new agent", AgentFieldV1::PositionX, 0),
                    qualification: &proof
                },
                expected: Some(AgentScalarRefV1::I64(7)),
                after: Some(AgentScalarRefV1::I64(i64::MIN)),
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
    let guard = tx.guard();
    let sealed = tx.seal(&guard, &live, 1).unwrap();
    let verified = TransitionVerifier {
        parent: &parent,
        operation: [13; 32],
        source: &state,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    }
    .verify(&sealed.record)
    .unwrap();
    let accepted = accept_agent_subtree_transition_v1(
        &proof,
        &TransitionVerifier {
            parent: &parent,
            operation: [13; 32],
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
    assert_eq!(accepted.agent_count(), 3);
    // The independent full bootstrap oracle must agree with changed-only receipt.
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
    let raw = fixture::qualify_max_agents(&sealed.record.after, &after, &memory, 3).unwrap();
    assert_eq!(raw.agent_count(), 3);
    assert_eq!(raw.mailbox_reference_count(), 0);
}
#[test]
fn creation_rejects_current_orphan_resources_and_aborts_successful_prefix() {
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
    let mut tx = ExecutionTransaction::begin(parent, [14; 32], &state, &memory, limits()).unwrap();
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
    let resource = write_agent_scalar_stream_v1(
        AgentScalarRefV1::I64(99),
        &mut ValueMutationContextV1 {
            sink: &mut sink,
            memory: &memory,
            io: &mut write_io,
            cancelled: || false,
        },
    )
    .unwrap();
    let key = AgentFieldKeyV1::new("new agent", AgentFieldV1::ResourceAmount, 1).unwrap();
    tx.write(
        AgentFieldV1::ResourceAmount.namespace(),
        key.as_bytes(),
        Some(resource.digest().unwrap()),
    )
    .unwrap();
    let cell = cell();
    let guard = tx.guard();
    assert!(matches!(
        create_agent_subtree_v1(
            AgentSubtreeCreateV1 {
                agent: "new agent",
                cell: &cell,
                qualification: &proof,
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
            &mut |_, _| panic!("empty original mailbox"),
            &guard,
            &live
        ),
        Err(crate::incremental_agent_mutation::BodyMutationError::Invalid(_))
    ));
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}
