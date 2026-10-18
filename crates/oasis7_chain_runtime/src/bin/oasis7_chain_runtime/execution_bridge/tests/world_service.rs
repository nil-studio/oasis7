use super::*;
use oasis7::world_service::*;
use oasis7_node::{NodeConsensusAction, NodeExecutionHook};
use oasis7_runtime::{Action, CognitionLeaseQuoteV1, CognitionLeaseRequestV1, WorldState};

fn commit(
    driver: &mut NodeRuntimeExecutionDriver,
    height: u64,
    request: SubmitIntentRequest<WorldServicePayloadV1>,
) {
    let payload = correlation::encode_consensus_intent(&request).unwrap();
    let actions = vec![NodeConsensusAction::from_payload(height, "node-a", payload).unwrap()];
    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".into(),
            node_id: "node-a".into(),
            proposer_id: "node-a".into(),
            height,
            slot: height,
            epoch: 0,
            node_block_hash: format!("node-h{height}"),
            action_root: compute_consensus_action_root(&actions).unwrap(),
            committed_actions: actions,
            committed_at_unix_ms: height as i64 * 1000,
        })
        .unwrap();
}

fn request(
    identity: &WorldIdentity,
    payload: WorldServicePayloadV1,
) -> SubmitIntentRequest<WorldServicePayloadV1> {
    SubmitIntentRequest {
        contract_version: 1,
        correlation: derive_correlation(identity.clone(), &payload).unwrap(),
        deadline_unix_ms: None,
        signed_payload: payload,
    }
}

pub(crate) fn live_fixture_world(world_id: &str, public: &str) -> RuntimeWorld {
    let mut world = oasis7::runtime_host::world::new_with_state(WorldState::default());
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent-a".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    world.submit_action(Action::ClaimStarterOc {
        agent_id: "agent-a".into(),
        player_id: "owner-a".into(),
        public_key: Some(public.into()),
    });
    world.step().unwrap();
    assert!(world.state().starter_oc_claims.contains_key("agent-a"));
    world
        .install_capability_agent_identity("agent-a", "owner-a", 1)
        .unwrap();
    world
        .bind_cognition_runtime(world_id, "main", 0, None, "pending", 0)
        .unwrap();
    world
        .provision_cognition_for_agent("agent-a", "provision-a", "owner-a", 100)
        .unwrap();
    world
        .install_test_provider_capability_fixture_without_cognition_balance("agent-a")
        .expect("install actual Runtime provider capability context");
    world
}

