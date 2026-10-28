use super::*;
use oasis7_formal_protocol::incremental_execution::{
    AuthorityBinding, ExecutionParent, RootSetV1, TransactionLimits, TransitionVerifier,
};
use oasis7_formal_protocol::incremental_state::{StateDeltaV1, StateWriteV1};
use oasis7_world_state::{AgentCell, AgentState};
use std::collections::BTreeMap;

#[derive(Default)]
struct Pages(BTreeMap<String, Vec<u8>>);
impl MerkleNodeReader for Pages {
    type Node = Vec<u8>;
    fn read_node(&self, id: &str, max: usize) -> Result<Option<Vec<u8>>, String> {
        let page = self.0.get(id).ok_or("missing test page")?;
        if page.len() > max {
            return Err("test page bound".into());
        }
        Ok(Some(page.clone()))
    }
}
struct Sink {
    fail: bool,
    writes: usize,
}
impl ValueNodeWriter for Sink {
    fn put_node(&mut self, _: [u8; 32], _: ReservedValue<Vec<u8>>) -> Result<(), ValueTreeError> {
        self.writes += 1;
        if self.fail {
            Err(ValueTreeError::Invalid("injected sink failure"))
        } else {
            Ok(())
        }
    }
}
fn io() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 100,
        read_bytes_remaining: 100_000,
        write_bytes_remaining: 100_000,
    }
}
fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 16,
        max_writes: 16,
        max_facts: 16,
        max_pending_node_bytes: 4 * 1024 * 1024,
    }
}
fn fixture(memory: &MemoryBudget) -> (World, ExecutionParent, Pages) {
    let mut world = World::new();
    world.state.time = 11;
    let mut agent = AgentState::new(
        "alice",
        oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    );
    agent.body_view = BodyKernelView {
        mass_kg: 10,
        radius_cm: 10,
        thrust_limit: 10,
        cross_section_cm2: 10,
    };
    world
        .state
        .agents
        .insert("alice".into(), AgentCell::new(agent, 3));
    let value = AgentBodyValueV1 {
        body_view: world.state.agents["alice"].state.body_view.clone(),
        last_active: 3,
    };
    let value_ref = write_body_value(
        &value,
        &mut BodyMutationContext {
            sink: &mut Discard,
            memory,
            io: &mut io(),
            cancelled: || false,
        },
    )
    .unwrap();
    let mut parent = ExecutionParent {
        commit: [1; 32],
        roots: RootSetV1::empty("body-test".into(), [2; 32]).unwrap(),
        authority: AuthorityBinding {
            token: [3; 32],
            epoch: 0,
        },
    };
    let mut pages = Pages::default();
    let delta = StateDeltaV1::build(
        parent.roots.state[0].clone(),
        vec![StateWriteV1 {
            key: AgentBodyKeyV1::new("alice").unwrap().as_bytes().to_vec(),
            before: None,
            after: Some(value_ref),
        }],
        &pages,
        4 * 1024 * 1024,
    )
    .unwrap();
    parent.roots.state[0] = delta.after;
    for page in delta.new_nodes {
        pages.0.insert(page.digest, page.bytes);
    }

    let clock_ref = write_field_bytes(
        &world_time_bytes_v1(world.state.time),
        &mut BodyMutationContext {
            sink: &mut Discard,
            memory,
            io: &mut io(),
            cancelled: || false,
        },
    )
    .unwrap();
    let delta = StateDeltaV1::build(
        parent.roots.state[1].clone(),
        vec![StateWriteV1 {
            key: WORLD_TIME_KEY_V1.to_vec(),
            before: None,
            after: Some(clock_ref),
        }],
        &pages,
        4 * 1024 * 1024,
    )
    .unwrap();
    parent.roots.state[1] = delta.after;
    for page in delta.new_nodes {
        pages.0.insert(page.digest, page.bytes);
    }
    (world, parent, pages)
}

