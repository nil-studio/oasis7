use super::*;
use crate::{Action, WorldState};
fn fixture() -> (World, String) {
    fixture_with_formal_history(false)
}
fn fixture_with_formal_history(formal: bool) -> (World, String) {
    let private = hex::encode([71u8; 32]);
    let public = sign_read_request("fixture", (), &private)
        .unwrap()
        .subject_public_key;
    let mut world = World::new_with_state(WorldState::default())
        .try_with_cognition_scheduler(
            crate::SchedulerPolicyV1 {
                schema_version: crate::SchedulerPolicyV1::SCHEMA_VERSION.into(),
                max_total_wakes_per_tick: 1,
                max_wakes_per_agent_per_tick: 1,
                aging_after_ticks: 1,
                max_starvation_ticks: 1,
                initial_priority: 0,
                comparator: crate::SchedulerPolicyV1::COMPARATOR.into(),
                service_order: crate::SchedulerPolicyV1::SERVICE_ORDER.into(),
            },
            1,
        )
        .unwrap();
    if formal {
        // Bind the fresh World before any task/history publication, then install C0.
        let snapshot = world.snapshot_with_chain_resource_context(
            crate::ChainResourceDerivationContext {
                world_id: "world",
                chain_id: "fixture-chain",
                genesis_ref: Some("fixture"),
                created_at_height: 0,
                manifest_height: 0,
                commit_block_hash: None,
                tick: 0,
            },
            "fixture",
            "fixture",
        );
        world = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
        world.initialize_empty_formal_history().unwrap();
        struct EmptyHistoryNodes;
        impl oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexNodeReader
            for EmptyHistoryNodes
        {
            fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
                Ok(None)
            }
        }
        let root = world.state().retained_history_root.clone().unwrap();
        let snapshot_bytes = serde_cbor::to_vec(&world.snapshot()).unwrap();
        let parent =
            crate::VerifiedFormalRetainedHistoryParentV1::for_test_snapshot(root, &snapshot_bytes);
        world
            .attach_verified_formal_history(parent, std::sync::Arc::new(EmptyHistoryNodes))
            .unwrap();
    }
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
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
    world
        .bind_cognition_runtime("world", "main", 0, None, "pending", 0)
        .unwrap();
    (world, private)
}
fn request(private: &str, id: &str) -> SubmitIntentRequest<WorldServicePayloadV1> {
    let create = GameTaskCreateV1 {
        api_rev: 1,
        client_operation_id: id.into(),
        world_id: "world".into(),
        genesis_digest: "fixture".into(),
        branch_id: "main".into(),
        reorg_epoch: DecimalU64(0),
        agent_id: "agent".into(),
        owner_binding: "owner".into(),
        owner_generation: DecimalU64(1),
    };
    let payload = WorldServicePayloadV1::GameTaskCreate(
        sign_read_request(GAME_TASK_CREATE_DOMAIN, create, private).unwrap(),
    );
    SubmitIntentRequest {
        contract_version: 1,
        correlation: derive_correlation(
            WorldIdentity {
                world_id: "world".into(),
                genesis_digest: "fixture".into(),
            },
            &payload,
        )
        .unwrap(),
        deadline_unix_ms: None,
        signed_payload: payload,
    }
}
fn result(
    world: &World,
    request: SubmitIntentRequest<WorldServicePayloadV1>,
) -> CanonicalIntentResultV1 {
    CanonicalIntentResultV1 {
        receipt: prepare_create(world, &request, []).unwrap(),
        request,
        action_id: 1,
        committed_height: 1,
        rejected: None,
    }
}
#[test]
fn game_task_bootstrap_persists_suspended_and_rejects_conflicting_create() {
    let (mut world, private) = fixture();
    let before = serde_json::to_value(world.snapshot()).unwrap();
    let first = result(&world, request(&private, "first"));
    assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
    let second = request(&private, "second");
    assert!(
        prepare_create(&world, &second, [&first])
            .unwrap_err()
            .contains("within block")
    );
    world.record_world_service_result(first.clone()).unwrap();
    world.record_world_service_result(first).unwrap(); // immutable exact publication replay
    assert!(
        prepare_create(&world, &second, [])
            .unwrap_err()
            .contains("already exists")
    );
    let snapshot = goal_snapshot(&world);
    let key = correlation::key_digest(&request(&private, "first").correlation.key).unwrap();
    let mut corrupt_snapshot = serde_json::to_value(&snapshot).unwrap();
    fn corrupt(value: &mut serde_json::Value, key: &str) -> bool {
        match value {
            serde_json::Value::Object(map) => {
                if let Some(result) = map.get_mut(key) {
                    result["receipt"]["executor_epoch"] = "9".into();
                    return true;
                }
                map.values_mut().any(|child| corrupt(child, key))
            }
            serde_json::Value::Array(values) => values.iter_mut().any(|child| corrupt(child, key)),
            _ => false,
        }
    }
    assert!(corrupt(&mut corrupt_snapshot, &key));
    assert!(
        World::from_snapshot(
            serde_json::from_value(corrupt_snapshot).unwrap(),
            world.journal().clone()
        )
        .is_err()
    );
    let restored = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
    let control = current_control(&restored, "agent").unwrap().unwrap();
    assert_eq!(control.task_status, GameTaskStatusV1::Suspended);
    assert_eq!(control.executor_epoch, DecimalU64(0));
    assert_eq!(control.active_goal_revision, DecimalU64(0));
    assert!(control.executor_id.is_none());
    assert_eq!(control.created_operation_id, "first");
}
#[test]
fn game_task_owner_history_and_receipt_integrity_fail_closed() {
    let (mut world, private) = fixture();
    let original = request(&private, "first");
    for variant in 0..6 {
        let mut invalid = original.clone();
        let WorldServicePayloadV1::GameTaskCreate(signed) = &mut invalid.signed_payload else {
            unreachable!()
        };
        match variant {
            0 => signed.request.world_id = "foreign".into(),
            1 => signed.request.branch_id = "fork".into(),
            2 => signed.request.reorg_epoch = DecimalU64(1),
            3 => signed.request.owner_binding = "other".into(),
            4 => signed.request.owner_generation = DecimalU64(2),
            _ => signed.subject_public_key = "00".repeat(32),
        }
        if variant < 5 {
            *signed = sign_read_request(GAME_TASK_CREATE_DOMAIN, signed.request.clone(), &private)
                .unwrap();
        }
        assert!(validate_owner(&world, signed).is_err());
    }
    let mut corrupt = result(&world, original.clone());
    corrupt.receipt["executor_epoch"] = "1".into();
    assert!(world.record_world_service_result(corrupt).is_err());
    let mut foreign = original.clone();
    let WorldServicePayloadV1::GameTaskCreate(signed) = &mut foreign.signed_payload else {
        unreachable!()
    };
    signed.request.genesis_digest = "foreign".into();
    *signed = sign_read_request(GAME_TASK_CREATE_DOMAIN, signed.request.clone(), &private).unwrap();
    assert!(
        derive_correlation(
            original.correlation.key.world.clone(),
            &foreign.signed_payload
        )
        .is_err()
    );
    let mut changed = original.clone();
    let WorldServicePayloadV1::GameTaskCreate(signed) = &mut changed.signed_payload else {
        unreachable!()
    };
    signed.request.branch_id = "other".into();
    *signed = sign_read_request(GAME_TASK_CREATE_DOMAIN, signed.request.clone(), &private).unwrap();
    let changed_correlation = derive_correlation(
        original.correlation.key.world.clone(),
        &changed.signed_payload,
    )
    .unwrap();
    assert_eq!(changed_correlation.key, original.correlation.key);
    assert_ne!(
        changed_correlation.payload_digest,
        original.correlation.payload_digest
    );
    let accepted = result(&world, original);
    world.record_world_service_result(accepted).unwrap();
    world.invalidate_cognition_for_reorg(1).unwrap();
    world
        .bind_cognition_runtime("world", "fork", 1, None, "pending", 1)
        .unwrap();
    assert!(current_control(&world, "agent").unwrap().is_none());
    let mut fresh = request(&private, "after-fork");
    let WorldServicePayloadV1::GameTaskCreate(signed) = &mut fresh.signed_payload else {
        unreachable!()
    };
    signed.request.branch_id = "fork".into();
    signed.request.reorg_epoch = DecimalU64(1);
    *signed = sign_read_request(GAME_TASK_CREATE_DOMAIN, signed.request.clone(), &private).unwrap();
    fresh.correlation =
        derive_correlation(fresh.correlation.key.world.clone(), &fresh.signed_payload).unwrap();
    let accepted = result(&world, fresh);
    world.record_world_service_result(accepted).unwrap();
    assert_eq!(
        current_control(&world, "agent")
            .unwrap()
            .unwrap()
            .created_operation_id,
        "after-fork"
    );
}

