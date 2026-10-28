use super::*;

pub(super) struct InitialTaskContextFixture {
    pub(super) private_goal_dir: std::path::PathBuf,
    pub(super) create: SubmitIntentRequest<WorldServicePayloadV1>,
    pub(super) store: PrivateGoalStore,
    pub(super) goal: oasis7_client_api::world_service::private_goal::SignedGoalPrepareV1,
    pub(super) proof: oasis7_client_api::world_service::SignedReadRequest<TaskContextPrepareV1>,
    pub(super) delegate: String,
    pub(super) delegate_public: String,
    pub(super) signer: crate::feedback_submit_api::FeedbackSubmitSigner,
    pub(super) runtime: Arc<Mutex<oasis7_node::NodeRuntime>>,
}

pub(super) struct RecoveredTaskContextFixture {
    pub(super) store: PrivateGoalStore,
    pub(super) prepared: PreparedTaskContextV1,
    pub(super) original_context_file: std::path::PathBuf,
    pub(super) get: oasis7_client_api::world_service::SignedReadRequest<TaskContextGetV1>,
}

pub(super) fn prepare_initial_task_context(
    dir: &std::path::Path,
    driver: &mut NodeRuntimeExecutionDriver,
    world: &WorldIdentity,
    owner: &str,
    owner_public: &str,
    authority_mode: Option<oasis7_agent_api::game::TaskContextAuthorityModeV1>,
) -> InitialTaskContextFixture {
    let create = game_task_tests::create(world, owner, "context-task");
    commit(driver, 3, create.clone());
    let control = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    let private = dir.canonicalize().unwrap().join("private-goals");
    let store = PrivateGoalStore::open(&private, [7; 32]).unwrap();
    let content = GoalContentV1 {
        goal_text: format!("完整任务正文{}", "x".repeat(1800)),
        soft_constraints: vec!["actual full constraint".into()],
    };
    let goal = sign_goal_prepare(
        GameGoalPrepareV1 {
            api_rev: 1,
            world_id: world.world_id.clone(),
            genesis_digest: world.genesis_digest.clone(),
            task_id: control.task_id.clone(),
            agent_id: control.agent_id.clone(),
            owner_public_key: owner_public.to_owned(),
            owner_binding: control.owner_binding.clone(),
            owner_generation: control.owner_generation,
            branch_id: control.branch_id.clone(),
            reorg_epoch: control.reorg_epoch,
            expected_task_revision: control.task_revision,
            expected_latest_goal_revision: control.latest_goal_revision,
            goal_revision: DecimalU64(1),
            client_operation_id: "context-goal".into(),
            client_nonce: "ab".repeat(32),
            goal_digest: game_goal_version_digest(&control.task_id, DecimalU64(1), &content)
                .unwrap(),
            content,
        },
        owner,
    )
    .unwrap();
    let receipt = store.prepare(&goal).unwrap();
    let publish = publication(&goal, &receipt, "context-publish".into()).unwrap();
    commit(
        driver,
        4,
        request(
            world,
            WorldServicePayloadV1::GameGoalPublish(
                sign_read_request(GAME_GOAL_PUBLISH_DOMAIN, publish, owner).unwrap(),
            ),
        ),
    );
    let delegate = hex::encode([73; 32]);
    let delegate_public = sign_read_request("key", (), &delegate)
        .unwrap()
        .subject_public_key;
    commit(
        driver,
        5,
        request(
            world,
            WorldServicePayloadV1::Delegation(
                sign_read_request(
                    "delegation",
                    AgentSignerDelegationChangeV1 {
                        world_id: world.world_id.clone(),
                        branch_id: "main".into(),
                        agent_id: "agent-a".into(),
                        owner_binding: "owner-a".into(),
                        agent_identity_generation: 1,
                        delegate_public_key: delegate_public.clone(),
                        generation: 1,
                        revoked: false,
                        nonce: 1,
                    },
                    owner,
                )
                .unwrap(),
            ),
        ),
    );
    let control = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    commit(
        driver,
        6,
        request(
            world,
            WorldServicePayloadV1::GameExecutorControl(
                sign_read_request(
                    GAME_EXECUTOR_CONTROL_DOMAIN,
                    GameExecutorControlV1 {
                        api_rev: 1,
                        client_operation_id: "bind-context".into(),
                        fence: GameTaskFenceV1::from_control(&control),
                        operation: GameExecutorOperationV1::Bind {
                            executor_id: "context-executor".into(),
                            executor_public_key: delegate_public.clone(),
                            delegation_generation: DecimalU64(1),
                        },
                    },
                    owner,
                )
                .unwrap(),
            ),
        ),
    );
    let control = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    let fence = GameTaskFenceV1::from_control(&control);
    let signer = crate::feedback_submit_api::FeedbackSubmitSigner {
        private_key_hex: owner.to_owned(),
        public_key_hex: owner_public.to_owned(),
    };
    let runtime = Arc::new(Mutex::new(oasis7_node::NodeRuntime::new(
        oasis7_node::NodeConfig::new("node-a", "w1", NodeRole::Sequencer).unwrap(),
    )));
    let proof = sign_read_request(
        TASK_CONTEXT_PREPARE_PATH,
        TaskContextPrepareV1 {
            authority_mode,
            schema_revision: oasis7_agent_api::game::TaskContextAuthorityModeV1::schema_revision(
                authority_mode,
            ),
            fence,
            client_operation_id: "context-operation".into(),
            client_nonce: "cd".repeat(32),
        },
        &delegate,
    )
    .unwrap();
    InitialTaskContextFixture {
        private_goal_dir: private,
        create,
        store,
        goal,
        proof,
        delegate,
        delegate_public,
        signer,
        runtime,
    }
}

