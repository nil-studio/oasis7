//! Actual Runtime raw GEN, mixed raw transaction, cold qualification and admission.
use super::complete::{Originals, Reader, context, io};
use super::*;
use oasis7_formal_protocol::{
    incremental_execution::*,
    incremental_schedule::ScheduleScanBudget,
    incremental_scratch_counter::{CounterScratchStore, ScratchCounterDelta},
    incremental_state::*,
};
use oasis7_world_state::{
    AgentCell, AgentState, ModuleInstanceState,
    incremental_agent_subtree::{AgentSubtreeScanWorkV1, AgentSubtreeTransitionWorkV1},
    incremental_module_instance_recovery::InstanceScratchBudget,
    incremental_pending_action_original_binding::PendingActionOriginalWorkV1,
    incremental_runtime_allocators_family::RuntimeAllocatorsWorkV1,
    incremental_world_raw_complete::*,
};
use std::{cell::RefCell, sync::Arc};
fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 10_000_000,
        read_bytes_remaining: 1 << 30,
        leaves_remaining: 1_000_000,
    }
}
pub(super) fn raw_work() -> WorldRawCompleteWorkV1 {
    WorldRawCompleteWorkV1 {
        state_scan: scan(),
        agents: AgentSubtreeScanWorkV1 {
            max_agents: 16,
            cells_remaining: 1_000_000,
            max_scalar_bytes: 1 << 20,
        },
        agent_changes: AgentSubtreeTransitionWorkV1 {
            cells_remaining: 1_000_000,
            max_scalar_bytes: 1 << 20,
        },
        module_states: ModuleStateContentBudgetV1 {
            states_remaining: 1000,
            logical_bytes_remaining: 16 << 20,
            max_state_bytes: 1 << 20,
        },
        instance_scratch: InstanceScratchBudget {
            reads: 1_000_000,
            read_bytes: 64 << 20,
            writes: 1_000_000,
            write_bytes: 64 << 20,
        },
        instance_relations_remaining: 1_000_000,
        max_instance_scalar_bytes: 1 << 20,
        allocators: RuntimeAllocatorsWorkV1 {
            records_remaining: 1_000_000,
        },
        schedule: WorldRawScheduleWorkV1 {
            cells_remaining: 1_000_000,
            index: ScheduleScanBudget {
                nodes_remaining: 1_000_000,
                read_bytes_remaining: 64 << 20,
                buckets_remaining: 1000,
                instances_remaining: 1000,
            },
        },
        mailbox_occurrences_remaining: 1000,
        originals: PendingActionOriginalWorkV1 {
            objects_remaining: 1000,
            original_bytes_remaining: 64 << 20,
            max_original_bytes: 1 << 20,
            canonical: oasis7_wasm_abi::CanonicalCborWorkV1 {
                visits_remaining: 1_000_000,
                bytes_remaining: 64 << 20,
                max_depth: 128,
                max_map_keys: 1024,
                max_key_bytes: 4096,
            },
        },
    }
}
pub(super) struct ScratchNode(ReservedValue<Vec<u8>>);
impl AsRef<[u8]> for ScratchNode {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
pub(super) struct Scratch<'a> {
    memory: &'a MemoryBudget,
    class: MemoryClass,
    pages: RefCell<BTreeMap<String, ReservedValue<Vec<u8>>>>,
    _metadata: oasis7_proto::memory_budget::MemoryReservation,
}
impl<'a> Scratch<'a> {
    pub(super) fn new(memory: &'a MemoryBudget, class: MemoryClass) -> Self {
        Self {
            memory,
            class,
            pages: RefCell::new(BTreeMap::new()),
            _metadata: memory.reserve(4096 * 256 + 512, class).unwrap(),
        }
    }
}
impl MerkleNodeReader for Scratch<'_> {
    type Node = ScratchNode;
    fn allocation_budget(&self) -> Option<&MemoryBudget> {
        Some(self.memory)
    }
    fn allocation_class(&self) -> Option<MemoryClass> {
        Some(self.class)
    }
    fn read_node(&self, id: &str, max: usize) -> Result<Option<ScratchNode>, String> {
        let pages = self.pages.borrow();
        let Some(bytes) = pages.get(id) else {
            return Ok(None);
        };
        if bytes.len() > max {
            return Err("scratch page bound".into());
        }
        Ok(Some(ScratchNode(
            self.memory
                .reserve(bytes.len() + 128, self.class)
                .map_err(|e| e.to_string())?
                .retain(bytes.to_vec()),
        )))
    }
}
impl CounterScratchStore for Scratch<'_> {
    fn put_scratch_pages(&self, delta: &ReservedValue<ScratchCounterDelta>) -> Result<(), String> {
        if !delta.uses_budget(self.memory) {
            return Err("scratch pool".into());
        }
        delta.verify_pages()?;
        for page in &delta.new_nodes {
            let mut pages = self.pages.borrow_mut();
            if pages.len() == 4096 && !pages.contains_key(&page.digest) {
                return Err("scratch finite pages".into());
            }
            let copy = self
                .memory
                .reserve(page.bytes.len() + 128, self.class)
                .map_err(|e| e.to_string())?
                .retain(page.bytes.clone());
            pages.insert(page.digest.clone(), copy);
        }
        Ok(())
    }
}
pub(super) struct Binder<'a> {
    pub(super) events: &'a BTreeMap<[u8; 32], oasis7_world_state::DomainEvent>,
    pub(super) calls: usize,
}
impl<O: oasis7_formal_protocol::incremental_material::OriginalObjectStreamReader>
    MailboxDomainEventOriginalReaderV1<O> for Binder<'_>
{
    fn checked_domain_event(
        &mut self,
        reference: oasis7_world_state::incremental_agent_subtree::AgentMailboxReferenceV1<'_>,
        objects: &mut O,
        memory: &MemoryBudget,
        class: MemoryClass,
        work: &mut DomainEventOriginalWorkV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<
        Arc<ReservedValue<CheckedDomainEventOriginalV1>>,
        DomainEventOriginalErrorV1<O::Error>,
    > {
        self.calls += 1;
        let event =
            self.events
                .get(&reference.original)
                .ok_or(DomainEventOriginalErrorV1::Invalid(
                    "missing actual borrowed DomainEvent",
                ))?;
        Ok(Arc::new(verify_domain_event_original_v1(
            event,
            reference.original,
            objects,
            memory,
            class,
            work,
            cancelled,
        )?))
    }
}
fn raw_gen(
    world: &World,
    pages: &mut Pages,
    memory: &MemoryBudget,
    events: &mut BTreeMap<[u8; 32], oasis7_world_state::DomainEvent>,
    originals: &mut BTreeMap<[u8; 32], ReservedValue<Vec<u8>>>,
) -> Vec<ReservedValue<ExecutionWriteV1>> {
    let mut quota = io();
    let mut budget = RuntimeRawGenesisWorkV1 {
        writes_remaining: 1000,
        agents_remaining: 100,
        instances_remaining: 100,
        instance_count_rows_remaining: 100,
        schedule_entries_remaining: 100,
        module_states: ModuleStateContentBudgetV1 {
            states_remaining: 100,
            logical_bytes_remaining: 1 << 20,
            max_state_bytes: 1 << 20,
        },
        max_scalar_bytes: 1 << 20,
    };
    let mut rows = Vec::with_capacity(1024);
    world
        .prepare_iws_runtime_raw_genesis_v1(
            &mut crate::world::incremental_body::BodyMutationContext {
                sink: pages,
                memory,
                io: &mut quota,
                cancelled: || false,
            },
            &mut budget,
            &mut |_, _, event| {
                let bytes = oasis7_wasm_abi::encode_canonical_cbor(event).unwrap();
                let id = *blake3::hash(&bytes).as_bytes();
                originals.insert(
                    id,
                    memory
                        .reserve(bytes.len() + 128, MemoryClass::Ordinary)
                        .unwrap()
                        .retain(bytes),
                );
                events.insert(id, event.clone());
                Ok(id)
            },
            &mut |row| {
                assert!(rows.len() < 1024);
                rows.push(row);
                Ok(())
            },
        )
        .unwrap();
    rows
}
fn install(
    memory: &MemoryBudget,
    pages: &mut Pages,
    rows: &[ReservedValue<ExecutionWriteV1>],
) -> RootSetV1 {
    let mut roots = RootSetV1::empty("raw-current-complete".into(), [71; 32]).unwrap();
    for root in &mut roots.state {
        let count = rows
            .iter()
            .filter(|r| r.namespace == root.namespace)
            .count();
        let _phase = memory
            .reserve((count + 1) * (512 << 10), MemoryClass::Ordinary)
            .unwrap();
        let mut writes: Vec<_> = rows
            .iter()
            .filter(|r| r.namespace == root.namespace)
            .map(|r| r.mutation.clone())
            .collect();
        writes.sort_by(|a, b| a.key.cmp(&b.key));
        let reader = Reader {
            pages,
            memory,
            class: MemoryClass::Ordinary,
        };
        let delta = StateDeltaV1::build(root.clone(), writes, &reader, 8 << 20).unwrap();
        for node in delta.new_nodes {
            pages
                .put_node(
                    *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                    memory
                        .reserve(node.bytes.len(), MemoryClass::Ordinary)
                        .unwrap()
                        .retain(node.bytes),
                )
                .unwrap();
        }
        *root = delta.after;
    }
    roots
}
#[test]
fn runtime_raw_complete_nonempty_genesis_mixed_wake_state_instance_mailbox_and_readmission() {
    let memory = MemoryBudget::new(768 << 20, 64 << 20).unwrap();
    let _fixture = memory.reserve(128 << 20, MemoryClass::Ordinary).unwrap();
    let mut pages = Pages::default();
    let mut world = World::new();
    world.state.agents.clear();
    world.state.module_instances.clear();
    world.state.module_states.clear();
    world.module_tick_schedule.clear();
    let event = oasis7_world_state::DomainEvent::ActionAccepted {
        action_id: 7,
        action_kind: "raw-mailbox".into(),
        actor_id: "alice".into(),
        eta_ticks: 0,
        notes: vec![],
    };
    let mut agent = AgentCell::new(
        AgentState::new(
            "alice",
            oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
        ),
        0,
    );
    agent.mailbox.push_back(event);
    world.state.agents.insert("alice".into(), agent);
    world.state.module_instances.insert(
        "module#1".into(),
        ModuleInstanceState {
            instance_id: "module#1".into(),
            module_id: "module".into(),
            module_version: "1".into(),
            wasm_hash: "borrowed-hash".into(),
            owner_agent_id: "alice".into(),
            install_target:
                oasis7_agent_api::world_action::ModuleInstallTarget::LocationInfrastructure {
                    location_id: "place".into(),
                },
            active: false,
            installed_at: 0,
        },
    );
    world
        .state
        .module_states
        .insert("module".into(), vec![0, 1, 255]);
    world.module_tick_schedule.insert("module#1".into(), 5);
    let mut events = BTreeMap::new();
    let mut raw_originals = BTreeMap::new();
    let rows = raw_gen(&world, &mut pages, &memory, &mut events, &mut raw_originals);
    let roots = install(&memory, &mut pages, &rows);
    let mut keys = SemanticKeyBudget {
        bytes_remaining: 128 << 20,
    };
    let mut quota = io();
    let mut budget = raw_work();
    let proof = {
        let reader = Reader {
            pages: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
        };
        let scratch = Scratch::new(&memory, MemoryClass::Ordinary);
        let mut objects = Originals {
            values: &raw_originals,
            memory: &memory,
            class: MemoryClass::Ordinary,
            calls: 0,
        };
        let mut binder = Binder {
            events: &events,
            calls: 0,
        };
        let result = qualify_world_raw_complete_v1(
            &roots,
            &mut context(&roots, &reader, &mut quota, &mut keys),
            &mut budget,
            &scratch,
            &mut objects,
            &mut binder,
        )
        .unwrap();
        assert_eq!(binder.calls, 1);
        assert_eq!(objects.calls, 1);
        assert_eq!(result.mailbox_reference_count(), 1);
        assert_eq!(result.schedule().entries(), 1);
        result
    };
    world
        .state
        .module_states
        .insert("module".into(), vec![0, 2, 255, 0]);
    world
        .state
        .module_instances
        .get_mut("module#1")
        .unwrap()
        .active = true;
    world.module_tick_schedule.insert("module#1".into(), 9);
    world.next_action_id += 1;
    world
        .state
        .agents
        .get_mut("alice")
        .unwrap()
        .mailbox
        .push_back(oasis7_world_state::DomainEvent::ActionAccepted {
            action_id: 8,
            action_kind: "raw-mailbox-next".into(),
            actor_id: "alice".into(),
            eta_ticks: 0,
            notes: vec![],
        });
    let after_rows = raw_gen(&world, &mut pages, &memory, &mut events, &mut raw_originals);
    let parent = ExecutionParent {
        roots: roots.clone(),
        commit: [51; 32],
        authority: AuthorityBinding {
            token: [52; 32],
            epoch: 3,
        },
    };
    let live = parent.fence().unwrap();
    let sealed = {
        let reader = Reader {
            pages: &pages,
            memory: &memory,
            class: MemoryClass::Ordinary,
        };
        let mut tx = ExecutionTransaction::begin(
            parent.clone(),
            [53; 32],
            &reader,
            &memory,
            TransactionLimits {
                max_reads: 1000,
                max_writes: 128,
                max_facts: 8,
                max_pending_node_bytes: 8 << 20,
            },
        )
        .unwrap();
        let mut count = 0;
        for row in &after_rows {
            if rows
                .iter()
                .find(|old| old.namespace == row.namespace && old.mutation.key == row.mutation.key)
                .and_then(|old| old.mutation.after)
                != row.mutation.after
            {
                tx.write(row.namespace, &row.mutation.key, row.mutation.after)
                    .unwrap();
                count += 1;
            }
        }
        assert!(count >= 6);
        let guard = tx.guard();
        tx.seal(&guard, &live, 1).unwrap()
    };
    for node in &sealed.new_nodes {
        pages
            .put_node(
                *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                memory
                    .reserve(node.bytes.len(), MemoryClass::Ordinary)
                    .unwrap()
                    .retain(node.bytes.clone()),
            )
            .unwrap();
    }
    let reader = Reader {
        pages: &pages,
        memory: &memory,
        class: MemoryClass::Ordinary,
    };
    let scratch = Scratch::new(&memory, MemoryClass::Ordinary);
    let mut objects = Originals {
        values: &raw_originals,
        memory: &memory,
        class: MemoryClass::Ordinary,
        calls: 0,
    };
    let mut binder = Binder {
        events: &events,
        calls: 0,
    };
    let verifier = TransitionVerifier {
        parent: &parent,
        operation: [53; 32],
        source: &reader,
        memory: &memory,
        class: MemoryClass::Ordinary,
        max_pending_node_bytes: 8 << 20,
    };
    let after = accept_world_raw_complete_transition_v1(
        &proof,
        &verifier,
        &sealed.record,
        &mut context(&sealed.record.after, &reader, &mut quota, &mut keys),
        &mut budget,
        &scratch,
        &mut objects,
        &mut binder,
    )
    .unwrap();
    after.validate_binding(&sealed.record.after).unwrap();
    assert_eq!(after.mailbox_reference_count(), 2);
    assert_eq!(binder.calls, 1, "unchanged old mailbox original inherited");
    assert_eq!(objects.calls, 1);
    assert_eq!(after.schedule().entries(), 1);
    let rec_reader = Reader {
        pages: &pages,
        memory: &memory,
        class: MemoryClass::Recovery,
    };
    let rec_scratch = Scratch::new(&memory, MemoryClass::Recovery);
    let mut rec_objects = Originals {
        values: &raw_originals,
        memory: &memory,
        class: MemoryClass::Recovery,
        calls: 0,
    };
    let mut rec_binder = Binder {
        events: &events,
        calls: 0,
    };
    let recovered = qualify_world_raw_complete_v1(
        &sealed.record.after,
        &mut context(&sealed.record.after, &rec_reader, &mut quota, &mut keys),
        &mut budget,
        &rec_scratch,
        &mut rec_objects,
        &mut rec_binder,
    )
    .unwrap();
    assert_eq!(rec_binder.calls, 2);
    assert_eq!(recovered.allocation_class(), MemoryClass::Recovery);
    let used = memory.used();
    let admitted = readmit_world_raw_complete_ordinary_v1(
        &recovered,
        &sealed.record.after,
        &memory,
        &mut budget,
        || false,
    )
    .unwrap();
    admitted.validate_binding(&sealed.record.after).unwrap();
    assert_eq!(admitted.allocation_class(), MemoryClass::Ordinary);
    drop(admitted);
    assert_eq!(memory.used(), used);
}
