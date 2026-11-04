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
#[path = "incremental_world_current_provenance_tests.rs"]
pub(super) mod protocol;
pub(super) struct Binder<'a> {
    events: &'a BTreeMap<[u8; 32], crate::WorldEvent>,
    parent: &'a protocol::Frame,
    head: &'a protocol::Frame,
    catalog: &'a Reader<'a>,
    pub(super) calls: usize,
    core: oasis7_formal_protocol::incremental_material::CoreBudget,
    facts: oasis7_formal_protocol::incremental_material::FactBudget,
    structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1,
}
impl<'a> Binder<'a> {
    pub(super) fn new(
        events: &'a BTreeMap<[u8; 32], crate::WorldEvent>,
        parent: &'a protocol::Frame,
        head: &'a protocol::Frame,
        catalog: &'a Reader<'a>,
    ) -> Self {
        Self {
            events,
            parent,
            head,
            catalog,
            calls: 0,
            core: oasis7_formal_protocol::incremental_material::CoreBudget {
                objects_remaining: 1024,
                original_bytes_remaining: 64 << 20,
                new_pages_remaining: 8192,
                merkle_reads_remaining: 1_000_000,
            },
            facts: protocol::fact_work(),
            structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
                bytes_remaining: 32 << 20,
                values_remaining: 1_000_000,
                max_depth: 64,
            },
        }
    }
}
impl<O: oasis7_formal_protocol::incremental_material::OriginalObjectStreamReader>
    MailboxWorldEventOriginalReaderV1<O> for Binder<'_>
{
    fn checked_world_event(
        &mut self,
        reference: oasis7_world_state::incremental_agent_subtree::AgentMailboxReferenceV1<'_>,
        scope: &MailboxProvenanceScopeV1<'_>,
        objects: &mut O,
        memory: &MemoryBudget,
        class: MemoryClass,
        work: &mut MailboxWorldEventOriginalWorkV1,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<
        Arc<ReservedValue<CheckedMailboxWorldEventOriginalV1>>,
        MailboxWorldEventOriginalErrorV1<O::Error>,
    > {
        use oasis7_formal_protocol::incremental_material::*;
        use oasis7_world_state::incremental_world_event_decode::*;
        self.calls += 1;
        let raw = objects
            .read_original(
                &reference.original,
                usize::try_from(work.max_original_bytes).map_err(|_| {
                    MailboxWorldEventOriginalErrorV1::Invalid("mailbox original bound")
                })?,
                memory,
            )
            .map_err(MailboxWorldEventOriginalErrorV1::Source)?;
        let zero = oasis7_wasm_abi::CanonicalCborWorkV1 {
            visits_remaining: 0,
            bytes_remaining: 0,
            max_depth: work.canonical.max_depth,
            max_map_keys: work.canonical.max_map_keys,
            max_key_bytes: work.canonical.max_key_bytes,
        };
        let mut decode_work = WorldEventDecodeWorkV1 {
            structure: std::mem::replace(
                &mut self.structure,
                oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
                    bytes_remaining: 0,
                    values_remaining: 0,
                    max_depth: 64,
                },
            ),
            canonical: std::mem::replace(&mut work.canonical, zero),
            max_original_bytes: usize::try_from(work.max_original_bytes).unwrap(),
            max_heap_bytes: 32 << 20,
        };
        let decoded =
            decode_world_event_original_v1(&raw, memory, class, &mut decode_work, &mut *cancelled);
        work.canonical = decode_work.canonical;
        self.structure = decode_work.structure;
        let decoded = decoded.map_err(MailboxWorldEventOriginalErrorV1::Value)?;
        assert_eq!(Some(decoded.event()), self.events.get(&reference.original));
        let occurrence = if let MailboxProvenanceScopeV1::Candidate {
            candidate,
            core,
            originals,
        } = scope
        {
            if let Some(ordinal) = core.facts().facts.iter().position(|fact| {
                fact.kind == ExecutionFactKindV1::Event && fact.id == reference.original
            }) {
                MailboxWorldEventOccurrenceV1::Candidate(
                    check_candidate_event_fact_v1(
                        &CandidateEventFactRequestV1 {
                            candidate,
                            core,
                            originals,
                            ordinal: ordinal as u64,
                        },
                        memory,
                        &mut *cancelled,
                    )
                    .map_err(|_| {
                        MailboxWorldEventOriginalErrorV1::Invalid(
                            "actual candidate Event occurrence",
                        )
                    })?,
                )
            } else {
                let parent = self.parent;
                MailboxWorldEventOccurrenceV1::Accepted(self.historical(
                    reference.original,
                    &parent.accepted,
                    &parent.roots,
                    objects,
                    memory,
                    class,
                    &mut *cancelled,
                )?)
            }
        } else if let MailboxProvenanceScopeV1::Accepted { head, roots } = scope {
            MailboxWorldEventOccurrenceV1::Accepted(self.historical(
                reference.original,
                head,
                roots,
                objects,
                memory,
                class,
                &mut *cancelled,
            )?)
        } else {
            unreachable!()
        };
        let _arc = memory
            .reserve(64, class)
            .map_err(|e| MailboxWorldEventOriginalErrorV1::Value(ValueTreeError::Memory(e)))?;
        Ok(Arc::new(verify_mailbox_world_event_original_v1(
            Arc::new(decoded),
            occurrence,
            scope,
            memory,
            class,
            work,
            cancelled,
        )?))
    }
}
impl Binder<'_> {
    fn historical<O: oasis7_formal_protocol::incremental_material::OriginalObjectStreamReader>(
        &mut self,
        id: [u8; 32],
        head: &oasis7_formal_protocol::controlled_authority::formal::VerifiedFormalCommitV1,
        roots: &RootSetV1,
        objects: &mut O,
        memory: &MemoryBudget,
        class: MemoryClass,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<
        ReservedValue<oasis7_formal_protocol::incremental_material::VerifiedEventFactOccurrenceV1>,
        MailboxWorldEventOriginalErrorV1<O::Error>,
    > {
        use oasis7_formal_protocol::incremental_material::*;
        let location = [self.parent, self.head]
            .into_iter()
            .find_map(|frame| {
                frame
                    .core
                    .facts()
                    .facts
                    .iter()
                    .position(|fact| fact.kind == ExecutionFactKindV1::Event && fact.id == id)
                    .map(|ordinal| EventFactLocationV1 {
                        height: frame.roots.height,
                        ordinal: ordinal as u64,
                        original: id,
                    })
            })
            .ok_or(MailboxWorldEventOriginalErrorV1::Invalid(
                "actual Event locator missing",
            ))?;
        verify_event_fact_occurrence_v1(
            &EventFactRequestV1 {
                head,
                roots,
                location,
            },
            &mut EventFactReadContextV1 {
                originals: objects,
                catalog_pages: self.catalog,
                memory,
                class,
                core_budget: &mut self.core,
                fact_budget: &mut self.facts,
                cancelled,
            },
        )
        .map_err(|_| MailboxWorldEventOriginalErrorV1::Invalid("actual accepted Event history"))
    }
}

