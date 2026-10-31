//! Real participant World read, honestly Blocked without a preinstalled resource grant.
use super::*;
use oasis7::world_service::{
    private_goal_prepare::sign_goal_prepare, private_goal_read::publication,
    private_goal_store::PrivateGoalStore,
};
use oasis7_client_api::world_service::resource_basis as wire;
fn apply(
    client: &RemoteWorldServiceClient,
    identity: &WorldIdentity,
    original: WorldServicePayloadV1,
) {
    let submit = request(identity, original.clone());
    let correlation = submit.correlation.clone();
    client.submit(submit).unwrap();
    let until = Instant::now() + Duration::from_secs(20);
    while Instant::now() < until {
        if let Ok(reply) = client.lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: correlation.key.clone(),
            },
            original.clone(),
        ) && matches!(reply.outcome, IntentOutcome::Committed { .. })
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("resource component canonical operation did not settle");
}
pub(super) fn exercise(
    endpoint: &str,
    root: &Path,
    identity: &WorldIdentity,
    signer: &crate::feedback_submit_api::FeedbackSubmitSigner,
    receipt: &GameParticipantReceiptV1,
    key: &str,
) {
    let client = client(endpoint, identity, signer, key);
    let before = pin(root, identity);
    let branch = before.commit.binding.branch_id.clone();
    let reorg = DecimalU64(before.commit.binding.reorg_generation);
    apply(
        &client,
        identity,
        WorldServicePayloadV1::GameTaskCreate(
            sign_read_request(
                GAME_TASK_CREATE_DOMAIN,
                GameTaskCreateV1 {
                    api_rev: 1,
                    client_operation_id: "resource-native-task".into(),
                    world_id: identity.world_id.clone(),
                    genesis_digest: identity.genesis_digest.clone(),
                    branch_id: branch.clone(),
                    reorg_epoch: reorg,
                    agent_id: receipt.agent_id.clone(),
                    owner_binding: receipt.owner_binding.clone(),
                    owner_generation: receipt.owner_generation,
                },
                key,
            )
            .unwrap(),
        ),
    );
    let now = pin(root, identity);
    let control = game_task::current_control(&now.world, &receipt.agent_id)
        .unwrap()
        .unwrap();
    let store =
        PrivateGoalStore::open(&root.join("resource-component-owner-goals"), [8; 32]).unwrap();
    let content = GoalContentV1 {
        goal_text: "component resource read; no custody approval or dispatch".into(),
        soft_constraints: vec![],
    };
    let proof = sign_goal_prepare(
        GameGoalPrepareV1 {
            api_rev: 1,
            world_id: identity.world_id.clone(),
            genesis_digest: identity.genesis_digest.clone(),
            task_id: control.task_id.clone(),
            agent_id: receipt.agent_id.clone(),
            owner_public_key: receipt.owner_public_key.clone(),
            owner_binding: control.owner_binding.clone(),
            owner_generation: control.owner_generation,
            branch_id: branch.clone(),
            reorg_epoch: reorg,
            expected_task_revision: control.task_revision,
            expected_latest_goal_revision: control.latest_goal_revision,
            goal_revision: DecimalU64(1),
            client_operation_id: "resource-native-goal".into(),
            client_nonce: "ab".repeat(32),
            goal_digest: game_goal_version_digest(&control.task_id, DecimalU64(1), &content)
                .unwrap(),
            content,
        },
        key,
    )
    .unwrap();
    let prepared = store.prepare(&proof).unwrap();
    let publish = publication(&proof, &prepared, "resource-native-publish".into()).unwrap();
    apply(
        &client,
        identity,
        WorldServicePayloadV1::GameGoalPublish(
            sign_read_request(GAME_GOAL_PUBLISH_DOMAIN, publish, key).unwrap(),
        ),
    );
    let delegate = sign_read_request("key", (), &hex::encode([79; 32]))
        .unwrap()
        .subject_public_key;
    let record = AgentSignerDelegationChangeV1 {
        world_id: identity.world_id.clone(),
        branch_id: branch,
        agent_id: receipt.agent_id.clone(),
        owner_binding: receipt.owner_binding.clone(),
        agent_identity_generation: receipt.owner_generation.0,
        generation: 1,
        delegate_public_key: delegate.clone(),
        revoked: false,
        nonce: 1,
    };
    apply(
        &client,
        identity,
        WorldServicePayloadV1::Delegation(
            sign_read_request("delegation", record.clone(), key).unwrap(),
        ),
    );
    let now = pin(root, identity);
    let control = game_task::current_control(&now.world, &receipt.agent_id)
        .unwrap()
        .unwrap();
    apply(
        &client,
        identity,
        WorldServicePayloadV1::GameExecutorControl(
            sign_read_request(
                GAME_EXECUTOR_CONTROL_DOMAIN,
                GameExecutorControlV1 {
                    api_rev: 1,
                    client_operation_id: "resource-native-bind".into(),
                    fence: GameTaskFenceV1::from_control(&control),
                    operation: GameExecutorOperationV1::Bind {
                        executor_id: "resource-native-executor".into(),
                        executor_public_key: delegate,
                        delegation_generation: DecimalU64(1),
                    },
                },
                key,
            )
            .unwrap(),
        ),
    );
    let now = pin(root, identity);
    let control = game_task::current_control(&now.world, &receipt.agent_id)
        .unwrap()
        .unwrap();
    let WorldServicePayloadV1::GameParticipantRegister { owner, admission } =
        payload(root, identity, "account-a", "register-a", key)
    else {
        unreachable!()
    };
    let request = wire::OwnerResourceReadV1 {
        api_version: 1,
        world: identity.into(),
        expected_fence: GameTaskFenceV1::from_control(&control),
        expected_signer_delegation: wire::ResourceSignerRecordV1 {
            world_id: record.world_id,
            branch_id: record.branch_id,
            agent_id: record.agent_id,
            owner_binding: record.owner_binding,
            agent_identity_generation: DecimalU64(record.agent_identity_generation),
            generation: DecimalU64(record.generation),
            delegate_public_key: record.delegate_public_key,
            revoked: record.revoked,
            nonce: DecimalU64(record.nonce),
        },
        session_id: "component-only-no-custody-session".into(),
        approval_operation_id: "component-only-not-approved".into(),
        prepared_resource_basis_digest: "de".repeat(32),
        selection: wire::ResourceBasisSelectionV1::Build {
            site: "not-installed".into(),
            factory: "not-installed".into(),
            factory_kind: "factory.smelter.mk1".into(),
        },
        observation_nonce: "ef".repeat(32),
    };
    let q = wire::ParticipantOwnerResourceViewRequestV1 {
        owner: owner.into(),
        admission: admission.into(),
        read: sign_read_request(wire::OWNER_RESOURCE_VIEW_PATH_V1, request, key)
            .unwrap()
            .into(),
    };
    let response = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
        .post(format!("{endpoint}{}", wire::OWNER_RESOURCE_VIEW_PATH_V1))
        .json(&q)
        .send()
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let bytes = response.bytes().unwrap();
    let reply: wire::SignedResourceResponseV1<wire::ParticipantOwnerResourceObservationV1> =
        serde_json::from_slice(&bytes).unwrap();
    wire::verify_owner_resource_response(&q, &reply, &signer.public_key_hex).unwrap();
    assert!(
        matches!(
            reply.payload.resource,
            wire::ResourceBasisResolutionV1::Blocked { .. }
        ),
        "missing real resource grant must not become an empty Ready fact"
    );
    assert_eq!(reply.payload.owner.control, control);
}
