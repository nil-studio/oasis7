//! Genuine cross-family Rule Modify regressions for selective Accepted.
//! The only public route is the closed test runner. This is a Root component
//! check, not Normal/tick, Node ingress, or formal commit acceptance.
use super::*;
use crate::world::{
    incremental_action_data::transfer_resource::TransferResourceWorkV1,
    incremental_body_event::{
        materials::test_support::{MaterialFixtureLimitsV1, MaterialFixtureV1},
        read_fixed,
    },
    incremental_material_normalization::transfer::MaterialTransferWorkV1,
    incremental_module_execution_event::PersistedWorldEventV1,
};
use oasis7_agent_api::world_action::{GeoPos, MaterialTransitPriority, ResourceKind};
use oasis7_world_state::{
    MaterialLedgerId,
    incremental_agent_body::{AGENT_BODY_VALUE_BYTES, AgentBodyKeyV1, AgentBodyValueV1},
    incremental_agent_fields::AgentFieldV1,
    incremental_agent_subtree::{
        AgentSubtreeQualifiedRequestV1, AgentSubtreeScalarRequestV1, AgentSubtreeValueV1,
        read_agent_subtree_scalar_overlay_v1,
    },
    incremental_event_fields::{
        MAILBOX_EVENT_REFERENCE_BYTES, MAILBOX_FRONTIER_BYTES, MailboxFrontierV1, MailboxKeyV1,
        mailbox_event_id_v1,
    },
    incremental_field_read::WorldFieldReadContextV1,
    incremental_inventory::AgentLedgerTransferWorkV1,
    incremental_inventory_read::LedgerQuantityV1,
    incremental_material_records::{MaterialProfileCurrentReadWorkV1, MaterialWorkBudgetV1},
};

fn source_action() -> crate::Action {
    crate::Action::FormAlliance {
        proposer_agent_id: "sender".into(),
        alliance_id: "unused-alliance".into(),
        members: vec!["receiver".into()],
        charter: "unused-charter".into(),
    }
}

fn replacement(material: bool) -> crate::Action {
    if material {
        crate::Action::TransferMaterial {
            requester_agent_id: "sender".into(),
            from_ledger: MaterialLedgerId::agent("sender"),
            to_ledger: MaterialLedgerId::agent("receiver"),
            kind: "water".into(),
            amount: 3,
            distance_km: 0,
            priority: Some(MaterialTransitPriority::Standard),
            route_id: None,
            route_ids: vec![],
            auto_reroute: false,
        }
    } else {
        // Native TransferResource deliberately rejects until a real Rule
        // supplies EmitResourceTransfer. This is that genuine Rule path.
        crate::Action::EmitResourceTransfer {
            from_agent_id: "sender".into(),
            to_agent_id: "receiver".into(),
            kind: ResourceKind::Electricity,
            amount: 3,
        }
    }
}

fn expected_business(material: bool) -> crate::WorldEventBody {
    crate::WorldEventBody::Domain(if material {
        crate::DomainEvent::MaterialTransferred {
            transfer_id: Some(1),
            requester_agent_id: "sender".into(),
            from_ledger: MaterialLedgerId::agent("sender"),
            to_ledger: MaterialLedgerId::agent("receiver"),
            kind: "water".into(),
            amount: 3,
            distance_km: 0,
            priority: MaterialTransitPriority::Standard,
            route_id: None,
        }
    } else {
        crate::DomainEvent::ResourceTransferred {
            from_agent_id: "sender".into(),
            to_agent_id: "receiver".into(),
            kind: ResourceKind::Electricity,
            amount: 3,
        }
    })
}

