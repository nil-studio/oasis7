//! SDK -> real HTTP admission queue -> ordinary Node engine -> terminal receipts.
use super::*;
use oasis7::world_service::client::{RemoteWorldServiceClient, WorldServicePort};
use oasis7_node::NodeExecutionCommitResult;

struct SharedDriver(Arc<Mutex<bridge::NodeRuntimeExecutionDriver>>);
impl NodeExecutionHook for SharedDriver {
    fn on_commit(
        &mut self,
        context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        self.0.lock().unwrap().on_commit(context)
    }
}

pub(super) fn run(
    options: &Options,
    receipt: &Receipt,
    driver: bridge::NodeRuntimeExecutionDriver,
    original: &oasis7_agent_api::ContinuousAgentRequestContextV1,
    executor: &str,
    generation: u64,
    deadline: Instant,
) {
    // This is the unknown model-outcome lane, not a fabricated ProviderError.
    // Its private recovery classification is executor_lost_after_dispatch:model_outcome_unknown;
    // the canonical Failure below deliberately keeps the allowed cognition_failed code.
    let before = driver.execution_world.snapshot();
    let before_economy = driver.execution_world.cognition_economy().unwrap();
    let original_lease = driver
        .execution_world
        .cognition_economy()
        .unwrap()
        .leases
        .values()
        .find(|lease| lease.request_digest == original.request_digest.to_string())
        .unwrap()
        .clone();
    let driver = Arc::new(Mutex::new(driver));
    let node = Arc::new(Mutex::new(stopped_node(options, &driver)));
    let (key, _) = super::super::super::super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(&options.key_dir).unwrap();
    let signer =
        super::super::super::super::super::build_feedback_submit_signer(&options.node_id, &key)
            .unwrap();
    let restore =
        bridge::world_service_read::RestoreContext::new(ReleaseSecurityPolicy::default(), None)
            .unwrap()
            .with_local_provider_terminal_recovery(
                oasis7_proto::storage_profile::StorageProfile::DevLocal,
            )
            .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            // One signed View before each original Submit, then its Lookup and final View.
            for _ in 0..7 {
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => std::thread::sleep(Duration::from_millis(2)),
                        Err(error) => panic!("terminal closure accept failed: {:?}", error.kind()),
                    }
                };
                stream.set_read_timeout(Some(deadline.saturating_duration_since(Instant::now()))).unwrap();
                let raw = read_request(&mut stream);
                let first = std::str::from_utf8(&raw).unwrap().lines().next().unwrap();
                let path = first.split_whitespace().nth(1).unwrap();
                assert!(matches!(path, VIEW_PATH | SUBMIT_PATH | LOOKUP_PATH));
                let before_height = driver.lock().unwrap().state.last_applied_committed_height;
                assert!(super::super::super::super::super::world_service_api::maybe_handle_with_context(
                    &mut stream, &raw, &node, "POST", path, &options.world_id,
                    &options.root.join("world"), &options.root.join("records"), &options.root.join("store"), &signer, &restore,
                ).unwrap());
                if let Some(reason) = super::super::super::super::super::world_service_api::take_last_service_error() {
                    let diagnostic = persist_service_diagnostic(&reason);
                    panic!("terminal closure service rejected; private diagnostic: {diagnostic:?}");
                }
                if path == SUBMIT_PATH {
                    // The dispatcher has already queued the actual HTTP payload. Only
                    // the normal Node worker may turn that pending action into a commit.
                    node.lock().unwrap().start().unwrap();
                    while driver.lock().unwrap().state.last_applied_committed_height == before_height {
                        assert!(Instant::now() < deadline, "real engine terminal commit deadline");
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    node.lock().unwrap().stop().unwrap();
                    let batches = node.lock().unwrap().drain_committed_action_batches();
                    assert_eq!(batches.iter().map(|batch| batch.actions.len()).sum::<usize>(), 1, "the real pending queue must produce exactly the original action");
                    *node.lock().unwrap() = stopped_node(options, &driver);
                }
            }
        });
        let client = RemoteWorldServiceClient::new(
            oasis7::world_service::client::WorldServiceClientConfig {
                formal_read_trust: None,
                endpoint,
                trusted_service_public_key: receipt.service_public_key.clone(),
                expected_world: receipt.world.clone(),
                scope_id: format!("agent:{}", options.provider.as_ref().unwrap().agent_id),
                read_private_key_hex: executor.into(),
                timeout: Duration::from_secs(10),
                max_response_bytes: 2 * 1024 * 1024,
            },
        )
        .unwrap();
        let mut last_commit = None;
        for (id, operation) in [
            (
                "expiry-original-failure",
                SchedulerOperationV1::ProviderFailure {
                    request: original.clone(),
                    reason: "cognition_failed".into(),
                },
            ),
            (
                "expiry-original-settle",
                SchedulerOperationV1::SettleLease {
                    lease_id: original_lease.lease_id.clone(),
                    consumed_amount: 1,
                },
            ),
        ] {
            let view = client
                .read_view(view_request(receipt, last_commit.clone()))
                .unwrap();
            assert!(
                view.projection().agent_context.is_none(),
                "expired grant must never regain fresh dispatch authority"
            );
            let binding = view.projection().runtime_binding.as_ref().unwrap();
            let base = oasis7_agent_api::world_cognition::RuntimeCognitionBaseBindingV1 {
                world_id: binding.world_id.clone(),
                branch_id: binding.branch_id.clone(),
                finality_epoch: binding.finality_epoch,
                finality_block_hash: binding
                    .finality_block_hash
                    .as_ref()
                    .map(ToString::to_string),
                finality_status: binding.finality_status.clone(),
                base_tick: binding.base_tick,
                base_world_hash: binding.base_world_hash.to_string(),
                reorg_epoch: binding.reorg_epoch,
                runtime_manifest_hash: binding.runtime_manifest_hash.to_string(),
            };
            let payload = WorldServicePayloadV1::Scheduler(
                sign_read_request(
                    "scheduler",
                    SchedulerIntentV1 {
                        agent_id: options.provider.as_ref().unwrap().agent_id.clone(),
                        request_id: id.into(),
                        delegation_generation: generation,
                        task_prefix_binding_revision: 0,
                        captured_base_binding: base,
                        operation,
                    },
                    executor,
                )
                .unwrap(),
            );
            let request = SubmitIntentRequest {
                contract_version: 1,
                correlation: derive_correlation(receipt.world.clone(), &payload).unwrap(),
                deadline_unix_ms: None,
                signed_payload: payload.clone(),
            };
            let SubmitObservation::Response(received) = client.submit(request.clone()).unwrap()
            else {
                panic!("original terminal admission is unknown")
            };
            assert!(matches!(
                received.outcome,
                IntentOutcome::Received {
                    durability: AdmissionDurability::Volatile
                }
            ));
            assert_eq!(received.correlation, request.correlation);
            let response = client
                .lookup(
                    LookupIntentRequest {
                        contract_version: 1,
                        key: request.correlation.key.clone(),
                    },
                    payload,
                )
                .unwrap();
            assert_eq!(response.correlation, request.correlation);
            let IntentOutcome::Committed { commit, .. } = response.outcome else {
                panic!("original terminal operation did not commit")
            };
            last_commit = Some(*commit);
        }
        let final_view = client
            .read_view(view_request(receipt, last_commit))
            .unwrap();
        let lease = final_view
            .projection()
            .cognition_leases
            .iter()
            .find(|lease| lease.lease_id == original_lease.lease_id)
            .unwrap();
        assert_eq!(
            lease.status,
            oasis7_runtime::CognitionLeaseStatusV1::Settled
        );
        assert_eq!(lease.settled_amount, 1);
        assert_eq!(lease.reserved_amount, original_lease.reserved_amount);
        assert_eq!(lease.request_digest, original_lease.request_digest);
        assert_eq!(lease.agent_id, original_lease.agent_id);
        assert_eq!(lease.agent_session_id, original_lease.agent_session_id);
        assert_eq!(lease.agent_turn_id, original_lease.agent_turn_id);
        assert_eq!(
            lease.decision_request_id,
            original_lease.decision_request_id
        );
        assert_eq!(lease.account_id, original_lease.account_id);
        assert_eq!(lease.idempotency_key, original_lease.idempotency_key);
        assert_eq!(lease.quote, original_lease.quote);
        assert_eq!(lease.reserved_at_tick, original_lease.reserved_at_tick);
        assert_eq!(
            (
                lease.released_amount,
                lease.refunded_amount,
                lease.compensated_amount
            ),
            (0, 0, 0)
        );
        worker.join().unwrap();
    });
    let locked = driver.lock().unwrap();
    let after = locked.execution_world.snapshot();
    assert_eq!(before.capability_grants_v2, after.capability_grants_v2);
    let after_economy = locked.execution_world.cognition_economy().unwrap();
    assert_eq!(before_economy.provisions, after_economy.provisions);
    assert_eq!(
        before_economy.provision_receipts,
        after_economy.provision_receipts
    );
    assert_eq!(
        before_economy.provision_journal,
        after_economy.provision_journal
    );
    assert_eq!(before_economy.leases.len(), after_economy.leases.len());
    let new_receipts = after_economy
        .receipts
        .iter()
        .filter(|(key, _)| !before_economy.receipts.contains_key(*key))
        .map(|(_, receipt)| receipt)
        .collect::<Vec<_>>();
    assert_eq!(new_receipts.len(), 1);
    assert_eq!(new_receipts[0].operation, "settle");
    assert_eq!(new_receipts[0].lease_id, original_lease.lease_id);
    assert_eq!(new_receipts[0].consumed_amount, 1);
    assert_eq!(new_receipts[0].net_amount, 1);
    assert_eq!(new_receipts[0].refunded_amount, 0);
    assert_eq!(new_receipts[0].released_amount, 0);
    let results = &locked
        .execution_world
        .capability_revocation_state()
        .world_service_results;
    assert_eq!(
        results.len(),
        before
            .capability_revocation_state
            .world_service_results
            .len()
            + 2
    );
    for (key, original_result) in &before.capability_revocation_state.world_service_results {
        assert_eq!(
            results.get(key),
            Some(original_result),
            "terminal recovery changed an original canonical result"
        );
    }
    let new_results = results
        .iter()
        .filter(|(key, _)| {
            !before
                .capability_revocation_state
                .world_service_results
                .contains_key(*key)
        })
        .map(|(_, value)| serde_json::from_value::<CanonicalIntentResultV1>(value.clone()).unwrap())
        .collect::<Vec<_>>();
    assert!(new_results.iter().all(|result| result.rejected.is_none()));
    assert_eq!(new_results.iter().filter(|result| matches!(&result.request.signed_payload, WorldServicePayloadV1::Scheduler(signed) if matches!(&signed.request.operation, SchedulerOperationV1::ProviderFailure { request, reason } if request == original && reason == "cognition_failed"))).count(), 1);
    assert_eq!(new_results.iter().filter(|result| matches!(&result.request.signed_payload, WorldServicePayloadV1::Scheduler(signed) if matches!(&signed.request.operation, SchedulerOperationV1::SettleLease { lease_id, consumed_amount } if lease_id == &original_lease.lease_id && *consumed_amount == 1))).count(), 1);
}

