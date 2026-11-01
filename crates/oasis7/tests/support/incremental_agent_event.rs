use oasis7::AgentState;
use oasis7_agent_api::world_action::{BodyKernelView, GeoPos};
use oasis7_distfs::incremental_page_store::{
    IncrementalPageStore, PageIoBudget, PageStoreError,
    objects::{IncrementalObjectStore, ObjectIo, ObjectStreamWriteError},
};
use oasis7_formal_protocol::{
    incremental_execution::{
        AuthorityBinding, ExecutionParent, ExecutionTransaction, PreparedTransition, RootSetV1,
        TransactionLimits,
    },
    incremental_state::{StateDeltaV1, StateNamespaceV1 as Ns, StateWriteV1},
    incremental_value::{ValueIoBudget, ValueStreamWriter},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_runtime::{
    AgentCell, AgentEventContext, AgentEventOriginalStreamV1, AgentEventOriginalV1,
    BodyMutationContext, BodyMutationError, CausedBy, DomainEvent, World, WorldEvent,
    WorldEventBody, WorldState,
    incremental_agent_genesis::AgentGenesisContext,
    incremental_event_fields::{EVENT_SEQUENCE_KEY_V1, EventSequenceV1},
};
use std::{collections::BTreeMap, io::Write, sync::atomic::AtomicBool};

pub fn physical() -> PageIoBudget {
    PageIoBudget {
        reads_remaining: 100_000,
        read_bytes_remaining: 128 << 20,
        writes_remaining: 100_000,
        write_bytes_remaining: 128 << 20,
    }
}
pub fn logical() -> ValueIoBudget {
    ValueIoBudget {
        nodes_remaining: 100_000,
        read_bytes_remaining: 128 << 20,
        write_bytes_remaining: 128 << 20,
    }
}
pub fn original_work() -> oasis7_wasm_abi::CanonicalCborWorkV1 {
    oasis7_wasm_abi::CanonicalCborWorkV1 {
        visits_remaining: 1_000_000,
        bytes_remaining: 16 << 20,
        max_depth: 32,
        max_map_keys: 64,
        max_key_bytes: 1024,
    }
}
pub fn persist_original(
    original: &mut dyn AgentEventOriginalV1,
    objects: &IncrementalObjectStore<'_>,
    io: &mut ObjectIo<'_>,
) -> Result<(), BodyMutationError> {
    original.persist_to(&mut |stream: &mut dyn AgentEventOriginalStreamV1,
                              cancelled: &mut dyn FnMut() -> bool| {
        let id = stream.original_id();
        let length = stream.original_bytes();
        objects
            .write_object_from_stream(length, id, io, cancelled, |output| stream.write_to(output))
            .map(|_| ())
            .map_err(|error| match error {
                ObjectStreamWriteError::Producer(error) => error,
                ObjectStreamWriteError::Store(PageStoreError::Memory(error)) => {
                    oasis7_formal_protocol::incremental_execution::ExecutionError::Memory(error)
                        .into()
                }
                ObjectStreamWriteError::Store(PageStoreError::ResourceLimited(label)) => {
                    oasis7_formal_protocol::incremental_execution::ExecutionError::ResourceLimited(
                        label,
                    )
                    .into()
                }
                ObjectStreamWriteError::Store(PageStoreError::Cancelled) => {
                    oasis7_formal_protocol::incremental_value::ValueTreeError::Cancelled.into()
                }
                ObjectStreamWriteError::Store(error) => {
                    BodyMutationError::Original(Box::new(error))
                }
            })
    })
}

pub struct Directory(pub std::path::PathBuf);
impl Directory {
    pub fn new() -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("oasis7-agent-event-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

pub fn fixture(store: &IncrementalPageStore, memory: &MemoryBudget) -> (World, ExecutionParent) {
    let mut state = WorldState {
        time: 11,
        ..WorldState::default()
    };
    let mut agent = AgentState::new("alice", GeoPos::new(0, 0, 0));
    agent.body_view = BodyKernelView {
        mass_kg: 10,
        radius_cm: 10,
        thrust_limit: 10,
        cross_section_cm2: 10,
    };
    state
        .agents
        .insert("alice".into(), AgentCell::new(agent, 3));
    let world = World::new_with_state(state);
    let mut fields = BTreeMap::<u8, Vec<StateWriteV1>>::new();
    let mut sink = store.value_session(physical());
    world
        .write_iws_agent_genesis_fields(
            "alice",
            &mut AgentGenesisContext {
                mutation: BodyMutationContext {
                    sink: &mut sink,
                    memory,
                    io: &mut logical(),
                    cancelled: || false,
                },
                emit: |ns, key: &[u8], id| {
                    fields.entry(ns as u8).or_default().push(StateWriteV1 {
                        key: key.to_vec(),
                        before: None,
                        after: Some(id),
                    });
                    Ok(())
                },
                mailbox_original: |_: u64, _: &DomainEvent| panic!("fixture mailbox is empty"),
                max_scalar_bytes: 4096,
            },
        )
        .unwrap();
    world
        .write_iws_clock_genesis(
            &mut BodyMutationContext {
                sink: &mut sink,
                memory,
                io: &mut logical(),
                cancelled: || false,
            },
            |key, id| {
                fields
                    .entry(Ns::Allocator as u8)
                    .or_default()
                    .push(StateWriteV1 {
                        key: key.to_vec(),
                        before: None,
                        after: Some(id),
                    });
                Ok(())
            },
        )
        .unwrap();
    {
        let (key, bytes) = (
            EVENT_SEQUENCE_KEY_V1,
            EventSequenceV1 { next: 1, era: 0 }
                .canonical_bytes()
                .to_vec(),
        );
        let mut quota = logical();
        let mut writer =
            ValueStreamWriter::new(&mut sink, memory, &mut quota, bytes.len() as u64, || false)
                .unwrap();
        writer.write_all(&bytes).unwrap();
        let id = writer.finish().unwrap().digest().unwrap();
        fields
            .entry(Ns::Allocator as u8)
            .or_default()
            .push(StateWriteV1 {
                key: key.to_vec(),
                before: None,
                after: Some(id),
            });
    }
    let mut parent = ExecutionParent {
        commit: [1; 32],
        roots: RootSetV1::empty("agent-event-physical".into(), [2; 32]).unwrap(),
        authority: AuthorityBinding {
            token: [3; 32],
            epoch: 0,
        },
    };
    let source = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let cancelled = AtomicBool::new(false);
    for (namespace, mut writes) in fields {
        // Genesis emits in field traversal order; StateDelta requires unique
        // canonical key order. This bounded fixture is the batch owner.
        writes.sort_unstable_by(|a, b| a.key.cmp(&b.key));
        assert!(writes.windows(2).all(|pair| pair[0].key != pair[1].key));
        let index = namespace as usize - 1;
        let delta =
            StateDeltaV1::build(parent.roots.state[index].clone(), writes, &source, 4 << 20)
                .unwrap();
        parent.roots.state[index] = delta.after;
        for node in delta.new_nodes {
            let id = hex::decode(node.digest).unwrap().try_into().unwrap();
            let charge = memory
                .reserve(node.bytes.len(), MemoryClass::Ordinary)
                .unwrap();
            store
                .put_page(id, &charge.retain(node.bytes), &mut physical(), &cancelled)
                .unwrap();
        }
    }
    drop(IncrementalObjectStore::create(store, &mut physical(), &cancelled).unwrap());
    (world, parent)
}

pub fn event(world: &World, id: u64, movement: bool) -> WorldEvent {
    WorldEvent {
        id,
        time: 11,
        caused_by: Some(CausedBy::Action(7)),
        body: WorldEventBody::Domain(if movement {
            DomainEvent::AgentMoved {
                agent_id: "alice".into(),
                from: GeoPos::new(999, 999, 999),
                to: if id == 1 {
                    GeoPos::new(3, 4, 5)
                } else {
                    GeoPos::new(0, 7, 8)
                },
            }
        } else {
            let mut view = world.state().agents["alice"].state.body_view.clone();
            view.mass_kg += id;
            DomainEvent::BodyAttributesUpdated {
                agent_id: "alice".into(),
                view,
                reason: "physical body event".into(),
            }
        }),
    }
}

pub fn contribute(
    world: &World,
    event: &WorldEvent,
    parent: ExecutionParent,
    store: &IncrementalPageStore,
    memory: &MemoryBudget,
) -> ReservedValue<PreparedTransition> {
    let source = store
        .state_session(physical(), MemoryClass::Ordinary)
        .unwrap();
    let mut sink = store.value_session(physical());
    let values = store.value_session(physical());
    let cancelled = AtomicBool::new(false);
    let mut physical_io = physical();
    let objects =
        IncrementalObjectStore::open(store, MemoryClass::Ordinary, &mut physical_io, &cancelled)
            .unwrap();
    let live = parent.fence().unwrap();
    let mut txn = ExecutionTransaction::begin(
        parent,
        [4; 32],
        &source,
        memory,
        TransactionLimits {
            max_reads: 16,
            max_writes: 16,
            max_facts: 16,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    world
        .contribute_iws_agent_event(
            event,
            &mut txn,
            &mut AgentEventContext {
                values: &values,
                mutation: BodyMutationContext {
                    sink: &mut sink,
                    memory,
                    io: &mut logical(),
                    cancelled: || false,
                },
                persist_original: |original: &mut dyn AgentEventOriginalV1| {
                    persist_original(
                        original,
                        &objects,
                        &mut ObjectIo {
                            physical: &mut physical_io,
                            logical: &mut logical(),
                            class: MemoryClass::Ordinary,
                            cancelled: &cancelled,
                        },
                    )
                },
                max_event_bytes: 1 << 20,
                original_work: original_work(),
            },
        )
        .unwrap();
    let guard = txn.guard();
    txn.seal(&guard, &live, 0).unwrap()
}
