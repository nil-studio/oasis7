use super::*;
use crate::world::incremental_module_invocation::CurrentInvocationEnumerationWorkV1;
fn directory_work() -> CurrentInvocationEnumerationWorkV1 {
    let scan = || StateScanBudget {
        nodes_remaining: 10_000_000,
        read_bytes_remaining: 512 << 20,
        leaves_remaining: 100_000,
    };
    CurrentInvocationEnumerationWorkV1 {
        instance_scan: scan(),
        registry_scan: scan(),
        pending_entries: 10000,
        comparison_bytes_remaining: 4_000_000_000,
        max_keys: 256,
        max_key_bytes: 1024,
        max_text_bytes: 1024,
    }
}
fn rule_output(case: u8) -> ModuleOutput {
    let mut output = ModuleOutput {
        new_state: Some(b"new".to_vec()),
        effects: vec![ModuleEffectIntent {
            kind: "fixture.effect".into(),
            params: serde_json::json!({"amount":1}),
            cap_ref: String::new(),
            cap_slot: Some("main".into()),
        }],
        emits: vec![ModuleEmit {
            kind: "rule.decision".into(),
            payload: serde_json::to_value(crate::RuleDecision {
                action_id: 1,
                verdict: crate::RuleVerdict::Allow,
                override_action: None,
                cost: crate::ResourceDelta {
                    entries: [(
                        oasis7_agent_api::world_action::ResourceKind::Electricity,
                        -3,
                    )]
                    .into(),
                },
                notes: vec!["real-rule".into()],
            })
            .unwrap(),
        }],
        tick_lifecycle: None,
        output_bytes: 0,
    };
    match case {
        1 => {
            output.emits[0].payload["verdict"] = serde_json::json!("modify");
            output.emits[0].payload["override_action"] =
                serde_json::to_value(crate::Action::MoveAgent {
                    agent_id: "alice".into(),
                    to: oasis7_agent_api::world_action::GeoPos::new(20, 30, 40),
                })
                .unwrap();
        }
        2 => {
            output.emits[0].payload = serde_json::json!({"action_id":"not-an-id","verdict":"allow"})
        }
        3 => output.emits[0].payload["action_id"] = serde_json::json!(999),
        4 => output.emits.push(output.emits[0].clone()),
        5 => {
            output.emits[0].payload =
                serde_json::json!({"action_id":"not-an-id","verdict":"allow"});
            output.emits.push(output.emits[0].clone());
        }
        6 => {
            output.emits[0].payload["action_id"] = serde_json::json!(999);
            output.emits.push(output.emits[0].clone());
        }
        _ => {}
    }
    output
}
#[test]
#[cfg(feature = "wasmtime")]
fn actual_rule_phase_matches_legacy_after_world_drop() {
    run_rule_fixture(0);
}
#[test]
#[cfg(feature = "wasmtime")]
fn actual_rule_modify_runs_nonempty_pricing_on_derived_input() {
    run_rule_fixture(1);
}
#[test]
#[cfg(feature = "wasmtime")]
fn actual_rule_invalid_payload_preserves_json_error_event() {
    run_rule_fixture(2);
}
#[test]
#[cfg(feature = "wasmtime")]
fn actual_rule_wrong_action_id_preserves_error_event() {
    run_rule_fixture(3);
}
#[test]
#[cfg(feature = "wasmtime")]
fn actual_rule_multiple_emits_preserves_error_event() {
    run_rule_fixture(4);
}
#[test]
#[cfg(feature = "wasmtime")]
fn actual_rule_invalid_first_of_two_emits_preserves_first_error() {
    run_rule_fixture(5);
}
#[test]
#[cfg(feature = "wasmtime")]
fn actual_rule_wrong_id_first_of_two_emits_preserves_first_error() {
    run_rule_fixture(6);
}
fn run_rule_fixture(case: u8) {
    let with_regex = false;
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    fn pre_action_world(with_regex: bool, case: u8) -> World {
        let mut world = initialized_world();
        let record = world
            .module_registry
            .records
            .get_mut(&crate::ModuleRegistry::record_key("main", "1"))
            .unwrap();
        record.manifest.subscriptions[0].stage = Some(ModuleSubscriptionStage::PreAction);
        record.manifest.subscriptions[0].action_kinds = vec!["action.move_agent".into()];
        if with_regex {
            record.manifest.subscriptions[0].filters = Some(serde_json::json!({
                "action": [{"path": "/action/data/agent_id", "re": "^alice$"}]
            }));
        }
        world.submit_action(crate::Action::MoveAgent {
            agent_id: "alice".into(),
            to: oasis7_agent_api::world_action::GeoPos::new(1, 2, 3),
        });
        let output = rule_output(case);
        let bytes = wasm(&output);
        use sha2::Digest;
        let hash = hex::encode(sha2::Sha256::digest(&bytes));
        world
            .register_module_artifact(hash.clone(), &bytes)
            .unwrap();
        world
            .state
            .module_artifact_owners
            .insert(hash.clone(), "alice".into());
        world
            .module_registry
            .records
            .get_mut(&crate::ModuleRegistry::record_key("main", "1"))
            .unwrap()
            .manifest
            .wasm_hash = hash.clone();
        world
            .state
            .module_instances
            .get_mut("main#stable")
            .unwrap()
            .wasm_hash = hash;
        if case == 1 {
            let bytes = wasm(&rule_output(0));
            let mut pricing = manifest("m1.rule.move", ModuleKind::Reducer, &bytes);
            pricing.subscriptions = vec![ModuleSubscription {
                event_kinds: vec![],
                action_kinds: vec!["action.move_agent".into()],
                stage: Some(ModuleSubscriptionStage::PreAction),
                filters: None,
            }];
            pricing.required_caps = vec!["cap-main".into()];
            pricing
                .abi_contract
                .cap_slots
                .insert("main".into(), "cap-main".into());
            pricing.abi_contract.policy_hooks = vec!["policy".into()];
            world
                .register_module_artifact(pricing.wasm_hash.clone(), &bytes)
                .unwrap();
            world
                .state
                .module_artifact_owners
                .insert(pricing.wasm_hash.clone(), "alice".into());
            world
                .module_registry
                .active
                .insert("m1.rule.move".into(), "1".into());
            world.module_registry.records.insert(
                crate::ModuleRegistry::record_key("m1.rule.move", "1"),
                crate::ModuleRecord {
                    manifest: pricing.clone(),
                    registered_at: 11,
                    registered_by: "genesis".into(),
                    audit_event_id: None,
                },
            );
            let mut instance = world.state.module_instances["main#stable"].clone();
            instance.instance_id = "pricing#real".into();
            instance.module_id = "m1.rule.move".into();
            instance.wasm_hash = pricing.wasm_hash;
            world
                .state
                .module_instances
                .insert(instance.instance_id.clone(), instance);
        }
        world
    }
    struct RuleOracle {
        case: u8,
        last: Option<(usize, [u8; 32])>,
    }
    impl ModuleSandbox for RuleOracle {
        fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
            if matches!(request.module_id.as_str(), "main" | "m1.rule.move") {
                self.last = Some((
                    request.input.len(),
                    *blake3::hash(&request.input).as_bytes(),
                ));
            }
            Ok(if request.module_id == "main" {
                rule_output(self.case)
            } else if request.module_id == "m1.rule.move" {
                rule_output(0)
            } else {
                ModuleOutput {
                    new_state: None,
                    effects: vec![],
                    emits: vec![],
                    tick_lifecycle: None,
                    output_bytes: 0,
                }
            })
        }
    }
    let mut reference = pre_action_world(false, case);
    let envelope = reference.pending_actions.front().unwrap().clone();
    let mut rule_oracle = RuleOracle { case, last: None };
    let expected_rule = reference.evaluate_rule_decisions(&envelope, &mut rule_oracle);
    let final_rule_input = rule_oracle.last;
    drop(reference);
    // The legacy oracle is independently initialized: a failed legacy route
    // may append journal events and must never contaminate the GEN source.
    let mut oracle = pre_action_world(with_regex, case);
    struct Capture(Option<(usize, [u8; 32])>);
    impl ModuleSandbox for Capture {
        fn call(&mut self, request: &ModuleCallRequest) -> Result<ModuleOutput, ModuleCallFailure> {
            if request.module_id == "main" {
                let input: ModuleCallInput = serde_cbor::from_slice(&request.input).unwrap();
                assert!(
                    input
                        .action
                        .as_ref()
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
    assert!(
        oracle
            .route_action_to_modules(&envelope, &mut capture)
            .is_err()
    );
    let mut expected_input = capture
        .0
        .expect("old route must select the genuine PRE-ACTION subscription");
    if case == 1 && cfg!(target_os = "linux") {
        expected_input = final_rule_input.unwrap();
    }
    drop(oracle);
    let world = pre_action_world(with_regex, case);
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
            &mut directory_work(),
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
    let mut rule_work = RootRulePhaseWorkV1 {
        enumeration: directory_work(),
        payload:
            oasis7_world_state::incremental_rule_decision_decode::RuleDecisionPayloadDecodeWorkV1 {
                structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
                    bytes_remaining: 1 << 20,
                    values_remaining: 100000,
                    max_depth: 64,
                },
                max_payload_bytes: 64 << 10,
                max_heap_bytes: 128 << 20,
            },
        canonical: genesis::canonical(),
        max_captured_outputs: 8,
        max_payload_bytes: 64 << 10,
        merge_units_remaining: 1_000_000_000,
    };
    rule_work.enumeration.instance_scan.nodes_remaining = 0;
    rule_work.enumeration.registry_scan.nodes_remaining = 0;
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
        target.execute_pre_action_rule_phases_v1(
            &selected,
            &directory,
            &mut rule_work,
            &mut live_owner,
        )
    };
    assert_eq!(observed, Some(expected_input));
    if cfg!(target_os = "linux") {
        if case >= 2 {
            let expected = expected_rule.unwrap_err();
            let Err(RootModuleExecutionErrorV1::Business(actual)) = result else {
                panic!("expected original business failure");
            };
            assert_eq!(format!("{:?}", &*actual), format!("{expected:?}"));
            let events: Vec<crate::WorldEvent> = saved
                .borrow()
                .iter()
                .map(|id| serde_cbor::from_slice(&fixture.objects.original(id)).unwrap())
                .collect();
            let crate::WorldEventBody::ModuleCallFailed(recorded) = &events.last().unwrap().body
            else {
                panic!("expected actual ModuleCallFailed original");
            };
            let crate::WorldError::ModuleCallFailed {
                module_id,
                trace_id,
                code,
                detail,
            } = &expected
            else {
                panic!("expected original module call failure");
            };
            assert_eq!(&recorded.module_id, module_id);
            assert_eq!(&recorded.trace_id, trace_id);
            assert_eq!(&recorded.code, code);
            assert_eq!(&recorded.detail, detail);
            assert_eq!(events.last().unwrap().caused_by, None);
            drop(overlay);
            let guard = transaction.guard();
            assert!(transaction.seal(&guard, &live, 1).is_err());
            return;
        }
        let outcome = result.unwrap();
        let expected_rule = expected_rule.unwrap();
        assert_eq!(outcome.decision, expected_rule.0);
        assert_eq!(outcome.movement_rejection_cost, expected_rule.1);
        let audits: Vec<_> = saved
            .borrow()
            .iter()
            .filter_map(|id| {
                let event: crate::WorldEvent =
                    serde_cbor::from_slice(&fixture.objects.original(id)).unwrap();
                if let crate::WorldEventBody::RuleDecisionRecorded(record) = event.body {
                    assert_eq!(
                        event.caused_by,
                        Some(crate::CausedBy::Action(selected.envelope().id))
                    );
                    Some(record)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(audits.len(), if case == 1 { 2 } else { 1 });
        if case == 1 {
            assert_eq!(audits[1].module_id, "m1.rule.move");
        }
        assert_eq!(audits[0].module_id, "main");
        assert_eq!(
            work.output.calls_remaining,
            initial_calls - if case == 1 { 4 } else { 2 }
        );
        assert_eq!(
            counter.journal_height(overlay.transaction).unwrap(),
            if case == 1 { 12 } else { 6 }
        );
    } else {
        assert!(matches!(
            result,
            Err(RootModuleExecutionErrorV1::Worker(
                RootModuleWorkerErrorV1::Host(_)
            ))
        ));
        assert_eq!(work.output.calls_remaining, initial_calls - 1);
        assert!(saved.borrow().is_empty());
        drop(overlay);
        let guard = transaction.guard();
        assert!(transaction.seal(&guard, &live, 1).is_err());
    }
}