fn goal_request(
    private: &str,
    c: &GameTaskControlV1,
    id: &str,
) -> SubmitIntentRequest<WorldServicePayloadV1> {
    let goal = GameGoalPublishV1 {
        api_rev: 1,
        world_id: c.world_id.clone(),
        genesis_digest: c.genesis_digest.clone(),
        task_id: c.task_id.clone(),
        agent_id: c.agent_id.clone(),
        owner_public_key: c.owner_public_key.clone(),
        owner_binding: c.owner_binding.clone(),
        owner_generation: c.owner_generation,
        branch_id: c.branch_id.clone(),
        reorg_epoch: c.reorg_epoch,
        expected_task_revision: c.task_revision,
        expected_latest_goal_revision: c.latest_goal_revision,
        goal_revision: DecimalU64(c.latest_goal_revision.0 + 1),
        client_operation_id: id.into(),
        prepare_operation_id: format!("prepare-{id}"),
        goal_ref: "ab".repeat(32),
    };
    let payload = WorldServicePayloadV1::GameGoalPublish(
        sign_read_request(GAME_GOAL_PUBLISH_DOMAIN, goal, private).unwrap(),
    );
    SubmitIntentRequest {
        contract_version: 1,
        correlation: derive_correlation(
            WorldIdentity {
                world_id: c.world_id.clone(),
                genesis_digest: c.genesis_digest.clone(),
            },
            &payload,
        )
        .unwrap(),
        deadline_unix_ms: None,
        signed_payload: payload,
    }
}
fn goal_result(
    world: &World,
    request: SubmitIntentRequest<WorldServicePayloadV1>,
    staged: &[&CanonicalIntentResultV1],
    height: u64,
) -> CanonicalIntentResultV1 {
    CanonicalIntentResultV1 {
        receipt: prepare_publish(world, &request, staged.iter().copied(), height).unwrap(),
        request,
        action_id: 2,
        committed_height: height,
        rejected: None,
    }
}
#[test]
fn game_goal_publish_staged_create_activation_restart_and_exact_receipt() {
    let (mut world, private) = fixture();
    let create = result(&world, request(&private, "goal-task"));
    let initial = create_control(&create.request).unwrap();
    assert!(
        serde_json::to_value(&initial)
            .unwrap()
            .get("active_goal_reference")
            .is_none()
    );
    let publication = goal_request(&private, &initial, "first-goal");
    let first = goal_result(&world, publication.clone(), &[&create], 7);
    assert!(
        prepare_publish(
            &world,
            &goal_request(&private, &initial, "conflict"),
            [&create, &first],
            7
        )
        .is_err()
    );
    world.record_world_service_result(create).unwrap();
    world.record_world_service_result(first.clone()).unwrap();
    let control = current_control(&world, "agent").unwrap().unwrap();
    assert_eq!(control.task_revision, DecimalU64(2));
    assert_eq!(control.active_goal_revision, DecimalU64(1));
    assert_eq!(control.latest_goal_revision, DecimalU64(1));
    assert_eq!(control.task_status, GameTaskStatusV1::Suspended);
    assert_eq!(control.executor_epoch, DecimalU64(0));
    assert!(control.executor_id.is_none());
    assert_eq!(active_publish(&world, "agent").unwrap().unwrap(), first);
    let active = control.active_goal_reference.as_ref().unwrap();
    assert_eq!(active.activation_committed_height, DecimalU64(7));
    assert_eq!(active.prepare_operation_id, "prepare-first-goal");
    assert_eq!(
        derive_correlation(
            publication.correlation.key.world.clone(),
            &publication.signed_payload
        )
        .unwrap(),
        publication.correlation
    );
    let second = goal_result(
        &world,
        goal_request(&private, &control, "second-goal"),
        &[],
        8,
    );
    world.record_world_service_result(second.clone()).unwrap();
    let snapshot = goal_snapshot(&world);
    let restored = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
    assert_eq!(
        current_control(&restored, "agent")
            .unwrap()
            .unwrap()
            .active_goal_revision,
        DecimalU64(2)
    );
    assert_eq!(active_publish(&restored, "agent").unwrap().unwrap(), second);
}
#[test]
fn game_goal_publish_signature_fences_and_body_independence() {
    let (mut world, private) = fixture();
    let create = result(&world, request(&private, "task"));
    let initial = create_control(&create.request).unwrap();
    world.record_world_service_result(create).unwrap();
    let original = goal_request(&private, &initial, "goal");
    let WorldServicePayloadV1::GameGoalPublish(signed) = &original.signed_payload else {
        unreachable!()
    };
    for field in 0..13 {
        let mut bad = signed.clone();
        let r = &mut bad.request;
        match field {
            0 => r.world_id.push('x'),
            1 => r.genesis_digest.push('x'),
            2 => r.task_id.push('x'),
            3 => r.agent_id.push('x'),
            4 => r.owner_binding.push('x'),
            5 => r.owner_generation.0 += 1,
            6 => r.branch_id.push('x'),
            7 => r.reorg_epoch.0 += 1,
            8 => r.expected_task_revision.0 += 1,
            9 => {
                r.expected_latest_goal_revision.0 += 1;
                r.goal_revision.0 += 1;
            }
            10 => r.prepare_operation_id.push('x'),
            11 => r.goal_ref = "cd".repeat(32),
            _ => r.client_operation_id.push('x'),
        };
        r.validate().unwrap();
        let edited = r.clone();
        assert!(verify_publish(&bad).is_err());
        let valid_signature =
            sign_read_request(GAME_GOAL_PUBLISH_DOMAIN, edited, &private).unwrap();
        let payload = WorldServicePayloadV1::GameGoalPublish(valid_signature);
        if let Ok(correlation) =
            derive_correlation(original.correlation.key.world.clone(), &payload)
        {
            let changed = SubmitIntentRequest {
                signed_payload: payload,
                correlation,
                ..original.clone()
            };
            if field < 10 {
                assert!(prepare_publish(&world, &changed, [], 2).is_err());
            }
        }
    }
    // A correctly signed metadata reference can activate without a local body.
    assert!(prepare_publish(&world, &original, [], 2).is_ok());
    let mut weak = signed.clone();
    weak.subject_public_key = "01".to_owned() + &"00".repeat(31);
    weak.request.owner_public_key = weak.subject_public_key.clone();
    weak.signature_hex = weak.subject_public_key.clone() + &"00".repeat(32);
    assert!(verify_publish(&weak).is_err());
    world
        .install_capability_agent_identity("agent", "owner", 2)
        .unwrap();
    assert!(prepare_publish(&world, &original, [], 2).is_err());
    assert!(current_control(&world, "agent").unwrap().is_none());
}
#[test]
fn game_goal_publish_restore_rejects_missing_predecessor_and_double_version() {
    let (mut world, private) = fixture();
    let create = result(&world, request(&private, "task"));
    let initial = create_control(&create.request).unwrap();
    world.record_world_service_result(create).unwrap();
    let mut gap = initial.clone();
    gap.task_revision = DecimalU64(2);
    gap.latest_goal_revision = DecimalU64(1);
    let gap_request = goal_request(&private, &gap, "gap");
    let broken = CanonicalIntentResultV1 {
        receipt: serde_json::to_value(publish_receipt(&gap_request, 3).unwrap()).unwrap(),
        request: gap_request,
        action_id: 2,
        committed_height: 3,
        rejected: None,
    };
    world.record_world_service_result(broken).unwrap();
    assert!(validate_history(&world).is_err());
    let error = match World::from_snapshot(goal_snapshot(&world), world.journal().clone()) {
        Err(error) => error,
        Ok(_) => panic!("corrupted canonical history must fail restore"),
    };
    assert!(format!("{error:?}").contains("goal publication predecessor"));
    let (mut world, private) = fixture();
    let create = result(&world, request(&private, "task"));
    let initial = create_control(&create.request).unwrap();
    world.record_world_service_result(create).unwrap();
    let a = goal_result(&world, goal_request(&private, &initial, "a"), &[], 2);
    let b_request = goal_request(&private, &initial, "b");
    let b = CanonicalIntentResultV1 {
        receipt: serde_json::to_value(publish_receipt(&b_request, 2).unwrap()).unwrap(),
        request: b_request,
        action_id: 3,
        committed_height: 2,
        rejected: None,
    };
    world.record_world_service_result(a).unwrap();
    world.record_world_service_result(b).unwrap();
    let error = match World::from_snapshot(goal_snapshot(&world), world.journal().clone()) {
        Err(error) => error,
        Ok(_) => panic!("corrupted canonical history must fail restore"),
    };
    assert!(format!("{error:?}").contains("goal publication predecessor"));
}

