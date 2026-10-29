//! Physical candidate persistence and body continuation, no canonical acceptance.
use super::super::physical::{Directory, install_fixture_pages, logical_io, physical_io};
use super::*;
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression,
    objects::{IncrementalObjectStore, ObjectIo},
};
use oasis7_formal_protocol::{
    incremental_execution::{RootSetV1, TransitionRecordV1},
    incremental_history::{CommitBodyV1, HistoryPrefixV1},
};
use std::sync::atomic::AtomicBool;

fn prepare(
    world: &World,
    request: &CommitPreparationRequest<'_>,
    event: &WorldEvent,
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
) -> ReservedValue<crate::PreparedRuntimeCommit> {
    let pages = store
        .state_session(physical_io(), MemoryClass::Ordinary)
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
                        persist_original: |bytes: &ReservedValue<Vec<u8>>, id| {
                            objects
                                .write_object(
                                    &mut bytes.as_slice(),
                                    bytes.len() as u64,
                                    Some(id),
                                    &mut ObjectIo {
                                        physical: &mut physical,
                                        logical: &mut logical,
                                        class: MemoryClass::Ordinary,
                                        cancelled: &cancelled,
                                    },
                                )
                                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
                            Ok(())
                        },
                        max_event_bytes: 1 << 20,
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
        let (world, parent, state, values, config, genesis, catalog) = setup(&memory);
        let second_event = event(&world, 2);
        let (expected, first_id, catalog_id, config_id) = {
            let store = IncrementalPageStore::create(&path, memory.clone(), compression).unwrap();
            install_fixture_pages(&store, &memory, &state, &values);
            let mut physical = physical_io();
            let mut logical = logical_io();
            let objects =
                IncrementalObjectStore::create(&store, &mut physical, &cancelled).unwrap();
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
                .state_session(physical_io(), MemoryClass::Ordinary)
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
        drop(state);
        drop(values);
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
            .state_session(physical_io(), MemoryClass::Recovery)
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
