use super::*;

pub(super) fn assert_expiry_retention_matches_canonical(
    driver: &NodeRuntimeExecutionDriver,
    expiry_prefix: &SubmitIntentRequest<WorldServicePayloadV1>,
    expiry: &PreparedTaskContextV1,
    delegate_public: &str,
    owner_public: &str,
) {
    let history = game_task::validate_retention_history(&driver.execution_world).unwrap();
    let expiry_recorded = game_task_tests::recorded(driver, expiry_prefix);
    assert_eq!(
        history.active_publish("agent-a").unwrap(),
        game_task::active_publish(&driver.execution_world, "agent-a").unwrap(),
        "batched active publication must match the canonical single-agent API"
    );
    let prefixes = history.retained_prefixes().collect::<Vec<_>>();
    assert!(prefixes.iter().any(|(result, context, signer)| {
        *result == &expiry_recorded && *context == &expiry.identity && *signer == delegate_public
    }));
    let mut canonical_selected = Vec::new();
    let mut batched_selected = Vec::new();
    for (result, context, signer) in prefixes {
        let canonical =
            game_task::validate_task_context_capture(&driver.execution_world, context, signer);
        let batched = history.validate_prefix_capture(context, signer);
        assert_eq!(canonical.is_ok(), batched.is_ok());
        if canonical.is_ok() {
            canonical_selected.push(result.committed_height);
        }
        if batched.is_ok() {
            batched_selected.push(result.committed_height);
        }
    }
    assert_eq!(canonical_selected, batched_selected);
    let mut substituted = expiry.identity.clone();
    substituted.agent_turn_id.push_str("-substituted");
    let canonical = game_task::validate_task_context_capture(
        &driver.execution_world,
        &substituted,
        delegate_public,
    );
    let batched = history.validate_prefix_capture(&substituted, delegate_public);
    assert_eq!(canonical.is_ok(), batched.is_ok());
    assert!(canonical.is_err());
    let canonical = game_task::validate_task_context_capture(
        &driver.execution_world,
        &expiry.identity,
        owner_public,
    );
    let batched = history.validate_prefix_capture(&expiry.identity, owner_public);
    assert_eq!(canonical.is_ok(), batched.is_ok());
    assert!(canonical.is_err());
}

pub(super) fn assert_stop_and_revocation_reject_context(
    driver: &NodeRuntimeExecutionDriver,
    expiry: &PreparedTaskContextV1,
    expiry_prefix: &SubmitIntentRequest<WorldServicePayloadV1>,
    world: &WorldIdentity,
    owner: &str,
    delegate_public: &str,
) {
    let mut stopped_world = boxed_context_value(|| driver.execution_world.clone());
    assert!(expiry.valid_until_tick.0 >= stopped_world.state().time);
    let canonical =
        game_task::validate_task_context_capture(&stopped_world, &expiry.identity, delegate_public);
    let before_stop_history = game_task::validate_retention_history(&stopped_world).unwrap();
    let batched = before_stop_history.validate_prefix_capture(&expiry.identity, delegate_public);
    assert_eq!(canonical.is_ok(), batched.is_ok());
    assert!(canonical.is_ok(), "fresh context must pass before Stop");
    let stop_request = request(
        world,
        WorldServicePayloadV1::GameExecutorControl(
            sign_read_request(
                GAME_EXECUTOR_CONTROL_DOMAIN,
                GameExecutorControlV1 {
                    api_rev: 1,
                    client_operation_id: "retention-stop-fence".into(),
                    fence: expiry.identity.fence.clone(),
                    operation: GameExecutorOperationV1::Stop,
                },
                owner,
            )
            .unwrap(),
        ),
    );
    let stop_height = game_task_tests::recorded(driver, expiry_prefix).committed_height + 1;
    let stop_record = CanonicalIntentResultV1 {
        receipt: game_task::prepare_executor_control(&stopped_world, &stop_request, []).unwrap(),
        request: stop_request,
        action_id: stop_height,
        committed_height: stop_height,
        rejected: None,
    };
    stopped_world
        .record_world_service_result(stop_record)
        .unwrap();
    assert!(expiry.valid_until_tick.0 >= stopped_world.state().time);
    let stopped_history = game_task::validate_retention_history(&stopped_world).unwrap();
    let canonical =
        game_task::validate_task_context_capture(&stopped_world, &expiry.identity, delegate_public);
    let batched = stopped_history.validate_prefix_capture(&expiry.identity, delegate_public);
    assert_eq!(canonical.is_ok(), batched.is_ok());
    assert!(canonical.is_err(), "Stop must reject a still-live context");
    let mut revoked_world = boxed_context_value(|| driver.execution_world.clone());
    revoked_world
        .apply_agent_signer_delegation(
            &sign_read_request(
                "delegation",
                AgentSignerDelegationChangeV1 {
                    world_id: "w1".into(),
                    branch_id: "main".into(),
                    agent_id: "agent-a".into(),
                    owner_binding: "owner-a".into(),
                    agent_identity_generation: 1,
                    delegate_public_key: delegate_public.to_owned(),
                    generation: 2,
                    revoked: true,
                    nonce: 2,
                },
                owner,
            )
            .unwrap(),
        )
        .unwrap();
    let revoked_history = game_task::validate_retention_history(&revoked_world).unwrap();
    let canonical =
        game_task::validate_task_context_capture(&revoked_world, &expiry.identity, delegate_public);
    let batched = revoked_history.validate_prefix_capture(&expiry.identity, delegate_public);
    assert_eq!(canonical.is_ok(), batched.is_ok());
    assert!(
        canonical.is_err(),
        "revoked delegation must fail in both paths"
    );
}

