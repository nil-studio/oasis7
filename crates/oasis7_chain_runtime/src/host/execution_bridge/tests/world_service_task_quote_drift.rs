//! Deterministic real-handler classification of read-only quote races.
//! These status codes do not grant admission or extend the captured lease.
use super::super::private_goal_tests::{http, http_with_diagnostic_hook, http_with_hook};
use super::*;
use oasis7::world_service::{private_task_context::*, task_factory_quote::*, task_recipe_quote::*};
use oasis7_client_api::world_service::task_context::TASK_CONTEXT_READ_DELIVERY_DRIFT;
use std::path::PathBuf;
struct Fixture {
    dir: PathBuf,
    driver: NodeRuntimeExecutionDriver,
    world: WorldIdentity,
    owner: String,
    delegate: String,
    store: PrivateGoalStore,
    signer: crate::feedback_submit_api::FeedbackSubmitSigner,
    runtime: Arc<Mutex<oasis7_node::NodeRuntime>>,
    get: SignedReadRequest<TaskContextGetV1>,
    create: SubmitIntentRequest<WorldServicePayloadV1>,
    height: u64,
}
fn advance_one(driver: &mut NodeRuntimeExecutionDriver, height: &mut u64) {
    // Administrative intent receipts (including duplicate creates) do not
    // advance simulation time. Use the canonical empty-block path.
    let block_height = *height;
    let before = driver.execution_world.state().time;
    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".into(),
            node_id: "node-a".into(),
            proposer_id: "node-a".into(),
            height: block_height,
            slot: block_height,
            epoch: 0,
            node_block_hash: format!("node-h{block_height}"),
            action_root: compute_consensus_action_root(&[]).unwrap(),
            committed_actions: vec![],
            committed_at_unix_ms: block_height as i64 * 1000,
        })
        .unwrap();
    *height += 1;
    assert!(
        driver.execution_world.state().time > before,
        "empty canonical simulation block must advance the real clock"
    );
}
fn expire_context_until(
    driver: &mut NodeRuntimeExecutionDriver,
    height: &mut u64,
    valid_until_tick: u64,
) {
    for _ in 0..=TASK_CONTEXT_LEASE_TICKS {
        if driver.execution_world.state().time > valid_until_tick {
            break;
        }
        advance_one(driver, height);
    }
    assert!(
        driver.execution_world.state().time > valid_until_tick,
        "captured context did not expire within its actual bounded lease"
    );
}
fn fixture(recipe: bool) -> Fixture {
    let (dir, mut driver, world, owner, owner_public) = production_playtest::fixture_kind(!recipe);
    let create = game_task_tests::create(&world, &owner, "context-task");
    commit(&mut driver, 3, create.clone());
    let c = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    let private = dir.canonicalize().unwrap().join("private-goals");
    let store = PrivateGoalStore::open(&private, [7; 32]).unwrap();
    let content = production_playtest::goal();
    let goal = sign_goal_prepare(
        GameGoalPrepareV1 {
            api_rev: 1,
            world_id: world.world_id.clone(),
            genesis_digest: world.genesis_digest.clone(),
            task_id: c.task_id.clone(),
            agent_id: c.agent_id.clone(),
            owner_public_key: owner_public.clone(),
            owner_binding: c.owner_binding.clone(),
            owner_generation: c.owner_generation,
            branch_id: c.branch_id.clone(),
            reorg_epoch: c.reorg_epoch,
            expected_task_revision: c.task_revision,
            expected_latest_goal_revision: c.latest_goal_revision,
            goal_revision: DecimalU64(1),
            client_operation_id: "context-goal".into(),
            client_nonce: "ab".repeat(32),
            goal_digest: game_goal_version_digest(&c.task_id, DecimalU64(1), &content).unwrap(),
            content,
        },
        &owner,
    )
    .unwrap();
    let receipt = store.prepare(&goal).unwrap();
    let publish = publication(&goal, &receipt, "context-publish".into()).unwrap();
    commit(
        &mut driver,
        4,
        request(
            &world,
            WorldServicePayloadV1::GameGoalPublish(
                sign_read_request(GAME_GOAL_PUBLISH_DOMAIN, publish, &owner).unwrap(),
            ),
        ),
    );
    let delegate = hex::encode([73; 32]);
    let delegate_public = sign_read_request("key", (), &delegate)
        .unwrap()
        .subject_public_key;
    commit(
        &mut driver,
        5,
        request(
            &world,
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
                    &owner,
                )
                .unwrap(),
            ),
        ),
    );
    let c = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    commit(
        &mut driver,
        6,
        request(
            &world,
            WorldServicePayloadV1::GameExecutorControl(
                sign_read_request(
                    GAME_EXECUTOR_CONTROL_DOMAIN,
                    GameExecutorControlV1 {
                        api_rev: 1,
                        client_operation_id: "bind-context".into(),
                        fence: GameTaskFenceV1::from_control(&c),
                        operation: GameExecutorOperationV1::Bind {
                            executor_id: "context-executor".into(),
                            executor_public_key: delegate_public,
                            delegation_generation: DecimalU64(1),
                        },
                    },
                    &owner,
                )
                .unwrap(),
            ),
        ),
    );

    let signer = crate::feedback_submit_api::FeedbackSubmitSigner {
        private_key_hex: owner.clone(),
        public_key_hex: owner_public,
    };
    let runtime = Arc::new(Mutex::new(oasis7_node::NodeRuntime::new(
        oasis7_node::NodeConfig::new("node-a", "w1", NodeRole::Sequencer).unwrap(),
    )));
    let control = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    let prepare = sign_read_request(
        TASK_CONTEXT_PREPARE_PATH,
        TaskContextPrepareV1 {
            schema_revision: 1,
            fence: GameTaskFenceV1::from_control(&control),
            client_operation_id: "quote-classification".into(),
            client_nonce: "cd".repeat(32),
        },
        &delegate,
    )
    .unwrap();
    let (status, body) = http(
        &dir,
        &runtime,
        &signer,
        Some(&store),
        TASK_CONTEXT_PREPARE_PATH,
        &prepare,
        "",
    );
    assert_eq!(status, 200, "[response payload omitted]");
    let prepared: PreparedTaskContextV1 = serde_json::from_value(body["payload"].clone()).unwrap();
    let prefix = request(
        &world,
        WorldServicePayloadV1::Scheduler(
            sign_read_request("scheduler", public_prefix(&prepared).unwrap(), &delegate).unwrap(),
        ),
    );
    commit(&mut driver, 7, prefix.clone());
    assert!(
        game_task_tests::recorded(&driver, &prefix)
            .rejected
            .is_none()
    );
    let get = sign_read_request(
        TASK_CONTEXT_READ_PATH,
        TaskContextGetV1 {
            schema_revision: 1,
            context: prepared.identity,
            prepare_operation_id: prepare.request.client_operation_id,
        },
        &delegate,
    )
    .unwrap();
    let (status, _body) = http(
        &dir,
        &runtime,
        &signer,
        Some(&store),
        TASK_CONTEXT_READ_PATH,
        &get,
        "",
    );
    assert_eq!(status, 200, "[response payload omitted]");
    Fixture {
        dir,
        driver,
        world,
        owner,
        delegate,
        store,
        signer,
        runtime,
        get,
        create,
        height: 8,
    }
}
impl Fixture {
    fn quote(&self, recipe: bool, stale: bool) -> (&'static str, serde_json::Value) {
        let mut base = self
            .driver
            .execution_world
            .current_cognition_base_binding()
            .unwrap();
        if stale {
            base.base_tick = base.base_tick.saturating_add(1);
        }
        if recipe {
            (
                TASK_RECIPE_QUOTE_PATH,
                serde_json::to_value(
                    sign_read_request(
                        TASK_RECIPE_QUOTE_PATH,
                        TaskRecipeQuoteRequestV1 {
                            schema_revision: 1,
                            context_proof: self.get.clone(),
                            expected_base_binding: base,
                            factory_id: "production-factory".into(),
                            recipe_id: oasis7_runtime::STARTER_SMELTER_RECIPE_ID.into(),
                            batches: 1,
                        },
                        &self.delegate,
                    )
                    .unwrap(),
                )
                .unwrap(),
            )
        } else {
            (
                TASK_FACTORY_QUOTE_PATH,
                serde_json::to_value(
                    sign_read_request(
                        TASK_FACTORY_QUOTE_PATH,
                        TaskFactoryQuoteRequestV1 {
                            schema_revision: 1,
                            context_proof: self.get.clone(),
                            expected_base_binding: base,
                            factory_id: oasis7_runtime::STARTER_SMELTER_FACTORY_ID.into(),
                            factory_kind: "factory.smelter.mk1".into(),
                            site_id: "starter-site".into(),
                        },
                        &self.delegate,
                    )
                    .unwrap(),
                )
                .unwrap(),
            )
        }
    }
    fn advance(&mut self) {
        advance_one(&mut self.driver, &mut self.height);
    }
}
#[test]
fn task_factory_and_recipe_quote_classify_only_base_and_delivery_drift() {
    for terminal in ["expiry", "fence"] {
        for recipe in [false, true] {
            let mut f = fixture(recipe);
            let (auth_path, mut invalid_auth) = f.quote(recipe, false);
            invalid_auth["signature_hex"] = serde_json::json!("00".repeat(64));
            let (status, body) = http(
                &f.dir,
                &f.runtime,
                &f.signer,
                Some(&f.store),
                auth_path,
                &invalid_auth,
                "",
            );
            assert!(
                matches!(status, 400 | 403 | 409),
                "invalid auth: {status} [response payload omitted]"
            );
            assert_ne!(body["error"], TASK_QUOTE_BASE_DRIFT);
            assert_ne!(body["error"], TASK_QUOTE_DELIVERY_DRIFT);
            let (path, stale) = f.quote(recipe, true);
            let (status, body) = http(
                &f.dir,
                &f.runtime,
                &f.signer,
                Some(&f.store),
                path,
                &stale,
                "",
            );
            assert_eq!(status, 409, "{path}: [response payload omitted]");
            assert_eq!(body["error"], TASK_QUOTE_BASE_DRIFT);
            let (path, current) = f.quote(recipe, false);
            // A business-authorized quote must reach delivery. The production
            // fixture grants recipe scheduling; the starter fixture separately
            // grants construction at its declared site/factory identity.
            let (status, _body) = http(
                &f.dir,
                &f.runtime,
                &f.signer,
                Some(&f.store),
                path,
                &current,
                "",
            );
            assert_eq!(
                status, 200,
                "authorized quote {path}: [response payload omitted]"
            );
            let delivered = std::cell::Cell::new(false);
            let (status, body) = http_with_hook(
                &f.dir,
                &f.runtime,
                &f.signer,
                Some(&f.store),
                path,
                &current,
                "",
                || {
                    delivered.set(true);
                    commit(&mut f.driver, f.height, f.create.clone());
                },
            );
            f.height += 1;
            assert!(
                delivered.get(),
                "authorized quote did not reach delivery hook: {path} [response payload omitted]"
            );
            assert_eq!(status, 409, "{path}: [response payload omitted]");
            assert_eq!(body["error"], TASK_QUOTE_DELIVERY_DRIFT);
            if terminal == "expiry" {
                for _ in 0..=TASK_CONTEXT_LEASE_TICKS {
                    if f.driver.execution_world.state().time
                        > f.get.request.context.valid_until_tick.0
                    {
                        break;
                    }
                    f.advance();
                }
                assert!(
                    f.driver.execution_world.state().time
                        > f.get.request.context.valid_until_tick.0,
                    "captured context did not expire within its actual bounded lease"
                );
            } else {
                let control = game_task::current_control(&f.driver.execution_world, "agent-a")
                    .unwrap()
                    .unwrap();
                let stop = request(
                    &f.world,
                    WorldServicePayloadV1::GameExecutorControl(
                        sign_read_request(
                            GAME_EXECUTOR_CONTROL_DOMAIN,
                            GameExecutorControlV1 {
                                api_rev: 1,
                                client_operation_id: "quote-stop".into(),
                                fence: GameTaskFenceV1::from_control(&control),
                                operation: GameExecutorOperationV1::Stop,
                            },
                            &f.owner,
                        )
                        .unwrap(),
                    ),
                );
                commit(&mut f.driver, f.height, stop);
            }
            {
                let (path, proof) = f.quote(recipe, false);
                let (status, body) = http(
                    &f.dir,
                    &f.runtime,
                    &f.signer,
                    Some(&f.store),
                    path,
                    &proof,
                    "",
                );
                assert!(
                    matches!(status, 403 | 409),
                    "{terminal} {path}: {status} [response payload omitted]"
                );
                assert_ne!(body["error"], TASK_QUOTE_BASE_DRIFT);
                assert_ne!(body["error"], TASK_QUOTE_DELIVERY_DRIFT);
            }
            drop(f.store);
            std::fs::remove_dir_all(f.dir).unwrap();
        }
    }
}

