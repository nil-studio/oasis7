//! Real owner-signed HTTP, canonical node execution, native factory quote and restore.
//! Bootstrap installs the industrial substrate only; no native grant is injected.
use super::super::private_goal_tests::http;
use super::*;
use oasis7_agent_api::world_action::ResourceKind;

const FACTORY: &str = "owner-grant.factory";
const KIND: &str = "factory.smelter.mk1";

fn industrial_substrate(world: &mut oasis7_runtime::World) {
    use oasis7_runtime::*;
    let spec = oasis7_agent_api::world_quote::builtin_factory_spec(KIND, FACTORY).unwrap();
    world
        .set_location_anchor(LocationAnchorV1 {
            location_id: "owner-grant.location".into(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .unwrap();
    world
        .set_agent_location_authority(AgentLocationAuthorityV1 {
            agent_id: "agent-a".into(),
            location_id: "owner-grant.location".into(),
            active: true,
            authority_revision: 1,
            effective_at: 0,
        })
        .unwrap();
    world
        .set_factory_site_authority(FactorySiteAuthorityV1 {
            site_id: "owner-grant.site".into(),
            location_id: "owner-grant.location".into(),
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
            factory_kind: KIND.into(),
            source_module_id: None,
            electricity_amount: 7,
            mode: FactoryConstructionPowerMode::StartOnlySink,
            authority_revision: 1,
            active: true,
        })
        .unwrap();
    world
        .upsert_factory_profile(FactoryProfileV1 {
            factory_id: FACTORY.into(),
            tier: spec.tier,
            recipe_slots: spec.recipe_slots,
            tags: spec.tags.clone(),
        })
        .unwrap();
    world
        .set_agent_resource_balance("agent-a", ResourceKind::Electricity, 100)
        .unwrap();
    for material in spec.build_cost {
        world
            .set_ledger_material_balance(
                MaterialLedgerId::agent("agent-a"),
                &material.kind,
                material.amount,
            )
            .unwrap();
    }
    assert!(
        world
            .agent_delegation_authorizations("agent-a")
            .unwrap()
            .is_empty()
    );
    // Commit the trusted substrate before persistence derives its baseline root.
    world.step().unwrap();
}

fn grant_operation(id: &str, limit: u64) -> GameNativeDelegationOperationV1 {
    GameNativeDelegationOperationV1::Install {
        grant_id: id.into(),
        object_id: FACTORY.into(),
        action_kinds: vec!["BuildFactory".into()],
        period_id: "first-factory".into(),
        valid_from_tick: DecimalU64(0),
        valid_until_tick: DecimalU64(10_000),
        limit_units: DecimalU64(limit),
    }
}

pub(super) fn signed(
    world: &WorldIdentity,
    owner: &str,
    fence: &GameTaskFenceV1,
    id: &str,
    operation: GameNativeDelegationOperationV1,
) -> SubmitIntentRequest<WorldServicePayloadV1> {
    request(
        world,
        WorldServicePayloadV1::GameNativeDelegation(
            sign_read_request(
                GAME_NATIVE_DELEGATION_DOMAIN,
                GameNativeDelegationControlV1 {
                    api_rev: 1,
                    client_operation_id: id.into(),
                    fence: fence.clone(),
                    operation,
                },
                owner,
            )
            .unwrap(),
        ),
    )
}

fn control(world: &oasis7_runtime::World) -> GameTaskFenceV1 {
    GameTaskFenceV1::from_control(
        &game_task::current_control(world, "agent-a")
            .unwrap()
            .unwrap(),
    )
}

fn lookup(original: &SubmitIntentRequest<WorldServicePayloadV1>) -> AuthenticatedLookup {
    AuthenticatedLookup {
        request: LookupIntentRequest {
            contract_version: 1,
            key: original.correlation.key.clone(),
        },
        original: original.signed_payload.clone(),
    }
}

pub(super) fn settled_http(
    root: &Path,
    node: &Arc<Mutex<oasis7_node::NodeRuntime>>,
    signer: &crate::feedback_submit_api::FeedbackSubmitSigner,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        let (status, body) = http(root, node, signer, None, LOOKUP_PATH, &lookup(original), "");
        if status == 200 {
            let outcome: IntentOutcome<serde_json::Value> =
                serde_json::from_value(body["payload"]["outcome"].clone()).unwrap();
            if let IntentOutcome::Committed { receipt, .. } = outcome {
                return receipt;
            }
            assert!(
                !matches!(outcome, IntentOutcome::Rejected { .. }),
                "unexpected canonical rejection: {body}"
            );
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    panic!("native delegation original did not commit");
}

#[test]
fn native_owner_grant_real_http_install_quote_replay_revoke_and_restore() {
    let (dir, mut driver, identity, owner, public) =
        canonical_agent_chat::driver_fixture_without_provider_with_resource_preload(
            industrial_substrate,
        );
    pin_resource_identity(&mut driver);
    let height = driver.execution_world.state().time + 1;
    commit(
        &mut driver,
        height,
        game_task_tests::create(&identity, &owner, "native-grant-task"),
    );
    let fence = control(&driver.execution_world);
    assert!(
        driver
            .execution_world
            .builtin_factory_quote("agent-a", "owner-grant.site", KIND, FACTORY)
            .is_err()
    );
    let original = signed(
        &identity,
        &owner,
        &fence,
        "grant-install",
        grant_operation("grant-build", 14),
    );
    let root = dir.canonicalize().unwrap();
    let signer = crate::feedback_submit_api::FeedbackSubmitSigner {
        private_key_hex: owner.clone(),
        public_key_hex: public,
    };
    let node = Arc::new(Mutex::new(runtime(&root, driver)));
    let stop = AtomicBool::new(false);
    let guard = Stop {
        stop: &stop,
        node: node.clone(),
    };
    let (status, body) = http(&root, &node, &signer, None, SUBMIT_PATH, &original, "");
    assert_eq!(status, 200, "install admission: {body}");
    let receipt = settled_http(&root, &node, &signer, &original);
    assert_eq!(
        receipt["authorization"]["grant"]["action_kinds"],
        serde_json::json!(["BuildFactory"])
    );
    assert_eq!(receipt["authorization"]["grant"]["owner_id"], "owner-a");
    assert_eq!(receipt["authorization"]["remaining_units"], 14);
    let current = pin(&root, &identity);
    assert!(
        current
            .world
            .canonical_agent_service_context("agent-a")
            .is_err()
    );
    let quote = current
        .world
        .builtin_factory_quote("agent-a", "owner-grant.site", KIND, FACTORY)
        .unwrap();
    assert_eq!(quote.authorization.cost_units, 7);
    assert_eq!(quote.authorization.grant.grant_id, "grant-build");
    let rows = current
        .world
        .agent_delegation_authorizations("agent-a")
        .unwrap();
    let (status, replay) = http(&root, &node, &signer, None, SUBMIT_PATH, &original, "");
    assert_eq!(status, 200, "exact replay: {replay}");
    assert_eq!(settled_http(&root, &node, &signer, &original), receipt);
    assert_eq!(
        pin(&root, &identity)
            .world
            .agent_delegation_authorizations("agent-a")
            .unwrap(),
        rows
    );
    let altered = signed(
        &identity,
        &owner,
        &fence,
        "grant-install",
        grant_operation("grant-build", 28),
    );
    assert_eq!(altered.correlation.key, original.correlation.key);
    assert_ne!(
        altered.correlation.payload_digest,
        original.correlation.payload_digest
    );
    assert_ne!(
        http(&root, &node, &signer, None, SUBMIT_PATH, &altered, "").0,
        200
    );
    let revoke = signed(
        &identity,
        &owner,
        &fence,
        "grant-revoke",
        GameNativeDelegationOperationV1::Revoke {
            grant_id: "grant-build".into(),
            expected_revision: DecimalU64(1),
        },
    );
    assert_eq!(
        http(&root, &node, &signer, None, SUBMIT_PATH, &revoke, "").0,
        200
    );
    let revoked = settled_http(&root, &node, &signer, &revoke);
    assert_eq!(revoked["authorization"]["grant"]["revision"], 2);
    assert_eq!(revoked["authorization"]["grant"]["revoked"], true);
    assert!(
        pin(&root, &identity)
            .world
            .builtin_factory_quote("agent-a", "owner-grant.site", KIND, FACTORY)
            .is_err()
    );
    assert_eq!(settled_http(&root, &node, &signer, &original), receipt);
    drop(guard);
    let restored = NodeRuntimeExecutionDriver::new(
        root.join("state.json"),
        root.join("world"),
        root.join("records"),
        root.join("store"),
    )
    .unwrap();
    let rows = restored
        .execution_world
        .agent_delegation_authorizations("agent-a")
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].grant.revoked);
    assert_eq!(rows[0].spent_units, 0);
    assert_eq!(
        game_task_tests::recorded(&restored, &original).receipt,
        receipt
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn native_owner_grant_rejects_scope_takeover_stale_fence_owner_and_budget() {
    let (dir, mut driver, identity, owner, _) =
        canonical_agent_chat::driver_fixture_without_provider_with_resource_preload(
            industrial_substrate,
        );
    pin_resource_identity(&mut driver);
    let mut height = driver.execution_world.state().time + 1;
    commit(
        &mut driver,
        height,
        game_task_tests::create(&identity, &owner, "native-grant-task"),
    );
    let fence = control(&driver.execution_world);
    let original = signed(
        &identity,
        &owner,
        &fence,
        "small-budget",
        grant_operation("small", 6),
    );
    height += 1;
    commit(&mut driver, height, original.clone());
    assert_eq!(game_task_tests::recorded(&driver, &original).rejected, None);
    assert!(
        driver
            .execution_world
            .builtin_factory_quote("agent-a", "owner-grant.site", KIND, FACTORY)
            .is_err()
    );
    let overlap = signed(
        &identity,
        &owner,
        &fence,
        "scope-takeover",
        grant_operation("new-id", 14),
    );
    height += 1;
    commit(&mut driver, height, overlap.clone());
    assert!(
        game_task_tests::recorded(&driver, &overlap)
            .rejected
            .unwrap()
            .contains("grant_scope_already_reserved")
    );
    height += 1;
    for field in 0..9 {
        let mut stale = fence.clone();
        match field {
            0 => stale.owner_generation.0 += 1,
            1 => stale.owner_binding = "other-owner".into(),
            2 => stale.branch_id = "other-branch".into(),
            3 => stale.reorg_epoch.0 += 1,
            4 => stale.task_revision.0 += 1,
            5 => stale.executor_epoch.0 += 1,
            6 => stale.task_id = "other-task".into(),
            7 => {
                stale.owner_public_key = sign_read_request("fixture", (), &hex::encode([99; 32]))
                    .unwrap()
                    .subject_public_key
            }
            _ => stale.genesis_digest = "other-genesis".into(),
        }
        let signed = sign_read_request(
            GAME_NATIVE_DELEGATION_DOMAIN,
            GameNativeDelegationControlV1 {
                api_rev: 1,
                client_operation_id: format!("stale-{field}"),
                fence: stale,
                operation: GameNativeDelegationOperationV1::Revoke {
                    grant_id: "small".into(),
                    expected_revision: DecimalU64(1),
                },
            },
            &owner,
        )
        .unwrap();
        let before = driver
            .execution_world
            .agent_delegation_authorizations("agent-a")
            .unwrap();
        assert!(
            oasis7_runtime::service_admission::native_delegation::validate_owner(
                &driver.execution_world,
                &signed
            )
            .is_err()
        );
        if field < 7 {
            let request = request(
                &identity,
                WorldServicePayloadV1::GameNativeDelegation(signed),
            );
            commit(&mut driver, height, request.clone());
            height += 1;
            assert!(
                game_task_tests::recorded(&driver, &request)
                    .rejected
                    .is_some()
            );
        }
        assert_eq!(
            driver
                .execution_world
                .agent_delegation_authorizations("agent-a")
                .unwrap(),
            before
        );
    }
    let WorldServicePayloadV1::GameNativeDelegation(valid) = &original.signed_payload else {
        unreachable!()
    };
    let forged = sign_read_request(
        GAME_NATIVE_DELEGATION_DOMAIN,
        valid.request.clone(),
        &hex::encode([99; 32]),
    )
    .unwrap();
    assert!(
        oasis7_runtime::service_admission::native_delegation::validate_owner(
            &driver.execution_world,
            &forged
        )
        .is_err()
    );
    let mut tampered = valid.clone();
    if let GameNativeDelegationOperationV1::Install { limit_units, .. } =
        &mut tampered.request.operation
    {
        *limit_units = DecimalU64(100);
    }
    assert!(
        oasis7_runtime::service_admission::native_delegation::validate_owner(
            &driver.execution_world,
            &tampered
        )
        .is_err()
    );
    let mut expired = valid.request.clone();
    expired.client_operation_id = "expired".into();
    if let GameNativeDelegationOperationV1::Install {
        grant_id,
        valid_until_tick,
        ..
    } = &mut expired.operation
    {
        *grant_id = "expired".into();
        *valid_until_tick = DecimalU64(driver.execution_world.state().time - 1);
    }
    let expired = sign_read_request(GAME_NATIVE_DELEGATION_DOMAIN, expired, &owner).unwrap();
    assert!(
        oasis7_runtime::service_admission::native_delegation::validate_owner(
            &driver.execution_world,
            &expired
        )
        .unwrap_err()
        .contains("already expired")
    );
    let revoked = signed(
        &identity,
        &owner,
        &fence,
        "revoke-small",
        GameNativeDelegationOperationV1::Revoke {
            grant_id: "small".into(),
            expected_revision: DecimalU64(1),
        },
    );
    commit(&mut driver, height, revoked.clone());
    height += 1;
    assert_eq!(game_task_tests::recorded(&driver, &revoked).rejected, None);
    let replacement = signed(
        &identity,
        &owner,
        &fence,
        "replacement",
        grant_operation("replacement", 14),
    );
    commit(&mut driver, height, replacement.clone());
    assert_eq!(
        game_task_tests::recorded(&driver, &replacement).rejected,
        None
    );
    assert_eq!(
        driver
            .execution_world
            .builtin_factory_quote("agent-a", "owner-grant.site", KIND, FACTORY)
            .unwrap()
            .authorization
            .grant
            .grant_id,
        "replacement"
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn native_owner_grant_same_block_task_change_invalidates_new_write_but_keeps_original_read() {
    let (dir, mut driver, identity, owner, public) =
        canonical_agent_chat::driver_fixture_without_provider();
    pin_resource_identity(&mut driver);
    commit(
        &mut driver,
        3,
        game_task_tests::create(&identity, &owner, "native-grant-task"),
    );
    let fence = control(&driver.execution_world);
    let original = signed(
        &identity,
        &owner,
        &fence,
        "grant-install",
        grant_operation("grant", 14),
    );
    commit(&mut driver, 4, original.clone());
    let stop = request(
        &identity,
        WorldServicePayloadV1::GameExecutorControl(
            sign_read_request(
                GAME_EXECUTOR_CONTROL_DOMAIN,
                GameExecutorControlV1 {
                    api_rev: 1,
                    client_operation_id: "stop-task".into(),
                    fence: fence.clone(),
                    operation: GameExecutorOperationV1::Stop,
                },
                &owner,
            )
            .unwrap(),
        ),
    );
    let stale = signed(
        &identity,
        &owner,
        &fence,
        "stale-revoke",
        GameNativeDelegationOperationV1::Revoke {
            grant_id: "grant".into(),
            expected_revision: DecimalU64(1),
        },
    );
    let actions = [stop, stale.clone()]
        .into_iter()
        .enumerate()
        .map(|(index, request)| {
            NodeConsensusAction::from_payload(
                10 + index as u64,
                "node-a",
                correlation::encode_consensus_intent(&request).unwrap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".into(),
            node_id: "node-a".into(),
            proposer_id: "node-a".into(),
            height: 5,
            slot: 5,
            epoch: 0,
            node_block_hash: "native-grant-h5".into(),
            action_root: compute_consensus_action_root(&actions).unwrap(),
            committed_actions: actions,
            committed_at_unix_ms: 5000,
        })
        .unwrap();
    assert!(
        game_task_tests::recorded(&driver, &stale)
            .rejected
            .unwrap()
            .contains("task control fence changed")
    );
    let signer = crate::feedback_submit_api::FeedbackSubmitSigner {
        private_key_hex: owner,
        public_key_hex: public,
    };
    let node = Arc::new(Mutex::new(oasis7_node::NodeRuntime::new(
        oasis7_node::NodeConfig::new("node-a", "w1", NodeRole::Sequencer).unwrap(),
    )));
    let receipt = settled_http(&dir, &node, &signer, &original);
    assert_eq!(
        receipt,
        game_task_tests::recorded(&driver, &original).receipt
    );
    let fresh_stale = signed(
        &identity,
        &signer.private_key_hex,
        &fence,
        "new-stale-revoke",
        GameNativeDelegationOperationV1::Revoke {
            grant_id: "grant".into(),
            expected_revision: DecimalU64(1),
        },
    );
    assert_ne!(
        http(&dir, &node, &signer, None, SUBMIT_PATH, &fresh_stale, "").0,
        200
    );
    fs::remove_dir_all(dir).unwrap();
}
