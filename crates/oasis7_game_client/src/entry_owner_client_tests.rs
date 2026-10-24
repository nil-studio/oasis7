use super::*;

fn key(byte: u8) -> String {
    hex::encode(
        SigningKey::from_bytes(&[byte; 32])
            .verifying_key()
            .to_bytes(),
    )
}
fn stopped_task(owner: &str) -> GameTaskControlV1 {
    serde_json::from_value(json!({"schema_revision":1,"task_id":"task","world_id":"world","genesis_digest":"genesis","branch_id":"current","reorg_epoch":"2","agent_id":"agent","owner_binding":"owner","owner_generation":"1","owner_public_key":owner,"task_revision":"5","task_status":"suspended","latest_goal_revision":"1","active_goal_revision":"1","active_goal_reference":{"goal_revision":"1","goal_ref":"ab".repeat(32),"prepare_operation_id":"goal","publish_operation_id":"publish","activation_committed_height":"1"},"executor_epoch":"2","executor_id":null,"created_operation_id":"task-create"})).unwrap()
}
fn old_executor_original(owner: &str, seed: u8) -> WorldServicePayloadV1 {
    use oasis7_client_api::world_service_wire::{SchedulerIntentV1, TaskSchedulerIntentV1};
    let mut fence = GameTaskFenceV1::from_control(&stopped_task(owner));
    fence.task_status = GameTaskStatusV1::Active;
    fence.task_revision = DecimalU64(2);
    fence.branch_id = "old".into();
    fence.reorg_epoch = DecimalU64(0);
    fence.executor_epoch = DecimalU64(1);
    fence.executor_binding = Some(GameExecutorBindingV1 {
        executor_id: "old-a".into(),
        executor_public_key: key(seed),
        delegation_generation: DecimalU64(1),
        executor_epoch: DecimalU64(1),
    });
    let request: TaskSchedulerIntentV1 = SchedulerIntentV1 {
        task_prefix_binding_revision: 0,
        agent_id: "agent".into(),
        request_id: "original-prefix".into(),
        delegation_generation: 1,
        captured_base_binding: oasis7_agent_api::world_cognition::RuntimeCognitionBaseBindingV1 {
            world_id: "world".into(),
            branch_id: "old".into(),
            finality_epoch: 1,
            finality_block_hash: None,
            finality_status: "provisional".into(),
            base_tick: 0,
            base_world_hash: format!("blake3:{}", "a".repeat(64)),
            reorg_epoch: 0,
            runtime_manifest_hash: format!("blake3:{}", "b".repeat(64)),
        },
        operation: SchedulerOperationV1::TaskProviderPrefix {
            context: TaskContextIdentityV1 {
                authority_mode: None,
                native_authorization_root: None,
                schema_revision: 1,
                fence,
                agent_session_id: "session".into(),
                agent_turn_id: "turn".into(),
                decision_request_id: "decision".into(),
                retry_seq: 1,
                transport_attempt: 1,
                request_commitment: format!("blake3:{}", "1".repeat(64)),
                context_commitment: format!("blake3:{}", "2".repeat(64)),
                provider_invocation_commitment: format!("blake3:{}", "3".repeat(64)),
                valid_until_tick: DecimalU64(16),
            },
        },
    };
    WorldServicePayloadV1::Scheduler(
        ws::sign_read_request("scheduler", request, &hex::encode([seed; 32])).unwrap(),
    )
}
#[test]
fn stopped_owner_recovery_queries_strict_original_without_reactivating_executor() {
    let owner = key(21);
    let current = stopped_task(&owner);
    let world = WorldIdentity {
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
    };
    let original = old_executor_original(&owner, 81);
    let correlation = validate_owner_executor_lookup(&original, &current, &owner, &world).unwrap();
    assert_eq!(correlation.key.verified_subject, key(81));
    assert_ne!(correlation.key.verified_subject, owner);
    let lookup = AuthenticatedLookup {
        request: LookupIntentRequest {
            contract_version: 1,
            key: correlation.key,
        },
        original: original.clone(),
    };
    let operation = EntryOwnerOperationV1::LookupExecutor(lookup);
    assert_eq!(
        serde_json::to_value(operation).unwrap()["kind"],
        "lookup_executor"
    );
    assert!(current.executor_binding.is_none());
    assert_eq!(current.task_status, GameTaskStatusV1::Suspended);
    let mut forged = original.clone();
    if let WorldServicePayloadV1::Scheduler(s) = &mut forged {
        s.signature_hex = "0".repeat(128);
    }
    assert!(validate_owner_executor_lookup(&forged, &current, &owner, &world).is_err());
    for field in [
        "task",
        "agent",
        "genesis",
        "owner_binding",
        "owner_generation",
        "owner_key",
    ] {
        let mut wrong = current.clone();
        match field {
            "task" => wrong.task_id = "other".into(),
            "agent" => wrong.agent_id = "other".into(),
            "genesis" => wrong.genesis_digest = "other".into(),
            "owner_binding" => wrong.owner_binding = "other".into(),
            "owner_generation" => wrong.owner_generation.0 += 1,
            _ => wrong.owner_public_key = key(22),
        };
        assert!(
            validate_owner_executor_lookup(&original, &wrong, &owner, &world).is_err(),
            "cross resource {field}"
        );
    }
    assert!(
        validate_owner_executor_lookup(
            &old_executor_original(&owner, 21),
            &current,
            &owner,
            &world
        )
        .is_err(),
        "owner cannot impersonate historical executor"
    );
    assert!(
        validate_owner_executor_lookup(&task_original().signed_payload, &current, &owner, &world)
            .is_err(),
        "owner recovery must not authorize owner submissions"
    );
    assert!(
        serde_json::from_value::<WorldServicePayloadV1>(
            json!({"kind":"prepare_context","request":{}})
        )
        .is_err(),
        "private context is outside original payload protocol"
    );
}
fn task_original() -> SubmitIntentRequest<WorldServicePayloadV1> {
    let payload = WorldServicePayloadV1::GameTaskCreate(
        ws::sign_read_request(
            GAME_TASK_CREATE_DOMAIN,
            GameTaskCreateV1 {
                api_rev: 1,
                client_operation_id: "task-original".into(),
                world_id: "world".into(),
                genesis_digest: "genesis".into(),
                branch_id: "main".into(),
                reorg_epoch: DecimalU64(0),
                agent_id: "agent".into(),
                owner_binding: "owner".into(),
                owner_generation: DecimalU64(1),
            },
            &hex::encode([21; 32]),
        )
        .unwrap(),
    );
    SubmitIntentRequest {
        contract_version: 1,
        correlation: oasis7_client_api::world_service_correlation::derive_correlation(
            WorldIdentity {
                world_id: "world".into(),
                genesis_digest: "genesis".into(),
            },
            &payload,
        )
        .unwrap(),
        deadline_unix_ms: None,
        signed_payload: payload,
    }
}
#[test]
fn executor_must_be_independent_strong_and_canonical() {
    let owner = key(1);
    let login = key(2);
    let executor = key(3);
    assert!(validate_independent_executor_key(&executor, &owner, &login).is_ok());
    for bad in [
        &owner,
        &login,
        &executor.to_uppercase(),
        &hex::encode([0; 32]),
        "invalid",
    ] {
        assert!(validate_independent_executor_key(bad, &owner, &login).is_err());
    }
}
#[test]
fn same_runtime_resources_never_share_owner_outbox_scope() {
    let world = WorldIdentity {
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
    };
    let first = resource_scope("trust", &world, "owned-a", Some("task-a"), 1).unwrap();
    for (agent, task, generation) in [
        ("delegated-b", "task-a", 1),
        ("owned-a", "task-b", 1),
        ("owned-a", "task-a", 2),
    ] {
        assert_ne!(
            first,
            resource_scope("trust", &world, agent, Some(task), generation).unwrap()
        );
    }
    let other = WorldIdentity {
        world_id: world.world_id.clone(),
        genesis_digest: "other-genesis".into(),
    };
    assert_ne!(
        first,
        resource_scope("trust", &other, "owned-a", Some("task-a"), 1).unwrap()
    );
}
#[test]
fn exact_canonical_task_receipt_rejects_substituted_resource_revision_or_id() {
    let original = task_original();
    let legacy = serde_json::from_value(serde_json::to_value(&original).unwrap()).unwrap();
    let receipt = serde_json::to_value(
        oasis7_client_api::legacy_task_results::create_control(&legacy).unwrap(),
    )
    .unwrap();
    validate_owner_receipt(&original, 1, &receipt).unwrap();
    for (field, replacement) in [
        ("task_id", json!("other")),
        ("agent_id", json!("other")),
        ("owner_generation", json!("2")),
        ("task_revision", json!("2")),
        ("branch_id", json!("other")),
        ("genesis_digest", json!("other")),
    ] {
        let mut changed = receipt.clone();
        changed[field] = replacement;
        assert!(validate_owner_receipt(&original, 1, &changed).is_err());
    }
}
#[test]
fn starter_receipt_requires_real_matching_agent_owner_and_positive_claim() {
    let r = GameStarterClaimV1 {
        api_rev: 1,
        client_operation_id: "starter-op".into(),
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
        branch_id: "main".into(),
        reorg_epoch: DecimalU64(0),
        agent_id: "agent".into(),
        owner_binding: "owner".into(),
        owner_generation: DecimalU64(1),
    };
    let signed =
        ws::sign_read_request(GAME_STARTER_CLAIM_DOMAIN, r, &hex::encode([21; 32])).unwrap();
    let payload = WorldServicePayloadV1::GameStarterClaim(signed.clone());
    let correlation = oasis7_client_api::world_service_correlation::derive_correlation(
        WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
        },
        &payload,
    )
    .unwrap();
    let request = SubmitIntentRequest {
        contract_version: 1,
        correlation,
        deadline_unix_ms: None,
        signed_payload: payload,
    };
    let receipt = json!({"claim":{"agent_id":"agent","player_id":"owner","public_key":signed.subject_public_key,"amount":100,"claimed_at":0}});
    validate_owner_receipt(&request, 1, &receipt).unwrap();
    let mut with_events = receipt.clone();
    with_events["events"] = json!([]);
    validate_owner_receipt(&request, 1, &with_events).unwrap();
    for (field, value) in [
        ("agent_id", json!("other")),
        ("player_id", json!("other")),
        ("public_key", json!(key(22))),
        ("amount", json!(0)),
    ] {
        let mut changed = receipt.clone();
        changed["claim"][field] = value;
        assert!(validate_owner_receipt(&request, 1, &changed).is_err());
    }
}
#[test]
fn owner_result_signature_is_bound_to_exact_original_path_and_pinned_service_key() {
    let original = task_original();
    let response = IntentResponse::<Value> {
        contract_version: 1,
        formal_profile: None,
        execution_evidence_scope: None,
        correlation: original.correlation.clone(),
        outcome: IntentOutcome::Unknown,
    };
    let signed = ws::sign_service_response(
        SUBMIT_PATH,
        ws::request_digest(SUBMIT_PATH, &original).unwrap(),
        response,
        &hex::encode([22; 32]),
    )
    .unwrap();
    let value = serde_json::to_value(&signed).unwrap();
    verify_intent_response(
        &original,
        SUBMIT_PATH,
        value.clone(),
        &original.correlation,
        &key(22),
    )
    .unwrap();
    assert!(
        verify_intent_response(
            &original,
            LOOKUP_PATH,
            value.clone(),
            &original.correlation,
            &key(22)
        )
        .is_err()
    );
    assert!(
        verify_intent_response(
            &original,
            SUBMIT_PATH,
            value.clone(),
            &original.correlation,
            &key(23)
        )
        .is_err()
    );
    let mut altered = original.clone();
    altered.deadline_unix_ms = Some(5);
    assert!(
        verify_intent_response(
            &altered,
            SUBMIT_PATH,
            value,
            &original.correlation,
            &key(22)
        )
        .is_err()
    );
}