#[test]
fn task_factory_and_recipe_quote_expiry_precedes_retriable_base_and_delivery_drift() {
    for recipe in [false, true] {
        let mut initial = fixture(recipe);
        let valid_until_tick = initial.get.request.context.valid_until_tick.0;
        expire_context_until(&mut initial.driver, &mut initial.height, valid_until_tick);
        let (path, stale_base) = initial.quote(recipe, true);
        let (status, body, stage) = http_with_diagnostic_hook(
            &initial.dir,
            &initial.runtime,
            &initial.signer,
            Some(&initial.store),
            path,
            &stale_base,
            "",
            || {},
        );
        assert_eq!(status, 409, "{path}: [response payload omitted]");
        assert_eq!(
            body,
            serde_json::json!({"error":"private_goal_conflict"}),
            "expired initial capture must not be classified as retriable quote drift"
        );
        assert_eq!(stage, Some("initial_capture_expired"));
        drop(initial.store);
        std::fs::remove_dir_all(initial.dir).unwrap();

        let mut delivery = fixture(recipe);
        let (path, proof) = delivery.quote(recipe, false);
        let valid_until_tick = delivery.get.request.context.valid_until_tick.0;
        let delivered = std::cell::Cell::new(false);
        let (status, body, stage) = http_with_diagnostic_hook(
            &delivery.dir,
            &delivery.runtime,
            &delivery.signer,
            Some(&delivery.store),
            path,
            &proof,
            "",
            || {
                delivered.set(true);
                expire_context_until(&mut delivery.driver, &mut delivery.height, valid_until_tick);
            },
        );
        assert!(delivered.get(), "quote did not reach delivery: {path}");
        assert_eq!(status, 409, "{path}: [response payload omitted]");
        assert_eq!(
            body,
            serde_json::json!({"error":"private_goal_conflict"}),
            "expired delivery capture must not be classified as retriable quote drift"
        );
        assert_eq!(stage, Some("delivery_capture_expired"));
        drop(delivery.store);
        std::fs::remove_dir_all(delivery.dir).unwrap();
    }
}

