//! Real-driver TCP conformance, without passing node paths to the application client.
use super::*;
mod application_admission;
mod application_admission_boundaries;
mod application_completion;
mod application_fairness;
mod application_feedback_ack_recovery;
mod application_feedback_ack_write_failure;
mod application_fresh;
mod application_harness;
mod application_hosted;
mod application_hosted_final_budget;
mod application_hosted_final_budget_recovery;
mod application_hosted_final_budget_write_failure;
mod application_hosted_resume_rejection;
mod application_hosted_resume_rejection_recovery;
mod application_hosted_resume_rejection_write_failure;
mod application_hosted_resume_write_failure;
mod application_hosted_unpublished_cadence;
mod application_hosted_wait;
mod application_hosted_wait_capture_crash;
mod application_hosted_wait_crash;
mod application_hosted_wait_recovery;
mod application_hosted_wait_rejection;
mod application_hosted_wait_rejection_recovery;
mod application_hosted_wait_rejection_write_failure;
mod application_hosted_wait_write_failure;
mod application_memory_authority;
mod application_memory_recovery;
mod application_memory_tamper;
mod application_metadata;
mod application_sandbox;
mod application_stream_boundaries;
mod controlled_admission;
mod fixture_stage_diagnostics;
mod hosted_wait_clock;
use fixture_stage_diagnostics::Stage;
mod provider_metadata;
use super::scenario_resource;
use application_harness::{run_isolated_application, run_isolated_application_provider_closure};
mod application_process_dispatch;
mod application_provider;
mod application_release;
mod application_wake;
mod application_world_coherence;
mod http_fixture;
use http_fixture::read_request;
mod legacy_nonce;
mod legacy_routes;
mod privacy_identity;
mod process_restart;
mod production_pressure;
mod publication_pin;
mod release_web_fixture;
mod response_privacy;
mod strict_http;
mod topology_no_mount;
mod topology_pressure;
mod viewer_process;
use oasis7::world_service::client::{
    RemoteWorldServiceClient, WorldServiceClientConfig, WorldServicePort,
};
use oasis7::world_service::*;
use oasis7_node::{
    NodeConfig, NodeConsensusAction, NodeExecutionCommitResult, NodeExecutionHook, NodeRuntime,
};
use oasis7_runtime::{Action, WorldState};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

struct SharedDriver(
    Arc<Mutex<NodeRuntimeExecutionDriver>>,
    Arc<AtomicBool>,
    Arc<AtomicBool>,
);
impl NodeExecutionHook for SharedDriver {
    fn on_commit(
        &mut self,
        context: NodeExecutionCommitContext,
    ) -> Result<NodeExecutionCommitResult, String> {
        while !self.1.load(Ordering::SeqCst) && !self.2.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(2));
        }
        if self.2.load(Ordering::SeqCst) {
            return Err("QA controlled automatic worker aborted at teardown".into());
        }
        self.0.lock().unwrap().on_commit(context)
    }
}