#[test]
fn game_goal_publish_restore_rejects_invalid_journal_tail() {
    for missing_predecessor in [true, false] {
        let (mut world, private) = fixture();
        let create = result(&world, request(&private, "task"));
        let initial = create_control(&create.request).unwrap();
        world.record_world_service_result(create).unwrap();
        let mut predecessor = initial.clone();
        if missing_predecessor {
            predecessor.task_revision = DecimalU64(2);
            predecessor.latest_goal_revision = DecimalU64(1);
        } else {
            let accepted =
                goal_result(&world, goal_request(&private, &initial, "accepted"), &[], 2);
            world.record_world_service_result(accepted).unwrap();
        }
        // The checkpoint is valid. Only the journal tail introduces an individually
        // valid signed receipt with an absent or already-consumed predecessor.
        validate_history(&world).unwrap();
        let checkpoint = goal_snapshot(&world);
        let signed = goal_request(&private, &predecessor, "invalid-tail");
        let tail = CanonicalIntentResultV1 {
            receipt: serde_json::to_value(publish_receipt(&signed, 3).unwrap()).unwrap(),
            request: signed,
            action_id: 3,
            committed_height: 3,
            rejected: None,
        };
        world.record_world_service_result(tail).unwrap();
        assert!(world.journal().len() > checkpoint.journal_len);
        let error = match World::from_snapshot(checkpoint, world.journal().clone()) {
            Err(error) => error,
            Ok(_) => panic!("invalid goal publication journal tail must fail restore"),
        };
        assert!(format!("{error:?}").contains("goal publication predecessor"));
    }
}

