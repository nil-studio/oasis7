//! Isolated bootstrap production world; no live owner-generated player actions.
use super::*;
#[path = "world_service_task_production_quote_retry.rs"]
mod quote_retry;
use std::path::PathBuf;
pub(super) fn fixture() -> (
    PathBuf,
    NodeRuntimeExecutionDriver,
    WorldIdentity,
    String,
    String,
) {
    fixture_kind(false)
}
pub(super) fn fixture_kind(
    starter_chain: bool,
) -> (
    PathBuf,
    NodeRuntimeExecutionDriver,
    WorldIdentity,
    String,
    String,
) {
    let verified_authority = true;
    let dir = temp_dir("world-service-owner-chat");
    let owner = hex::encode([7u8; 32]);
    let public = sign_read_request("owner", (), &owner)
        .unwrap()
        .subject_public_key;
    let identity = WorldIdentity {
        world_id: "w1".into(),
        genesis_digest: "fixture-genesis-v1".into(),
    };
    let mut world = oasis7::runtime_host::world::new_with_state(if starter_chain {
        super::starter_chain_playtest::initial_state()
    } else {
        initial_state()
    });
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-a".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    world
        .set_agent_resource_balance(
            "agent-a",
            oasis7_agent_api::world_action::ResourceKind::Electricity,
            100,
        )
        .unwrap();
    world.submit_action(Action::ClaimStarterOc {
        agent_id: "agent-a".into(),
        player_id: "owner-a".into(),
        public_key: Some(public.clone()),
    });
    world.step().unwrap();
    assert!(world.state().starter_oc_claims.contains_key("agent-a"));
    world
        .install_capability_agent_identity("agent-a", "owner-a", 1)
        .unwrap();
    world
        .bind_cognition_runtime(
            "w1",
            "main",
            0,
            verified_authority.then(|| {
                format!(
                    "blake3:{}",
                    blake3::hash(b"native-task-finalized-provider-fixture:w1:main:0")
                )
            }),
            "pending",
            0,
        )
        .unwrap();
    world
        .install_test_provider_capability_fixture_without_cognition_balance("agent-a")
        .unwrap();
    if verified_authority {
        assert!(world.promote_cognition_runtime_finality().unwrap());
    }
    world
        .provision_cognition_for_agent("agent-a", "provision-a", "owner-a", 100)
        .unwrap();
    if starter_chain {
        super::starter_chain_playtest::authorize(&mut world);
    } else {
        world
            .install_test_native_delegation_grant(oasis7_runtime::AgentDelegationGrantV1 {
                grant_id: "production-recipe-budget".into(),
                source_id: format!(
                    "starter_claim:agent-a:{}",
                    world.state().starter_oc_claims["agent-a"].claimed_at
                ),
                issuer_id: "owner-a".into(),
                owner_id: "owner-a".into(),
                organization_id: None,
                agent_id: "agent-a".into(),
                object_id: "production-factory".into(),
                action_kinds: vec!["ScheduleRecipe".into()],
                revision: 1,
                period_id: "production-playtest".into(),
                valid_from_tick: 0,
                valid_until_tick: 1000,
                limit_units: 16,
                resource_kind: "electricity".into(),
                revoked: false,
            })
            .unwrap();
        let quote = world
            .builtin_recipe_quote(
                "agent-a",
                "production-factory",
                "recipe.smelter.iron_ingot",
                1,
            )
            .unwrap();
        assert!(quote.rejection.is_none());
        assert_eq!(quote.authorization.cost_units, 8);
    }
    let context_hash = super::super::super::execution_hash::execution_resource_context_hash("w1");
    world
        .save_to_dir_with_chain_resource_context(
            dir.join("world"),
            oasis7_runtime::ChainResourceDerivationContext {
                world_id: "w1",
                chain_id: "w1",
                genesis_ref: Some(identity.genesis_digest.as_str()),
                created_at_height: world.state().time,
                manifest_height: world.state().time,
                commit_block_hash: None,
                tick: world.state().time,
            },
            context_hash.clone(),
            context_hash,
        )
        .unwrap();
    fs::write(
        dir.join("world/world-service-identity.json"),
        serde_json::to_vec(&identity).unwrap(),
    )
    .unwrap();
    let baseline = super::super::super::local_bootstrap::derive_local_execution_bootstrap(
        &dir.join("world"),
        &dir.join("records"),
        "w1",
        world.state().time,
        "world-service-local-setup-boundary",
        &oasis7_runtime::ReleaseSecurityPolicy::default(),
    )
    .unwrap();
    let driver = NodeRuntimeExecutionDriver::new_with_local_bootstrap(
        dir.join("state.json"),
        dir.join("world"),
        dir.join("records"),
        dir.join("store"),
        &oasis7_proto::storage_profile::StorageProfileConfig::default(),
        baseline,
    )
    .unwrap();
    (dir, driver, identity, owner, public)
}
fn initial_state() -> WorldState {
    let mut state = WorldState::default();
    state.material_ledgers.insert(
        oasis7_runtime::MaterialLedgerId::site("production-site"),
        [
            ("iron_ore".into(), 8),
            ("carbon_fuel".into(), 2),
            ("iron_ingot".into(), 0),
        ]
        .into_iter()
        .collect(),
    );
    state.factory_profiles.insert(
        "production-factory".into(),
        oasis7_runtime::FactoryProfileV1 {
            factory_id: "production-factory".into(),
            tier: 1,
            recipe_slots: 1,
            tags: vec!["smelter".into()],
        },
    );
    state.factories.insert(
        "production-factory".into(),
        oasis7_runtime::FactoryState {
            factory_id: "production-factory".into(),
            site_id: "production-site".into(),
            builder_agent_id: "agent-a".into(),
            spec: oasis7_wasm_abi::FactoryModuleSpec {
                factory_id: "production-factory".into(),
                display_name: "Receiving depot".into(),
                tier: 1,
                tags: vec!["smelter".into()],
                build_cost: vec![],
                build_time_ticks: 1,
                base_power_draw: 0,
                recipe_slots: 1,
                throughput_bps: 10000,
                maintenance_per_tick: 0,
            },
            input_ledger: oasis7_runtime::MaterialLedgerId::site("production-site"),
            output_ledger: oasis7_runtime::MaterialLedgerId::site("production-site"),
            durability_ppm: 1000000,
            production: Default::default(),
            site_authority_revision: None,
            site_location_id: None,
            location_anchor_revision: None,
            construction_power_profile_key: None,
            construction_power_profile_revision: None,
            built_at: 0,
        },
    );
    state
}
pub(super) fn goal() -> GoalContentV1 {
    GoalContentV1 {
        goal_text:"Produce the first three iron ingots at production-factory using its available smelter and local feedstock. Verify that the finished products have arrived at production-site; merely queuing work does not fulfill the goal.".into(),
        soft_constraints:vec!["Do not repeat a settled production batch or produce more than the requested amount.".into()],
    }
}
pub(super) fn witness(world: &RuntimeWorld) -> serde_json::Value {
    serde_json::json!({"input_iron_ore":world.ledger_material_balance(&oasis7_runtime::MaterialLedgerId::site("production-site"),"iron_ore"),"input_carbon_fuel":world.ledger_material_balance(&oasis7_runtime::MaterialLedgerId::site("production-site"),"carbon_fuel"),"output_iron_ingot":world.ledger_material_balance(&oasis7_runtime::MaterialLedgerId::site("production-site"),"iron_ingot"),"output_slag":world.ledger_material_balance(&oasis7_runtime::MaterialLedgerId::site("production-site"),"slag"),"pending_recipe_jobs":world.state().pending_recipe_jobs,"factory":world.state().factories.get("production-factory"),"causal_receipts":world.agent_causal_receipts("agent-a").unwrap(),"electricity":world.state().agents["agent-a"].state.resources.get(oasis7_agent_api::world_action::ResourceKind::Electricity)})
}
#[test]
#[ignore = "explicit production prepared-world service; independent player uses shipped task Skill"]
fn private_task_production_prepared_world_live_service() {
    let ready = PathBuf::from(
        std::env::var_os("OASIS7_TASK_PRODUCTION_PLAYTEST_READY")
            .expect("explicit production ready path required"),
    );
    super::run_prepared_world_service_kind(
        ready,
        Duration::from_secs(5),
        false,
        None,
        false,
        Duration::from_secs(30),
        super::PreparedWorldKind::Production,
        Duration::ZERO,
    );
}
#[test]
fn private_task_production_signed_quote_first_product_and_restart() {
    let _scenario_permit = super::private_task_resource_permit();
    let _probe_slot = super::prepared_world_probe_slot();
    use oasis7::world_service::private_task_context::*;
    let ready = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "oasis7-production-test-ready-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    // Ordinary probe can slow the real tick to expose assumptions about
    // completion timing; the explicit live host retains its fixed cadence.
    let cadence_ms = std::env::var("OASIS7_QA_PRODUCTION_TICK_MS")
        .map(|value| {
            value
                .parse::<u64>()
                .expect("production probe tick must be u64")
        })
        .unwrap_or(5000);
    assert!((5000..=20000).contains(&cadence_ms));
    let cadence = Duration::from_millis(cadence_ms);
    // Cadence excludes canonical persistence and terminal-rejection recovery.
    // Use one host-bounded deadline for startup and the complete task chain;
    // starting before the host leaves at least 60 seconds of service reserve.
    let deadline = Instant::now() + Duration::from_secs(300);
    std::thread::scope(|scope| {
        let host_permit = _scenario_permit.clone();
        let host = scope.spawn(|| {
            let _host_permit = host_permit;
            super::run_prepared_world_service_kind_with_cognition_503(
                ready.clone(),
                cadence,
                false,
                None,
                false,
                Duration::from_secs(30),
                super::PreparedWorldKind::Production,
                Duration::ZERO,
                true,
            )
        });
        let until = Instant::now() + Duration::from_secs(15);
        while !ready.exists() {
            assert!(Instant::now() < until, "production host readiness timeout");
            std::thread::sleep(Duration::from_millis(20));
        }
        let metadata: serde_json::Value =
            serde_json::from_slice(&fs::read(&ready).unwrap()).unwrap();
        let root = std::path::PathBuf::from(metadata["root"].as_str().unwrap());
        struct StopHost(std::path::PathBuf);
        impl Drop for StopHost {
            fn drop(&mut self) {
                let _ = fs::write(self.0.join("service-stop"), b"stop");
            }
        }
        let stop_host = StopHost(root.clone());
        let config: serde_json::Value =
            serde_json::from_slice(&fs::read(metadata["config"].as_str().unwrap()).unwrap())
                .unwrap();
        let world = WorldIdentity {
            world_id: "w1".into(),
            genesis_digest: "fixture-genesis-v1".into(),
        };
        let delegate = hex::encode([73; 32]);
        let client = RemoteWorldServiceClient::new(WorldServiceClientConfig {
            endpoint: metadata["endpoint"].as_str().unwrap().into(),
            trusted_service_public_key: config["service_public_key"].as_str().unwrap().into(),
            expected_world: world.clone(),
            scope_id: "agent:agent-a".into(),
            read_private_key_hex: delegate.clone(),
            timeout: Duration::from_secs(5),
            formal_read_trust: None,
            max_response_bytes: 2 * 1024 * 1024,
        })
        .unwrap();
        let pin = || {
            super::super::super::super::world_service_read::pin(
                &root.join("records"),
                &root.join("store"),
                &world,
                None,
            )
            .unwrap()
        };
        let initial = pin();
        assert_eq!(
            initial.world.ledger_material_balance(
                &oasis7_runtime::MaterialLedgerId::site("production-site"),
                "iron_ore"
            ),
            8
        );
        let control = game_task::current_control(&initial.world, "agent-a")
            .unwrap()
            .unwrap();
        let mut categories = CleanupCategories::default();
        let mut attempts = Vec::new();
        let mut forced_action_rejection = false;
        let mut first_cognition_original_seen = false;
        let mut committed = None;
        for attempt in 0..8 {
            assert!(
                Instant::now() < deadline,
                "bounded task chain deadline exhausted"
            );
            let proof = sign_read_request(
                TASK_CONTEXT_PREPARE_PATH,
                TaskContextPrepareV1 {
                    schema_revision: 1,
                    fence: GameTaskFenceV1::from_control(&control),
                    client_operation_id: format!("production-delivery-context-{attempt}"),
                    client_nonce: format!("{:064x}", attempt + 145),
                },
                &delegate,
            )
            .unwrap();
            let prepared = loop {
                match client.prepare_task_context(&proof) {
                    Ok(prepared) => break prepared,
                    Err(
                        oasis7::world_service::client::WorldServiceClientError::Transport(_)
                        | oasis7::world_service::client::WorldServiceClientError::Cooldown { .. },
                    ) => {
                        assert!(
                            Instant::now() < deadline,
                            "original private preparation transport unresolved"
                        );
                        std::thread::sleep(Duration::from_millis(100));
                    }
                    Err(error) => panic!("private preparation rejected: {error}"),
                }
            };
            if attempt == 0 {
                let tick = prepared.snapshot.captured_base_binding.base_tick;
                while pin().world.state().time < tick + 2 {
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            let prefix = request(
                &world,
                WorldServicePayloadV1::Scheduler(
                    sign_read_request("scheduler", public_prefix(&prepared).unwrap(), &delegate)
                        .unwrap(),
                ),
            );
            match settle_cleanup_original(&client, &prefix, deadline, &mut categories) {
                Ok(_) => {}
                Err("canonical cleanup rejected") => {
                    retain_terminal_original(&client, &prefix, "prefix", &root, &mut attempts);
                    continue;
                }
                Err(reason) => panic!("original prefix remains unresolved: {reason}"),
            }
            let get = sign_read_request(
                TASK_CONTEXT_READ_PATH,
                TaskContextGetV1 {
                    schema_revision: 1,
                    context: prepared.identity.clone(),
                    prepare_operation_id: proof.request.client_operation_id.clone(),
                },
                &delegate,
            )
            .unwrap();
            let mut captured = retry_private(|| client.read_task_context(&get), deadline);
            let (quoted, quote_proof) =
                verified_quote(&client, &delegate, &get, &mut captured, deadline);
            if !forced_action_rejection {
                check_quote_read_contract(
                    &client,
                    &delegate,
                    &root,
                    &world,
                    &quote_proof,
                    &quoted,
                    &captured,
                    deadline,
                );
            }
            captured.current_public_context = quoted.current_public_context.clone();
            let action = quoted.quote.action.clone();
            let intent = cognition_intent(&captured, action).unwrap();
            // Force one real stale cognition as well as the first stale prefix.
            if !forced_action_rejection {
                let tick = pin().world.state().time;
                while pin().world.state().time < tick + 2 {
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            let original = request(
                &world,
                WorldServicePayloadV1::Cognition(
                    sign_read_request("cognition", intent, &delegate).unwrap(),
                ),
            );
            let expects_one_shot_503 = !first_cognition_original_seen;
            first_cognition_original_seen = true;
            let cleanup_before_original = categories;
            match settle_cleanup_original(&client, &original, deadline, &mut categories) {
                Ok(first) => {
                    assert_eq!(
                        categories.submit_http_503 - cleanup_before_original.submit_http_503,
                        if expects_one_shot_503 { 1 } else { 0 },
                        "one-shot cognition 503 count must match this original"
                    );
                    if expects_one_shot_503 {
                        assert!(
                            categories.lookup_unknown > cleanup_before_original.lookup_unknown,
                            "the exact original must be looked up after the unsigned 503"
                        );
                    }
                    committed = Some((captured, original, first, quote_proof, quoted));
                    break;
                }
                Err("canonical cleanup rejected") => {
                    assert_eq!(
                        categories.submit_http_503 - cleanup_before_original.submit_http_503,
                        if expects_one_shot_503 { 1 } else { 0 },
                        "one-shot cognition 503 count must match this original"
                    );
                    if expects_one_shot_503 {
                        assert!(
                            categories.lookup_unknown > cleanup_before_original.lookup_unknown,
                            "the exact original must be looked up after the unsigned 503"
                        );
                    }
                    retain_terminal_original(&client, &original, "action", &root, &mut attempts);
                    forced_action_rejection = true;
                    assert_eq!(
                        pin().world.ledger_material_balance(
                            &oasis7_runtime::MaterialLedgerId::site("production-site"),
                            "iron_ore"
                        ),
                        8,
                        "rejected action must have no production effect"
                    );
                    // Exact stale decisions now terminalize their original turn.
                    // Generic rejection still requires the public cleanup lane.
                    let already_terminal = verified_terminal_stale_action(
                        &client,
                        &pin().world,
                        &prepared.identity,
                        &original,
                    );
                    if !already_terminal {
                        let mut closed = false;
                        for outcome_attempt in 0..8 {
                            assert!(Instant::now() < deadline);
                            let current_base =
                                pin().world.current_cognition_base_binding().unwrap();
                            let reason =
                                if current_base.base_tick > prepared.identity.valid_until_tick.0 {
                                    "context_expired"
                                } else {
                                    "executor_aborted"
                                };
                            let mut outcome =
                                public_outcome_intent(&prepared.identity, current_base, reason)
                                    .unwrap();
                            // Scheduler request identity is independent of the exact context
                            // identity; each new operation follows a verified terminal result.
                            outcome.request_id = format!(
                                "{}:outcome:{outcome_attempt}",
                                prepared.identity.decision_request_id
                            );
                            let abort = request(
                                &world,
                                WorldServicePayloadV1::Scheduler(
                                    sign_read_request("scheduler", outcome, &delegate).unwrap(),
                                ),
                            );
                            match settle_cleanup_original(
                                &client,
                                &abort,
                                deadline,
                                &mut categories,
                            ) {
                                Ok(receipt) => {
                                    attempts.push(serde_json::json!({"phase":"abort","original":abort,"settled":receipt}));
                                    closed = true;
                                    break;
                                }
                                Err("canonical cleanup rejected") => retain_terminal_original(
                                    &client,
                                    &abort,
                                    "abort",
                                    &root,
                                    &mut attempts,
                                ),
                                Err(reason) => {
                                    panic!("original abort remains unresolved: {reason}")
                                }
                            }
                        }
                        assert!(closed, "bounded public abort attempts exhausted");
                    }
                    let subject = sign_read_request("key", (), &delegate)
                        .unwrap()
                        .subject_public_key;
                    assert!(
                        game_task::validate_task_context_capture(
                            &pin().world,
                            &prepared.identity,
                            &subject
                        )
                        .is_err(),
                        "old context must be inactive before fresh preparation"
                    );
                    write_public_json(
                        &root.join("production-task-attempts.json"),
                        &serde_json::json!({"settled_attempts":attempts}),
                    );
                }
                Err(reason) => panic!("original action remains unresolved: {reason}"),
            }
        }
        assert!(
            attempts.iter().any(|a| a["phase"] == "prefix"),
            "forced prefix must settle rejected"
        );
        assert!(
            forced_action_rejection,
            "forced stale action must settle rejected"
        );
        let (captured, original, first, quote_proof, quoted) =
            committed.expect("maximum eight task attempts exhausted");
        let duplicate = settle_cleanup_original(
            &client,
            &original,
            Instant::now() + Duration::from_secs(5),
            &mut categories,
        )
        .unwrap();
        assert_eq!(first, duplicate);
        let WorldServicePayloadV1::Cognition(signed) = &original.signed_payload else {
            unreachable!()
        };
        let mut altered_action = signed.request.action.clone();
        if let Action::ScheduleRecipe { plan, .. } = &mut altered_action {
            plan.accepted_batches += 1;
        } else {
            unreachable!();
        }
        let changed = request(
            &world,
            WorldServicePayloadV1::Cognition(
                sign_read_request(
                    "cognition",
                    cognition_intent(&captured, altered_action).unwrap(),
                    &delegate,
                )
                .unwrap(),
            ),
        );
        assert!(matches!(
            client.submit(changed.clone()),
            Err(
                oasis7::world_service::client::WorldServiceClientError::Rejected {
                    status: 503,
                    ..
                }
            )
        ));
        assert!(
            client
                .lookup(
                    LookupIntentRequest {
                        contract_version: 1,
                        key: original.correlation.key.clone()
                    },
                    changed.signed_payload
                )
                .is_err()
        );
        let delivered = loop {
            let p = pin();
            if p.world.ledger_material_balance(
                &oasis7_runtime::MaterialLedgerId::site("production-site"),
                "iron_ingot",
            ) == 3
            {
                break p;
            }
            assert!(
                Instant::now() < deadline,
                "first product never completed within task chain deadline"
            );
            std::thread::sleep(Duration::from_millis(100));
        };
        assert_eq!(
            delivered.world.ledger_material_balance(
                &oasis7_runtime::MaterialLedgerId::site("production-site"),
                "iron_ore"
            ),
            4
        );
        assert_eq!(
            delivered.world.ledger_material_balance(
                &oasis7_runtime::MaterialLedgerId::site("production-site"),
                "carbon_fuel"
            ),
            1
        );
        assert_eq!(
            delivered.world.ledger_material_balance(
                &oasis7_runtime::MaterialLedgerId::site("production-site"),
                "slag"
            ),
            1
        );
        assert!(delivered.world.state().pending_recipe_jobs.is_empty());
        let causal = delivered.world.agent_causal_receipts("agent-a").unwrap();
        let receipt = causal
            .iter()
            .find(|r| r.action_kind == "ScheduleRecipe" && r.disposition == "applied")
            .unwrap();
        assert_eq!(
            receipt.authorization.as_ref().unwrap().grant.grant_id,
            "production-recipe-budget"
        );
        assert_eq!(receipt.authorization.as_ref().unwrap().cost_units, 8);
        let starts=delivered.world.journal().events.iter().filter(|e|matches!(&e.body,oasis7_runtime::WorldEventBody::Domain(oasis7_runtime::DomainEvent::RecipeStarted {requester_agent_id,..}) if requester_agent_id=="agent-a")).count();
        let finishes=delivered.world.journal().events.iter().filter(|e|matches!(&e.body,oasis7_runtime::WorldEventBody::Domain(oasis7_runtime::DomainEvent::RecipeCompleted {requester_agent_id,..}) if requester_agent_id=="agent-a")).count();
        assert_eq!(starts, 1);
        assert_eq!(finishes, 1);
        quote_retry::assert_delayed_quote_retry_reuses_original(&quote_proof, &quoted, &delegate);
        drop(stop_host);
        assert_eq!(host.join().unwrap(), root);
        let bootstrap =
            super::super::super::super::local_bootstrap::derive_service_execution_bootstrap(
                &root.join("world"),
                &root.join("records"),
                &root.join("store"),
                "w1",
            )
            .unwrap();
        let restarted = NodeRuntimeExecutionDriver::new_with_local_bootstrap(
            root.join("state.json"),
            root.join("world"),
            root.join("records"),
            root.join("store"),
            &oasis7_proto::storage_profile::StorageProfileConfig::default(),
            bootstrap,
        )
        .unwrap();
        let matches = restarted
            .execution_world
            .capability_revocation_state()
            .world_service_results
            .values()
            .filter_map(|v| serde_json::from_value::<CanonicalIntentResultV1>(v.clone()).ok())
            .filter(|r| r.request.correlation.key == original.correlation.key)
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1);
        assert!(matches[0].rejected.is_none());
        assert_eq!(
            restarted.execution_world.ledger_material_balance(
                &oasis7_runtime::MaterialLedgerId::site("production-site"),
                "iron_ingot"
            ),
            3
        );
        fs::remove_file(&ready).unwrap();
        fs::remove_dir_all(root).unwrap();
    });
}

fn retain_terminal_original(
    client: &RemoteWorldServiceClient,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
    phase: &str,
    root: &Path,
    attempts: &mut Vec<serde_json::Value>,
) {
    let settled = client
        .lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: original.correlation.key.clone(),
            },
            original.signed_payload.clone(),
        )
        .unwrap();
    assert!(
        matches!(
            settled.outcome,
            IntentOutcome::Rejected { .. } | IntentOutcome::Expired
        ),
        "only authenticated terminal result permits replacement"
    );
    println!(
        "TASK_LOGISTICS_SETTLED_REJECTION {}",
        serde_json::to_string(&settled).unwrap()
    );
    attempts.push(serde_json::json!({"phase":phase,"original":original,"settled":settled}));
    write_public_json(
        &root.join("production-task-attempts.json"),
        &serde_json::json!({"settled_attempts":attempts}),
    );
}

fn verified_quote(
    client: &RemoteWorldServiceClient,
    delegate: &str,
    get: &SignedReadRequest<oasis7::world_service::private_task_context::TaskContextGetV1>,
    captured: &mut oasis7::world_service::private_task_context::CapturedTaskContextV1,
    deadline: Instant,
) -> (
    oasis7::world_service::task_recipe_quote::TaskRecipeQuoteV1,
    SignedReadRequest<oasis7::world_service::task_recipe_quote::TaskRecipeQuoteRequestV1>,
) {
    use oasis7::world_service::task_recipe_quote::*;
    while Instant::now() < deadline {
        let proof = sign_read_request(
            TASK_RECIPE_QUOTE_PATH,
            TaskRecipeQuoteRequestV1 {
                schema_revision: 1,
                context_proof: get.clone(),
                expected_base_binding: captured
                    .current_public_context
                    .captured_base_binding
                    .clone(),
                factory_id: "production-factory".into(),
                recipe_id: "recipe.smelter.iron_ingot".into(),
                batches: 1,
            },
            delegate,
        )
        .unwrap();
        loop {
            match client.quote_task_recipe(&proof) {
                Ok(quote) => {
                    assert!(quote.quote.rejection.is_none());
                    return (quote, proof);
                }
                Err(
                    oasis7::world_service::client::WorldServiceClientError::Transport(_)
                    | oasis7::world_service::client::WorldServiceClientError::Cooldown { .. },
                ) => {
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(oasis7::world_service::client::WorldServiceClientError::Rejected {
                    status: 409,
                    ..
                }) => break,
                Err(error) => panic!("typed quote read rejected: {error}"),
            }
        }
        // A quote read reserves nothing. An explicit base conflict permits a
        // fresh read of the same captured identity; no action has been submitted.
        *captured = retry_private(|| client.read_task_context(get), deadline);
    }
    panic!("production chain deadline exhausted during quote refresh");
}
fn private_http(endpoint: &str, body: &serde_json::Value) -> (u16, serde_json::Value) {
    use oasis7::world_service::task_recipe_quote::TASK_RECIPE_QUOTE_PATH;
    let mut stream =
        std::net::TcpStream::connect(endpoint.strip_prefix("http://").unwrap()).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let body = serde_json::to_vec(body).unwrap();
    // A connected client may send its first request bytes later. Exercise that
    // boundary even when the fixture listener polls with nonblocking accept.
    std::thread::sleep(Duration::from_millis(50));
    write!(stream,"POST {TASK_RECIPE_QUOTE_PATH} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",body.len()).unwrap();
    stream.write_all(&body).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(
        response.contains("Cache-Control: no-store"),
        "private quote response missing no-store header (response_bytes={})",
        response.len()
    );
    let (headers, body) = response.split_once("\r\n\r\n").unwrap();
    (
        headers.split_whitespace().nth(1).unwrap().parse().unwrap(),
        serde_json::from_str(body).unwrap(),
    )
}
#[expect(
    clippy::too_many_arguments,
    reason = "Keeps the original quote proof and bounded task deadline explicit"
)]
fn check_quote_read_contract(
    client: &RemoteWorldServiceClient,
    delegate: &str,
    root: &Path,
    world: &WorldIdentity,
    proof: &SignedReadRequest<oasis7::world_service::task_recipe_quote::TaskRecipeQuoteRequestV1>,
    quoted: &oasis7::world_service::task_recipe_quote::TaskRecipeQuoteV1,
    captured: &oasis7::world_service::private_task_context::CapturedTaskContextV1,
    deadline: Instant,
) {
    use oasis7::world_service::task_recipe_quote::*;
    let config: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("client-device/client.json")).unwrap()).unwrap();
    let endpoint = config["endpoint"].as_str().unwrap();
    let pin = || {
        super::super::super::super::world_service_read::pin(
            &root.join("records"),
            &root.join("store"),
            world,
            None,
        )
        .unwrap()
    };
    let before = pin();
    let repeated = retry_same_signed_private_quote_until(proof, deadline, |original| {
        client.quote_task_recipe(original)
    });
    match repeated {
        Ok(repeated) => {
            // A canonical commit can advance without changing the runtime base.
            // The repeated read must report its actual pin, while every quote
            // and context field other than that read provenance stays identical.
            assert_eq!(repeated.read_commit.world, quoted.read_commit.world);
            assert_eq!(repeated.read_commit.binding, quoted.read_commit.binding);
            assert!(repeated.read_commit.position >= quoted.read_commit.position);
            assert_eq!(
                repeated.current_public_context.read_commit,
                repeated.read_commit
            );
            assert_eq!(
                quoted.current_public_context.read_commit,
                quoted.read_commit
            );
            if repeated.read_commit.position == quoted.read_commit.position {
                assert_eq!(repeated.read_commit, quoted.read_commit);
            }
            let mut expected = quoted.clone();
            expected.read_commit = repeated.read_commit.clone();
            expected.current_public_context.read_commit = repeated.read_commit.clone();
            assert_eq!(
                serde_json::to_value(&repeated).unwrap(),
                serde_json::to_value(&expected).unwrap()
            );
            let after = pin();
            if before.commit == after.commit {
                assert_eq!(
                    serde_json::to_value(before.world.snapshot()).unwrap(),
                    serde_json::to_value(after.world.snapshot()).unwrap()
                );
                assert_eq!(
                    serde_json::to_value(before.world.journal()).unwrap(),
                    serde_json::to_value(after.world.journal()).unwrap()
                );
            }
        }
        Err(oasis7::world_service::client::WorldServiceClientError::Rejected {
            status: 409,
            ..
        }) => assert_ne!(pin().commit, quoted.read_commit),
        Err(error) => panic!("repeated pure quote failed: {error}"),
    }
    let outsider = sign_read_request(
        TASK_RECIPE_QUOTE_PATH,
        proof.request.clone(),
        &hex::encode([74; 32]),
    )
    .unwrap();
    assert_eq!(
        private_http(endpoint, &serde_json::to_value(outsider).unwrap()).0,
        400
    );
    let mut wrong = proof.request.clone();
    wrong.expected_base_binding = pin().world.current_cognition_base_binding().unwrap();
    wrong.expected_base_binding.base_tick += 1;
    let wrong = sign_read_request(TASK_RECIPE_QUOTE_PATH, wrong, delegate).unwrap();
    assert_eq!(
        private_http(endpoint, &serde_json::to_value(wrong).unwrap()).0,
        409
    );
    let mut wrong = proof.request.clone();
    wrong.expected_base_binding = pin().world.current_cognition_base_binding().unwrap();
    wrong.context_proof.request.context.context_commitment = format!("blake3:{}", "44".repeat(32));
    wrong.context_proof = sign_read_request(
        oasis7::world_service::private_task_context::TASK_CONTEXT_READ_PATH,
        wrong.context_proof.request,
        delegate,
    )
    .unwrap();
    let wrong = sign_read_request(TASK_RECIPE_QUOTE_PATH, wrong, delegate).unwrap();
    assert_eq!(
        private_http(endpoint, &serde_json::to_value(wrong).unwrap()).0,
        409
    );
    let mut raw = serde_json::to_value(proof).unwrap();
    raw["request"]["plan"] = serde_json::json!({"produce":[]});
    assert_eq!(private_http(endpoint, &raw).0, 400);
    let encoded = serde_json::to_string(quoted).unwrap();
    assert!(
        !encoded.contains(
            &captured
                .prepared
                .snapshot
                .active_goal
                .owner_proof
                .request
                .content
                .goal_text
        )
    );
    assert!(!encoded.contains(&captured.prepared.reference_salt));
    let mut forged = quoted.clone();
    let Action::ScheduleRecipe { plan, .. } = &mut forged.quote.action else {
        unreachable!()
    };
    plan.produce.clear();
    assert!(forged.validate_against(proof).is_err());
    assert!(
        pin().world.state().pending_recipe_jobs.is_empty(),
        "quotes must not reserve production jobs"
    );
}

