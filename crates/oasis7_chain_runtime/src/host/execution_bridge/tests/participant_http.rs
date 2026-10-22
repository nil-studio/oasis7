//! Real loopback HTTP handlers, NodeRuntime worker and execution bridge; private fixture only.
use super::*;
use oasis7::world_service::client::{
    RemoteWorldServiceClient, WorldServiceClientConfig, WorldServicePort,
};
use oasis7_agent_api::game::*;
use oasis7_runtime::service_admission::participant::*;
use std::{
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
#[path = "participant_governance.rs"]
mod governance;
#[path = "participant_starter.rs"]
mod starter;
#[path = "participant_owner_view.rs"]
mod owner_view;
#[path = "participant_transport.rs"]
mod transport;
fn namespace() -> GameIdentityNamespaceV1 {
    GameIdentityNamespaceV1 {
        service_id: "private-fixture-identity".into(),
        environment_id: "integration".into(),
        instance_epoch: DecimalU64(1),
    }
}
fn pin_resource_identity(driver: &mut NodeRuntimeExecutionDriver) {
    let manifest = driver.execution_world.chain_resource_manifest();
    driver.resource_identity = Some(super::super::super::driver::ExecutionResourceIdentity {
        chain_id: manifest.chain_id.clone(),
        genesis_ref: manifest
            .genesis_ref
            .clone()
            .expect("fixture has trusted genesis"),
    });
}
fn empty_commit(driver: &mut NodeRuntimeExecutionDriver) {
    pin_resource_identity(driver);
    let height = driver.state.last_applied_committed_height + 1;
    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".into(),
            node_id: "node-a".into(),
            proposer_id: "node-a".into(),
            height,
            slot: height,
            epoch: 0,
            node_block_hash: format!("participant-policy-h{height}"),
            action_root: compute_consensus_action_root(&[]).unwrap(),
            committed_actions: Vec::new(),
            committed_at_unix_ms: height as i64 * 1000,
        })
        .unwrap();
}
fn runtime(root: &Path, mut driver: NodeRuntimeExecutionDriver) -> oasis7_node::NodeRuntime {
    pin_resource_identity(&mut driver);
    let bootstrap = super::super::super::local_bootstrap::derive_service_execution_bootstrap(
        &root.join("world"),
        &root.join("records"),
        &root.join("store"),
        "w1",
    )
    .unwrap();
    let config = oasis7_node::NodeConfig::new("node-a", "w1", NodeRole::Sequencer)
        .unwrap()
        .with_tick_interval(Duration::from_millis(100))
        .unwrap();
    let mut node = oasis7_node::NodeRuntime::new(config)
        .with_execution_hook(driver)
        .with_local_execution_bootstrap(bootstrap);
    node.start().unwrap();
    node
}
fn pin(
    root: &Path,
    identity: &WorldIdentity,
) -> super::super::super::world_service_read::PinnedWorld {
    super::super::super::world_service_read::pin(
        &root.join("records"),
        &root.join("store"),
        identity,
        None,
    )
    .unwrap()
}
fn payload(
    root: &Path,
    identity: &WorldIdentity,
    account: &str,
    op: &str,
    key: &str,
) -> WorldServicePayloadV1 {
    let current = pin(root, identity);
    let binding = current.world.current_cognition_runtime_binding().unwrap();
    let tick = current.world.state().time;
    let policy = current
        .world
        .capability_revocation_state()
        .participants
        .policy
        .as_ref()
        .unwrap();
    let original_namespace = if matches!(op, "register-a" | "register-b") {
        namespace()
    } else {
        policy.identity_namespace.clone()
    };
    let r = GameParticipantRegisterV1 {
        api_rev: 1,
        client_operation_id: op.into(),
        identity_namespace: original_namespace,
        account_id: account.into(),
        world_id: identity.world_id.clone(),
        genesis_digest: identity.genesis_digest.clone(),
        owner_public_key: sign_read_request("fixture", (), key)
            .unwrap()
            .subject_public_key,
        owner_key_version: DecimalU64(1),
    };
    let admission = GameParticipantAdmissionV1 {
        api_rev: 1,
        business_digest: business_digest(&r).unwrap(),
        identity_namespace: policy.identity_namespace.clone(),
        account_id: account.into(),
        world_id: r.world_id.clone(),
        genesis_digest: r.genesis_digest.clone(),
        owner_public_key: r.owner_public_key.clone(),
        branch_id: binding.branch_id,
        reorg_epoch: DecimalU64(binding.reorg_epoch),
        policy_generation: DecimalU64(policy.generation),
        valid_from_tick: DecimalU64(tick),
        valid_until_tick: DecimalU64(tick + 128),
    };
    WorldServicePayloadV1::GameParticipantRegister {
        owner: sign_read_request(GAME_PARTICIPANT_REGISTER_DOMAIN, r, key).unwrap(),
        admission: sign_read_request(
            GAME_PARTICIPANT_ADMISSION_DOMAIN,
            admission,
            &hex::encode([71; 32]),
        )
        .unwrap(),
    }
}
fn client(
    endpoint: &str,
    identity: &WorldIdentity,
    signer: &crate::feedback_submit_api::FeedbackSubmitSigner,
    key: &str,
) -> RemoteWorldServiceClient {
    RemoteWorldServiceClient::new(WorldServiceClientConfig {
        endpoint: endpoint.into(),
        trusted_service_public_key: signer.public_key_hex.clone(),
        expected_world: identity.clone(),
        scope_id: "public".into(),
        read_private_key_hex: key.into(),
        timeout: Duration::from_secs(10),
        formal_read_trust: None,
        max_response_bytes: 2 * 1024 * 1024,
    })
    .unwrap()
}
fn settled(
    client: &RemoteWorldServiceClient,
    root: &Path,
    identity: &WorldIdentity,
    account: &str,
    op: &str,
    key: &str,
) -> GameParticipantReceiptV1 {
    let deadline = Instant::now() + Duration::from_secs(40);
    while Instant::now() < deadline {
        let original = payload(root, identity, account, op, key);
        let correlation = derive_correlation(identity.clone(), &original).unwrap();
        if let Ok(response) = client.lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: correlation.key,
            },
            original,
        ) && let IntentOutcome::Committed { receipt, .. } = response.outcome {
                return serde_json::from_value(receipt).unwrap();
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("participant canonical HTTP result did not settle");
}
struct Stop<'a> {
    stop: &'a AtomicBool,
    node: Arc<Mutex<oasis7_node::NodeRuntime>>,
}
impl Drop for Stop<'_> {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self
            .node
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stop();
    }
}

