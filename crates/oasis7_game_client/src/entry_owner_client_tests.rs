use super::*;

fn key(byte: u8) -> String {
    hex::encode(
        SigningKey::from_bytes(&[byte; 32])
            .verifying_key()
            .to_bytes(),
    )
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
