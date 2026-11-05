fn stage_directory_work()
-> crate::world::incremental_module_invocation::CurrentInvocationEnumerationWorkV1 {
    let scan = || StateScanBudget {
        nodes_remaining: 10_000_000,
        read_bytes_remaining: 512 << 20,
        leaves_remaining: 100_000,
    };
    crate::world::incremental_module_invocation::CurrentInvocationEnumerationWorkV1 {
        instance_scan: scan(),
        registry_scan: scan(),
        pending_entries: 10000,
        comparison_bytes_remaining: 4_000_000_000,
        max_keys: 256,
        max_key_bytes: 1024,
        max_text_bytes: 1024,
    }
}
use super::*;
#[test]
#[cfg(feature = "wasmtime")]
fn actual_pre_action_selected_invocation_uses_sealed_vm_after_world_drop() {
    run_pre_action_fixture(false);
}

#[test]
#[cfg(all(
    feature = "wasmtime",
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn actual_pre_action_regex_uses_same_sealed_worker_and_preserves_legacy_input() {
    run_pre_action_fixture(true);
}

#[cfg(feature = "wasmtime")]
fn run_pre_action_fixture(with_regex: bool) {
    run_action_stage_fixture(with_regex, false, false);
}

#[test]
#[cfg(feature = "wasmtime")]
fn actual_post_action_uses_current_directory_and_preserves_legacy_input() {
    run_action_stage_fixture(false, true, false);
}

