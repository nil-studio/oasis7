//! Isolated bootstrap logistics world; no live owner-generated player actions.
use super::*;
use std::path::PathBuf;
pub(super) fn fixture() -> (
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
    let mut world = oasis7::runtime_host::world::new_with_state(initial_state());
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-a".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
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
    world
        .install_test_native_delegation_grant(oasis7_runtime::AgentDelegationGrantV1 {
            grant_id: "logistics-transfer-budget".into(),
            source_id: format!(
                "starter_claim:agent-a:{}",
                world.state().starter_oc_claims["agent-a"].claimed_at
            ),
            issuer_id: "owner-a".into(),
            owner_id: "owner-a".into(),
            organization_id: None,
            agent_id: "agent-a".into(),
            object_id: "agent:agent-a->site:logistics-site:iron_ingot".into(),
            action_kinds: vec!["TransferMaterial".into()],
            revision: 1,
            period_id: "logistics-playtest".into(),
            valid_from_tick: 0,
            valid_until_tick: 1000,
            limit_units: 100,
            resource_kind: "electricity".into(),
            revoked: false,
        })
        .unwrap();
    let quote = world
        .logistics_transfer_quote(
            "agent-a",
            &oasis7_runtime::MaterialLedgerId::agent("agent-a"),
            &oasis7_runtime::MaterialLedgerId::site("logistics-site"),
            "iron_ingot",
            10,
            1,
            None,
        )
        .unwrap();
    assert!(
        quote.submission_feasible,
        "initial logistics quote must be executable"
    );
    assert_eq!(quote.expected_received_amount, 10);
    assert_eq!(quote.ticks_until_arrival, 1);
    assert_eq!(
        quote.tariff_electricity_total, 0,
        "direct path uses native zero tariff, no fabricated cost"
    );
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
        oasis7_runtime::MaterialLedgerId::agent("agent-a"),
        [("iron_ingot".into(), 20)].into_iter().collect(),
    );
    state.material_ledgers.insert(
        oasis7_runtime::MaterialLedgerId::site("logistics-site"),
        [("iron_ingot".into(), 0)].into_iter().collect(),
    );
    state.factories.insert(
        "logistics-factory".into(),
        oasis7_runtime::FactoryState {
            factory_id: "logistics-factory".into(),
            site_id: "logistics-site".into(),
            builder_agent_id: "agent-a".into(),
            spec: oasis7_wasm_abi::FactoryModuleSpec {
                factory_id: "logistics-factory".into(),
                display_name: "Receiving depot".into(),
                tier: 1,
                tags: vec!["logistics".into()],
                build_cost: vec![],
                build_time_ticks: 1,
                base_power_draw: 0,
                recipe_slots: 1,
                throughput_bps: 10000,
                maintenance_per_tick: 0,
            },
            input_ledger: oasis7_runtime::MaterialLedgerId::site("logistics-site"),
            output_ledger: oasis7_runtime::MaterialLedgerId::site("logistics-site"),
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
    GoalContentV1 { goal_text: "Deliver exactly 10 units of iron_ingot from your agent inventory to the receiving depot at logistics-site. The destination is the input site ledger of logistics-factory, one kilometer away. Choose and submit a valid native material transfer yourself, then verify the material has arrived; movement alone does not fulfill the goal.".into(), soft_constraints: vec!["Use current canonical executor authority and the task action lane. Do not repeat a settled delivery.".into()] }
}
pub(super) fn witness(world: &RuntimeWorld) -> serde_json::Value {
    serde_json::json!({"source_balance":world.ledger_material_balance(&oasis7_runtime::MaterialLedgerId::agent("agent-a"),"iron_ingot"),"destination_balance":world.ledger_material_balance(&oasis7_runtime::MaterialLedgerId::site("logistics-site"),"iron_ingot"),"pending_transits":world.state().pending_material_transits,"transfer_receipts":world.state().direct_material_transfer_receipts,"causal_receipts":world.agent_causal_receipts("agent-a").unwrap(),"factory":world.state().factories.get("logistics-factory")})
}
#[test]
#[ignore = "explicit logistics prepared-world service; independent player uses shipped task Skill"]
fn private_task_logistics_prepared_world_live_service() {
    let ready = std::path::PathBuf::from(
        std::env::var_os("OASIS7_TASK_LOGISTICS_PLAYTEST_READY")
            .expect("explicit logistics ready path required"),
    );
    super::run_prepared_world_service_scenario(
        ready,
        Duration::from_secs(5),
        false,
        None,
        false,
        Duration::from_secs(30),
        true,
        Duration::ZERO,
    );
}

#[test]
fn private_task_logistics_signed_transfer_once_restart_and_conflict() {
    let _probe_slot = super::prepared_world_probe_slot();
    use oasis7::world_service::private_task_context::*;
    let ready = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "oasis7-logistics-test-ready-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    // The former two-second cadence was shorter than CI durable commit work
    // (canonical commit total up to 4.6 seconds, including retention/persistence).
    // Match the live five-second cadence and bound the
    // full retry/release chain to 24 ticks, retaining the eight-attempt cap.
    let cadence = Duration::from_secs(5);
    std::thread::scope(|scope| {
        let host = scope.spawn(|| {
            super::run_prepared_world_service_scenario(
                ready.clone(),
                cadence,
                false,
                None,
                false,
                Duration::from_secs(30),
                true,
                Duration::ZERO,
            )
        });
        let until = Instant::now() + Duration::from_secs(15);
        while !ready.exists() {
            assert!(Instant::now() < until, "logistics host readiness timeout");
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
                &oasis7_runtime::MaterialLedgerId::agent("agent-a"),
                "iron_ingot"
            ),
            20
        );
        let control = game_task::current_control(&initial.world, "agent-a")
            .unwrap()
            .unwrap();
        let mut categories = CleanupCategories::default();
        let deadline = Instant::now() + cadence * 24;
        let action = Action::TransferMaterial {
            requester_agent_id: "agent-a".into(),
            from_ledger: oasis7_runtime::MaterialLedgerId::agent("agent-a"),
            to_ledger: oasis7_runtime::MaterialLedgerId::site("logistics-site"),
            kind: "iron_ingot".into(),
            amount: 10,
            distance_km: 1,
            priority: None,
            route_id: None,
            route_ids: vec![],
            auto_reroute: false,
        };
        let mut attempts = Vec::new();
        let mut forced_action_rejection = false;
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
                    client_operation_id: format!("logistics-delivery-context-{attempt}"),
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
            let captured = client.read_task_context(&get).unwrap();
            let intent = cognition_intent(&captured, action.clone()).unwrap();
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
            match settle_cleanup_original(&client, &original, deadline, &mut categories) {
                Ok(first) => {
                    committed = Some((captured, original, first));
                    break;
                }
                Err("canonical cleanup rejected") => {
                    retain_terminal_original(&client, &original, "action", &root, &mut attempts);
                    forced_action_rejection = true;
                    assert_eq!(
                        pin().world.ledger_material_balance(
                            &oasis7_runtime::MaterialLedgerId::agent("agent-a"),
                            "iron_ingot"
                        ),
                        20,
                        "rejected action must have no transfer effect"
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
                        &root.join("logistics-task-attempts.json"),
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
        let (captured, original, first) = committed.expect("maximum eight task attempts exhausted");
        let duplicate = settle_cleanup_original(
            &client,
            &original,
            Instant::now() + Duration::from_secs(5),
            &mut categories,
        )
        .unwrap();
        assert_eq!(first, duplicate);
        let mut altered_action = action;
        if let Action::TransferMaterial { amount, .. } = &mut altered_action {
            *amount = 11;
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
        let arrival = Instant::now() + Duration::from_secs(15);
        let delivered = loop {
            let p = pin();
            if p.world.ledger_material_balance(
                &oasis7_runtime::MaterialLedgerId::site("logistics-site"),
                "iron_ingot",
            ) == 10
            {
                break p;
            }
            assert!(Instant::now() < arrival, "material never arrived");
            std::thread::sleep(Duration::from_millis(100));
        };
        assert_eq!(
            delivered.world.ledger_material_balance(
                &oasis7_runtime::MaterialLedgerId::agent("agent-a"),
                "iron_ingot"
            ),
            10
        );
        assert!(delivered.world.state().pending_material_transits.is_empty());
        assert!(
            delivered
                .world
                .agent_causal_receipts("agent-a")
                .unwrap()
                .iter()
                .any(|r| r.action_kind == "TransferMaterial" && r.disposition == "applied")
        );
        let causal = delivered.world.agent_causal_receipts("agent-a").unwrap();
        let receipt = causal
            .iter()
            .find(|r| r.action_kind == "TransferMaterial" && r.disposition == "applied")
            .unwrap();
        assert_eq!(
            receipt.authorization.as_ref().unwrap().grant.grant_id,
            "logistics-transfer-budget"
        );
        let starts=delivered.world.journal().events.iter().filter(|e|matches!(&e.body,oasis7_runtime::WorldEventBody::Domain(oasis7_runtime::DomainEvent::MaterialTransitStarted {requester_agent_id,..}) if requester_agent_id=="agent-a")).count();
        let finishes=delivered.world.journal().events.iter().filter(|e|matches!(&e.body,oasis7_runtime::WorldEventBody::Domain(oasis7_runtime::DomainEvent::MaterialTransitCompleted {requester_agent_id,..}) if requester_agent_id=="agent-a")).count();
        assert_eq!(starts, 1);
        assert_eq!(finishes, 1);
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
                &oasis7_runtime::MaterialLedgerId::site("logistics-site"),
                "iron_ingot"
            ),
            10
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
        &root.join("logistics-task-attempts.json"),
        &serde_json::json!({"settled_attempts":attempts}),
    );
}
