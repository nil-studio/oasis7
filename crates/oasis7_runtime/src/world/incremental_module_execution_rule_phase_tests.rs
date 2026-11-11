#[path = "incremental_module_execution_rule_phase_fixture.rs"]
mod fixture;
use super::*;
use crate::world::incremental_module_invocation::CurrentInvocationEnumerationWorkV1;
use fixture::{RuleOracle, pre_action_world};
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
    run_rule_fixture_with_override(case, false);
}

pub(super) fn run_rule_fixture_with_override(case: u8, publish_override: bool) {
    run_rule_fixture_composed(case, publish_override, 0);
}
#[test]
#[cfg(all(feature = "wasmtime", target_os = "linux"))]
fn selected_core_consumes_actual_nonempty_rule_modify_and_pricing() {
    run_rule_fixture_composed(1, true, 1);
}
#[test]
#[cfg(all(feature = "wasmtime", target_os = "linux"))]
fn selected_core_rejected_move_settles_general_cost_without_pricing_cost() {
    run_rule_fixture_composed(1, true, 2);
}
#[test]
#[cfg(all(feature = "wasmtime", target_os = "linux"))]
fn selected_core_insufficient_rule_cost_preserves_position_and_balance() {
    run_rule_fixture_composed(1, true, 3);
}
#[test]
#[cfg(all(feature = "wasmtime", target_os = "linux"))]
fn selected_core_genuine_rule_denial_preserves_balance_and_position() {
    run_rule_fixture_composed(7, false, 4);
}
pub(super) fn run_rule_fixture_composed(case: u8, publish_override: bool, core_mode: u8) {
    let execute_core = core_mode != 0;
    let with_regex = false;
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut reference = pre_action_world(false, case, core_mode);
    let envelope = reference.pending_actions.front().unwrap().clone();
    let mut rule_oracle = RuleOracle {
        case,
        core_mode,
        last: None,
    };
    let expected_rule = reference.evaluate_rule_decisions(&envelope, &mut rule_oracle);
    let final_rule_input = rule_oracle.last;
    let post_rule_resources = reference.state.resources.clone();
    let expected_override = if publish_override {
        assert_eq!(case, 1);
        let replacement = expected_rule
            .as_ref()
            .unwrap()
            .0
            .override_action
            .as_ref()
            .unwrap();
        reference
            .record_action_override(
                crate::ActionOverrideRecord {
                    action_id: envelope.id,
                    original_action: envelope.action.clone(),
                    override_action: replacement.clone(),
                },
                Some(crate::CausedBy::Action(envelope.id)),
            )
            .unwrap();
        Some(
            oasis7_wasm_abi::encode_canonical_cbor(reference.journal.events.last().unwrap())
                .unwrap(),
        )
    } else {
        None
    };

    let mut expected_core_resources = None;
    let mut expected_core_position = None;
    let expected_core = if execute_core {
        let rule = expected_rule.as_ref().unwrap();
        let effective = crate::ActionEnvelope {
            governance_authorization: None,
            committed_recipe_origin: None,
            id: envelope.id,
            action: rule
                .0
                .override_action
                .as_ref()
                .unwrap_or(&envelope.action)
                .clone(),
        };
        let mut body = if rule.0.verdict == crate::RuleVerdict::Deny {
            crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
                action_id: envelope.id,
                reason: crate::RejectReason::RuleDenied {
                    notes: rule.0.notes.clone(),
                },
            })
        } else {
            reference.action_to_event(&effective).unwrap()
        };
        if rule.0.verdict != crate::RuleVerdict::Deny {
            let cost = if matches!(
                &body,
                crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected { .. })
            ) {
                rule.1.as_ref().unwrap_or(&rule.0.cost)
            } else {
                &rule.0.cost
            };
            let deficits = cost.deficits(&reference.state.resources);
            if !deficits.is_empty() {
                body = crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
                    action_id: envelope.id,
                    reason: crate::RejectReason::InsufficientResources { deficits },
                });
            } else if let Err(error) = reference.apply_resource_delta(cost) {
                body = crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
                    action_id: envelope.id,
                    reason: crate::RejectReason::RuleDenied {
                        notes: vec![format!("rule decision cost apply rejected: {error:?}")],
                    },
                });
            }
        }
        if core_mode == 3 {
            assert!(
                matches!(&body,
                crate::WorldEventBody::Domain(crate::DomainEvent::ActionRejected {
                    reason: crate::RejectReason::InsufficientResources { deficits }, ..
                }) if !deficits.is_empty()),
                "insufficient-cost fixture must actually reject"
            );
        }
        reference
            .append_event(body, Some(crate::CausedBy::Action(envelope.id)))
            .unwrap();
        expected_core_resources = Some(reference.state.resources.clone());
        expected_core_position = Some(reference.state.agents["alice"].state.pos);
        Some(
            oasis7_wasm_abi::encode_canonical_cbor(reference.journal.events.last().unwrap())
                .unwrap(),
        )
    } else {
        None
    };

    drop(reference);
    // The legacy oracle is independently initialized: a failed legacy route
    // may append journal events and must never contaminate the GEN source.
    let mut oracle = pre_action_world(with_regex, case, core_mode);
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
    let world = pre_action_world(with_regex, case, core_mode);
    assert!(world.journal.events.is_empty());
    let expected_manifest_hash = crate::util::hash_json(&world.manifest).unwrap();
    let material_limits = crate::world::incremental_body_event::materials::test_support::MaterialFixtureLimitsV1::for_state(&world.state);
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
    let mut override_audit = None;
    let mut completed_core = None;
    let mut override_work =
        oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1 {
            structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
                bytes_remaining: 2 << 20,
                values_remaining: 100000,
                max_depth: 64,
            },
            canonical: genesis::canonical(),
            max_original_bytes: 64 << 10,
            max_heap_bytes: 64 << 20,
        };
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
            #[cfg(any(test, feature = "test-support"))]
            input_observation: Some(&mut observed),
        };
        if execute_core {
            // Exercise the full Root selected entry once: it owns both real
            // admission reads, Rule/Modify audit, settlement and post routing.
            let mut backing = crate::world::incremental_body_event::materials::test_support::MaterialFixtureV1::build(
                &fixture.roots, &reader, &reader, &memory, material_limits,
                || reader.cas.error(), || false);
            let mut materials = backing.context(&fixture.roots, &reader, || reader.cas.error());
            let mut ai = genesis::io();
            let mut ak = genesis::keys();
            let mut admission_read =
                oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
                    root: fixture.roots.state_root(StateNamespaceV1::Authorization),
                    state: &reader,
                    values: ValueReadContext {
                        source: &reader,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut ai,
                    },
                    key_work: &mut ak,
                    semantic_bytes: 4096,
                    record_bytes: 16 << 20,
                    take_state_error: || reader.cas.error(),
                    cancelled: || false,
                };
            let mut policy_scan = genesis::scan();
            let mut policy_work = genesis::current_work(&fixture.roots).runtime.revocation;
            let guard = target.ctx.transaction.guard();
            let current = oasis7_world_state::incremental_runtime_revocation::begin_runtime_revocation_current_v1(
                fixture.current.runtime().revocation(), target.ctx, &mut admission_read,
                &mut policy_scan, &mut policy_work, target.members, &guard, &live).unwrap();
            let mut tail =
                crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1 {
                    originals_remaining: 1,
                    original_bytes_remaining: 1 << 20,
                    decode: super::action_body::audit_work(),
                };
            let (outcome, completed) = target
                .execute_selected_core_v1(
                    &selected,
                    fixture.current.runtime().revocation(),
                    &current,
                    &mut admission_read,
                    &mut materials,
                    &mut read,
                    &mut policy_scan,
                    &mut policy_work,
                    &mut override_work,
                    &directory,
                    &mut rule_work,
                    &mut directory_work(),
                    &mut tail,
                    &mut live_owner,
                )
                .unwrap();
            completed_core = Some(completed);
            Ok(outcome)
        } else {
            target
                .execute_pre_action_rule_phases_v1(
                    &selected,
                    &directory,
                    &mut rule_work,
                    &mut live_owner,
                )
                .and_then(|outcome| {
                    if publish_override {
                        override_audit = target.publish_action_override_v1(
                            &selected,
                            &outcome,
                            &mut override_work,
                            &mut live_owner,
                        )?;
                    }
                    Ok(outcome)
                })
        }
    };
    assert_eq!(observed, Some(expected_input));
    if cfg!(target_os = "linux") {
        if (2..=6).contains(&case) {
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
        if publish_override && execute_core {
            let expected = expected_override.as_ref().unwrap();
            assert!(
                saved
                    .borrow()
                    .iter()
                    .any(|id| fixture.objects.original(id) == *expected),
                "Root must persist the exact old World Modify audit once"
            );
            assert_eq!(
                saved
                    .borrow()
                    .iter()
                    .filter(|id| fixture.objects.original(id) == *expected)
                    .count(),
                1
            );
        }
        if publish_override && !execute_core {
            let audit = override_audit
                .as_ref()
                .expect("actual Modify must publish its original audit");
            audit.validate(overlay.transaction).unwrap();
            let expected = expected_override.as_ref().unwrap();
            assert_eq!(
                &oasis7_wasm_abi::encode_canonical_cbor(audit.event()).unwrap(),
                expected
            );
            let last = *saved.borrow().last().unwrap();
            assert_eq!(&fixture.objects.original(&last), expected);
            let unrelated = ExecutionTransaction::begin(
                fixture.parent.clone(),
                [177; 32],
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
            assert!(audit.validate(&unrelated).is_err());
        }

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
            if case == 1 {
                12 + u64::from(publish_override) + u64::from(execute_core)
            } else {
                6 + u64::from(execute_core)
            }
        );
        if execute_core {
            drop(overlay);
            let completed = completed_core
                .take()
                .expect("actual Root selected Core result");
            if case == 7 {
                assert!(!completed.event_before_post_action());
                assert_eq!(
                    expected_core_resources.as_ref().unwrap(),
                    &post_rule_resources,
                    "Deny preserves the treasury after genuine module fees, with no Rule business debit"
                );
            }
            let persisted = crate::world::incremental_module_execution_event::PersistedWorldEventV1::from_core_action(&completed);
            persisted.validate(&transaction).unwrap();
            let expected = expected_core.as_ref().unwrap();
            assert_eq!(
                &oasis7_wasm_abi::encode_canonical_cbor(persisted.event()).unwrap(),
                expected
            );
            assert_eq!(
                &fixture.objects.original(saved.borrow().last().unwrap()),
                expected
            );
            assert_eq!(
                counter.journal_height(&transaction).unwrap(),
                if case == 7 { 7 } else { 14 }
            );
            assert_eq!(
                completed.event().time,
                11,
                "reborrowing phases must not advance the tick clock"
            );
            let mut inventory = WorldFieldOverlayContextV1 {
                transaction: &mut transaction,
                values: ValueReadContext {
                    source: &reader,
                    memory: &memory,
                    class: MemoryClass::Ordinary,
                    io: &mut *read.values.io,
                },
                key_work: &mut *read.key_work,
                semantic_bytes: read.semantic_bytes,
                record_bytes: read.record_bytes,
                take_state_error: &mut read.take_state_error,
                cancelled: &mut read.cancelled,
            };
            for (&kind, &expected) in expected_core_resources.as_ref().unwrap() {
                assert_eq!(
                    inventory.resource(kind).unwrap(),
                    Some(expected),
                    "Current global resource {kind:?}"
                );
            }
            use oasis7_world_state::incremental_agent_fields::AgentFieldV1;
            use oasis7_world_state::incremental_agent_subtree::*;
            let position = expected_core_position.unwrap();
            for (field, expected) in [
                (AgentFieldV1::PositionX, position.x_cm),
                (AgentFieldV1::PositionY, position.y_cm),
                (AgentFieldV1::PositionZ, position.z_cm),
            ] {
                let guard = inventory.transaction.guard();
                let actual = read_agent_subtree_scalar_overlay_v1(
                    AgentSubtreeQualifiedRequestV1 {
                        cell: AgentSubtreeScalarRequestV1 {
                            agent: "alice",
                            field,
                            ordinal: 0,
                        },
                        qualification: fixture.current.raw().agents(),
                    },
                    &mut inventory,
                    16 << 20,
                    &mut std::io::sink(),
                    &guard,
                    &live,
                )
                .unwrap();
                assert!(
                    matches!(actual, Some(AgentSubtreeValueV1::I64(value)) if value == expected)
                );
            }
        }
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