fn transfer_world(material: bool) -> World {
    let mut world = initialized_world();
    world.state.resources.insert(ResourceKind::Electricity, 100);
    world.state.resources.insert(ResourceKind::Data, 100);
    for id in ["sender", "receiver"] {
        let mut agent = oasis7_world_state::agent_state::AgentState::new(id, GeoPos::new(0, 0, 0));
        if id == "sender" {
            agent.resources.set(ResourceKind::Electricity, 8).unwrap();
        }
        world
            .state
            .agents
            .insert(id.into(), crate::AgentCell::new(agent, 0));
    }
    world.state.material_ledgers.insert(
        MaterialLedgerId::agent("sender"),
        [("water".into(), 8)].into(),
    );
    let output = ModuleOutput {
        new_state: None,
        effects: vec![],
        emits: vec![ModuleEmit {
            kind: "rule.decision".into(),
            payload: serde_json::to_value(crate::RuleDecision {
                action_id: 1,
                verdict: crate::RuleVerdict::Modify,
                override_action: Some(replacement(material)),
                cost: crate::ResourceDelta {
                    entries: [(ResourceKind::Electricity, -3)].into(),
                },
                notes: vec!["actual-cross-family-rule".into()],
            })
            .unwrap(),
        }],
        tick_lifecycle: None,
        output_bytes: 0,
    };
    let bytes = wasm(&output);
    use sha2::Digest;
    let hash = hex::encode(sha2::Sha256::digest(&bytes));
    world
        .register_module_artifact(hash.clone(), &bytes)
        .unwrap();
    // The genuine VM fee owner is independent of the two transfer agents.
    world
        .state
        .module_artifact_owners
        .insert(hash.clone(), "alice".into());
    let record = world
        .module_registry
        .records
        .get_mut(&crate::ModuleRegistry::record_key("main", "1"))
        .unwrap();
    record.manifest.wasm_hash = hash.clone();
    record.manifest.subscriptions = vec![ModuleSubscription {
        event_kinds: vec![],
        action_kinds: vec!["action.gameplay.form_alliance".into()],
        stage: Some(ModuleSubscriptionStage::PreAction),
        filters: None,
    }];
    world
        .state
        .module_instances
        .get_mut("main#stable")
        .unwrap()
        .wasm_hash = hash;
    world.submit_action(source_action());
    assert_eq!(world.pending_actions.front().unwrap().id, 1);
    assert!(World::should_emit_action_accepted(&source_action()));
    assert!(!World::should_emit_action_accepted(&replacement(material)));
    world
}

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

fn current_resource<'a>(
    fixture: &genesis::Fixture<'a>,
    reader: &Reader<'a>,
    tx: &mut ExecutionTransaction<'_, Reader<'a>>,
    memory: &MemoryBudget,
    live: &ExecutionFence,
    agent: &str,
    ordinal: u64,
) -> Option<i64> {
    // Verification reads have their own finite work, after the production
    // consumer has finished. They never renew its Rule/admission/owner budgets.
    let mut io = genesis::io();
    let mut keys = genesis::keys();
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: tx,
        values: ValueReadContext {
            source: reader,
            memory,
            class: MemoryClass::Ordinary,
            io: &mut io,
        },
        key_work: &mut keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let guard = overlay.transaction.guard();
    match read_agent_subtree_scalar_overlay_v1(
        AgentSubtreeQualifiedRequestV1 {
            cell: AgentSubtreeScalarRequestV1 {
                agent,
                field: AgentFieldV1::ResourceAmount,
                ordinal,
            },
            qualification: fixture.current.raw().agents(),
        },
        &mut overlay,
        1024,
        &mut std::io::sink(),
        &guard,
        live,
    )
    .unwrap()
    {
        None => None,
        Some(AgentSubtreeValueV1::I64(value)) => Some(value),
        other => panic!("unexpected Current resource value: {other:?}"),
    }
}

