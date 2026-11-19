//! Real GEN pending originals consumed without retaining a World.
use super::core_materials::{MaterialFixtureLimitsV1, MaterialFixtureV1};
use super::*;
use crate::world::incremental_step::IwsStepContext;
use oasis7_agent_api::world_action::GeoPos;
#[test]
fn safe_alternative_action_index_comes_from_genesis_and_current_records() {
    let memory = MemoryBudget::new(512 << 20, 8 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    // Storage qualification and derived lookup only: these observations are
    // deliberately not business admission or signed safe-alternative permits.
    for (lineage, action) in [
        ("lineage-z", Some(17)),
        ("lineage-a", Some(9)),
        ("unbound", None),
    ] {
        world.capability_revocation_state.safe_alternatives.insert(
            lineage.into(),
            serde_json::json!({"authorization":{"action_id":action}}),
        );
    }
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let qualification = fixture.current.runtime().revocation();
    assert_eq!(qualification.safe_alternatives_len(), 3);
    assert_eq!(
        qualification.safe_alternative_lineage_for_action(17),
        Some("lineage-z")
    );
    assert_eq!(
        qualification.safe_alternative_lineage_for_action(9),
        Some("lineage-a")
    );
    assert_eq!(qualification.safe_alternative_lineage_for_action(10), None);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };
    let mut tx = ExecutionTransaction::begin(
        fixture.parent.clone(),
        [123; 32],
        &reader,
        &memory,
        TransactionLimits {
            max_reads: 128,
            max_writes: 16,
            max_facts: 8,
            max_pending_node_bytes: 4 << 20,
        },
    )
    .unwrap();
    let live = tx.parent_fence();
    let mut read_io = genesis::io();
    let mut read_keys = genesis::keys();
    let mut overlay_io = genesis::io();
    let mut overlay_keys = genesis::keys();
    let mut read = oasis7_world_state::incremental_field_read::WorldFieldReadContextV1 {
        root: fixture.roots.state_root(StateNamespaceV1::Authorization),
        state: &reader,
        values: ValueReadContext {
            source: &reader,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut read_io,
        },
        key_work: &mut read_keys,
        semantic_bytes: 4096,
        record_bytes: 16 << 20,
        take_state_error: || reader.cas.error(),
        cancelled: || false,
    };
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
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
        cancelled: || false,
    };
    let mut sink = fixture.cas.clone();
    let mut write_io = genesis::io();
    let mut write_keys = genesis::keys();
    let mut write = genesis::member(&mut sink, &memory, &mut write_io, &mut write_keys, || false);
    let mut scan = genesis::scan();
    let mut work = genesis::current_work(&fixture.roots).runtime.revocation;
    let guard = overlay.transaction.guard();
    let current =
        oasis7_world_state::incremental_runtime_revocation::begin_runtime_revocation_current_v1(
            qualification,
            &mut overlay,
            &mut read,
            &mut scan,
            &mut work,
            &mut write,
            &guard,
            &live,
        )
        .unwrap();
    let guard = overlay.transaction.guard();
    let selected = oasis7_world_state::incremental_runtime_revocation::read_current_safe_alternative_for_action_v1(
        17, qualification, &current, &mut overlay, &mut read, &mut scan, &mut work, &guard, &live).unwrap().unwrap();
    assert_eq!(selected.lineage(), "lineage-z");
    assert_eq!(
        selected.record()["authorization"]["action_id"].as_u64(),
        Some(17)
    );
    selected.validate(overlay.transaction, &live).unwrap();
    let guard = overlay.transaction.guard();
    assert!(oasis7_world_state::incremental_runtime_revocation::read_current_safe_alternative_for_action_v1(
        10, qualification, &current, &mut overlay, &mut read, &mut scan, &mut work, &guard, &live).unwrap().is_none());
    overlay
        .transaction
        .write(
            StateNamespaceV1::Scheduler,
            b"observation-invalidated",
            Some([1; 32]),
        )
        .unwrap();
    assert!(selected.validate(overlay.transaction, &live).is_err());
}
#[test]
fn real_queued_register_then_move_consumes_originals_and_updates_current_position() {
    queued_register_move(false, false, false, false);
}
#[test]
fn selected_core_uses_real_rules_and_preserves_counted_result_owner() {
    queued_register_move(true, false, false, false);
}
#[test]
fn selected_core_success_and_rejections_match_original_world_bytes() {
    queued_register_move(true, true, false, false);
}
#[test]
fn selected_core_settlement_preserves_source_error_and_aborts_prefix() {
    queued_register_move(true, false, true, false);
}
#[test]
fn selected_common_admission_revision_change_rejects_prepare_and_aborts() {
    queued_register_move(true, false, false, true);
}
fn queued_register_move(
    with_rules: bool,
    with_rejections: bool,
    source_failure: bool,
    stale_admission: bool,
) {
    let memory = MemoryBudget::new(512 << 20, 8 << 20).unwrap();
    let workspace = memory.reserve(32 << 20, MemoryClass::Ordinary).unwrap();
    let mut world = World::new();
    world.submit_action(crate::Action::RegisterAgent {
        agent_id: "queued-alice".into(),
        pos: GeoPos {
            x_cm: 1,
            y_cm: 2,
            z_cm: 3,
        },
    });
    world.submit_action(crate::Action::MoveAgent {
        agent_id: "queued-alice".into(),
        to: GeoPos {
            x_cm: 11,
            y_cm: 22,
            z_cm: 33,
        },
    });
    if with_rejections {
        world
            .state
            .gameplay_policy
            .forbidden_location_ids
            .push("44:55:66".into());
        world.submit_action(crate::Action::RegisterAgent {
            agent_id: "queued-alice".into(),
            pos: GeoPos::new(7, 8, 9),
        });
        world.submit_action(crate::Action::MoveAgent {
            agent_id: "missing".into(),
            to: GeoPos::new(11, 22, 33),
        });
        world.submit_action(crate::Action::MoveAgent {
            agent_id: "queued-alice".into(),
            to: GeoPos::new(44, 55, 66),
        });
    }
    // Independent old World semantic oracle. It is discarded before GEN-backed
    // execution; no materialized World is available to the incremental kernel.
    if with_rules {
        world.capability_revocation_state.safe_alternatives.insert(
            "unrelated-lineage".into(),
            serde_json::json!({"authorization":{"action_id":99999}}),
        );
    }
    let mut oracle = World::new();
    oracle.capability_revocation_state = world.capability_revocation_state.clone();
    oracle.state.gameplay_policy.forbidden_location_ids =
        world.state.gameplay_policy.forbidden_location_ids.clone();
    for input in &world.pending_actions {
        oracle.submit_action(input.action.clone());
    }
    oracle.step().unwrap();
    let expected_events: Vec<_> = oracle
        .journal
        .events
        .iter()
        .map(|event| oasis7_wasm_abi::encode_canonical_cbor(event).unwrap())
        .collect();
    drop(oracle);
    let count = world.pending_actions.len();
    // The fixed workspace above prepays these two bounded canonical input oracles.
    let expected_inputs: Vec<_> = world
        .pending_actions
        .iter()
        .map(|envelope| {
            let bytes = oasis7_wasm_abi::encode_canonical_cbor(envelope).unwrap();
            assert!(bytes.len() <= 1 << 20);
            *blake3::hash(&bytes).as_bytes()
        })
        .collect();
    let expected_time = world.state.time;
    let material_limits = MaterialFixtureLimitsV1::for_state(&world.state);
    let fixture = genesis::build(&world, &memory, workspace);
    drop(world);
    let reader = Reader {
        cas: fixture.cas.clone(),
        memory: &memory,
    };

    let mut material_backing = MaterialFixtureV1::build(
        &fixture.roots,
        &reader,
        &reader,
        &memory,
        material_limits,
        || reader.cas.error(),
        || false,
    );
    let mut materials = material_backing.context(&fixture.roots, &reader, || reader.cas.error());
    let mut originals = Originals {
        objects: fixture.objects.clone(),
        memory: &memory,
    };
    let mut tx = ExecutionTransaction::begin(
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
    let live = tx.parent_fence();
    let counter =
        WorldEventCounterV1::begin(&fixture.runtime, &fixture.head, &tx, &memory).unwrap();
    // A second mint must fail before either counter contributes a fact;
    // clones of the original remain the only producer counter for this tx.
    assert!(WorldEventCounterV1::begin(&fixture.runtime, &fixture.head, &tx, &memory).is_err());
    let directory = if with_rules {
        let mut io = genesis::io();
        let mut keys = genesis::keys();
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &reader,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut io,
            },
            key_work: &mut keys,
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            take_state_error: || reader.cas.error(),
            cancelled: || false,
        };
        Some(
            build_accepted_invocation_directory_v1(
                &fixture.runtime,
                &fixture.head,
                fixture.current.raw().instances(),
                fixture.current.runtime().config(),
                &reader,
                &mut overlay,
                &mut super::action_body::rules::directory_work(),
                &live,
            )
            .unwrap(),
        )
    } else {
        None
    };
    let mut pending_work = genesis::current_work(&fixture.roots).runtime.pending;
    let guard = tx.guard();
    let mut pending =
        oasis7_world_state::incremental_pending_action_overlay::begin_pending_action_current_v1(
            fixture.current.runtime().pending(),
            &fixture.roots,
            &mut tx,
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
    let mut write_io = genesis::io();
    let mut sink = fixture.cas.clone();
    let mut mutation = BodyMutationContext {
        sink: &mut sink,
        memory: &memory,
        io: &mut write_io,
        cancelled: || false,
    };
    let mut canonical = genesis::canonical();
    let guard = tx.guard();
    let mut step = IwsStepContext::begin_counted(
        &mut tx,
        expected_time,
        &guard,
        &live,
        &reader,
        &mut mutation,
        &mut canonical,
        &counter,
    )
    .unwrap();
    let mut json = genesis::json();
    let mut scan = genesis::scan();
    let mut policy_work = genesis::current_work(&fixture.roots).runtime.revocation;
    let mut entries = 100000;
    let mut events = Vec::new();
    let mut cost_cas = fixture.cas.clone();
    let mut cost_io = genesis::io();
    let mut cost_keys = genesis::keys();
    let mut cost_write =
        genesis::member(&mut cost_cas, &memory, &mut cost_io, &mut cost_keys, || {
            false
        });
    let mut audits = super::action_body::audit_work();
    for _ in 0..count {
        let mut persist =
            |producer: &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1| {
                producer.persist_to(&mut |stream:&mut dyn crate::world::incremental_body_event::AgentEventOriginalStreamV1,cancelled:&mut dyn FnMut()->bool| {
                let length=usize::try_from(stream.original_bytes()).unwrap();assert!(length<=64<<10);
                let charge=memory.reserve(length+128,MemoryClass::Ordinary).map_err(ExecutionError::Memory)?;
                let mut bytes=Vec::with_capacity(length);stream.write_to(&mut bytes)?;
                assert!(!cancelled());assert_eq!(fixture.objects.put(&bytes),stream.original_id());
                events.push(serde_cbor::from_slice::<crate::WorldEvent>(&bytes).unwrap());
                drop(charge);Ok(())
            })
            };
        if let Some(directory) = &directory {
            let selected = crate::world::incremental_action_execution::take_pending_action(
                &mut pending,
                step.transaction,
                &mut read,
                &mut pending_work,
                &mut originals,
                &live,
            )
            .unwrap()
            .unwrap();
            if !source_failure && !stale_admission {
                // Exercise the actual Root consumer, including result routing,
                // PostAction and counter-backed tail recovery in the same tx.
                let (_, completed) = super::action_body::rules::evaluate_with_core(
                    &fixture,
                    &reader,
                    step.transaction,
                    &selected,
                    directory,
                    &counter,
                    &mut originals,
                    &memory,
                    step.time(),
                    &live,
                    Some(super::action_body::rules::CoreExecutionFixtureV1 {
                        limits: material_limits,
                        persist: &mut persist,
                    }),
                );
                let completed = completed.expect("actual Root Core consumer result");
                assert!(completed.event_before_post_action());
                completed.validate(step.transaction).unwrap();
                continue;
            }
            let admission = super::action_admission_fixture::admit(
                &selected,
                step.transaction,
                &fixture,
                &reader,
                &memory,
                &live,
            );
            let now = step.time();
            let outcome = super::action_body::rules::outcome(
                &fixture,
                &reader,
                step.transaction,
                &selected,
                directory,
                &counter,
                &mut originals,
                &memory,
                now,
                &live,
            );
            // Rule outputs may advance the transaction. Re-read the actual
            // Current admission input; never just re-sign the old observation.
            let admission = if stale_admission {
                admission
            } else {
                super::action_admission_fixture::admit(
                    &selected,
                    step.transaction,
                    &fixture,
                    &reader,
                    &memory,
                    &live,
                )
            };
            if stale_admission {
                step.transaction
                    .write(
                        StateNamespaceV1::Scheduler,
                        b"admission-invalidating-write",
                        Some([3; 32]),
                    )
                    .unwrap();
                let guard = step.transaction.guard();
                let result = crate::world::incremental_action_execution_core::prepare_selected_core_action_v1(
                    &mut step, &selected, &admission, &outcome, fixture.current.raw().agents(),
                    &mut read, &mut scan, &mut policy_work, &guard, &live);
                let observed = match &result {
                    Ok(_) => "unexpected successful prepare".to_owned(),
                    Err(error) => format!("{error:?}"),
                };
                assert!(
                    matches!(
                        result,
                        Err(
                            crate::world::incremental_body::BodyMutationError::Execution(
                                ExecutionError::StaleGuard
                            )
                        )
                    ),
                    "{observed}"
                );
                drop(step);
                let guard = tx.guard();
                assert!(matches!(
                    tx.seal(&guard, &live, 1),
                    Err(ExecutionError::Aborted)
                ));
                assert!(events.is_empty());
                return;
            }
            let guard = step.transaction.guard();
            let before = step.transaction.ordered_fact_count();
            let prepared =
                crate::world::incremental_action_execution_core::prepare_selected_core_action_v1(
                    &mut step,
                    &selected,
                    &admission,
                    &outcome,
                    fixture.current.raw().agents(),
                    &mut read,
                    &mut scan,
                    &mut policy_work,
                    &guard,
                    &live,
                )
                .unwrap();
            assert_eq!(
                step.transaction.ordered_fact_count(),
                before,
                "preflight must not publish an event"
            );
            let settled = {
                let mut inventory = WorldFieldOverlayContextV1 {
                    transaction: step.transaction,
                    values: ValueReadContext {
                        source: &reader,
                        memory: &memory,
                        class: MemoryClass::Ordinary,
                        io: &mut *read.values.io,
                    },
                    key_work: &mut *read.key_work,
                    semantic_bytes: read.semantic_bytes,
                    record_bytes: read.record_bytes,
                    take_state_error: || {
                        if source_failure {
                            Some(ValueTreeError::Invalid(
                                "injected Current resource source error",
                            ))
                        } else {
                            (read.take_state_error)()
                        }
                    },
                    cancelled: &mut read.cancelled,
                };
                let result =
                    crate::world::incremental_action_execution_core::settle_prepared_core_action_v1(
                        prepared,
                        &selected,
                        &mut inventory,
                        &mut cost_write,
                        &live,
                    );
                if source_failure {
                    let error = match result {
                        Err(error) => error,
                        Ok(_) => panic!("source error must fail settlement"),
                    };
                    let guard = inventory.transaction.guard();
                    drop(inventory);
                    drop(step);
                    drop(persist);
                    assert!(matches!(
                        tx.seal(&guard, &live, fixture.roots.height + 1),
                        Err(ExecutionError::Aborted)
                    ));
                    assert!(matches!(error,
                        crate::world::incremental_action_execution_core::CoreSettlementErrorV1::Resources(
                            crate::world::incremental_rule_resources::RuleResourceError::Value(
                                ValueTreeError::Invalid("injected Current resource source error")
                            )
                        )
                    ), "settlement must preserve the first concrete source error: {error:?}");
                    assert!(events.is_empty());
                    return;
                }
                result.unwrap()
            };
            let guard = step.transaction.guard();
            let completed =
                crate::world::incremental_action_execution_core::apply_settled_core_action_v1(
                    &mut step,
                    &mut materials,
                    &selected,
                    settled,
                    fixture.current.raw().agents(),
                    &mut read,
                    &mut audits,
                    64 << 10,
                    &guard,
                    &live,
                    &mut persist,
                )
                .unwrap();
            assert!(
                completed.event_before_post_action(),
                "actual zero-cost settlement takes the pre-PostAction Event branch"
            );
            let persisted = crate::world::incremental_module_execution_event::PersistedWorldEventV1::from_core_action(&completed);
            persisted.validate(step.transaction).unwrap();
            assert_eq!(
                persisted.event().caused_by,
                Some(crate::CausedBy::Action(selected.envelope().id))
            );
        } else {
            assert!(
                crate::world::incremental_action_execution_core::consume_bootstrap_action(
                    &mut step,
                    &mut materials,
                    &mut pending,
                    &mut read,
                    &mut pending_work,
                    &mut originals,
                    fixture.current.raw().agents(),
                    fixture.current.runtime().module_authorization(),
                    fixture.current.runtime().config(),
                    fixture.current.runtime().revocation(),
                    &mut json,
                    &mut scan,
                    &mut policy_work,
                    &mut entries,
                    64 << 10,
                    &live,
                    &mut persist,
                )
                .unwrap()
            );
        }
    }
    assert_eq!(step.journal_height().unwrap(), count as u64);
    let actual_events: Vec<_> = events
        .iter()
        .map(|event| oasis7_wasm_abi::encode_canonical_cbor(event).unwrap())
        .collect();
    assert_eq!(actual_events, expected_events);
    assert!(
        matches!(&events[0].body,crate::WorldEventBody::Domain(crate::DomainEvent::AgentRegistered{agent_id,..}) if agent_id=="queued-alice")
    );
    assert!(
        matches!(&events[1].body,crate::WorldEventBody::Domain(crate::DomainEvent::AgentMoved{from,to,..}) if from.x_cm==1&&from.y_cm==2&&from.z_cm==3&&to.x_cm==11&&to.y_cm==22&&to.z_cm==33)
    );
    drop(step);
    // PostAction may append further WorldEvents. Recover the last genuine
    // counted producer original, rather than retaining the action's first result.
    {
        use crate::world::incremental_world_event_tail::*;
        let mut work = CurrentWorldEventTailWorkV1 {
            originals_remaining: 1,
            original_bytes_remaining: 1 << 20,
            decode: oasis7_world_state::incremental_world_event_decode::WorldEventDecodeWorkV1 {
                structure: oasis7_proto::cbor_decode_preflight::CborPreflightWorkV1 {
                    bytes_remaining: 8 << 20,
                    values_remaining: 100000,
                    max_depth: 32,
                },
                canonical: genesis::canonical(),
                max_original_bytes: 1 << 20,
                max_heap_bytes: 32 << 20,
            },
        };
        let recovered = recover_latest_counted_world_event_v1(
            &counter,
            &mut tx,
            &mut originals,
            &memory,
            expected_time + 1,
            &mut work,
            || false,
            &mut || live.clone(),
        )
        .unwrap()
        .unwrap();
        let persisted = crate::world::incremental_module_execution_event::PersistedWorldEventV1::from_counted_tail(&recovered);
        persisted.validate(&tx).unwrap();
        assert_eq!(
            oasis7_wasm_abi::encode_canonical_cbor(persisted.event()).unwrap(),
            *expected_events.last().unwrap()
        );
        assert_eq!(work.originals_remaining, 0);
    }
    // Independent current getter detects an event-only implementation that
    // emits AgentMoved but fails to mutate the actual position leaves.
    {
        use oasis7_world_state::incremental_agent_fields::AgentFieldV1;
        use oasis7_world_state::incremental_agent_subtree::*;
        let mut overlay = WorldFieldOverlayContextV1 {
            transaction: &mut tx,
            values: ValueReadContext {
                source: &reader,
                memory: &memory,
                class: MemoryClass::Ordinary,
                io: &mut *read.values.io,
            },
            key_work: &mut *read.key_work,
            semantic_bytes: 4096,
            record_bytes: 16 << 20,
            take_state_error: || reader.cas.error(),
            cancelled: || false,
        };
        for (field, expected) in [
            (AgentFieldV1::PositionX, 11),
            (AgentFieldV1::PositionY, 22),
            (AgentFieldV1::PositionZ, 33),
        ] {
            let guard = overlay.transaction.guard();
            let actual = read_agent_subtree_scalar_overlay_v1(
                AgentSubtreeQualifiedRequestV1 {
                    cell: AgentSubtreeScalarRequestV1 {
                        agent: "queued-alice",
                        field,
                        ordinal: 0,
                    },
                    qualification: fixture.current.raw().agents(),
                },
                &mut overlay,
                16 << 20,
                &mut std::io::sink(),
                &guard,
                &live,
            )
            .unwrap();
            assert!(matches!(actual,Some(AgentSubtreeValueV1::I64(v)) if v==expected));
        }
    }
    let head = oasis7_world_state::incremental_pending_action_overlay::peek_pending_action_v1(
        &pending,
        &mut tx,
        &memory,
        &live,
        &mut pending_work.index,
        || false,
    )
    .unwrap();
    assert!(head.is_none());
    let guard = tx.guard();
    let prepared = tx.seal(&guard, &live, fixture.roots.height + 1).unwrap();
    assert_eq!(
        prepared
            .record
            .facts
            .iter()
            .filter(|f| f.kind == ExecutionFactKindV1::Event)
            .count(),
        count
    );
    let actual_inputs: Vec<_> = prepared
        .record
        .facts
        .iter()
        .filter(|f| f.kind == ExecutionFactKindV1::Input)
        .map(|f| f.id)
        .collect();
    assert_eq!(actual_inputs, expected_inputs);
    assert_eq!(
        prepared.record.facts.len(),
        2 * count,
        "one authenticated input and one business event per consumed action"
    );
}