pub(super) fn advance_context_until_expired(
    driver: &mut NodeRuntimeExecutionDriver,
    expiry: &PreparedTaskContextV1,
    initial_height: u64,
    delegate_public: &str,
) -> u64 {
    let mut expiry_height = initial_height;
    // Administrative receipts preserve simulation time; actual empty simulation
    // blocks advance ticks without fabricating a clock or player action.
    while driver.execution_world.state().time <= expiry.valid_until_tick.0 {
        expiry_height += 1;
        assert!(expiry_height <= initial_height + TASK_CONTEXT_LEASE_TICKS + 1);
        driver
            .on_commit(NodeExecutionCommitContext {
                world_id: "w1".into(),
                node_id: "node-a".into(),
                proposer_id: "node-a".into(),
                height: expiry_height,
                slot: expiry_height,
                epoch: 0,
                node_block_hash: format!("node-h{expiry_height}"),
                action_root: compute_consensus_action_root(&[]).unwrap(),
                committed_actions: vec![],
                committed_at_unix_ms: expiry_height as i64 * 1000,
            })
            .unwrap();
    }
    assert!(driver.execution_world.state().time > expiry.valid_until_tick.0);
    let history = game_task::validate_retention_history(&driver.execution_world).unwrap();
    let canonical = game_task::validate_task_context_capture(
        &driver.execution_world,
        &expiry.identity,
        delegate_public,
    );
    let batched = history.validate_prefix_capture(&expiry.identity, delegate_public);
    assert_eq!(canonical.is_ok(), batched.is_ok());
    assert!(
        canonical.is_err(),
        "expired context must fail closed in both paths"
    );
    expiry_height
}

