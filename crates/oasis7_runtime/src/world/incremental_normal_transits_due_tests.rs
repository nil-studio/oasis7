//! Genuinely signed normal parent + legacy World transit oracle.
//!
//! This fixture owns the complete DATA conjunction only. It does not mint a
//! durable Host lease or a completed whole-tick result.
use super::*;
use crate::world::{
    incremental_body::BodyMutationContext,
    incremental_body_event::materials::test_support::{MaterialFixtureLimitsV1, MaterialFixtureV1},
    incremental_module_execution_event::ModuleOutputEventContextV1,
    incremental_module_execution_worker::RootModuleWorkerAllocatorV1,
    incremental_module_fee_contribution::RootModuleFeeWriteContextV1,
    incremental_module_invocation::{
        RootModuleExecutionFamiliesV1, RootModuleExecutionWorkV1,
        build_accepted_invocation_directory_v1,
    },
    incremental_normal_sources::{NormalTickParentRefsV1, bind_normal_tick_parent_v1},
    incremental_normal_tail::NormalTailWorkV1,
    incremental_step::IwsStepContext,
    incremental_world_event_counter::WorldEventCounterV1,
};
use oasis7_agent_api::world_action::GeoPos;
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::{FormalBoundedContextV1, FormalBoundedWorkV1},
    incremental_execution::{ExecutionError, ExecutionTransaction, TransactionLimits},
    incremental_state::{StateNamespaceV1, StateScanBudget},
    incremental_value::{ValueIoBudget, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryBudget, MemoryClass};
use oasis7_wasm_abi::{MaterialDefaultPriority, MaterialProfileV1, MaterialTransportLossClass};
use oasis7_wasm_executor::bounded_worker::{
    BoundedWorkerSandboxV1, WorkerExecutableV1, WorkerLimitsV1,
};
use oasis7_world_state::{
    MaterialLedgerId, incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_field_read::WorldFieldReadContextV1,
    incremental_industry_jobs::IndustryFamilyReadContextV1,
    incremental_inventory_read::LedgerQuantityV1,
    incremental_logistics_records::LogisticsWorkBudgetV1,
};
use std::sync::atomic::AtomicBool;

fn lifecycle_work() -> crate::world::incremental_action_factory_lifecycle::FactoryLifecycleWorkV1 {
    use crate::world::incremental_action_factory_lifecycle::FactoryLifecycleWorkV1;
    use oasis7_world_state::{
        incremental_factory_lifecycle_index::FactoryLifecycleIndexWorkV1,
        incremental_factory_records::FactoryWorkBudgetV1,
        incremental_industry_jobs::IndustryWorkBudgetV1,
        incremental_inventory::FactoryOutputLedgerGroupWorkV1,
        incremental_json_records::{JsonReadWorkV1, JsonWriteWorkV1},
        incremental_world_native::WorldNativeSelectedReadWorkV1,
        incremental_world_state_genesis::TypedRecordWriteWorkV1,
    };
    FactoryLifecycleWorkV1 {
        steps_remaining: 1_000_000,
        bytes_remaining: 64 << 20,
        pending_entries_remaining: 100_000,
        max_scalar_bytes: 64 << 10,
        max_material_entries: 1024,
        factory: FactoryWorkBudgetV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 16 << 20,
            scan: super::genesis::scan(),
        },
        industry: IndustryWorkBudgetV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 16 << 20,
            scan: super::genesis::scan(),
        },
        lifecycle: FactoryLifecycleIndexWorkV1 {
            cells_remaining: 1_000_000,
            comparison_bytes_remaining: 64 << 20,
            pending_entries_remaining: 100_000,
            max_factories: 16,
            max_recipe_jobs: 16,
            max_text_bytes: 4096,
            max_pending_records: 128,
        },
        native_scan: super::genesis::scan(),
        native_read: WorldNativeSelectedReadWorkV1 {
            json: JsonReadWorkV1 {
                cells_remaining: 1_000_000,
                json_nodes_remaining: 1_000_000,
                max_depth: 64,
                max_payload_bytes: 16 << 20,
            },
            max_retained_trees: 4096,
            max_tree_heap: 16 << 20,
        },
        native_write: TypedRecordWriteWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 16 << 20,
            json: JsonWriteWorkV1 {
                cells_remaining: 1_000_000,
                json_nodes_remaining: 1_000_000,
                max_json_depth: 64,
                max_pending_node_bytes: 16 << 20,
            },
        },
        records_remaining: 100_000,
        output_group: FactoryOutputLedgerGroupWorkV1 {
            pending_entries_remaining: 100_000,
            max_ledger_bytes: 4096,
        },
    }
}

