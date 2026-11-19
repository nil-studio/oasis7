use super::*;
use oasis7_agent_api::world_action::GeoPos;
#[test]
fn selected_pending_action_identity_rejects_other_transaction_after_real_pop() {
    let memory = MemoryBudget::new(512 << 20, 8 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    world.submit_action(crate::Action::RegisterAgent {
        agent_id: "queued-alice".into(),
        pos: GeoPos {
            x_cm: 1,
            y_cm: 2,
            z_cm: 3,
        },
    });
    world.submit_action(crate::Action::MoveAgent {
        agent_id: "queued-alice".into(),
        to: GeoPos {
            x_cm: 11,
            y_cm: 22,
            z_cm: 33,
        },
    });
    // The fixed workspace above prepays these two bounded canonical input oracles.
    let expected_inputs: Vec<_> = world
        .pending_actions
        .iter()
        .map(|envelope| {
            let bytes = oasis7_wasm_abi::encode_canonical_cbor(envelope).unwrap();
            assert!(bytes.len() <= 1 << 20);
            *blake3::hash(&bytes).as_bytes()
        })
        .collect();
    let material_limits = crate::world::incremental_body_event::materials::test_support::MaterialFixtureLimitsV1::for_state(world.state());
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut material_backing =
        crate::world::incremental_body_event::materials::test_support::MaterialFixtureV1::build(
            &fixture.roots,
            &reader,
            &reader,
            &memory,
            material_limits,
            || reader.cas.error(),
            || false,
        );
    let mut materials = material_backing.context(&fixture.roots, &reader, || reader.cas.error());
    let mut originals = Originals {
        objects: fixture.objects.clone(),
        memory: &memory,
    };
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [92; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 2048,
            max_writes: 1024,
            max_facts: 32,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let live = tx.parent_fence();
    let counter =
        WorldEventCounterV1::begin(&fixture.runtime, &fixture.head, &tx, &memory).unwrap();
    let mut pending_work = genesis::current_work(&fixture.roots).runtime.pending;
    let guard = tx.guard();
    let mut pending =
        oasis7_world_state::incremental_pending_action_overlay::begin_pending_action_current_v1(
            fixture.current.runtime().pending(),
            &fixture.roots,
            &mut tx,
            &memory,
            &guard,
            &live,
            &mut pending_work.index,
            || false,
        )
        .unwrap();
    let mut read_io = genesis::io();
    let mut keys = genesis::keys();
    let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Business),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut read_io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let selected = crate::world::incremental_action_execution::take_pending_action(
        &mut pending,
        &mut tx,
        &mut read,
        &mut pending_work,
        &mut originals,
        &live,
    )
    .unwrap()
    .unwrap();
    selected.validate(&tx).unwrap();
    tx.write(
        StateNamespaceV1::Scheduler,
        b"identity-revision",
        Some([8; 32]),
    )
    .unwrap();
    selected.validate(&tx).unwrap();
    let mut other = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [93; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 2048,
            max_writes: 1024,
            max_facts: 32,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    assert!(selected.validate(&other).is_err());
    let other_counter =
        WorldEventCounterV1::begin(&fixture.runtime, &fixture.head, &other, &memory).unwrap();
    let other_live = other.parent_fence();
    let guard = other.guard();
    let mut cas = fixture.cas.clone();
    let mut io = genesis::io();
    let mut canonical = genesis::canonical();
    let mut mutation = BodyMutationContext {
        sink: &mut cas,
        memory: &memory,
        io: &mut io,
        cancelled: || false,
    };
    let mut called = false;
    {
        let mut step = crate::world::incremental_step::IwsStepContext::begin_counted(
            &mut other,
            0,
            &guard,
            &other_live,
            &reader,
            &mut mutation,
            &mut canonical,
            &other_counter,
        )
        .unwrap();
        assert!(
            step.append_selected_rejection(
                &mut materials,
                &other_live,
                &selected,
                crate::RejectReason::AgentNotFound {
                    agent_id: "queued-alice".into()
                },
                64 << 10,
                |_: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                    called = true;
                    Ok(())
                }
            )
            .is_err()
        );
    }
    assert!(
        !called,
        "identity must fail before the original sink is invoked"
    );
    let guard = other.guard();
    assert!(other.seal(&guard, &other_live, 1).is_err());
    let mut sink_cas = fixture.cas.clone();
    let mut sink_io = genesis::io();
    let mut original_work = genesis::canonical();
    let mut sink = BodyMutationContext {
        sink: &mut sink_cas,
        memory: &memory,
        io: &mut sink_io,
        cancelled: || false,
    };
    let guard = tx.guard();
    let mut saved = Vec::new();
    let mut audit_work = audit::audit_work();
    let initial_values = audit_work.structure.values_remaining;
    let audit = {
        let mut step = crate::world::incremental_step::IwsStepContext::begin_counted(
            &mut tx,
            0,
            &guard,
            &live,
            &reader,
            &mut sink,
            &mut original_work,
            &counter,
        )
        .unwrap();
        step.append_selected_rejection_owned(&mut materials, &live, &selected, crate::RejectReason::AgentNotFound { agent_id: "queued-alice".into() }, 64 << 10, &mut audit_work,
            |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                producer.persist_to(&mut |stream: &mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1, _: &mut dyn FnMut() -> bool| {
                    let len = usize::try_from(stream.original_bytes()).unwrap();
                    assert!(len <= 64 << 10);
                    let _fee = memory.reserve(len, MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
                    let mut bytes = Vec::with_capacity(len); stream.write_to(&mut bytes)?;
                    assert_eq!(*blake3::hash(&bytes).as_bytes(), stream.original_id());
                    assert_eq!(fixture.objects.put(&bytes), stream.original_id());
                    saved.push(bytes); Ok(())
                })
            }
        ).unwrap()
    };
    audit.validate(&tx).unwrap();
    let audit_token =
        crate::world::incremental_module_execution_event::PersistedWorldEventV1::from_action_audit(
            &audit,
        );
    audit_token.validate(&tx).unwrap();
    assert!(std::ptr::eq(audit_token.event(), audit.event()));
    let unrelated = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [99; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 2048,
            max_writes: 1024,
            max_facts: 32,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    assert!(
        audit_token.validate(&unrelated).is_err(),
        "same-parent foreign transaction cannot consume the audit event token"
    );

    assert!(audit_work.structure.values_remaining < initial_values);
    let expected = crate::WorldEvent {
        id: 1,
        time: 1,
        caused_by: Some(crate::CausedBy::Action(selected.envelope().id)),
        body: crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
            action_id: selected.envelope().id,
            reason: crate::RejectReason::AgentNotFound {
                agent_id: "queued-alice".into(),
            },
        }),
    };
    let bytes = oasis7_wasm_abi::encode_canonical_cbor(&expected).unwrap();
    assert_eq!(
        oasis7_wasm_abi::encode_canonical_cbor(audit.event()).unwrap(),
        bytes
    );
    assert_eq!(saved, vec![bytes]);
    assert_eq!(counter.journal_height(&tx).unwrap(), 1);
    drop(counter);
    drop(expected_inputs);
}

#[path = "incremental_action_audit_tests.rs"]
mod audit;