pub(super) fn verify_retained_evidence_and_fail_closed_on_missing_history(
    dir: &Path,
    cas: &LocalCasStore,
    world: &WorldIdentity,
    expiry_height: u64,
    g2_context: &PreparedTaskContextV1,
    authority_mode: Option<oasis7_agent_api::game::TaskContextAuthorityModeV1>,
    fence: &GameTaskFenceV1,
    delegate: &str,
    signer: &crate::feedback_submit_api::FeedbackSubmitSigner,
    store: &PrivateGoalStore,
    runtime: &Arc<Mutex<oasis7_node::NodeRuntime>>,
    owner: &str,
    age: u64,
) {
    // Settled and expired contexts no longer authorize writes, but their exact
    // receipts remain recoverable for the current live task fence beyond hot history.
    checkpoint::run_execution_bridge_retention_maintenance(&dir.join("records"), &cas, 1).unwrap();
    let retained = [
        age + 10,
        age + 11,
        age + 12,
        age + 13,
        expiry_height + 1,
        expiry_height + 2,
        expiry_height + 3,
    ];
    let shards = cas
        .list_pin_scope_shards("execution_bridge_live_tasks_v1")
        .unwrap();
    assert_eq!(
        shards.len(),
        retained.len(),
        "only current full-fence receipts are owned"
    );
    for height in retained {
        assert!(
            shards.contains(&format!("record-{height:020}")),
            "live task receipt at {height} lost after context settlement or expiry"
        );
        assert!(
            super::super::super::world_service_read::pin_at_height(
                &dir.join("records"),
                &dir.join("store"),
                &world,
                height,
            )
            .is_ok(),
            "live task historical receipt {height} must remain readable"
        );
    }
    // Truly absent historical full evidence is a recovery error, not an unavailable retry.
    let activation = g2_context
        .snapshot
        .control
        .active_goal_reference
        .as_ref()
        .unwrap()
        .activation_committed_height
        .0;
    let historical_path =
        checkpoint::execution_bridge_record_path(&dir.join("records"), activation);
    let historical_bytes = std::fs::read(&historical_path).unwrap();
    let mut compacted: serde_json::Value = serde_json::from_slice(&historical_bytes).unwrap();
    compacted["snapshot_ref"] = serde_json::Value::Null;
    compacted["journal_ref"] = serde_json::Value::Null;
    compacted["latest_state_ref"] = serde_json::Value::Null;
    std::fs::write(&historical_path, serde_json::to_vec(&compacted).unwrap()).unwrap();
    let snapshot_request = sign_read_request(
        TASK_SNAPSHOT_PATH,
        TaskGoalSnapshotRequestV1 {
            authority_mode,
            schema_revision: oasis7_agent_api::game::TaskContextAuthorityModeV1::schema_revision(
                authority_mode,
            ),
            fence: (*fence).clone(),
        },
        &delegate,
    )
    .unwrap();
    let (status, error) = http(
        &dir,
        &runtime,
        &signer,
        Some(&store),
        TASK_SNAPSHOT_PATH,
        &snapshot_request,
        "",
    );
    assert_eq!(status, 503);
    assert_eq!(error["error"], "private_goal_recovery_required");
    assert!(error.get("payload").is_none());
    let owner_request = sign_read_request(
        oasis7::world_service::private_goal_read::GOAL_CURRENT_PATH,
        oasis7::world_service::private_goal_read::CurrentGoalRequest {
            schema_revision: 1,
            world: (*world).clone(),
            agent_id: "agent-a".into(),
            task_id: fence.task_id.clone(),
        },
        &owner,
    )
    .unwrap();
    let (status, error) = http(
        &dir,
        &runtime,
        &signer,
        Some(&store),
        oasis7::world_service::private_goal_read::GOAL_CURRENT_PATH,
        &owner_request,
        "",
    );
    assert_eq!(status, 503);
    assert_eq!(error["error"], "private_goal_recovery_required");
    assert!(error.get("payload").is_none());
    std::fs::write(&historical_path, &historical_bytes).unwrap();
    // Unknown cache entries must prevent retention, never cause arbitrary path/height retention.
    let cache_dir = dir.join("store/pin_scopes/execution_bridge_live_tasks_v1");
    let unknown = cache_dir.join("unexpected.json");
    std::fs::write(&unknown, b"{}").unwrap();
    assert!(
        checkpoint::run_execution_bridge_retention_maintenance(&dir.join("records"), &cas, 1)
            .is_err()
    );
    std::fs::remove_file(unknown).unwrap();
}

pub(super) struct StopDeliveryContext {
    pub(super) dir: std::path::PathBuf,
    pub(super) private: std::path::PathBuf,
    pub(super) driver: Box<NodeRuntimeExecutionDriver>,
    pub(super) world: WorldIdentity,
    pub(super) runtime: Arc<Mutex<oasis7_node::NodeRuntime>>,
    pub(super) signer: crate::feedback_submit_api::FeedbackSubmitSigner,
    pub(super) store: PrivateGoalStore,
    pub(super) g2_prepare:
        oasis7_client_api::world_service::SignedReadRequest<TaskContextPrepareV1>,
    pub(super) fence: GameTaskFenceV1,
    pub(super) delegate: String,
    pub(super) owner: String,
    pub(super) goal: oasis7_client_api::world_service::private_goal::SignedGoalPrepareV1,
    pub(super) cas: LocalCasStore,
    pub(super) expiry_height: u64,
    pub(super) age: u64,
    pub(super) create: SubmitIntentRequest<WorldServicePayloadV1>,
}