#[test]
fn game_goal_publish_restore_accepts_valid_create_publish_journal_tail() {
    let (mut world, private) = fixture();
    let checkpoint = goal_snapshot(&world);
    let create = result(&world, request(&private, "task"));
    let initial = create_control(&create.request).unwrap();
    world.record_world_service_result(create).unwrap();
    let accepted = goal_result(&world, goal_request(&private, &initial, "accepted"), &[], 2);
    world.record_world_service_result(accepted.clone()).unwrap();
    let recovered = World::from_snapshot(checkpoint, world.journal().clone()).unwrap();
    assert_eq!(
        current_control(&recovered, "agent").unwrap(),
        current_control(&world, "agent").unwrap()
    );
    assert_eq!(active_publish(&recovered, "agent").unwrap(), Some(accepted));
}

#[test]
fn game_goal_publish_restore_rejects_orphan_journal_tail() {
    let (mut world, private) = fixture();
    let checkpoint = goal_snapshot(&world);
    let create = result(&world, request(&private, "task"));
    let initial = create_control(&create.request).unwrap();
    // No create receipt exists in either the checkpoint or the tail.
    let signed = goal_request(&private, &initial, "orphan");
    world
        .record_world_service_result(CanonicalIntentResultV1 {
            receipt: serde_json::to_value(publish_receipt(&signed, 2).unwrap()).unwrap(),
            request: signed,
            action_id: 2,
            committed_height: 2,
            rejected: None,
        })
        .unwrap();
    let error = match World::from_snapshot(checkpoint, world.journal().clone()) {
        Err(error) => error,
        Ok(_) => panic!("orphan goal publication journal tail must fail restore"),
    };
    assert!(format!("{error:?}").contains("goal publication has no canonical task predecessor"));
}