fn logistics_work() -> LogisticsWorkBudgetV1 {
    LogisticsWorkBudgetV1 {
        cells_remaining: 1_000_000,
        max_pending_node_bytes: 16 << 20,
        scan: super::genesis::scan(),
    }
}

fn due_world() -> World {
    let (mut world, _, _) = super::signed_world(false);
    // Consume the genuinely signed SafeAlternative command before creating the
    // old World transit. The latter is produced by the normal Action path.
    world.step().unwrap();
    let receiver =
        oasis7_world_state::agent_state::AgentState::new("receiver", GeoPos::new(2, 0, 0));
    world
        .state
        .agents
        .insert("receiver".into(), crate::AgentCell::new(receiver, 0));
    world.state.material_profiles.insert(
        "water".into(),
        MaterialProfileV1 {
            kind: "water".into(),
            tier: 1,
            category: "liquid".into(),
            stack_limit: 1000,
            transport_loss_class: MaterialTransportLossClass::Medium,
            decay_bps_per_tick: 0,
            default_priority: MaterialDefaultPriority::Standard,
        },
    );
    world.state.material_ledgers.insert(
        MaterialLedgerId::agent("agent"),
        [("water".into(), 20)].into(),
    );
    world.submit_action(crate::Action::TransferMaterial {
        requester_agent_id: "agent".into(),
        from_ledger: MaterialLedgerId::agent("agent"),
        to_ledger: MaterialLedgerId::agent("receiver"),
        kind: "water".into(),
        amount: 7,
        distance_km: 100,
        priority: None,
        route_id: None,
        route_ids: Vec::new(),
        auto_reroute: false,
    });
    world.step().unwrap();
    assert_eq!(
        world.pending_material_transits_len(),
        1,
        "legacy Action creates a real due transit"
    );
    world
}

fn scan() -> StateScanBudget {
    StateScanBudget {
        nodes_remaining: 10_000_000,
        read_bytes_remaining: 512 << 20,
        leaves_remaining: 100_000,
    }
}

fn field_read<'a>(
    roots: &'a oasis7_formal_protocol::incremental_execution::RootSetV1,
    reader: &'a super::super::fixture_store::Reader<'a>,
    io: &'a mut ValueIoBudget,
    keys: &'a mut oasis7_world_state::incremental_semantic_key::SemanticKeyBudget,
) -> WorldFieldReadContextV1<
    'a,
    super::super::fixture_store::Reader<'a>,
    super::super::fixture_store::Reader<'a>,
    impl FnMut() -> Option<ValueTreeError> + 'a,
    impl FnMut() -> bool,
> {
    WorldFieldReadContextV1 {
        root: roots.state_root(StateNamespaceV1::Business),
        state: reader,
        values: ValueReadContext {
            source: reader,
            memory: reader.memory,
            class: MemoryClass::Ordinary,
            io,
        },
        key_work: keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    }
}

