//! Existing full actual-artifact scenario; real commits, service HTTP and SDK.
use super::*;
use oasis7::world_service::*;
use oasis7_node::{
    NodeConsensusAction, NodeExecutionCommitContext, NodeExecutionHook,
    compute_consensus_action_root,
};
use std::{
    io::Read,
    net::TcpListener,
    sync::{Arc, Mutex},
};

fn commit(
    driver: &mut bridge::NodeRuntimeExecutionDriver,
    world: &WorldIdentity,
    submit: Option<SubmitIntentRequest<WorldServicePayloadV1>>,
) {
    let height = driver.state.last_applied_committed_height + 1;
    let actions = submit
        .into_iter()
        .map(|request| {
            let hash = blake3::hash(
                correlation::key_digest(&request.correlation.key)
                    .unwrap()
                    .as_bytes(),
            );
            let id = u64::from_be_bytes(hash.as_bytes()[..8].try_into().unwrap()).max(1);
            NodeConsensusAction::from_payload(
                id,
                "expiry-node",
                correlation::encode_consensus_intent(&request).unwrap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: world.world_id.clone(),
            node_id: "expiry-node".into(),
            proposer_id: "expiry-node".into(),
            height,
            slot: height,
            epoch: 0,
            node_block_hash: format!("expiry-node-{height}"),
            action_root: compute_consensus_action_root(&actions).unwrap(),
            committed_actions: actions,
            committed_at_unix_ms: height as i64 * 1000,
        })
        .unwrap();
}
fn submit(
    driver: &bridge::NodeRuntimeExecutionDriver,
    world: &WorldIdentity,
    key: &str,
    generation: u64,
    id: &str,
    operation: SchedulerOperationV1,
) -> SubmitIntentRequest<WorldServicePayloadV1> {
    let payload = WorldServicePayloadV1::Scheduler(
        sign_read_request(
            "scheduler",
            SchedulerIntentV1 {
                agent_id: "starter-agent-0".into(),
                request_id: id.into(),
                delegation_generation: generation,
                task_prefix_binding_revision: 0,
                captured_base_binding: driver
                    .execution_world
                    .current_cognition_base_binding()
                    .unwrap(),
                operation,
            },
            key,
        )
        .unwrap(),
    );
    SubmitIntentRequest {
        contract_version: 1,
        correlation: derive_correlation(world.clone(), &payload).unwrap(),
        deadline_unix_ms: None,
        signed_payload: payload,
    }
}
pub(super) fn run(options: &Options, receipt: &Receipt) {
    let input = options.provider.as_ref().unwrap();
    let executor = hex::encode(fs::read(options.root.join("private/executor.key")).unwrap());
    let (key, _) = super::super::super::super::node_keypair_config::read_existing_node_keypair_in_secure_config_dir(&options.key_dir).unwrap();
    let signer =
        super::super::super::super::build_feedback_submit_signer(&options.node_id, &key).unwrap();
    let restore =
        bridge::world_service_read::RestoreContext::new(ReleaseSecurityPolicy::default(), None)
            .unwrap()
            .with_local_provider_terminal_recovery(
                oasis7_proto::storage_profile::StorageProfile::DevLocal,
            )
            .unwrap();
    let baseline = bridge::derive_service_execution_bootstrap(
        &options.root.join("world"),
        &options.root.join("records"),
        &options.root.join("store"),
        &options.world_id,
    )
    .unwrap();
    let mut driver = bridge::NodeRuntimeExecutionDriver::new_with_local_bootstrap(
        options.root.join("state.json"),
        options.root.join("world"),
        options.root.join("records"),
        options.root.join("store"),
        &StorageProfileConfig::default(),
        baseline,
    )
    .unwrap();
    driver.hot_window_heights = 1024; // This fixture explicitly retains the original Prefix proof.
    let captured = captured_request(&driver.execution_world, &input.agent_id);
    let captured_tick = captured.runtime_binding.base_tick;
    commit(&mut driver, &receipt.world, None); // actual admission must not use the captured base tick.
    let generation = driver
        .execution_world
        .capability_revocation_state()
        .agent_signer_delegations[&input.agent_id]
        .generation;
    let quote =
        oasis7_runtime::CognitionLeaseQuoteV1::new("expiry-real-quote", "cognition_units", 1)
            .with_authority(
                &input.owner_binding,
                "cognition_units.v1",
                "provider_cognition",
                "agent_turn",
                "fixed_unit_experimental",
                "local-test-provider-authority",
                driver
                    .execution_world
                    .current_cognition_runtime_binding()
                    .unwrap()
                    .base_world_hash
                    .to_string(),
            );
    let reserve = submit(
        &driver,
        &receipt.world,
        &executor,
        generation,
        "expiry-real-reserve",
        SchedulerOperationV1::ReserveLease(oasis7_runtime::CognitionLeaseRequestV1::new(
            captured.provider_invocation_key().to_string(),
            &input.owner_binding,
            &input.agent_id,
            &captured.agent_session_id,
            &captured.agent_turn_id,
            &captured.decision_request_id,
            captured.request_digest.to_string(),
            quote,
        )),
    );
    commit(&mut driver, &receipt.world, Some(reserve));
    let mut logical = captured.clone();
    logical.transport_attempt = 1;
    let prefix = submit(
        &driver,
        &receipt.world,
        &executor,
        generation,
        "expiry-real-prefix",
        SchedulerOperationV1::ProviderPrefix {
            request: captured.clone(),
            context_digest: oasis7_agent_api::h_v1("oasis7.cognition.context.v1", &logical)
                .to_string(),
        },
    );
    let admitted_tick = driver.execution_world.state().time;
    assert!(admitted_tick > captured_tick);
    commit(&mut driver, &receipt.world, Some(prefix.clone()));
    let result_key = correlation::key_digest(&prefix.correlation.key).unwrap();
    let result: CanonicalIntentResultV1 = serde_json::from_value(
        driver
            .execution_world
            .capability_revocation_state()
            .world_service_results[&result_key]
            .clone(),
    )
    .unwrap();
    assert!(result.rejected.is_none());
    let authority: oasis7_runtime::ProviderBackedBootstrapAuthorityV1 = serde_json::from_slice(
        &fs::read(options.root.join("private/provider-authority.json")).unwrap(),
    )
    .unwrap();
    while driver.execution_world.state().time <= authority.grant.expires_at_tick.unwrap() {
        commit(&mut driver, &receipt.world, None);
    }
    let pin = bridge::world_service_read::pin_with_context(
        &options.root.join("records"),
        &options.root.join("store"),
        &receipt.world,
        None,
        &restore,
    )
    .unwrap();
    assert!(
        pin.world
            .optional_canonical_agent_service_context(&input.agent_id)
            .unwrap()
            .is_none()
    );
    let runtime = Arc::new(Mutex::new(NodeRuntime::new(
        NodeConfig::new(&options.node_id, &options.world_id, NodeRole::Sequencer).unwrap(),
    )));
    let http_deadline = Instant::now() + Duration::from_secs(10);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    std::thread::scope(|scope| {
        let runtime = &runtime;
        let restore = &restore;
        let signer = &signer;
        let worker = scope.spawn(move || {
            let deadline = http_deadline;
            let mut stream = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("terminal fixture accept: {e}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut raw = Vec::new();
            let mut buf = [0; 4096];
            loop {
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0);
                raw.extend_from_slice(&buf[..n]);
                assert!(raw.len() <= 2 * 1024 * 1024);
                if let Some(end) = raw.windows(4).position(|b| b == b"\r\n\r\n") {
                    let headers = std::str::from_utf8(&raw[..end]).unwrap();
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.split_once(':')
                                .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                                .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    if raw.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            assert!(
                super::super::super::super::world_service_api::maybe_handle_with_context(
                    &mut stream,
                    &raw,
                    runtime,
                    "POST",
                    TERMINAL_LOOKUP_PATH,
                    &options.world_id,
                    &options.root.join("world"),
                    &options.root.join("records"),
                    &options.root.join("store"),
                    signer,
                    restore
                )
                .unwrap()
            );
            super::super::super::super::world_service_api::take_last_service_error()
                .map(|reason| persist_service_diagnostic(&reason))
        });
        let client = oasis7::world_service::client::RemoteWorldServiceClient::new(
            oasis7::world_service::client::WorldServiceClientConfig {
                formal_read_trust: None,
                endpoint,
                trusted_service_public_key: receipt.service_public_key.clone(),
                expected_world: receipt.world.clone(),
                scope_id: format!("agent:{}", input.agent_id),
                read_private_key_hex: executor.clone(),
                timeout: Duration::from_secs(10),
                max_response_bytes: 2 * 1024 * 1024,
            },
        )
        .unwrap();
        let response = client.lookup_terminal_original(
            LookupIntentRequest {
                contract_version: 1,
                key: prefix.correlation.key.clone(),
            },
            prefix.signed_payload.clone(),
            &pin.commit,
        );
        let diagnostic = worker.join().unwrap();
        assert!(
            response.is_ok(),
            "terminal Lookup failed; private diagnostic: {:?}",
            diagnostic
        );
        let response = response.unwrap();
        let proof = response.terminal_recovery_authority.as_ref().unwrap();
        assert_eq!(proof.schema_version, 1);
        assert!(proof.prefix_commit.is_some());
        assert_eq!(proof.admitted_at_tick, admitted_tick);
        assert_ne!(proof.admitted_at_tick, captured_tick);
        assert_eq!(proof.prefix_committed_height, result.committed_height);
        assert_eq!(proof.grant_id, authority.grant.grant_id);
        assert_eq!(
            proof.lease.idempotency_key,
            captured.provider_invocation_key().to_string()
        );
    });
    // Exercise the real ordinary retention path, not fabricated old proof bytes.
    driver.hot_window_heights = StorageProfileConfig::default().execution_hot_head_heights;
    driver.retention_reconcile_pending = true;
    driver.retention_reconcile_next_height = None;
    commit(&mut driver, &receipt.world, None);
    assert!(
        bridge::world_service_read::pin_retained_at_height_with_context(
            &options.root.join("records"),
            &options.root.join("store"),
            &receipt.world,
            result.committed_height,
            &restore,
        )
        .unwrap()
        .is_none()
    );
    terminal_closure::run(
        options,
        receipt,
        driver,
        &captured,
        &prefix,
        &executor,
        generation,
        http_deadline,
    );
}

fn captured_request(
    world: &oasis7_runtime::World,
    agent: &str,
) -> oasis7_agent_api::ContinuousAgentRequestContextV1 {
    let context = world.canonical_agent_service_context(agent).unwrap();
    let digest = format!("blake3:{}", "11".repeat(32));
    let mut request: oasis7_agent_api::ContinuousAgentRequestContextV1 = serde_json::from_value(serde_json::json!({
        "base_decision_request":{
            "observation":{"agent_id":agent,"world_time":world.state().time,
                "observation":oasis7_agent_api::ProviderObservation::default(),
                "action_catalog":[{"action_ref":"wait","summary":"Wait"}],"timeout_budget_ms":60000},
            "timeout_budget_ms":60000,"capability_catalog":context.capability_catalog,
            "capability_invocation_context":context.capability_invocation_context},
        "context_discriminator":"oasis7.continuous-agent-context","context_version":1,
        "protocol_version":"continuous-agent-v1","agent_session_id":"session-terminal",
        "agent_turn_id":"turn-terminal","decision_request_id":"request-terminal","retry_seq":1,"transport_attempt":1,
        "agent_subject":agent,"runtime_binding":world.current_cognition_runtime_binding().unwrap(),
        "observation_digest":digest,"capability_catalog_digest":digest,"capability_invocation_context_digest":digest,
        "memory_snapshot_digest":digest,"goal_snapshot_digest":digest,"continuation_digest":digest,
        "adapter_protocol_version":"loopback-http-v1","budget_contract":{"max_latency_ms":60000,"max_repair_attempts":2,"max_model_calls":1,"max_tool_calls":0},
        "request_digest":digest
    })).unwrap();
    request.capability_catalog_digest = oasis7_agent_api::h_v1(
        "oasis7.cognition.capability-catalog.v1",
        request
            .base_decision_request
            .capability_catalog
            .as_ref()
            .unwrap(),
    );
    request.capability_invocation_context_digest = oasis7_agent_api::h_v1(
        "oasis7.cognition.capability-invocation-context.v1",
        request
            .base_decision_request
            .capability_invocation_context
            .as_ref()
            .unwrap(),
    );
    request.request_digest = request.request_digest();
    request.validate_production_lane().unwrap();
    request
}

fn persist_service_diagnostic(reason: &str) -> std::path::PathBuf {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.tmp/5265-terminal-http-diagnostics");
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let metadata = std::fs::symlink_metadata(&directory).unwrap();
    assert!(metadata.is_dir() && !metadata.file_type().is_symlink());
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
    loop {
        let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = directory.join(format!(
            "lookup-{}-{timestamp}-{sequence}.private.txt",
            std::process::id()
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(reason.as_bytes()).unwrap();
                file.sync_all().unwrap();
                return path;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("private diagnostic creation failed: {:?}", error.kind()),
        }
    }
}

#[path = "preview_world_init_terminal_closure_tests.rs"]
mod terminal_closure;