#[test]
fn retention_history_matches_current_task_selection_across_agent_and_owner_changes() {
    let (mut world, private) = fixture();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-b".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(1, 0, 0),
    });
    world.step().unwrap();

    let create = result(&world, request(&private, "retention-task"));
    let initial = create_control(&create.request).unwrap();
    world.record_world_service_result(create).unwrap();
    let publication = goal_result(
        &world,
        goal_request(&private, &initial, "retention-goal"),
        &[],
        3,
    );
    world.record_world_service_result(publication).unwrap();

    let view = validate_retention_history(&world).unwrap();
    for agent in ["agent", "agent-b", "agent-without-task"] {
        assert_eq!(
            view.active_publish(agent).unwrap(),
            active_publish(&world, agent).unwrap(),
            "batched current publication differs for {agent}"
        );
    }
    assert!(view.retained_prefixes().next().is_none());

    world
        .install_capability_agent_identity("agent", "owner", 2)
        .unwrap();
    let transferred = validate_retention_history(&world).unwrap();
    assert_eq!(
        transferred.active_publish("agent").unwrap(),
        active_publish(&world, "agent").unwrap(),
        "owner-generation replacement must remove the stale task publication"
    );
    assert!(transferred.active_publish("agent").unwrap().is_none());

    world.invalidate_cognition_for_reorg(1).unwrap();
    world
        .bind_cognition_runtime("world", "fork", 1, None, "pending", 1)
        .unwrap();
    let forked = validate_retention_history(&world).unwrap();
    assert_eq!(
        forked.active_publish("agent").unwrap(),
        active_publish(&world, "agent").unwrap(),
        "reorg history must use the same current-owner selection"
    );
    assert!(forked.active_publish("agent").unwrap().is_none());
}