#[test]
fn restart_retains_original_fence_and_wire_before_any_network_and_isolates_vaults() {
    const PASSWORD: &[u8] = b"synthetic owner control test unlock";
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).unwrap();
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("entry-owner-test-{}", hex::encode(random)));
    std::fs::create_dir(&root).unwrap();
    let config = ParticipantClientConfig {
        identity: IdentityClientConfig {
            endpoint: "https://identity.invalid".into(),
            identity_service_id: "identity".into(),
            environment_id: "private".into(),
            audience: "https://identity.invalid".into(),
            state_directory: root.join("identity"),
            test_loopback: false,
        },
        identity_instance_epoch: DecimalU64(1),
        world: WorldIdentity {
            world_id: "world".into(),
            genesis_digest: "genesis".into(),
        },
        trusted_world_service_key: key(22),
        owner_state_directory: root.join("owner"),
    };
    IdentityClient::initialize(&config.identity, PASSWORD).unwrap();
    ParticipantClient::initialize_owner(&config, PASSWORD).unwrap();
    let client = ParticipantClient::new(config.clone(), PASSWORD).unwrap();
    let owner_public = hex::encode(client.key.verifying_key().to_bytes());
    let old_executor = old_executor_original(&owner_public, 81);
    assert!(
        client
            .owner_lookup_executor_original(
                "participant",
                "missing-login",
                "lookup-original",
                old_executor.clone()
            )
            .is_err()
    );
    let saved: (String, WorldServicePayloadV1) = client
        .owner
        .read_original(
            &client.scope,
            "lookup-original",
            "owner-executor-lookup-original",
        )
        .unwrap()
        .unwrap();
    assert_eq!(saved.0, "participant");
    assert_eq!(saved.1, old_executor);
    assert!(
        client
            .owner
            .read_original::<Value>(&client.scope, "lookup-original", "owner-intent-index")
            .unwrap()
            .is_none(),
        "lookup original must never enter owner submit journal"
    );
    let conflict = client.owner_lookup_executor_original(
        "participant",
        "missing-login",
        "lookup-original",
        old_executor_original(&owner_public, 82),
    );
    assert!(matches!(conflict,Err(ref error) if error.contains("content conflict")));
    assert!(
        client
            .create_task("participant", "missing-login", "lookup-original")
            .is_err(),
        "read-only recovery operation cannot be reused as a write"
    );
    let status = client
        .owner_operation_status("participant", "lookup-original")
        .unwrap();
    assert_eq!(status.state, "unknown");
    assert_eq!(status.stage, "executor_original_lookup");
    assert!(status.canonical_commit.is_none());
    assert!(status.receipt_ref.is_none());
    let settlement: IntentResponse<Value> = IntentResponse {
        contract_version: 1,
        formal_profile: None,
        execution_evidence_scope: None,
        correlation: oasis7_client_api::world_service_correlation::derive_correlation(
            client.config.world.clone(),
            &old_executor,
        )
        .unwrap(),
        outcome: IntentOutcome::Committed {
            commit: Box::new(CommitRef {
                world: client.config.world.clone(),
                binding: ExecutionBinding {
                    provider_world_id: "world".into(),
                    branch_id: "old".into(),
                    finality_ref: "fixture".into(),
                    reorg_generation: 0,
                    governing_manifest_ref: "manifest".into(),
                    authority_generation: 1,
                    permission_generation: 1,
                },
                position: 1,
                execution_block_hash: "block".into(),
                state_root_ref: "root".into(),
            }),
            receipt: json!({"applied":true}),
        },
    };
    client
        .owner
        .put_original(
            &status.resource_scope,
            "lookup-original",
            "owner-executor-lookup-settlement",
            &settlement,
        )
        .unwrap();
    let historical = client
        .owner_operation_status("participant", "lookup-original")
        .unwrap();
    assert_eq!(historical.state, "confirmed");
    assert_eq!(
        historical.next_action,
        "historical_receipt_only_no_current_authority_or_resume"
    );
    assert!(historical.canonical_commit.is_some());
    assert!(historical.receipt_ref.is_some());
    let request = GameTaskCreateV1 {
        api_rev: 1,
        client_operation_id: "original-task".into(),
        world_id: "world".into(),
        genesis_digest: "genesis".into(),
        branch_id: "original-branch".into(),
        reorg_epoch: DecimalU64(4),
        agent_id: "owned-agent".into(),
        owner_binding: "owner".into(),
        owner_generation: DecimalU64(2),
    };
    let payload = WorldServicePayloadV1::GameTaskCreate(
        ws::sign_read_request(GAME_TASK_CREATE_DOMAIN, request, &client.owner_private()).unwrap(),
    );
    let original = client
        .save_intent(
            "participant",
            "original-task",
            "owned-agent",
            None,
            2,
            payload,
        )
        .unwrap();
    // Missing login fails before networking, but exact lookup and submit have
    // already been atomically retained. No current/newer fence was substituted.
    assert!(
        client
            .create_task("participant", "missing-login", "original-task")
            .is_err()
    );
    let before: Value = client
        .owner
        .read_original(
            &original.resource_scope,
            "original-task",
            "owner-wire-original",
        )
        .unwrap()
        .unwrap();
    let executor = client
        .initialize_executor(&root.join("executor-a"), PASSWORD)
        .unwrap();
    assert_ne!(executor, hex::encode(client.key.verifying_key().to_bytes()));
    assert!(
        client
            .initialize_executor(&root.join("owner"), PASSWORD)
            .is_err()
    );
    assert!(
        client
            .initialize_executor(&root.join("identity"), PASSWORD)
            .is_err()
    );
    assert!(
        client
            .initialize_executor(&root.join("executor-a"), PASSWORD)
            .is_err()
    );
    assert!(
        client
            .bind_executor(
                "participant",
                "missing-login",
                "original-task",
                "executor",
                &executor,
                1
            )
            .is_err()
    );
    drop(client);
    let restarted = ParticipantClient::new(config, PASSWORD).unwrap();
    assert!(
        restarted
            .create_task("participant", "missing-login", "original-task")
            .is_err()
    );
    let after: Value = restarted
        .owner
        .read_original(
            &original.resource_scope,
            "original-task",
            "owner-wire-original",
        )
        .unwrap()
        .unwrap();
    assert_eq!(before, after);
    let (_, wire): (
        AuthenticatedLookup,
        SubmitIntentRequest<WorldServicePayloadV1>,
    ) = serde_json::from_value(after).unwrap();
    let WorldServicePayloadV1::GameTaskCreate(signed) = wire.signed_payload else {
        panic!("original task payload missing")
    };
    assert_eq!(signed.request.branch_id, "original-branch");
    assert_eq!(signed.request.reorg_epoch.0, 4);
    assert_eq!(signed.request.owner_generation.0, 2);
    assert_eq!(
        restarted
            .owner_operation_status("participant", "original-task")
            .unwrap()
            .state,
        "unknown"
    );
    let bytes = std::fs::read(root.join("executor-a/executor-control.encrypted-v1")).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("synthetic owner control"));
    drop(restarted);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn owner_signed_history_unavailable_blocks_and_preserves_original_without_submit() {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let (_root, c, client) =
        super::super::tests::history_client(format!("http://{}", listener.local_addr().unwrap()));
    let server = super::super::tests::history_server(listener, c.world.clone(), true);
    assert_eq!(
        client.register("participant", "login").unwrap().next_action,
        "verify_canonical_owner_view"
    );
    let payload = WorldServicePayloadV1::GameTaskCreate(
        ws::sign_read_request(
            GAME_TASK_CREATE_DOMAIN,
            GameTaskCreateV1 {
                api_rev: 1,
                client_operation_id: "history-task".into(),
                world_id: "world".into(),
                genesis_digest: "genesis".into(),
                branch_id: "main".into(),
                reorg_epoch: DecimalU64(0),
                agent_id: "agent".into(),
                owner_binding: "binding".into(),
                owner_generation: DecimalU64(1),
            },
            &client.owner_private(),
        )
        .unwrap(),
    );
    let original = client
        .save_intent("participant", "history-task", "agent", None, 1, payload)
        .unwrap();
    let first = client
        .create_task("participant", "login", "history-task")
        .unwrap();
    let before: Value = client
        .owner
        .read_original(
            &original.resource_scope,
            "history-task",
            "owner-wire-original",
        )
        .unwrap()
        .unwrap();
    let second = client
        .create_task("participant", "login", "history-task")
        .unwrap();
    let after: Value = client
        .owner
        .read_original(
            &original.resource_scope,
            "history-task",
            "owner-wire-original",
        )
        .unwrap()
        .unwrap();
    server.join().unwrap();
    assert_eq!(before, after);
    for progress in [first, second] {
        assert_eq!(progress.state, "blocked");
        assert_eq!(progress.next_action, "restore_original_history");
        assert!(progress.canonical_commit.is_none());
    }
    assert!(
        client
            .owner
            .read_original::<IntentResponse<Value>>(
                &original.resource_scope,
                "history-task",
                "owner-settlement"
            )
            .unwrap()
            .is_none()
    );
}
