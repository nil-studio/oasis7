use super::*;
use crate::service_admission::{self as service, CanonicalIntentResultV1, WorldServicePayloadV1};
use crate::tests::agent_cognition_runtime_hardening::continuous_commit_request;

fn service_request(
    payload: WorldServicePayloadV1,
) -> service::SubmitIntentRequest<WorldServicePayloadV1> {
    service::SubmitIntentRequest {
        contract_version: 1,
        correlation: service::derive_correlation(
            service::WorldIdentity {
                world_id: "result-world".into(),
                genesis_digest: "result-genesis".into(),
            },
            &payload,
        )
        .unwrap(),
        deadline_unix_ms: None,
        signed_payload: payload,
    }
}
fn record(
    world: &mut World,
    request: service::SubmitIntentRequest<WorldServicePayloadV1>,
    receipt: serde_json::Value,
) {
    world
        .record_world_service_result(CanonicalIntentResultV1 {
            request,
            action_id: 100,
            committed_height: world.state().time,
            receipt,
            rejected: None,
        })
        .unwrap();
}

fn execute(expired: bool) -> World {
    let secret = hex::encode([71u8; 32]);
    let key = service::sign_read_request("test", (), &secret)
        .unwrap()
        .subject_public_key;
    let mut state = industrial_fixture();
    state.time = 23;
    state
        .starter_oc_claims
        .get_mut(AGENT_ID)
        .unwrap()
        .public_key = Some(key.clone());
    let mut world = World::new_with_state(state);
    world
        .install_capability_agent_identity(AGENT_ID, "player", 1)
        .unwrap();
    let snapshot = world.snapshot_with_chain_resource_context(
        crate::ChainResourceDerivationContext {
            world_id: "result-world",
            chain_id: "result-chain",
            genesis_ref: Some("result-genesis"),
            created_at_height: 1,
            manifest_height: 1,
            commit_block_hash: None,
            tick: 23,
        },
        "a".repeat(64),
        "b".repeat(64),
    );
    world = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
    let q = world
        .state()
        .agent_manufacturing_quote_with_binding(
            AGENT_ID,
            "assembly",
            23,
            world.world_rule_binding().as_deref(),
        )
        .unwrap();
    if expired {
        for _ in 0..4 {
            world.step().unwrap();
        }
    }
    world
        .bind_cognition_runtime(
            "result-world",
            "main",
            0,
            Some(format!("blake3:{}", "e".repeat(64))),
            "verified",
            0,
        )
        .unwrap();
    world
        .install_test_provider_capability_fixture(AGENT_ID)
        .unwrap();
    let authority = world
        .capability_revocation_state()
        .authority_records
        .values()
        .next()
        .unwrap()
        .clone();
    world
        .bind_cognition_runtime(
            &authority.world_id,
            &authority.branch_id,
            authority.finality_epoch,
            Some(authority.finality_block_hash.clone()),
            "verified",
            0,
        )
        .unwrap();
    let create = service::sign_read_request(
        oasis7_agent_api::game::GAME_TASK_CREATE_DOMAIN,
        oasis7_agent_api::game::GameTaskCreateV1 {
            api_rev: 1,
            client_operation_id: "read-task".into(),
            world_id: "result-world".into(),
            genesis_digest: "result-genesis".into(),
            branch_id: "main".into(),
            reorg_epoch: oasis7_agent_api::game::DecimalU64(0),
            agent_id: AGENT_ID.into(),
            owner_binding: "player".into(),
            owner_generation: oasis7_agent_api::game::DecimalU64(1),
        },
        &secret,
    )
    .unwrap();
    let create_payload = WorldServicePayloadV1::GameTaskCreate(create);
    let create_request = service::SubmitIntentRequest {
        contract_version: 1,
        correlation: service::derive_correlation(
            service::WorldIdentity {
                world_id: "result-world".into(),
                genesis_digest: "result-genesis".into(),
            },
            &create_payload,
        )
        .unwrap(),
        deadline_unix_ms: None,
        signed_payload: create_payload,
    };
    let control = service::game_task::prepare_create(&world, &create_request, []).unwrap();
    world
        .record_world_service_result(CanonicalIntentResultV1 {
            request: create_request,
            action_id: 98,
            committed_height: world.state().time,
            receipt: control,
            rejected: None,
        })
        .unwrap();
    let control = service::game_task::current_control(&world, AGENT_ID)
        .unwrap()
        .unwrap();
    use oasis7_agent_api::game::*;
    let goal = GameGoalPublishV1 {
        api_rev: 1,
        world_id: control.world_id.clone(),
        genesis_digest: control.genesis_digest.clone(),
        task_id: control.task_id.clone(),
        agent_id: AGENT_ID.into(),
        owner_public_key: control.owner_public_key.clone(),
        owner_binding: control.owner_binding.clone(),
        owner_generation: control.owner_generation,
        branch_id: control.branch_id.clone(),
        reorg_epoch: control.reorg_epoch,
        expected_task_revision: control.task_revision,
        expected_latest_goal_revision: control.latest_goal_revision,
        goal_revision: DecimalU64(1),
        client_operation_id: "goal".into(),
        prepare_operation_id: "prepare-goal".into(),
        goal_ref: "ab".repeat(32),
    };
    let goal_request = service_request(WorldServicePayloadV1::GameGoalPublish(
        service::sign_read_request(GAME_GOAL_PUBLISH_DOMAIN, goal, &secret).unwrap(),
    ));
    let goal_receipt =
        service::game_task::prepare_publish(&world, &goal_request, [], world.state().time).unwrap();
    record(&mut world, goal_request, goal_receipt);
    let delegate_secret = hex::encode([74u8; 32]);
    let delegate_key = service::sign_read_request("key", (), &delegate_secret)
        .unwrap()
        .subject_public_key;
    world
        .apply_agent_signer_delegation(
            &service::sign_read_request(
                "delegation",
                service::AgentSignerDelegationChangeV1 {
                    world_id: "result-world".into(),
                    branch_id: "main".into(),
                    agent_id: AGENT_ID.into(),
                    owner_binding: "player".into(),
                    agent_identity_generation: 1,
                    delegate_public_key: delegate_key.clone(),
                    generation: 1,
                    revoked: false,
                    nonce: 1,
                },
                &secret,
            )
            .unwrap(),
        )
        .unwrap();
    let control = service::game_task::current_control(&world, AGENT_ID)
        .unwrap()
        .unwrap();
    let executor_request = service_request(WorldServicePayloadV1::GameExecutorControl(
        service::sign_read_request(
            GAME_EXECUTOR_CONTROL_DOMAIN,
            GameExecutorControlV1 {
                api_rev: 1,
                client_operation_id: "bind".into(),
                fence: GameTaskFenceV1::from_control(&control),
                operation: GameExecutorOperationV1::Bind {
                    executor_id: "executor".into(),
                    executor_public_key: delegate_key,
                    delegation_generation: DecimalU64(1),
                },
            },
            &secret,
        )
        .unwrap(),
    ));
    let executor_receipt =
        service::game_task::prepare_executor_control(&world, &executor_request, []).unwrap();
    record(&mut world, executor_request, executor_receipt);
    let control = service::game_task::current_control(&world, AGENT_ID)
        .unwrap()
        .unwrap();
    let fence = GameTaskFenceV1::from_control(&control);
    let request = continuous_commit_request(&world);
    let context = TaskContextIdentityV1 {
            authority_mode: None,
            native_authorization_root: None,
        schema_revision: 1,
        fence: fence.clone(),
        valid_until_tick: DecimalU64(world.state().time + 5),
        agent_session_id: request.agent_session_id.clone(),
        agent_turn_id: request.agent_turn_id.clone(),
        decision_request_id: request.decision_request_id.clone(),
        retry_seq: request.retry_seq,
        transport_attempt: request.transport_attempt,
        request_commitment: request.request_digest.clone(),
        context_commitment: request.context_digest.clone(),
        provider_invocation_commitment:
            crate::AgentDecisionEnvelopeV1::provider_invocation_key_for_request(
                &request.request_digest,
            ),
    };
    let prefix_signed = service::sign_read_request(
        "scheduler",
        service::SchedulerIntentV1 {
            agent_id: AGENT_ID.into(),
            request_id: "prefix".into(),
            delegation_generation: 1,
            captured_base_binding: world.current_cognition_base_binding().unwrap(),
            operation: service::SchedulerOperationV1::TaskProviderPrefix {
                context: context.clone(),
            },
        },
        &delegate_secret,
    )
    .unwrap();
    let prefix_receipt = world
        .apply_authenticated_scheduler(prefix_signed.clone())
        .unwrap();
    record(
        &mut world,
        service_request(WorldServicePayloadV1::Scheduler(prefix_signed)),
        prefix_receipt,
    );
    let action = Action::WorldRule {
        actor_id: AGENT_ID.into(),
        request_id: "result-order".into(),
        command: WorldRuleCommandV1::ConfirmAgentOrder {
            factory_id: "assembly".into(),
            quoted_at: 23,
            quote_digest: q.quote_digest,
        },
    };
    let intent = service::game_task::task_cognition_intent(
        &world,
        &context,
        action.clone(),
        &service::sign_read_request("key", (), &delegate_secret)
            .unwrap()
            .subject_public_key,
    )
    .unwrap();
    let request = intent.request.clone();
    let artifact = intent.response_artifact.clone();
    let signed = service::sign_read_request("cognition", intent, &delegate_secret).unwrap();
    let (commit, lineage) = world
        .commit_task_cognition_action(
            request,
            action,
            artifact,
            (
                &fence,
                &service::sign_read_request("key", (), &delegate_secret)
                    .unwrap()
                    .subject_public_key,
                1,
            ),
            &[],
        )
        .unwrap();
    let payload = WorldServicePayloadV1::Cognition(signed);
    let correlation = service::derive_correlation(
        service::WorldIdentity {
            world_id: "result-world".into(),
            genesis_digest: "result-genesis".into(),
        },
        &payload,
    )
    .unwrap();
    world
        .record_world_service_result(CanonicalIntentResultV1 {
            request: service::SubmitIntentRequest {
                contract_version: 1,
                correlation,
                deadline_unix_ms: None,
                signed_payload: payload,
            },
            action_id: 99,
            committed_height: world.state().time.max(1),
            receipt: json!({"commit_record":commit,"lineage":lineage}),
            rejected: None,
        })
        .unwrap();
    world
}