#[test]
fn world_service_driver_admin_commit_pin_restart_and_stale_fence() {
    let dir = temp_dir("world-service-admin");
    let owner = hex::encode([7u8; 32]);
    let public = sign_read_request("owner", (), &owner)
        .unwrap()
        .subject_public_key;
    let delegate = sign_read_request("delegate", (), &hex::encode([8u8; 32]))
        .unwrap()
        .subject_public_key;
    let identity = WorldIdentity {
        world_id: "w1".into(),
        genesis_digest: "fixture-genesis-v1".into(),
    };
    let world = live_fixture_world("w1", &public);
    let provider_invocation = world
        .capability_invocation_contexts()
        .values()
        .find(|context| {
            matches!(&context.subject,
            oasis7_wasm_abi::CapabilitySubject::Agent { agent_id, .. } if agent_id == "agent-a")
                && context.presenter.presenter_kind == "provider"
        })
        .expect("actual Runtime provider invocation");
    let quote_authority = oasis7::simulator::h_v1(
        oasis7::simulator::COGNITION_CAPABILITY_INVOCATION_CONTEXT_DOMAIN,
        provider_invocation,
    )
    .to_string();
    let base = world.current_cognition_base_binding().unwrap();
    let context_hash = super::super::execution_hash::execution_resource_context_hash("w1");
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
    let baseline = super::super::local_bootstrap::derive_local_execution_bootstrap(
        &dir.join("world"),
        &dir.join("records"),
        "w1",
        world.state().time,
        "world-service-local-setup-boundary",
        &oasis7_runtime::ReleaseSecurityPolicy::default(),
    )
    .unwrap();
    let mut driver = NodeRuntimeExecutionDriver::new_with_local_bootstrap(
        dir.join("state.json"),
        dir.join("world"),
        dir.join("records"),
        dir.join("store"),
        &oasis7_proto::storage_profile::StorageProfileConfig::default(),
        baseline,
    )
    .unwrap();
    let change = AgentSignerDelegationChangeV1 {
        world_id: "w1".into(),
        branch_id: "main".into(),
        agent_id: "agent-a".into(),
        owner_binding: "owner-a".into(),
        agent_identity_generation: 1,
        generation: 1,
        delegate_public_key: delegate,
        revoked: false,
        nonce: 1,
    };
    let registration = request(
        &identity,
        WorldServicePayloadV1::Delegation(sign_read_request("delegation", change, &owner).unwrap()),
    );
    // Node admission preserves the opaque legacy outer frame as well as the
    // registered inner frame. The canonical driver authenticates the request;
    // direct local World execution and unsigned controller actions stay fenced.
    use oasis7::consensus_action_payload::*;
    let node = oasis7_node::NodeRuntime::new(
        oasis7_node::NodeConfig::new("node-a", "w1", NodeRole::Sequencer)
            .expect("valid node admission fixture"),
    );
    let old_frame = encode_consensus_action_payload(&ConsensusActionPayloadEnvelope {
        gameplay_submission_origin: None,
        version: 1,
        auth: None,
        body: ConsensusActionPayloadBody::WorldServiceIntent {
            request: registration.clone(),
        },
    })
    .unwrap();
    assert_eq!(
        decode_consensus_action_payload(&old_frame).unwrap(),
        ConsensusActionPayloadBody::WorldServiceIntent {
            request: registration.clone(),
        },
    );
    node.submit_consensus_action_payload(100, old_frame)
        .unwrap();
    let registered_frame = correlation::encode_consensus_intent(&registration).unwrap();
    let ConsensusActionPayloadBody::RuntimeAction {
        action: Action::WorldServiceIntent { request: decoded },
    } = decode_consensus_action_payload(&registered_frame).unwrap()
    else {
        panic!("registered frame must retain the Runtime world-service intent");
    };
    assert_eq!(decoded, serde_json::to_value(&registration).unwrap());
    node.submit_consensus_action_payload(101, registered_frame)
        .unwrap();
    let mut local_world = driver.execution_world.clone();
    let local_tick = local_world.state().time;
    local_world.submit_action(Action::WorldServiceIntent {
        request: serde_json::to_value(&registration).unwrap(),
    });
    assert!(
        format!("{:?}", local_world.step().unwrap_err()).contains("canonical driver admission")
    );
    assert_eq!(local_world.state().time, local_tick);
    let controller_action: Action =
        serde_json::from_value(serde_json::json!({"type":"TransferMainToken","data":{
            "from_account_id":"a","to_account_id":"b","amount":1,"nonce":1
        }}))
        .unwrap();
    let unsigned_controller = encode_consensus_action_payload(&ConsensusActionPayloadEnvelope {
        gameplay_submission_origin: None,
        version: 1,
        auth: None,
        body: ConsensusActionPayloadBody::RuntimeAction {
            action: controller_action,
        },
    })
    .unwrap();
    assert!(
        format!(
            "{:?}",
            node.submit_consensus_action_payload(102, unsigned_controller)
                .unwrap_err()
        )
        .contains("missing_main_token_auth")
    );
    commit(&mut driver, 3, registration.clone());
    assert_eq!(
        driver
            .execution_world
            .current_cognition_base_binding()
            .unwrap(),
        base
    );
    let authorization_root = driver
        .execution_world
        .capability_authorization_root()
        .to_owned();
    let reserve = SchedulerIntentV1 {
        agent_id: "agent-a".into(),
        request_id: "reserve-1".into(),
        delegation_generation: 0,
        captured_base_binding: base.clone(),
        operation: SchedulerOperationV1::ReserveLease(CognitionLeaseRequestV1::new(
            "lease-1",
            "owner-a",
            "agent-a",
            "session-a",
            "turn-a",
            "decision-a",
            "digest-a",
            CognitionLeaseQuoteV1::new("quote-a", "cognition_units", 1).with_authority(
                "owner-a",
                oasis7_runtime::COGNITION_RESOURCE_VERSION_V1,
                "provider_cognition",
                "agent_turn",
                oasis7_runtime::COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION,
                quote_authority,
                driver
                    .execution_world
                    .current_cognition_runtime_binding()
                    .unwrap()
                    .base_world_hash
                    .to_string(),
            ),
        )),
    };
    let reservation = request(
        &identity,
        WorldServicePayloadV1::Scheduler(sign_read_request("scheduler", reserve, &owner).unwrap()),
    );
    commit(&mut driver, 4, reservation.clone());
    assert_eq!(
        driver
            .execution_world
            .current_cognition_base_binding()
            .unwrap(),
        base
    );
    assert_eq!(
        driver.execution_world.capability_authorization_root(),
        authorization_root
    );
    let result_key = correlation::key_digest(&reservation.correlation.key).unwrap();
    let result: CanonicalIntentResultV1 = serde_json::from_value(
        driver
            .execution_world
            .capability_revocation_state()
            .world_service_results[&result_key]
            .clone(),
    )
    .unwrap();
    assert_eq!(result.rejected, None);
    assert_eq!(result.receipt["reserved_amount"], 1);
    commit(&mut driver, 5, reservation.clone());
    assert_eq!(
        driver
            .execution_world
            .capability_revocation_state()
            .world_service_results
            .len(),
        2
    );
    let pinned = super::super::world_service_read::pin(
        &dir.join("records"),
        &dir.join("store"),
        &identity,
        None,
    )
    .unwrap();
    assert_eq!(pinned.commit.position, 5);
    assert_eq!(pinned.world.state().time, base.base_tick);
    let restarted = NodeRuntimeExecutionDriver::new(
        dir.join("state.json"),
        dir.join("world"),
        dir.join("records"),
        dir.join("store"),
    )
    .unwrap();
    assert_eq!(
        restarted
            .execution_world
            .capability_revocation_state()
            .world_service_results[&result_key],
        driver
            .execution_world
            .capability_revocation_state()
            .world_service_results[&result_key]
    );
    // An ordinary tick must invalidate the captured base, despite admin-only
    // commits preserving it. A fresh request key cannot hide that mismatch.
    driver.execution_world.step().unwrap();
    let mut stale = match reservation.signed_payload {
        WorldServicePayloadV1::Scheduler(signed) => signed.request,
        _ => unreachable!(),
    };
    stale.request_id = "stale-reserve".into();
    let stale = request(
        &identity,
        WorldServicePayloadV1::Scheduler(sign_read_request("scheduler", stale, &owner).unwrap()),
    );
    commit(&mut driver, 6, stale.clone());
    let key = correlation::key_digest(&stale.correlation.key).unwrap();
    let result: CanonicalIntentResultV1 = serde_json::from_value(
        driver
            .execution_world
            .capability_revocation_state()
            .world_service_results[&key]
            .clone(),
    )
    .unwrap();
    assert!(result.rejected.unwrap().contains("base binding changed"));
    let _ = fs::remove_dir_all(dir);
}

