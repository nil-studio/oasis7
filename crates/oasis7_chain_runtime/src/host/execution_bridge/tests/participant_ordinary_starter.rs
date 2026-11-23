//! Fresh participant → public finite claim → real native industrial actions.
//! No per-participant bootstrap inventory, provider fixture, or test grant.
use super::*;
use oasis7_world_service::{
    private_goal_prepare::sign_goal_prepare, private_goal_read::publication,
    private_goal_store::PrivateGoalStore, private_task_context::*, task_factory_quote::*,
    task_recipe_quote::*,
};
use oasis7_agent_api::world_action::{
    MaterialLedgerId, ORDINARY_STARTER_FACTORY_KIND, ORDINARY_STARTER_PROFILE_ID,
    ORDINARY_STARTER_RECIPE_ID, OrdinaryStarterCommandV1, OrdinaryStarterStatusV1, ResourceKind,
    WorldRuleCommandV1,
};
use oasis7_client_api::world_service::participant_discovery::*;
use serde_json::Value;
use std::cell::RefCell;
#[path = "participant_ordinary_starter_handoff.rs"]
mod handoff;
#[path = "participant_ordinary_starter_recovery.rs"]
mod recovery;

fn agent_client(
    endpoint: &str,
    identity: &WorldIdentity,
    signer: &crate::feedback_submit_api::FeedbackSubmitSigner,
    key: &str,
    agent_id: &str,
) -> RemoteWorldServiceClient {
    RemoteWorldServiceClient::new(WorldServiceClientConfig {
        endpoint: endpoint.into(),
        trusted_service_public_key: signer.public_key_hex.clone(),
        expected_world: identity.clone(),
        scope_id: format!("agent:{agent_id}"),
        read_private_key_hex: key.into(),
        timeout: Duration::from_secs(10),
        formal_read_trust: None,
        max_response_bytes: 2 * 1024 * 1024,
    })
    .unwrap()
}