#[test]
fn signed_canonical_world_rule_results_preserve_actual_rejection_and_success() {
    for expired in [true, false] {
        let world = execute(expired);
        let status = world.world_rule_status(Some(AGENT_ID)).unwrap();
        let result = &status["recent_action_results"][0];
        assert_eq!(
            result["status"],
            if expired { "rejected" } else { "committed" }
        );
        assert_eq!(result["business_request_id"], "result-order");
        if expired {
            assert!(
                result["reason"]
                    .to_string()
                    .contains("manufacturing_quote_expired")
            );
            assert_eq!(result["next_step"], "refresh_quote_and_submit_new_request");
            assert!(
                world
                    .state()
                    .world_rules
                    .as_ref()
                    .is_none_or(|r| r.agent_orders.is_empty())
            );
        } else {
            assert_eq!(
                world
                    .state()
                    .world_rules
                    .as_ref()
                    .unwrap()
                    .agent_orders
                    .len(),
                1
            );
        }
        // A valid signature and the same opaque request digest cannot assign a
        // different action to an existing canonical receipt.
        let mut forged = world.clone();
        let mut record: CanonicalIntentResultV1 = serde_json::from_value(
            world
                .capability_revocation_state()
                .world_service_results
                .values()
                .find(|v| v["request"]["signed_payload"]["kind"] == "cognition")
                .unwrap()
                .clone(),
        )
        .unwrap();
        let WorldServicePayloadV1::Cognition(signed) = &mut record.request.signed_payload else {
            unreachable!()
        };
        let mut altered = signed.request.clone();
        let Action::WorldRule {
            command: WorldRuleCommandV1::ConfirmAgentOrder { quote_digest, .. },
            ..
        } = &mut altered.action
        else {
            unreachable!()
        };
        *quote_digest = "different-signed-action".into();
        *signed =
            service::sign_read_request("cognition", altered, &hex::encode([72u8; 32])).unwrap();
        record.request.correlation = service::derive_correlation(
            record.request.correlation.key.world.clone(),
            &record.request.signed_payload,
        )
        .unwrap();
        forged.record_world_service_result(record.clone()).unwrap();
        assert_eq!(
            forged.world_rule_status(Some(AGENT_ID)).unwrap()["recent_action_results"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        record.receipt["lineage"]["action_id"] = json!("action:999999");
        let WorldServicePayloadV1::Cognition(signed) = &mut record.request.signed_payload else {
            unreachable!()
        };
        *signed = service::sign_read_request(
            "cognition",
            signed.request.clone(),
            &hex::encode([73u8; 32]),
        )
        .unwrap();
        record.request.correlation = service::derive_correlation(
            record.request.correlation.key.world.clone(),
            &record.request.signed_payload,
        )
        .unwrap();
        forged.record_world_service_result(record).unwrap();
        assert_eq!(
            forged.world_rule_status(Some(AGENT_ID)).unwrap()["recent_action_results"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let manifest = world.chain_resource_manifest();
        let snapshot = world.snapshot_with_chain_resource_context(
            crate::ChainResourceDerivationContext {
                world_id: "foreign-world",
                chain_id: &manifest.chain_id,
                genesis_ref: manifest.genesis_ref.as_deref(),
                created_at_height: manifest.created_at_height,
                manifest_height: manifest.manifest_height,
                commit_block_hash: manifest.created_at_block_hash.as_deref(),
                tick: world.state().time,
            },
            "c".repeat(64),
            "d".repeat(64),
        );
        let missing_genesis = world.snapshot_with_chain_resource_context(
            crate::ChainResourceDerivationContext {
                world_id: &manifest.world_id,
                chain_id: &manifest.chain_id,
                genesis_ref: None,
                created_at_height: manifest.created_at_height,
                manifest_height: manifest.manifest_height,
                commit_block_hash: manifest.created_at_block_hash.as_deref(),
                tick: world.state().time,
            },
            "1".repeat(64),
            "2".repeat(64),
        );
        let missing_genesis =
            World::from_snapshot(missing_genesis, world.journal().clone()).unwrap();
        let missing_status = missing_genesis.world_rule_status(Some(AGENT_ID)).unwrap();
        assert!(
            missing_status["recent_action_results"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            missing_status["recent_action_results_history"]["status"],
            "unavailable"
        );
        assert_eq!(
            missing_status["recent_action_results_history"]["reason"],
            "protocol_genesis_binding_unavailable"
        );
        assert_eq!(
            missing_status["recent_action_results_history"]["missing_result"],
            "unknown"
        );
        let foreign = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
        assert!(
            foreign
                .world_rule_status(Some(AGENT_ID))
                .is_none_or(|s| s["recent_action_results"].as_array().unwrap().is_empty())
        );
        let clipped = world
            .clone()
            .with_runtime_memory_limits(crate::WorldRuntimeMemoryLimits {
                max_journal_events: 1,
                ..Default::default()
            });
        let clipped_status = clipped.world_rule_status(Some(AGENT_ID)).unwrap();
        assert_eq!(
            clipped_status["recent_action_results_history"]["missing_result"],
            "unknown"
        );
        assert!(
            clipped_status["recent_action_results"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let mut changed = world.clone();
        changed
            .install_capability_agent_identity(AGENT_ID, "new-player", 2)
            .unwrap();
        assert!(
            changed.world_rule_status(Some(AGENT_ID)).unwrap()["recent_action_results"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            world
                .world_rule_status(None)
                .is_none_or(|s| s["recent_action_results"].as_array().unwrap().is_empty())
        );
    }
}