pub(super) fn retry_same_signed_private_quote_until<Q, T>(
    original: &SignedReadRequest<Q>,
    deadline: Instant,
    mut read: impl FnMut(
        &SignedReadRequest<Q>,
    ) -> Result<T, oasis7::world_service::client::WorldServiceClientError>,
) -> Result<T, oasis7::world_service::client::WorldServiceClientError> {
    use oasis7::world_service::client::WorldServiceClientError;

    loop {
        if Instant::now() >= deadline {
            return Err(WorldServiceClientError::Transport(
                "private quote retry deadline elapsed".into(),
            ));
        }
        match read(original) {
            Err(error)
                if matches!(
                    &error,
                    WorldServiceClientError::Transport(_)
                        | WorldServiceClientError::Cooldown { .. }
                ) =>
            {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(error);
                }
                std::thread::sleep(Duration::from_millis(100).min(remaining));
                if Instant::now() >= deadline {
                    return Err(error);
                }
            }
            result => return result,
        }
    }
}

#[test]
fn repeated_private_quote_retries_same_signed_read_within_original_deadline() {
    use oasis7::world_service::client::WorldServiceClientError;

    #[derive(serde::Serialize)]
    struct PinnedReadRequest {
        original_pin: u64,
    }

    let proof = sign_read_request(
        "read-only-recipe-quote-fixture",
        PinnedReadRequest { original_pin: 17 },
        &hex::encode([73; 32]),
    )
    .unwrap();
    let original_wire = serde_json::to_vec(&proof).unwrap();
    let original_pin = proof.request.original_pin;
    let original_address = &proof as *const _;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut calls = 0;
    let repeated = retry_same_signed_private_quote_until(&proof, deadline, |observed| {
        calls += 1;
        assert!(std::ptr::eq(original_address, observed));
        assert_eq!(observed.request.original_pin, original_pin);
        assert!(
            serde_json::to_vec(observed).unwrap() == original_wire,
            "retry must retain the exact original signed request and pin"
        );
        if calls == 1 {
            std::thread::sleep(Duration::from_millis(20));
            Err(WorldServiceClientError::Transport(
                "private transport timed out".into(),
            ))
        } else {
            Ok(42)
        }
    });
    assert_eq!(repeated.unwrap(), 42);
    assert_eq!(calls, 2);
    assert!(Instant::now() < deadline);

    let expired_deadline = Instant::now() + Duration::from_millis(10);
    let mut expired_calls = 0;
    let expired = retry_same_signed_private_quote_until(&proof, expired_deadline, |_| {
        expired_calls += 1;
        std::thread::sleep(Duration::from_millis(20));
        Err::<(), _>(WorldServiceClientError::Transport(
            "private transport timed out".into(),
        ))
    });
    assert!(matches!(
        expired,
        Err(WorldServiceClientError::Transport(_))
    ));
    assert_eq!(expired_calls, 1);

    let mut conflict_calls = 0;
    let conflict = retry_same_signed_private_quote_until(
        &proof,
        Instant::now() + Duration::from_secs(1),
        |_| {
            conflict_calls += 1;
            Err::<(), _>(WorldServiceClientError::Rejected {
                status: 409,
                message: "private goal request rejected".into(),
            })
        },
    );
    assert!(matches!(
        conflict,
        Err(WorldServiceClientError::Rejected { status: 409, .. })
    ));
    assert_eq!(conflict_calls, 1);
}
