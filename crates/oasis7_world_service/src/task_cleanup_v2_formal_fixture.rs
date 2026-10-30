//! Actual Runtime mutation with the existing formal-read test authority identity.
//! Local cryptographic authorities are not external finality/deployment evidence.
use super::*;
use oasis7_agent_api::game::*;
use oasis7_client_api::world_service::{WorldIdentity, sign_read_request};
use oasis7_runtime::{
    Action, SchedulerPolicyV1, World, WorldState, service_admission as admission,
};
struct Empty;
impl oasis7_formal_protocol::controlled_authority::formal::business_index::BusinessIndexNodeReader
    for Empty
{
    fn read_node(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
}
pub(super) fn begin(world: &mut World, at: i64) -> oasis7_runtime::RuntimeCommittedTickContext {
    let snapshot = world.snapshot();
    let bytes = oasis7_world_state::codec::snapshot_cbor(&snapshot).unwrap();
    let root = world.state().retained_history_root.clone().unwrap();
    let parent = oasis7_runtime::VerifiedFormalRetainedHistoryParentV1::for_test_snapshot_at_tick(
        root,
        &bytes,
        world.state().time,
    );
    world
        .attach_verified_formal_history(parent, std::sync::Arc::new(Empty))
        .unwrap();
    let context = oasis7_runtime::RuntimeCommittedTickContext {
        height: world.state().time.max(4) + 1,
        slot: 0,
        epoch: 0,
        node_block_hash: "aa".repeat(32),
        action_root: "bb".repeat(32),
        authority_node_id: world.tick_consensus_authority_source().into(),
        committed_at_unix_ms: at,
    };
    world.begin_formal_execution_tick(&context).unwrap();
    context
}
fn envelope(
    payload: admission::WorldServicePayloadV1,
) -> admission::SubmitIntentRequest<admission::WorldServicePayloadV1> {
    admission::SubmitIntentRequest {
        contract_version: 1,
        correlation: admission::derive_correlation(
            WorldIdentity {
                world_id: "formal-fixture-world".into(),
                genesis_digest: crate::formal_read_fixtures::h(8),
            },
            &payload,
        )
        .unwrap(),
        signed_payload: payload,
        deadline_unix_ms: None,
    }
}
fn record(
    world: &mut World,
    request: admission::SubmitIntentRequest<admission::WorldServicePayloadV1>,
    receipt: serde_json::Value,
    height: u64,
) {
    world
        .record_world_service_result(admission::CanonicalIntentResultV1 {
            request,
            receipt,
            action_id: height,
            committed_height: height,
            rejected: None,
        })
        .unwrap();
}
pub(super) fn fixture() -> (
    World,
    oasis7_client_api::world_service::SignedReadRequest<GameTaskRevokeAndCloseV2>,
    String,
) {
    let private = "11".repeat(32);
    let public = sign_read_request("key", (), &private)
        .unwrap()
        .subject_public_key;
    let mut world = World::new_with_state(WorldState::default())
        .try_with_cognition_scheduler(
            SchedulerPolicyV1 {
                schema_version: SchedulerPolicyV1::SCHEMA_VERSION.into(),
                max_total_wakes_per_tick: 1,
                max_wakes_per_agent_per_tick: 1,
                aging_after_ticks: 1,
                max_starvation_ticks: 1,
                initial_priority: 0,
                comparator: SchedulerPolicyV1::COMPARATOR.into(),
                service_order: SchedulerPolicyV1::SERVICE_ORDER.into(),
            },
            1,
        )
        .unwrap();
    let snapshot = world.snapshot_with_chain_resource_context(
        oasis7_runtime::ChainResourceDerivationContext {
            world_id: "formal-fixture-world",
            chain_id: "formal-fixture-chain",
            genesis_ref: Some(&crate::formal_read_fixtures::h(8)),
            created_at_height: 0,
            manifest_height: 0,
            commit_block_hash: None,
            tick: 0,
        },
        crate::formal_read_fixtures::h(10),
        crate::formal_read_fixtures::h(11),
    );
    world = World::from_snapshot(snapshot, world.journal().clone()).unwrap();
    world.initialize_empty_formal_history().unwrap();
    world.submit_action(Action::RegisterAgent {
        agent_id: "agent".into(),
        pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
    });
    world.step().unwrap();
    world.submit_action(Action::ClaimStarterOc {
        agent_id: "agent".into(),
        player_id: "owner".into(),
        public_key: Some(public),
    });
    world.step().unwrap();
    world
        .install_capability_agent_identity("agent", "owner", 1)
        .unwrap();
    world
        .bind_cognition_runtime("formal-fixture-world", "main", 0, None, "pending", 0)
        .unwrap();
    let create = envelope(admission::WorldServicePayloadV1::GameTaskCreate(
        sign_read_request(
            GAME_TASK_CREATE_DOMAIN,
            GameTaskCreateV1 {
                api_rev: 1,
                client_operation_id: "create".into(),
                world_id: "formal-fixture-world".into(),
                genesis_digest: crate::formal_read_fixtures::h(8),
                branch_id: "main".into(),
                reorg_epoch: DecimalU64(0),
                agent_id: "agent".into(),
                owner_binding: "owner".into(),
                owner_generation: DecimalU64(1),
            },
            &private,
        )
        .unwrap(),
    ));
    let receipt = admission::game_task::prepare_create(&world, &create, []).unwrap();
    record(&mut world, create, receipt, 1);
    let initial = admission::game_task::current_control(&world, "agent")
        .unwrap()
        .unwrap();
    let goal = GameGoalPublishV1 {
        api_rev: 1,
        world_id: initial.world_id.clone(),
        genesis_digest: initial.genesis_digest.clone(),
        task_id: initial.task_id.clone(),
        agent_id: initial.agent_id.clone(),
        owner_public_key: initial.owner_public_key.clone(),
        owner_binding: initial.owner_binding.clone(),
        owner_generation: initial.owner_generation,
        branch_id: initial.branch_id.clone(),
        reorg_epoch: initial.reorg_epoch,
        expected_task_revision: initial.task_revision,
        expected_latest_goal_revision: initial.latest_goal_revision,
        goal_revision: DecimalU64(1),
        client_operation_id: "goal".into(),
        prepare_operation_id: "prepare".into(),
        goal_ref: "33".repeat(32),
    };
    let publish = envelope(admission::WorldServicePayloadV1::GameGoalPublish(
        sign_read_request(GAME_GOAL_PUBLISH_DOMAIN, goal, &private).unwrap(),
    ));
    let receipt = admission::game_task::prepare_publish(&world, &publish, [], 2).unwrap();
    record(&mut world, publish, receipt, 2);
    let delegate = sign_read_request("key", (), &"22".repeat(32))
        .unwrap()
        .subject_public_key;
    world
        .apply_agent_signer_delegation(
            &sign_read_request(
                "delegation",
                admission::AgentSignerDelegationChangeV1 {
                    world_id: "formal-fixture-world".into(),
                    branch_id: "main".into(),
                    agent_id: "agent".into(),
                    owner_binding: "owner".into(),
                    agent_identity_generation: 1,
                    generation: 1,
                    delegate_public_key: delegate.clone(),
                    revoked: false,
                    nonce: 1,
                },
                &private,
            )
            .unwrap(),
        )
        .unwrap();
    let before_bind = admission::game_task::current_control(&world, "agent")
        .unwrap()
        .unwrap();
    let bind = envelope(admission::WorldServicePayloadV1::GameExecutorControl(
        sign_read_request(
            GAME_EXECUTOR_CONTROL_DOMAIN,
            GameExecutorControlV1 {
                api_rev: 1,
                client_operation_id: "bind".into(),
                fence: GameTaskFenceV1::from_control(&before_bind),
                operation: GameExecutorOperationV1::Bind {
                    executor_id: "executor".into(),
                    executor_public_key: delegate.clone(),
                    delegation_generation: DecimalU64(1),
                },
            },
            &private,
        )
        .unwrap(),
    ));
    let receipt = admission::game_task::prepare_executor_control(&world, &bind, []).unwrap();
    record(&mut world, bind, receipt, 3);
    let fence = GameTaskFenceV1::from_control(
        &admission::game_task::current_control(&world, "agent")
            .unwrap()
            .unwrap(),
    );
    let original = sign_read_request(
        GAME_TASK_CLEANUP_DOMAIN_V2,
        GameTaskRevokeAndCloseV2 {
            api_revision: 2,
            client_operation_id: "close".into(),
            custody_account_id: "account".into(),
            termination_plan_digest: "44".repeat(32),
            permitted_branches: GameTaskCloseConditionsV2 {
                exact_initial_active: fence,
                own_stop_intent: None,
            },
            signer_public_key: delegate,
            signer_generation: DecimalU64(1),
            signer_nonce: DecimalU64(1),
            revoke_generation: DecimalU64(2),
            revoke_nonce: DecimalU64(2),
            cleanup_until_ms: DecimalU64(5000),
        },
        &private,
    )
    .unwrap();
    (world, original, private)
}
