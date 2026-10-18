use super::*;
use std::path::PathBuf;
pub(super) fn driver_fixture() -> (
    PathBuf,
    NodeRuntimeExecutionDriver,
    WorldIdentity,
    String,
    String,
) {
    fixture(false)
}
pub(super) fn driver_fixture_with_verified_authority() -> (
    PathBuf,
    NodeRuntimeExecutionDriver,
    WorldIdentity,
    String,
    String,
) {
    fixture(true)
}
fn fixture(
    verified_authority: bool,
) -> (
    PathBuf,
    NodeRuntimeExecutionDriver,
    WorldIdentity,
    String,
    String,
) {
    let dir = temp_dir("world-service-owner-chat");
    let owner = hex::encode([7u8; 32]);
    let public = sign_read_request("owner", (), &owner)
        .unwrap()
        .subject_public_key;
    let identity = WorldIdentity {
        world_id: "w1".into(),
        genesis_digest: "fixture-genesis-v1".into(),
    };
    let mut world = oasis7::runtime_host::world::new_with_state(WorldState::default());
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
fn owner_chat(
    owner: &str,
    public: &str,
    nonce: u64,
    replaces: Option<String>,
) -> WorldServicePayloadV1 {
    let mut chat = oasis7::viewer::AgentChatRequest {
        agent_id: "agent-a".into(),
        message: format!("private canonical goal {nonce}"),
        player_id: Some("owner-a".into()),
        public_key: Some(public.into()),
        auth: None,
        intent_tick: Some(2),
        intent_seq: Some(nonce),
        world_id: Some("w1".into()),
        reorg_epoch: Some(0),
        authority_scope: Some("player_agent_chat".into()),
        replaces_intent_id: replaces,
        canonical_authority: Some(oasis7_proto::viewer::CanonicalAgentChatAuthorityV1 {
            branch_id: "main".into(),
            agent_identity_generation: 1,
        }),
    };
    chat.auth =
        Some(oasis7::viewer::sign_agent_chat_auth_proof(&chat, nonce, public, owner).unwrap());
    WorldServicePayloadV1::AgentChat(chat)
}
#[test]
fn canonical_owner_chat_driver_dedup_restart_rejection_and_replacement() {
    let (dir, mut driver, identity, owner, public) = driver_fixture();
    let original = request(&identity, owner_chat(&owner, &public, 1, None));
    commit(&mut driver, 3, original.clone());
    let key = correlation::key_digest(&original.correlation.key).unwrap();
    let canonical: CanonicalIntentResultV1 = serde_json::from_value(
        driver
            .execution_world
            .capability_revocation_state()
            .world_service_results[&key]
            .clone(),
    )
    .unwrap();
    assert_eq!(canonical.rejected, None);
    assert_eq!(canonical.committed_height, 3);
    let intent_id = canonical.receipt["intent_id"].as_str().unwrap().to_string();
    let intent_before = driver.execution_world.state().agent_intent_ledger.clone();
    commit(&mut driver, 4, original.clone());
    assert_eq!(
        driver.execution_world.state().agent_intent_ledger,
        intent_before
    );
    let stale = request(&identity, owner_chat(&owner, &public, 2, None));
    commit(&mut driver, 5, stale.clone());
    let stale_key = correlation::key_digest(&stale.correlation.key).unwrap();
    let rejected: CanonicalIntentResultV1 = serde_json::from_value(
        driver
            .execution_world
            .capability_revocation_state()
            .world_service_results[&stale_key]
            .clone(),
    )
    .unwrap();
    assert!(rejected.rejected.unwrap().contains("replacement"));
    assert_eq!(
        driver.execution_world.state().agent_intent_ledger,
        intent_before
    );
    let pinned = super::super::super::world_service_read::pin(
        &dir.join("records"),
        &dir.join("store"),
        &identity,
        None,
    )
    .unwrap();
    let authorized = projection::WorldServiceProjection::from_authenticated_world(
        &pinned.world,
        Some("agent-a"),
        &public,
    )
    .unwrap();
    assert_eq!(
        authorized
            .canonical_agent_chat
            .unwrap()
            .goal
            .unwrap()
            .message,
        "private canonical goal 1"
    );
    let mut restarted = NodeRuntimeExecutionDriver::new(
        dir.join("state.json"),
        dir.join("world"),
        dir.join("records"),
        dir.join("store"),
    )
    .unwrap();
    assert_eq!(
        restarted.execution_world.state().agent_intent_ledger,
        intent_before
    );
    // Rejected nonce 2 is consumed in the canonical result; correction uses 3.
    let correction = request(
        &identity,
        owner_chat(&owner, &public, 3, Some(intent_id.clone())),
    );
    commit(&mut restarted, 6, correction);
    assert_eq!(
        restarted.execution_world.state().agent_intent_ledger[&intent_id].status,
        "superseded"
    );
    assert_eq!(
        agent_chat::owner_view(&restarted.execution_world, "agent-a")
            .unwrap()
            .unwrap()
            .goal
            .unwrap()
            .message,
        "private canonical goal 3"
    );
    // Exact original replay after replacement resolves original canonical result,
    // never restores an earlier goal or republishes the intent.
    commit(&mut restarted, 7, original);
    assert_eq!(
        agent_chat::owner_view(&restarted.execution_world, "agent-a")
            .unwrap()
            .unwrap()
            .goal
            .unwrap()
            .message,
        "private canonical goal 3"
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn canonical_chat_same_block_nonce_fence_and_duplicate_preserve_first_goal() {
    let (dir, mut driver, identity, owner, public) = driver_fixture();
    let first = request(&identity, owner_chat(&owner, &public, 100, None));
    let WorldServicePayloadV1::AgentChat(chat) = &first.signed_payload else {
        unreachable!()
    };
    let predicted = driver
        .execution_world
        .clone()
        .apply_authenticated_agent_chat(chat)
        .unwrap();
    let mut low = owner_chat(
        &owner,
        &public,
        1,
        Some(predicted["intent_id"].as_str().unwrap().into()),
    );
    let WorldServicePayloadV1::AgentChat(chat) = &mut low else {
        unreachable!()
    };
    chat.intent_seq = Some(101);
    chat.auth = Some(oasis7::viewer::sign_agent_chat_auth_proof(chat, 1, &public, &owner).unwrap());
    let low = request(&identity, low);
    let actions: Vec<_> = [first.clone(), low.clone(), first.clone()]
        .iter()
        .enumerate()
        .map(|(i, request)| {
            NodeConsensusAction::from_payload(
                i as u64 + 1,
                "node-a",
                correlation::encode_consensus_intent(request).unwrap(),
            )
            .unwrap()
        })
        .collect();
    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".into(),
            node_id: "node-a".into(),
            proposer_id: "node-a".into(),
            height: 3,
            slot: 3,
            epoch: 0,
            node_block_hash: "node-h3".into(),
            action_root: compute_consensus_action_root(&actions).unwrap(),
            committed_actions: actions,
            committed_at_unix_ms: 3000,
        })
        .unwrap();
    let key = correlation::key_digest(&low.correlation.key).unwrap();
    let rejected: CanonicalIntentResultV1 = serde_json::from_value(
        driver
            .execution_world
            .capability_revocation_state()
            .world_service_results[&key]
            .clone(),
    )
    .unwrap();
    assert!(
        rejected
            .rejected
            .unwrap()
            .contains("nonce did not advance within block")
    );
    assert_eq!(driver.execution_world.state().agent_intent_ledger.len(), 1);
    assert_eq!(
        driver
            .execution_world
            .capability_revocation_state()
            .world_service_results
            .len(),
        2
    );
    assert_eq!(
        agent_chat::owner_view(&driver.execution_world, "agent-a")
            .unwrap()
            .unwrap()
            .goal
            .unwrap()
            .message,
        "private canonical goal 100"
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn canonical_owner_goal_http_view_excludes_public_and_registered_delegate() {
    use std::io::Read;
    let (dir, mut driver, identity, owner, public) = driver_fixture();
    commit(
        &mut driver,
        3,
        request(&identity, owner_chat(&owner, &public, 1, None)),
    );
    let delegate_private = hex::encode([8u8; 32]);
    let delegate_public = sign_read_request("fixture", (), &delegate_private)
        .unwrap()
        .subject_public_key;
    let delegation = AgentSignerDelegationChangeV1 {
        world_id: "w1".into(),
        branch_id: "main".into(),
        agent_id: "agent-a".into(),
        owner_binding: "owner-a".into(),
        agent_identity_generation: 1,
        generation: 1,
        delegate_public_key: delegate_public,
        revoked: false,
        nonce: 1,
    };
    commit(
        &mut driver,
        4,
        request(
            &identity,
            WorldServicePayloadV1::Delegation(
                sign_read_request("delegation", delegation, &owner).unwrap(),
            ),
        ),
    );
    oasis7::viewer::ViewerRuntimeLiveServer::canonical_agent_service_context(
        &driver.execution_world,
        "agent-a",
    )
    .unwrap();
    let signer = crate::feedback_submit_api::FeedbackSubmitSigner {
        private_key_hex: owner.clone(),
        public_key_hex: public,
    };
    let runtime = std::sync::Arc::new(std::sync::Mutex::new(oasis7_node::NodeRuntime::new(
        oasis7_node::NodeConfig::new("node-a", "w1", NodeRole::Sequencer).unwrap(),
    )));
    for (key, scope, is_owner) in [
        (&owner, "agent:agent-a", true),
        (&delegate_private, "agent:agent-a", false),
        (&delegate_private, "public", false),
    ] {
        let mut original_projection = None;
        for path in [VIEW_PATH, oasis7_client_api::world_service::VIEW_JSON_PATH] {
            let signed = sign_read_request(
                path,
                ReadWorldViewRequest {
                    observation_nonce: None,
                    formal_anchor: None,
                    contract_version: 1,
                    world: identity.clone(),
                    scope_id: scope.into(),
                    min_commit: None,
                    fixed_commit: None,
                    deadline_unix_ms: None,
                },
                key,
            )
            .unwrap();
            let body = serde_json::to_vec(&signed).unwrap();
            let mut bytes = format!(
                "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
                body.len()
            )
            .into_bytes();
            bytes.extend(body);
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let mut client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            let (mut server, _) = listener.accept().unwrap();
            crate::world_service_api::maybe_handle(
                &mut server,
                &bytes,
                &runtime,
                "POST",
                path,
                "w1",
                &dir.join("world"),
                &dir.join("records"),
                &dir.join("store"),
                &signer,
            )
            .unwrap();
            drop(server);
            let mut response = String::new();
            client.read_to_string(&mut response).unwrap();
            assert!(response.starts_with("HTTP/1.1 200"), "{response}");
            let value: serde_json::Value =
                serde_json::from_str(response.split("\r\n\r\n").nth(1).unwrap()).unwrap();
            assert_eq!(
                value.to_string().contains("private canonical goal 1"),
                is_owner
            );
            let projection = &value["payload"]["view"];
            assert_eq!(projection.get("canonical_agent_chat").is_some(), is_owner);
            if path == VIEW_PATH {
                let typed: SignedServiceResponse<
                    ReadWorldViewResponse<
                        oasis7::world_service::projection::WorldServiceProjection,
                    >,
                > = serde_json::from_value(value.clone()).unwrap();
                verify_service_response(
                    path,
                    &request_digest(path, &signed).unwrap(),
                    &typed,
                    &signer.public_key_hex,
                )
                .unwrap();
                original_projection = Some(value["payload"].clone());
            } else {
                assert_eq!(original_projection.as_ref(), Some(&value["payload"]));
                let mut normalized: SignedServiceResponse<
                    ReadWorldViewResponse<serde_json::Value>,
                > = serde_json::from_value(value).unwrap();
                verify_service_response(
                    path,
                    &request_digest(path, &signed).unwrap(),
                    &normalized,
                    &signer.public_key_hex,
                )
                .unwrap();
                assert!(
                    verify_service_response(
                        VIEW_PATH,
                        &request_digest(path, &signed).unwrap(),
                        &normalized,
                        &signer.public_key_hex
                    )
                    .is_err()
                );
                normalized.payload.view["state"]["time"] = serde_json::json!(999);
                assert!(
                    verify_service_response(
                        path,
                        &request_digest(path, &signed).unwrap(),
                        &normalized,
                        &signer.public_key_hex
                    )
                    .is_err()
                );
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