pub(super) fn run(material: bool) {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let world = transfer_world(material);

    assert!(world.journal.events.is_empty());
    let expected_manifest_hash = crate::util::hash_json(&world.manifest).unwrap();
    let material_limits = MaterialFixtureLimitsV1::for_state(&world.state);
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

    let mut parent_io = genesis::io();
    let mut parent_keys = genesis::keys();
    let mut parent_read = WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Business),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut parent_io,
        },
        key_work: &mut parent_keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut material_work = MaterialTransferWorkV1 {
        steps_remaining: 10_000,
        bytes_remaining: 16 << 20,
        profile: MaterialProfileCurrentReadWorkV1 {
            scan: genesis::scan(),
            cells_remaining: 100_000,
            max_payload_bytes: 1 << 20,
        },
        inventory: AgentLedgerTransferWorkV1 {
            steps_remaining: 10_000,
            bytes_remaining: 16 << 20,
            pending_entries_remaining: 100_000,
        },
        receipt: MaterialWorkBudgetV1 {
            cells_remaining: 100_000,
            max_pending_node_bytes: 16 << 20,
            scan: genesis::scan(),
        },
    };
    let mut resource_work = TransferResourceWorkV1 {
        steps_remaining: 10_000,
        bytes_remaining: 16 << 20,
        pending_entries_remaining: 100_000,
        max_scalar_bytes: 1024,
    };
    let mut accepted_entries = 100_000usize;
    let mut backing = MaterialFixtureV1::build(
        &fixture.roots,
        &reader,
        &reader,
        &memory,
        material_limits,
        || reader.cas.error(),
        || false,
    );
    let mut materials = backing.context(&fixture.roots, &reader, || reader.cas.error());
    let mut admission_io = genesis::io();
    let mut admission_keys = genesis::keys();
    let mut admission_read = WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Authorization),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut admission_io,
        },
        key_work: &mut admission_keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut policy_scan = genesis::scan();
    let mut policy_work = genesis::current_work(&fixture.roots).runtime.revocation;
    let mut tail = crate::world::incremental_world_event_tail::CurrentWorldEventTailWorkV1 {
        originals_remaining: 1,
        original_bytes_remaining: 1 << 20,
        decode: super::action_body::audit_work(),
    };
    assert_eq!(selected.envelope().id, 1);
    assert_eq!(selected.envelope().action, source_action());
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
            input_observation: None,
        };

        let guard = target.ctx.transaction.guard();
        let current =
            oasis7_world_state::incremental_runtime_revocation::begin_runtime_revocation_current_v1(
                fixture.current.runtime().revocation(),
                target.ctx,
                &mut admission_read,
                &mut policy_scan,
                &mut policy_work,
                target.members,
                &guard,
                &live,
            ).unwrap();
        let (outcome, admission) = target
            .prepare_selected_rule_lane_v1(
                &selected,
                fixture.current.runtime().revocation(),
                &current,
                &mut admission_read,
                &mut policy_scan,
                &mut policy_work,
                &mut override_work,
                &directory,
                &mut rule_work,
                &mut live_owner,
            )
            .unwrap();
        assert_eq!(outcome.decision.action_id, 1);
        assert_eq!(outcome.decision.verdict, crate::RuleVerdict::Modify);
        assert_eq!(
            outcome.decision.override_action.as_ref(),
            Some(&replacement(material)),
        );
        assert_eq!(
            outcome.decision.cost,
            crate::ResourceDelta {
                entries: [(ResourceKind::Electricity, -3)].into(),
            },
        );
        // Do not insert any Current read here. Admission binds the exact
        // revision returned by the genuine second admission after Rule/Modify.
        if material {
            target
                .execute_material_transfer_after_rules_v1(
                    &selected,
                    &admission,
                    &outcome,
                    &mut materials,
                    &mut read,
                    &mut parent_read,
                    &mut material_work,
                    Some(&mut accepted_entries),
                    fixture.current.native().material(),
                    &mut override_work,
                    &directory,
                    &mut directory_work(),
                    &mut tail,
                    &mut live_owner,
                )
                .map(|completed| {
                    let event = PersistedWorldEventV1::from_material_transfer(&completed);
                    event.validate(target.ctx.transaction).unwrap();
                    assert_eq!(event.event().time, 11);
                    assert_eq!(event.event().caused_by, Some(crate::CausedBy::Action(1)));
                    assert_eq!(event.event().body, expected_business(material));
                })
        } else {
            target
                .execute_transfer_resource_after_rules_v1(
                    &selected,
                    &admission,
                    &outcome,
                    &mut materials,
                    &mut read,
                    &mut parent_read,
                    &mut resource_work,
                    Some(&mut accepted_entries),
                    &mut override_work,
                    &directory,
                    &mut directory_work(),
                    &mut tail,
                    &mut live_owner,
                )
                .map(|completed| {
                    let event = PersistedWorldEventV1::from_transfer_resource(&completed);
                    event.validate(target.ctx.transaction).unwrap();
                    assert_eq!(event.event().time, 11);
                    assert_eq!(event.event().caused_by, Some(crate::CausedBy::Action(1)));
                    assert_eq!(event.event().body, expected_business(material));
                })
        }
    };
    let originals: Vec<crate::WorldEvent> = saved
        .borrow()
        .iter()
        .map(|id| serde_cbor::from_slice(&fixture.objects.original(id)).unwrap())
        .collect();
    let accepted_count = originals
        .iter()
        .filter(|event| {
            matches!(
                &event.body,
                crate::WorldEventBody::Domain(crate::DomainEvent::ActionAccepted { .. })
            )
        })
        .count();
    let transfer_count = originals
        .iter()
        .filter(|event| {
            matches!(
                &event.body,
                crate::WorldEventBody::Domain(
                    crate::DomainEvent::MaterialTransferred { .. }
                        | crate::DomainEvent::ResourceTransferred { .. }
                )
            )
        })
        .count();
    if let Err(error) = result {
        // A baseline failure is evidence only, never a passing alternative.
        // Accepted is a provisional audit; poison must prevent any sealed fact
        // set from claiming that the failed business transfer completed.
        assert_eq!(
            format!("{error:?}"),
            "Module(Body(Execution(StaleGuard)))",
            "expected the confirmed old guard after real selective Accepted",
        );
        assert_eq!(accepted_count, 1);
        assert_eq!(transfer_count, 0);
        drop(overlay);
        let guard = transaction.guard();
        assert!(matches!(
            transaction.seal(&guard, &live, 1),
            Err(ExecutionError::Aborted),
        ));
        panic!("real cross-family Modify aborted after selective Accepted");
    }
    drop(overlay);
    assert_eq!(accepted_count, 1);
    assert_eq!(transfer_count, 1);
    // One genuine main Rule invocation. The emitted decision has no effects,
    // so module_runtime does not invoke an effect's Pure policy hook here.
    assert_eq!(work.output.calls_remaining, initial_calls - 1);
    // Both actual agents exist. Their point-presence route consumes value IO,
    // not the absent-agent pending-write scan quota. Do not invent scan work.
    assert_eq!(accepted_entries, 100_000);
    if material {
        assert!(material_work.steps_remaining < 10_000);
        assert!(material_work.inventory.steps_remaining < 10_000);
        assert_eq!(material_work.inventory.pending_entries_remaining, 100_000);
    } else {
        assert!(resource_work.steps_remaining < 10_000);
        assert_eq!(resource_work.pending_entries_remaining, 100_000);
    }
    assert_eq!(
        counter.journal_height(&transaction).unwrap(),
        u64::try_from(originals.len()).unwrap(),
    );
    let expected_override = crate::WorldEventBody::ActionOverridden(crate::ActionOverrideRecord {
        action_id: 1,
        original_action: source_action(),
        override_action: replacement(material),
    });
    let override_positions: Vec<_> = originals
        .iter()
        .enumerate()
        .filter_map(|(i, event)| (event.body == expected_override).then_some(i))
        .collect();
    let accepted_positions: Vec<_> = originals
        .iter()
        .enumerate()
        .filter_map(|(i, event)| {
            matches!(&event.body, crate::WorldEventBody::Domain(
                crate::DomainEvent::ActionAccepted {
                    action_id: 1, action_kind, actor_id, eta_ticks: 0, notes,
                }
            ) if action_kind == "action.gameplay.form_alliance"
                && actor_id == "sender"
                && notes.len() == 1
                && notes[0] == "accepted_for_gameplay_processing"
            )
            .then_some(i)
        })
        .collect();
    let transfer_positions: Vec<_> = originals
        .iter()
        .enumerate()
        .filter_map(|(i, event)| (event.body == expected_business(material)).then_some(i))
        .collect();
    assert_eq!(override_positions.len(), 1);
    assert_eq!(accepted_positions.len(), 1);
    assert_eq!(transfer_positions.len(), 1);
    assert!(override_positions[0] < accepted_positions[0]);
    assert!(accepted_positions[0] < transfer_positions[0]);
    for index in [
        override_positions[0],
        accepted_positions[0],
        transfer_positions[0],
    ] {
        assert_eq!(originals[index].time, 11);
        assert_eq!(originals[index].caused_by, Some(crate::CausedBy::Action(1)));
    }
    assert!(!originals.iter().any(|event| {
        matches!(
            &event.body,
            crate::WorldEventBody::Domain(
                crate::DomainEvent::ActionRejected { .. }
                    | crate::DomainEvent::AllianceFormed { .. }
            )
        )
    }));
    let decisions: Vec<_> = originals
        .iter()
        .filter_map(|event| {
            if let crate::WorldEventBody::RuleDecisionRecorded(record) = &event.body {
                Some(record)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].module_id, "main");
    assert_eq!(decisions[0].action_id, 1);
    assert_eq!(decisions[0].verdict, crate::RuleVerdict::Modify);
    assert_eq!(
        decisions[0].override_action.as_ref(),
        Some(&replacement(material))
    );
    assert_eq!(
        decisions[0].cost,
        crate::ResourceDelta {
            entries: [(ResourceKind::Electricity, -3)].into()
        },
    );

    // Independent conservation expectations use fixed initial balances and
    // the genuine persisted fee originals, not a hydrated post-state oracle.
    let charges: Vec<_> = originals
        .iter()
        .filter_map(|event| {
            if let crate::WorldEventBody::ModuleRuntimeCharged(charge) = &event.body {
                Some(charge)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        charges.len(),
        1,
        "only the main artifact has a paying owner"
    );
    let charge = charges[0];
    assert_eq!(charge.module_id, "main");
    assert_eq!(charge.payer_agent_id, "alice");
    assert_eq!(charge.compute_fee_kind, ResourceKind::Data);
    assert_eq!(charge.electricity_fee_kind, ResourceKind::Electricity);
    assert!(charge.compute_fee_amount > 0);
    assert!(charge.electricity_fee_amount > 0);
    assert!(charge.input_bytes > 0);
    assert!(charge.output_bytes > 0);
    assert_eq!(charge.effect_count, 0);
    assert_eq!(charge.emit_count, 1);

    {
        let mut mirror = WorldFieldOverlayContextV1 {
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
        assert_eq!(
            mirror.resource(ResourceKind::Electricity).unwrap(),
            Some(100 + charge.electricity_fee_amount - 3),
        );
        assert_eq!(
            mirror.resource(ResourceKind::Data).unwrap(),
            Some(100 + charge.compute_fee_amount),
        );
        assert_eq!(
            mirror
                .ledger(&MaterialLedgerId::agent("sender"), "water")
                .unwrap(),
            LedgerQuantityV1::Value(if material { 5 } else { 8 }),
        );
        assert_eq!(
            mirror
                .ledger(&MaterialLedgerId::agent("receiver"), "water")
                .unwrap(),
            if material {
                LedgerQuantityV1::Value(3)
            } else {
                LedgerQuantityV1::LedgerAbsent
            },
        );
    }
    assert_eq!(
        current_resource(
            &fixture,
            &reader,
            &mut transaction,
            &memory,
            &live,
            "sender",
            0
        ),
        Some(if material { 8 } else { 5 }),
    );
    assert_eq!(
        current_resource(
            &fixture,
            &reader,
            &mut transaction,
            &memory,
            &live,
            "receiver",
            0
        ),
        if material { None } else { Some(3) },
    );
    assert_eq!(
        current_resource(
            &fixture,
            &reader,
            &mut transaction,
            &memory,
            &live,
            "alice",
            0
        ),
        Some(1000 - charge.electricity_fee_amount),
    );
    assert_eq!(
        current_resource(
            &fixture,
            &reader,
            &mut transaction,
            &memory,
            &live,
            "alice",
            1
        ),
        Some(1000 - charge.compute_fee_amount),
    );
    let accepted_original = saved.borrow()[accepted_positions[0]];
    let transfer_original = saved.borrow()[transfer_positions[0]];
    for (agent, expected_last_active, expected_refs) in [
        ("sender", 11, vec![accepted_original, transfer_original]),
        (
            "receiver",
            if material { 0 } else { 11 },
            if material {
                vec![]
            } else {
                vec![transfer_original]
            },
        ),
    ] {
        let key = AgentBodyKeyV1::new(agent).unwrap();
        let body = AgentBodyValueV1::from_canonical_bytes(
            &read_fixed::<AGENT_BODY_VALUE_BYTES, _, _, _, _>(
                &mut transaction,
                StateNamespaceV1::Business,
                key.as_bytes(),
                &reader,
                &mut events.mutation,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            body.last_active, expected_last_active,
            "{agent} last_active"
        );
        let key = MailboxKeyV1::new(agent, None).unwrap();
        let frontier = MailboxFrontierV1::from_canonical_bytes(
            &read_fixed::<MAILBOX_FRONTIER_BYTES, _, _, _, _>(
                &mut transaction,
                StateNamespaceV1::Scheduler,
                key.as_bytes(),
                &reader,
                &mut events.mutation,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            (frontier.head, frontier.tail),
            (0, u64::try_from(expected_refs.len()).unwrap()),
            "{agent} exact mailbox frontier",
        );
        for (ordinal, expected_id) in expected_refs.into_iter().enumerate() {
            let key = MailboxKeyV1::new(agent, Some(u64::try_from(ordinal).unwrap())).unwrap();
            let reference = read_fixed::<MAILBOX_EVENT_REFERENCE_BYTES, _, _, _, _>(
                &mut transaction,
                StateNamespaceV1::Scheduler,
                key.as_bytes(),
                &reader,
                &mut events.mutation,
            )
            .unwrap();
            assert_eq!(
                mailbox_event_id_v1(&reference).unwrap(),
                expected_id,
                "{agent} mailbox original {ordinal}",
            );
        }
    }
}
