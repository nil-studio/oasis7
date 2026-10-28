//! Actual Runtime contribution on physical pages/original objects. This is a
//! body-path fixture, not whole-world activation/authority/recovery acceptance.
use super::*;
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageCompression, PageIoBudget,
    objects::{IncrementalObjectStore, ObjectIo},
};
use std::sync::atomic::AtomicBool;

fn physical_io() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 100_000,
        read_bytes_remaining: 128 << 20,
        writes_remaining: 100_000,
        write_bytes_remaining: 128 << 20,
    }
}
fn logical_io() -> oasis7_formal_protocol::incremental_value::ValueIoBudget {
    oasis7_formal_protocol::incremental_value::ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 128 << 20,
        write_bytes_remaining: 128 << 20,
    }
}
struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("oasis7-body-event-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn contribute(
    world: &World,
    event: &WorldEvent,
    parent: ExecutionParent,
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
) -> ReservedValue<oasis7_formal_protocol::incremental_execution::PreparedTransition> {
    let source = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap();
    let mut sink = store.value_session(physical_io());
    let values = store.value_session(physical_io());
    let cancelled = AtomicBool::new(false);
    let mut physical = physical_io();
    let objects =
        IncrementalObjectStore::open(store, MemoryClass::Ordinary, &mut physical, &cancelled)
            .unwrap();
    let live = parent.fence().unwrap();
    let mut txn = ExecutionTransaction::begin(parent, [4; 32], &source, memory, limits()).unwrap();
    world
        .contribute_iws_body_event(
            event,
            &mut txn,
            &mut BodyEventContext {
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
                                logical: &mut logical_io(),
                                class: MemoryClass::Ordinary,
                                cancelled: &cancelled,
                            },
                        )
                        .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
                    Ok(())
                },
                max_event_bytes: 1024 * 1024,
            },
        )
        .unwrap();
    let guard = txn.guard();
    txn.seal(&guard, &live, 0).unwrap()
}

fn install_fixture_pages(
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
    state: &Pages,
    values: &Values,
) {
    let cancelled = AtomicBool::new(false);
    for (id, bytes) in &state.0 {
        let id: [u8; 32] = hex::decode(id).unwrap().try_into().unwrap();
        let charge = memory.reserve(bytes.len(), MemoryClass::Ordinary).unwrap();
        store
            .put_page(
                id,
                &charge.retain(bytes.clone()),
                &mut physical_io(),
                &cancelled,
            )
            .unwrap();
    }
    for (id, bytes) in values.pages.borrow().iter() {
        store
            .put_page(*id, bytes, &mut physical_io(), &cancelled)
            .unwrap();
    }
}

#[test]
fn physical_event_original_reopens_and_actual_body_continuation_is_deterministic() {
    let directory = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (world, mut parent, mut state) = fixture(&memory);
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
    let root = directory.0.join("pages");
    let store = IncrementalPageStore::create(&root, memory.clone(), PageCompression::Raw).unwrap();
    let cancelled = AtomicBool::new(false);
    install_fixture_pages(&store, &memory, &state, &values);
    let objects = IncrementalObjectStore::create(&store, &mut physical_io(), &cancelled).unwrap();
    drop(objects);
    let original = event(&world, 1);
    let first = contribute(&world, &original, parent.clone(), &store, &memory);
    let mut source = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap();
    source.persist_transition_pages(&first).unwrap();
    let t = ExecutionParent {
        commit: [8; 32],
        roots: first.record.after.clone(),
        authority: parent.authority,
    };
    let mut second_event = event(&world, 2);
    if let WorldEventBody::Domain(DomainEvent::BodyAttributesUpdated { view, .. }) =
        &mut second_event.body
    {
        view.mass_kg += 1;
    }
    let expected = contribute(&world, &second_event, t.clone(), &store, &memory);
    drop(source);
    drop(store);

    let replay_memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let reopened = IncrementalPageStore::open(
        &root,
        replay_memory.clone(),
        PageCompression::Raw,
        MemoryClass::Recovery,
    )
    .unwrap();
    let source = reopened
        .state_session(physical_io(), MemoryClass::Recovery)
        .unwrap();
    TransitionVerifier {
        parent: &parent,
        operation: [4; 32],
        source: &source,
        memory: &replay_memory,
        class: MemoryClass::Recovery,
        max_pending_node_bytes: 4 << 20,
    }
    .verify(&first.record)
    .unwrap();
    let objects = IncrementalObjectStore::open(
        &reopened,
        MemoryClass::Recovery,
        &mut physical_io(),
        &cancelled,
    )
    .unwrap();
    let original_id = first.record.facts[0].id;
    let bytes = objects
        .read_object_owned(
            &original_id,
            1024 * 1024,
            &mut ObjectIo {
                physical: &mut physical_io(),
                logical: &mut logical_io(),
                class: MemoryClass::Recovery,
                cancelled: &cancelled,
            },
        )
        .unwrap();
    assert_eq!(
        bytes.as_slice(),
        oasis7_wasm_abi::encode_canonical_cbor(&original).unwrap()
    );
    let restored_event: WorldEvent = serde_cbor::from_slice(&bytes).unwrap();
    assert_eq!(restored_event, original);
    let resumed = contribute(&world, &second_event, t, &reopened, &replay_memory);
    assert_eq!(resumed.record, expected.record);
    assert_eq!(resumed.new_nodes, expected.new_nodes);
}