pub(super) fn verify_stop_fences_delivery_and_cleanup(context: StopDeliveryContext) {
    let StopDeliveryContext {
        dir,
        private,
        mut driver,
        world,
        runtime,
        signer,
        store,
        g2_prepare,
        fence,
        delegate,
        owner,
        goal,
        cas,
        expiry_height,
        age,
        create,
    } = context;
    // A Stop committed after preparation/decryption and before delivery fences the reply.
    let mut next = g2_prepare.request.clone();
    next.client_operation_id = "next-context".into();
    let next = sign_read_request(TASK_CONTEXT_PREPARE_PATH, next, &delegate).unwrap();
    let stop = request(
        &world,
        WorldServicePayloadV1::GameExecutorControl(
            sign_read_request(
                GAME_EXECUTOR_CONTROL_DOMAIN,
                GameExecutorControlV1 {
                    api_rev: 1,
                    client_operation_id: "stop-context".into(),
                    fence,
                    operation: GameExecutorOperationV1::Stop,
                },
                &owner,
            )
            .unwrap(),
        ),
    );
    let (status, body) = http_with_hook(
        &dir,
        &runtime,
        &signer,
        Some(&store),
        TASK_CONTEXT_PREPARE_PATH,
        &next,
        "",
        || {
            commit(&mut driver, expiry_height + 4, stop);
        },
    );
    assert!(status == 403 || status == 409);
    assert!(body.get("payload").is_none());
    assert!(!body.to_string().contains(&goal.request.content.goal_text));
    assert_eq!(
        cas.list_pin_scope_shards("execution_bridge_live_tasks_v1")
            .unwrap(),
        vec![format!("record-{:020}", age + 10)],
        "Stop releases retired executor receipts; the active goal retains its publication"
    );
    drop(store);
    let bytes = std::fs::read_dir(&private)
        .unwrap()
        .flat_map(|e| std::fs::read(e.unwrap().path()).unwrap())
        .collect::<Vec<_>>();
    assert!(
        !bytes
            .windows(goal.request.content.goal_text.len())
            .any(|b| b == goal.request.content.goal_text.as_bytes())
    );
    let records = dir.join("records");
    let stale_height = checkpoint::list_execution_bridge_record_heights(&records)
        .unwrap()
        .last()
        .copied()
        .unwrap();
    let stale_record = checkpoint::load_execution_bridge_record(
        checkpoint::execution_bridge_record_path(&records, stale_height).as_path(),
    )
    .unwrap();
    let stale_validation =
        checkpoint::refresh_live_task_read_evidence(&records, &cas, &driver.execution_world, 2)
            .unwrap();
    commit(&mut driver, stale_height + 1, create.clone());
    assert!(
        checkpoint::run_execution_bridge_incremental_retention_maintenance_after_task_refresh(
            &records,
            &cas,
            &stale_record,
            stale_validation,
            2,
            32,
            3,
        )
        .is_err(),
        "task validation must not be reused after a higher durable record is appended"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

pub(super) struct ReplacementGoalInputs<'a> {
    pub(super) driver: &'a mut NodeRuntimeExecutionDriver,
    pub(super) dir: &'a Path,
    pub(super) world: &'a WorldIdentity,
    pub(super) owner: &'a str,
    pub(super) runtime: &'a Arc<Mutex<oasis7_node::NodeRuntime>>,
    pub(super) signer: &'a crate::feedback_submit_api::FeedbackSubmitSigner,
    pub(super) store: &'a PrivateGoalStore,
    pub(super) get: &'a oasis7_client_api::world_service::SignedReadRequest<TaskContextGetV1>,
    pub(super) cas: &'a LocalCasStore,
    pub(super) prepared: &'a PreparedTaskContextV1,
    pub(super) goal: &'a oasis7_client_api::world_service::private_goal::SignedGoalPrepareV1,
    pub(super) authority_mode: Option<oasis7_agent_api::game::TaskContextAuthorityModeV1>,
    pub(super) delegate: &'a str,
    pub(super) age: u64,
}

pub(super) struct ReplacementTaskContext {
    pub(super) goal: oasis7_client_api::world_service::private_goal::SignedGoalPrepareV1,
    pub(super) prepared: PreparedTaskContextV1,
    pub(super) prepare: oasis7_client_api::world_service::SignedReadRequest<TaskContextPrepareV1>,
    pub(super) fence: GameTaskFenceV1,
}

pub(super) fn publish_replacement_goal(
    inputs: ReplacementGoalInputs<'_>,
) -> ReplacementTaskContext {
    let ReplacementGoalInputs {
        driver,
        dir,
        world,
        owner,
        runtime,
        signer,
        store,
        get,
        cas,
        prepared,
        goal,
        authority_mode,
        delegate,
        age,
    } = inputs;
    // Publish G2 through the canonical owner lane, retire G1, bind, and consume G2 in full.
    let before_goal = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    let mut g2 = goal.request.clone();
    g2.expected_task_revision = before_goal.task_revision;
    g2.expected_latest_goal_revision = before_goal.latest_goal_revision;
    g2.goal_revision = DecimalU64(2);
    g2.client_operation_id = "g2-prepare".into();
    g2.client_nonce = "56".repeat(32);
    g2.content.goal_text = "G2 move two cells with new complete goal".into();
    g2.goal_digest = game_goal_version_digest(&g2.task_id, g2.goal_revision, &g2.content).unwrap();
    let g2 = sign_goal_prepare(g2, &owner).unwrap();
    let g2_receipt = store.prepare(&g2).unwrap();
    commit(
        driver,
        age + 10,
        request(
            &world,
            WorldServicePayloadV1::GameGoalPublish(
                sign_read_request(
                    GAME_GOAL_PUBLISH_DOMAIN,
                    publication(&g2, &g2_receipt, "g2-publish".into()).unwrap(),
                    &owner,
                )
                .unwrap(),
            ),
        ),
    );
    assert_eq!(
        http(
            &dir,
            &runtime,
            &signer,
            Some(&store),
            TASK_CONTEXT_READ_PATH,
            &get,
            ""
        )
        .0,
        403
    );
    let retired = checkpoint::load_execution_bridge_record(
        &checkpoint::execution_bridge_record_path(&dir.join("records"), 4),
    )
    .unwrap();
    assert!(
        retired.snapshot_ref.is_none() && retired.journal_ref.is_none(),
        "replaced goal must release out-of-window full record pins"
    );
    assert!(
        !cas.list_pin_scope_shards("execution_bridge_live_tasks_v1")
            .unwrap()
            .contains(&format!("record-{:020}", 4))
    );
    assert_eq!(
        cas.list_pin_scope_shards("execution_bridge_live_tasks_v1")
            .unwrap(),
        vec![format!("record-{:020}", age + 10)],
        "retained task evidence shards must equal the live dependency set after goal replacement"
    );
    let current = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    assert_eq!(
        current.executor_epoch,
        prepared.identity.fence.executor_epoch
    );
    assert_eq!(
        current.executor_binding,
        prepared.identity.fence.executor_binding
    );
    let fence = GameTaskFenceV1::from_control(
        &game_task::current_control(&driver.execution_world, "agent-a")
            .unwrap()
            .unwrap(),
    );
    let g2_prepare = sign_read_request(
        TASK_CONTEXT_PREPARE_PATH,
        TaskContextPrepareV1 {
            authority_mode,
            schema_revision: oasis7_agent_api::game::TaskContextAuthorityModeV1::schema_revision(
                authority_mode,
            ),
            fence: fence.clone(),
            client_operation_id: "g2-context".into(),
            client_nonce: "78".repeat(32),
        },
        &delegate,
    )
    .unwrap();
    let g2_context = consumer(&dir, &runtime, &signer, &store, &world, |client| {
        client.prepare_task_context(&g2_prepare).unwrap()
    });
    assert_eq!(
        g2_context.snapshot.active_goal.owner_proof.request.content,
        g2.request.content
    );
    assert_ne!(
        g2_context.identity.context_commitment,
        prepared.identity.context_commitment
    );
    commit(
        driver,
        age + 11,
        request(
            &world,
            WorldServicePayloadV1::Scheduler(
                sign_read_request("scheduler", public_prefix(&g2_context).unwrap(), &delegate)
                    .unwrap(),
            ),
        ),
    );
    let g2_get = sign_read_request(
        TASK_CONTEXT_READ_PATH,
        TaskContextGetV1 {
            schema_revision: 1,
            context: g2_context.identity.clone(),
            prepare_operation_id: g2_prepare.request.client_operation_id.clone(),
        },
        &delegate,
    )
    .unwrap();
    let g2_captured = consumer(&dir, &runtime, &signer, &store, &world, |client| {
        client.read_task_context(&g2_get).unwrap()
    });
    let g2_action = request(
        &world,
        WorldServicePayloadV1::Cognition(
            sign_read_request(
                "cognition",
                cognition_intent(
                    &g2_captured,
                    Action::MoveAgent {
                        agent_id: "agent-a".into(),
                        to: oasis7_agent_api::world_action::GeoPos::new(2, 0, 0),
                    },
                )
                .unwrap(),
                &delegate,
            )
            .unwrap(),
        ),
    );
    commit(driver, age + 12, g2_action.clone());
    assert!(
        game_task_tests::recorded(&driver, &g2_action)
            .rejected
            .is_none()
    );
    assert_eq!(
        driver.execution_world.state().agents["agent-a"].state.pos,
        oasis7_agent_api::world_action::GeoPos::new(2, 0, 0)
    );
    ReplacementTaskContext {
        goal: g2,
        prepared: g2_context,
        prepare: g2_prepare,
        fence,
    }
}

pub(super) struct InitialCaptureInputs<'a> {
    pub(super) driver: &'a mut NodeRuntimeExecutionDriver,
    pub(super) dir: &'a Path,
    pub(super) world: &'a WorldIdentity,
    pub(super) owner: &'a str,
    pub(super) authority_mode: Option<oasis7_agent_api::game::TaskContextAuthorityModeV1>,
    pub(super) runtime: &'a Arc<Mutex<oasis7_node::NodeRuntime>>,
    pub(super) signer: &'a crate::feedback_submit_api::FeedbackSubmitSigner,
    pub(super) store: &'a PrivateGoalStore,
    pub(super) goal: &'a oasis7_client_api::world_service::private_goal::SignedGoalPrepareV1,
    pub(super) create: &'a SubmitIntentRequest<WorldServicePayloadV1>,
    pub(super) proof: &'a oasis7_client_api::world_service::SignedReadRequest<TaskContextPrepareV1>,
    pub(super) delegate: &'a str,
    pub(super) prepared: &'a PreparedTaskContextV1,
    pub(super) original_context_file: &'a Path,
    pub(super) get: &'a oasis7_client_api::world_service::SignedReadRequest<TaskContextGetV1>,
    pub(super) cas: &'a LocalCasStore,
    pub(super) prefix_revision: u16,
    pub(super) age: u64,
}