fn settled_outcome(
    client: &RemoteWorldServiceClient,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> IntentOutcome<Value> {
    client
        .submit(original.clone())
        .expect("submit retained original");
    let until = Instant::now() + Duration::from_secs(20);
    while Instant::now() < until {
        let reply = client
            .lookup(
                LookupIntentRequest {
                    contract_version: 1,
                    key: original.correlation.key.clone(),
                },
                original.signed_payload.clone(),
            )
            .expect("lookup retained original");
        match reply.outcome {
            terminal @ (IntentOutcome::Committed { .. }
            | IntentOutcome::Rejected { .. }
            | IntentOutcome::Expired) => return terminal,
            _ => std::thread::sleep(Duration::from_millis(40)),
        }
    }
    panic!("ordinary starter original did not settle");
}

fn settle(
    client: &RemoteWorldServiceClient,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> Value {
    match settled_outcome(client, original) {
        IntentOutcome::Committed { receipt, .. } => receipt,
        other => panic!("ordinary starter canonical rejection: {other:?}"),
    }
}

fn owner_read(
    endpoint: &str,
    root: &Path,
    identity: &WorldIdentity,
    signer: &crate::feedback_submit_api::FeedbackSubmitSigner,
    receipt: &GameParticipantReceiptV1,
    owner: &str,
) -> ParticipantOwnerControlResponseV1 {
    let WorldServicePayloadV1::GameParticipantRegister {
        owner: proof,
        admission,
    } = payload(
        root,
        identity,
        "ordinary-account",
        "ordinary-register",
        owner,
    )
    else {
        unreachable!()
    };
    let access = ParticipantOwnerViewRequestV1 {
        owner: proof,
        admission,
        read: sign_read_request(
            VIEW_JSON_PATH,
            ReadWorldViewRequest {
                contract_version: 1,
                world: identity.clone(),
                scope_id: format!("agent:{}", receipt.agent_id),
                observation_nonce: Some("cd".repeat(32)),
                formal_anchor: None,
                min_commit: None,
                fixed_commit: None,
                deadline_unix_ms: None,
            },
            owner,
        )
        .unwrap(),
        response_profile: Some(ParticipantOwnerViewProfileV1::OwnerIndustryV1),
    };
    let response = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
        .post(format!("{endpoint}{PARTICIPANT_OWNER_VIEW_PATH}"))
        .json(&access)
        .send()
        .unwrap();
    let status = response.status();
    let bytes = response.bytes().unwrap();
    assert_eq!(
        status.as_u16(),
        200,
        "owner read: {}",
        String::from_utf8_lossy(&bytes)
    );
    let response: SignedServiceResponse<ParticipantOwnerControlResponseV1> =
        serde_json::from_slice(&bytes).unwrap();
    verify_service_response(
        PARTICIPANT_OWNER_VIEW_PATH,
        &request_digest(PARTICIPANT_OWNER_VIEW_PATH, &access).unwrap(),
        &response,
        &signer.public_key_hex,
    )
    .unwrap();
    response.payload.validate(&access).unwrap();
    response.payload
}

fn configure_task(
    client: &RemoteWorldServiceClient,
    identity: &WorldIdentity,
    receipt: &GameParticipantReceiptV1,
    owner: &str,
    root: &Path,
    delegate: &str,
    prepared_handoff: bool,
) -> GameTaskFenceV1 {
    let current = pin(root, identity);
    let binding = current.world.current_cognition_runtime_binding().unwrap();
    settle(
        client,
        &request(
            identity,
            WorldServicePayloadV1::GameTaskCreate(
                sign_read_request(
                    GAME_TASK_CREATE_DOMAIN,
                    GameTaskCreateV1 {
                        api_rev: 1,
                        client_operation_id: "ordinary-task".into(),
                        world_id: identity.world_id.clone(),
                        genesis_digest: identity.genesis_digest.clone(),
                        branch_id: binding.branch_id.clone(),
                        reorg_epoch: DecimalU64(binding.reorg_epoch),
                        agent_id: receipt.agent_id.clone(),
                        owner_binding: receipt.owner_binding.clone(),
                        owner_generation: receipt.owner_generation,
                    },
                    owner,
                )
                .unwrap(),
            ),
        ),
    );
    let current = pin(root, identity);
    let control = game_task::current_control(&current.world, &receipt.agent_id)
        .unwrap()
        .unwrap();
    let content = GoalContentV1 {
        goal_text: if prepared_handoff {
            "In your own legally authorized factory, complete exactly one batch of iron ingots and verify its actual outputs."
        } else {
            "Claim one public starter slot, build its Smelter, move the inputs and complete exactly one iron-ingot batch."
        }.into(),
        soft_constraints: vec![],
    };
    let goal = sign_goal_prepare(
        GameGoalPrepareV1 {
            api_rev: 1,
            world_id: identity.world_id.clone(),
            genesis_digest: identity.genesis_digest.clone(),
            task_id: control.task_id.clone(),
            agent_id: receipt.agent_id.clone(),
            owner_public_key: receipt.owner_public_key.clone(),
            owner_binding: receipt.owner_binding.clone(),
            owner_generation: receipt.owner_generation,
            branch_id: binding.branch_id.clone(),
            reorg_epoch: DecimalU64(binding.reorg_epoch),
            expected_task_revision: control.task_revision,
            expected_latest_goal_revision: control.latest_goal_revision,
            goal_revision: DecimalU64(1),
            client_operation_id: "ordinary-goal".into(),
            client_nonce: "ab".repeat(32),
            goal_digest: game_goal_version_digest(&control.task_id, DecimalU64(1), &content)
                .unwrap(),
            content,
        },
        owner,
    )
    .unwrap();
    let prepared = client.prepare_goal(&goal).unwrap();
    settle(
        client,
        &request(
            identity,
            WorldServicePayloadV1::GameGoalPublish(
                sign_read_request(
                    GAME_GOAL_PUBLISH_DOMAIN,
                    publication(&goal, &prepared, "ordinary-goal-publish".into()).unwrap(),
                    owner,
                )
                .unwrap(),
            ),
        ),
    );
    let public = sign_read_request("fixture", (), delegate)
        .unwrap()
        .subject_public_key;
    settle(
        client,
        &request(
            identity,
            WorldServicePayloadV1::Delegation(
                sign_read_request(
                    "delegation",
                    AgentSignerDelegationChangeV1 {
                        world_id: identity.world_id.clone(),
                        branch_id: binding.branch_id,
                        agent_id: receipt.agent_id.clone(),
                        owner_binding: receipt.owner_binding.clone(),
                        agent_identity_generation: receipt.owner_generation.0,
                        delegate_public_key: public.clone(),
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
    let current = pin(root, identity);
    let control = game_task::current_control(&current.world, &receipt.agent_id)
        .unwrap()
        .unwrap();
    settle(
        client,
        &request(
            identity,
            WorldServicePayloadV1::GameExecutorControl(
                sign_read_request(
                    GAME_EXECUTOR_CONTROL_DOMAIN,
                    GameExecutorControlV1 {
                        api_rev: 1,
                        client_operation_id: "ordinary-executor".into(),
                        fence: GameTaskFenceV1::from_control(&control),
                        operation: GameExecutorOperationV1::Bind {
                            executor_id: "ordinary-direct-executor".into(),
                            executor_public_key: public,
                            delegation_generation: DecimalU64(1),
                        },
                    },
                    owner,
                )
                .unwrap(),
            ),
        ),
    );
    GameTaskFenceV1::from_control(
        &game_task::current_control(&pin(root, identity).world, &receipt.agent_id)
            .unwrap()
            .unwrap(),
    )
}

struct Execution<'a> {
    root: &'a Path,
    identity: &'a WorldIdentity,
    client: &'a RemoteWorldServiceClient,
    owner_client: &'a RemoteWorldServiceClient,
    owner: &'a str,
    delegate: &'a str,
    fence: &'a GameTaskFenceV1,
    stale_originals: RefCell<Vec<CanonicalIntentResultV1>>,
}
impl Execution<'_> {
    fn grant(&self, id: &str, object: &str, action: &str, limit: u64) {
        let request = native_delegation::signed(
            self.identity,
            self.owner,
            self.fence,
            id,
            GameNativeDelegationOperationV1::Install {
                grant_id: id.into(),
                object_id: object.into(),
                action_kinds: vec![action.into()],
                period_id: "ordinary-first-session".into(),
                valid_from_tick: DecimalU64(0),
                valid_until_tick: DecimalU64(10_000),
                limit_units: DecimalU64(limit),
            },
        );
        assert_eq!(
            settle(self.owner_client, &request)["authorization"]["grant"]["grant_id"],
            id
        );
    }

    fn action(
        &self,
        id: &str,
        mut make: impl FnMut(&SignedReadRequest<TaskContextGetV1>, &mut CapturedTaskContextV1) -> Action,
    ) -> SubmitIntentRequest<WorldServicePayloadV1> {
        for attempt in 0..3 {
            let context_operation = format!("{id}-context-{attempt}");
            let proof = sign_read_request(
                TASK_CONTEXT_PREPARE_PATH,
                TaskContextPrepareV1 {
                    authority_mode: Some(TaskContextAuthorityModeV1::NativeExecutorV1),
                    schema_revision: 2,
                    fence: self.fence.clone(),
                    client_operation_id: context_operation.clone(),
                    client_nonce: hex::encode(
                        blake3::hash(context_operation.as_bytes()).as_bytes(),
                    ),
                },
                self.delegate,
            )
            .unwrap();
            let prepared = self
                .client
                .prepare_task_context(&proof)
                .expect("prepare explicit native context");
            assert!(prepared.snapshot.provider_authority.is_none());
            let prefix = request(
                self.identity,
                WorldServicePayloadV1::Scheduler(
                    sign_read_request(
                        "scheduler",
                        public_prefix(&prepared).unwrap(),
                        self.delegate,
                    )
                    .unwrap(),
                ),
            );
            settle(self.client, &prefix);
            let get = sign_read_request(
                TASK_CONTEXT_READ_PATH,
                TaskContextGetV1 {
                    schema_revision: 1,
                    context: prepared.identity.clone(),
                    prepare_operation_id: proof.request.client_operation_id,
                },
                self.delegate,
            )
            .unwrap();
            let mut captured = self
                .client
                .read_task_context(&get)
                .expect("read committed native context");
            let action = make(&get, &mut captured);
            let original = request(
                self.identity,
                WorldServicePayloadV1::Cognition(
                    sign_read_request(
                        "cognition",
                        cognition_intent(&captured, action).unwrap(),
                        self.delegate,
                    )
                    .unwrap(),
                ),
            );
            match settled_outcome(self.client, &original) {
                IntentOutcome::Committed { receipt, .. } => {
                    assert_eq!(
                        settle(self.client, &original),
                        receipt,
                        "exact action replay must retain its receipt"
                    );
                    return original;
                }
                IntentOutcome::Rejected { .. } => {
                    // A new decision is legal only after this exact original has a
                    // verified terminal stale result and its old turn is closed.
                    recovery::retain_stale(self, &prepared.identity, &original);
                }
                other => panic!("ordinary action has no retryable terminal result: {other:?}"),
            }
        }
        panic!("ordinary action {id} exhausted three terminally settled attempts");
    }
}

fn starter(world: &oasis7_runtime::World, agent: &str) -> OrdinaryStarterStatusV1 {
    world.ordinary_starter_status(Some(agent))
}

#[test]
fn ordinary_participant_real_http_claim_factory_transfer_first_production_and_restart() {
    run_ordinary_first_session(None);
}

pub(super) fn run_prepared_world_service(ready: &Path) {
    run_ordinary_first_session(Some(ready));
}

fn run_ordinary_first_session(handoff_ready: Option<&Path>) {
    let mut pool = None;
    let (dir, mut driver, identity, _, _) =
        canonical_agent_chat::driver_fixture_without_provider_with_resource_preload(|world| {
            pool = Some(oasis7_world_service::ordinary_starter::initialize(world).unwrap());
        });
    let pool = pool.unwrap();
    let binding = driver
        .execution_world
        .ordinary_starter_status(None)
        .world_binding
        .unwrap();
    let offer = driver
        .execution_world
        .state()
        .ordinary_starter_offer(
            ORDINARY_STARTER_PROFILE_ID,
            &pool.supplier_agent_id,
            &binding,
            &pool.slots,
            driver.execution_world.state().time,
        )
        .unwrap();
    driver
        .execution_world
        .publish_ordinary_starter_offer(offer)
        .unwrap();
    governance::install(&mut driver.execution_world, &identity, 1, true);
    empty_commit(&mut driver);
    let root = dir.canonicalize().unwrap();
    // A longer private fixture slot keeps the unchanged 16-tick context lease
    // deterministic while genuine HTTP and fsync-backed commits are exercised.
    pin_resource_identity(&mut driver);
    let bootstrap =
        super::super::super::super::local_bootstrap::derive_service_execution_bootstrap(
            &root.join("world"),
            &root.join("records"),
            &root.join("store"),
            "w1",
        )
        .unwrap();
    let config = oasis7_node::NodeConfig::new("node-a", "w1", NodeRole::Sequencer)
        .unwrap()
        .with_tick_interval(if handoff_ready.is_some() {
            Duration::from_secs(5)
        } else {
            Duration::from_millis(500)
        })
        .unwrap();
    let mut runtime = oasis7_node::NodeRuntime::new(config)
        .with_execution_hook(driver)
        .with_local_execution_bootstrap(bootstrap);
    runtime.start().unwrap();
    let node = Arc::new(Mutex::new(runtime));
    let service_key = hex::encode([9; 32]);
    let signer = crate::feedback_submit_api::FeedbackSubmitSigner {
        public_key_hex: sign_read_request("fixture", (), &service_key)
            .unwrap()
            .subject_public_key,
        private_key_hex: service_key,
    };
    let private = PrivateGoalStore::open(&root.join("ordinary-private-goals"), [8; 32]).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let stop = AtomicBool::new(false);
    let dropped = AtomicBool::new(true);
    let (participant, first_production, originals, stale_originals) = std::thread::scope(|scope| {
        let guard = Stop {
            stop: &stop,
            node: node.clone(),
        };
        let server = scope.spawn(|| {
            transport::serve_with_private(
                listener,
                &node,
                &root,
                &signer,
                &stop,
                &dropped,
                Some(&private),
            )
        });
        let owner = hex::encode([72; 32]);
        let delegate = hex::encode([79; 32]);
        let owner_client = client(&endpoint, &identity, &signer, &owner);
        let register = request(
            &identity,
            payload(
                &root,
                &identity,
                "ordinary-account",
                "ordinary-register",
                &owner,
            ),
        );
        let participant: GameParticipantReceiptV1 =
            serde_json::from_value(settle(&owner_client, &register)).unwrap();
        let owner_client =
            agent_client(&endpoint, &identity, &signer, &owner, &participant.agent_id);
        let initial = pin(&root, &identity);
        assert_eq!(
            initial
                .world
                .agent_resource_balance(&participant.agent_id, ResourceKind::Electricity)
                .unwrap(),
            0
        );
        assert!(
            initial
                .world
                .state()
                .material_ledgers
                .get(&MaterialLedgerId::agent(&participant.agent_id))
                .is_none_or(|items| items.is_empty())
        );
        assert!(
            !initial
                .world
                .state()
                .starter_oc_claims
                .contains_key(&participant.agent_id)
        );
        assert!(
            initial
                .world
                .agent_delegation_authorizations(&participant.agent_id)
                .unwrap()
                .is_empty()
        );
        let read = owner_read(&endpoint, &root, &identity, &signer, &participant, &owner);
        let offer_status = read.view.ordinary_starter.unwrap();
        assert!(offer_status.eligible);
        let offer = offer_status.offers.values().next().unwrap();
        assert_eq!(offer.available_slots.len(), 8);
        let selected = offer.offer.slots[&offer.available_slots[0]].clone();
        let fence = configure_task(
            &owner_client,
            &identity,
            &participant,
            &owner,
            &root,
            &delegate,
            handoff_ready.is_some(),
        );
        let executor_client = agent_client(
            &endpoint,
            &identity,
            &signer,
            &delegate,
            &participant.agent_id,
        );
        let execution = Execution {
            root: &root,
            identity: &identity,
            client: &executor_client,
            owner_client: &owner_client,
            owner: &owner,
            delegate: &delegate,
            fence: &fence,
            stale_originals: RefCell::new(Vec::new()),
        };
        execution.grant(
            "ordinary-claim-grant",
            &offer.offer.offer_id,
            "WorldRule",
            1,
        );
        let claim = execution.action("ordinary-claim", |_, _| Action::WorldRule {
            actor_id: participant.agent_id.clone(),
            request_id: "ordinary-claim-domain".into(),
            command: WorldRuleCommandV1::OrdinaryStarter {
                command: Box::new(OrdinaryStarterCommandV1::Claim {
                    offer_id: offer.offer.offer_id.clone(),
                    offer_revision: offer.offer.revision,
                    slot_id: selected.slot_id.clone(),
                    offer_digest: offer.offer_digest.clone(),
                    owner_binding: participant.owner_binding.clone(),
                    owner_generation: participant.owner_generation.0,
                }),
            },
        });
        let after_claim = pin(&root, &identity);
        let claimed = starter(&after_claim.world, &participant.agent_id)
            .claim
            .unwrap();
        assert_eq!(claimed.slot_id, selected.slot_id);
        assert_eq!(
            after_claim
                .world
                .agent_resource_balance(&participant.agent_id, ResourceKind::Electricity)
                .unwrap(),
            claimed.electricity
        );
        assert_eq!(
            after_claim.world.state().material_ledgers
                [&MaterialLedgerId::agent(&participant.agent_id)],
            claimed.materials
        );
        assert_eq!(
            after_claim
                .world
                .ordinary_starter_status(None)
                .offers
                .values()
                .next()
                .unwrap()
                .available_slots
                .len(),
            7
        );
        let read = owner_read(&endpoint, &root, &identity, &signer, &participant, &owner);
        assert_eq!(read.view.ordinary_starter.unwrap().claim.unwrap(), claimed);
        assert_eq!(
            read.view
                .native_delegation_authorizations
                .as_ref()
                .unwrap()
                .len(),
            1
        );
        execution.grant(
            "ordinary-build-grant",
            &selected.factory_id,
            "BuildFactory",
            selected.construction_electricity as u64,
        );
        let build = execution.action("ordinary-build", |get, captured| {
            loop {
                let proof = sign_read_request(
                    TASK_FACTORY_QUOTE_PATH,
                    TaskFactoryQuoteRequestV1 {
                        schema_revision: 1,
                        context_proof: get.clone(),
                        expected_base_binding: captured
                            .current_public_context
                            .captured_base_binding
                            .clone(),
                        factory_id: selected.factory_id.clone(),
                        factory_kind: ORDINARY_STARTER_FACTORY_KIND.into(),
                        site_id: selected.site_id.clone(),
                    },
                    &delegate,
                )
                .unwrap();
                match executor_client.quote_task_factory(&proof) {
                    Ok(quote) => {
                        assert!(quote.quote.rejection.is_none());
                        captured.current_public_context = quote.current_public_context;
                        break quote.quote.action;
                    }
                    Err(oasis7_world_service::client::WorldServiceClientError::Rejected {
                        status: 409,
                        ..
                    }) => {
                        *captured = executor_client.read_task_context(get).unwrap();
                    }
                    Err(error) => panic!("ordinary factory quote: {error}"),
                }
            }
        });
        let until =
            Instant::now() + Duration::from_secs(if handoff_ready.is_some() { 120 } else { 15 });
        while !pin(&root, &identity)
            .world
            .state()
            .factories
            .contains_key(&selected.factory_id)
        {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(50));
        }
        let stock = pin(&root, &identity).world.state().material_ledgers
            [&MaterialLedgerId::agent(&participant.agent_id)]
            .clone();
        let mut originals = vec![register, claim, build];
        for (index, (kind, amount)) in stock
            .into_iter()
            .filter(|(_, amount)| *amount > 0)
            .enumerate()
        {
            let source = MaterialLedgerId::agent(&participant.agent_id);
            let target = MaterialLedgerId::site(&selected.site_id);
            execution.grant(
                &format!("ordinary-transfer-grant-{index}"),
                &format!("{source}->{target}:{kind}"),
                "TransferMaterial",
                1,
            );
            originals.push(
                execution.action(&format!("ordinary-transfer-{index}"), |_, _| {
                    Action::TransferMaterial {
                        requester_agent_id: participant.agent_id.clone(),
                        from_ledger: source.clone(),
                        to_ledger: target.clone(),
                        kind: kind.clone(),
                        amount,
                        distance_km: 1,
                        priority: None,
                        route_id: None,
                        route_ids: vec![],
                        auto_reroute: false,
                    }
                }),
            );
            let until = Instant::now()
                + Duration::from_secs(if handoff_ready.is_some() { 120 } else { 15 });
            while pin(&root, &identity)
                .world
                .ledger_material_balance(&target, &kind)
                != amount
            {
                assert!(Instant::now() < until);
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        let plan =
            oasis7_agent_api::world_quote::builtin_recipe_plan(ORDINARY_STARTER_RECIPE_ID, 1)
                .unwrap();
        execution.grant(
            "ordinary-recipe-grant",
            &selected.factory_id,
            "ScheduleRecipe",
            plan.power_required as u64,
        );
        if let Some(ready) = handoff_ready {
            let first = handoff::wait(handoff::Inputs {
                ready,
                root: &root,
                endpoint: &endpoint,
                identity: &identity,
                service_public_key: &signer.public_key_hex,
                participant: &participant,
                executor_private_key: &delegate,
            });
            drop(guard);
            server.join().unwrap();
            return (
                participant,
                first,
                originals,
                execution.stale_originals.into_inner(),
            );
        }
        originals.push(execution.action("ordinary-first-batch", |get, captured| {
            loop {
                let proof = sign_read_request(
                    TASK_RECIPE_QUOTE_PATH,
                    TaskRecipeQuoteRequestV1 {
                        schema_revision: 1,
                        context_proof: get.clone(),
                        expected_base_binding: captured
                            .current_public_context
                            .captured_base_binding
                            .clone(),
                        factory_id: selected.factory_id.clone(),
                        recipe_id: ORDINARY_STARTER_RECIPE_ID.into(),
                        batches: 1,
                    },
                    &delegate,
                )
                .unwrap();
                match executor_client.quote_task_recipe(&proof) {
                    Ok(quote) => {
                        assert!(
                            quote.quote.rejection.is_none(),
                            "{:?}",
                            quote.quote.rejection
                        );
                        captured.current_public_context = quote.current_public_context;
                        break quote.quote.action;
                    }
                    Err(oasis7_world_service::client::WorldServiceClientError::Rejected {
                        status: 409,
                        ..
                    }) => {
                        *captured = executor_client.read_task_context(get).unwrap();
                    }
                    Err(error) => panic!("ordinary recipe quote: {error}"),
                }
            }
        }));
        let until =
            Instant::now() + Duration::from_secs(if handoff_ready.is_some() { 120 } else { 20 });
        let first = loop {
            if let Some(first) = starter(&pin(&root, &identity).world, &participant.agent_id)
                .claim
                .unwrap()
                .first_production
            {
                break first;
            }
            assert!(
                Instant::now() < until,
                "first production must come from real completion"
            );
            std::thread::sleep(Duration::from_millis(50));
        };
        assert_eq!(first.accepted_batches, 1);
        assert_eq!(first.factory_id, selected.factory_id);
        let produced = pin(&root, &identity);
        for item in &first.produce {
            assert_eq!(
                produced
                    .world
                    .ledger_material_balance(&first.output_ledger, &item.kind),
                item.amount
            );
        }
        assert!(
            produced
                .world
                .canonical_agent_service_context(&participant.agent_id)
                .is_err()
        );
        let rows = produced
            .world
            .agent_delegation_authorizations(&participant.agent_id)
            .unwrap();
        assert_eq!(
            rows.iter()
                .find(|row| row.grant.grant_id == "ordinary-build-grant")
                .unwrap()
                .spent_units,
            selected.construction_electricity as u64
        );
        assert_eq!(
            rows.iter()
                .find(|row| row.grant.grant_id == "ordinary-recipe-grant")
                .unwrap()
                .spent_units,
            plan.power_required as u64
        );
        let before = first.clone();
        settle(&executor_client, originals.last().unwrap());
        assert_eq!(
            starter(&pin(&root, &identity).world, &participant.agent_id)
                .claim
                .unwrap()
                .first_production
                .unwrap(),
            before
        );
        drop(guard);
        server.join().unwrap();
        (
            participant,
            Some(first),
            originals,
            execution.stale_originals.into_inner(),
        )
    });
    let restored = NodeRuntimeExecutionDriver::new(
        root.join("state.json"),
        root.join("world"),
        root.join("records"),
        root.join("store"),
    )
    .unwrap();
    assert_eq!(
        starter(&restored.execution_world, &participant.agent_id)
            .claim
            .unwrap()
            .first_production,
        first_production,
    );
    if handoff_ready.is_none() {
        for stale in stale_originals {
            assert_eq!(
                serde_json::to_value(game_task_tests::recorded(&restored, &stale.request)).unwrap(),
                serde_json::to_value(stale).unwrap(),
                "terminal originals must retain the same rejection across restart",
            );
        }
        for original in originals {
            assert!(
                game_task_tests::recorded(&restored, &original)
                    .rejected
                    .is_none()
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