struct Fixture {
    driver: Arc<Mutex<NodeRuntimeExecutionDriver>>,
    automatic_commit_gate: Arc<AtomicBool>,
    abort_automatic_commit: Arc<AtomicBool>,
    preserve_root: bool,
    writer_lock: oasis7::viewer::ExclusiveDirectoryProcessLock,
    root: std::path::PathBuf,
    node: Arc<Mutex<NodeRuntime>>,
    halt: Arc<AtomicBool>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
    client: RemoteWorldServiceClient,
    owner: String,
    lose_next_submit: Arc<AtomicBool>,
    lose_next_release: Arc<AtomicBool>,
    controlled_submit_commit: Arc<AtomicBool>,
    world_gate: Arc<http_fixture::WorldGate>,
    concurrent_dispatch: Arc<AtomicBool>,
    lookup_digests: Arc<Mutex<Vec<String>>>,
    outage: Arc<http_fixture::Outage>,
    tamper_next_describe: Arc<AtomicBool>,
    stage_diagnostics: Arc<fixture_stage_diagnostics::Diagnostics>,
    controlled_completion: Arc<controlled_admission::Completion>,
    // Last field: release after the fixture worker/node/storage fields drop.
    scenario_permit: Arc<scenario_resource::ScenarioResourcePermit>,
}
impl Fixture {
    fn new() -> Self {
        Self::with_controlled_commits(false)
    }
    fn with_controlled_commits(controlled: bool) -> Self {
        Self::with_options(controlled, false)
    }
    fn with_options(controlled: bool, scheduler: bool) -> Self {
        Self::with_clock_options(controlled, scheduler, false)
    }
    fn with_clock_options(controlled: bool, scheduler: bool, quiet_clock: bool) -> Self {
        let scenario_permit = scenario_resource::ScenarioResourcePermit::acquire();
        let root = temp_dir("qa-world-service-tcp");
        let writer_lock =
            crate::world_writer_lock::acquire_live_world_writer_lock(&root.join("world")).unwrap();
        let owner = hex::encode([7u8; 32]);
        let public = sign_read_request("owner", (), &owner)
            .unwrap()
            .subject_public_key;
        let identity = WorldIdentity {
            world_id: "w1".into(),
            genesis_digest: "fixture-genesis-v1".into(),
        };
        let mut state = WorldState::default();
        if quiet_clock {
            // Isolate a genuine empty-clock observation from unrelated automatic
            // crisis lifecycle events. This is original canonical fixture state,
            // never a mutation or a filter applied around the clock assertion.
            state.crises.insert(
                "fixture.clock.active".into(),
                oasis7_runtime::CrisisState {
                    crisis_id: "fixture.clock.active".into(),
                    kind: "supply_shock".into(),
                    severity: 1,
                    status: oasis7_runtime::CrisisStatus::Active,
                    opened_at: 0,
                    expires_at: u64::MAX,
                    resolver_agent_id: None,
                    strategy: None,
                    success: None,
                    impact: 0,
                    resolved_at: None,
                },
            );
        }
        let mut world = oasis7::runtime_host::world::new_with_state(state);
        if scheduler {
            use oasis7_runtime::SchedulerPolicyV1;
            world = world
                .try_with_cognition_scheduler(
                    SchedulerPolicyV1 {
                        schema_version: SchedulerPolicyV1::SCHEMA_VERSION.into(),
                        max_total_wakes_per_tick: 8,
                        max_wakes_per_agent_per_tick: 1,
                        aging_after_ticks: 2,
                        max_starvation_ticks: 4,
                        initial_priority: 0,
                        comparator: SchedulerPolicyV1::COMPARATOR.into(),
                        service_order: SchedulerPolicyV1::SERVICE_ORDER.into(),
                    },
                    1,
                )
                .unwrap();
        }
        world.submit_action(Action::RegisterAgent {
            agent_id: "agent-a".into(),
            pos: oasis7_agent_api::world_action::GeoPos::new(0, 0, 0),
        });
        world.step().unwrap();
        world.submit_action(Action::ClaimStarterOc {
            agent_id: "agent-a".into(),
            player_id: "owner-a".into(),
            public_key: Some(public),
        });
        world.step().unwrap();
        world
            .set_agent_resource_balance(
                "agent-a",
                oasis7_agent_api::world_action::ResourceKind::Electricity,
                100_000,
            )
            .unwrap();
        world
            .install_capability_agent_identity("agent-a", "owner-a", 1)
            .unwrap();
        world
            .bind_cognition_runtime(
                "w1",
                "main",
                0,
                Some(format!(
                    "blake3:{}",
                    blake3::hash(b"qa-signed-finalized-provider-fixture:w1:main:0")
                )),
                "pending",
                0,
            )
            .unwrap();
        world
            .install_test_provider_capability_fixture_without_cognition_balance("agent-a")
            .unwrap();
        assert!(
            world.promote_cognition_runtime_finality().unwrap(),
            "signed finalized fixture authority must promote exact runtime binding"
        );
        assert_eq!(
            world.capability_revocation_state().agent_identities["agent-a"].owner_binding,
            "owner-a"
        );
        world
            .provision_cognition_for_agent(
                "agent-a",
                "qa-canonical-provision",
                "qa-runtime-authority",
                128,
            )
            .unwrap();
        let context_hash = super::super::execution_hash::execution_resource_context_hash("w1");
        world
            .save_to_dir_with_chain_resource_context(
                root.join("world"),
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
            root.join("world/world-service-identity.json"),
            serde_json::to_vec(&identity).unwrap(),
        )
        .unwrap();
        let baseline = super::super::local_bootstrap::derive_local_execution_bootstrap(
            &root.join("world"),
            &root.join("records"),
            "w1",
            2,
            "qa-local-setup-boundary-h2",
            &oasis7_runtime::ReleaseSecurityPolicy::default(),
        )
        .unwrap();
        let driver = NodeRuntimeExecutionDriver::new_with_local_bootstrap(
            root.join("state.json"),
            root.join("world"),
            root.join("records"),
            root.join("store"),
            &oasis7_proto::storage_profile::StorageProfileConfig::default(),
            baseline.clone(),
        )
        .unwrap();
        let driver = Arc::new(Mutex::new(driver));
        let node_baseline = if controlled {
            commit_request(&mut driver.lock().unwrap(), 1, None);
            super::super::local_bootstrap::derive_service_execution_bootstrap(
                &root.join("world"),
                &root.join("records"),
                &root.join("store"),
                "w1",
            )
            .unwrap()
        } else {
            baseline
        };
        let automatic_commit_gate = Arc::new(AtomicBool::new(!controlled));
        let abort_automatic_commit = Arc::new(AtomicBool::new(false));
        let config = NodeConfig::new("node-a", "w1", NodeRole::Sequencer)
            .unwrap()
            .with_tick_interval(if controlled {
                Duration::from_secs(60)
            } else {
                Duration::from_millis(40)
            })
            .unwrap();
        let mut node = NodeRuntime::new(config)
            .with_local_execution_bootstrap(node_baseline)
            .with_execution_hook(SharedDriver(
                driver.clone(),
                automatic_commit_gate.clone(),
                abort_automatic_commit.clone(),
            ));
        node.start().unwrap();
        let node = Arc::new(Mutex::new(node));
        let deadline = Instant::now() + Duration::from_secs(10);
        while super::super::world_service_read::pin(
            &root.join("records"),
            &root.join("store"),
            &identity,
            None,
        )
        .is_err()
        {
            assert!(
                Instant::now() < deadline,
                "real driver did not publish bootstrap record"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let key = hex::encode([9u8; 32]);
        let signer = crate::feedback_submit_api::FeedbackSubmitSigner {
            public_key_hex: sign_read_request("service", (), &key)
                .unwrap()
                .subject_public_key,
            private_key_hex: key,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let halt = Arc::new(AtomicBool::new(false));
        let lose_next_submit = Arc::new(AtomicBool::new(false));
        let worker_lose = lose_next_submit.clone();
        let lose_next_release = Arc::new(AtomicBool::new(false));
        let worker_release = lose_next_release.clone();
        let tamper_next_describe = Arc::new(AtomicBool::new(false));
        let worker_tamper = tamper_next_describe.clone();
        let controlled_submit_commit = Arc::new(AtomicBool::new(false));
        let worker_controlled = controlled_submit_commit.clone();
        let world_gate = Arc::new(http_fixture::WorldGate::default());
        let worker_gate = world_gate.clone();
        let concurrent_dispatch = Arc::new(AtomicBool::new(false));
        let worker_concurrent = concurrent_dispatch.clone();
        let lookup_digests = Arc::new(Mutex::new(Vec::new()));
        let worker_lookups = lookup_digests.clone();
        let outage = Arc::new(http_fixture::Outage::default());
        let worker_outage = outage.clone();
        let worker_driver = driver.clone();
        let worker_halt = halt.clone();
        let worker_node = node.clone();
        let worker_root = root.clone();
        let stage_diagnostics = Arc::new(fixture_stage_diagnostics::Diagnostics::default());
        let worker_diagnostics = stage_diagnostics.clone();
        let controlled_completion = Arc::new(controlled_admission::Completion::default());
        let worker_completion = controlled_completion.clone();
        // QA-only causal probe: delay only a verified successful View response.
        // Each delay remains below the client's transport timeout; cumulative
        // protocol stages may exceed the former completion observation window.
        let successful_view_delay = std::env::var("PRE2_CONFORMANCE_VIEW_DELAY_MS")
            .ok()
            .map(|value| Duration::from_millis(value.parse().expect("View delay milliseconds")))
            .unwrap_or_default();
        assert!(
            successful_view_delay <= Duration::from_millis(1000),
            "causal View delay must remain below the transport timeout"
        );
        let trusted_service_public_key = signer.public_key_hex.clone();
        let worker = thread::spawn(move || {
            let concurrency_gate = worker_gate.clone();
            let handler = Arc::new(move |mut stream: TcpStream| {
                http_fixture::configure_accepted_stream(&stream);
                let bytes = read_request(&mut stream);
                if bytes.is_empty() {
                    return;
                }
                let first = std::str::from_utf8(&bytes).unwrap().lines().next().unwrap();
                let mut parts = first.split_whitespace();
                let method = parts.next().unwrap();
                let path = parts.next().unwrap();
                if matches!(
                    worker_gate.pause(path, &bytes),
                    http_fixture::GateDisposition::FeedbackAckAbandoned
                        | http_fixture::GateDisposition::ResumeViewAbandoned
                        | http_fixture::GateDisposition::WaitAdmitViewAbandoned
                        | http_fixture::GateDisposition::CompensationSettleViewAbandoned
                ) {
                    return;
                }
                if worker_outage.reject(&stream) {
                    return;
                }
                if path == LOOKUP_PATH {
                    http_fixture::record_lookup_digest(&bytes, &worker_lookups);
                }
                let release = path == SUBMIT_PATH
                    && http_fixture::record_submit_digest(&bytes, &worker_lookups);
                let lose = path == SUBMIT_PATH
                    && (worker_lose.swap(false, Ordering::SeqCst)
                        || (release && worker_release.swap(false, Ordering::SeqCst)));
                if lose {
                    stream.shutdown(std::net::Shutdown::Write).unwrap();
                }
                let tamper = path == DESCRIBE_PATH && worker_tamper.swap(false, Ordering::SeqCst);
                let capture_ack = worker_gate.capture_feedback_ack_response(path, &bytes);
                let gated_ack = if path == SUBMIT_PATH
                    && worker_gate
                        .root
                        .lock()
                        .unwrap()
                        .as_ref()
                        .is_some_and(|root| {
                            root.join("world-feedback-ack-single-submit-proof").exists()
                        }) {
                    let body = crate::feedback_submit_api::extract_http_json_body(&bytes).unwrap();
                    let request: SubmitIntentRequest<WorldServicePayloadV1> =
                        serde_json::from_slice(body).unwrap();
                    matches!(
                        &request.signed_payload,
                        WorldServicePayloadV1::FeedbackAck(_)
                    )
                    .then_some(request)
                } else {
                    None
                };
                // Drain authenticated read responses concurrently so the real dispatcher completes
                // before a client teardown can interrupt the response relay.
                let capture_read = method == "POST"
                    && (matches!(path, VIEW_PATH | CHANGES_PATH | LOOKUP_PATH)
                        || (path == SUBMIT_PATH
                            && application_hosted_unpublished_cadence::active(&worker_gate)));
                let mut read_reader = None;
                let mut capture_peer = None;
                let mut output_stream = if capture_read {
                    let capture = TcpListener::bind("127.0.0.1:0").unwrap();
                    let mut peer = TcpStream::connect(capture.local_addr().unwrap()).unwrap();
                    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                    read_reader = Some(thread::spawn(move || {
                        let mut response = Vec::new();
                        (&mut peer)
                            .take(16 * 1024 * 1024 + 1)
                            .read_to_end(&mut response)
                            .unwrap();
                        assert!(
                            response.len() <= 16 * 1024 * 1024,
                            "Read response capture exceeds bound"
                        );
                        response
                    }));
                    capture.accept().unwrap().0
                } else if tamper || capture_ack || gated_ack.is_some() {
                    let capture = TcpListener::bind("127.0.0.1:0").unwrap();
                    capture_peer = Some(TcpStream::connect(capture.local_addr().unwrap()).unwrap());
                    capture.accept().unwrap().0
                } else {
                    stream.try_clone().unwrap()
                };
                let dispatch_started = Instant::now();
                let result = crate::world_service_api::maybe_handle(
                    &mut output_stream,
                    &bytes,
                    &worker_node,
                    method,
                    path,
                    "w1",
                    &worker_root.join("world"),
                    &worker_root.join("records"),
                    &worker_root.join("store"),
                    &signer,
                );
                worker_diagnostics.record(Stage::Dispatcher, dispatch_started.elapsed());
                eprintln!("fixture_dispatch method={method} path={path} result={result:?}");
                // A verified whole-process exit at the intentional Settle gate may
                // close its response socket. Admission and canonical execution must
                // still complete, exactly as the real node's separate commit worker does.
                let expected_settle_disconnect = path == SUBMIT_PATH
                    && worker_controlled.load(Ordering::SeqCst)
                    && result.as_ref().err().is_some_and(|error| {
                        error == &std::io::Error::from_raw_os_error(libc::EPIPE).to_string()
                    })
                    && worker_gate.root.lock().unwrap().as_ref().is_some_and(|root| {
                        root.join("world-settle-crash-exit73-confirmed").exists()
                    })
                    && crate::feedback_submit_api::extract_http_json_body(&bytes).ok()
                        .and_then(|body| serde_json::from_slice::<SubmitIntentRequest<WorldServicePayloadV1>>(body).ok())
                        .is_some_and(|request| matches!(&request.signed_payload,
                            WorldServicePayloadV1::Scheduler(signed) if matches!(signed.request.operation, SchedulerOperationV1::SettleLease { .. }))
                            && worker_gate.root.lock().unwrap().as_ref().is_some_and(|root| {
                                fs::read(root.join("world-settle-started")).ok()
                                    .and_then(|bytes| serde_json::from_slice::<SubmitIntentRequest<WorldServicePayloadV1>>(&bytes).ok())
                                    .is_some_and(|original| original == request)
                            }));
                if expected_settle_disconnect {
                    println!(
                        "PRE2_VERIFIED_CRASH_SETTLE_RESPONSE_DISCONNECTED original_bytes_preserved=true canonical_commit_still_required=true"
                    );
                }
                if !lose && !expected_settle_disconnect {
                    assert!(
                        result.unwrap(),
                        "actual dispatcher did not handle requested route"
                    );
                }
                if gated_ack.is_some() && worker_controlled.load(Ordering::SeqCst) {
                    let body = crate::feedback_submit_api::extract_http_json_body(&bytes).unwrap();
                    let request = serde_json::from_slice(body).unwrap();
                    controlled_admission::execute(
                        &worker_driver,
                        &worker_root.join("store"),
                        request,
                        &worker_diagnostics,
                        &worker_completion,
                    )
                    .expect("controlled admission must retain the original published result");
                }
                let relay_started = Instant::now();
                drop(output_stream);
                if let Some(reader) = read_reader {
                    let response = reader.join().expect("Read response capture reader failed");
                    let verified_success = response.starts_with(b"HTTP/1.1 200 ");
                    if verified_success {
                        let body =
                            crate::feedback_submit_api::extract_http_json_body(&bytes).unwrap();
                        let offset = response
                            .windows(4)
                            .position(|window| window == b"\r\n\r\n")
                            .unwrap()
                            + 4;
                        application_hosted_unpublished_cadence::verify_captured_response(
                            path,
                            body,
                            &response[offset..],
                            &signer.public_key_hex,
                            &worker_gate,
                        );
                    }
                    if verified_success && path == VIEW_PATH && !successful_view_delay.is_zero() {
                        println!(
                            "conformance_successful_view_delay_ms={} original_request_digest={} response_unchanged=true",
                            successful_view_delay.as_millis(),
                            blake3_hex(&bytes)
                        );
                        thread::sleep(successful_view_delay);
                    }
                    if let Err(error) = stream.write_all(&response) {
                        assert!(
                            verified_success
                                && matches!(
                                    error.raw_os_error(),
                                    Some(libc::EPIPE) | Some(libc::ECONNRESET)
                                ),
                            "Read response relay failed without a verified successful read: {error}"
                        );
                        println!(
                            "PRE2_VERIFIED_READ_RESPONSE_CANCELLED route={path} dispatcher_completed=true signed_response_verified=true unchanged_response_relay_attempted=true"
                        );
                    }
                }
                if let Some(mut peer) = capture_peer {
                    let mut response = Vec::new();
                    peer.read_to_end(&mut response).unwrap();
                    if tamper {
                        let offset = response
                            .windows(4)
                            .position(|window| window == b"\r\n\r\n")
                            .unwrap()
                            + 4;
                        let mut body: serde_json::Value =
                            serde_json::from_slice(&response[offset..]).unwrap();
                        body["signature_hex"] = serde_json::json!("00".repeat(64));
                        crate::write_json_response(
                            &mut stream,
                            200,
                            &serde_json::to_vec(&body).unwrap(),
                            false,
                        )
                        .unwrap();
                    } else if let Some(request) = gated_ack.as_ref() {
                        // The existing real execution driver remains the sole committer.
                        // Never retain its mutex while waiting for a future commit.
                        let key = correlation::key_digest(&request.correlation.key).unwrap();
                        let deadline = Instant::now() + Duration::from_secs(15);
                        loop {
                            let result = {
                                let driver = worker_driver.lock().unwrap();
                                driver
                                    .execution_world
                                    .capability_revocation_state()
                                    .world_service_results
                                    .get(&key)
                                    .cloned()
                            };
                            if let Some(result) = result {
                                let result: wire::CanonicalIntentResultV1 =
                                    serde_json::from_value(result).unwrap();
                                assert_eq!(result.request.correlation, request.correlation);
                                assert_eq!(result.request.signed_payload, request.signed_payload);
                                assert!(result.rejected.is_none());
                                assert!(
                                    worker_root
                                        .join("records")
                                        .join(format!("{:020}.json", result.committed_height))
                                        .exists()
                                );
                                break;
                            }
                            assert!(
                                Instant::now() < deadline,
                                "real automatic ACK commit did not reach proof boundary"
                            );
                            thread::sleep(Duration::from_millis(2));
                        }
                        assert!(
                            !response.is_empty(),
                            "actual ACK dispatcher response captured"
                        );
                        if !capture_ack {
                            stream.write_all(&response).unwrap();
                        }
                    } else {
                        assert!(capture_ack);
                        assert!(
                            !response.is_empty(),
                            "actual ACK dispatcher response captured"
                        );
                    }
                }
                worker_diagnostics.record(Stage::ResponseCaptureRelay, relay_started.elapsed());
                if path == SUBMIT_PATH
                    && gated_ack.is_none()
                    && worker_controlled.load(Ordering::SeqCst)
                {
                    application_hosted_unpublished_cadence::hold_original_commit(&worker_gate);
                    let body = crate::feedback_submit_api::extract_http_json_body(&bytes).unwrap();
                    let request = serde_json::from_slice(body).unwrap();
                    controlled_admission::execute(
                        &worker_driver,
                        &worker_root.join("store"),
                        request,
                        &worker_diagnostics,
                        &worker_completion,
                    )
                    .expect("controlled admission must retain the original published result");
                }
                if capture_ack {
                    worker_gate.hold_feedback_ack_after_commit(path, &bytes);
                }
            });
            http_fixture::serve_listener(
                listener,
                worker_halt,
                worker_concurrent,
                concurrency_gate,
                handler,
            );
        });
        let client = RemoteWorldServiceClient::new(WorldServiceClientConfig {
            formal_read_trust: None,
            endpoint,
            trusted_service_public_key,
            expected_world: identity,
            scope_id: "public".into(),
            read_private_key_hex: owner.clone(),
            timeout: Duration::from_secs(2),
            max_response_bytes: 1_048_576,
        })
        .unwrap();
        http_fixture::wait_ready(&client, &worker);
        Self {
            driver,
            automatic_commit_gate,
            abort_automatic_commit,
            preserve_root: false,
            writer_lock,
            root,
            node,
            halt,
            worker: Mutex::new(Some(worker)),
            client,
            owner,
            lose_next_submit,
            lose_next_release,
            controlled_submit_commit,
            world_gate,
            concurrent_dispatch,
            lookup_digests,
            outage,
            tamper_next_describe,
            stage_diagnostics,
            controlled_completion,
            scenario_permit,
        }
    }
    fn request(
        &self,
        payload: WorldServicePayloadV1,
    ) -> SubmitIntentRequest<WorldServicePayloadV1> {
        SubmitIntentRequest {
            contract_version: 1,
            correlation: derive_correlation(self.client.config().expected_world.clone(), &payload)
                .unwrap(),
            deadline_unix_ms: None,
            signed_payload: payload,
        }
    }
    fn delegation(&self) -> SubmitIntentRequest<WorldServicePayloadV1> {
        self.request(WorldServicePayloadV1::Delegation(
            sign_read_request(
                "delegation",
                AgentSignerDelegationChangeV1 {
                    world_id: "w1".into(),
                    branch_id: "main".into(),
                    agent_id: "agent-a".into(),
                    owner_binding: "owner-a".into(),
                    agent_identity_generation: 1,
                    generation: 1,
                    delegate_public_key: sign_read_request("delegate", (), &hex::encode([8u8; 32]))
                        .unwrap()
                        .subject_public_key,
                    revoked: false,
                    nonce: 1,
                },
                &self.owner,
            )
            .unwrap(),
        ))
    }
    fn committed(&self, request: &SubmitIntentRequest<WorldServicePayloadV1>) -> CommitRef {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let result = self
                .client
                .lookup(
                    LookupIntentRequest {
                        contract_version: 1,
                        key: request.correlation.key.clone(),
                    },
                    request.signed_payload.clone(),
                )
                .unwrap();
            match result.outcome {
                IntentOutcome::Committed { commit, .. } => return *commit,
                IntentOutcome::Unknown
                | IntentOutcome::Received { .. }
                | IntentOutcome::Pending => {}
                other => {
                    let driver = self.driver.lock().unwrap();
                    let key = correlation::key_digest(&request.correlation.key).unwrap();
                    let reason = driver
                        .execution_world
                        .capability_revocation_state()
                        .world_service_results
                        .get(&key)
                        .and_then(|value| value.get("rejected"))
                        .cloned();
                    panic!(
                        "expected canonical commit, got {other:?}; canonical_rejected_reason={reason:?}"
                    );
                }
            }
            assert!(Instant::now() < deadline, "canonical receipt not published");
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn view(&self, min_commit: Option<CommitRef>) -> ReadWorldViewRequest {
        ReadWorldViewRequest {
            observation_nonce: None,
            formal_anchor: None,
            contract_version: 1,
            world: self.client.config().expected_world.clone(),
            scope_id: "public".into(),
            min_commit,
            fixed_commit: None,
            deadline_unix_ms: None,
        }
    }
}
impl Fixture {
    fn finish_http_workers(&self) -> Result<(), &'static str> {
        self.halt.store(true, Ordering::SeqCst);
        let worker = self
            .worker
            .lock()
            .map_err(|_| "actual fixture listener ownership lock poisoned")?
            .take();
        if let Some(worker) = worker {
            worker
                .join()
                .map_err(|_| "actual fixture listener or connection worker failed")?;
        }
        Ok(())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.finish_http_workers();
        self.abort_automatic_commit.store(true, Ordering::SeqCst);
        self.automatic_commit_gate.store(true, Ordering::SeqCst);
        let _ = self.node.lock().unwrap().stop();
        if !self.preserve_root {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
#[test]
fn real_tcp_five_operations_delegation_and_minimum_commit() {
    let fixture = Fixture::new();
    fixture.client.describe().unwrap();
    let original = fixture.delegation();
    fixture.client.submit(original.clone()).unwrap();
    let commit = fixture.committed(&original);
    let view = fixture
        .client
        .read_view(fixture.view(Some(commit.clone())))
        .unwrap();
    assert!(view.version().commit.satisfies_minimum(&commit).unwrap());
    let changes = fixture
        .client
        .read_changes(ReadWorldChangesRequest {
            contract_version: 1,
            cursor: view.continuation().clone(),
            max_items: 32,
            max_bytes: 65_536,
        })
        .unwrap();
    assert_eq!(changes.next_cursor.scope_id, "public");
    let replay = fixture.client.submit(original.clone()).unwrap();
    assert!(matches!(
        replay,
        SubmitObservation::Response(response)
            if matches!(response.outcome, IntentOutcome::Committed { .. })
    ));
    let mut future = commit;
    future.position = u64::MAX;
    assert!(
        fixture
            .client
            .read_view(fixture.view(Some(future)))
            .is_err()
    );
    let mut generation = view.version().commit.clone();
    generation.binding.permission_generation += 1;
    assert!(
        fixture
            .client
            .read_view(fixture.view(Some(generation)))
            .is_err()
    );
}
#[test]
fn real_tcp_rejects_forged_delegation_and_cursor() {
    let fixture = Fixture::new();
    let mut forged = fixture.delegation();
    if let WorldServicePayloadV1::Delegation(signed) = &mut forged.signed_payload {
        signed.signature_hex = "00".repeat(64);
    }
    assert!(fixture.client.submit(forged).is_err());
    let view = fixture.client.read_view(fixture.view(None)).unwrap();
    let mut cursor = view.continuation().clone();
    cursor.stream_id = "unrelated-stream".into();
    assert!(
        fixture
            .client
            .read_changes(ReadWorldChangesRequest {
                contract_version: 1,
                cursor,
                max_items: 32,
                max_bytes: 65_536
            })
            .is_err()
    );
}

#[test]
fn real_tcp_lost_response_signed_gameplay_and_driver_restart() {
    use oasis7::viewer::{CollectDataCommand, CollectDataRequest, sign_collect_data_auth_proof};
    let fixture = Fixture::new();
    let public = sign_read_request("owner", (), &fixture.owner)
        .unwrap()
        .subject_public_key;
    let mut command = CollectDataCommand::Submit {
        request: CollectDataRequest {
            electricity_cost: 7,
            data_amount: 11,
            player_id: "owner-a".into(),
            public_key: Some(public.clone()),
            auth: None,
        },
    };
    let proof = sign_collect_data_auth_proof(&command, 9, &public, &fixture.owner).unwrap();
    let CollectDataCommand::Submit { request } = &mut command else {
        unreachable!()
    };
    request.auth = Some(proof);
    let original = fixture.request(WorldServicePayloadV1::GameplayJson(
        serde_json::to_vec(&command).unwrap(),
    ));
    fixture.lose_next_submit.store(true, Ordering::SeqCst);
    assert!(matches!(
        fixture.client.submit(original.clone()).unwrap(),
        SubmitObservation::OutcomeUnknown(_)
    ));
    let commit = fixture.committed(&original);
    fixture.node.lock().unwrap().stop().unwrap();
    let restarted = NodeRuntimeExecutionDriver::new(
        fixture.root.join("state.json"),
        fixture.root.join("world"),
        fixture.root.join("records"),
        fixture.root.join("store"),
    )
    .unwrap();
    assert_eq!(
        restarted
            .execution_world
            .state()
            .authenticated_collect_data_last_nonces["owner-a"][&public],
        9
    );
    let outcome = fixture
        .client
        .lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: original.correlation.key.clone(),
            },
            original.signed_payload.clone(),
        )
        .unwrap();
    assert!(
        matches!(outcome.outcome,IntentOutcome::Committed { commit: recovered,.. } if *recovered==commit)
    );
    // The restarted driver and immutable persisted service read agree on the exact receipt.
    let key = correlation::key_digest(&original.correlation.key).unwrap();
    assert!(
        restarted
            .execution_world
            .capability_revocation_state()
            .world_service_results
            .contains_key(&key)
    );
}

#[test]
fn real_tcp_application_artifact_with_os_denied_node_directory() {
    run_isolated_application_provider_closure();
}

#[test]
fn real_tcp_application_genuine_wait_wake_no_node_directory() {
    run_isolated_application(true, false, false, false, false, false);
}

#[test]
#[ignore = "child application entrypoint; parent supplies explicit config and OS sandbox"]
fn application_process_probe() {
    application_process_dispatch::run();
}

// Explicit controlled canonical execution is a deterministic service fixture;
// it does not claim automatic consensus-worker scheduling coverage.
fn commit_request(
    driver: &mut NodeRuntimeExecutionDriver,
    _logical_fixture_step: u64,
    request: Option<SubmitIntentRequest<WorldServicePayloadV1>>,
) {
    let height = driver.state.last_applied_committed_height + 1;
    let actions = request
        .into_iter()
        .map(|request| {
            let digest = correlation::key_digest(&request.correlation.key).unwrap();
            let hash = blake3::hash(digest.as_bytes());
            let action_id = u64::from_be_bytes(hash.as_bytes()[..8].try_into().unwrap()).max(1);
            let bytes = correlation::encode_consensus_intent(&request).unwrap();
            NodeConsensusAction::from_payload(action_id, "node-a", bytes).unwrap()
        })
        .collect::<Vec<_>>();
    driver
        .on_commit(NodeExecutionCommitContext {
            world_id: "w1".into(),
            node_id: "node-a".into(),
            proposer_id: "node-a".into(),
            height,
            slot: height,
            epoch: 0,
            node_block_hash: format!("controlled-node-h{height}"),
            action_root: compute_consensus_action_root(&actions).unwrap(),
            committed_actions: actions,
            committed_at_unix_ms: height as i64 * 1000,
        })
        .unwrap();
}
#[path = "qa_conformance/agent_cognition_ack.rs"]
mod agent_cognition_ack;

#[test]
fn real_tcp_controlled_agent_cognition_receipt_and_read() {
    agent_cognition_ack::run();
}

fn cognition_request(fixture: &Fixture) -> SubmitIntentRequest<WorldServicePayloadV1> {
    use oasis7_runtime::{RuntimeCognitionCommitRequestV1, RuntimeCognitionResponseArtifactV1};
    let world = fixture.driver.lock().unwrap().execution_world.clone();
    let digest =
        |label: &str| oasis7::simulator::h_v1("oasis7.test.qa-conformance.v1", &label).to_string();
    let request = RuntimeCognitionCommitRequestV1 {
        agent_id: "agent-a".into(),
        agent_session_id: "qa-session".into(),
        agent_turn_id: "qa-turn".into(),
        decision_request_id: "qa-cognition".into(),
        retry_seq: 1,
        transport_attempt: 1,
        request_digest: digest("request"),
        observation_digest: digest("observation"),
        context_digest: digest("context"),
        capability_snapshot_hash: oasis7::simulator::h_v1(
            "oasis7.runtime.manifest.v1",
            &world.capability_authorization_root(),
        )
        .to_string(),
        authority_context_hash: oasis7::simulator::h_v1(
            "oasis7.runtime.authority-context.v1",
            &world.capability_authorization_root(),
        )
        .to_string(),
        captured_base_binding: world.current_cognition_base_binding().unwrap(),
    };
    let mut response_artifact = RuntimeCognitionResponseArtifactV1 {
        schema_version: 1,
        context_discriminator: RuntimeCognitionResponseArtifactV1::CONTEXT_DISCRIMINATOR.into(),
        context_version: RuntimeCognitionResponseArtifactV1::CONTEXT_VERSION,
        agent_session_id: request.agent_session_id.clone(),
        agent_turn_id: request.agent_turn_id.clone(),
        decision_request_id: request.decision_request_id.clone(),
        retry_seq: request.retry_seq,
        transport_attempt: request.transport_attempt,
        request_digest: request.request_digest.clone(),
        response_digest: digest("response"),
        artifact_digest: String::new(),
    };
    response_artifact.refresh_artifact_digest();
    let payload = WorldServicePayloadV1::Cognition(
        sign_read_request(
            "cognition",
            CognitionIntentV1 {
                request,
                action: Action::MoveAgent {
                    agent_id: "agent-a".into(),
                    to: oasis7_agent_api::world_action::GeoPos::new(1, 1, 0),
                },
                response_artifact,
                delegation_generation: 1,
                task_fence: None,
                causal_proposal: Some(CognitionCausalProposalV1 {
                    expected_consequence: serde_json::json!({"provenance":"agent_explanation_unverified", "prediction":"reach destination"}),
                    stakes: serde_json::json!({"provenance":"agent_explanation_unverified", "summary":"travel cost"}),
                    alternative: serde_json::json!({"provenance":"agent_explanation_unverified", "alternatives":["wait"]}),
                    reason: Some("move towards goal".into()),
                    evidence_refs: vec![digest("observation"), digest("memory")],
                    correction_refs: vec!["correction:qa-public".into()],
                    dissent: Some("waiting is safer".into()),
                }),
            },
            &hex::encode([8u8; 32]),
        )
        .unwrap(),
    );
    fixture.request(payload)
}

#[test]
fn real_tcp_conflicting_key_and_revoked_delegate_leave_canonical_result_unchanged() {
    let fixture = Fixture::with_controlled_commits(true);
    let registration = fixture.delegation();
    fixture.client.submit(registration.clone()).unwrap();
    commit_request(
        &mut fixture.driver.lock().unwrap(),
        2,
        Some(registration.clone()),
    );
    let original_commit = fixture.committed(&registration);
    let cognition = cognition_request(&fixture);
    let WorldServicePayloadV1::Delegation(signed) = &registration.signed_payload else {
        unreachable!()
    };
    let mut conflicting = signed.request.clone();
    conflicting.revoked = true;
    let conflicting = fixture.request(WorldServicePayloadV1::Delegation(
        sign_read_request("delegation", conflicting, &fixture.owner).unwrap(),
    ));
    assert_eq!(conflicting.correlation.key, registration.correlation.key);
    assert_ne!(
        conflicting.correlation.payload_digest,
        registration.correlation.payload_digest
    );
    assert!(fixture.client.submit(conflicting).is_err());
    assert_eq!(fixture.committed(&registration), original_commit);
    let mut revocation = signed.request.clone();
    revocation.revoked = true;
    revocation.generation = 2;
    revocation.nonce = 2;
    let revocation = fixture.request(WorldServicePayloadV1::Delegation(
        sign_read_request("delegation", revocation, &fixture.owner).unwrap(),
    ));
    fixture.client.submit(revocation.clone()).unwrap();
    commit_request(
        &mut fixture.driver.lock().unwrap(),
        3,
        Some(revocation.clone()),
    );
    fixture.committed(&revocation);
    assert!(
        fixture.client.submit(cognition).is_err(),
        "revoked provider cannot admit cognition"
    );
    let mut config = fixture.client.config().clone();
    config.scope_id = "agent:agent-a".into();
    config.read_private_key_hex = hex::encode([11u8; 32]);
    let client = RemoteWorldServiceClient::new(config).unwrap();
    let mut read = fixture.view(None);
    read.scope_id = "agent:agent-a".into();
    assert!(
        client.read_view(read).is_err(),
        "unrelated key cannot read protected Agent scope"
    );
}

#[test]
fn real_tcp_tampered_service_response_is_not_trusted() {
    let fixture = Fixture::new();
    fixture.client.describe().unwrap();
    fixture.tamper_next_describe.store(true, Ordering::SeqCst);
    assert!(
        matches!(
            fixture.client.describe(),
            Err(oasis7::world_service::client::WorldServiceClientError::Assurance(_))
        ),
        "tampered actual service response must fail configured trust"
    );
    fixture.client.describe().unwrap();
}

#[test]
fn real_tcp_live_writer_lock_excludes_second_writer() {
    let fixture = Fixture::new();
    let error =
        crate::world_writer_lock::acquire_live_world_writer_lock(&fixture.root.join("world"))
            .err()
            .expect("second writer must be excluded");
    assert!(
        error.contains("held") && error.contains("lock"),
        "unexpected lock failure: {error}"
    );
    fixture.client.describe().unwrap();
    let _held = &fixture.writer_lock;
}

mod application_periodic_late_completion;

mod application_act_recovery;

#[path = "qa_conformance/game_task.rs"]
mod game_task_bootstrap;