#[path = "world_service_agent_chat.rs"]
mod canonical_agent_chat;
#[test]
fn service_recipe_preserves_authenticated_origin_consensus_context_and_replay() {
    let private = hex::encode([41u8; 32]);
    let public = sign_read_request("owner", (), &private)
        .unwrap()
        .subject_public_key;
    let mut gameplay = oasis7::viewer::GameplayActionRequest {
        action_id: oasis7::viewer::ACTION_SCHEDULE_SMELTER_IRON_INGOT.into(),
        target_agent_id: "builder-a".into(),
        actor_agent_id: None,
        player_id: "browser-player".into(),
        public_key: Some(public.clone()),
        auth: None,
    };
    gameplay.auth = Some(
        oasis7::viewer::sign_gameplay_action_auth_proof(&gameplay, 7, &public, &private).unwrap(),
    );
    let identity = WorldIdentity {
        world_id: "w1".into(),
        genesis_digest: "fixture-genesis-v1".into(),
    };
    let original = request(
        &identity,
        WorldServicePayloadV1::GameplayJson(serde_json::to_vec(&gameplay).unwrap()),
    );
    let payload = correlation::encode_consensus_intent(&original).unwrap();
    let committed = NodeConsensusAction::from_payload(17, "node-transport", payload).unwrap();
    let context = NodeExecutionCommitContext {
        world_id: "w1".into(),
        node_id: "node-a".into(),
        proposer_id: "node-a".into(),
        height: 3,
        slot: 3,
        epoch: 0,
        node_block_hash: "node-h3".into(),
        action_root: compute_consensus_action_root(std::slice::from_ref(&committed)).unwrap(),
        committed_actions: vec![committed.clone()],
        committed_at_unix_ms: 3000,
    };
    let mut world = oasis7::runtime_host::world::new();
    for index in 0..4 {
        world.submit_action(Action::RegisterAgent {
            agent_id: format!("queued-{index}"),
            pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
        });
    }
    let staged = super::super::world_service_execution::apply_intents(
        &mut world,
        &context,
        Some(&identity),
        vec![(17, original.clone()), (17, original.clone())],
    )
    .unwrap();
    let snapshot = world.snapshot();
    assert_eq!(snapshot.pending_actions.len(), 5);
    assert_eq!(snapshot.pending_actions[4].id, 5);
    let origin = snapshot.pending_actions[4]
        .committed_recipe_origin
        .clone()
        .expect("actual service recipe origin");
    assert_eq!(origin.consensus_action_id, 17);
    assert_eq!(origin.consensus_submitter_player_id, "node-transport");
    assert_eq!(origin.action_payload_hash, committed.payload_hash);
    assert_eq!(origin.committed_height, 3);
    assert_eq!(origin.action_root, context.action_root);
    assert_eq!(origin.submission.verified_player_id, gameplay.player_id);
    assert_eq!(origin.submission.public_key, public);
    assert_eq!(origin.submission.auth_nonce, 7);
    assert_eq!(origin.submission.hosted_registration_nonce, None);
    let (action, submission) =
        gameplay::authenticated_action_with_origin(&world, &serde_json::to_vec(&gameplay).unwrap())
            .unwrap();
    assert_eq!(submission, Some(origin.submission.clone()));
    assert_eq!(snapshot.pending_actions[4].action, action);
    let dir = temp_dir("service-recipe-origin");
    world.save_to_dir(&dir).unwrap();
    let restored = oasis7::runtime_host::world::load_from_dir(&dir).unwrap();
    assert_eq!(
        restored.snapshot().pending_actions[4].committed_recipe_origin,
        Some(origin)
    );
    world.step().unwrap();
    super::super::world_service_execution::finalize_intents(&mut world, staged).unwrap();
    let key = correlation::key_digest(&original.correlation.key).unwrap();
    let result: CanonicalIntentResultV1 = serde_json::from_value(
        world.capability_revocation_state().world_service_results[&key].clone(),
    )
    .unwrap();
    assert_eq!(result.request, original);
    assert_eq!(result.receipt["runtime_action_id"], 5);
    assert_eq!(
        result.receipt["consensus_action_payload_hash"],
        committed.payload_hash
    );
    let before = serde_json::to_value(world.snapshot()).unwrap();
    let replay = super::super::world_service_execution::apply_intents(
        &mut world,
        &context,
        Some(&identity),
        vec![(17, original)],
    )
    .unwrap();
    super::super::world_service_execution::finalize_intents(&mut world, replay).unwrap();
    assert_eq!(serde_json::to_value(world.snapshot()).unwrap(), before);
    std::fs::remove_dir_all(dir).unwrap();
}

#[path = "world_service_game_task.rs"]
mod game_task_tests;

#[cfg(unix)]
#[path = "world_service_private_goal.rs"]
mod private_goal_tests;

#[cfg(unix)]
#[path = "world_service_private_task_context.rs"]
mod private_task_context_tests;

#[cfg(unix)]
#[path = "world_service_private_task_context_read_race.rs"]
mod private_task_context_read_race_tests;

#[cfg(unix)]
#[path = "world_service_task_playtest.rs"]
mod task_playtest;

#[path = "world_service_cognition_rejection.rs"]
mod cognition_rejection_tests;