#[test]
fn signed_normal_parent_executes_actual_due_material_transit_against_world_oracle() {
    let memory = MemoryBudget::new(768 << 20, 64 << 20).unwrap();
    let workspace = memory.reserve(48 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = due_world();
    let job = world
        .state
        .pending_material_transits
        .values()
        .next()
        .unwrap()
        .clone();
    let parent_time = world.state.time;
    let now = parent_time + 1;
    assert!(job.ready_at <= now);
    let before_journal = world.journal.events.len();
    let mut oracle = World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
    oracle.step().unwrap();
    assert_eq!(oracle.pending_material_transits_len(), 0);
    let expected_event = oracle.journal.events.iter().skip(before_journal).find(|event|
        matches!(&event.body, crate::WorldEventBody::Domain(crate::DomainEvent::MaterialTransitCompleted { job_id, .. }) if *job_id == job.job_id)
    ).expect("legacy World emits the completion original").clone();
    let expected_source = oracle
        .state
        .material_ledgers
        .get(&job.from_ledger)
        .and_then(|x| x.get(&job.kind))
        .copied()
        .unwrap_or(0);
    let expected_target = oracle
        .state
        .material_ledgers
        .get(&job.to_ledger)
        .and_then(|x| x.get(&job.kind))
        .copied()
        .unwrap_or(0);
    let material_limits = MaterialFixtureLimitsV1::for_state(&world.state);
    let fixture = super::fixture::build_normal_parent(&world, &memory, workspace);
    drop(world);

    let core = fixture
        .normal_core
        .as_ref()
        .expect("full normal parent retains checked Core");
    let authority = fixture
        .normal_authority
        .as_ref()
        .expect("full normal parent retains verified authority");
    let storage = fixture
        .normal_storage
        .as_ref()
        .expect("full normal parent retains checked storage");
    let cognition = fixture
        .normal_cognition
        .as_ref()
        .expect("full normal parent retains complete Cognition");
    let roots = fixture
        .normal_roots
        .as_ref()
        .expect("full normal parent retains charged roots");
    let image = super::super::fixture_worker::Image::new(&memory);
    let stop = AtomicBool::new(false);
    let reader = super::super::fixture_store::Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut original_reader = super::super::fixture_store::Originals {
        objects: fixture.objects.clone(),
        memory: &memory,
    };
    let mut transaction = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [93; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 4096,
            max_writes: 2048,
            max_facts: 64,
            max_pending_node_bytes: 32 << 20,
        },
    )
    .unwrap();
    let counter =
        WorldEventCounterV1::begin(&fixture.runtime, &fixture.head, &transaction, &memory).unwrap();
    assert_eq!(
        counter.journal_height(&transaction).unwrap(),
        before_journal as u64
    );
    let live = transaction.parent_fence();
    let mut meter = FormalBoundedWorkV1::new(40_000_000, 256 << 20).unwrap();
    let mut cancelled = || stop.load(std::sync::atomic::Ordering::Acquire);
    let mut common = FormalBoundedContextV1::new(&memory, &meter, &mut cancelled);
    let parent = bind_normal_tick_parent_v1(
        NormalTickParentRefsV1 {
            core,
            storage,
            roots,
            runtime: &fixture.runtime,
            cognition,
            head: &fixture.head,
            authority,
        },
        &mut common,
    )
    .unwrap();

    // Counted clock/input phase is established on this same transaction before
    // any normal-tail reducer is called.
    {
        let mut clock_cas = fixture.cas.clone();
        let mut clock_io = super::genesis::io();
        let mut canonical = super::genesis::canonical();
        let mut clock = BodyMutationContext {
            sink: &mut clock_cas,
            memory: &memory,
            io: &mut clock_io,
            cancelled: || false,
        };
        let guard = transaction.guard();
        let counted = IwsStepContext::begin_counted(
            &mut transaction,
            parent_time,
            &guard,
            &live,
            &reader,
            &mut clock,
            &mut canonical,
            &counter,
        )
        .unwrap();
        drop(counted);
    }

    let mut current_io = super::genesis::io();
    let mut current_keys = super::genesis::keys();
    let mut current_read = field_read(&fixture.roots, &reader, &mut current_io, &mut current_keys);
    let mut agent_io = super::genesis::io();
    let mut agent_keys = super::genesis::keys();
    let mut agent_parent_read = field_read(&fixture.roots, &reader, &mut agent_io, &mut agent_keys);
    let mut industry_io = super::genesis::io();
    let mut industry_keys = super::genesis::keys();
    let mut industry_scan = scan();
    let mut industry_read = IndustryFamilyReadContextV1 {
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut industry_io,
        },
        key_work: &mut industry_keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        max_payload_bytes: 16 << 20,
        outer_scan: &mut industry_scan,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut directory_io = super::genesis::io();
    let mut directory_keys = super::genesis::keys();
    let directory = {
        let mut dc = WorldFieldOverlayContextV1 {
            transaction: &mut transaction,
            values: ValueReadContext {
                source: &reader,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut directory_io,
            },
            key_work: &mut directory_keys,
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            take_state_error: || reader.cas.error(),
            cancelled: || false,
        };
        build_accepted_invocation_directory_v1(
            &fixture.runtime,
            &fixture.head,
            fixture.current.raw().instances(),
            fixture.current.runtime().config(),
            &reader,
            &mut dc,
            &mut super::directory_work(),
            &live,
        )
        .unwrap()
    };

    let mut overlay_io = super::genesis::io();
    let mut overlay_keys = super::genesis::keys();
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: &mut transaction,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut overlay_io,
        },
        key_work: &mut overlay_keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: super::stopped(&stop),
    };
    let mut materials = MaterialFixtureV1::build(
        &fixture.roots,
        &reader,
        &reader,
        &memory,
        material_limits,
        || reader.cas.error(),
        || false,
    );
    let mut material_context = materials.context(&fixture.roots, &reader, || reader.cas.error());
    let mut member_store = fixture.cas.clone();
    let mut member_io = super::genesis::io();
    let mut member_keys = super::genesis::keys();
    let mut members = super::genesis::member(
        &mut member_store,
        &memory,
        &mut member_io,
        &mut member_keys,
        super::stopped(&stop),
    );
    let mut fee_store = fixture.cas.clone();
    let mut fee_io = super::genesis::io();
    let mut fee_keys = super::genesis::keys();
    let mut fees = RootModuleFeeWriteContextV1 {
        values: oasis7_world_state::incremental_value_mutation::ValueMutationContextV1 {
            sink: &mut fee_store,
            memory: &memory,
            io: &mut fee_io,
            cancelled: super::stopped(&stop),
        },
        key_work: &mut fee_keys,
        member_limits: oasis7_world_state::incremental_field_member::MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 8 << 20,
            record_bytes: 9 << 20,
        },
    };
    let mut event_store = fixture.cas.clone();
    let mut event_io = super::genesis::io();
    let originals = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Vec<u8>>::new()));
    let saved = originals.clone();
    let objects = fixture.objects.clone();
    let original_memory = &memory;
    let mut events = ModuleOutputEventContextV1 {
        values: &reader,
        world_events: &counter,
        mutation: BodyMutationContext {
            sink: &mut event_store,
            memory: &memory,
            io: &mut event_io,
            cancelled: super::stopped(&stop),
        },
        persist_original:
            move |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                producer.persist_to(&mut |stream: &mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1, cancelled: &mut dyn FnMut() -> bool| {
                let len = usize::try_from(stream.original_bytes()).unwrap();
                let fee = original_memory.reserve(len + 128, MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
                let mut bytes = Vec::with_capacity(len);
                stream.write_to(&mut bytes)?;
                assert_eq!(bytes.len(), len);
                assert_eq!(*blake3::hash(&bytes).as_bytes(), stream.original_id());
                if cancelled() { return Err(ValueTreeError::Cancelled.into()); }
                assert_eq!(objects.put(&bytes), stream.original_id());
                saved.borrow_mut().push(bytes);
                drop(fee);
                Ok(())
            })
            },
        max_event_bytes: 64 << 10,
        original_work: super::genesis::canonical(),
    };
    let allocator = RootModuleWorkerAllocatorV1::new(&memory);
    let mut worker = BoundedWorkerSandboxV1 {
        executable: WorkerExecutableV1::SealedFd(&image.file),
        allocator: &allocator,
        cancelled: &stop,
        limits: WorkerLimitsV1 {
            address_space_bytes: 256 << 20,
            max_request_bytes: 1 << 20,
            max_output_bytes: 16 << 10,
            wall_time_ms: 30_000,
        },
    };
    let mut work: RootModuleExecutionWorkV1 = super::super::execution_work();
    let mut hash = super::genesis::hash_work();
    let families = RootModuleExecutionFamiliesV1 {
        roots: &fixture.roots,
        parent: &fixture.runtime,
        head: &fixture.head,
        instances: fixture.current.raw().instances(),
        config: fixture.current.runtime().config(),
        content: fixture.current.runtime().content(),
        agents: fixture.current.raw().agents(),
        authorization: fixture.current.runtime().module_authorization(),
        effects: fixture.current.runtime().effects(),
        state: fixture.current.raw().module_states(),
        agent_authority: fixture.current.native().agent_authority(),
        marketplace: fixture.current.native().market(),
    };
    let mut normal = NormalTailWorkV1::new(&mut common);
    let mut enumeration = super::directory_work();
    let mut lifecycle = lifecycle_work();
    let mut logistics = logistics_work();
    let mut live_owner = || live;
    let routed = {
        let mut target = RootModuleExecutionContextV1 {
            families,
            now,
            state: &reader,
            ctx: &mut overlay,
            members: &mut members,
            fees: &mut fees,
            events: &mut events,
            originals: &mut original_reader,
            worker: &mut worker,
            manifest_cache: &fixture.manifest_hash,
            manifest_work: &mut hash,
            work: &mut work,
            #[cfg(any(test, feature = "test-support"))]
            input_observation: None,
        };
        target
            .execute_due_material_transits_v1(
                &parent,
                &mut material_context,
                &mut current_read,
                &mut agent_parent_read,
                &mut industry_read,
                &mut lifecycle,
                &mut logistics,
                &mut normal,
                &directory,
                &mut enumeration,
                &mut live_owner,
            )
            .unwrap()
    };
    drop(overlay);
    assert_eq!(
        routed, 0,
        "the genuine parent has no subscriptions for system completion events"
    );
    assert_eq!(
        counter.journal_height(&transaction).unwrap(),
        before_journal as u64 + 1
    );
    assert_eq!(
        originals.borrow().len(),
        1,
        "one actual counted original was retained"
    );
    let expected_bytes = oasis7_wasm_abi::encode_canonical_cbor(&expected_event).unwrap();
    assert_eq!(&*originals.borrow(), &[expected_bytes]);
    assert_eq!(
        oracle
            .state
            .material_ledgers
            .get(&job.from_ledger)
            .and_then(|x| x.get(&job.kind))
            .copied(),
        Some(expected_source)
    );
    assert_eq!(
        oracle
            .state
            .material_ledgers
            .get(&job.to_ledger)
            .and_then(|x| x.get(&job.kind))
            .copied(),
        Some(expected_target)
    );
    let mut mirror = WorldFieldOverlayContextV1 {
        transaction: &mut transaction,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut *current_read.values.io,
        },
        key_work: &mut *current_read.key_work,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    assert_eq!(
        mirror.ledger(&job.from_ledger, &job.kind).unwrap(),
        LedgerQuantityV1::Value(expected_source)
    );
    assert_eq!(
        mirror.ledger(&job.to_ledger, &job.kind).unwrap(),
        LedgerQuantityV1::Value(expected_target)
    );
}