fn raw_gen(
    world: &World,
    pages: &mut Pages,
    memory: &MemoryBudget,
    events: &mut BTreeMap<[u8; 32], crate::WorldEvent>,
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
                let original=world.journal.events.iter().find(|original|matches!(&original.body,crate::WorldEventBody::Domain(domain) if domain==event)).expect("actual mailbox WorldEvent original");
                let bytes = oasis7_wasm_abi::encode_canonical_cbor(original).unwrap();
                let id = *blake3::hash(&bytes).as_bytes();
                originals.insert(
                    id,
                    memory
                        .reserve(bytes.len() + 128, MemoryClass::Ordinary)
                        .unwrap()
                        .retain(bytes),
                );
                events.insert(id, original.clone());
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
    world.journal.events.push(crate::WorldEvent {
        id: 17,
        time: 0,
        caused_by: Some(oasis7_world_state::CausedBy::Action(7)),
        body: crate::WorldEventBody::Domain(event.clone()),
    });
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
    let installed = install(&memory, &mut pages, &rows);
    let genesis_frame = protocol::genesis(
        &memory,
        &mut pages,
        installed.state,
        events.keys().copied(),
        &mut raw_originals,
    );
    let roots = genesis_frame.roots.clone();
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
        let mut binder = Binder::new(&events, &genesis_frame, &genesis_frame, &reader);
        let result = qualify_world_raw_complete_v1(
            &roots,
            &genesis_frame.candidate_scope(),
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
    let second = world
        .state
        .agents
        .get("alice")
        .unwrap()
        .mailbox
        .back()
        .unwrap()
        .clone();
    world.journal.events.push(crate::WorldEvent {
        id: 18,
        time: 0,
        caused_by: Some(oasis7_world_state::CausedBy::Action(8)),
        body: crate::WorldEventBody::Domain(second),
    });
    let after_rows = raw_gen(&world, &mut pages, &memory, &mut events, &mut raw_originals);
    let parent = genesis_frame.parent();
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
        let input = b"actual raw ordinary Input";
        tx.append_fact(ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Input,
            id: *blake3::hash(input).as_bytes(),
        })
        .unwrap();
        for id in events.keys().filter(|id| {
            !genesis_frame
                .core
                .facts()
                .facts
                .iter()
                .any(|fact| fact.id == **id)
        }) {
            tx.append_fact(ExecutionFactRefV1 {
                kind: ExecutionFactKindV1::Event,
                id: *id,
            })
            .unwrap();
        }
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
    let input = b"actual raw ordinary Input";
    raw_originals.insert(
        *blake3::hash(input).as_bytes(),
        memory
            .reserve(input.len() + 128, MemoryClass::Ordinary)
            .unwrap()
            .retain(input.to_vec()),
    );
    let next = protocol::normal(
        &memory,
        &mut pages,
        &genesis_frame,
        &sealed.record,
        *blake3::hash(input).as_bytes(),
        &mut raw_originals,
    );
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
    let mut binder = Binder::new(&events, &genesis_frame, &next, &reader);
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
        &next.candidate_scope(),
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
    let mut rec_binder = Binder::new(&events, &genesis_frame, &next, &rec_reader);
    let recovered = qualify_world_raw_complete_v1(
        &sealed.record.after,
        &next.accepted_scope(),
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
        &next.accepted_scope(),
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