fn stopped_node(
    options: &Options,
    driver: &Arc<Mutex<bridge::NodeRuntimeExecutionDriver>>,
) -> NodeRuntime {
    let baseline = bridge::derive_service_execution_bootstrap(
        &options.root.join("world"),
        &options.root.join("records"),
        &options.root.join("store"),
        &options.world_id,
    )
    .unwrap();
    NodeRuntime::new(
        NodeConfig::new("expiry-node", &options.world_id, NodeRole::Sequencer)
            .unwrap()
            .with_tick_interval(Duration::from_millis(40))
            .unwrap(),
    )
    .with_local_execution_bootstrap(baseline)
    .with_execution_hook(SharedDriver(driver.clone()))
}
fn view_request(receipt: &Receipt, minimum: Option<CommitRef>) -> ReadWorldViewRequest {
    ReadWorldViewRequest {
        observation_nonce: None,
        formal_anchor: None,
        contract_version: 1,
        world: receipt.world.clone(),
        scope_id: "agent:starter-agent-0".into(),
        min_commit: minimum,
        fixed_commit: None,
        deadline_unix_ms: None,
    }
}
fn read_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
    let mut raw = Vec::new();
    loop {
        let mut bytes = [0; 4096];
        let count = stream.read(&mut bytes).unwrap();
        assert!(count > 0);
        raw.extend_from_slice(&bytes[..count]);
        assert!(raw.len() <= 2 * 1024 * 1024);
        if let Some(end) = raw.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
            let length = std::str::from_utf8(&raw[..end])
                .unwrap()
                .lines()
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                        .map(|(_, value)| value.trim().parse::<usize>().unwrap())
                })
                .unwrap();
            if raw.len() >= end + 4 + length {
                return raw;
            }
        }
    }
}
