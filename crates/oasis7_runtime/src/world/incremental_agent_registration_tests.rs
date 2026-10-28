use super::*;
use crate::world::incremental_agent_genesis::AgentGenesisContext;
use oasis7_agent_api::world_action::GeoPos;
use oasis7_world_state::{
    incremental_agent_fields::{AgentFieldKeyV1, AgentFieldV1},
    state::core_policy_transition::PreparedCorePolicyEvent,
};

pub(super) fn registration(id: u64, agent: &str, version: u8) -> WorldEvent {
    WorldEvent {
        id,
        time: 11,
        caused_by: Some(crate::CausedBy::Action(7)),
        body: WorldEventBody::Domain(DomainEvent::AgentRegistered {
            registration_contract_version: version,
            agent_id: agent.into(),
            pos: GeoPos::new(-7, 8, 9),
        }),
    }
}
pub(super) fn registration_limits()
-> oasis7_formal_protocol::incremental_execution::TransactionLimits {
    let mut limits = limits();
    limits.max_reads = 256;
    limits.max_writes = 256;
    limits
}
pub(super) fn registration_io() -> oasis7_formal_protocol::incremental_value::ValueIoBudget {
    let mut io = io();
    io.nodes_remaining = 10_000;
    io.read_bytes_remaining = 16 << 20;
    io.write_bytes_remaining = 16 << 20;
    io
}

#[test]
fn registration_then_move_matches_existing_reducer_fields_without_installing_cache() {
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
    let source = values.clone();
    let live = parent.fence().unwrap();
    let mut txn = ExecutionTransaction::begin(
        parent.clone(),
        [4; 32],
        &state,
        &memory,
        registration_limits(),
    )
    .unwrap();
    let first = registration(1, "bob", 1);
    let next = WorldEvent {
        id: 2,
        time: 11,
        caused_by: None,
        body: WorldEventBody::Domain(DomainEvent::AgentMoved {
            agent_id: "bob".into(),
            from: GeoPos::new(-7, 8, 9),
            to: GeoPos::new(i64::MIN, 8, i64::MAX),
        }),
    };
    let mut originals = Vec::new();
    // Independent existing reducer creates and updates the expected cell.
    let mut expected = World::new();
    expected.state.time = 11;
    for event in [&first, &next] {
        let WorldEventBody::Domain(domain) = &event.body else {
            unreachable!()
        };
        PreparedCorePolicyEvent::prepare(&expected.state, domain, 11)
            .unwrap()
            .install_infallible(&mut expected.state);
        expected
            .state
            .agents
            .get_mut("bob")
            .unwrap()
            .mailbox
            .push_back(domain.clone());
        world
            .contribute_iws_agent_event(
                event,
                &mut txn,
                &mut AgentEventContext {
                    values: &source,
                    mutation: BodyMutationContext {
                        sink: &mut values,
                        memory: &memory,
                        io: &mut registration_io(),
                        cancelled: || false,
                    },
                    persist_original: |bytes: &ReservedValue<Vec<u8>>, id| {
                        assert_eq!(
                            bytes.as_slice(),
                            oasis7_wasm_abi::encode_canonical_cbor(event).unwrap()
                        );
                        originals.push(id);
                        Ok(())
                    },
                    max_event_bytes: 4096,
                },
            )
            .unwrap();
    }
    let guard = txn.guard();
    let prepared = txn.seal(&guard, &live, 0).unwrap();
    assert_eq!(world.state, unchanged);
    assert!(!world.state.agents.contains_key("bob"));
    assert_eq!(
        prepared
            .record
            .facts
            .iter()
            .map(|f| f.id)
            .collect::<Vec<_>>(),
        originals
    );
    let mut combined = Pages(state.0.clone());
    for page in &prepared.new_nodes {
        combined.0.insert(page.digest.clone(), page.bytes.clone());
    }
    let mut expected_values = Values::default();
    let mut visited = 0;
    expected
        .write_iws_agent_genesis_fields(
            "bob",
            &mut AgentGenesisContext {
                mutation: BodyMutationContext {
                    sink: &mut expected_values,
                    memory: &memory,
                    io: &mut registration_io(),
                    cancelled: || false,
                },
                emit: |ns, key: &[u8], id| {
                    assert_eq!(
                        prepared.record.after.state[ns as usize - 1]
                            .get(key, &combined)
                            .unwrap(),
                        Some(id)
                    );
                    visited += 1;
                    Ok(())
                },
                mailbox_original: |ordinal, _: &DomainEvent| Ok(originals[ordinal as usize]),
                max_scalar_bytes: 4096,
            },
        )
        .unwrap();
    assert_eq!(visited + 1, prepared.record.writes.len()); // plus event allocator
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
fn registration_conflicts_limits_and_late_failure_abort_outer_transaction() {
    for failure in 0..7 {
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
        let source = values.clone();
        let live = parent.fence().unwrap();
        let mut profile = registration_limits();
        if failure == 3 {
            profile.max_writes = 2;
        }
        let mut txn =
            ExecutionTransaction::begin(parent, [4; 32], &state, &memory, profile).unwrap();
        txn.write(Ns::Effects, b"outer-write", Some([99; 32]))
            .unwrap();
        if failure == 4 {
            let key = AgentFieldKeyV1::new("bob", AgentFieldV1::SlotInstalled, 0).unwrap();
            txn.write(Ns::Business, key.as_bytes(), Some([88; 32]))
                .unwrap();
        }
        let event = registration(
            1,
            match failure {
                1 => "industrial:reserved",
                2 => "alice",
                _ => "bob",
            },
            if failure == 0 { 2 } else { 1 },
        );
        let result = world.contribute_iws_agent_event(
            &event,
            &mut txn,
            &mut AgentEventContext {
                values: &source,
                mutation: BodyMutationContext {
                    sink: &mut values,
                    memory: &memory,
                    io: &mut registration_io(),
                    cancelled: || failure == 5,
                },
                persist_original: |_: &ReservedValue<Vec<u8>>, _| {
                    if failure == 6 {
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
