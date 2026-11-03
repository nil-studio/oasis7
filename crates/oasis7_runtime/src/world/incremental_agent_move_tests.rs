use super::*;
use crate::world::incremental_agent_genesis::write_agent_scalar_value;
use oasis7_agent_api::world_action::GeoPos;
use oasis7_world_state::incremental_agent_fields::{
    AgentFieldKeyV1, AgentFieldV1, AgentScalarRefV1,
};

pub(super) fn seed_positions(
    world: &World,
    parent: &mut ExecutionParent,
    state: &mut Pages,
    values: &mut Values,
    memory: &MemoryBudget,
) {
    let pos = world.state.agents["alice"].state.pos;
    let mut writes = Vec::new();
    for (field, value) in [
        (AgentFieldV1::PositionX, pos.x_cm),
        (AgentFieldV1::PositionY, pos.y_cm),
        (AgentFieldV1::PositionZ, pos.z_cm),
    ] {
        let id = write_agent_scalar_value(
            field,
            AgentScalarRefV1::I64(value),
            &mut BodyMutationContext {
                sink: values,
                memory,
                io: &mut io(),
                cancelled: || false,
            },
            4096,
        )
        .unwrap();
        writes.push(StateWriteV1 {
            key: AgentFieldKeyV1::new("alice", field, 0)
                .unwrap()
                .as_bytes()
                .to_vec(),
            before: None,
            after: Some(id),
        });
    }
    let delta = StateDeltaV1::build(parent.roots.state[0].clone(), writes, state, 4 << 20).unwrap();
    parent.roots.state[0] = delta.after;
    for page in delta.new_nodes {
        state.0.insert(page.digest, page.bytes);
    }
}
pub(super) fn moved(id: u64, to: GeoPos) -> WorldEvent {
    WorldEvent {
        id,
        time: 11,
        caused_by: Some(crate::CausedBy::Action(7)),
        body: WorldEventBody::Domain(DomainEvent::AgentMoved {
            agent_id: "alice".into(),
            // Existing reducer semantics retain `from` in the original event,
            // but apply `to`; it is not a materialized-cache precondition.
            from: GeoPos::new(999, 999, 999),
            to,
        }),
    }
}
#[test]
fn successive_moves_use_overlay_positions_and_exact_originals_without_agent_clone() {
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let (mut world, mut parent, mut state) = fixture(&memory);
    let mut values = Values::default();
    seed(
        &world,
        &mut parent,
        &mut state,
        &mut values,
        &memory,
        EventSequenceV1 { next: 1, era: 0 },
        MailboxFrontierV1 { head: 0, tail: 0 },
    );
    seed_positions(&world, &mut parent, &mut state, &mut values, &memory);
    world.state.agents.get_mut("alice").unwrap().state.pos = GeoPos::new(-30, -40, -50);
    let unchanged = world.state.clone();
    let source = values.clone();
    let live = parent.fence().unwrap();
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [4; 32], &state, &memory, limits()).unwrap();
    let mut originals = Vec::new();
    for (id, to) in [
        (1, GeoPos::new(4, 5, 6)),
        (2, GeoPos::new(i64::MIN, 5, i64::MAX)),
    ] {
        let event = moved(id, to);
        world
            .contribute_iws_agent_event(
                &event,
                &mut txn,
                &mut AgentEventContext {
                    values: &source,
                    mutation: BodyMutationContext {
                        sink: &mut values,
                        memory: &memory,
                        io: &mut io(),
                        cancelled: || false,
                    },
                    persist_original: |bytes: &ReservedValue<Vec<u8>>, digest| {
                        assert_eq!(
                            bytes.as_slice(),
                            oasis7_wasm_abi::encode_canonical_cbor(&event).unwrap()
                        );
                        originals.push(digest);
                        Ok(())
                    },
                    max_event_bytes: 4096,
                },
            )
            .unwrap();
    }
    let guard = txn.guard();
    let prepared = txn.seal(&guard, &live, 0).unwrap();
    assert_eq!(prepared.record.writes.len(), 8);
    assert_eq!(
        prepared
            .record
            .facts
            .iter()
            .map(|f| f.id)
            .collect::<Vec<_>>(),
        originals
    );
    assert_eq!(world.state, unchanged);
    let mut quota = io();
    let mut reader = ValueReadContext {
        source: &source,
        memory: &memory,
        class: MemoryClass::Ordinary,
        io: &mut quota,
    };
    for (field, expected) in [
        (AgentFieldV1::PositionX, i64::MIN),
        (AgentFieldV1::PositionY, 5),
        (AgentFieldV1::PositionZ, i64::MAX),
    ] {
        let key = AgentFieldKeyV1::new("alice", field, 0).unwrap();
        let write = prepared
            .record
            .writes
            .iter()
            .find(|w| w.mutation.key == key.as_bytes())
            .unwrap();
        let root = reader.load_root(&write.mutation.after.unwrap()).unwrap();
        let mut bytes = Vec::new();
        reader
            .read_range(&root, 0, root.0.logical_len, &mut bytes, || false)
            .unwrap();
        assert_eq!(
            AgentScalarRefV1::from_canonical_bytes(&bytes, field).unwrap(),
            AgentScalarRefV1::I64(expected)
        );
    }
    TransitionVerifier {
        parent: &parent,
        operation: [4; 32],
        source: &state,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 4 << 20,
    }
    .verify(&prepared.record)
    .unwrap();
}
#[test]
fn move_failure_aborts_all_outer_writes_and_preserves_world() {
    for failure in 0..6 {
        let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
        let (world, mut parent, mut state) = fixture(&memory);
        let unchanged = world.state.clone();
        let mut values = Values::default();
        seed(
            &world,
            &mut parent,
            &mut state,
            &mut values,
            &memory,
            EventSequenceV1 { next: 1, era: 0 },
            MailboxFrontierV1 { head: 0, tail: 0 },
        );
        if failure != 0 {
            seed_positions(&world, &mut parent, &mut state, &mut values, &memory);
        }
        if failure == 5 {
            let key = AgentFieldKeyV1::new("alice", AgentFieldV1::PositionX, 0).unwrap();
            let mut bytes = Vec::new();
            // Empty text has exactly the same encoded length as i64, so this
            // failure must come from the type decoder rather than length.
            AgentScalarRefV1::Text("").write_to(&mut bytes).unwrap();
            let id = write_field_bytes(
                &bytes,
                &mut BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut io(),
                    cancelled: || false,
                },
            )
            .unwrap();
            let delta = StateDeltaV1::build(
                parent.roots.state[0].clone(),
                vec![StateWriteV1 {
                    key: key.as_bytes().to_vec(),
                    before: parent.roots.state[0].get(key.as_bytes(), &state).unwrap(),
                    after: Some(id),
                }],
                &state,
                4 << 20,
            )
            .unwrap();
            parent.roots.state[0] = delta.after;
            for page in delta.new_nodes {
                state.0.insert(page.digest, page.bytes);
            }
        }
        let source = values.clone();
        let live = parent.fence().unwrap();
        let mut txn =
            ExecutionTransaction::begin(parent, [4; 32], &state, &memory, limits()).unwrap();
        txn.write(Ns::Effects, b"outer-write", Some([99; 32]))
            .unwrap();
        let mut event = moved(1, GeoPos::new(4, 5, 6));
        if failure == 1 {
            event.time += 1;
        }
        let mut quota = io();
        if failure == 4 {
            quota.write_bytes_remaining = 0;
        }
        let result = world.contribute_iws_agent_event(
            &event,
            &mut txn,
            &mut AgentEventContext {
                values: &source,
                mutation: BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut quota,
                    cancelled: || failure == 2,
                },
                persist_original: |_: &ReservedValue<Vec<u8>>, _| {
                    if failure == 3 {
                        Err(BodyMutationError::Invalid("injected original failure"))
                    } else {
                        Ok(())
                    }
                },
                max_event_bytes: 4096,
            },
        );
        assert!(result.is_err(), "failure {failure}");
        let guard = txn.guard();
        assert!(txn.seal(&guard, &live, 0).is_err());
        assert_eq!(world.state, unchanged);
    }
}