#[cfg(feature = "wasmtime")]
fn run_action_stage_fixture(with_regex: bool, post: bool, event_route: bool) {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    fn pre_action_world(with_regex: bool, post: bool, event_route: bool) -> World {
        let mut world = initialized_world();
        let record = world
            .module_registry
            .records
            .get_mut(&crate::ModuleRegistry::record_key("main", "1"))
            .unwrap();
        record.manifest.subscriptions[0].stage = Some(if post {
            ModuleSubscriptionStage::PostAction
        } else {
            ModuleSubscriptionStage::PreAction
        });
        record.manifest.subscriptions[0].action_kinds = vec!["action.move_agent".into()];
        if with_regex {
            record.manifest.subscriptions[0].filters = Some(serde_json::json!({
                "action": [{"path": "/action/data/agent_id", "re": "^alice$"}]
            }));
        }
        if event_route {
            record.manifest.subscriptions[0].stage = Some(ModuleSubscriptionStage::PostEvent);
            record.manifest.subscriptions[0].action_kinds.clear();
            record.manifest.subscriptions[0].event_kinds = vec!["*".into()];
        }
        world.submit_action(crate::Action::MoveAgent {
            agent_id: "alice".into(),
            to: oasis7_agent_api::world_action::GeoPos::new(1, 2, 3),
        });
        world
    }
    // The legacy oracle is independently initialized: a failed legacy route
    // may append journal events and must never contaminate the GEN source.
    let mut oracle = pre_action_world(with_regex, post, event_route);
    struct Capture(Option<(usize, [u8; 32])>);
    impl ModuleSandbox for Capture {
        fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
            if request.module_id == "main" {
                let input: ModuleCallInput = serde_cbor::from_slice(&request.input).unwrap();
                assert!(
                    input
                        .action
                        .as_ref()
                        .or(input.event.as_ref())
                        .unwrap()
                        .starts_with(&[0xd9, 0xd9, 0xf7]),
                    "old route nested Action must retain self-describe tag"
                );
                self.0 = Some((
                    request.input.len(),
                    *blake3::hash(&request.input).as_bytes(),
                ));
            }
            Err(ModuleCallFailure {
                module_id: request.module_id.clone(),
                trace_id: request.trace_id.clone(),
                code: ModuleCallErrorCode::InvalidOutput,
                detail: "golden capture only".into(),
            })
        }
    }
    let envelope = oracle.pending_actions.front().unwrap().clone();
    let expected_nested_action = crate::util::to_canonical_cbor(&envelope).unwrap();
    let abi_action = oasis7_wasm_abi::encode_canonical_cbor(&envelope).unwrap();
    assert_ne!(
        expected_nested_action, abi_action,
        "legacy nested Action CBOR carries the original tag"
    );
    let mut capture = Capture(None);
    let mut routed_event = crate::WorldEvent {
        id: 1,
        time: 11,
        caused_by: None,
        body: crate::WorldEventBody::ModuleCallFailed(ModuleCallFailure {
            module_id: "source".into(),
            trace_id: "source-event".into(),
            code: ModuleCallErrorCode::InvalidOutput,
            detail: "actual persisted event".into(),
        }),
    };
    if post {
        routed_event.caused_by = Some(crate::CausedBy::Action(envelope.id));
        routed_event.body = crate::WorldEventBody::Domain(crate::DomainEvent::AgentMoved {
            agent_id: "alice".into(),
            from: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
            to: oasis7_agent_api::world_action::GeoPos::new(1, 2, 3),
        });
        // The real root sink has already applied and published this result
        // before PostAction. Advance the independent legacy oracle through
        // that same business event so its journal height is authoritative.
        assert_eq!(
            oracle
                .append_event_for_test(routed_event.body.clone(), routed_event.caused_by.clone())
                .unwrap(),
            routed_event.id,
        );
        assert_eq!(oracle.journal.events.last(), Some(&routed_event));
    }
    if event_route {
        assert!(
            oracle
                .route_event_to_modules(&routed_event, &mut capture)
                .is_err()
        );
    } else {
        assert!(
            oracle
                .route_action_to_modules_with_stage_and_event(
                    &envelope,
                    if post {
                        ModuleSubscriptionStage::PostAction
                    } else {
                        ModuleSubscriptionStage::PreAction
                    },
                    if post { Some(&routed_event) } else { None },
                    &mut capture
                )
                .is_err()
        );
    }
    let expected_input = capture
        .0
        .expect("old route must select the genuine PRE-ACTION subscription");
    drop(oracle);
    let mut world = pre_action_world(with_regex, post, event_route);
    if post {
        world.state.time = 10;
    }
    assert!(world.journal.events.is_empty());
    let expected_manifest_hash = crate::util::hash_json(&world.manifest).unwrap();
    let fixture = genesis::build(&world, &memory, workspace);
    assert!(fixture.memory.same_pool(&memory));
    drop(world); // Execution below has no materialized World to hydrate/fall back to.
    assert_eq!(fixture.manifest_hash.hash(), expected_manifest_hash);
    let image = fixture_worker::Image::new(&memory);
    let stop = AtomicBool::new(false);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut original_reader = Originals {
        objects: fixture.objects.clone(),
        memory: &memory,
    };
    let mut transaction = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [91; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 2048,
            max_writes: 1024,
            max_facts: 32,
            max_pending_node_bytes: 16 << 20,
        },
    )
    .unwrap();
    let counter =
        WorldEventCounterV1::begin(&fixture.runtime, &fixture.head, &transaction, &memory).unwrap();
    assert_eq!(counter.journal_height(&transaction).unwrap(), 0);
    let live = transaction.parent_fence();
    let mut pending_work = genesis::current_work(&fixture.roots).runtime.pending;
    let guard = transaction.guard();
    let mut pending =
        oasis7_world_state::incremental_pending_action_overlay::begin_pending_action_current_v1(
            fixture.current.runtime().pending(),
            &fixture.roots,
            &mut transaction,
            &memory,
            &guard,
            &live,
            &mut pending_work.index,
            || false,
        )
        .unwrap();
    let mut read_io = genesis::io();
    let mut keys = genesis::keys();
    let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Business),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut read_io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let directory = {
        let mut di = genesis::io();
        let mut dk = genesis::keys();
        let mut dc = WorldFieldOverlayContextV1 {
            transaction: &mut transaction,
            values: ValueReadContext {
                source: &reader,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut di,
            },
            key_work: &mut dk,
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            take_state_error: || reader.cas.error(),
            cancelled: || false,
        };
        crate::world::incremental_module_invocation::build_accepted_invocation_directory_v1(
            &fixture.runtime,
            &fixture.head,
            fixture.current.raw().instances(),
            fixture.current.runtime().config(),
            &reader,
            &mut dc,
            &mut stage_directory_work(),
            &live,
        )
        .unwrap()
    };
    let selected = crate::world::incremental_action_execution::take_pending_action(
        &mut pending,
        &mut transaction,
        &mut read,
        &mut pending_work,
        &mut original_reader,
        &live,
    )
    .unwrap()
    .unwrap();
    let mut live_owner = || live;
    let mut value_io = genesis::io();
    let mut semantic = genesis::keys();
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: &mut transaction,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut value_io,
        },
        key_work: &mut semantic,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: stopped(&stop),
    };
    let mut member_cas = fixture.cas.clone();
    let mut fee_cas = fixture.cas.clone();
    let mut event_cas = fixture.cas.clone();
    let mut member_io = genesis::io();
    let mut member_keys = genesis::keys();
    let mut members = genesis::member(
        &mut member_cas,
        &memory,
        &mut member_io,
        &mut member_keys,
        stopped(&stop),
    );
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
        member_limits: MemberRecordLimitsV1 {
            semantic_bytes: 4096,
            payload_bytes: 8 << 20,
            record_bytes: 9 << 20,
        },
    };
    let mut event_io = genesis::io();
    let saved = std::rc::Rc::new(std::cell::RefCell::new(Vec::with_capacity(16)));
    let saved_sink = saved.clone();
    let objects = fixture.objects.clone();
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
                producer.persist_to(&mut |stream:&mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1,cancelled:&mut dyn FnMut()->bool| {
                let length=usize::try_from(stream.original_bytes()).unwrap();assert!(length<=64<<10);
                let fee=memory.reserve(length+128,MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
                let mut bytes=Vec::with_capacity(length);stream.write_to(&mut bytes)?;
                assert_eq!(bytes.len(),length);assert_eq!(*blake3::hash(&bytes).as_bytes(),stream.original_id());
                if cancelled(){return Err(ValueTreeError::Cancelled.into());}
                let id=objects.put(&bytes);assert_eq!(id,stream.original_id());
                assert!(saved_sink.borrow().len()<16);saved_sink.borrow_mut().push(id);drop(fee);Ok(())
            })
            },
        max_event_bytes: 64 << 10,
        original_work: genesis::canonical(),
    };
    let persisted = if post {
        let guard = overlay.transaction.guard();
        let mut step = crate::world::incremental_step::IwsStepContext::begin_counted(
            overlay.transaction,
            10,
            &guard,
            &live,
            &reader,
            &mut events.mutation,
            &mut events.original_work,
            &counter,
        )
        .unwrap();
        Some(
            step.dispatch_persisted_agent_event(
                &routed_event,
                64 << 10,
                &mut events.persist_original,
            )
            .unwrap(),
        )
    } else if event_route {
        Some(
            crate::world::incremental_module_execution_event::persist_module_output_event_v1(
                11,
                &routed_event,
                overlay.transaction,
                &mut events,
            )
            .unwrap(),
        )
    } else {
        None
    };
    if let Some(event) = persisted.as_ref() {
        let other = ExecutionTransaction::begin(
            fixture.parent.clone(),
            [92; 32],
            &reader,
            &memory,
            TransactionLimits {
                max_reads: 2048,
                max_writes: 1024,
                max_facts: 32,
                max_pending_node_bytes: 16 << 20,
            },
        )
        .unwrap();
        assert!(
            event.validate(&other).is_err(),
            "a persisted event must reject a distinct same-parent transaction"
        );
        event.validate(overlay.transaction).unwrap();
    }
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
    let mut work = execution_work();
    let initial_calls = work.output.calls_remaining;
    let mut hash_work = genesis::hash_work();
    let mut observed = None;
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
            now: 11,
            state: &reader,
            ctx: &mut overlay,
            members: &mut members,
            fees: &mut fees,
            events: &mut events,
            originals: &mut original_reader,
            worker: &mut worker,
            manifest_cache: &fixture.manifest_hash,
            manifest_work: &mut hash_work,
            work: &mut work,
            #[cfg(test)]
            input_observation: Some(&mut observed),
        };
        if event_route {
            let mut enumeration = stage_directory_work();
            enumeration.instance_scan.nodes_remaining = 0;
            enumeration.registry_scan.nodes_remaining = 0;
            target
                .route_persisted_event_v1(
                    persisted.as_ref().unwrap(),
                    None,
                    &directory,
                    &mut enumeration,
                    &mut live_owner,
                )
                .map(|count| (count > 0).then_some(()))
        } else if post {
            let mut enumeration = stage_directory_work();
            enumeration.instance_scan.nodes_remaining = 0;
            enumeration.registry_scan.nodes_remaining = 0;
            target
                .route_post_action_v1(
                    &selected,
                    persisted.as_ref(),
                    &directory,
                    &mut enumeration,
                    &mut live_owner,
                )
                .map(|count| (count > 0).then_some(()))
        } else {
            target
                .execute_pre_action_invocation_v1("main#stable", &selected, &mut live_owner)
                .map(|output| output.map(|_| ()))
        }
    };
    assert_eq!(observed, Some(expected_input));
    if cfg!(target_os = "linux") {
        assert!(result.unwrap().is_some());
        assert_eq!(work.output.calls_remaining, initial_calls - 2);
        assert_eq!(
            counter.journal_height(overlay.transaction).unwrap(),
            5 + u64::from(event_route || post)
        );
    } else {
        assert!(matches!(
            result,
            Err(RootModuleExecutionErrorV1::Worker(
                RootModuleWorkerErrorV1::Host(_)
            ))
        ));
        assert_eq!(work.output.calls_remaining, initial_calls - 1);
        assert_eq!(saved.borrow().len(), usize::from(event_route || post));
        drop(overlay);
        let guard = transaction.guard();
        assert!(transaction.seal(&guard, &live, 1).is_err());
    }
}

#[test]
#[cfg(feature = "wasmtime")]
fn actual_persisted_event_uses_current_directory_and_preserves_legacy_input() {
    run_action_stage_fixture(false, false, true);
}