#[test]
fn actual_prepared_body_updates_one_leaf_in_shared_transaction() {
    let memory = MemoryBudget::new(32 * 1024 * 1024, 1024 * 1024).unwrap();
    let (world, parent, pages) = fixture(&memory);
    let original = world.state.clone();
    let live = parent.fence().unwrap();
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [4; 32], &pages, &memory, limits()).unwrap();
    txn.write(
        StateNamespaceV1::Allocator,
        b"outer-event-sequence",
        Some([5; 32]),
    )
    .unwrap();
    let view = BodyKernelView {
        mass_kg: 12,
        ..world.state.agents["alice"].state.body_view.clone()
    };
    let mut sink = Sink {
        fail: false,
        writes: 0,
    };
    world
        .contribute_iws_body_attributes_update(
            "alice",
            &view,
            &mut txn,
            &mut BodyMutationContext {
                sink: &mut sink,
                memory: &memory,
                io: &mut io(),
                cancelled: || false,
            },
        )
        .unwrap();
    let guard = txn.guard();
    let prepared = txn.seal(&guard, &live, 0).unwrap();
    assert_eq!(prepared.record.writes.len(), 2);
    assert_eq!(prepared.record.reads.len(), 3);
    let expected = AgentBodyValueV1 {
        body_view: view,
        last_active: 11,
    };
    let expected_ref = write_body_value(
        &expected,
        &mut BodyMutationContext {
            sink: &mut Discard,
            memory: &memory,
            io: &mut io(),
            cancelled: || false,
        },
    )
    .unwrap();
    assert_eq!(prepared.record.writes[0].mutation.after, Some(expected_ref));
    assert_eq!(world.state, original);
    assert_eq!(sink.writes, 2); // only after leaf and its descriptor
    TransitionVerifier {
        parent: &parent,
        operation: [4; 32],
        source: &pages,
        memory: &memory,
        class: oasis7_proto::memory_budget::MemoryClass::Ordinary,
        max_pending_node_bytes: 4 * 1024 * 1024,
    }
    .verify(&prepared.record)
    .unwrap();
}

#[test]
fn all_body_failures_poison_preceding_outer_write_without_mutating_world() {
    for mode in 0..8 {
        let memory = MemoryBudget::new(32 * 1024 * 1024, 1024 * 1024).unwrap();
        let (mut world, parent, pages) = fixture(&memory);
        if mode == 0 {
            world.state.agents.get_mut("alice").unwrap().last_active += 1;
        }
        if mode == 7 {
            world.state.time += 1;
        }
        let original = world.state.clone();
        let live = parent.fence().unwrap();
        let mut txn =
            ExecutionTransaction::begin(parent, [4; 32], &pages, &memory, limits()).unwrap();
        txn.write(
            StateNamespaceV1::Allocator,
            b"debit-before-body",
            Some([5; 32]),
        )
        .unwrap();
        let mut quota = io();
        if mode == 2 {
            quota.nodes_remaining = 0;
        }
        let alien = MemoryBudget::new(32 * 1024 * 1024, 0).unwrap();
        let mut view = world.state.agents["alice"].state.body_view.clone();
        if mode == 4 {
            view.mass_kg = 0;
        } else {
            view.mass_kg += 1;
        }
        let mut sink = Sink {
            fail: mode == 1,
            writes: 0,
        };
        let result = world.contribute_iws_body_attributes_update(
            if mode == 6 { "missing" } else { "alice" },
            &view,
            &mut txn,
            &mut BodyMutationContext {
                sink: &mut sink,
                memory: if mode == 5 { &alien } else { &memory },
                io: &mut quota,
                cancelled: || mode == 3,
            },
        );
        assert!(result.is_err(), "mode {mode}");
        let guard = txn.guard();
        assert!(matches!(
            txn.seal(&guard, &live, 0),
            Err(ExecutionError::Aborted)
        ));
        assert_eq!(world.state, original);
    }
}

#[test]
fn body_codec_has_exact_fixed_scalars_and_rejects_wrong_domain_and_length() {
    let value = AgentBodyValueV1 {
        body_view: BodyKernelView {
            mass_kg: 1,
            radius_cm: 2,
            thrust_limit: 3,
            cross_section_cm2: 4,
        },
        last_active: 5,
    };
    let bytes = value.canonical_bytes();
    assert_eq!(
        &bytes[bytes.len() - 40..],
        &[1u64, 2, 3, 4, 5]
            .into_iter()
            .flat_map(u64::to_be_bytes)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        AgentBodyValueV1::from_canonical_bytes(&bytes).unwrap(),
        value
    );
    assert!(AgentBodyValueV1::from_canonical_bytes(&bytes[..bytes.len() - 1]).is_err());
    let mut wrong = bytes;
    wrong[0] ^= 1;
    assert!(AgentBodyValueV1::from_canonical_bytes(&wrong).is_err());
    assert!(AgentBodyKeyV1::new("").is_err());
    assert!(AgentBodyKeyV1::new(&"x".repeat(1024)).is_err());
}
