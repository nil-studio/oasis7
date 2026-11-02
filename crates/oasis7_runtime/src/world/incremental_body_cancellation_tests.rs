use super::tests::{fixture, io, limits};
use super::*;
use oasis7_formal_protocol::incremental_state::{StateDeltaV1, StateWriteV1};
use oasis7_proto::memory_budget::{MemoryBudget, ReservedValue};
use oasis7_world_state::{AgentCell, AgentState};
use std::{cell::Cell, rc::Rc};
struct Puts(Rc<Cell<usize>>);
impl ValueNodeWriter for Puts {
    fn put_node(&mut self, _: [u8; 32], _: ReservedValue<Vec<u8>>) -> Result<(), ValueTreeError> {
        self.0.set(self.0.get() + 1);
        Ok(())
    }
}
#[test]
fn standalone_body_final_cancellation_cannot_seal_another_agents_typed_prefix() {
    let memory = MemoryBudget::new(32 << 20, 1 << 20).unwrap();
    let (mut world, mut parent, mut pages) = fixture(&memory);
    let mut bob = AgentState::new("bob", oasis7_agent_api::world_action::GeoPos::new(0, 0, 0));
    bob.body_view = world.state.agents["alice"].state.body_view.clone();
    world
        .state
        .agents
        .insert("bob".into(), AgentCell::new(bob, 3));
    let puts = Rc::new(Cell::new(0));
    let mut sink = Puts(puts.clone());
    let body = AgentBodyValueV1 {
        body_view: world.state.agents["bob"].state.body_view.clone(),
        last_active: 3,
    };
    let id = write_body_value(
        &body,
        &mut BodyMutationContext {
            sink: &mut sink,
            memory: &memory,
            io: &mut io(),
            cancelled: || false,
        },
    )
    .unwrap();
    let delta = StateDeltaV1::build(
        parent.roots.state[0].clone(),
        vec![StateWriteV1 {
            key: AgentBodyKeyV1::new("bob").unwrap().as_bytes().to_vec(),
            before: None,
            after: Some(id),
        }],
        &pages,
        4 << 20,
    )
    .unwrap();
    parent.roots.state[0] = delta.after;
    for page in delta.new_nodes {
        pages.0.insert(page.digest, page.bytes);
    }
    let mut after = world.state.agents["alice"].state.body_view.clone();
    after.mass_kg = 13;
    // The independent value writer determines both the final real put and its
    // tail checks; the contributor under test never calibrates its own count.
    let calibration_start = puts.get();
    let mut seen = calibration_start;
    let mut tail = 0;
    write_body_value(
        &AgentBodyValueV1 {
            body_view: after.clone(),
            last_active: 11,
        },
        &mut BodyMutationContext {
            sink: &mut sink,
            memory: &memory,
            io: &mut io(),
            cancelled: || {
                if puts.get() != seen {
                    seen = puts.get();
                    tail = 0;
                }
                tail += 1;
                false
            },
        },
    )
    .unwrap();
    let writer_puts = puts.get() - calibration_start;
    let writer_tail = tail;
    assert!(writer_puts > 0 && writer_tail > 0);
    let mut txn =
        ExecutionTransaction::begin(parent.clone(), [93; 32], &pages, &memory, limits()).unwrap();
    let mut prefix = world.state.agents["bob"].state.body_view.clone();
    prefix.mass_kg = 12;
    world
        .contribute_iws_body_attributes_update(
            "bob",
            &prefix,
            &mut txn,
            &mut BodyMutationContext {
                sink: &mut sink,
                memory: &memory,
                io: &mut io(),
                cancelled: || false,
            },
        )
        .unwrap();
    let start = puts.get();
    seen = start;
    tail = 0;
    let result = world.contribute_iws_body_attributes_update(
        "alice",
        &after,
        &mut txn,
        &mut BodyMutationContext {
            sink: &mut sink,
            memory: &memory,
            io: &mut io(),
            cancelled: || {
                if puts.get() != seen {
                    seen = puts.get();
                    tail = 0;
                }
                tail += 1;
                puts.get() == start + writer_puts && tail == writer_tail + 1
            },
        },
    );
    assert_eq!(
        puts.get(),
        start + writer_puts,
        "must reach final descriptor put"
    );
    let cancelled = matches!(
        result,
        Err(BodyMutationError::Value(ValueTreeError::Cancelled))
    );
    let guard = txn.guard();
    let sealed = txn.seal(&guard, &parent.fence().unwrap(), 0);
    let aborted = matches!(sealed, Err(ExecutionError::Aborted));
    let writes = sealed
        .as_ref()
        .map(|value| value.record.writes.len())
        .unwrap_or(0);
    assert!(
        cancelled && aborted,
        "tail={tail}, expected={}, cancelled={cancelled}, aborted={aborted}, sealed_writes={writes}",
        writer_tail + 1
    );
}
