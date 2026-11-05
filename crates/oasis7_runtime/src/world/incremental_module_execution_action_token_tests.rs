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
            let bytes = crate::util::to_canonical_cbor(envelope).unwrap();
            assert!(bytes.len() <= 1 << 20);
            *blake3::hash(&bytes).as_bytes()
        })
        .collect();
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
    let other = ExecutionTransaction::begin(
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
    drop(counter);
    drop(expected_inputs);
}
