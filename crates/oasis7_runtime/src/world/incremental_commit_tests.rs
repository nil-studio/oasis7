//! The actual Runtime body event flows through driver/transition/history/catalog.
//! Fixture parents are structural content, not qualified authority proofs.
use super::*;
use crate::world::incremental_commit::{CommitPreparationRequest, ParentCommitView};
use oasis7_formal_protocol::incremental_history::{
    CatalogRootV1, GenesisConfigV1, HeaderPreparation, PreparedHistoryV1,
};

pub(super) fn setup(
    memory: &MemoryBudget,
) -> (
    World,
    ExecutionParent,
    Pages,
    Values,
    GenesisConfigV1,
    ReservedValue<PreparedHistoryV1>,
    CatalogRootV1,
) {
    let (world, mut parent, mut state) = fixture(memory);
    let mut values = Values::default();
    seed(
        &world,
        &mut parent,
        &mut state,
        &mut values,
        memory,
        EventSequenceV1 { next: 1, era: 0 },
        MailboxFrontierV1 { head: 0, tail: 0 },
    );
    let config = GenesisConfigV1 {
        world_id: parent.roots.world_id.clone(),
        format_profile: [21; 32],
        initial_content: [22; 32],
        activation_config: [23; 32],
        execution_environment: [24; 32],
    };
    let scope = config.scope().unwrap();
    parent.roots.genesis_config = scope.genesis_config;
    let empty = CatalogRootV1::empty(scope).unwrap();
    let genesis = HeaderPreparation {
        config: &config,
        catalog: &empty,
        roots: &parent.roots,
        parent: None,
        parent_history: None,
        transition: None,
        genesis_facts: &[],
        operation: [25; 32],
        authority: parent.authority,
        execution_environment: config.execution_environment,
        checkpoint: true,
    }
    .prepare(memory)
    .unwrap();
    parent.commit = genesis.body.digest().unwrap();
    let delta = empty
        .extend(&genesis.body, &state, memory, 4 << 20)
        .unwrap();
    for node in &delta.new_nodes {
        state.0.insert(node.digest.clone(), node.bytes.clone());
    }
    let catalog = delta.after.clone();
    (world, parent, state, values, config, genesis, catalog)
}
pub(super) fn request<'a>(
    parent: &'a ExecutionParent,
    config: &'a GenesisConfigV1,
    history: &'a PreparedHistoryV1,
    catalog: &'a CatalogRootV1,
) -> CommitPreparationRequest<'a> {
    CommitPreparationRequest {
        parent: ParentCommitView {
            config,
            roots: &parent.roots,
            body: &history.body,
            history: &history.prefix,
            catalog,
            authority: parent.authority,
        },
        operation: [4; 32],
        next_tick: 0,
        execution_environment: config.execution_environment,
        checkpoint: true,
        limits: limits(),
    }
}
fn value_charge(values: &Values) -> usize {
    values
        .pages
        .borrow()
        .values()
        .map(|value| value.reserved_bytes())
        .sum()
}

#[test]
fn actual_body_driver_seals_one_consistent_candidate_and_keeps_all_charges() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (world, parent, state, mut values, config, genesis, catalog) = setup(&memory);
    let source = values.clone();
    let before = world.state.clone();
    let fence = parent.fence().unwrap();
    let calls = Cell::new(0);
    let mut stored = Vec::new();
    let mut quota = io();
    let original = event(&world, 1);
    let request = request(&parent, &config, &genesis, &catalog);
    let prepared = world
        .prepare_iws_commit(
            &request,
            &state,
            &memory,
            || {
                calls.set(calls.get() + 1);
                Ok::<_, BodyMutationError>(fence)
            },
            |world, txn| {
                world.contribute_iws_body_event(
                    &original,
                    txn,
                    &mut BodyEventContext {
                        values: &source,
                        mutation: BodyMutationContext {
                            sink: &mut values,
                            memory: &memory,
                            io: &mut quota,
                            cancelled: || false,
                        },
                        persist_original: |_: &ReservedValue<Vec<u8>>, id| {
                            stored.push(id);
                            Ok(())
                        },
                        max_event_bytes: 1024 * 1024,
                    },
                )
            },
        )
        .unwrap();
    assert_eq!(calls.get(), 3);
    assert_eq!(prepared.transition().record.after.height, 1);
    assert_eq!(prepared.transition().record.after.simulation_tick, 0);
    assert_eq!(prepared.history().body.parent_commit, Some(parent.commit));
    assert_eq!(prepared.history().body.catalog_before, catalog);
    assert_eq!(prepared.catalog().after.count, 2);
    assert_eq!(
        prepared.catalog().entry.commit,
        prepared.history().body.digest().unwrap()
    );
    assert_eq!(
        prepared.commit_id(),
        prepared.history().body.digest().unwrap()
    );
    assert_eq!(prepared.history().facts.facts[0].id, stored[0]);
    assert_eq!(
        *blake3::hash(prepared.root_set_bytes()).as_bytes(),
        prepared.history().body.roots
    );
    assert_eq!(
        CatalogRootV1::from_canonical_bytes(prepared.catalog_bytes()).unwrap(),
        prepared.catalog().after
    );
    assert!(prepared.uses_budget(&memory));
    assert!(prepared.root_set_bytes().uses_budget(&memory));
    assert!(prepared.catalog_bytes().uses_budget(&memory));
    assert_eq!(world.state, before);
    let mut changed = fence;
    changed.authority.epoch += 1;
    assert!(matches!(
        prepared.validate_live_fence(&changed),
        Err(ExecutionError::ParentConflict)
    ));
    assert!(memory.used() > value_charge(&values));
    drop(prepared);
    drop(genesis);
    assert_eq!(memory.used(), value_charge(&values));
}