#[test]
fn participant_real_http_execution_two_accounts_lost_response_closed_policy_restart() {
    let (dir, mut driver, identity, _, _) = canonical_agent_chat::driver_fixture_without_provider();
    let original_agent = driver.execution_world.state().agents["agent-a"].clone();
    starter::fund_fixture_reserve(&mut driver.execution_world);
    governance::install(&mut driver.execution_world, &identity, 1, true);
    empty_commit(&mut driver);
    let root = dir.canonicalize().unwrap();
    let committed_baseline = pin(&root, &identity);
    let checkpoint = committed_baseline.world.snapshot();
    let original_created_at = committed_baseline
        .world
        .chain_resource_manifest()
        .created_at_height;
    let node = Arc::new(Mutex::new(runtime(&root, driver)));
    let service_key = hex::encode([9; 32]);
    let signer = crate::feedback_submit_api::FeedbackSubmitSigner {
        public_key_hex: sign_read_request("fixture", (), &service_key)
            .unwrap()
            .subject_public_key,
        private_key_hex: service_key,
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let stop = AtomicBool::new(false);
    let dropped = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let _stop = Stop {
            stop: &stop,
            node: node.clone(),
        };
        let server =
            scope.spawn(|| transport::serve(listener, &node, &root, &signer, &stop, &dropped));
        let key_a = hex::encode([72; 32]);
        let key_b = hex::encode([73; 32]);
        let a = client(&endpoint, &identity, &signer, &key_a);
        let b = client(&endpoint, &identity, &signer, &key_b);
        let first = request(
            &identity,
            payload(&root, &identity, "account-a", "register-a", &key_a),
        );
        assert!(
            matches!(a.submit(first.clone()).unwrap(),SubmitObservation::OutcomeUnknown(ref correlation) if correlation==&first.correlation),
            "first admitted response is deliberately lost"
        );
        let receipt_a = settled(&a, &root, &identity, "account-a", "register-a", &key_a);
        owner_view::exercise(&endpoint, &root, &identity, &signer, &receipt_a, &key_a);
        assert!(dropped.load(Ordering::SeqCst));
        let again = a
            .submit(request(
                &identity,
                payload(&root, &identity, "account-a", "register-a", &key_a),
            ))
            .unwrap();
        assert!(
            matches!(again,SubmitObservation::Response(response) if matches!(response.outcome,IntentOutcome::Committed {..}))
        );
        b.submit(request(
            &identity,
            payload(&root, &identity, "account-b", "register-b", &key_b),
        ))
        .unwrap();
        let receipt_b = settled(&b, &root, &identity, "account-b", "register-b", &key_b);
        assert_ne!(receipt_a.agent_id, receipt_b.agent_id);
        let starter_original =
            starter::exercise(&a, &b, &root, &identity, &receipt_a, &key_a, &key_b);
        let unknown_original = payload(&root, &identity, "account-a", "never-submitted", &key_a);
        let unknown_key = derive_correlation(identity.clone(), &unknown_original)
            .unwrap()
            .key;
        let unknown = a
            .lookup(
                LookupIntentRequest {
                    contract_version: 1,
                    key: unknown_key,
                },
                unknown_original,
            )
            .unwrap();
        assert!(
            matches!(unknown.outcome, IntentOutcome::Unknown),
            "an existing account binding is not an original operation result"
        );
        assert!(b
            .submit(request(
                &identity,
                payload(&root, &identity, "account-a", "wrong-owner", &key_b)
            ))
            .is_err());
        let before_close = pin(&root, &identity);
        assert_eq!(before_close.world.state().agents.len(), 3);
        for (receipt, key) in [(&receipt_a, &key_a), (&receipt_b, &key_b)] {
            assert!(before_close
                .world
                .state()
                .agents
                .contains_key(&receipt.agent_id));
            assert_eq!(
                agent_authority::owner_public_key(&before_close.world, &receipt.agent_id).unwrap(),
                sign_read_request("fixture", (), key)
                    .unwrap()
                    .subject_public_key
            );
            assert_eq!(
                before_close
                    .world
                    .state()
                    .starter_oc_claims
                    .contains_key(&receipt.agent_id),
                receipt.agent_id == receipt_a.agent_id,
                "only the explicitly claimed account has a starter receipt"
            );
        }
        assert_eq!(
            before_close.world.state().agents["agent-a"].state.agent_id,
            original_agent.state.agent_id
        );
        assert_eq!(
            before_close
                .world
                .chain_resource_manifest()
                .created_at_height,
            original_created_at
        );
        node.lock().unwrap().stop().unwrap();
        let mut driver = NodeRuntimeExecutionDriver::new(
            root.join("state.json"),
            root.join("world"),
            root.join("records"),
            root.join("store"),
        )
        .unwrap();
        governance::install(&mut driver.execution_world, &identity, 2, false);
        empty_commit(&mut driver);
        let journal = driver.execution_world.journal().clone();
        let replayed = RuntimeWorld::from_snapshot(checkpoint, journal).unwrap();
        assert_eq!(
            replayed.capability_revocation_state().participants,
            driver
                .execution_world
                .capability_revocation_state()
                .participants
        );
        assert_eq!(
            replayed.state().starter_oc_claims,
            driver.execution_world.state().starter_oc_claims
        );
        assert_eq!(
            replayed.main_token_supply(),
            driver.execution_world.main_token_supply()
        );
        *node.lock().unwrap() = runtime(&root, driver);
        assert_eq!(
            starter::recovered(&a, &starter_original)["claim"]["agent_id"],
            receipt_a.agent_id
        );
        assert_eq!(
            settled(&a, &root, &identity, "account-a", "register-a", &key_a),
            receipt_a
        );
        let closed_unknown = payload(&root, &identity, "account-c", "register-c", &key_a);
        let closed_key = derive_correlation(identity.clone(), &closed_unknown)
            .unwrap()
            .key;
        assert!(matches!(
            a.lookup(
                LookupIntentRequest {
                    contract_version: 1,
                    key: closed_key
                },
                closed_unknown
            )
            .unwrap()
            .outcome,
            IntentOutcome::Unknown
        ));
        assert!(a
            .submit(request(
                &identity,
                payload(&root, &identity, "account-c", "register-c", &key_a)
            ))
            .is_err());
        a.submit(request(
            &identity,
            payload(
                &root,
                &identity,
                "account-a",
                "recover-new-operation",
                &key_a,
            ),
        ))
        .unwrap();
        assert_eq!(
            settled(
                &a,
                &root,
                &identity,
                "account-a",
                "recover-new-operation",
                &key_a
            ),
            receipt_a
        );
        assert_eq!(pin(&root, &identity).world.state().agents.len(), 3);
        stop.store(true, Ordering::SeqCst);
        server.join().unwrap();
    });
    fs::remove_dir_all(root).unwrap();
}