#[test]
fn retention_history_validates_rejected_unrelated_records_before_filtering() {
    let (mut world, private) = fixture();
    let payload = WorldServicePayloadV1::Scheduler(
        sign_read_request(
            "scheduler",
            SchedulerIntentV1 {
                agent_id: "agent".into(),
                request_id: "rejected-unrelated".into(),
                delegation_generation: 1,
                captured_base_binding: world.current_cognition_base_binding().unwrap(),
                operation: SchedulerOperationV1::ReleaseLease {
                    lease_id: "missing-lease".into(),
                },
            },
            &private,
        )
        .unwrap(),
    );
    let request = SubmitIntentRequest {
        contract_version: 1,
        correlation: derive_correlation(
            WorldIdentity {
                world_id: "world".into(),
                genesis_digest: "fixture".into(),
            },
            &payload,
        )
        .unwrap(),
        signed_payload: payload,
        deadline_unix_ms: None,
    };
    world
        .record_world_service_result(CanonicalIntentResultV1 {
            request,
            action_id: 1,
            committed_height: 1,
            receipt: serde_json::Value::Null,
            rejected: Some("lease_not_found".into()),
        })
        .unwrap();
    let history = validate_retention_history(&world).unwrap();
    assert!(history.retained_prefixes().next().is_none());

    let mut corrupted = world
        .capability_revocation_state()
        .world_service_results
        .clone();
    let record = corrupted.values_mut().next().unwrap();
    assert_eq!(record["rejected"], "lease_not_found");
    record["committed_height"] = 0.into();
    assert!(validate_retention_history_results(&world, &corrupted).is_err());
}

#[test]
fn retention_history_rejects_duplicate_canonical_owner_scope() {
    let (mut world, private) = fixture();
    let first = result(&world, request(&private, "same-owner-scope-first"));
    let second = result(&world, request(&private, "same-owner-scope-second"));
    world.record_world_service_result(first).unwrap();
    world.record_world_service_result(second).unwrap();

    let error = validate_retention_history(&world).err().unwrap();
    assert!(error.contains("duplicate canonical task owner/history scope"));
    let restore_error = World::from_snapshot(goal_snapshot(&world), world.journal().clone())
        .err()
        .unwrap();
    assert!(matches!(
        restore_error,
        crate::WorldError::CapabilityAuthorizationDenied { ref reason }
            if reason.contains("duplicate canonical task owner/history scope")
    ));
}

