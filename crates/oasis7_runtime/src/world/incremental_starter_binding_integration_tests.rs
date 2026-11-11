//! Genuine whole GEN qualification and strict Current observations; no business authority.
use super::*;
use oasis7_world_state::{
    gameplay_state::StarterOcClaimState,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_semantic_key::hash_semantic_key_v1,
    incremental_world_field_key::{WorldFieldKeyV1, WorldFieldRecordKindV1},
    incremental_world_native::{StarterBindingIndexWorkV1, observe_current_starter_binding_v1},
};
#[test]
fn starter_binding_whole_genesis_and_current_preserve_duplicate_and_revision_semantics() {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    for (outer, agent, key) in [
        ("outer-a", "alice", Some("key")),
        ("outer-b", "bob", Some("key")),
        ("none", "none-agent", None),
        ("empty", "empty-agent", Some("")),
    ] {
        world.state.starter_oc_claims.insert(
            outer.into(),
            StarterOcClaimState {
                player_id: "player".into(),
                agent_id: agent.into(),
                public_key: key.map(str::to_owned),
                ..Default::default()
            },
        );
    }
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let index = fixture.current.native().native().starter_binding_index();
    assert_eq!(index.records(), 4);
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
        root: fixture.roots.state_root(StateNamespaceV1::Authorization),
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
    let mut scan = genesis::scan();
    let mut native = genesis::current_work(&fixture.roots).native.native;
    let mut work = StarterBindingIndexWorkV1 {
        max_rows: 256,
        max_row_text_bytes: 64 << 10,
        max_pending_rows: 1024,
        steps_remaining: 100_000,
        bytes_remaining: 64 << 20,
        comparison_bytes_remaining: 64 << 20,
        moved_slots_remaining: 1_000_000,
        pending_entries_remaining: 100_000,
    };
    let mut observe = |tx: &mut ExecutionTransaction<'_, Reader<'_>>, player: &str, key: &str| {
        let guard = tx.guard();
        observe_current_starter_binding_v1(
            player,
            key,
            index,
            &fixture.roots,
            tx,
            &mut read,
            &mut scan,
            &mut native,
            &mut work,
            &guard,
            &live,
        )
        .unwrap()
    };
    let duplicate = observe(&mut tx, "player", "key");
    assert_eq!(duplicate.matching_claims(), 2);
    assert_eq!(duplicate.unique_agent_id(), None);
    duplicate.validate(&mut tx, &memory, &live).unwrap();
    let empty = observe(&mut tx, "player", "");
    assert_eq!(empty.matching_claims(), 1);
    assert_eq!(empty.unique_agent_id(), Some("empty-agent"));
    assert_eq!(observe(&mut tx, "missing", "key").matching_claims(), 0);
    let field = *oasis7_world_state::incremental_projection::WORLD_STATE_FIELDS
        .iter()
        .find(|f| f.name == "starter_oc_claims")
        .unwrap();
    let digest = hash_semantic_key_v1(
        "outer-b",
        &memory,
        MemoryClass::Ordinary,
        &mut genesis::keys(),
        || false,
    )
    .unwrap();
    let key =
        WorldFieldKeyV1::new(field, WorldFieldRecordKindV1::Entry, Some(digest.digest)).unwrap();
    tx.write(StateNamespaceV1::Authorization, key.as_bytes(), None)
        .unwrap();
    let unique = observe(&mut tx, "player", "key");
    assert_eq!(unique.matching_claims(), 1);
    assert_eq!(unique.unique_agent_id(), Some("alice"));
    unique.validate(&mut tx, &memory, &live).unwrap();
    assert!(matches!(
        duplicate.validate(&mut tx, &memory, &live),
        Err(
            oasis7_world_state::incremental_field_overlay::WorldFieldOverlayErrorV1::Execution(
                ExecutionError::StaleGuard
            )
        )
    ));
    assert_eq!(tx.ordered_fact_count(), 0);
    let guard = tx.guard();
    assert!(matches!(
        tx.seal(&guard, &live, 1),
        Err(ExecutionError::Aborted)
    ));
}
