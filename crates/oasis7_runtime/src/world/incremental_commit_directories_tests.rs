//! Genuine accepted caches built before clock contribution; ordinary complete factory phase.
use super::*;
use crate::world::incremental_commit::*;
use crate::world::incremental_directory_bundle::*;
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
fn run_commit_directories(mode: usize) {
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

    use oasis7_formal_protocol::controlled_authority::formal::test_fixtures::incremental;
    // The occurrence's fact_body_id is the containing CommitBody, not the
    // Result original. Recreate GEN with the actual authenticated capsule.
    let capsule = fixture.runtime.manifest().canonical_bytes().unwrap();
    let capsule_id = *blake3::hash(&capsule).as_bytes();
    assert_eq!(fixture.objects.original(&capsule_id), capsule);
    assert_eq!(
        fixture.runtime.occurrence().fact_body_id(),
        fixture.parent.commit
    );
    let parent_protocol = incremental::setup_with_config_state_and_facts(
        memory.clone(),
        incremental::genesis_config([4; 32]),
        fixture.roots.state.clone(),
        incremental::FixtureExecutionArtifactRefsV1 {
            manifest: [5; 32],
            wasm: [6; 32],
            rules: [7; 32],
            policy: [8; 32],
        },
        &[ExecutionFactRefV1 {
            kind: ExecutionFactKindV1::Result,
            id: capsule_id,
        }],
    );
    assert_eq!(parent_protocol.roots, fixture.roots);
    assert_eq!(
        parent_protocol.history.body.digest().unwrap(),
        fixture.parent.commit
    );
    let (_core_objects, parent_catalog) =
        incremental::core_objects(&parent_protocol, &parent_protocol.history);
    let request = CommitPreparationRequest {
        parent: ParentCommitView {
            config: &parent_protocol.config,
            roots: &fixture.roots,
            body: &parent_protocol.history.body,
            history: &parent_protocol.history.prefix,
            catalog: &parent_catalog.after,
            authority: fixture.parent.authority,
        },
        operation: [129; 32],
        next_tick: now,
        execution_environment: parent_protocol.config.execution_environment,
        checkpoint: false,
        limits: TransactionLimits {
            max_reads: 4096,
            max_writes: 1024,
            max_facts: 32,
            max_pending_node_bytes: 16 << 20,
        },
    };
    let live = fixture.parent.fence().unwrap();
    let applied = std::cell::Cell::new(false);
    // Expand the shared real phase fixture separately for each transaction.
    // A single conditional reference would unify their invariant source
    // lifetimes and incorrectly require the captured fixture to be static.
    macro_rules! prepare_bundle {
        ($transaction:expr,$identity_work:expr) => {{
            let tx = $transaction;
        let counter =
            WorldEventCounterV1::begin(&fixture.runtime, &fixture.head, &tx, &memory).unwrap();
        let live = tx.parent_fence();
        let mut oi = genesis::io();
        let mut ok = genesis::keys();
        let mut ctx = WorldFieldOverlayContextV1 {
            transaction: tx,
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
                max_ids: 8,
                max_id_bytes: 128,
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
        let mut material_read =
            oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
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
        let inventory_identity_work=&mut *$identity_work;
        let inventory_directory=build_inventory_identity_directory_v1(native.inventory(),&fixture.roots,&mut material_read,
            &mut genesis::scan(),&mut material_family,inventory_identity_work).unwrap();
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

            let outcome = target
                .begin_current_factory_prologue_v1(
                    &caches,
                    &families,
                    old_time,
                    &mut read,
                    &mut fc,
                    &mut rc,
                    &mut phase_work,
                    &mut live_owner,
                )
                .unwrap();
            let frozen = target
                .freeze_current_runtime_directories_v1(
                    &caches,
                    &families,
                    &mut fc,
                    &mut rc,
                    &mut phase_work,
                    &inventory_directory,native.inventory(),inventory_identity_work,
                    &mut live_owner,
                )
                .unwrap();
            (outcome, frozen)
        };

        let (outcome, frozen) = result;

        assert_eq!(outcome.published_and_routed_events, oracle.len());
        assert_eq!(
            frozen.iter_factory_ids().collect::<Vec<_>>(),
            vec!["a-outer", "assembler", "ghost-outer"]
        );
        assert_eq!(
            frozen
                .iter_order_indices()
                .map(|(id, _)| id)
                .collect::<Vec<_>>(),
            vec!["order"]
        );
        assert_eq!(frozen.iter_recipe_job_ids().len(), 0);

        if mode == 1 {
            let guard = ctx.transaction.guard();
            let _step = crate::world::incremental_step::IwsStepContext::begin_counted(
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
        }
        Ok(frozen)
        }};
    }
    let apply = |actual_tx: &mut ExecutionTransaction<'_, _>,
                 inventory_identity_work: &mut InventoryIdentityDirectoryWorkV1|
     -> Result<
        ReservedValue<FrozenRuntimeDirectoryBundleV1>,
        DirectoryCommitTestError,
    > {
        applied.set(true);
        if mode == 2 {
            let mut foreign = ExecutionTransaction::begin(
                fixture.parent.clone(),
                [130; 32],
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
            prepare_bundle!(&mut foreign, inventory_identity_work)
        } else {
            prepare_bundle!(actual_tx, inventory_identity_work)
        }
    };
    let another_pool = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let actual_memory = if mode == 3 { &another_pool } else { &memory };
    let mut material_rebind = InventoryMaterialScopeWorkV1 {
        max_entries_per_map: 8,
        max_material_bytes: 128,
        pending_entries_remaining: 100000,
        comparison_bytes_remaining: 64 << 20,
    };
    let mut inventory_identity_work = InventoryIdentityDirectoryWorkV1 {
        visits_remaining: 1000000,
        comparison_bytes_remaining: 64 << 20,
        pending_entries_remaining: 100000,
        max_identity_bytes: 4096,
        max_output_entries: 10000,
    };
    let prepared = if mode == 4 {
        prepare_iws_runtime_commit_with_results_and_directories_v1(
            &request,
            &reader,
            actual_memory,
            || Ok::<_, DirectoryCommitTestError>(live),
            apply,
            &mut material_rebind,
            &mut inventory_identity_work,
            || false,
            |after| {
                assert_eq!(after.simulation_tick, now);
                Err(DirectoryCommitTestError::ResultFailure)
            },
        )
    } else {
        prepare_iws_runtime_commit_with_directories_v1(
            &request,
            &reader,
            actual_memory,
            || Ok::<_, DirectoryCommitTestError>(live),
            apply,
            &mut material_rebind,
            &mut inventory_identity_work,
            || mode == 5,
        )
    };
    match mode {
        0 => {
            let prepared = prepared.unwrap();
            let bound = prepared.require_directories_v1().unwrap();
            bound.validate_candidate(&prepared, &memory).unwrap();
            bound
                .sealed_directories()
                .inventory_identity_parent()
                .validate_binding(&prepared.transition().record.after)
                .unwrap();
            assert_eq!(
                bound.candidate_commit(),
                prepared.history().body.digest().unwrap()
            );
            assert_eq!(
                bound.after_roots_digest(),
                prepared.transition().record.after.digest().unwrap()
            );
            assert_eq!(prepared.commit_id(), bound.candidate_commit());
            bound
                .sealed_directories()
                .validate_after_roots(&prepared.transition().record.after, &memory)
                .unwrap();
            // Provisional structural after-root readback, not accepted/durable promotion.
            let mut cas = fixture.cas.clone();
            for node in &prepared.transition().new_nodes {
                cas.put_node(
                    *blake3::Hash::from_hex(&node.digest).unwrap().as_bytes(),
                    memory
                        .reserve(node.bytes.len() + 128, MemoryClass::Ordinary)
                        .unwrap()
                        .retain(node.bytes.clone()),
                )
                .unwrap();
            }
            let after = prepared.transition().record.after.clone();
            let next_parent = ExecutionParent {
                roots: after.clone(),
                commit: prepared.commit_id(),
                authority: fixture.parent.authority,
            };
            let mut next = ExecutionTransaction::begin(
                next_parent,
                [131; 32],
                &reader,
                &memory,
                request.limits,
            )
            .unwrap();
            let next_live = next.parent_fence();
            let mut ni = genesis::io();
            let mut nk = genesis::keys();
            let mut nc = WorldFieldOverlayContextV1 {
                transaction: &mut next,
                values: ValueReadContext {
                    source: &reader,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut ni,
                },
                key_work: &mut nk,
                semantic_bytes: 4096,
                record_bytes: 16 << 20,
                take_state_error: || reader.cas.error(),
                cancelled: || false,
            };
            let mut family_work = InventoryFamilyWorkV1 {
                records_remaining: 100000,
                index_steps_remaining: 100000,
                max_semantic_bytes: 4096,
            };
            let guard = nc.transaction.guard();
            let material_source = open_current_inventory_material_scope_v1(
                bound.sealed_directories().material_parent(),
                &after,
                &reader,
                &mut nc,
                &mut family_work,
                &mut material_rebind,
                &guard,
                &next_live,
            )
            .unwrap();
            let legacy: std::collections::BTreeMap<_, _> = material_source
                .legacy_materials()
                .map(|v| (v.material().to_owned(), v.quantity()))
                .collect();
            let world: std::collections::BTreeMap<_, _> = material_source
                .world_materials()
                .map(|v| (v.material().to_owned(), v.quantity()))
                .collect();
            assert_eq!(legacy, expected_legacy);
            assert_eq!(world, expected_world);
            assert_eq!(
                read_factory_cell_overlay_v1(
                    FactoryRecordKeyV1::Factory("assembler"),
                    FactoryCellV1::Column(FactoryColumnV1::FactoryDurabilityPpm),
                    &mut nc,
                    8,
                    &mut std::io::sink()
                )
                .unwrap(),
                FactoryCellReadV1::Value(FactoryValueV1::Signed(expected_factory))
            );
            let old = prepare_iws_runtime_commit_v1(
                &request,
                &reader,
                &memory,
                || Ok::<_, DirectoryCommitTestError>(live),
                |_| Ok(()),
            )
            .unwrap();
            assert!(old.require_directories_v1().is_err());
        }
        1 | 2 => assert!(matches!(
            prepared,
            Err(DirectoryCommitTestError::Freeze(
                RuntimeDirectoryFreezeErrorV1::Execution(ExecutionError::StaleGuard)
            ))
        )),
        3 => {
            assert!(!applied.get());
            assert!(matches!(
                prepared,
                Err(DirectoryCommitTestError::Execution(
                    ExecutionError::Invalid("driver source memory pool")
                ))
            ));
        }
        4 => assert!(matches!(
            prepared,
            Err(DirectoryCommitTestError::ResultFailure)
        )),
        5 => assert!(prepared.is_err()),
        _ => unreachable!(),
    }
}
#[derive(Debug)]
enum DirectoryCommitTestError {
    Execution(ExecutionError),
    Freeze(RuntimeDirectoryFreezeErrorV1),
    ResultFailure,
}
impl From<ExecutionError> for DirectoryCommitTestError {
    fn from(e: ExecutionError) -> Self {
        Self::Execution(e)
    }
}
impl From<RuntimeDirectoryFreezeErrorV1> for DirectoryCommitTestError {
    fn from(e: RuntimeDirectoryFreezeErrorV1) -> Self {
        Self::Freeze(e)
    }
}
#[test]
fn real_commit_apply_seal_history_candidate_directory_binding_and_failure_boundaries() {
    for mode in 0..6 {
        run_commit_directories(mode)
    }
}