struct InitialPrefixInputs<'a> {
    driver: &'a mut NodeRuntimeExecutionDriver,
    dir: &'a Path,
    world: &'a WorldIdentity,
    runtime: &'a Arc<Mutex<oasis7_node::NodeRuntime>>,
    signer: &'a crate::feedback_submit_api::FeedbackSubmitSigner,
    store: &'a PrivateGoalStore,
    goal: &'a oasis7_client_api::world_service::private_goal::SignedGoalPrepareV1,
    create: &'a SubmitIntentRequest<WorldServicePayloadV1>,
    proof: &'a oasis7_client_api::world_service::SignedReadRequest<TaskContextPrepareV1>,
    delegate: &'a str,
    prepared: &'a PreparedTaskContextV1,
    cas: &'a LocalCasStore,
    prefix_revision: u16,
    age: u64,
}

fn admit_initial_context_prefix(inputs: InitialPrefixInputs<'_>) {
    let InitialPrefixInputs {
        driver,
        dir,
        world,
        runtime,
        signer,
        store,
        goal,
        create,
        proof,
        delegate,
        prepared,
        cas: _cas,
        prefix_revision,
        age,
    } = inputs;
    let mut prefix = public_prefix(&prepared).unwrap();
    prefix.task_prefix_binding_revision = prefix_revision;
    // A valid bound executor cannot extend a public context past the lease
    // simply by bypassing encrypted preparation and signing another prefix.
    let mut overlong = prefix.clone();
    let SchedulerOperationV1::TaskProviderPrefix { context } = &mut overlong.operation else {
        unreachable!()
    };
    context.valid_until_tick.0 += 1;
    let overlong = sign_read_request("scheduler", overlong, &delegate).unwrap();
    assert!(
        derive_correlation(
            (*world).clone(),
            &WorldServicePayloadV1::Scheduler(overlong.clone())
        )
        .is_err(),
        "correctly signed overlong public prefix must fail correlation validation"
    );
    let before_overlong = boxed_context_value(|| driver.execution_world.snapshot());
    assert!(
        driver
            .execution_world
            .apply_authenticated_scheduler(overlong)
            .is_err()
    );
    run_context_stage(|| assert_eq!(driver.execution_world.snapshot(), *before_overlong));

    // Capture and prefix admission may straddle a real empty simulation block.
    // Preserve the original signed preparation; subsequent reads must still
    // verify its capture receipt and return a fresh public observation bundle.
    if prefix_revision == 1 {
        commit_empty_context_block(driver, 41);
        assert!(driver.execution_world.state().time > prefix.captured_base_binding.base_tick);
    } else {
        // Legacy exact-base admission still needs contiguous commit history,
        // but its intervening administrative block must preserve the base.
        commit(driver, 41, create.clone());
    }
    let public = serde_json::to_string(&prefix).unwrap();
    for secret in [
        &goal.request.content.goal_text,
        &goal.request.goal_digest.0,
        &prepared.reference_salt,
        &proof.request.client_nonce,
    ] {
        assert!(!public.contains(secret));
    }
    let submit = request(
        &world,
        WorldServicePayloadV1::Scheduler(
            sign_read_request("scheduler", prefix, &delegate).unwrap(),
        ),
    );
    commit(driver, age + 7, submit.clone());
    assert!(
        game_task_tests::recorded(&driver, &submit)
            .rejected
            .is_none(),
        "canonical action rejection: {:?}",
        game_task_tests::recorded(&driver, &submit).rejected
    );
    // A distinct preparation uses a fresh public base, but cannot open a second live turn.
    let mut competing = proof.request.clone();
    competing.client_operation_id = "competing-context".into();
    let competing = sign_read_request(TASK_CONTEXT_PREPARE_PATH, competing, &delegate).unwrap();
    let competing = consumer(&dir, &runtime, &signer, &store, &world, |client| {
        client.prepare_task_context(&competing).unwrap()
    });
    let competing_submit = request(
        &world,
        WorldServicePayloadV1::Scheduler(
            sign_read_request("scheduler", public_prefix(&competing).unwrap(), &delegate).unwrap(),
        ),
    );
    commit(driver, age + 8, competing_submit.clone());
    assert!(
        game_task_tests::recorded(&driver, &competing_submit)
            .rejected
            .is_some()
    );
}

