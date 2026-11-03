#![cfg(all(unix, not(target_arch = "wasm32")))]
//! Physical candidate persistence and body continuation, no canonical acceptance.
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression,
    objects::{IncrementalObjectStore, ObjectIo},
};
use oasis7_formal_protocol::{
    incremental_execution::{ExecutionParent, TransactionLimits, TransitionVerifier},
    incremental_history::{CatalogRootV1, GenesisConfigV1, HeaderPreparation, PreparedHistoryV1},
};
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1},
    incremental_history::{CommitBodyV1, HistoryPrefixV1},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass, ReservedValue};
use oasis7_runtime::{
    AgentEventContext, AgentEventOriginalV1, BodyMutationContext, BodyMutationError,
    CommitPreparationRequest, ParentCommitView, PreparedRuntimeCommit, World, WorldEvent,
};
use std::sync::atomic::AtomicBool;
#[allow(dead_code)]
#[path = "support/incremental_agent_event.rs"]
mod agent_fixture;
use agent_fixture::{Directory, logical as logical_io, physical as physical_io};

fn prepare(
    world: &World,
    request: &CommitPreparationRequest<'_>,
    event: &WorldEvent,
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
) -> ReservedValue<PreparedRuntimeCommit> {
    let pages = store
        .state_session(agent_fixture::state_physical(), MemoryClass::Ordinary)
        .unwrap();
    let values = store.value_session(physical_io());
    let mut sink = store.value_session(physical_io());
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let mut logical = logical_io();
    let objects =
        IncrementalObjectStore::open(store, MemoryClass::Ordinary, &mut physical, &cancelled)
            .unwrap();
    let fence = ExecutionParent {
        commit: request.parent.body.digest().unwrap(),
        roots: request.parent.roots.clone(),
        authority: request.parent.authority,
    }
    .fence()
    .unwrap();
    world
        .prepare_iws_commit(
            request,
            &pages,
            memory,
            || Ok::<_, BodyMutationError>(fence),
            |world, txn| {
                world.contribute_iws_agent_event(
                    event,
                    txn,
                    &mut AgentEventContext {
                        values: &values,
                        mutation: BodyMutationContext {
                            sink: &mut sink,
                            memory,
                            io: &mut logical_io(),
                            cancelled: || false,
                        },
                        persist_original: |original: &mut dyn AgentEventOriginalV1| {
                            agent_fixture::persist_original(
                                original,
                                &objects,
                                &mut ObjectIo {
                                    physical: &mut physical,
                                    logical: &mut logical,
                                    class: MemoryClass::Ordinary,
                                    cancelled: &cancelled,
                                },
                            )
                        },
                        max_event_bytes: 1 << 20,
                        original_work: agent_fixture::original_work(),
                    },
                )
            },
        )
        .unwrap()
}