#[test]
fn world_snapshot_restore_rejects_corrupt_non_task_result_identity() {
    let (mut world, private) = fixture();
    let payload = WorldServicePayloadV1::Scheduler(
        sign_read_request(
            "scheduler",
            SchedulerIntentV1 {
                agent_id: "agent".into(),
                request_id: "restore-corrupt-non-task".into(),
                delegation_generation: 1,
                captured_base_binding: world.current_cognition_base_binding().unwrap(),
                operation: SchedulerOperationV1::ReleaseLease {
                    lease_id: "missing-lease".into(),
                },
            },
            &private,
        )
        .unwrap(),
    );
    let request = SubmitIntentRequest {
        contract_version: 1,
        correlation: derive_correlation(
            WorldIdentity {
                world_id: "world".into(),
                genesis_digest: "fixture".into(),
            },
            &payload,
        )
        .unwrap(),
        signed_payload: payload,
        deadline_unix_ms: None,
    };
    let key = correlation::key_digest(&request.correlation.key).unwrap();
    world
        .record_world_service_result(CanonicalIntentResultV1 {
            request,
            action_id: 1,
            committed_height: 1,
            receipt: serde_json::Value::Null,
            rejected: Some("lease_not_found".into()),
        })
        .unwrap();
    let snapshot = goal_snapshot(&world);
    assert!(World::from_snapshot(snapshot.clone(), world.journal().clone()).is_ok());
    let assert_restore_denied = |candidate| {
        let error = World::from_snapshot(candidate, world.journal().clone())
            .err()
            .unwrap();
        assert!(matches!(
            error,
            crate::WorldError::CapabilityAuthorizationDenied { .. }
        ));
    };

    let mut invalid_signature = snapshot.clone();
    invalid_signature
        .capability_revocation_state
        .world_service_results
        .get_mut(&key)
        .unwrap()["request"]["signed_payload"]["payload"]["signature_hex"] = "00".repeat(64).into();
    assert!(
        validate_retention_history_results(
            &world,
            &invalid_signature
                .capability_revocation_state
                .world_service_results
        )
        .is_err()
    );
    assert_restore_denied(invalid_signature);

    let mut invalid_payload_digest = snapshot.clone();
    invalid_payload_digest
        .capability_revocation_state
        .world_service_results
        .get_mut(&key)
        .unwrap()["request"]["correlation"]["payload_digest"] = "00".repeat(32).into();
    assert!(
        validate_retention_history_results(
            &world,
            &invalid_payload_digest
                .capability_revocation_state
                .world_service_results
        )
        .is_err()
    );
    assert_restore_denied(invalid_payload_digest);

    let mut invalid_outer_key = snapshot;
    let record = invalid_outer_key
        .capability_revocation_state
        .world_service_results
        .remove(&key)
        .unwrap();
    invalid_outer_key
        .capability_revocation_state
        .world_service_results
        .insert("wrong-outer-key".into(), record);
    assert!(
        validate_retention_history_results(
            &world,
            &invalid_outer_key
                .capability_revocation_state
                .world_service_results
        )
        .is_err()
    );
    assert_restore_denied(invalid_outer_key);
}

fn goal_snapshot(world: &World) -> crate::Snapshot {
    world.snapshot_with_chain_resource_context(
        crate::ChainResourceDerivationContext {
            world_id: "world",
            chain_id: "fixture-chain",
            genesis_ref: None,
            created_at_height: world.state().time,
            manifest_height: world.state().time,
            commit_block_hash: None,
            tick: world.state().time,
        },
        "fixture",
        "fixture",
    )
}

#[path = "tests/executor.rs"]
mod executor_tests;

