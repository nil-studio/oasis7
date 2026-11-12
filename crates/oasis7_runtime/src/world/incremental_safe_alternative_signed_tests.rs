//! Genuine signed positive/authority-changed/retry inputs. The fixture keeps
//! every original and reaches the actual private Root matching/Core/CAS lane.
//! Complete authorization publication is added when its shared publisher lands.
use super::*;
#[path = "incremental_safe_alternative_signed_fixture.rs"]
mod fixture;
use crate::world::{
    incremental_factory_directory_orders::*,
    incremental_industrial_eligibility::incremental_industrial_eligibility_factory::IndustrialEligibilityReadWorkV1,
    incremental_industrial_eligibility::*,
};
use crate::{Action, WorldState};
use oasis7_agent_api::world_action::GeoPos;
use oasis7_client_api::{
    safe_alternative::{
        FineGrainedMoveRequestV1, SafeAlternativeCommandV1, SafeAlternativeOperationV1 as Op,
    },
    world_service::sign_read_request,
};
use oasis7_formal_protocol::controlled_authority::formal::bounded_context::{
    FormalBoundedContextV1, FormalBoundedWorkV1,
};
use oasis7_world_state::incremental_cognition_proof_ledger::*;
use oasis7_world_state::incremental_world_rule_composite_family::bind_current_agent_orders_v1;
use oasis7_world_state::incremental_world_rule_receipts::qualify_world_rule_agent_orders_v1;
use serde_json::{Value, json};
fn signed(
    key: &str,
    operation: Op,
) -> crate::service_admission::SignedReadRequest<SafeAlternativeCommandV1> {
    let request = SafeAlternativeCommandV1 {
        api_rev: 1,
        client_operation_id: format!("signed-{}", crate::util::hash_json(&operation).unwrap()),
        world_id: "world".into(),
        genesis_digest: "fixture".into(),
        branch_id: "main".into(),
        reorg_epoch: 0,
        agent_id: "agent".into(),
        owner_binding: "owner".into(),
        owner_generation: 1,
        operation,
    };
    sign_read_request(request.signing_domain(), request, key).unwrap()
}
fn signed_world(
    revoke: bool,
) -> (
    World,
    crate::service_admission::SignedReadRequest<SafeAlternativeCommandV1>,
    String,
) {
    let key = hex::encode([71u8; 32]);
    let public = sign_read_request("fixture", (), &key)
        .unwrap()
        .subject_public_key;
    let mut world = World::new_with_state(WorldState::default());
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent".into(),
        pos: GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    world.submit_action(Action::ClaimStarterOc {
        agent_id: "agent".into(),
        player_id: "owner".into(),
        public_key: Some(public),
    });
    world.step().unwrap();
    world
        .install_capability_agent_identity("agent", "owner", 1)
        .unwrap();
    world.chain_resource_manifest = world
        .snapshot_with_chain_resource_context(
            crate::ChainResourceDerivationContext {
                world_id: "world",
                chain_id: "fixture-chain",
                genesis_ref: Some("fixture"),
                created_at_height: 0,
                manifest_height: 0,
                commit_block_hash: None,
                tick: world.state.time,
            },
            "fixture-config",
            "fixture-generation",
        )
        .chain_resource_manifest;
    world
        .bind_cognition_runtime("world", "main", 0, None, "pending", 0)
        .unwrap();
    let proposed = world
        .apply_authenticated_safe_alternative(&signed(
            &key,
            Op::Propose {
                request_id: "signed-request".into(),
                original: FineGrainedMoveRequestV1 {
                    goal: "Take path by waypoint with precise arrival".into(),
                    waypoints: vec![GeoPos::new(20, 0, 0), GeoPos::new(40, 0, 0)],
                    arrival_tick: Some(10),
                },
                valid_until_tick: 20,
                predecessor_request_id: None,
            },
        ))
        .unwrap();
    assert_eq!(
        world.pending_actions_len(),
        0,
        "proposal never queues execution"
    );
    let confirmed = world
        .apply_authenticated_safe_alternative(&signed(
            &key,
            Op::Confirm {
                request_id: "signed-request".into(),
                generation: proposed["generation"].as_u64().unwrap(),
                candidate_id: proposed["candidates"][0]["candidate_id"]
                    .as_str()
                    .unwrap()
                    .into(),
                candidate_digest: proposed["candidates"][0]["candidate_digest"]
                    .as_str()
                    .unwrap()
                    .into(),
                confirmation_id: "signed-confirm".into(),
                valid_until_tick: 15,
            },
        ))
        .unwrap();
    let call = signed(
        &key,
        Op::Execute {
            request_id: "signed-request".into(),
            authorization_id: confirmed["authorization"]["authorization_id"]
                .as_str()
                .unwrap()
                .into(),
            execution_id: "signed-execution".into(),
        },
    );
    let bound = world.apply_authenticated_safe_alternative(&call).unwrap();
    assert_eq!(bound["state"], "execution_bound");
    assert_eq!(world.pending_actions_len(), 1);
    let journal = world.journal.events.len();
    let queued = oasis7_wasm_abi::encode_canonical_cbor(&world.pending_actions).unwrap();
    assert_eq!(
        world.apply_authenticated_safe_alternative(&call).unwrap(),
        bound
    );
    assert_eq!(world.pending_actions_len(), 1);
    assert_eq!(
        oasis7_wasm_abi::encode_canonical_cbor(&world.pending_actions).unwrap(),
        queued,
        "exact retry preserves the unique original queued envelope"
    );
    assert_eq!(
        world.journal.events.len(),
        journal + 1,
        "original retry records exactly one complete signed command"
    );
    match &world.journal.events[journal].body {
        crate::WorldEventBody::CapabilityAuthorization(
            crate::CapabilityAuthorizationEvent::SafeAlternativeCommandRecorded { signed },
        ) => assert_eq!(
            oasis7_wasm_abi::encode_canonical_cbor(signed).unwrap(),
            oasis7_wasm_abi::encode_canonical_cbor(&call).unwrap()
        ),
        other => panic!("exact retry must retain typed original signed command, got {other:?}"),
    }
    assert_eq!(world.journal.events[journal].caused_by, None);
    let lineage = crate::world::safe_alternative::lineage_key(&call.request);
    if revoke {
        world
            .install_capability_agent_identity("agent", "owner", 2)
            .unwrap();
    }
    (world, call, lineage)
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
fn typed() -> oasis7_world_state::incremental_runtime_revocation::RevocationReadWorkV1 {
    oasis7_world_state::incremental_runtime_revocation::RevocationReadWorkV1 {
        json: genesis::json(),
        max_retained_trees: 64,
        max_tree_heap: 8 << 20,
    }
}
fn order_work() -> FactoryOrderDirectoryWorkV1 {
    FactoryOrderDirectoryWorkV1 {
        ids:
            oasis7_world_state::incremental_world_rule_composite_family::AgentOrderDirectoryWorkV1 {
                max_ids: 32,
                max_id_bytes: 256,
                comparison_bytes_remaining: 16 << 20,
            },
        pending_entries_remaining: 10000,
        merge_comparison_bytes_remaining: 16 << 20,
        max_index_text_bytes: 256,
    }
}
#[test]
fn real_signed_matching_genesis_scope_core_rejection_and_exact_retry() {
    for revoke in [false, true] {
        run_signed_case(revoke);
    }
}
fn run_signed_case(revoke: bool) {
    let memory = MemoryBudget::new(512 << 20, 32 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let (world, call, lineage) = signed_world(revoke);
    let original_journal = world.journal.events.len() as u64;
    let now = world.state.time + 1;
    let action_id = world.pending_actions.front().unwrap().id;
    let material_limits=crate::world::incremental_body_event::materials::test_support::MaterialFixtureLimitsV1::for_state(&world.state);
    let mut oracle = World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
    oracle.step().unwrap();
    let expected = oracle.capability_revocation_state.safe_alternatives[&lineage].clone();
    assert_eq!(
        expected["outcome"]["status"],
        if revoke { "rejected" } else { "applied" }
    );
    assert_eq!(oracle.pending_actions_len(), 0);
    let retry_journal = oracle.journal.events.len();
    if revoke {
        assert_eq!(
            oracle
                .apply_authenticated_safe_alternative(&call)
                .unwrap_err(),
            "authority_changed"
        );
        assert_eq!(
            oracle.journal.events.len(),
            retry_journal,
            "changed authority rejects before command publication"
        );
    } else {
        assert_eq!(
            oracle.apply_authenticated_safe_alternative(&call).unwrap(),
            expected
        );
        assert_eq!(oracle.journal.events.len(), retry_journal + 1);
        match &oracle.journal.events[retry_journal].body {
            crate::WorldEventBody::CapabilityAuthorization(
                crate::CapabilityAuthorizationEvent::SafeAlternativeCommandRecorded { signed },
            ) => assert_eq!(
                oasis7_wasm_abi::encode_canonical_cbor(signed).unwrap(),
                oasis7_wasm_abi::encode_canonical_cbor(&call).unwrap()
            ),
            other => panic!("consumed exact retry must retain typed signed command, got {other:?}"),
        }
    }
    assert_eq!(
        oracle.capability_revocation_state.safe_alternatives[&lineage], expected,
        "retry leaves the durable unique result unchanged"
    );
    assert_eq!(
        oracle.pending_actions_len(),
        0,
        "consumed exact retry cannot queue twice"
    );
    let expected_core = oracle
        .journal
        .events
        .iter()
        .find(|event| {
            event.caused_by == Some(crate::CausedBy::Action(action_id))
                && matches!(
                    event.body,
                    crate::WorldEventBody::Domain(
                        crate::DomainEvent::AgentMoved { .. }
                            | crate::DomainEvent::ActionRejected { .. }
                    )
                )
        })
        .unwrap()
        .clone();
    use crate::world::incremental_authorization_root_current::{
        AuthorizationRootCacheLimitsV1, AuthorizationRootSourceReadWorkV1, qualify_accepted_parent,
    };
    let meter = FormalBoundedWorkV1::new(20_000_000, 128 << 20).unwrap();
    let mut cancelled = || false;
    let mut common = FormalBoundedContextV1::new(&memory, &meter, &mut cancelled);
    let projection = world
        .prepare_iws_authorization_projection_data_v1(
            AuthorizationRootCacheLimitsV1 {
                max_wire_bytes: 8 << 20,
                max_depth: 64,
                max_map_keys: 4096,
                max_key_bytes: 65536,
            },
            &mut common,
            MemoryClass::Ordinary,
        )
        .unwrap();
    let fixture = fixture::build_signed(&world, &memory, workspace);
    drop(world);
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
    assert_eq!(
        counter.journal_height(&transaction).unwrap(),
        original_journal
    );
    let live = transaction.parent_fence();
    {
        let mut clock_cas = fixture.cas.clone();
        let mut clock_io = genesis::io();
        let mut canonical = genesis::canonical();
        let mut clock = BodyMutationContext {
            sink: &mut clock_cas,
            memory: &memory,
            io: &mut clock_io,
            cancelled: || false,
        };
        let guard = transaction.guard();
        let step = crate::world::incremental_step::IwsStepContext::begin_counted(
            &mut transaction,
            now - 1,
            &guard,
            &live,
            &reader,
            &mut clock,
            &mut canonical,
            &counter,
        )
        .unwrap();
        drop(step);
    }

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

    let mut scope_scan = genesis::scan();
    let mut order_typed = typed();
    let mut oi = genesis::io();
    let mut ok = genesis::keys();
    let mut order_read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Authorization),
        state: &reader,
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
        cancelled: || false,
    };
    let orders_parent = qualify_world_rule_agent_orders_v1(
        &fixture.roots,
        &mut order_read,
        &mut scope_scan,
        &mut order_typed,
    )
    .unwrap();
    let guard = overlay.transaction.guard();
    let orders_source = bind_current_agent_orders_v1(
        &orders_parent,
        &fixture.roots,
        &reader,
        &mut overlay,
        &mut scope_scan,
        &mut order_typed,
        &guard,
        &live,
    )
    .unwrap();
    assert!(
        matches!(
            overlay.transaction.validate_guard(&guard, &live),
            Err(ExecutionError::StaleGuard)
        ),
        "first actual orders read extends the read set; no old guard may be reused"
    );
    let mut order_work = order_work();
    let guard = overlay.transaction.guard();
    let accepted_orders = build_accepted_factory_order_directory_v1(
        &fixture.runtime,
        &fixture.head,
        &orders_source,
        &reader,
        &mut overlay,
        &mut scope_scan,
        &mut order_typed,
        &mut order_work,
        &guard,
        &live,
    )
    .unwrap();
    let guard = overlay.transaction.guard();
    let orders = open_current_factory_order_directory_v1(
        &accepted_orders,
        &orders_source,
        &reader,
        &mut overlay,
        &mut scope_scan,
        &mut order_typed,
        &mut order_work,
        &guard,
        &live,
    )
    .unwrap();
    let industrial = IndustrialEligibilitySourcesV1 {
        roots: &fixture.roots,
        orders: &orders,
        agents: fixture.current.raw().agents(),
        native: fixture.current.native().native(),
        factories: fixture.current.native().factory(),
        sites: fixture.current.native().factory_authority(),
        anchors: fixture.current.native().agent_authority(),
        revocation: fixture.current.runtime().revocation(),
        artifacts: fixture.current.runtime().resources(),
    };
    let mut policy_scan = genesis::scan();
    let mut policy_work = typed();
    let guard = overlay.transaction.guard();
    let mut revocation =
        oasis7_world_state::incremental_runtime_revocation::begin_runtime_revocation_current_v1(
            fixture.current.runtime().revocation(),
            &mut overlay,
            &mut order_read,
            &mut policy_scan,
            &mut policy_work,
            &mut members,
            &guard,
            &live,
        )
        .unwrap();
    let mut authorization_work=AuthorizationRootSourceReadWorkV1 {
        json:genesis::json(),typed:typed(),scan:genesis::scan(),
        capabilities:oasis7_world_state::incremental_runtime_capability_records::RuntimeCapabilityRecordsWorkV1 {cells_remaining:1_000_000,max_pending_node_bytes:8<<20},
        pending_entries:100_000,max_selected_heap:8<<20,max_text_bytes:65536,
    };
    let accepted_projection = qualify_accepted_parent(
        projection,
        &fixture.head,
        &fixture.runtime,
        fixture.current.runtime(),
        &revocation,
        &fixture.roots,
        &mut overlay,
        &mut order_read,
        &mut authorization_work,
        &mut common,
        &live,
    )
    .unwrap();
    let mut authorization = accepted_projection
        .begin_current(&fixture.head, overlay.transaction, &mut common, &live)
        .unwrap();
    let ledger = begin_cognition_proof_ledger_current_v1(
        &fixture.ledger,
        &fixture.head,
        overlay.transaction,
        &mut common,
        &live,
    )
    .unwrap();
    let mut scope_work = IndustrialEligibilityWorkV1 {
        read: IndustrialEligibilityReadWorkV1 {
            cells_remaining: 1_000_000,
            comparison_bytes_remaining: 32 << 20,
            max_text_bytes: 4096,
        },
        typed: typed(),
        scan: genesis::scan(),
        revocation: typed(),
        pending_entries_remaining: 100000,
    };
    let mut scope_module_io = genesis::io();
    let mut ledger_scan = genesis::scan();
    let mut ledger_work = typed();
    let mut write_work =
        oasis7_world_state::incremental_runtime_revocation::RevocationWriteWorkV1 {
            cells_remaining: 1_000_000,
            max_pending_node_bytes: 8 << 20,
            json: oasis7_world_state::incremental_json_records::JsonWriteWorkV1 {
                cells_remaining: 1_000_000,
                json_nodes_remaining: 1_000_000,
                max_json_depth: 64,
                max_pending_node_bytes: 8 << 20,
            },
        };
    let mut audit = fixture::decode_work();
    let mut observed = None;
    let mut backing =
        crate::world::incremental_body_event::materials::test_support::MaterialFixtureV1::build(
            &fixture.roots,
            &reader,
            &reader,
            &memory,
            material_limits,
            || reader.cas.error(),
            || false,
        );
    let mut materials = backing.context(&fixture.roots, &reader, || reader.cas.error());
    let mut pi = genesis::io();
    let mut pk = genesis::keys();
    let mut parent_read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Business),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut pi,
        },
        key_work: &mut pk,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut accepted_entries = 100000;
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
        target
            .execute_selected_matching_core_before_publication_v1(
                &selected,
                &ledger,
                fixture.current.runtime().revocation(),
                &mut revocation,
                &industrial,
                &mut materials,
                &mut read,
                &mut parent_read,
                &mut accepted_entries,
                &mut policy_scan,
                &mut policy_work,
                &mut ledger_scan,
                &mut ledger_work,
                &mut scope_work,
                &mut scope_module_io,
                &mut write_work,
                &mut audit,
                &directory,
                &mut rule_work,
                &mut directory_work(),
                &mut common,
                &mut live_owner,
            )
            .unwrap()
    };
    use crate::world::incremental_module_invocation::MatchingBeforePublicationV1;
    let publication = match result {
        MatchingBeforePublicationV1::Applied {
            outcome,
            completed,
            publication,
        } => {
            assert!(!revoke);
            assert_eq!(completed.event(), &expected_core);
            assert_eq!(outcome.decision.verdict, crate::RuleVerdict::Allow);
            publication
        }
        MatchingBeforePublicationV1::Rejected { publication } => {
            assert!(revoke);
            publication
        }
        MatchingBeforePublicationV1::NoMatch => {
            panic!("genuine signed Current record must enter matching lane")
        }
    };
    publication
        .validate(overlay.transaction, &mut common, &live)
        .unwrap();
    assert!(
        matches!(publication.authorization_event(),crate::CapabilityAuthorizationEvent::SafeAlternativeOutcomeRecorded {
        key,action_id:actual,event_id,
    } if key==&lineage&&*actual==action_id&&*event_id==expected_core.id)
    );
    assert_eq!(
        work.output.calls_remaining, initial_calls,
        "empty registry/instances invoke no Rule or module worker"
    );
    assert_eq!(
        saved.borrow().len(),
        2,
        "exact Accepted and actual result originals, before the still-private auth publication"
    );
    let projected = authorization
        .refresh_safe_alternative_outcome(
            publication,
            fixture.current.runtime(),
            &revocation,
            &fixture.roots,
            &mut overlay,
            &mut order_read,
            &mut authorization_work,
            &mut common,
            &live,
        )
        .unwrap();
    let mut registration_work = 100_000;
    let final_projection = authorization
        .finish_for_publication(
            projected,
            &revocation,
            overlay.transaction,
            &mut registration_work,
            &mut common,
            &live,
        )
        .unwrap();
    let root_publication = final_projection
        .publish_root(
            fixture.current.runtime().authorization_root(),
            &mut overlay,
            &mut members,
            &mut registration_work,
            &mut common,
            &live,
        )
        .unwrap();
    let published=crate::world::incremental_module_execution_event::publish_safe_alternative_outcome_event_v1(
        root_publication,now,&mut common,&live,overlay.transaction,&mut events).unwrap();
    published
        .validate(overlay.transaction, &mut common, &live)
        .unwrap();
    assert!(
        matches!(&published.event().body,crate::WorldEventBody::CapabilityAuthorization(
        crate::CapabilityAuthorizationEvent::SafeAlternativeOutcomeRecorded {key,action_id:actual,event_id})
        if key==&lineage&&*actual==action_id&&*event_id==expected_core.id)
    );
    assert_eq!(
        saved.borrow().len(),
        3,
        "Accepted, actual Core result and fixed authorization outcome originals"
    );
    let oracle_root: &[u8; 64] = oracle
        .capability_authorization_root
        .as_bytes()
        .try_into()
        .unwrap();
    let guard = overlay.transaction.guard();
    assert!(!oasis7_world_state::incremental_runtime_authorization_root::replace_current_authorization_root_v1(
        oracle_root,oracle_root,fixture.current.runtime().authorization_root(),&mut overlay,&mut members,&guard,&live).unwrap(),
        "actual Current Text equals independent old World authorization root");
    let guard = overlay.transaction.guard();
    let current=oasis7_world_state::incremental_runtime_revocation::read_current_safe_alternative_for_action_v1(action_id,
        fixture.current.runtime().revocation(),&revocation,&mut overlay,&mut order_read,&mut policy_scan,&mut policy_work,&guard,&live).unwrap().unwrap();
    assert_eq!(
        current.record(),
        &expected,
        "actual exact-member CAS equals independent old World projection"
    );
    assert!(
        crate::world::incremental_action_execution::take_pending_action(
            &mut pending,
            overlay.transaction,
            &mut read,
            &mut pending_work,
            &mut original_reader,
            &live
        )
        .unwrap()
        .is_none(),
        "genuine FIFO input was consumed exactly once"
    );
    // The fixed root CAS and counted OutcomeRecorded original succeeded.
    // Tick consensus, formal acceptance and WorldReady remain separate owners.
}

#[path = "incremental_normal_transits_due_tests.rs"]
mod normal_transits_due;
