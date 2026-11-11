//! Genuine GEN/Current common-admission setup shared by the queue/VM fixtures.
use super::*;
use crate::world::incremental_action_admission::*;
use crate::world::incremental_action_execution::SelectedPendingActionV1;
pub(super) fn observe<'selected, 'parent, 'memory>(
    selected: &'selected SelectedPendingActionV1,
    tx: &mut ExecutionTransaction<'parent, Reader<'memory>>,
    fixture: &genesis::Fixture<'memory>,
    reader: &Reader<'memory>,
    memory: &'memory MemoryBudget,
    live: &ExecutionFence,
) -> Result<SelectedActionAdmissionV1<'selected>, ActionAdmissionErrorV1> {
    let mut ri = genesis::io();
    let mut rk = genesis::keys();
    let mut oi = genesis::io();
    let mut ok = genesis::keys();
    let mut wi = genesis::io();
    let mut wk = genesis::keys();
    let mut sink = fixture.cas.clone();
    let mut write = genesis::member(&mut sink, memory, &mut wi, &mut wk, || false);
    let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Authorization),
        state: reader,
        values: ValueReadContext {
            source: reader,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut ri,
        },
        key_work: &mut rk,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: tx,
        values: ValueReadContext {
            source: reader,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut oi,
        },
        key_work: &mut ok,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut scan = genesis::scan();
    let mut work = genesis::current_work(&fixture.roots).runtime.revocation;
    let guard = overlay.transaction.guard();
    let current =
        oasis7_world_state::incremental_runtime_revocation::begin_runtime_revocation_current_v1(
            fixture.current.runtime().revocation(),
            &mut overlay,
            &mut read,
            &mut scan,
            &mut work,
            &mut write,
            &guard,
            live,
        )
        .unwrap();
    let guard = overlay.transaction.guard();
    admit_selected_action_v1(
        selected,
        fixture.current.runtime().revocation(),
        &current,
        &mut overlay,
        &mut read,
        &mut scan,
        &mut work,
        &guard,
        live,
    )
}

pub(super) fn admit<'selected, 'parent, 'memory>(
    selected: &'selected SelectedPendingActionV1,
    tx: &mut ExecutionTransaction<'parent, Reader<'memory>>,
    fixture: &genesis::Fixture<'memory>,
    reader: &Reader<'memory>,
    memory: &'memory MemoryBudget,
    live: &ExecutionFence,
) -> ReservedValue<UnboundActionAdmissionV1<'selected>> {
    match observe(selected, tx, fixture, reader, memory, live).unwrap() {
        SelectedActionAdmissionV1::Unbound(admission) => admission,
        SelectedActionAdmissionV1::RequiresSafeAlternativeScope(_) => {
            panic!("fixture requires full safe-alternative scope")
        }
    }
}

#[test]
fn common_admission_actual_match_never_mints_unbound_permission() {
    let memory = MemoryBudget::new(512 << 20, 8 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    world.submit_action(crate::Action::RegisterAgent {
        agent_id: "selected".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(1, 2, 3),
    });
    world.capability_revocation_state.safe_alternatives.insert(
        "matching".into(),
        serde_json::json!({"authorization":{"action_id":1}}),
    );
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [124; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 256,
            max_writes: 32,
            max_facts: 8,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    let live = tx.parent_fence();
    let guard = tx.guard();
    let mut work = genesis::current_work(&fixture.roots).runtime.pending;
    let mut pending =
        oasis7_world_state::incremental_pending_action_overlay::begin_pending_action_current_v1(
            fixture.current.runtime().pending(),
            &fixture.roots,
            &mut tx,
            &memory,
            &guard,
            &live,
            &mut work.index,
            || false,
        )
        .unwrap();
    let mut ri = genesis::io();
    let mut rk = genesis::keys();
    let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Authorization),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut ri,
        },
        key_work: &mut rk,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut originals = Originals {
        objects: fixture.objects.clone(),
        memory: &memory,
    };
    let selected = crate::world::incremental_action_execution::take_pending_action(
        &mut pending,
        &mut tx,
        &mut read,
        &mut work,
        &mut originals,
        &live,
    )
    .unwrap()
    .unwrap();
    match observe(&selected, &mut tx, &fixture, &reader, &memory, &live).unwrap() {
        SelectedActionAdmissionV1::RequiresSafeAlternativeScope(record) => {
            record.validate(&tx, &live).unwrap();
            assert_eq!(record.lineage(), "matching");
        }
        SelectedActionAdmissionV1::Unbound(_) => panic!("matching action bypassed scope"),
    }
    assert_eq!(
        tx.ordered_fact_count(),
        1,
        "admission must not publish business events"
    );
}