#[derive(Clone, Default)]
struct TaskHistoryNodes(std::sync::Arc<std::sync::Mutex<BTreeMap<String, Vec<u8>>>>);
impl oasis7_distfs::controlled_authority::formal::business_index::BusinessIndexNodeReader
    for TaskHistoryNodes
{
    fn read_node(&self, digest: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.0.lock().unwrap().get(digest).cloned())
    }
}
fn archive_task_history(world: &mut World) -> TaskHistoryNodes {
    let nodes = TaskHistoryNodes::default();
    let parent = crate::VerifiedFormalRetainedHistoryParentV1::for_test_snapshot(
        world.state().retained_history_root.clone().unwrap(),
        &serde_cbor::to_vec(&world.snapshot()).unwrap(),
    );
    world
        .attach_verified_formal_history(parent, std::sync::Arc::new(nodes.clone()))
        .unwrap();
    let plan = world.prepare_formal_retention_candidate().unwrap();
    let delta = plan.delta.as_ref().unwrap();
    for node in &delta.node_writes {
        nodes
            .0
            .lock()
            .unwrap()
            .insert(node.digest.clone(), node.bytes.clone());
    }
    world.apply_formal_retention_plan(&plan).unwrap();
    let parent = crate::VerifiedFormalRetainedHistoryParentV1::for_test_snapshot(
        world.state().retained_history_root.clone().unwrap(),
        &serde_cbor::to_vec(&world.snapshot()).unwrap(),
    );
    world
        .attach_verified_formal_history(parent, std::sync::Arc::new(nodes.clone()))
        .unwrap();
    nodes
}

#[test]
fn game_goal_cold_revision_chain_restores_and_accepts_next_publication() {
    let (mut world, private) = fixture_with_formal_history(true);
    let create = result(&world, request(&private, "retention-task"));
    world.record_world_service_result(create).unwrap();
    for revision in 0..40 {
        let control = current_control(&world, "agent").unwrap().unwrap();
        let published = goal_result(
            &world,
            goal_request(&private, &control, &format!("retention-goal-{revision}")),
            &[],
            revision + 2,
        );
        world.record_world_service_result(published).unwrap();
    }
    let expected = reduce(collect(&world, []).unwrap()).unwrap()
        [&current_control(&world, "agent").unwrap().unwrap().task_id]
        .control
        .clone();
    let nodes = archive_task_history(&mut world);
    assert!(
        world
            .capability_revocation_state()
            .world_service_results
            .len()
            < 41
    );
    assert_eq!(current_control(&world, "agent").unwrap().unwrap(), expected);
    let parent = crate::VerifiedFormalRetainedHistoryParentV1::for_test_snapshot(
        world.state().retained_history_root.clone().unwrap(),
        &serde_cbor::to_vec(&world.snapshot()).unwrap(),
    );
    let mut restored = World::from_verified_formal_snapshot(
        world.snapshot(),
        world.journal().clone(),
        parent,
        std::sync::Arc::new(nodes.clone()),
    )
    .unwrap();
    assert_eq!(
        current_control(&restored, "agent").unwrap().unwrap(),
        expected
    );
    let next = goal_result(
        &restored,
        goal_request(&private, &expected, "after-cold"),
        &[],
        43,
    );
    restored.record_world_service_result(next).unwrap();
    assert_eq!(
        current_control(&restored, "agent")
            .unwrap()
            .unwrap()
            .latest_goal_revision
            .0,
        41
    );
    nodes.0.lock().unwrap().clear();
    assert!(
        current_control(&world, "agent").is_err(),
        "missing cold history must fail closed"
    );
}

#[test]
fn game_task_canonical_empty_retained_checkpoint_restores_hot_chain_without_reader() {
    let (mut world, private) = fixture_with_formal_history(true);
    let create = result(&world, request(&private, "empty-history-task"));
    let initial = create_control(&create.request).unwrap();
    world.record_world_service_result(create).unwrap();
    let publication = goal_result(
        &world,
        goal_request(&private, &initial, "empty-history-goal"),
        &[],
        2,
    );
    world
        .record_world_service_result(publication.clone())
        .unwrap();
    let expected = current_control(&world, "agent").unwrap();
    // Restoration intentionally has no process-local CAS reader. The exact
    // canonical empty commitment proves all task results are still hot.
    let restored = World::from_snapshot(world.snapshot(), world.journal().clone()).unwrap();
    assert_eq!(current_control(&restored, "agent").unwrap(), expected);
    assert_eq!(
        active_publish(&restored, "agent").unwrap(),
        Some(publication)
    );
}
