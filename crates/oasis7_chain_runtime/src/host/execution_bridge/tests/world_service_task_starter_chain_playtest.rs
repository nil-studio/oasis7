//! Claimed-Agent construction and first production through the real native service.
use super::*;
use oasis7::world_service::private_task_context::*;
use oasis7::world_service::task_factory_quote::*;
use oasis7::world_service::task_recipe_quote::*;
use oasis7_runtime::*;
#[path = "world_service_task_starter_chain_expiry_recovery.rs"]
mod expiry_recovery;
#[path = "world_service_task_starter_chain_factory_quote_retry.rs"]
mod factory_quote_retry;
const SITE: &str = "starter-site";
const FACTORY: &str = STARTER_SMELTER_FACTORY_ID;

pub(super) fn initial_state() -> WorldState {
    let mut state = WorldState::default();
    state.material_ledgers.insert(
        MaterialLedgerId::agent("agent-a"),
        [
            ("structural_frame".into(), 12),
            ("heat_coil".into(), 4),
            ("refractory_brick".into(), 6),
            ("iron_ore".into(), 8),
            ("carbon_fuel".into(), 2),
        ]
        .into_iter()
        .collect(),
    );
    // Profile upsert is a bootstrap state input, unlike journaled authority setters.
    // Install it before the first tick rather than mutating a committed state root.
    let spec = builtin_factory_spec("factory.smelter.mk1", FACTORY).unwrap();
    state.factory_profiles.insert(
        FACTORY.into(),
        FactoryProfileV1 {
            factory_id: FACTORY.into(),
            tier: spec.tier,
            recipe_slots: spec.recipe_slots,
            tags: spec.tags,
        },
    );
    state
}
pub(super) fn authorize(world: &mut RuntimeWorld) {
    world
        .set_location_anchor(LocationAnchorV1 {
            location_id: "starter-location".into(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .unwrap();
    world
        .set_agent_location_authority(AgentLocationAuthorityV1 {
            agent_id: "agent-a".into(),
            location_id: "starter-location".into(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .unwrap();
    world
        .set_factory_site_authority(FactorySiteAuthorityV1 {
            site_id: SITE.into(),
            location_id: "starter-location".into(),
            owner_agent_id: "agent-a".into(),
            authorized_agent_ids: vec![],
            chunk_ready: true,
            active: true,
            authority_revision: 1,
            registered_at: 0,
        })
        .unwrap();
    world
        .set_factory_construction_power_profile(FactoryConstructionPowerProfileV1 {
            factory_id: FACTORY.into(),
            factory_kind: "factory.smelter.mk1".into(),
            source_module_id: None,
            electricity_amount: 6,
            mode: FactoryConstructionPowerMode::StartOnlySink,
            authority_revision: 1,
            active: true,
        })
        .unwrap();
    for (id, object, kind, limit) in [
        ("starter-build", FACTORY.to_string(), "BuildFactory", 6),
        ("starter-recipe", FACTORY.to_string(), "ScheduleRecipe", 8),
        (
            "starter-ore",
            format!("agent:agent-a->site:{SITE}:iron_ore"),
            "TransferMaterial",
            100,
        ),
        (
            "starter-fuel",
            format!("agent:agent-a->site:{SITE}:carbon_fuel"),
            "TransferMaterial",
            100,
        ),
    ] {
        world
            .install_test_native_delegation_grant(AgentDelegationGrantV1 {
                grant_id: id.into(),
                source_id: format!(
                    "starter_claim:agent-a:{}",
                    world.state().starter_oc_claims["agent-a"].claimed_at
                ),
                issuer_id: "owner-a".into(),
                owner_id: "owner-a".into(),
                organization_id: None,
                agent_id: "agent-a".into(),
                object_id: object,
                action_kinds: vec![kind.into()],
                revision: 1,
                period_id: "starter-chain-playtest".into(),
                valid_from_tick: 0,
                valid_until_tick: 1000,
                limit_units: limit,
                resource_kind: "electricity".into(),
                revoked: false,
            })
            .unwrap();
    }
    assert!(world.state().factories.is_empty());
    let before = serde_json::to_value(world.snapshot()).unwrap();
    let quote = world
        .builtin_factory_quote("agent-a", SITE, "factory.smelter.mk1", FACTORY)
        .unwrap();
    assert!(quote.rejection.is_none());
    assert_eq!(quote.authorization.cost_units, 6);
    assert_eq!(
        quote
            .preview
            .as_ref()
            .unwrap()
            .construction_power_obligation
            .electricity_amount,
        6
    );
    assert_eq!(before, serde_json::to_value(world.snapshot()).unwrap());
}
pub(super) fn goal() -> GoalContentV1 {
    GoalContentV1 {
        goal_text: "Build a Smelter MK1 at starter-site and finish the first three iron ingots there using the construction materials and raw feedstock in your inventory. Verify finished products and the starter industrial milestone; queued work alone is not completion.".into(),
        soft_constraints: vec!["Use only your legitimate starting inventory and avoid duplicate construction or production.".into()],
    }
}
pub(super) fn witness(world: &RuntimeWorld) -> serde_json::Value {
    serde_json::json!({"factory":world.state().factories.get(FACTORY),
        "agent_ledger":world.state().material_ledgers.get(&MaterialLedgerId::agent("agent-a")),
        "site_ledger":world.state().material_ledgers.get(&MaterialLedgerId::site(SITE)),
        "site_authority":world.state().factory_site_authorities.get(SITE),
        "construction_power":world.state().factory_construction_power_profiles.get(FACTORY),
        "pending_recipe_jobs":world.state().pending_recipe_jobs,
        "industry_progress":world.state().industry_progress,
        "causal_receipts":world.agent_causal_receipts("agent-a").unwrap(),
        "electricity":world.state().agents["agent-a"].state.resources.get(oasis7_agent_api::world_action::ResourceKind::Electricity)})
}
#[test]
#[ignore = "explicit claimed-Agent starter-chain service; independent player uses shipped task Skill"]
fn private_task_starter_chain_prepared_world_live_service() {
    let ready = std::path::PathBuf::from(
        std::env::var_os("OASIS7_TASK_STARTER_CHAIN_PLAYTEST_READY")
            .expect("explicit starter-chain ready path required"),
    );
    super::run_prepared_world_service_kind(
        ready,
        Duration::from_secs(5),
        false,
        None,
        false,
        Duration::from_secs(30),
        super::PreparedWorldKind::StarterChain,
        Duration::ZERO,
    );
}

#[test]
fn private_task_starter_chain_build_feed_first_product_and_restart() {
    let _scenario_permit = super::private_task_resource_permit();
    let _probe_slot = super::prepared_world_probe_slot();
    let ready = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "oasis7-starter-chain-ready-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let deadline = Instant::now() + Duration::from_secs(300);
    std::thread::scope(|scope| {
        let host_permit = _scenario_permit.clone();
        let host = scope.spawn(|| {
            let _host_permit = host_permit;
            super::run_prepared_world_service_kind(
                ready.clone(),
                Duration::from_secs(5),
                false,
                None,
                false,
                Duration::from_secs(30),
                super::PreparedWorldKind::StarterChain,
                Duration::ZERO,
            )
        });
        while !ready.exists() {
            assert!(Instant::now() < deadline, "starter host readiness deadline");
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
        assert!(initial.world.state().factories.is_empty());
        assert_eq!(
            initial
                .world
                .ledger_material_balance(&MaterialLedgerId::agent("agent-a"), "iron_ore"),
            8
        );
        assert_eq!(
            initial
                .world
                .ledger_material_balance(&MaterialLedgerId::site(SITE), "iron_ore"),
            0
        );
        let control = game_task::current_control(&initial.world, "agent-a")
            .unwrap()
            .unwrap();
        let mut categories = CleanupCategories::default();
        let mut originals = Vec::new();
        let mut factory_quote_probe_state = FactoryQuoteProbeState::default();
        for stage in 0..4 {
            let mut settled = None;
            for attempt in 0..8 {
                assert!(
                    Instant::now() < deadline,
                    "starter chain deadline exhausted"
                );
                let proof = sign_read_request(
                    TASK_CONTEXT_PREPARE_PATH,
                    TaskContextPrepareV1 {
                        schema_revision: 1,
                        fence: GameTaskFenceV1::from_control(&control),
                        client_operation_id: format!("starter-chain-{stage}-{attempt}"),
                        client_nonce: format!("{:064x}", 900 + stage * 8 + attempt),
                    },
                    &delegate,
                )
                .unwrap();
                let prepared = retry_private(|| client.prepare_task_context(&proof), deadline);
                let prefix = request(
                    &world,
                    WorldServicePayloadV1::Scheduler(
                        sign_read_request(
                            "scheduler",
                            public_prefix(&prepared).unwrap(),
                            &delegate,
                        )
                        .unwrap(),
                    ),
                );
                match settle_cleanup_original(&client, &prefix, deadline, &mut categories) {
                    Ok(_) => (),
                    Err("canonical cleanup rejected") => {
                        assert_terminal(&client, &prefix);
                        continue;
                    }
                    Err(e) => panic!("unresolved original prefix: {e}"),
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
                let current_base = || pin().world.current_cognition_base_binding().unwrap();
                let current_tick = || current_base().base_tick;
                let mut captured =
                    match read_context_until_expiry(&client, &get, deadline, &current_tick) {
                        ContextRead::Captured(captured) => *captured,
                        ContextRead::Expired => {
                            let _ = settle_expired_context(
                                &client,
                                &delegate,
                                &world,
                                &prepared.identity,
                                &current_base,
                                deadline,
                                &mut categories,
                            );
                            continue;
                        }
                    };
                let action = match stage {
                    0 => build_action(
                        &client,
                        &delegate,
                        &get,
                        &mut captured,
                        &mut factory_quote_probe_state,
                        deadline,
                        metadata["endpoint"].as_str().unwrap(),
                        &root,
                        &world,
                        &current_tick,
                    ),
                    1 | 2 => Some(Action::TransferMaterial {
                        requester_agent_id: "agent-a".into(),
                        from_ledger: MaterialLedgerId::agent("agent-a"),
                        to_ledger: MaterialLedgerId::site(SITE),
                        kind: if stage == 1 {
                            "iron_ore"
                        } else {
                            "carbon_fuel"
                        }
                        .into(),
                        amount: if stage == 1 { 8 } else { 2 },
                        distance_km: 1,
                        priority: None,
                        route_id: None,
                        route_ids: vec![],
                        auto_reroute: false,
                    }),
                    3 => recipe_action(
                        &client,
                        &delegate,
                        &get,
                        &mut captured,
                        deadline,
                        &current_tick,
                    ),
                    _ => unreachable!(),
                };
                let Some(action) = action else {
                    let _ = settle_expired_context(
                        &client,
                        &delegate,
                        &world,
                        &prepared.identity,
                        &current_base,
                        deadline,
                        &mut categories,
                    );
                    continue;
                };
                let original = request(
                    &world,
                    WorldServicePayloadV1::Cognition(
                        sign_read_request(
                            "cognition",
                            cognition_intent(&captured, action).unwrap(),
                            &delegate,
                        )
                        .unwrap(),
                    ),
                );
                match settle_cleanup_original(&client, &original, deadline, &mut categories) {
                    Ok(first) => {
                        let duplicate =
                            settle_cleanup_original(&client, &original, deadline, &mut categories)
                                .unwrap();
                        assert_eq!(
                            first, duplicate,
                            "exact original duplicate must recover settled result"
                        );
                        if stage == 0 {
                            let WorldServicePayloadV1::Cognition(signed) = &original.signed_payload
                            else {
                                unreachable!()
                            };
                            let mut altered = signed.request.clone();
                            let Action::BuildFactory { spec, .. } = &mut altered.action else {
                                unreachable!()
                            };
                            spec.build_cost.clear();
                            let changed = request(
                                &world,
                                WorldServicePayloadV1::Cognition(
                                    sign_read_request("cognition", altered, &delegate).unwrap(),
                                ),
                            );
                            assert!(matches!(client.submit(changed.clone()), Err(oasis7::world_service::client::WorldServiceClientError::Rejected {status:503,..})));
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
                        }
                        settled = Some(original);
                        break;
                    }
                    Err("canonical cleanup rejected") => {
                        assert_terminal(&client, &original);
                        // Close the rejected captured turn using the public failure lane.
                        let mut closed = false;
                        for abort_attempt in 0..8 {
                            assert!(Instant::now() < deadline);
                            let base = pin().world.current_cognition_base_binding().unwrap();
                            let reason = if base.base_tick > prepared.identity.valid_until_tick.0 {
                                "context_expired"
                            } else {
                                "executor_aborted"
                            };
                            let mut intent =
                                public_outcome_intent(&prepared.identity, base, reason).unwrap();
                            intent.request_id = format!(
                                "{}:abort:{abort_attempt}",
                                prepared.identity.decision_request_id
                            );
                            let abort = request(
                                &world,
                                WorldServicePayloadV1::Scheduler(
                                    sign_read_request("scheduler", intent, &delegate).unwrap(),
                                ),
                            );
                            match settle_cleanup_original(
                                &client,
                                &abort,
                                deadline,
                                &mut categories,
                            ) {
                                Ok(_) => {
                                    closed = true;
                                    break;
                                }
                                Err("canonical cleanup rejected") => {
                                    assert_terminal(&client, &abort)
                                }
                                Err(e) => panic!("unresolved original abort: {e}"),
                            }
                        }
                        assert!(closed, "bounded public abort attempts exhausted");
                    }
                    Err(e) => panic!("unresolved original action: {e}"),
                }
            }
            originals.push(settled.expect("maximum eight original stage attempts exhausted"));
            loop {
                let current = pin();
                let finished = match stage {
                    0 => current.world.state().factories.contains_key(FACTORY),
                    1 => {
                        current
                            .world
                            .ledger_material_balance(&MaterialLedgerId::site(SITE), "iron_ore")
                            == 8
                    }
                    2 => {
                        current
                            .world
                            .ledger_material_balance(&MaterialLedgerId::site(SITE), "carbon_fuel")
                            == 2
                    }
                    3 => {
                        current
                            .world
                            .ledger_material_balance(&MaterialLedgerId::site(SITE), "iron_ingot")
                            == 3
                    }
                    _ => unreachable!(),
                };
                if finished {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "stage {stage} did not finish within original chain deadline"
                );
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        let delivered = pin();
        for (kind, amount) in [
            ("structural_frame", 0),
            ("heat_coil", 0),
            ("refractory_brick", 0),
            ("iron_ore", 0),
            ("carbon_fuel", 0),
        ] {
            assert_eq!(
                delivered
                    .world
                    .ledger_material_balance(&MaterialLedgerId::agent("agent-a"), kind),
                amount
            );
        }
        for (kind, amount) in [
            ("iron_ore", 4),
            ("carbon_fuel", 1),
            ("iron_ingot", 3),
            ("slag", 1),
        ] {
            assert_eq!(
                delivered
                    .world
                    .ledger_material_balance(&MaterialLedgerId::site(SITE), kind),
                amount
            );
        }
        assert!(delivered.world.state().pending_recipe_jobs.is_empty());
        assert!(delivered.world.state().pending_material_transits.is_empty());
        assert!(delivered.world.state().pending_factory_builds.is_empty());
        assert!(
            delivered
                .world
                .state()
                .industry_progress
                .starter_industrial_milestone
                .is_some()
        );
        let causal = delivered.world.agent_causal_receipts("agent-a").unwrap();
        let applied = causal
            .iter()
            .filter(|r| {
                r.disposition == "applied"
                    && ["BuildFactory", "TransferMaterial", "ScheduleRecipe"]
                        .contains(&r.action_kind.as_str())
            })
            .collect::<Vec<_>>();
        assert_eq!(applied.len(), 4);
        let total = applied
            .iter()
            .map(|r| r.authorization.as_ref().unwrap().cost_units)
            .sum::<u64>();
        assert_eq!(
            delivered.world.state().agents["agent-a"]
                .state
                .resources
                .get(oasis7_agent_api::world_action::ResourceKind::Electricity),
            100 - i64::try_from(total).unwrap()
        );
        for (kind, grant, cost) in [
            ("BuildFactory", "starter-build", 6),
            ("ScheduleRecipe", "starter-recipe", 8),
        ] {
            let receipt = applied.iter().find(|r| r.action_kind == kind).unwrap();
            assert_eq!(
                receipt.authorization.as_ref().unwrap().grant.grant_id,
                grant
            );
            assert_eq!(receipt.authorization.as_ref().unwrap().cost_units, cost);
        }
        let transfer_grants = applied
            .iter()
            .filter(|r| r.action_kind == "TransferMaterial")
            .map(|r| r.authorization.as_ref().unwrap().grant.grant_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            transfer_grants,
            ["starter-ore", "starter-fuel"].into_iter().collect()
        );
        let mut counts = [0; 4];
        for event in &delivered.world.journal().events {
            if let WorldEventBody::Domain(body) = &event.body {
                match body {
                    DomainEvent::FactoryBuildStarted {
                        spec,
                        construction_power_obligation: Some(power),
                        ..
                    } if spec.factory_id == FACTORY => {
                        counts[0] += 1;
                        assert_eq!(power.electricity_amount, 6);
                        assert_eq!(
                            power.material_ledger,
                            Some(MaterialLedgerId::agent("agent-a"))
                        );
                        assert_eq!(power.electricity_before, Some(100));
                        assert_eq!(power.electricity_after, Some(94));
                    }
                    DomainEvent::FactoryBuilt { spec, .. } if spec.factory_id == FACTORY => {
                        counts[1] += 1
                    }
                    DomainEvent::RecipeStarted { factory_id, .. } if factory_id == FACTORY => {
                        counts[2] += 1
                    }
                    DomainEvent::RecipeCompleted { factory_id, .. } if factory_id == FACTORY => {
                        counts[3] += 1
                    }
                    _ => (),
                }
            }
        }
        assert_eq!(counts, [1, 1, 1, 1]);
        write_public_json(
            &root.join("starter-chain-result.json"),
            &witness(&delivered.world),
        );
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
        for original in originals {
            let settled = restarted
                .execution_world
                .capability_revocation_state()
                .world_service_results
                .values()
                .filter_map(|v| serde_json::from_value::<CanonicalIntentResultV1>(v.clone()).ok())
                .filter(|r| r.request.correlation.key == original.correlation.key)
                .collect::<Vec<_>>();
            assert_eq!(settled.len(), 1);
            assert!(settled[0].rejected.is_none());
        }
        assert_eq!(
            restarted
                .execution_world
                .ledger_material_balance(&MaterialLedgerId::site(SITE), "iron_ingot"),
            3
        );
        assert!(
            restarted
                .execution_world
                .state()
                .industry_progress
                .starter_industrial_milestone
                .is_some()
        );
        fs::remove_file(&ready).unwrap();
        fs::remove_dir_all(root).unwrap();
    });
}
fn assert_terminal(
    client: &RemoteWorldServiceClient,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
) {
    let result = client
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
            result.outcome,
            IntentOutcome::Rejected { .. } | IntentOutcome::Expired
        ),
        "replacement requires authenticated terminal result"
    );
}

enum ContextRead {
    Captured(Box<CapturedTaskContextV1>),
    Expired,
}

#[derive(Default)]
struct FactoryQuoteProbeState {
    contract_probes_completed: bool,
    stale_base_rejected_while_live: bool,
}

fn context_expired_conflict(
    error: &oasis7::world_service::client::WorldServiceClientError,
    canonical_tick: u64,
    valid_until_tick: u64,
) -> bool {
    matches!(
        error,
        oasis7::world_service::client::WorldServiceClientError::Rejected { status: 409, .. }
    ) && canonical_tick > valid_until_tick
}

fn read_context_until_expiry<F: Fn() -> u64>(
    client: &RemoteWorldServiceClient,
    get: &SignedReadRequest<TaskContextGetV1>,
    deadline: Instant,
    canonical_tick: &F,
) -> ContextRead {
    let valid_until_tick = get.request.context.valid_until_tick.0;
    loop {
        assert!(
            Instant::now() < deadline,
            "starter chain deadline exhausted during context read"
        );
        if canonical_tick() > valid_until_tick {
            return ContextRead::Expired;
        }
        match client.read_task_context(get) {
            Ok(captured) => return ContextRead::Captured(Box::new(captured)),
            Err(error) if context_expired_conflict(&error, canonical_tick(), valid_until_tick) => {
                return ContextRead::Expired;
            }
            Err(oasis7::world_service::client::WorldServiceClientError::Rejected {
                status,
                ..
            }) => panic!("starter task context read rejected while still live (HTTP {status})"),
            Err(
                oasis7::world_service::client::WorldServiceClientError::Transport(_)
                | oasis7::world_service::client::WorldServiceClientError::Cooldown { .. },
            ) => {
                let pause = Duration::from_millis(100)
                    .min(deadline.saturating_duration_since(Instant::now()));
                if !pause.is_zero() {
                    std::thread::sleep(pause);
                }
            }
            Err(_) => panic!("starter task context read failed"),
        }
    }
}

fn settle_expired_context<F: Fn() -> oasis7_runtime::RuntimeCognitionBaseBindingV1>(
    client: &RemoteWorldServiceClient,
    delegate: &str,
    world: &WorldIdentity,
    identity: &oasis7_agent_api::game::TaskContextIdentityV1,
    current_base: &F,
    deadline: Instant,
    categories: &mut CleanupCategories,
) -> SubmitIntentRequest<WorldServicePayloadV1> {
    for abort_attempt in 0..8 {
        assert!(
            Instant::now() < deadline,
            "starter chain deadline exhausted during expired-context cleanup"
        );
        let base = current_base();
        assert!(
            base.base_tick > identity.valid_until_tick.0,
            "expired-context cleanup requires a canonical tick past the captured lease"
        );
        let mut outcome = public_outcome_intent(identity, base, "context_expired")
            .expect("expired-context cleanup must match the public task identity");
        outcome.request_id = format!(
            "{}:expired-context:{abort_attempt}",
            identity.decision_request_id
        );
        let abort = request(
            world,
            WorldServicePayloadV1::Scheduler(
                sign_read_request("scheduler", outcome, delegate).unwrap(),
            ),
        );
        match settle_cleanup_original(client, &abort, deadline, categories) {
            Ok(_) => return abort,
            Err("canonical cleanup rejected") => assert_terminal(client, &abort),
            Err(error) => panic!("unresolved expired-context cleanup: {error}"),
        }
    }
    panic!("bounded expired-context cleanup attempts exhausted");
}

#[expect(
    clippy::too_many_arguments,
    reason = "Real service quote and canonical pin security probe"
)]
fn build_action(
    client: &RemoteWorldServiceClient,
    delegate: &str,
    get: &SignedReadRequest<TaskContextGetV1>,
    captured: &mut CapturedTaskContextV1,
    probe_state: &mut FactoryQuoteProbeState,
    deadline: Instant,
    endpoint: &str,
    root: &Path,
    world: &WorldIdentity,
    canonical_tick: &impl Fn() -> u64,
) -> Option<Action> {
    while Instant::now() < deadline {
        if canonical_tick() > get.request.context.valid_until_tick.0 {
            return None;
        }
        let proof = sign_read_request(
            TASK_FACTORY_QUOTE_PATH,
            TaskFactoryQuoteRequestV1 {
                schema_revision: 1,
                context_proof: get.clone(),
                expected_base_binding: captured
                    .current_public_context
                    .captured_base_binding
                    .clone(),
                factory_id: FACTORY.into(),
                factory_kind: "factory.smelter.mk1".into(),
                site_id: SITE.into(),
            },
            delegate,
        )
        .unwrap();
        let quote = loop {
            match client.quote_task_factory(&proof) {
                Ok(value) => break Some(value),
                Err(oasis7::world_service::client::WorldServiceClientError::Rejected {
                    status: 409,
                    ..
                }) => break None,
                Err(
                    oasis7::world_service::client::WorldServiceClientError::Transport(_)
                    | oasis7::world_service::client::WorldServiceClientError::Cooldown { .. },
                ) => {
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => panic!("factory quote rejected: {e}"),
            }
        };
        let Some(quote) = quote else {
            match read_context_until_expiry(client, get, deadline, canonical_tick) {
                ContextRead::Captured(refreshed) => *captured = *refreshed,
                ContextRead::Expired => return None,
            }
            continue;
        };
        assert!(quote.quote.rejection.is_none());
        assert_eq!(quote.quote.authorization.cost_units, 6);
        assert_eq!(quote.quote.electricity_after, Some(94));
        let refresh_after_probes =
            !probe_state.contract_probes_completed || !probe_state.stale_base_rejected_while_live;
        if probe_state.contract_probes_completed {
            if !probe_state.stale_base_rejected_while_live {
                if !reject_stale_factory_quote_while_live(
                    client,
                    &proof,
                    deadline,
                    get.request.context.valid_until_tick.0,
                    canonical_tick,
                ) {
                    return None;
                }
                probe_state.stale_base_rejected_while_live = true;
            }
            if refresh_after_probes {
                match read_context_until_expiry(client, get, deadline, canonical_tick) {
                    ContextRead::Captured(refreshed) => *captured = *refreshed,
                    ContextRead::Expired => return None,
                }
                eprintln!("TASK_FACTORY_QUOTE_PROBE phase=post_probe_context_refreshed");
                continue;
            }
            eprintln!("TASK_FACTORY_QUOTE_PROBE phase=post_probe_quote_selected");
            captured.current_public_context = quote.current_public_context;
            return Some(quote.quote.action);
        }

        factory_quote_retry::assert_delayed_factory_quote_retry_reuses_original(
            &proof, &quote, delegate,
        );
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
        let repeated = super::production_playtest::retry_same_signed_private_quote_until(
            &proof,
            deadline,
            |original| client.quote_task_factory(original),
        );
        match repeated {
            Ok(repeated) => {
                assert_eq!(repeated.quote, quote.quote);
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
            }) => (),
            Err(e) => panic!("repeated quote failed: {e}"),
        }
        let outsider = sign_read_request(
            TASK_FACTORY_QUOTE_PATH,
            proof.request.clone(),
            &hex::encode([74; 32]),
        )
        .unwrap();
        assert_eq!(
            private_http(endpoint, &serde_json::to_value(outsider).unwrap(), deadline),
            400
        );
        let mut wrong = proof.request.clone();
        wrong.expected_base_binding = pin().world.current_cognition_base_binding().unwrap();
        wrong.expected_base_binding.base_tick += 1;
        assert_eq!(
            private_http(
                endpoint,
                &serde_json::to_value(
                    sign_read_request(TASK_FACTORY_QUOTE_PATH, wrong, delegate).unwrap()
                )
                .unwrap(),
                deadline
            ),
            409
        );
        let mut wrong = proof.request.clone();
        wrong.expected_base_binding = pin().world.current_cognition_base_binding().unwrap();
        wrong.context_proof.request.context.context_commitment =
            format!("blake3:{}", "44".repeat(32));
        wrong.context_proof = sign_read_request(
            TASK_CONTEXT_READ_PATH,
            wrong.context_proof.request,
            delegate,
        )
        .unwrap();
        assert_eq!(
            private_http(
                endpoint,
                &serde_json::to_value(
                    sign_read_request(TASK_FACTORY_QUOTE_PATH, wrong, delegate).unwrap()
                )
                .unwrap(),
                deadline
            ),
            409
        );
        let mut wrong = proof.request.clone();
        wrong.context_proof.request.context.fence.executor_epoch.0 += 1;
        // Keep the proof internally valid so the live fence rejects the stale epoch.
        wrong
            .context_proof
            .request
            .context
            .fence
            .executor_binding
            .as_mut()
            .unwrap()
            .executor_epoch = wrong.context_proof.request.context.fence.executor_epoch;
        wrong.context_proof = sign_read_request(
            TASK_CONTEXT_READ_PATH,
            wrong.context_proof.request,
            delegate,
        )
        .unwrap();
        assert_eq!(
            private_http(
                endpoint,
                &serde_json::to_value(
                    sign_read_request(TASK_FACTORY_QUOTE_PATH, wrong, delegate).unwrap()
                )
                .unwrap(),
                deadline
            ),
            403
        );
        let mut raw = serde_json::to_value(&proof).unwrap();
        raw["request"]["spec"] = serde_json::json!({"build_cost":[]});
        assert_eq!(private_http(endpoint, &raw, deadline), 400);
        let mut tampered = proof.clone();
        tampered.request.site_id = "foreign-site".into();
        assert_eq!(
            private_http(endpoint, &serde_json::to_value(tampered).unwrap(), deadline),
            400
        );
        let mut forged = quote.clone();
        if let Action::BuildFactory { spec, .. } = &mut forged.quote.action {
            spec.build_cost.clear();
        } else {
            unreachable!();
        }
        assert!(forged.validate_against(&proof).is_err());
        assert!(
            pin().world.state().factories.is_empty(),
            "read-only quote cannot build a factory"
        );
        let encoded = serde_json::to_string(&quote).unwrap();
        assert!(!encoded.contains(&captured.prepared.reference_salt));
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
        probe_state.contract_probes_completed = true;
        eprintln!("TASK_FACTORY_QUOTE_PROBE phase=probe_batch_completed");
        if !reject_stale_factory_quote_while_live(
            client,
            &proof,
            deadline,
            get.request.context.valid_until_tick.0,
            canonical_tick,
        ) {
            return None;
        }
        probe_state.stale_base_rejected_while_live = true;
        match read_context_until_expiry(client, get, deadline, canonical_tick) {
            ContextRead::Captured(refreshed) => *captured = *refreshed,
            ContextRead::Expired => return None,
        }
        eprintln!("TASK_FACTORY_QUOTE_PROBE phase=post_probe_context_refreshed");
        continue;
    }
    panic!("starter chain deadline exhausted during factory quote refresh");
}

fn reject_stale_factory_quote_while_live(
    client: &RemoteWorldServiceClient,
    proof: &SignedReadRequest<TaskFactoryQuoteRequestV1>,
    deadline: Instant,
    valid_until_tick: u64,
    canonical_tick: &impl Fn() -> u64,
) -> bool {
    let quoted_base_tick = proof.request.expected_base_binding.base_tick;
    while canonical_tick() <= quoted_base_tick {
        if canonical_tick() > valid_until_tick {
            return false;
        }
        assert!(
            Instant::now() < deadline,
            "starter chain deadline exhausted waiting for a canonical quote-base change"
        );
        let pause =
            Duration::from_millis(25).min(deadline.saturating_duration_since(Instant::now()));
        if !pause.is_zero() {
            std::thread::sleep(pause);
        }
    }
    if canonical_tick() > valid_until_tick {
        return false;
    }
    let stale_quote = super::production_playtest::retry_same_signed_private_quote_until(
        proof,
        deadline,
        |original| client.quote_task_factory(original),
    );
    assert!(
        matches!(
            stale_quote,
            Err(
                oasis7::world_service::client::WorldServiceClientError::Rejected {
                    status: 409,
                    ..
                }
            )
        ),
        "the original quote proof must be rejected after a canonical base change"
    );
    if canonical_tick() > valid_until_tick {
        return false;
    }
    eprintln!(
        "TASK_FACTORY_QUOTE_PROBE phase=stale_base_rejected tick_advanced=true lease_live=true"
    );
    true
}

fn recipe_action(
    client: &RemoteWorldServiceClient,
    delegate: &str,
    get: &SignedReadRequest<TaskContextGetV1>,
    captured: &mut CapturedTaskContextV1,
    deadline: Instant,
    canonical_tick: &impl Fn() -> u64,
) -> Option<Action> {
    while Instant::now() < deadline {
        if canonical_tick() > get.request.context.valid_until_tick.0 {
            return None;
        }
        let proof = sign_read_request(
            TASK_RECIPE_QUOTE_PATH,
            TaskRecipeQuoteRequestV1 {
                schema_revision: 1,
                context_proof: get.clone(),
                expected_base_binding: captured
                    .current_public_context
                    .captured_base_binding
                    .clone(),
                factory_id: FACTORY.into(),
                recipe_id: STARTER_SMELTER_RECIPE_ID.into(),
                batches: 1,
            },
            delegate,
        )
        .unwrap();
        loop {
            match client.quote_task_recipe(&proof) {
                Ok(quote) => {
                    assert!(quote.quote.rejection.is_none());
                    assert_eq!(quote.quote.authorization.cost_units, 8);
                    captured.current_public_context = quote.current_public_context;
                    return Some(quote.quote.action);
                }
                Err(oasis7::world_service::client::WorldServiceClientError::Rejected {
                    status: 409,
                    ..
                }) => break,
                Err(
                    oasis7::world_service::client::WorldServiceClientError::Transport(_)
                    | oasis7::world_service::client::WorldServiceClientError::Cooldown { .. },
                ) => {
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => panic!("recipe quote rejected: {e}"),
            }
        }
        match read_context_until_expiry(client, get, deadline, canonical_tick) {
            ContextRead::Captured(refreshed) => *captured = *refreshed,
            ContextRead::Expired => return None,
        }
    }
    panic!("starter chain deadline exhausted during recipe quote refresh");
}
fn private_http(endpoint: &str, body: &serde_json::Value, deadline: Instant) -> u16 {
    let mut stream =
        std::net::TcpStream::connect(endpoint.strip_prefix("http://").unwrap()).unwrap();
    let remaining = deadline.saturating_duration_since(Instant::now());
    assert!(
        !remaining.is_zero(),
        "starter chain HTTP probe deadline exhausted"
    );
    stream.set_read_timeout(Some(remaining)).unwrap();
    stream.set_write_timeout(Some(remaining)).unwrap();
    let body = serde_json::to_vec(body).unwrap();
    write!(stream,"POST {TASK_FACTORY_QUOTE_PATH} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",body.len()).unwrap();
    stream.write_all(&body).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(
        Instant::now() < deadline,
        "starter chain HTTP probe exceeded deadline"
    );
    assert!(
        response.contains("Cache-Control: no-store"),
        "private HTTP probe missing no-store: {response:?}"
    );
    response.split_whitespace().nth(1).unwrap().parse().unwrap()
}