#[test]
fn task_context_read_refuses_delivery_head_drift() {
    let mut f = fixture(true);
    let error_code =
        |body: &serde_json::Value| match body.get("error").and_then(serde_json::Value::as_str) {
            Some(TASK_CONTEXT_READ_DELIVERY_DRIFT) => TASK_CONTEXT_READ_DELIVERY_DRIFT,
            Some("private_goal_forbidden") => "private_goal_forbidden",
            Some("private_goal_conflict") => "private_goal_conflict",
            Some("private_goal_invalid") => "private_goal_invalid",
            _ => "other_or_none",
        };
    // Independent request handles let the delivery hook advance the actual
    // fixture clock while the handler retains its original immutable pin.
    let dir = f.dir.clone();
    let runtime = Arc::clone(&f.runtime);
    let signer = crate::feedback_submit_api::FeedbackSubmitSigner {
        private_key_hex: f.signer.private_key_hex.clone(),
        public_key_hex: f.signer.public_key_hex.clone(),
    };
    let store = PrivateGoalStore::open(&dir.canonicalize().unwrap().join("private-goals"), [7; 32])
        .unwrap();
    let get = f.get.clone();
    let proof_before = serde_json::to_value(&get).unwrap();
    let (status, body) = http(
        &dir,
        &runtime,
        &signer,
        Some(&store),
        TASK_CONTEXT_READ_PATH,
        &get,
        "",
    );
    assert_eq!(status, 200, "initial read error_code={}", error_code(&body));
    let before: CapturedTaskContextV1 = serde_json::from_value(body["payload"].clone()).unwrap();
    let results_before = f
        .driver
        .execution_world
        .capability_revocation_state()
        .world_service_results
        .clone();
    let delivered = std::cell::Cell::new(false);
    let (status, body) = http_with_hook(
        &dir,
        &runtime,
        &signer,
        Some(&store),
        TASK_CONTEXT_READ_PATH,
        &get,
        "",
        || {
            delivered.set(true);
            f.advance();
        },
    );
    assert!(delivered.get(), "context read did not reach delivery hook");
    assert_eq!(
        status,
        409,
        "delivery must not expose the stale public base; error_code={}",
        error_code(&body)
    );
    assert!(
        body["error"] == TASK_CONTEXT_READ_DELIVERY_DRIFT,
        "delivery error_code={}",
        error_code(&body)
    );
    let (status, body) = http(
        &dir,
        &runtime,
        &signer,
        Some(&store),
        TASK_CONTEXT_READ_PATH,
        &get,
        "",
    );
    assert_eq!(
        status,
        200,
        "same original read after drift error_code={}",
        error_code(&body)
    );
    let recovered: CapturedTaskContextV1 = serde_json::from_value(body["payload"].clone()).unwrap();
    assert!(
        serde_json::to_value(&get).unwrap() == proof_before,
        "original get proof changed"
    );
    assert!(recovered.prepared.identity == before.prepared.identity);
    assert!(recovered.prepared.identity == get.request.context);
    assert!(
        recovered.prepared.valid_until_tick == before.prepared.valid_until_tick,
        "immutable prepared TTL changed"
    );
    assert!(
        recovered.prepared.prepare_proof.request.client_nonce
            == before.prepared.prepare_proof.request.client_nonce,
        "immutable prepared nonce changed"
    );
    assert_eq!(
        recovered.current_public_context.captured_base_binding,
        f.driver
            .execution_world
            .current_cognition_base_binding()
            .unwrap()
    );
    assert!(
        recovered.current_public_context.read_commit.position
            > before.current_public_context.read_commit.position
    );
    // Reads and an empty simulation block cannot submit another action or
    // synthesize a WorldService result under a replacement identity.
    assert!(
        f.driver
            .execution_world
            .capability_revocation_state()
            .world_service_results
            == results_before,
        "context reads created or changed canonical intent results"
    );
    assert!(
        f.driver
            .execution_world
            .state()
            .pending_recipe_jobs
            .is_empty()
    );
    assert!(
        f.driver
            .execution_world
            .state()
            .pending_factory_builds
            .is_empty()
    );
    drop(store);
    drop(f.store);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn task_factory_selection_denial_is_signed_read_only_and_bound_to_exact_request() {
    use oasis7_client_api::world_service::task_context::verify_private_reply;
    let mut f = fixture(false);
    let (path, value) = f.quote(false, false);
    let mut proof: SignedReadRequest<TaskFactoryQuoteRequestV1> =
        serde_json::from_value(value).unwrap();
    proof.request.factory_id = "ungranted-model-choice".into();
    proof = sign_read_request(path, proof.request, &f.delegate).unwrap();
    let before = f
        .driver
        .execution_world
        .current_cognition_base_binding()
        .unwrap();
    let results = f
        .driver
        .execution_world
        .capability_revocation_state()
        .world_service_results
        .clone();
    let (status, body) = http(
        &f.dir,
        &f.runtime,
        &f.signer,
        Some(&f.store),
        path,
        &proof,
        "",
    );
    assert_eq!(status, 200);
    let response: SignedServiceResponse<TaskFactoryQuoteResponseV1> =
        serde_json::from_value(body).unwrap();
    verify_private_reply(path, &proof, &response, &f.signer.public_key_hex).unwrap();
    response.payload.validate_against(&proof).unwrap();
    let TaskFactoryQuoteResponseV1::Denied(denial) = &response.payload else {
        panic!("selection must be denied")
    };
    assert_eq!(denial.reason, FactoryQuoteDenialV1::FactoryNotAuthorized);
    assert!(denial.no_action_submitted);
    let mut changed = proof.clone();
    changed.request.factory_id = "other-choice".into();
    changed = sign_read_request(path, changed.request, &f.delegate).unwrap();
    assert!(verify_private_reply(path, &changed, &response, &f.signer.public_key_hex).is_err());
    let mut tampered = response.clone();
    let TaskFactoryQuoteResponseV1::Denied(denial) = &mut tampered.payload else {
        unreachable!()
    };
    denial.no_action_submitted = false;
    assert!(tampered.payload.validate_against(&proof).is_err());
    assert!(verify_private_reply(path, &proof, &tampered, &f.signer.public_key_hex).is_err());
    let mut wrong_context = response.clone();
    let TaskFactoryQuoteResponseV1::Denied(denial) = &mut wrong_context.payload else {
        unreachable!()
    };
    denial.context.fence.agent_id = "other-agent".into();
    assert!(wrong_context.payload.validate_against(&proof).is_err());
    let foreign = sign_read_request("key", (), &hex::encode([24; 32]))
        .unwrap()
        .subject_public_key;
    assert!(verify_private_reply(path, &proof, &response, &foreign).is_err());
    assert_eq!(
        before,
        f.driver
            .execution_world
            .current_cognition_base_binding()
            .unwrap()
    );
    assert!(
        results
            == f.driver
                .execution_world
                .capability_revocation_state()
                .world_service_results
    );
    assert!(
        f.driver
            .execution_world
            .state()
            .pending_factory_builds
            .is_empty()
    );
    let control = game_task::current_control(&f.driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    let stop = request(
        &f.world,
        WorldServicePayloadV1::GameExecutorControl(
            sign_read_request(
                GAME_EXECUTOR_CONTROL_DOMAIN,
                GameExecutorControlV1 {
                    api_rev: 1,
                    client_operation_id: "denial-delivery-stop".into(),
                    fence: GameTaskFenceV1::from_control(&control),
                    operation: GameExecutorOperationV1::Stop,
                },
                &f.owner,
            )
            .unwrap(),
        ),
    );
    let (status, _) = http_with_hook(
        &f.dir,
        &f.runtime,
        &f.signer,
        Some(&f.store),
        path,
        &proof,
        "",
        || {
            commit(&mut f.driver, f.height, stop);
        },
    );
    assert!(
        matches!(status, 403 | 409),
        "revoked fence cannot deliver a correctable denial"
    );
    let (status, _) = http(
        &f.dir,
        &f.runtime,
        &f.signer,
        Some(&f.store),
        path,
        &proof,
        "",
    );
    assert!(matches!(status, 403 | 409));
    drop(f.store);
    std::fs::remove_dir_all(f.dir).unwrap();
}