struct InitialCaptureReadInputs<'a> {
    dir: &'a Path,
    world: &'a WorldIdentity,
    runtime: &'a Arc<Mutex<oasis7_node::NodeRuntime>>,
    signer: &'a crate::feedback_submit_api::FeedbackSubmitSigner,
    store: &'a PrivateGoalStore,
    goal: &'a oasis7_client_api::world_service::private_goal::SignedGoalPrepareV1,
    proof: &'a oasis7_client_api::world_service::SignedReadRequest<TaskContextPrepareV1>,
    delegate: &'a str,
    prepared: &'a PreparedTaskContextV1,
    original_context_file: &'a Path,
    get: &'a oasis7_client_api::world_service::SignedReadRequest<TaskContextGetV1>,
    cas: &'a LocalCasStore,
    age: u64,
}

fn read_and_verify_initial_context_capture(
    inputs: InitialCaptureReadInputs<'_>,
) -> CapturedTaskContextV1 {
    let InitialCaptureReadInputs {
        dir,
        world,
        runtime,
        signer,
        store,
        goal,
        proof,
        delegate: _delegate,
        prepared,
        original_context_file,
        get,
        cas,
        age,
    } = inputs;
    // A pending capture remains usable even when a full maintenance window excludes its height.
    checkpoint::run_execution_bridge_retention_maintenance(&dir.join("records"), &cas, 1).unwrap();
    assert!(
        super::super::super::world_service_read::pin_at_height(
            &dir.join("records"),
            &dir.join("store"),
            &world,
            age + 7
        )
        .is_ok()
    );
    let captured = consumer(&dir, &runtime, &signer, &store, &world, |client| {
        client.read_task_context(&get).unwrap()
    });
    assert_eq!(captured.capture_commit.position, age + 7);
    assert_eq!(
        captured
            .prepared
            .snapshot
            .active_goal
            .owner_proof
            .request
            .content,
        goal.request.content
    );
    // Missing historical capture evidence fails before decrypting/delivering context.
    let capture_path = checkpoint::execution_bridge_record_path(&dir.join("records"), age + 7);
    let capture_bytes = std::fs::read(&capture_path).unwrap();
    let mut missing: serde_json::Value = serde_json::from_slice(&capture_bytes).unwrap();
    missing["snapshot_ref"] = serde_json::Value::Null;
    std::fs::write(&capture_path, serde_json::to_vec(&missing).unwrap()).unwrap();
    let (status, error) = http(
        &dir,
        &runtime,
        &signer,
        Some(&store),
        TASK_CONTEXT_READ_PATH,
        &get,
        "",
    );
    assert_eq!(status, 503);
    assert_eq!(error["error"], "private_goal_recovery_required");
    assert!(error.get("payload").is_none());
    std::fs::write(&capture_path, capture_bytes).unwrap();
    // Authenticated capture cannot conceal a missing or corrupted encrypted record.
    let context_file = original_context_file;
    let encrypted = std::fs::read(&context_file).unwrap();
    std::fs::write(&context_file, b"corrupt").unwrap();
    assert_eq!(
        http(
            &dir,
            &runtime,
            &signer,
            Some(&store),
            TASK_CONTEXT_READ_PATH,
            &get,
            ""
        )
        .0,
        503
    );
    std::fs::write(&context_file, &encrypted).unwrap();
    for field in 0..4 {
        let mut bad = prepared.clone();
        match field {
            0 => bad
                .snapshot
                .active_goal
                .owner_proof
                .request
                .content
                .goal_text
                .push('!'),
            1 => bad.reference_salt = "00".repeat(32),
            2 => {
                bad.identity.provider_invocation_commitment =
                    "blake3:".to_owned() + &"00".repeat(32)
            }
            _ => bad.prepare_proof.request.client_nonce = "12".repeat(32),
        };
        assert!(bad.validate_against(&proof).is_err());
    }
    captured
}

