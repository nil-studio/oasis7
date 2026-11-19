//! Genuine accepted caches built before clock contribution; ordinary complete factory phase.
use super::*;
use crate::world::incremental_factory_phase::*;
use crate::world::{
    incremental_factory_depreciation::reduce::*, incremental_factory_depreciation::*,
    incremental_factory_directory_orders::*, incremental_factory_directory_records::*,
    incremental_industrial_eligibility::incremental_industrial_eligibility_factory::IndustrialEligibilityReadWorkV1,
    incremental_industrial_eligibility::*,
};
use oasis7_world_state::{
    MaterialLedgerId, incremental_factory_records::*, incremental_industry_jobs::*,
    incremental_inventory::*,
    incremental_world_rule_composite_family::bind_current_agent_orders_v1,
    incremental_world_rule_receipts::qualify_world_rule_agent_orders_v1,
};
fn typed() -> oasis7_world_state::incremental_runtime_revocation::RevocationReadWorkV1 {
    oasis7_world_state::incremental_runtime_revocation::RevocationReadWorkV1 {
        json: oasis7_world_state::incremental_json_records::JsonReadWorkV1 {
            cells_remaining: 1000000,
            json_nodes_remaining: 1000000,
            max_depth: 64,
            max_payload_bytes: 1 << 20,
        },
        max_retained_trees: 4096,
        max_tree_heap: 1 << 20,
    }
}
fn run_phase(mode: usize) {
    let mut world = super::industrial_eligibility::producing_world();
    let old_time = world.state.time;
    assert!(world.module_registry.active.is_empty());
    assert!(world.state.module_instances.is_empty());
    let now = old_time.saturating_add(1);
    let original = world.state.factories["assembler"].clone();
    let mut a = original.clone();
    a.durability_ppm = 900_000;
    world.state.factories.insert("a-outer".into(), a);
    let mut ghost = original.clone();
    ghost.factory_id = "missing-target".into();
    ghost.spec.factory_id = "missing-target".into();
    world.state.factories.insert("ghost-outer".into(), ghost);
    world.state.materials = [
        ("legacy-zero".into(), 0),
        ("legacy-negative".into(), -3),
        ("conflict".into(), 8),
    ]
    .into();
    world.state.material_ledgers.clear();
    if mode == 4 {
        world.state.materials.clear();
    }
    if mode > 0 && mode != 4 {
        world
            .state
            .material_ledgers
            .insert(MaterialLedgerId::World, std::collections::BTreeMap::new());
    }
    if mode == 2 {
        world
            .state
            .material_ledgers
            .get_mut(&MaterialLedgerId::World)
            .unwrap()
            .extend([
                ("world-zero".into(), 0),
                ("world-negative".into(), -9),
                ("conflict".into(), -1),
            ]);
    }
    world.state.material_ledgers.insert(
        MaterialLedgerId::Agent("other".into()),
        [("unrelated".into(), -7)].into(),
    );
    if mode == 7 {
        world.submit_action(crate::Action::QueryObservation {
            agent_id: "owner".into(),
        });
    }
    world.state.time = now;
    let oracle = world.prepare_factory_depreciation_event_bodies_for_test();
    world.state.time = old_time;
    assert_eq!(oracle.len(), 3);
    let mut expected = world.state.clone();
    for body in &oracle {
        let crate::WorldEventBody::Domain(event) = body else {
            panic!("original domain depreciation")
        };
        expected.apply_domain_event(event, now).unwrap();
    }
    let expected_legacy = expected.materials.clone();
    let expected_world = expected.material_ledgers[&MaterialLedgerId::World].clone();
    let expected_factory = expected.factories["assembler"].durability_ppm;
    assert!(!expected.factories.contains_key("missing-target"));
    drop(expected);
    // The mixed sequencing fixture compares the complete legacy originals,
    // including real event IDs and the post-prologue action cause.
    let fifo_originals = if mode == 7 {
        let mut legacy = world.clone();
        legacy.state.time = now;
        for body in &oracle {
            legacy.append_event_for_test(body.clone(), None).unwrap();
        }
        let envelope = legacy.pending_actions.pop_front().unwrap();
        let body = legacy.action_to_event(&envelope).unwrap();
        legacy
            .append_event_for_test(body, Some(crate::CausedBy::Action(envelope.id)))
            .unwrap();
        legacy
            .journal
            .events
            .iter()
            .map(|event| oasis7_wasm_abi::encode_canonical_cbor(event).unwrap())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let stop = AtomicBool::new(false);
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut ri = genesis::io();
    let mut rk = genesis::keys();
    let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Authorization),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut ri,
        },
        key_work: &mut rk,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || stop.load(Ordering::Acquire),
    };
    let mut scan = genesis::scan();
    let mut order_typed = typed();
    let proof =
        qualify_world_rule_agent_orders_v1(&fixture.roots, &mut read, &mut scan, &mut order_typed)
            .unwrap();
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [91; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 4096,
            max_writes: 1024,
            max_facts: 32,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let counter =
        WorldEventCounterV1::begin(&fixture.runtime, &fixture.head, &tx, &memory).unwrap();
    let live = tx.parent_fence();
    let mut oi = genesis::io();
    let mut ok = genesis::keys();
    let mut ctx = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut oi,
        },
        key_work: &mut ok,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: stopped(&stop),
    };
    let guard = ctx.transaction.guard();
    let source = bind_current_agent_orders_v1(
        &proof,
        &fixture.roots,
        &reader,
        &mut ctx,
        &mut scan,
        &mut order_typed,
        &guard,
        &live,
    )
    .unwrap();
    let mut directory_work = FactoryOrderDirectoryWorkV1 {
        ids:
            oasis7_world_state::incremental_world_rule_composite_family::AgentOrderDirectoryWorkV1 {
                max_ids: 32,
                max_id_bytes: 256,
                comparison_bytes_remaining: 16 << 20,
            },
        pending_entries_remaining: 10000,
        merge_comparison_bytes_remaining: 16 << 20,
        max_index_text_bytes: 256,
    };
    let guard = ctx.transaction.guard();
    let accepted = build_accepted_factory_order_directory_v1(
        &fixture.runtime,
        &fixture.head,
        &source,
        &reader,
        &mut ctx,
        &mut scan,
        &mut order_typed,
        &mut directory_work,
        &guard,
        &live,
    )
    .unwrap();
    let native = fixture.current.native();
    let runtime = fixture.current.runtime();
    let work = IndustrialEligibilityWorkV1 {
        read: IndustrialEligibilityReadWorkV1 {
            cells_remaining: 1000000,
            comparison_bytes_remaining: 64 << 20,
            max_text_bytes: 4096,
        },
        typed: typed(),
        scan: genesis::scan(),
        revocation: typed(),
        pending_entries_remaining: 100000,
    };
    let mut fi = genesis::io();
    let mut fk = genesis::keys();
    let mut fs = genesis::scan();
    let mut rfi = genesis::io();
    let mut rfk = genesis::keys();
    let mut rfs = genesis::scan();
    let mut factory = FactoryFamilyReadContextV1 {
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut fi,
        },
        key_work: &mut fk,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        max_payload_bytes: 1 << 20,
        outer_scan: &mut fs,
        take_state_error: || reader.cas.error(),
        cancelled: || stop.load(Ordering::Acquire),
    };
    let mut recipe = IndustryFamilyReadContextV1 {
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut rfi,
        },
        key_work: &mut rfk,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        max_payload_bytes: 1 << 20,
        outer_scan: &mut rfs,
        take_state_error: || reader.cas.error(),
        cancelled: || stop.load(Ordering::Acquire),
    };
    let mut fw = FactoryWorkBudgetV1 {
        cells_remaining: 1000000,
        max_pending_node_bytes: 16 << 20,
        scan: genesis::scan(),
    };
    let mut rw = IndustryWorkBudgetV1 {
        cells_remaining: 1000000,
        max_pending_node_bytes: 16 << 20,
        scan: genesis::scan(),
    };
    let mut dw = FactoryRecipeDirectoryWorkV1 {
        max_factories: 8,
        max_recipe_jobs: 8,
        max_factory_id_bytes: 128,
        max_outer_changes: 16,
        comparisons_remaining: 64 << 20,
        pending_entries_remaining: 100000,
    };
    let guard = ctx.transaction.guard();
    let parent = build_accepted_factory_recipe_directory_v1(
        &fixture.runtime,
        &fixture.head,
        &fixture.roots,
        native.factory(),
        native.industry(),
        ctx.transaction,
        &mut factory,
        &mut recipe,
        &mut fw,
        &mut rw,
        &mut dw,
        &guard,
        &live,
    )
    .unwrap();
    let mut fci = genesis::io();
    let mut fck = genesis::keys();
    let mut rci = genesis::io();
    let mut rck = genesis::keys();
    let mut fc = FactoryTransitionReadContextV1 {
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut fci,
        },
        key_work: &mut fck,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        max_payload_bytes: 1 << 20,
        cancelled: || stop.load(Ordering::Acquire),
    };
    let mut rc = IndustryTransitionReadContextV1 {
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut rci,
        },
        key_work: &mut rck,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        max_payload_bytes: 1 << 20,
        cancelled: || stop.load(Ordering::Acquire),
    };
    let depreciation = FactoryDepreciationWorkV1 {
        max_factories: 32,
        max_workload_entries: 64,
        max_text_bytes: 4096,
        cells_remaining: 1000000,
        comparison_bytes_remaining: 64 << 20,
    };
    let mut enum_work = CurrentInvocationEnumerationWorkV1 {
        instance_scan: genesis::scan(),
        registry_scan: genesis::scan(),
        pending_entries: 100000,
        comparison_bytes_remaining: 64 << 20,
        max_keys: 8,
        max_key_bytes: 1024,
        max_text_bytes: 256,
    };
    let invocations = build_accepted_invocation_directory_v1(
        &fixture.runtime,
        &fixture.head,
        fixture.current.raw().instances(),
        fixture.current.runtime().config(),
        &reader,
        &mut ctx,
        &mut enum_work,
        &live,
    )
    .unwrap();
    let mut mi = genesis::io();
    let mut mk = genesis::keys();
    let mut material_read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Business),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut mi,
        },
        key_work: &mut mk,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || stop.load(Ordering::Acquire),
    };
    let mut material_family = InventoryFamilyWorkV1 {
        records_remaining: 1000000,
        index_steps_remaining: 1000000,
        max_semantic_bytes: 4096,
    };
    let mut material_scope_work = InventoryMaterialScopeWorkV1 {
        max_entries_per_map: 8,
        max_material_bytes: 128,
        pending_entries_remaining: 100000,
        comparison_bytes_remaining: 64 << 20,
    };
    let material_parent = build_parent_inventory_material_scope_v1(
        native.inventory(),
        &fixture.roots,
        &mut material_read,
        &mut genesis::scan(),
        &mut material_family,
        &mut material_scope_work,
    )
    .unwrap();
    let reduce_work = FactoryDepreciationReduceWorkV1 {
        max_material_entries: 64,
        comparison_bytes_remaining: 64 << 20,
        cells_remaining: 1000000,
    };
    let group_work = InventoryWorldGroupWorkV1 {
        steps_remaining: 10000,
    };
    let mut sink = fixture.cas.clone();
    let mut wi = genesis::io();
    let mut wk = genesis::keys();
    let mut write = oasis7_world_state::incremental_field_member::MemberRecordWriteContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut wi,
        key_work: &mut wk,
        limits: oasis7_world_state::incremental_field_member::MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 1 << 20,
            record_bytes: 16 << 20,
        },
        cancelled: stopped(&stop),
    };
    let mut fee_cas = fixture.cas.clone();
    let mut fee_io = genesis::io();
    let mut fee_keys = genesis::keys();
    let mut fees = RootModuleFeeWriteContextV1 {
        values: ValueMutationContextV1 {
            sink: &mut fee_cas,
            memory: &memory,
            io: &mut fee_io,
            cancelled: stopped(&stop),
        },
        key_work: &mut fee_keys,
        member_limits: oasis7_world_state::incremental_field_member::MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 8 << 20,
            record_bytes: 16 << 20,
        },
    };
    let mut event_cas = fixture.cas.clone();
    let mut event_io = genesis::io();
    let objects = fixture.objects.clone();
    let saved = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let saved_sink = saved.clone();
    let mut events = ModuleOutputEventContextV1 {
        values: &reader,
        world_events: &counter,
        mutation: BodyMutationContext {
            sink: &mut event_cas,
            memory: &memory,
            io: &mut event_io,
            cancelled: stopped(&stop),
        },
        persist_original:
            |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                producer.persist_to(&mut |stream:&mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1,cancelled:&mut dyn FnMut()->bool|{
            let length=usize::try_from(stream.original_bytes()).unwrap();let fee=memory.reserve(length+128,MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
            let mut bytes=Vec::with_capacity(length);stream.write_to(&mut bytes)?;if cancelled(){return Err(ValueTreeError::Cancelled.into())}
            let id=objects.put(&bytes);assert_eq!(id,stream.original_id());saved_sink.borrow_mut().push(id);drop(fee);Ok(())
        })
            },
        max_event_bytes: 64 << 10,
        original_work: genesis::canonical(),
    };
    let image = fixture_worker::Image::new(&memory);
    let allocator = RootModuleWorkerAllocatorV1::new(&memory);
    let mut worker = BoundedWorkerSandboxV1 {
        executable: WorkerExecutableV1::SealedFd(&image.file),
        allocator: &allocator,
        cancelled: &stop,
        limits: WorkerLimitsV1 {
            address_space_bytes: 256 << 20,
            max_request_bytes: 1 << 20,
            max_output_bytes: 16 << 10,
            wall_time_ms: 30000,
        },
    };
    let mut original_reader = Originals {
        objects: fixture.objects.clone(),
        memory: &memory,
    };
    let mut root_work = execution_work();
    let mut hash_work = genesis::hash_work();
    let mut live_owner = || live;

    // ALL accepted parent caches above were built on the genuine pristine
    // transaction, before clock or Input/Event/Result contributions.
    let caches = FactoryPhaseAcceptedCachesV1 {
        orders: &accepted,
        records: &parent,
        materials: &material_parent,
        invocations: &invocations,
    };
    let families = FactoryPhaseFamiliesV1 {
        orders: &proof,
        native: native.native(),
        factories: native.factory(),
        recipes: native.industry(),
        sites: native.factory_authority(),
        anchors: native.agent_authority(),
        revocation: runtime.revocation(),
        artifacts: runtime.resources(),
    };
    let mut phase_work = FactoryPhaseWorkV1 {
        orders: directory_work,
        records: dw,
        material_family,
        material_scope: material_scope_work,
        factory: fw,
        recipe: rw,
        industrial: work,
        prepare: depreciation,
        reduce: reduce_work,
        group: group_work,
        enumeration: enum_work,
        scan,
        typed: order_typed,
    };
    // No ordinary invocation fallback to accepted namespace scans.
    phase_work.enumeration.instance_scan.nodes_remaining = 0;
    phase_work.enumeration.registry_scan.nodes_remaining = 0;
    let result = {
        let mut target = RootModuleExecutionContextV1 {
            families: RootModuleExecutionFamiliesV1 {
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
            },
            now,
            state: &reader,
            ctx: &mut ctx,
            members: &mut write,
            fees: &mut fees,
            events: &mut events,
            originals: &mut original_reader,
            worker: &mut worker,
            manifest_cache: &fixture.manifest_hash,
            manifest_work: &mut hash_work,
            work: &mut root_work,
            input_observation: None,
        };
        // A caller cannot turn a same-height request into an authenticated resume,
        // even if it supplies empty phase budgets.
        if mode == 3 {
            phase_work.prepare.cells_remaining = 0;
            phase_work.orders.pending_entries_remaining = 0;
            phase_work.records.pending_entries_remaining = 0;
        }
        if mode == 4 {
            stop.store(true, Ordering::Release);
        }
        if mode == 5 {
            phase_work.prepare.cells_remaining = 0;
        }
        target.begin_current_factory_prologue_v1(
            &caches,
            &families,
            if mode == 3 { now } else { old_time },
            &mut read,
            &mut fc,
            &mut rc,
            &mut phase_work,
            &mut live_owner,
        )
    };
    if mode == 3 || mode == 4 || mode == 5 {
        assert!(result.is_err());
        let guard = ctx.transaction.guard();
        assert!(tx.seal(&guard, &live, 1).is_err());
        return;
    }
    let outcome = result.unwrap();
    assert_eq!(outcome.prepared_events, oracle.len());
    assert_eq!(outcome.published_and_routed_events, oracle.len());
    assert_eq!(saved.borrow().len(), oracle.len());
    if mode == 7 {
        // Use the already-built genuine parent material carrier and the same
        // continuous scope/group work consumed by the prologue. Never re-Q GEN.
        let mut policy_material_write_keys = genesis::keys();
        let mut normalization_cells = 1_000_000;
        let mut normalization_bytes = 64 << 20;
        let mut materials =
            crate::world::incremental_body_event::materials::CorePolicyMaterialContextV1 {
                parent: &material_parent,
                roots: &fixture.roots,
                pages: &reader,
                read_io: &mut *material_read.values.io,
                read_keys: &mut *material_read.key_work,
                write_keys: &mut policy_material_write_keys,
                semantic_bytes: 4096,
                record_bytes: 16 << 20,
                write_limits: oasis7_world_state::incremental_field_member::MemberRecordLimitsV1 {
                    semantic_bytes: 4096,
                    payload_bytes: 1 << 20,
                    record_bytes: 16 << 20,
                },
                take_state_error: || reader.cas.error(),
                family_work: &mut phase_work.material_family,
                scope_work: &mut phase_work.material_scope,
                group_work: &mut phase_work.group,
                normalization_max_union_entries: 16,
                normalization_cells_remaining: &mut normalization_cells,
                normalization_comparison_bytes_remaining: &mut normalization_bytes,
            };
        let mut pending_work = genesis::current_work(&fixture.roots).runtime.pending;
        let guard = ctx.transaction.guard();
        let mut pending = oasis7_world_state::incremental_pending_action_overlay::begin_pending_action_current_v1(
            fixture.current.runtime().pending(), &fixture.roots, ctx.transaction, &memory,
            &guard, &live, &mut pending_work.index, || false,
        ).unwrap();
        let mut admission_scan = genesis::scan();
        let mut admission_work = genesis::current_work(&fixture.roots).runtime.revocation;
        let guard = ctx.transaction.guard();
        let current = oasis7_world_state::incremental_runtime_revocation::begin_runtime_revocation_current_v1(
            fixture.current.runtime().revocation(), &mut ctx, &mut read,
            &mut admission_scan, &mut admission_work, &mut write, &guard, &live,
        ).unwrap();
        let mut business_read =
            oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
                root: fixture.roots.state_root(StateNamespaceV1::Business),
                state: &reader,
                values: ValueReadContext {
                    source: &reader,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *fc.values.io,
                },
                key_work: &mut *fc.key_work,
                semantic_bytes: 4096,
                record_bytes: 16 << 20,
                take_state_error: || reader.cas.error(),
                cancelled: || false,
            };
        let mut parent_read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
            root: fixture.roots.state_root(StateNamespaceV1::Business),
            state: &reader,
            values: ValueReadContext {
                source: &reader,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut *rc.values.io,
            },
            key_work: &mut *rc.key_work,
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            take_state_error: || reader.cas.error(),
            cancelled: || false,
        };
        let mut lifecycle_work =
            oasis7_world_state::incremental_factory_lifecycle_index::FactoryLifecycleIndexWorkV1 {
                cells_remaining: 100_000,
                comparison_bytes_remaining: 64 << 20,
                pending_entries_remaining: 100_000,
                max_factories: 8,
                max_recipe_jobs: 8,
                max_text_bytes: 4096,
                max_pending_records: 32,
            };
        let guard = ctx.transaction.guard();
        let lifecycle = oasis7_world_state::incremental_factory_lifecycle_index::observe_current_factory_lifecycle_v1(
            "assembler", fixture.current.native().factory_lifecycle_index(),
            fixture.current.native().factory(), fixture.current.native().industry(),
            &fixture.roots, ctx.transaction, &mut business_read, &mut phase_work.factory,
            &mut phase_work.recipe, &mut lifecycle_work, 1 << 20, &guard, &live,
        ).unwrap();
        assert!(lifecycle.factory_exists());
        assert_eq!(lifecycle.active_jobs(), 0);
        assert_eq!(lifecycle.completed_jobs_without_factory().unwrap(), 0);
        assert!(!lifecycle.stable_without_factory());
        lifecycle.validate(ctx.transaction, &memory, &live).unwrap();
        drop(lifecycle);
        let mut body_work = crate::world::incremental_action_body::BodyActionWorkV1 {
            steps_remaining: 10_000,
            bytes_remaining: 16 << 20,
            pending_entries_remaining: 100_000,
        };
        let mut audit = action_body::audit_work();
        let mut tail = crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1 {
            originals_remaining: 8,
            original_bytes_remaining: 1 << 20,
            decode: action_body::audit_work(),
        };
        let mut rule = RootRulePhaseWorkV1 {
            enumeration: action_body::rules::directory_work(),
            payload: oasis7_world_state::incremental_rule_decision_decode::RuleDecisionPayloadDecodeWorkV1 {
                structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 { bytes_remaining: 1 << 20, values_remaining: 100_000, max_depth: 64 },
                max_payload_bytes: 64 << 10, max_heap_bytes: 128 << 20,
            },
            canonical: genesis::canonical(), max_captured_outputs: 8, max_payload_bytes: 64 << 10,
            merge_units_remaining: 1_000_000_000,
        };
        let mut target = RootModuleExecutionContextV1 {
            families: RootModuleExecutionFamiliesV1 {
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
            },
            now,
            state: &reader,
            ctx: &mut ctx,
            members: &mut write,
            fees: &mut fees,
            events: &mut events,
            originals: &mut original_reader,
            worker: &mut worker,
            manifest_cache: &fixture.manifest_hash,
            manifest_work: &mut hash_work,
            work: &mut root_work,
            input_observation: None,
        };
        for expected_some in [true, false] {
            let result = target
                .execute_next_nonmovement_v1(
                    &outcome,
                    &mut pending,
                    &mut pending_work,
                    fixture.current.runtime().revocation(),
                    &current,
                    &mut read,
                    &mut admission_scan,
                    &mut admission_work,
                    &mut materials,
                    &mut business_read,
                    &mut parent_read,
                    crate::world::incremental_module_invocation::NonmovementSelectedWorkV1::Body(
                        &mut body_work,
                    ),
                    &mut audit,
                    &invocations,
                    &mut rule,
                    &mut phase_work.enumeration,
                    &mut tail,
                    &mut live_owner,
                )
                .unwrap();
            assert_eq!(result.is_some(), expected_some);
        }
        assert_eq!(saved.borrow().len(), fifo_originals.len());
        for (id, expected) in saved.borrow().iter().zip(&fifo_originals) {
            assert_eq!(fixture.objects.original(id), *expected);
            assert_eq!(*id, *blake3::hash(expected).as_bytes());
        }
        assert_eq!(counter.journal_height(target.ctx.transaction).unwrap(), 4);
        drop(target);
        drop(materials);
        let guard = ctx.transaction.guard();
        let material_current = open_current_inventory_material_scope_v1(
            &material_parent,
            &fixture.roots,
            &reader,
            &mut ctx,
            &mut phase_work.material_family,
            &mut phase_work.material_scope,
            &guard,
            &live,
        )
        .unwrap();
        assert_eq!(
            material_current
                .legacy_materials()
                .map(|entry| (entry.material().to_owned(), entry.quantity()))
                .collect::<std::collections::BTreeMap<_, _>>(),
            expected_legacy
        );
        assert_eq!(
            material_current
                .world_materials()
                .map(|entry| (entry.material().to_owned(), entry.quantity()))
                .collect::<std::collections::BTreeMap<_, _>>(),
            expected_world
        );
        drop(material_current);
        let guard = ctx.transaction.guard();
        let step = crate::world::incremental_step::IwsStepContext::borrow_counted_at(
            ctx.transaction,
            now,
            &guard,
            &live,
            &reader,
            &mut events.mutation,
            &mut events.original_work,
            &counter,
        )
        .unwrap();
        assert_eq!(step.time(), now);
        drop(step);
        let guard = ctx.transaction.guard();
        let sealed = tx.seal(&guard, &live, fixture.roots.height + 1).unwrap();
        assert_eq!(
            sealed
                .record
                .facts
                .iter()
                .filter(|fact| fact.kind == ExecutionFactKindV1::Input)
                .count(),
            1
        );
        assert_eq!(
            sealed
                .record
                .facts
                .iter()
                .filter(|fact| fact.kind == ExecutionFactKindV1::Event)
                .count(),
            4
        );
        assert_eq!(
            sealed
                .record
                .facts
                .iter()
                .map(|fact| fact.kind)
                .collect::<Vec<_>>(),
            vec![
                ExecutionFactKindV1::Event,
                ExecutionFactKindV1::Event,
                ExecutionFactKindV1::Event,
                ExecutionFactKindV1::Input,
                ExecutionFactKindV1::Event,
            ]
        );
        return;
    }
    if mode == 6 {
        // A second prologue in the same sole transaction is rejected, even
        // when another Root context and fresh guard are supplied.
        let second = {
            let mut target = RootModuleExecutionContextV1 {
                families: RootModuleExecutionFamiliesV1 {
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
                },
                now,
                state: &reader,
                ctx: &mut ctx,
                members: &mut write,
                fees: &mut fees,
                events: &mut events,
                originals: &mut original_reader,
                worker: &mut worker,
                manifest_cache: &fixture.manifest_hash,
                manifest_work: &mut hash_work,
                work: &mut root_work,
                input_observation: None,
            };

            target.begin_current_factory_prologue_v1(
                &caches,
                &families,
                old_time,
                &mut read,
                &mut fc,
                &mut rc,
                &mut phase_work,
                &mut live_owner,
            )
        };
        assert!(second.is_err());
        assert_eq!(saved.borrow().len(), oracle.len());
        let guard = ctx.transaction.guard();
        assert!(tx.seal(&guard, &live, 1).is_err());
        return;
    }

    let guard = ctx.transaction.guard();
    let materials = open_current_inventory_material_scope_v1(
        &material_parent,
        &fixture.roots,
        &reader,
        &mut ctx,
        &mut phase_work.material_family,
        &mut phase_work.material_scope,
        &guard,
        &live,
    )
    .unwrap();
    let actual_legacy: std::collections::BTreeMap<_, _> = materials
        .legacy_materials()
        .map(|v| (v.material().to_owned(), v.quantity()))
        .collect();
    let actual_world: std::collections::BTreeMap<_, _> = materials
        .world_materials()
        .map(|v| (v.material().to_owned(), v.quantity()))
        .collect();
    assert_eq!(actual_legacy, expected_legacy);
    assert_eq!(actual_world, expected_world);
    assert_eq!(
        read_factory_cell_overlay_v1(
            FactoryRecordKeyV1::Factory("assembler"),
            FactoryCellV1::Column(FactoryColumnV1::FactoryDurabilityPpm),
            &mut ctx,
            8,
            &mut std::io::sink()
        )
        .unwrap(),
        FactoryCellReadV1::Value(FactoryValueV1::Signed(expected_factory))
    );
}
#[test]
fn real_factory_phase_uses_accepted_caches_current_deltas_and_original_world_oracle() {
    for mode in 0..7 {
        eprintln!("counted factory prologue actual mode={mode}");
        run_phase(mode)
    }
}

#[test]
fn completed_factory_prologue_gates_fifo_and_preserves_current_and_legacy_originals() {
    run_phase(7)
}