#[test]
fn live_fence_conflicts_at_each_driver_boundary_discard_candidate_only() {
    for boundary in 0..3 {
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let (world, parent, state, mut values, config, genesis, catalog) = setup(&memory);
        let source = values.clone();
        let before = world.state.clone();
        let fence = parent.fence().unwrap();
        let calls = Cell::new(0);
        let applied = Cell::new(false);
        let mut quota = io();
        let original = event(&world, 1);
        let result = world.prepare_iws_commit(
            &request(&parent, &config, &genesis, &catalog),
            &state,
            &memory,
            || {
                let call = calls.get();
                calls.set(call + 1);
                let mut live = fence;
                if call == boundary {
                    live.authority.token[0] ^= 1;
                }
                Ok::<_, BodyMutationError>(live)
            },
            |world, txn| {
                applied.set(true);
                world.contribute_iws_body_event(
                    &original,
                    txn,
                    &mut BodyEventContext {
                        values: &source,
                        mutation: BodyMutationContext {
                            sink: &mut values,
                            memory: &memory,
                            io: &mut quota,
                            cancelled: || false,
                        },
                        persist_original: |_: &ReservedValue<Vec<u8>>, _| Ok(()),
                        max_event_bytes: 1024 * 1024,
                    },
                )
            },
        );
        assert!(matches!(
            result,
            Err(BodyMutationError::Execution(ExecutionError::ParentConflict))
        ));
        assert_eq!(applied.get(), boundary > 0);
        assert_eq!(world.state, before);
        drop(genesis);
        assert_eq!(memory.used(), value_charge(&values));
    }
}

#[test]
fn inconsistent_parent_material_is_rejected_before_running_runtime() {
    for mode in 0..4 {
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let (world, mut parent, mut state, values, config, genesis, mut catalog) = setup(&memory);
        match mode {
            0 => parent.roots.simulation_tick += 1,
            1 => catalog.count += 1,
            2 => catalog.last_commit = Some([99; 32]),
            _ => state.0.clear(),
        }
        let result = world.prepare_iws_commit(
            &request(&parent, &config, &genesis, &catalog),
            &state,
            &memory,
            || panic!("invalid parent cannot reach live authority read"),
            |_, _| -> Result<(), BodyMutationError> { panic!("invalid parent cannot run runtime") },
        );
        assert!(result.is_err());
        drop(genesis);
        assert_eq!(memory.used(), value_charge(&values));
    }
}

#[test]
fn parent_catalog_requires_its_certified_prefix_not_only_a_readable_latest_entry() {
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (world, parent, state, _, config, genesis, catalog) = setup(&memory);
    let fence = parent.fence().unwrap();
    let prepared = world
        .prepare_iws_commit(
            &request(&parent, &config, &genesis, &catalog),
            &state,
            &memory,
            || Ok::<_, BodyMutationError>(fence),
            |_, _| Ok(()),
        )
        .unwrap();
    let mut tail_only = Pages::default();
    for node in &prepared.catalog().new_nodes {
        tail_only.0.insert(node.digest.clone(), node.bytes.clone());
    }
    assert!(
        prepared
            .catalog()
            .after
            .lookup(
                oasis7_formal_protocol::incremental_history::CatalogQueryV1::Height(1),
                &tail_only
            )
            .unwrap()
            .is_some()
    );
    let child_parent = ExecutionParent {
        commit: prepared.commit_id(),
        roots: prepared.transition().record.after.clone(),
        authority: parent.authority,
    };
    let next = request(
        &child_parent,
        &config,
        prepared.history(),
        &prepared.catalog().after,
    );
    let result = world.prepare_iws_commit(
        &next,
        &tail_only,
        &memory,
        || panic!("prefix validation must precede live authority read"),
        |_, _| -> Result<(), BodyMutationError> {
            panic!("incomplete certified prefix must precede runtime apply")
        },
    );
    assert!(result.is_err());
}
