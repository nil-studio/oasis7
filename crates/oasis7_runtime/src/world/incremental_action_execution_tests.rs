//! Real GEN pending originals consumed without retaining a World.
use super::*;
use crate::world::incremental_step::IwsStepContext;
use oasis7_agent_api::world_action::GeoPos;
#[test]
fn real_queued_register_then_move_consumes_originals_and_updates_current_position() {
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
    let expected_time = world.state.time;
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
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
    let mut write_io = genesis::io();
    let mut sink = fixture.cas.clone();
    let mut mutation = BodyMutationContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut write_io,
        cancelled: || false,
    };
    let mut canonical = genesis::canonical();
    let guard = tx.guard();
    let mut step = IwsStepContext::begin_counted(
        &mut tx,
        expected_time,
        &guard,
        &live,
        &reader,
        &mut mutation,
        &mut canonical,
        &counter,
    )
    .unwrap();
    let mut json = genesis::json();
    let mut scan = genesis::scan();
    let mut policy_work = genesis::current_work(&fixture.roots).runtime.revocation;
    let mut entries = 100000;
    let mut events = Vec::new();
    for _ in 0..2 {
        assert!(crate::world::incremental_action_execution_core::consume_bootstrap_action(
            &mut step,&mut pending,&mut read,&mut pending_work,&mut originals,
            fixture.current.raw().agents(),fixture.current.runtime().module_authorization(),
            fixture.current.runtime().config(),fixture.current.runtime().revocation(),
            &mut json,&mut scan,&mut policy_work,&mut entries,64<<10,&live,
            |producer:&mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                producer.persist_to(&mut |stream:&mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1,cancelled:&mut dyn FnMut()->bool| {
                    let length=usize::try_from(stream.original_bytes()).unwrap();assert!(length<=64<<10);
                    let charge=memory.reserve(length+128,MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
                    let mut bytes=Vec::with_capacity(length);stream.write_to(&mut bytes)?;
                    assert!(!cancelled());assert_eq!(fixture.objects.put(&bytes),stream.original_id());
                    events.push(serde_cbor::from_slice::<crate::WorldEvent>(&bytes).unwrap());
                    drop(charge);Ok(())
                })
            }).unwrap());
    }
    assert_eq!(step.journal_height().unwrap(), 2);
    assert!(
        matches!(&events[0].body,crate::WorldEventBody::Domain(crate::DomainEvent::AgentRegistered{agent_id,..}) if agent_id=="queued-alice")
    );
    assert!(
        matches!(&events[1].body,crate::WorldEventBody::Domain(crate::DomainEvent::AgentMoved{from,to,..}) if from.x_cm==1&&from.y_cm==2&&from.z_cm==3&&to.x_cm==11&&to.y_cm==22&&to.z_cm==33)
    );
    drop(step);
    // Independent current getter detects an event-only implementation that
    // emits AgentMoved but fails to mutate the actual position leaves.
    {
        use oasis7_world_state::incremental_agent_fields::AgentFieldV1;
        use oasis7_world_state::incremental_agent_subtree::*;
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &reader,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            take_state_error: || reader.cas.error(),
            cancelled: || false,
        };
        for (field, expected) in [
            (AgentFieldV1::PositionX, 11),
            (AgentFieldV1::PositionY, 22),
            (AgentFieldV1::PositionZ, 33),
        ] {
            let guard = overlay.transaction.guard();
            let actual = read_agent_subtree_scalar_overlay_v1(
                AgentSubtreeQualifiedRequestV1 {
                    cell: AgentSubtreeScalarRequestV1 {
                        agent: "queued-alice",
                        field,
                        ordinal: 0,
                    },
                    qualification: fixture.current.raw().agents(),
                },
                &mut overlay,
                16 << 20,
                &mut std::io::sink(),
                &guard,
                &live,
            )
            .unwrap();
            assert!(matches!(actual,Some(AgentSubtreeValueV1::I64(v)) if v==expected));
        }
    }
    let head = oasis7_world_state::incremental_pending_action_overlay::peek_pending_action_v1(
        &pending,
        &mut tx,
        &memory,
        &live,
        &mut pending_work.index,
        || false,
    )
    .unwrap();
    assert!(head.is_none());
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, fixture.roots.height + 1).unwrap();
    assert_eq!(
        prepared
            .record
            .facts
            .iter()
            .filter(|f| f.kind == ExecutionFactKindV1::Event)
            .count(),
        2
    );
    let actual_inputs: Vec<_> = prepared
        .record
        .facts
        .iter()
        .filter(|f| f.kind == ExecutionFactKindV1::Input)
        .map(|f| f.id)
        .collect();
    assert_eq!(actual_inputs, expected_inputs);
    assert_eq!(
        prepared.record.facts.len(),
        4,
        "two authenticated inputs and two business events"
    );
}
