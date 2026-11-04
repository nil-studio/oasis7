use super::*;
use oasis7_distfs::incremental_page_store::{
    PageCompression,
    objects::{IncrementalObjectStore, ObjectIo},
};
use oasis7_formal_protocol::{
    incremental_state::StateNamespaceV1, incremental_value::ValueReadContext,
};
use oasis7_proto::memory_budget::MemoryClass;
use oasis7_runtime::incremental_action_original::{
    ActionOriginalStreamV1, persist_action_original_v1, persist_world_event_original_v1,
};
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::{
    Action, GeoPos, ModuleInstanceState, WorldEventBody, WorldState,
    incremental_event_fields::{MailboxKeyV1, mailbox_event_ref_v1},
    incremental_module_instance_fields::{ModuleInstanceFieldV1, ModuleInstanceKeyV1},
};
use std::{cell::RefCell, collections::BTreeMap, path::PathBuf, sync::atomic::AtomicU64};
#[path = "incremental_genesis_test_work.rs"]
mod work;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "oasis7-runtime-physical-gen-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn config() -> GenesisConfigV1 {
    GenesisConfigV1 {
        world_id: "runtime-physical".into(),
        format_profile: [1; 32],
        initial_content: [2; 32],
        activation_config: [3; 32],
        execution_environment: [4; 32],
    }
}
fn canonical_work() -> CanonicalCborWorkV1 {
    CanonicalCborWorkV1 {
        visits_remaining: 1_000_000,
        bytes_remaining: 16 << 20,
        max_depth: 32,
        max_map_keys: 256,
        max_key_bytes: 4096,
    }
}
fn limits() -> MemberRecordLimitsV1 {
    MemberRecordLimitsV1 {
        semantic_bytes: 4096,
        payload_bytes: 8 << 20,
        record_bytes: 9 << 20,
    }
}
fn source() -> World {
    let mut state = WorldState::default();
    state.module_instances.insert(
        "outer-instance".into(),
        ModuleInstanceState {
            instance_id: "inner-instance".into(),
            module_id: "inactive-module".into(),
            module_version: "original-version".into(),
            wasm_hash: String::new(),
            owner_agent_id: "alice".into(),
            install_target: Default::default(),
            active: false,
            installed_at: 0,
        },
    );
    let mut world = World::new_with_state(state);
    world.submit_action(Action::RegisterAgent {
        agent_id: "alice".into(),
        pos: GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    assert!(
        world.agent_mailbox_len("alice").unwrap() > 0,
        "fixture must retain actual registered-agent mailbox events"
    );
    world.submit_action(Action::QueryObservation {
        agent_id: "alice".into(),
    });
    world
}
fn persist_stream(
    objects: &IncrementalObjectStore<'_>,
    stream: &mut dyn ActionOriginalStreamV1,
    cancelled: &mut dyn FnMut() -> bool,
    io: &mut ObjectIo<'_>,
) -> Result<(), BodyMutationError> {
    objects
        .write_object_from_stream(
            stream.original_bytes(),
            stream.original_id(),
            io,
            cancelled,
            |out| stream.write_to(out),
        )
        .map_err(|_| BodyMutationError::Invalid("physical original stream"))?;
    Ok(())
}
#[test]
fn current_runtime_genesis_physically_persists_originals_and_reopens_cold_indexes() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let _source_charge = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let world = source();
    let cancelled = AtomicBool::new(false);
    let store = IncrementalPageStore::create(
        &fixture.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let mut object_physical = work::physical_io();
    let objects = IncrementalObjectStore::create(&store, &mut object_physical, &cancelled).unwrap();
    let mut object_logical = work::logical();
    let object_io = RefCell::new(ObjectIo {
        physical: &mut object_physical,
        logical: &mut object_logical,
        class: MemoryClass::Ordinary,
        cancelled: &cancelled,
    });
    let originals = RefCell::new(BTreeMap::new());
    let mail = RefCell::new(BTreeMap::new());
    let canonical = RefCell::new(canonical_work());
    let mut logical = work::logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    let result=prepare_runtime_genesis_index_v1(&world,&store,&config(),&mut RuntimeGenesisPhysicalContextV1{memory:&memory,values:&mut logical,keys:&mut keys,limits:limits(),cancelled:&cancelled},&mut work::work(),work::physical(),
 &mut |reference,bytes| { objects.write_object(&mut &bytes[..],reference.byte_len,Some(reference.id),&mut object_io.borrow_mut()).map_err(|_|ValueTreeError::Invalid("physical artifact original"))?; Ok(()) },
 &mut |ordinal,action| {
 let receipt=persist_action_original_v1(action,&memory,&mut canonical.borrow_mut(),&mut ||cancelled.load(Ordering::Acquire),1<<20,&mut |stream:&mut dyn ActionOriginalStreamV1,poll:&mut dyn FnMut()->bool|persist_stream(&objects,stream,poll,&mut object_io.borrow_mut())).map_err(|_|ValueTreeError::Invalid("action original persistence"))?;
 originals.borrow_mut().insert(ordinal,receipt.original_id()); Ok(OriginalArtifactRefV1 {id:receipt.original_id(),byte_len:receipt.original_bytes()}) },
 &mut |agent,ordinal,event| {
 let original=world.journal().events.iter().find(|candidate|matches!(&candidate.body,WorldEventBody::Domain(domain) if domain==event)).ok_or(BodyMutationError::Invalid("mailbox has no actual journal original"))?;
 let receipt=persist_world_event_original_v1(original,&memory,&mut canonical.borrow_mut(),&mut ||cancelled.load(Ordering::Acquire),1<<20,&mut |stream:&mut dyn ActionOriginalStreamV1,poll:&mut dyn FnMut()->bool|persist_stream(&objects,stream,poll,&mut object_io.borrow_mut()))?;
 mail.borrow_mut().insert((agent.to_owned(),ordinal),receipt.original_id()); Ok(receipt.original_id()) }).unwrap();
    assert!(!originals.borrow().is_empty());
    assert!(!mail.borrow().is_empty());
    assert_eq!(result.summary.typed.runtime_fields, 46);
    drop(object_io);
    drop(objects);
    drop(store);
    let store = IncrementalPageStore::open(
        &fixture.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
        MemoryClass::Recovery,
    )
    .unwrap();
    let mut physical = work::physical_io();
    let objects =
        IncrementalObjectStore::open(&store, MemoryClass::Recovery, &mut physical, &cancelled)
            .unwrap();
    let mut object_logical = work::logical();
    let mut io = ObjectIo {
        physical: &mut physical,
        logical: &mut object_logical,
        class: MemoryClass::Recovery,
        cancelled: &cancelled,
    };
    for id in originals.borrow().values() {
        let bytes = objects.read_object_owned(id, 1 << 20, &mut io).unwrap();
        let original: ActionEnvelope = serde_cbor::from_slice(&bytes).unwrap();
        assert!(
            matches!(original.action,Action::QueryObservation{ref agent_id} if agent_id=="alice")
        );
        assert_eq!(*blake3::hash(&bytes).as_bytes(), *id);
    }
    for id in mail.borrow().values() {
        let bytes = objects.read_object_owned(id, 1 << 20, &mut io).unwrap();
        let original: oasis7_world_state::WorldEvent = serde_cbor::from_slice(&bytes).unwrap();
        assert!(world.journal().events.contains(&original));
        assert_eq!(*blake3::hash(&bytes).as_bytes(), *id);
    }
    let state = store
        .state_session_with_cancelled(work::physical_io(), MemoryClass::Recovery, &cancelled)
        .unwrap();
    let values = store.value_session_with_cancelled(work::physical_io(), &cancelled);
    let mut logical = work::logical();
    let mut read = ValueReadContext {
        source: &values,
        memory: &memory,
        class: MemoryClass::Recovery,
        io: &mut logical,
    };
    for ((agent, ordinal), id) in mail.borrow().iter() {
        let key = MailboxKeyV1::new(agent, Some(*ordinal)).unwrap();
        let value = result
            .roots
            .state_root(StateNamespaceV1::Scheduler)
            .get(key.as_bytes(), &state)
            .unwrap()
            .unwrap();
        let root = read.load_root(&value).unwrap();
        let mut out =
            [0u8; oasis7_world_state::incremental_event_fields::MAILBOX_EVENT_REFERENCE_BYTES];
        read.read_range(&root, 0, out.len() as u64, &mut &mut out[..], || false)
            .unwrap();
        assert_eq!(out, mailbox_event_ref_v1(*id));
    }
    // Cold typed projection retains the exact original input and flat ID at
    // the stable (era, ordinal) position; an action ID is not a queue key.
    drop(read);
    for (ordinal, id) in originals.borrow().iter() {
        use oasis7_world_state::{
            incremental_field_read::WorldFieldReadContextV1,
            incremental_runtime_pending_action_record::{
                PendingActionPositionV1, PendingActionReadWorkV1, recover_pending_action_record_v1,
            },
        };
        let mut recovery_keys = SemanticKeyBudget {
            bytes_remaining: 8 << 20,
        };
        let mut context = WorldFieldReadContextV1 {
            root: result.roots.state_root(StateNamespaceV1::Scheduler),
            state: &state,
            values: ValueReadContext {
                source: &values,
                memory: &memory,
                class: MemoryClass::Recovery,
                io: &mut logical,
            },
            key_work: &mut recovery_keys,
            semantic_bytes: 4096,
            record_bytes: 9 << 20,
            take_state_error: || {
                state
                    .take_read_error()
                    .map(|_| ValueTreeError::Invalid("cold pending state read"))
            },
            cancelled: || false,
        };
        let recovered = recover_pending_action_record_v1(
            PendingActionPositionV1 {
                era: 0,
                ordinal: *ordinal,
            },
            &mut context,
            &mut oasis7_formal_protocol::incremental_state::StateScanBudget {
                nodes_remaining: 1_000_000,
                read_bytes_remaining: 64 << 20,
                leaves_remaining: 100000,
            },
            &mut PendingActionReadWorkV1 {
                max_retained_trees: 4096,
                max_tree_heap: 1 << 20,
                json: oasis7_world_state::incremental_json_records::JsonReadWorkV1 {
                    cells_remaining: 100000,
                    json_nodes_remaining: 100000,
                    max_depth: 64,
                    max_payload_bytes: 1 << 20,
                },
            },
        )
        .unwrap()
        .unwrap();
        let original = recovered.value().original().unwrap();
        assert_eq!(original.id, *id);
        let bytes = objects.read_object_owned(id, 1 << 20, &mut io).unwrap();
        assert_eq!(original.byte_len, bytes.len() as u64);
        let envelope: ActionEnvelope = serde_cbor::from_slice(&bytes).unwrap();
        assert_eq!(recovered.value().envelope(), &envelope);
    }
    let key = ModuleInstanceKeyV1::new("outer-instance", ModuleInstanceFieldV1::Active).unwrap();
    assert!(
        result
            .roots
            .state_root(StateNamespaceV1::Business)
            .get(key.as_bytes(), &state)
            .unwrap()
            .is_some()
    );
    let wrong = ModuleInstanceKeyV1::new("inner-instance", ModuleInstanceFieldV1::Active).unwrap();
    assert!(
        result
            .roots
            .state_root(StateNamespaceV1::Business)
            .get(wrong.as_bytes(), &state)
            .unwrap()
            .is_none()
    );
    assert!(state.take_read_error().is_none());
}
#[test]
fn physical_genesis_rejects_foreign_pool_and_pre_cancel_before_original_callbacks() {
    let fixture = Fixture::new();
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let foreign = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let store = IncrementalPageStore::create(
        &fixture.0.join("pages"),
        memory.clone(),
        PageCompression::Raw,
    )
    .unwrap();
    let world = World::new();
    let cancelled = AtomicBool::new(false);
    let callbacks = std::cell::Cell::new(0);
    let mut logical = work::logical();
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 64 << 20,
    };
    for (pool, cancel) in [(&foreign, false), (&memory, true)] {
        cancelled.store(cancel, Ordering::Release);
        let error = prepare_runtime_genesis_index_v1(
            &world,
            &store,
            &config(),
            &mut RuntimeGenesisPhysicalContextV1 {
                memory: pool,
                values: &mut logical,
                keys: &mut keys,
                limits: limits(),
                cancelled: &cancelled,
            },
            &mut work::work(),
            work::physical(),
            &mut |_, _| {
                callbacks.set(callbacks.get() + 1);
                Ok(())
            },
            &mut |_, _| {
                callbacks.set(callbacks.get() + 1);
                Ok(OriginalArtifactRefV1 {
                    id: [0; 32],
                    byte_len: 1,
                })
            },
            &mut |_, _, _| {
                callbacks.set(callbacks.get() + 1);
                Ok([0; 32])
            },
        )
        .err()
        .unwrap();
        assert!(matches!(
            error,
            RuntimeGenesisPhysicalErrorV1::Value(
                ValueTreeError::Cancelled | ValueTreeError::Invalid(_)
            )
        ));
    }
    assert_eq!(callbacks.get(), 0);
}
