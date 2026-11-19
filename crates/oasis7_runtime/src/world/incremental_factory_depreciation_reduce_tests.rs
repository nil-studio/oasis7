//! Complete real batch reduction against the original WorldState event reducer.
use super::*;
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
fn run_reduce(mode: usize) {
    let mut world = super::industrial_eligibility::producing_world();
    let now = world.state.time;
    assert!(world.module_registry.active.is_empty());
    assert!(world.state.module_instances.is_empty());
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
    let oracle = world.prepare_factory_depreciation_event_bodies_for_test();
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
    let guard = ctx.transaction.guard();
    let directory = open_current_factory_order_directory_v1(
        &accepted,
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
    let sources = IndustrialEligibilitySourcesV1 {
        roots: &fixture.roots,
        orders: &directory,
        agents: fixture.current.raw().agents(),
        native: native.native(),
        factories: native.factory(),
        sites: native.factory_authority(),
        anchors: native.agent_authority(),
        revocation: runtime.revocation(),
        artifacts: runtime.resources(),
    };

    let mut work = IndustrialEligibilityWorkV1 {
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
        max_factories: 32,
        max_recipe_jobs: 32,
        max_factory_id_bytes: 256,
        max_outer_changes: 64,
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
    let guard = ctx.transaction.guard();
    let records = open_current_factory_recipe_directory_v1(
        &parent,
        native.factory(),
        native.industry(),
        ctx.transaction,
        &mut fc,
        &mut rc,
        &mut fw,
        &mut rw,
        &mut dw,
        &guard,
        &live,
    )
    .unwrap();
    let mut depreciation = FactoryDepreciationWorkV1 {
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
    let guard = ctx.transaction.guard();
    let batch = prepare_current_factory_depreciation_v1(
        &records,
        &sources,
        now,
        false,
        &reader,
        &mut ctx,
        &mut read,
        &mut fc,
        &mut rc,
        &mut fw,
        &mut rw,
        &mut dw,
        &mut work,
        &mut depreciation,
        &guard,
        &live,
    )
    .unwrap();

    assert_eq!(batch.bodies(), oracle.as_slice());
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
    let guard = ctx.transaction.guard();
    let mut reducer =
        start_factory_depreciation_reducer_v1(&batch, &mut ctx, now, &guard, &live).unwrap();
    if mode == 3 {
        let guard = ctx.transaction.guard();
        assert!(
            start_factory_depreciation_reducer_v1(&batch, &mut ctx, now, &guard, &live).is_err()
        );
        let guard = ctx.transaction.guard();
        assert!(tx.seal(&guard, &live, 1).is_err());
        return;
    }
    let mut reduce_work = FactoryDepreciationReduceWorkV1 {
        max_material_entries: 64,
        comparison_bytes_remaining: 64 << 20,
        cells_remaining: 1000000,
    };
    let mut group_work = InventoryWorldGroupWorkV1 {
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
                if mode == 5 {
                    return Err(crate::world::incremental_body::BodyMutationError::Invalid(
                        "injected actual original sink failure",
                    ));
                }
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
    for index in 0..batch.bodies().len() {
        let guard = ctx.transaction.guard();
        let materials = open_current_inventory_material_scope_v1(
            &material_parent,
            &fixture.roots,
            &reader,
            &mut ctx,
            &mut material_family,
            &mut material_scope_work,
            &guard,
            &live,
        )
        .unwrap();
        let guard = ctx.transaction.guard();
        let applied = reducer
            .apply_next_v1(
                &materials,
                &mut ctx,
                &mut write,
                &mut fw,
                &mut group_work,
                &mut reduce_work,
                &guard,
                &live,
            )
            .unwrap()
            .unwrap();
        assert_eq!(applied.ordinal(), index);
        if mode == 7 {
            let guard = ctx.transaction.guard();
            assert!(
                reducer
                    .apply_next_v1(
                        &materials,
                        &mut ctx,
                        &mut write,
                        &mut fw,
                        &mut group_work,
                        &mut reduce_work,
                        &guard,
                        &live
                    )
                    .is_err()
            );
            assert_eq!(reducer.applied_items(), 0);
            assert!(saved.borrow().is_empty());
            let guard = ctx.transaction.guard();
            assert!(tx.seal(&guard, &live, 1).is_err());
            return;
        }
        if mode == 6 {
            enum_work.pending_entries = 0;
        }

        let publication = {
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
            target.publish_route_depreciation_item_v1(
                &mut reducer,
                applied,
                &invocations,
                &mut enum_work,
                &mut live_owner,
            )
        };
        if mode == 5 || mode == 6 {
            assert!(publication.is_err());
            assert_eq!(saved.borrow().len(), usize::from(mode == 6));
            assert_eq!(reducer.applied_items(), 0);
            let guard = ctx.transaction.guard();
            assert!(tx.seal(&guard, &live, 1).is_err());
            return;
        }
        let published = publication.unwrap();
        assert_eq!(&published.event().body, &oracle[index]);
        assert_eq!(published.event().caused_by, None);
        assert_eq!(reducer.applied_items(), index + 1);
    }
    let guard = ctx.transaction.guard();
    let materials = open_current_inventory_material_scope_v1(
        &material_parent,
        &fixture.roots,
        &reader,
        &mut ctx,
        &mut material_family,
        &mut material_scope_work,
        &guard,
        &live,
    )
    .unwrap();
    assert!(materials.world_ledger_present());
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
    assert_eq!(reducer.remaining_items(), 0);
    let guard = ctx.transaction.guard();
    assert!(
        reducer
            .apply_next_v1(
                &materials,
                &mut ctx,
                &mut write,
                &mut fw,
                &mut group_work,
                &mut reduce_work,
                &guard,
                &live
            )
            .unwrap()
            .is_none()
    );
    assert_eq!(saved.borrow().len(), oracle.len());
    assert_eq!(
        ctx.ledger(&MaterialLedgerId::Agent("other".into()), "unrelated")
            .unwrap(),
        oasis7_world_state::incremental_inventory_read::LedgerQuantityV1::Value(-7)
    );
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
    assert_eq!(
        read_factory_cell_overlay_v1(
            FactoryRecordKeyV1::Factory("missing-target"),
            FactoryCellV1::Record,
            &mut ctx,
            8,
            &mut std::io::sink()
        )
        .unwrap(),
        FactoryCellReadV1::RecordAbsent
    );
    if mode == 0 {
        // Another consumer cannot restart this already consumed batch.
        let guard = ctx.transaction.guard();
        assert!(
            start_factory_depreciation_reducer_v1(&batch, &mut ctx, now, &guard, &live).is_err()
        );
    } else {
        if mode == 2 {
            reduce_work.cells_remaining = 0;
        } else {
            stop.store(true, Ordering::Release);
        }
        let guard = ctx.transaction.guard();
        assert!(
            reducer
                .apply_next_v1(
                    &materials,
                    &mut ctx,
                    &mut write,
                    &mut fw,
                    &mut group_work,
                    &mut reduce_work,
                    &guard,
                    &live
                )
                .is_err()
        );
    }
    let guard = ctx.transaction.guard();
    assert!(tx.seal(&guard, &live, 1).is_err());
}
#[test]
fn prepared_factory_reducer_matches_original_world_material_maps_and_missing_factory() {
    for mode in 0..8 {
        run_reduce(mode);
    }
}