pub(super) fn prepare_and_reopen_task_context(
    dir: &std::path::Path,
    private: &std::path::Path,
    runtime: &Arc<Mutex<oasis7_node::NodeRuntime>>,
    signer: &crate::feedback_submit_api::FeedbackSubmitSigner,
    mut store: PrivateGoalStore,
    world: &WorldIdentity,
    proof: &oasis7_client_api::world_service::SignedReadRequest<TaskContextPrepareV1>,
    goal: &oasis7_client_api::world_service::private_goal::SignedGoalPrepareV1,
    delegate: &str,
    authority_mode: Option<oasis7_agent_api::game::TaskContextAuthorityModeV1>,
    other_provider: bool,
) -> RecoveredTaskContextFixture {
    let prepared = consumer(dir, runtime, signer, &store, world, |client| {
        client.prepare_task_context(proof).unwrap()
    });
    if authority_mode.is_none() {
        authority_compat::verify_original_provider_bytes(&prepared);
    }
    if !other_provider {
        authority_compat::export_test_snapshot_if_requested(&prepared);
    }
    assert_eq!(prepared.identity.authority_mode, authority_mode);
    assert_eq!(prepared.snapshot.authority_mode().unwrap(), authority_mode);
    assert_eq!(
        prepared.snapshot.active_goal.owner_proof.request.content,
        goal.request.content
    );
    let original_context_file = std::fs::read_dir(private)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("context-")
        })
        .unwrap();
    drop(store);
    store = PrivateGoalStore::open(private, [7; 32]).unwrap();
    let recovered = store.lookup_task_context(proof).unwrap().unwrap();
    assert_eq!(prepared.identity, recovered.identity);
    assert_eq!(prepared.reference_salt, recovered.reference_salt);
    let mut changed = proof.request.clone();
    changed.client_nonce = "ef".repeat(32);
    let changed = sign_read_request(TASK_CONTEXT_PREPARE_PATH, changed, delegate).unwrap();
    assert!(matches!(
        store.lookup_task_context(&changed),
        Err(oasis7::world_service::private_goal_store::PrivateTaskContextError::Conflict)
    ));
    let get = sign_read_request(
        TASK_CONTEXT_READ_PATH,
        TaskContextGetV1 {
            schema_revision: 1,
            context: prepared.identity.clone(),
            prepare_operation_id: proof.request.client_operation_id.clone(),
        },
        delegate,
    )
    .unwrap();
    assert_eq!(
        http(
            dir,
            runtime,
            signer,
            Some(&store),
            TASK_CONTEXT_READ_PATH,
            &get,
            ""
        )
        .0,
        409
    );
    RecoveredTaskContextFixture {
        store,
        prepared,
        original_context_file,
        get,
    }
}