#[test]
fn runtime_candidate_history_reopens_and_continues_from_original_parent_material() {
    let dir = Directory::new();
    let cancelled = AtomicBool::new(false);
    for compression in [PageCompression::Raw, PageCompression::Zstd] {
        let path = dir.0.join(format!("{compression:?}"));
        let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let _fixture = memory.reserve(4 << 20, MemoryClass::Ordinary).unwrap();
        let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
        let (world, parent, config, genesis, catalog) = setup(&store, &memory);
        let second_event = event(&world, 2);
        let (expected, first_id, catalog_id, config_id) = {
            let mut physical = physical_io();
            let mut logical = logical_io();
            let objects = IncrementalObjectStore::open(
                &store,
                MemoryClass::Ordinary,
                &mut physical,
                &cancelled,
            )
            .unwrap();
            let mut io = ObjectIo {
                physical: &mut physical,
                logical: &mut logical,
                class: MemoryClass::Ordinary,
                cancelled: &cancelled,
            };
            let config_bytes = config.canonical_bytes().unwrap();
            let config_id = objects
                .write_object(
                    &mut config_bytes.as_slice(),
                    config_bytes.len() as u64,
                    None,
                    &mut io,
                )
                .unwrap();
            let parent_roots = parent.roots.canonical_bytes().unwrap();
            objects
                .write_object(
                    &mut parent_roots.as_slice(),
                    parent_roots.len() as u64,
                    Some(parent.roots.digest().unwrap()),
                    &mut io,
                )
                .unwrap();
            objects.persist_history_material(&genesis, &mut io).unwrap();
            let first = prepare(
                &world,
                &request(&parent, &config, &genesis, &catalog),
                &event(&world, 1),
                &store,
                &memory,
            );
            let mut pages = store
                .state_session(agent_fixture::state_physical(), MemoryClass::Ordinary)
                .unwrap();
            pages.persist_transition_pages(first.transition()).unwrap();
            pages.persist_catalog_pages(first.catalog()).unwrap();
            let first_id = objects
                .persist_history_material(first.history(), &mut io)
                .unwrap();
            assert_eq!(first_id, first.commit_id());
            objects
                .write_object(
                    &mut first.root_set_bytes().as_slice(),
                    first.root_set_bytes().len() as u64,
                    Some(first.history().body.roots),
                    &mut io,
                )
                .unwrap();
            let catalog_id = objects
                .write_object(
                    &mut first.catalog_bytes().as_slice(),
                    first.catalog_bytes().len() as u64,
                    None,
                    &mut io,
                )
                .unwrap();
            let next_parent = ExecutionParent {
                commit: first_id,
                roots: first.transition().record.after.clone(),
                authority: parent.authority,
            };
            let mut next = request(
                &next_parent,
                &config,
                first.history(),
                &first.catalog().after,
            );
            next.operation = [5; 32];
            next.next_tick = 1;
            let expected = prepare(&world, &next, &second_event, &store, &memory);
            assert_eq!(expected.transition().record.after.height, 2);
            assert_eq!(expected.catalog().after.count, 3);
            assert_eq!(expected.history().body.parent_commit, Some(first_id));
            assert_eq!(
                expected.history().facts.facts[0].id,
                *blake3::hash(&oasis7_wasm_abi::encode_canonical_cbor(&second_event).unwrap())
                    .as_bytes()
            );
            (expected, first_id, catalog_id, config_id)
        };
        // Production readers and sources are closed. The retained expected
        // candidate is a charged reference oracle, never a recovery source.
        drop(genesis);
        drop(store);
        let recovery = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
        let store =
            IncrementalPageStore::open(&path, recovery.clone(), compression, MemoryClass::Recovery)
                .unwrap();
        let objects = IncrementalObjectStore::open(
            &store,
            MemoryClass::Recovery,
            &mut physical_io(),
            &cancelled,
        )
        .unwrap();
        let mut physical = physical_io();
        let mut logical = logical_io();
        let mut io = ObjectIo {
            physical: &mut physical,
            logical: &mut logical,
            class: MemoryClass::Recovery,
            cancelled: &cancelled,
        };
        let config_bytes = objects
            .read_object_owned(&config_id, 4096, &mut io)
            .unwrap();
        let config = GenesisConfigV1::from_canonical_bytes(&config_bytes).unwrap();
        let body_bytes = objects.read_object_owned(&first_id, 4096, &mut io).unwrap();
        let body = CommitBodyV1::from_canonical_bytes(&body_bytes).unwrap();
        let root_bytes = objects
            .read_object_owned(&body.roots, 4096, &mut io)
            .unwrap();
        let roots = RootSetV1::from_canonical_bytes(&root_bytes).unwrap();
        let prefix_bytes = objects
            .read_object_owned(&body.history, 4096, &mut io)
            .unwrap();
        let history = HistoryPrefixV1::from_canonical_bytes(&prefix_bytes).unwrap();
        let catalog_bytes = objects
            .read_object_owned(&catalog_id, 4096, &mut io)
            .unwrap();
        let catalog = CatalogRootV1::from_canonical_bytes(&catalog_bytes).unwrap();
        let transition_bytes = objects
            .read_object_owned(&body.transition.unwrap(), 4 << 20, &mut io)
            .unwrap();
        let transition = TransitionRecordV1::from_canonical_bytes(
            &transition_bytes,
            &recovery,
            MemoryClass::Recovery,
        )
        .unwrap();
        let pages = store
            .state_session(agent_fixture::state_physical(), MemoryClass::Recovery)
            .unwrap();
        TransitionVerifier {
            parent: &parent,
            operation: [4; 32],
            source: &pages,
            memory: &recovery,
            class: MemoryClass::Recovery,
            max_pending_node_bytes: 4 << 20,
        }
        .verify(&transition)
        .unwrap();
        assert_eq!(transition.after, roots);
        let original = objects
            .read_object_owned(&transition.facts[0].id, 1 << 20, &mut io)
            .unwrap();
        let first_event = event(&world, 1);
        assert_eq!(
            original.as_slice(),
            oasis7_wasm_abi::encode_canonical_cbor(&first_event).unwrap()
        );
        assert_eq!(
            serde_cbor::from_slice::<WorldEvent>(&original).unwrap(),
            first_event
        );
        let next_parent = ExecutionParent {
            commit: first_id,
            roots,
            authority: parent.authority,
        };
        let next = CommitPreparationRequest {
            parent: ParentCommitView {
                config: &config,
                roots: &next_parent.roots,
                body: &body,
                history: &history,
                catalog: &catalog,
                authority: next_parent.authority,
            },
            operation: [5; 32],
            next_tick: 1,
            execution_environment: config.execution_environment,
            checkpoint: true,
            limits: limits(),
        };
        let resumed = prepare(&world, &next, &second_event, &store, &recovery);
        assert_eq!(resumed.commit_id(), expected.commit_id());
        assert_eq!(resumed.transition().record, expected.transition().record);
        assert_eq!(resumed.history().body, expected.history().body);
        assert_eq!(resumed.catalog().after, expected.catalog().after);
        assert!(resumed.uses_budget(&recovery));
    }
}

fn limits() -> TransactionLimits {
    TransactionLimits {
        max_reads: 16,
        max_writes: 16,
        max_facts: 16,
        max_pending_node_bytes: 4 << 20,
    }
}
fn event(world: &World, id: u64) -> WorldEvent {
    agent_fixture::event(world, id, false)
}
fn setup(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
) -> (
    World,
    ExecutionParent,
    GenesisConfigV1,
    ReservedValue<PreparedHistoryV1>,
    CatalogRootV1,
) {
    let (world, mut parent) = agent_fixture::fixture(store, memory);
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
    let mut pages = store
        .state_session(agent_fixture::state_physical(), MemoryClass::Ordinary)
        .unwrap();
    let delta = empty
        .extend(&genesis.body, &pages, memory, 4 << 20)
        .unwrap();
    pages.persist_catalog_pages(&delta).unwrap();
    let catalog = delta.after.clone();
    (world, parent, config, genesis, catalog)
}

fn request<'a>(
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