struct CapturedActionInputs<'a> {
    driver: &'a mut NodeRuntimeExecutionDriver,
    dir: &'a Path,
    world: &'a WorldIdentity,
    runtime: &'a Arc<Mutex<oasis7_node::NodeRuntime>>,
    signer: &'a crate::feedback_submit_api::FeedbackSubmitSigner,
    store: &'a PrivateGoalStore,
    goal: &'a oasis7_client_api::world_service::private_goal::SignedGoalPrepareV1,
    proof: &'a oasis7_client_api::world_service::SignedReadRequest<TaskContextPrepareV1>,
    delegate: &'a str,
    prepared: &'a PreparedTaskContextV1,
    get: &'a oasis7_client_api::world_service::SignedReadRequest<TaskContextGetV1>,
    cas: &'a LocalCasStore,
    owner: &'a str,
    authority_mode: Option<oasis7_agent_api::game::TaskContextAuthorityModeV1>,
    age: u64,
    captured: CapturedTaskContextV1,
}

fn apply_captured_context_and_publish_replacement(
    inputs: CapturedActionInputs<'_>,
) -> ReplacementTaskContext {
    let CapturedActionInputs {
        driver,
        dir,
        world,
        runtime,
        signer,
        store,
        goal,
        proof,
        delegate,
        prepared,
        get,
        cas,
        owner,
        authority_mode,
        age,
        captured,
    } = inputs;
    let action = cognition_intent(
        &captured,
        Action::MoveAgent {
            agent_id: "agent-a".into(),
            to: oasis7_agent_api::world_action::GeoPos::new(1, 0, 0),
        },
    )
    .unwrap();
    let submit = request(
        &world,
        WorldServicePayloadV1::Cognition(
            sign_read_request("cognition", action, &delegate).unwrap(),
        ),
    );
    commit(driver, age + 9, submit.clone());
    assert!(
        game_task_tests::recorded(&driver, &submit)
            .rejected
            .is_none(),
        "canonical action rejection: {:?}",
        game_task_tests::recorded(&driver, &submit).rejected
    );
    assert_eq!(
        driver.execution_world.state().agents["agent-a"].state.pos,
        oasis7_agent_api::world_action::GeoPos::new(1, 0, 0)
    );
    assert_eq!(
        http(
            &dir,
            &runtime,
            &signer,
            Some(&store),
            TASK_CONTEXT_READ_PATH,
            &get,
            ""
        )
        .0,
        409
    );
    let canonical =
        boxed_context_value(|| serde_json::to_string(&driver.execution_world.snapshot()).unwrap());
    for secret in [
        &goal.request.content.goal_text,
        &goal.request.goal_digest.0,
        &prepared.reference_salt,
        &proof.request.client_nonce,
    ] {
        assert!(!canonical.contains(secret));
    }
    let replacement = publish_replacement_goal(ReplacementGoalInputs {
        driver,
        dir: &dir,
        world: &world,
        owner: &owner,
        runtime: &runtime,
        signer: &signer,
        store: &store,
        get: &get,
        cas: &cas,
        prepared: &prepared,
        goal: &goal,
        authority_mode,
        delegate: &delegate,
        age,
    });
    replacement
}

pub(super) fn verify_initial_context_capture(
    inputs: InitialCaptureInputs<'_>,
) -> ReplacementTaskContext {
    let InitialCaptureInputs {
        driver,
        dir,
        world,
        owner,
        authority_mode,
        runtime,
        signer,
        store,
        goal,
        create,
        proof,
        delegate,
        prepared,
        original_context_file,
        get,
        cas,
        prefix_revision,
        age,
    } = inputs;
    admit_initial_context_prefix(InitialPrefixInputs {
        driver: &mut *driver,
        dir,
        world,
        runtime,
        signer,
        store,
        goal,
        create,
        proof,
        delegate,
        prepared,
        cas,
        prefix_revision,
        age,
    });
    let captured = read_and_verify_initial_context_capture(InitialCaptureReadInputs {
        dir,
        world,
        runtime,
        signer,
        store,
        goal,
        proof,
        delegate,
        prepared,
        original_context_file,
        get,
        cas,
        age,
    });
    apply_captured_context_and_publish_replacement(CapturedActionInputs {
        driver: &mut *driver,
        dir,
        world,
        runtime,
        signer,
        store,
        goal,
        proof,
        delegate,
        prepared,
        get,
        cas,
        owner,
        authority_mode,
        age,
        captured,
    })
}
