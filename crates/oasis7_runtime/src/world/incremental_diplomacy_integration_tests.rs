//! Whole GEN-qualified diplomacy observations through the actual native aggregate.
use super::*;
use oasis7_world_state::{
    AllianceState, WarState,
    incremental_diplomacy::{
        DiplomacyIndexWorkV1, observe_current_active_wars_v1,
        observe_current_alliance_memberships_v1,
    },
    incremental_field_read::WorldFieldReadContextV1,
};
#[test]
fn diplomacy_whole_genesis_observes_multiple_memberships_and_only_active_wars() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    for (outer, embedded, members) in [
        ("outer-a", "different-a", vec!["alice", "bob"]),
        ("outer-b", "different-b", vec!["alice"]),
    ] {
        world.state.alliances.insert(
            outer.into(),
            AllianceState {
                alliance_id: embedded.into(),
                members: members.into_iter().map(str::to_owned).collect(),
                charter: "charter".into(),
                formed_by_agent_id: "alice".into(),
                formed_at: 0,
            },
        );
    }
    for (outer, active) in [("active", true), ("inactive", false)] {
        world.state.wars.insert(
            outer.into(),
            WarState {
                war_id: "different-war".into(),
                initiator_agent_id: "alice".into(),
                aggressor_alliance_id: "outer-a".into(),
                defender_alliance_id: "outer-b".into(),
                objective: "objective".into(),
                intensity: 1,
                active,
                declared_mobilization_electricity_cost: 0,
                declared_mobilization_data_cost: 0,
                max_duration_ticks: 10,
                aggressor_score: 0,
                defender_score: 0,
                concluded_at: None,
                winner_alliance_id: None,
                loser_alliance_id: None,
                settlement_summary: None,
                participant_outcomes: vec![],
                declared_at: 0,
            },
        );
    }
    let expected: Vec<String> = world
        .state
        .alliances
        .iter()
        .filter(|(_, a)| a.members.iter().any(|m| m == "alice"))
        .map(|(id, _)| id.clone())
        .collect();
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let qualification = fixture.current.native().diplomacy();
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [173; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: oasis7_formal_protocol::incremental_execution::EXECUTION_MAX_READS,
            max_writes: 1024,
            max_facts: 64,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let live = tx.parent_fence();
    let mut io = genesis::io();
    let mut keys = genesis::keys();
    let mut read = WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Business),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };

    let mut work = DiplomacyIndexWorkV1 {
        cells_remaining: 100_000,
        comparison_bytes_remaining: 16 << 20,
        max_rows: 64,
        max_members: 64,
        max_text_bytes: 4096,
        max_pending_rows: 32,
        pending_entries_remaining: 100_000,
    };
    let guard = tx.guard();
    let memberships = observe_current_alliance_memberships_v1(
        "alice",
        qualification,
        &fixture.roots,
        &mut tx,
        &mut read,
        &mut work,
        &guard,
        &live,
    )
    .unwrap();
    assert_eq!(
        memberships.ids().collect::<Vec<_>>(),
        expected.iter().map(String::as_str).collect::<Vec<_>>()
    );
    memberships.validate(&mut tx, &memory, &live).unwrap();
    for alliance in ["outer-a", "outer-b"] {
        let guard = tx.guard();
        let wars = observe_current_active_wars_v1(
            alliance,
            qualification,
            &fixture.roots,
            &mut tx,
            &mut read,
            &mut work,
            &guard,
            &live,
        )
        .unwrap();
        assert_eq!(wars.ids().collect::<Vec<_>>(), vec!["active"]);
    }
    let guard = tx.guard();
    let missing = observe_current_alliance_memberships_v1(
        "missing",
        qualification,
        &fixture.roots,
        &mut tx,
        &mut read,
        &mut work,
        &guard,
        &live,
    )
    .unwrap();
    assert_eq!(missing.ids().len(), 0);
    tx.write(
        StateNamespaceV1::Scheduler,
        b"diplomacy-revision-probe",
        None,
    )
    .unwrap();
    assert!(matches!(
        memberships.validate(&mut tx, &memory, &live),
        Err(
            oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(
                ExecutionError::StaleGuard
            )
        )
    ));
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}