#[test]
fn prepared_driver_material_persists_roots_history_and_catalog_without_selecting_head() {
    use super::commit::{request, setup};
    use oasis7_formal_protocol::incremental_execution::RootSetV1;
    use oasis7_formal_protocol::incremental_history::{
        CatalogQueryV1, CatalogRootV1, CommitBodyV1,
    };
    let directory = Directory::new();
    let memory = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let (world, parent, state, values, config, genesis, catalog) = setup(&memory);
    let root = directory.0.join("driver-pages");
    let store = IncrementalPageStore::create(&root, memory.clone(), PageCompression::Raw).unwrap();
    install_fixture_pages(&store, &memory, &state, &values);
    let cancelled = AtomicBool::new(false);
    let objects = IncrementalObjectStore::create(&store, &mut physical_io(), &cancelled).unwrap();
    let source = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap();
    let mut sink = store.value_session(physical_io());
    let reader = store.value_session(physical_io());
    let fence = parent.fence().unwrap();
    let original = event(&world, 1);
    let prepared = world
        .prepare_iws_commit(
            &request(&parent, &config, &genesis, &catalog),
            &source,
            &memory,
            || Ok::<_, BodyMutationError>(fence),
            |world, txn| {
                world.contribute_iws_body_event(
                    &original,
                    txn,
                    &mut BodyEventContext {
                        values: &reader,
                        mutation: BodyMutationContext {
                            sink: &mut sink,
                            memory: &memory,
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
                                        physical: &mut physical_io(),
                                        logical: &mut logical_io(),
                                        class: MemoryClass::Ordinary,
                                        cancelled: &cancelled,
                                    },
                                )
                                .map_err(|error| BodyMutationError::Original(Box::new(error)))?;
                            Ok(())
                        },
                        max_event_bytes: 1024 * 1024,
                    },
                )
            },
        )
        .unwrap();
    drop(reader);
    drop(sink);
    drop(source);
    let mut source = store
        .state_session(physical_io(), MemoryClass::Ordinary)
        .unwrap();
    source
        .persist_transition_pages(prepared.transition())
        .unwrap();
    let body_id = objects
        .persist_history_material(
            prepared.history(),
            &mut ObjectIo {
                physical: &mut physical_io(),
                logical: &mut logical_io(),
                class: MemoryClass::Ordinary,
                cancelled: &cancelled,
            },
        )
        .unwrap();
    let roots = prepared.root_set_bytes();
    let root_id = objects
        .write_object(
            &mut roots.as_slice(),
            roots.len() as u64,
            Some(prepared.history().body.roots),
            &mut ObjectIo {
                physical: &mut physical_io(),
                logical: &mut logical_io(),
                class: MemoryClass::Ordinary,
                cancelled: &cancelled,
            },
        )
        .unwrap();
    source.persist_catalog_pages(prepared.catalog()).unwrap();
    let catalog_bytes = prepared.catalog_bytes();
    let catalog_id = objects
        .write_object(
            &mut catalog_bytes.as_slice(),
            catalog_bytes.len() as u64,
            None,
            &mut ObjectIo {
                physical: &mut physical_io(),
                logical: &mut logical_io(),
                class: MemoryClass::Ordinary,
                cancelled: &cancelled,
            },
        )
        .unwrap();
    assert_eq!(body_id, prepared.commit_id());
    drop(source);
    drop(objects);
    drop(store);

    let recovery = MemoryBudget::new(32 << 20, 4 << 20).unwrap();
    let store = IncrementalPageStore::open(
        &root,
        recovery.clone(),
        PageCompression::Raw,
        MemoryClass::Recovery,
    )
    .unwrap();
    let objects = IncrementalObjectStore::open(
        &store,
        MemoryClass::Recovery,
        &mut physical_io(),
        &cancelled,
    )
    .unwrap();
    let load = |id| {
        objects
            .read_object_owned(
                &id,
                4096,
                &mut ObjectIo {
                    physical: &mut physical_io(),
                    logical: &mut logical_io(),
                    class: MemoryClass::Recovery,
                    cancelled: &cancelled,
                },
            )
            .unwrap()
    };
    let body = CommitBodyV1::from_canonical_bytes(&load(body_id)).unwrap();
    let roots = RootSetV1::from_canonical_bytes(&load(root_id)).unwrap();
    let catalog = CatalogRootV1::from_canonical_bytes(&load(catalog_id)).unwrap();
    assert_eq!(body, prepared.history().body);
    assert_eq!(roots, prepared.transition().record.after);
    assert_eq!(catalog, prepared.catalog().after);
    let source = store
        .state_session(physical_io(), MemoryClass::Recovery)
        .unwrap();
    assert_eq!(
        catalog
            .lookup(CatalogQueryV1::Height(1), &source)
            .unwrap()
            .unwrap()
            .commit,
        body_id
    );
    TransitionVerifier {
        parent: &parent,
        operation: [4; 32],
        source: &source,
        memory: &recovery,
        class: MemoryClass::Recovery,
        max_pending_node_bytes: 4 << 20,
    }
    .verify(&prepared.transition().record)
    .unwrap();
}
