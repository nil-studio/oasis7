use super::*;
use oasis7_simulator::WorldEventKind;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[path = "chain_sync_recovery.rs"]
mod chain_sync_recovery;
#[path = "service_submission_admission.rs"]
mod service_submission_admission;

struct HostedLocalMockProviderEnvSnapshot {
    previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl HostedLocalMockProviderEnvSnapshot {
    fn capture() -> Self {
        let keys = [
            VIEWER_AGENT_DECISION_SOURCE_ENV,
            VIEWER_AGENT_PROVIDER_BACKEND_ENV,
            VIEWER_AGENT_PROVIDER_CONTRACT_ENV,
            VIEWER_AGENT_PROVIDER_TRANSPORT_ENV,
            VIEWER_AGENT_PROVIDER_URL_ENV,
            VIEWER_AGENT_PROVIDER_PROFILE_ENV,
            VIEWER_AGENT_EXECUTION_LANE_ENV,
            VIEWER_AGENT_PROVIDER_MODE_ENV,
        ];
        let previous = keys
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect();
        clear_runtime_provider_env();
        Self { previous }
    }
}

impl Drop for HostedLocalMockProviderEnvSnapshot {
    fn drop(&mut self) {
        for (key, value) in self.previous.drain(..) {
            // SAFETY: The test holds the canonical provider environment lock.
            unsafe {
                match value {
                    Some(value) => oasis7::env_mut::set_var(key, value),
                    None => oasis7::env_mut::remove_var(key),
                }
            }
        }
    }
}

fn configure_hosted_local_mock_provider_env() {
    // SAFETY: The caller holds the canonical provider environment lock.
    unsafe {
        oasis7::env_mut::set_var(VIEWER_AGENT_DECISION_SOURCE_ENV, "provider_backed");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_MODE_ENV, "provider_backed");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_BACKEND_ENV, "provider_local_mock");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_CONTRACT_ENV, "worldsim_provider_v1");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_TRANSPORT_ENV, "loopback_http");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_URL_ENV, "http://127.0.0.1:9");
        oasis7::env_mut::set_var(VIEWER_AGENT_PROVIDER_PROFILE_ENV, "oasis7_p0_low_freq_npc");
        oasis7::env_mut::set_var(VIEWER_AGENT_EXECUTION_LANE_ENV, "player_parity");
    }
}

pub(crate) struct TestChainStatusServer {
    pub(crate) addr: String,
    pub(crate) committed_height: Arc<AtomicU64>,
    status_requests: Arc<AtomicU64>,
    submitted_gameplay_requests: Arc<Mutex<Vec<crate::viewer::GameplayActionRequest>>>,
    stop: Arc<AtomicBool>,
    join_handle: Option<thread::JoinHandle<()>>,
}

impl TestChainStatusServer {
    pub(crate) fn start(execution_world_dir: std::path::PathBuf) -> Self {
        Self::start_with_release_security_policy(
            execution_world_dir.clone(),
            ReleaseSecurityPolicy::production_hardened(),
        )
    }

    pub(crate) fn start_with_release_security_policy(
        execution_world_dir: std::path::PathBuf,
        release_security_policy: ReleaseSecurityPolicy,
    ) -> Self {
        Self::start_with_release_security_policy_and_delay(
            execution_world_dir.clone(),
            release_security_policy,
            Duration::ZERO,
        )
    }

    pub(crate) fn start_with_status_delay(
        execution_world_dir: std::path::PathBuf,
        status_delay: Duration,
    ) -> Self {
        Self::start_with_release_security_policy_and_delay(
            execution_world_dir.clone(),
            ReleaseSecurityPolicy::production_hardened(),
            status_delay,
        )
    }

    fn start_with_release_security_policy_and_delay(
        execution_world_dir: std::path::PathBuf,
        release_security_policy: ReleaseSecurityPolicy,
        status_delay: Duration,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind chain status server");
        listener
            .set_nonblocking(true)
            .expect("set chain status listener nonblocking");
        let addr = listener.local_addr().expect("chain status local addr");
        let committed_height = Arc::new(AtomicU64::new(0));
        let status_requests = Arc::new(AtomicU64::new(0));
        let submitted_gameplay_requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let committed_height_for_thread = Arc::clone(&committed_height);
        let status_requests_for_thread = Arc::clone(&status_requests);
        let submitted_requests_for_thread = Arc::clone(&submitted_gameplay_requests);
        let next_gameplay_action_id_for_thread = Arc::new(AtomicU64::new(1));
        let stop_for_thread = Arc::clone(&stop);
        let execution_world_dir_for_thread = execution_world_dir.clone();
        let release_security_policy_for_thread = release_security_policy.clone();
        let status_delay_for_thread = status_delay;
        let join_handle = thread::spawn(move || {
            loop {
                if stop_for_thread.load(Ordering::SeqCst) {
                    break;
                }
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        if stop_for_thread.load(Ordering::SeqCst) {
                            break;
                        }
                        let request = read_test_http_request(&mut stream);
                        let request_bytes = request.as_slice();
                        let request_text = String::from_utf8_lossy(request_bytes);
                        let mut parts = request_text
                            .lines()
                            .next()
                            .unwrap_or_default()
                            .split_whitespace();
                        let method = parts.next().unwrap_or_default();
                        let path = parts
                            .next()
                            .unwrap_or_default()
                            .split('?')
                            .next()
                            .unwrap_or_default();

                        match (method, path) {
                            ("GET", "/v1/chain/status") => {
                                status_requests_for_thread.fetch_add(1, Ordering::SeqCst);
                                if !status_delay_for_thread.is_zero() {
                                    thread::sleep(status_delay_for_thread);
                                }
                                let body = serde_json::json!({
                                    "consensus": {
                                        "committed_height": committed_height_for_thread.load(Ordering::SeqCst),
                                    },
                                    "execution_world_dir": execution_world_dir_for_thread,
                                    "release_security_policy": release_security_policy_for_thread,
                                });
                                let body =
                                    serde_json::to_vec(&body).expect("encode chain status body");
                                let response = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    body.len()
                                );
                                stream
                                    .write_all(response.as_bytes())
                                    .expect("write chain status header");
                                stream
                                    .write_all(body.as_slice())
                                    .expect("write chain status body");
                                stream.flush().expect("flush chain status response");
                            }
                            ("POST", "/v1/chain/gameplay/submit") => {
                                let boundary = request_bytes
                                    .windows(4)
                                    .position(|window| window == b"\r\n\r\n")
                                    .expect("gameplay submit body boundary");
                                let body = &request_bytes[(boundary + 4)..];
                                let gameplay_request = serde_json::from_slice::<
                                    crate::viewer::GameplayActionRequest,
                                >(body)
                                .expect("decode gameplay submit request");
                                submitted_requests_for_thread
                                    .lock()
                                    .expect("lock submitted requests")
                                    .push(gameplay_request);
                                let action_id = next_gameplay_action_id_for_thread
                                    .fetch_add(1, Ordering::SeqCst);
                                let body = serde_json::json!({
                                    "ok": true,
                                    "action_id": action_id,
                                    "submitted_at_unix_ms": test_now_unix_ms(),
                                });
                                let body =
                                    serde_json::to_vec(&body).expect("encode gameplay submit body");
                                let response = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    body.len()
                                );
                                stream
                                    .write_all(response.as_bytes())
                                    .expect("write gameplay submit header");
                                stream
                                    .write_all(body.as_slice())
                                    .expect("write gameplay submit body");
                                stream.flush().expect("flush gameplay submit response");
                            }
                            _ => {
                                stream
                                .write_all(
                                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 21\r\nConnection: close\r\n\r\n{\"error\":\"not found\"}",
                                )
                                .expect("write 404 response");
                                stream.flush().expect("flush 404 response");
                            }
                        }
                    }
                    Err(err) => {
                        if err.kind() == std::io::ErrorKind::WouldBlock {
                            thread::sleep(Duration::from_millis(10));
                            continue;
                        }
                        panic!("accept chain status connection failed: {err}");
                    }
                }
            }
        });
        Self {
            addr: addr.to_string(),
            committed_height,
            status_requests,
            submitted_gameplay_requests,
            stop,
            join_handle: Some(join_handle),
        }
    }

    pub(crate) fn submitted_gameplay_requests(&self) -> Vec<crate::viewer::GameplayActionRequest> {
        self.submitted_gameplay_requests
            .lock()
            .expect("lock submitted requests")
            .clone()
    }

    pub(crate) fn status_requests(&self) -> u64 {
        self.status_requests.load(Ordering::SeqCst)
    }
}

impl Drop for TestChainStatusServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr.as_str());
        if let Some(join_handle) = self.join_handle.take() {
            join_handle.join().expect("join chain status server");
        }
    }
}

#[test]
fn chain_linked_formal_default_starts_without_local_fallback_agent() {
    let server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::formal_release_default()
            .with_chain_status_bind("127.0.0.1:1")
            .with_chain_link_policy(ChainLinkPolicy::Enforcing),
    )
    .expect("runtime server");

    assert!(
        server.world.state().agents.is_empty(),
        "chain-linked default entry should wait for committed runtime or player claim"
    );
}

#[test]
fn hosted_local_mock_chain_viewer_defers_fixture_install_until_authoritative_sync() {
    let _env_guard = runtime_provider_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _provider_env_snapshot = HostedLocalMockProviderEnvSnapshot::capture();
    configure_hosted_local_mock_provider_env();

    let execution_world_dir = runtime_live_temp_dir("chain_sync_hosted_local_mock_init");
    let mut execution_world = crate::runtime::World::new_production_hardened();
    execution_world
        .bind_cognition_runtime(
            VIEWER_FORMAL_RELEASE_DEFAULT_WORLD_ID,
            "hosted-local-mock-branch",
            0,
            Some(
                "blake3:1111111111111111111111111111111111111111111111111111111111111111"
                    .to_string(),
            ),
            "verified",
            0,
        )
        .expect("bind authoritative chain cognition runtime");
    execution_world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "hosted-chain-agent".to_string(),
        pos: crate::geometry::GeoPos::new(1, 2, 0),
    });
    execution_world
        .step()
        .expect("register authoritative chain Agent");
    execution_world
        .save_to_dir_with_chain_resource_context(
            execution_world_dir.as_path(),
            crate::runtime::ChainResourceDerivationContext {
                world_id: VIEWER_FORMAL_RELEASE_DEFAULT_WORLD_ID,
                chain_id: "hosted-local-mock-chain",
                genesis_ref: Some("hosted-local-mock-genesis"),
                created_at_height: 1,
                manifest_height: 1,
                commit_block_hash: Some("hosted-local-mock-block-1"),
                tick: execution_world.state().time,
            },
            "hosted-local-mock-world-config",
            "hosted-local-mock-generation",
        )
        .expect("persist authoritative chain world");

    let chain_status = TestChainStatusServer::start(execution_world_dir.clone());
    chain_status.committed_height.store(1, Ordering::SeqCst);

    let mut viewer = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::formal_release_default()
            .with_hosted_public_join_mode(true)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone()),
    )
    .expect("empty pre-sync formal world must not abort Hosted local-mock startup");
    assert!(
        viewer.world.state().agents.is_empty(),
        "the viewer must remain empty until authoritative chain sync"
    );
    assert!(
        viewer.llm_sidecar.supports_prompt_control_result(),
        "Hosted local-mock lane must be active while fixture installation is pending"
    );

    let mut session = RuntimeLiveSession::new();
    let (mut writer, peer) = test_writer_pair();
    viewer
        .handle_request(
            ViewerRequest::HelloV2 {
                client: "hosted-local-mock-chain-client".to_string(),
                version: VIEWER_PROTOCOL_VERSION,
                capabilities: vec![PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
            },
            &mut session,
            &mut writer,
        )
        .expect("HelloV2 must be accepted before the first chain snapshot");
    let hello_responses = read_available_runtime_live_responses(&peer, Duration::from_millis(200));
    assert!(
        hello_responses.iter().any(|response| matches!(
            response,
            ViewerResponse::HelloAck { capabilities, .. }
                if capabilities
                    .iter()
                    .any(|capability| capability == PROMPT_CONTROL_RESULT_CAPABILITY)
        )),
        "Hosted local-mock chain startup must negotiate prompt_control_result before sync: {hello_responses:?}"
    );
    viewer
        .handle_request(ViewerRequest::RequestSnapshot, &mut session, &mut writer)
        .expect("RequestSnapshot must reuse the authoritative HelloV2 prime");
    let snapshot_line = read_response_line(&peer, Duration::from_millis(200))
        .expect("RequestSnapshot must serialize a response");
    assert!(
        snapshot_line.contains("\"snapshot\"") || snapshot_line.contains("\"Snapshot\""),
        "RequestSnapshot must serialize the already-authoritative world: {snapshot_line}"
    );
    assert_eq!(
        chain_status.status_requests(),
        1,
        "HelloV2 and RequestSnapshot must not double-prime the same chain world"
    );
    viewer
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect("authoritative chain sync should install the pending fixture");

    assert!(
        viewer
            .world
            .state()
            .agents
            .contains_key("hosted-chain-agent"),
        "authoritative Agent must be adopted before local fixture installation"
    );
    assert!(
        !viewer.world.capability_invocation_contexts().is_empty(),
        "fixture installation must produce a proof-bearing invocation context"
    );
    let _ = read_response_line(&peer, Duration::from_millis(200));

    let mut canonical_tick = crate::runtime::World::load_from_dir(execution_world_dir.as_path())
        .expect("reload authoritative chain world");
    canonical_tick
        .step()
        .expect("advance authoritative chain world");
    canonical_tick
        .save_to_dir_with_chain_resource_context(
            execution_world_dir.as_path(),
            crate::runtime::ChainResourceDerivationContext {
                world_id: VIEWER_FORMAL_RELEASE_DEFAULT_WORLD_ID,
                chain_id: "hosted-local-mock-chain",
                genesis_ref: Some("hosted-local-mock-genesis"),
                created_at_height: 1,
                manifest_height: 2,
                commit_block_hash: Some("hosted-local-mock-block-2"),
                tick: canonical_tick.state().time,
            },
            "hosted-local-mock-world-config",
            "hosted-local-mock-generation",
        )
        .expect("persist next authoritative chain world");
    chain_status.committed_height.store(2, Ordering::SeqCst);
    viewer
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect("subsequent authoritative sync should retain local fixture");
    assert!(
        !viewer.world.capability_invocation_contexts().is_empty(),
        "each authoritative projection must retain a proof-bearing local fixture"
    );
}

#[test]
fn shared_hosted_prime_does_not_starve_v1_presence_hello() {
    let _env_guard = runtime_provider_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _provider_env_snapshot = HostedLocalMockProviderEnvSnapshot::capture();
    configure_hosted_local_mock_provider_env();

    let execution_world_dir = runtime_live_temp_dir("chain_sync_shared_prime_probe");
    let mut execution_world = crate::runtime::World::new_production_hardened();
    execution_world
        .bind_cognition_runtime(
            VIEWER_FORMAL_RELEASE_DEFAULT_WORLD_ID,
            "hosted-local-mock-probe-branch",
            0,
            Some(
                "blake3:1111111111111111111111111111111111111111111111111111111111111111"
                    .to_string(),
            ),
            "verified",
            0,
        )
        .expect("bind probe chain cognition runtime");
    execution_world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "hosted-probe-agent".to_string(),
        pos: crate::geometry::GeoPos::new(1, 2, 0),
    });
    execution_world.step().expect("register probe chain Agent");
    execution_world
        .save_to_dir_with_chain_resource_context(
            execution_world_dir.as_path(),
            crate::runtime::ChainResourceDerivationContext {
                world_id: VIEWER_FORMAL_RELEASE_DEFAULT_WORLD_ID,
                chain_id: "hosted-local-mock-probe-chain",
                genesis_ref: Some("hosted-local-mock-probe-genesis"),
                created_at_height: 1,
                manifest_height: 1,
                commit_block_hash: Some("hosted-local-mock-probe-block-1"),
                tick: execution_world.state().time,
            },
            "hosted-local-mock-probe-config",
            "hosted-local-mock-probe-generation",
        )
        .expect("persist probe chain world");

    let chain_status = TestChainStatusServer::start_with_status_delay(
        execution_world_dir.clone(),
        Duration::from_millis(250),
    );
    chain_status.committed_height.store(1, Ordering::SeqCst);
    let viewer = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::formal_release_default()
            .with_hosted_public_join_mode(true)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone()),
    )
    .expect("empty Hosted local-mock viewer should start before sync");
    let shared = Arc::new(Mutex::new(viewer));
    let prime_shared = Arc::clone(&shared);
    let prime_thread = thread::spawn(move || {
        let request = ViewerRequest::HelloV2 {
            client: "hosted-prime".to_string(),
            version: VIEWER_PROTOCOL_VERSION,
            capabilities: vec![PROMPT_CONTROL_RESULT_CAPABILITY.to_string()],
        };
        let session = RuntimeLiveSession::new();
        ViewerRuntimeLiveServer::prime_shared_request_if_needed(&prime_shared, &request, &session)
    });

    let deadline = Instant::now() + Duration::from_secs(1);
    while chain_status.status_requests() == 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        chain_status.status_requests(),
        1,
        "shared prime should be in its network read before the probe runs"
    );

    let started_at = Instant::now();
    let mut probe_session = RuntimeLiveSession::new();
    let (mut writer, peer) = test_writer_pair();
    shared
        .lock()
        .expect("lock shared viewer while prime reads chain")
        .handle_request(
            ViewerRequest::Hello {
                client: "launcher-presence-probe".to_string(),
                version: VIEWER_PROTOCOL_VERSION,
            },
            &mut probe_session,
            &mut writer,
        )
        .expect("presence Hello must not wait for shared chain prime");
    assert!(
        started_at.elapsed() < Duration::from_millis(150),
        "presence Hello was blocked by the in-flight chain prime: {:?}",
        started_at.elapsed()
    );
    assert!(
        read_available_runtime_live_responses(&peer, Duration::from_millis(100))
            .iter()
            .any(|response| matches!(response, ViewerResponse::HelloAck { .. })),
        "presence probe must receive HelloAck while chain prime is in flight"
    );

    let _ = prime_thread
        .join()
        .expect("shared prime thread should join")
        .expect("authoritative shared prime should succeed")
        .expect("chain status prime should advance the viewer");
}

#[test]
fn hosted_local_mock_chain_sync_rejects_empty_authoritative_world() {
    let _env_guard = runtime_provider_env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _provider_env_snapshot = HostedLocalMockProviderEnvSnapshot::capture();
    configure_hosted_local_mock_provider_env();

    let execution_world_dir = runtime_live_temp_dir("chain_sync_hosted_local_mock_empty");
    let mut execution_world = crate::runtime::World::new_production_hardened();
    execution_world
        .bind_cognition_runtime(
            VIEWER_FORMAL_RELEASE_DEFAULT_WORLD_ID,
            "hosted-local-mock-empty-branch",
            0,
            Some(
                "blake3:1111111111111111111111111111111111111111111111111111111111111111"
                    .to_string(),
            ),
            "verified",
            0,
        )
        .expect("bind empty authoritative chain cognition runtime");
    execution_world
        .save_to_dir_with_chain_resource_context(
            execution_world_dir.as_path(),
            crate::runtime::ChainResourceDerivationContext {
                world_id: VIEWER_FORMAL_RELEASE_DEFAULT_WORLD_ID,
                chain_id: "hosted-local-mock-empty-chain",
                genesis_ref: Some("hosted-local-mock-empty-genesis"),
                created_at_height: 1,
                manifest_height: 1,
                commit_block_hash: Some("hosted-local-mock-empty-block-1"),
                tick: execution_world.state().time,
            },
            "hosted-local-mock-empty-world-config",
            "hosted-local-mock-empty-generation",
        )
        .expect("persist empty authoritative chain world");

    let chain_status = TestChainStatusServer::start(execution_world_dir.clone());
    chain_status.committed_height.store(1, Ordering::SeqCst);
    let mut viewer = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::formal_release_default()
            .with_hosted_public_join_mode(true)
            .with_decision_mode(ViewerLiveDecisionMode::Llm)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone()),
    )
    .expect("empty pre-sync formal world must be allowed to await chain sync");
    let mut session = RuntimeLiveSession::new();
    let (mut writer, _peer) = test_writer_pair();

    let error = viewer
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect_err("empty authoritative Hosted local-mock world must fail closed");
    assert!(
        format!("{error:?}").contains(
            "Hosted local-mock authoritative chain world requires a Runtime-seeded Agent"
        ),
        "unexpected empty authoritative world error: {error:?}"
    );
}

#[test]
fn chain_linked_runtime_sync_advances_without_play() {
    let execution_world_dir = runtime_live_temp_dir("chain_sync_progress");
    let mut execution_world = crate::runtime::World::new_production_hardened();
    execution_world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "chain-agent".to_string(),
        pos: crate::geometry::GeoPos::new(1, 2, 0),
    });
    execution_world.step().expect("advance execution world");
    execution_world.submit_action(RuntimeAction::MoveAgent {
        agent_id: "chain-agent".to_string(),
        to: crate::geometry::GeoPos::new(5, 2, 0),
    });
    execution_world
        .step()
        .expect("advance execution world again");
    execution_world
        .save_to_dir(execution_world_dir.as_path())
        .expect("persist execution world");

    let chain_status = TestChainStatusServer::start(execution_world_dir.clone());
    chain_status.committed_height.store(1, Ordering::SeqCst);

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone())
            .with_chain_poll_interval(Duration::from_millis(50)),
    )
    .expect("runtime server");
    let mut session = RuntimeLiveSession::new();
    session.playing = false;
    session.subscribed.insert(ViewerStream::Events);
    session.subscribed.insert(ViewerStream::Snapshot);
    let (mut writer, peer) = test_writer_pair();

    let progressed = server
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect("chain sync should succeed");

    assert!(progressed, "chain-linked sync should report progress");
    assert_eq!(server.world.state().time, execution_world.state().time);
    let line =
        read_response_line(&peer, Duration::from_millis(200)).expect("expected sync response");
    assert!(!line.trim().is_empty());
}

#[test]
fn chain_linked_provider_authority_is_chain_published_across_tick_and_restart() {
    let execution_world_dir = runtime_live_temp_dir("chain_sync_provider_authority");
    let mut execution_world = crate::runtime::World::new_production_hardened();
    execution_world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "chain-provider-agent".to_string(),
        pos: crate::geometry::GeoPos::new(1, 2, 0),
    });
    execution_world
        .step()
        .expect("register provider chain agent");
    execution_world
        .bind_cognition_runtime(
            "chain-provider-world",
            "main",
            1,
            Some(
                "blake3:1111111111111111111111111111111111111111111111111111111111111111"
                    .to_string(),
            ),
            "verified",
            0,
        )
        .expect("bind provider chain world");
    execution_world
        .install_test_provider_capability_fixture_without_cognition_balance("chain-provider-agent")
        .expect("install provider capability authority");
    let authority = execution_world
        .test_provider_backed_bootstrap_authority(
            "chain-provider-agent",
            "chain-provider-provision-1",
            "chain-provider-authority",
            7,
        )
        .expect("build provider authority bundle");
    // The chain writer has not published the authority yet. The viewer
    // receives the bundle as an admission proof but cannot apply it itself.
    execution_world
        .save_to_dir_with_chain_resource_context(
            execution_world_dir.as_path(),
            crate::chain_resource_schema::ChainResourceDerivationContext {
                world_id: "chain-provider-world",
                chain_id: "runtime-chain",
                genesis_ref: None,
                created_at_height: 1,
                manifest_height: 1,
                commit_block_hash: Some("chain-provider-block-1"),
                tick: execution_world.state().time,
            },
            "chain-provider-world-config",
            "chain-provider-generation",
        )
        .expect("persist chain provider world before authority publication");

    let persisted_before = crate::runtime::World::load_from_dir(execution_world_dir.as_path())
        .expect("load chain-published provider world");
    let economy_before = persisted_before
        .cognition_economy()
        .expect("read persisted provider economy");
    assert!(economy_before.provision_journal.is_empty());

    let chain_status = TestChainStatusServer::start(execution_world_dir.clone());
    chain_status.committed_height.store(1, Ordering::SeqCst);
    let config = ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
        .with_chain_status_bind(chain_status.addr.clone())
        .with_chain_execution_world_dir(execution_world_dir.clone())
        .with_provider_backed_bootstrap_authority(authority.clone());
    let mut viewer = ViewerRuntimeLiveServer::new(config.clone()).expect("start chain viewer");
    let mut session = RuntimeLiveSession::new();
    session.subscribed.insert(ViewerStream::Events);
    let (mut writer, peer) = test_writer_pair();
    let error = viewer
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect_err("viewer must wait for chain-owned authority publication");
    assert!(
        format!("{error:?}").contains("not durably committed"),
        "missing canonical provisioning must fail closed: {error:?}"
    );
    let _ = read_response_line(&peer, Duration::from_millis(50));
    let persisted_after_rejection =
        crate::runtime::World::load_from_dir(execution_world_dir.as_path())
            .expect("reload provider world after rejected viewer sync");
    assert_eq!(
        persisted_after_rejection
            .cognition_economy()
            .expect("read economy after rejected viewer sync"),
        economy_before,
        "a rejected observer sync must not publish a local allowance"
    );

    // Simulate the chain writer's canonical publication. The next viewer poll
    // must retry the same input and observe the durable Runtime transaction.
    let mut chain_writer = persisted_after_rejection;
    chain_writer
        .bootstrap_provider_backed_authority(authority.clone())
        .expect("publish provider authority through Runtime");
    chain_writer
        .save_to_dir_with_chain_resource_context(
            execution_world_dir.as_path(),
            crate::chain_resource_schema::ChainResourceDerivationContext {
                world_id: "chain-provider-world",
                chain_id: "runtime-chain",
                genesis_ref: None,
                created_at_height: 1,
                manifest_height: 1,
                commit_block_hash: Some("chain-provider-block-1"),
                tick: chain_writer.state().time,
            },
            "chain-provider-world-config",
            "chain-provider-generation",
        )
        .expect("persist chain-published provider world");
    let economy_after_publication =
        crate::runtime::World::load_from_dir(execution_world_dir.as_path())
            .expect("reload chain-published provider world")
            .cognition_economy()
            .expect("read published provider economy");
    assert_eq!(economy_after_publication.provision_journal.len(), 1);

    let (mut writer, peer) = test_writer_pair();
    viewer
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect("admit chain-published authority");
    let _ = read_response_line(&peer, Duration::from_millis(200));

    // The loaded execution world is an observer projection. A local provider
    // bookkeeping mutation must not write back into the chain writer's dir.
    viewer
        .world
        .set_cognition_resource_balance("observer-local", "cognition_units", 1)
        .expect("mutate detached observer economy");

    let persisted_after_viewer =
        crate::runtime::World::load_from_dir(execution_world_dir.as_path())
            .expect("reload provider world after viewer sync");
    assert_eq!(
        persisted_after_viewer
            .cognition_economy()
            .expect("read economy after viewer sync"),
        economy_after_publication,
        "observer bootstrap must not mutate the chain writer's canonical world"
    );

    // A later canonical chain tick must retain the original one-time
    // provisioning record and allowance. This is the durable handoff the
    // viewer observes on every subsequent poll.
    // Preserve the locally admitted provider lease across a newer chain poll
    // while its outcome is pending. The poll must not replace the Runtime
    // projection with a chain snapshot that lacks that exact lease.
    let binding = viewer
        .world
        .current_cognition_runtime_binding()
        .expect("viewer Runtime binding after authority sync");
    let provider_lease = viewer
        .world
        .reserve_cognition_lease(crate::runtime::CognitionLeaseRequestV1::new(
            "chain-viewer-provider-continuity",
            authority.owner_binding.clone(),
            authority.agent_id.clone(),
            "chain-viewer-session",
            "chain-viewer-turn",
            "chain-viewer-request",
            "chain-viewer-request-digest",
            crate::runtime::CognitionLeaseQuoteV1::new(
                "chain-viewer-provider-quote",
                "cognition_units",
                1,
            )
            .with_authority(
                authority.owner_binding.clone(),
                crate::runtime::COGNITION_RESOURCE_VERSION_V1,
                "provider_cognition",
                "agent_turn",
                crate::runtime::COGNITION_FIXED_UNIT_EXPERIMENTAL_POLICY_REVISION,
                "chain-provider-authority",
                binding.base_world_hash.to_string(),
            ),
        ))
        .expect("reserve viewer provider lease for continuity test");
    viewer
        .llm_sidecar
        .bind_provider_cognition_lease(authority.agent_id.clone(), provider_lease.clone());

    let mut chain_tick = persisted_after_viewer.clone();
    chain_tick.step().expect("advance canonical chain tick");
    chain_tick
        .save_to_dir_with_chain_resource_context(
            execution_world_dir.as_path(),
            crate::chain_resource_schema::ChainResourceDerivationContext {
                world_id: "chain-provider-world",
                chain_id: "runtime-chain",
                genesis_ref: None,
                created_at_height: 1,
                manifest_height: 2,
                commit_block_hash: Some("chain-provider-block-2"),
                tick: chain_tick.state().time,
            },
            "chain-provider-world-config",
            "chain-provider-generation",
        )
        .expect("persist canonical chain tick");
    chain_status.committed_height.store(2, Ordering::SeqCst);
    let (mut writer, peer) = test_writer_pair();
    let deferred = viewer
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect("defer chain tick while provider lease is in flight");
    assert!(
        !deferred,
        "in-flight provider lease must retain its Runtime projection"
    );
    assert_eq!(
        viewer.world.state().time,
        persisted_after_viewer.state().time,
        "deferred chain sync must not replace the in-flight provider world"
    );
    let _ = read_response_line(&peer, Duration::from_millis(50));
    viewer
        .world
        .release_cognition_lease(provider_lease.lease_id.as_str())
        .expect("release continuity test lease");
    viewer
        .llm_sidecar
        .clear_provider_cognition_lease(authority.agent_id.as_str());
    let (mut writer, peer) = test_writer_pair();
    viewer
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect("admit authority on later chain tick");
    let _ = read_response_line(&peer, Duration::from_millis(200));
    let persisted_after_tick = crate::runtime::World::load_from_dir(execution_world_dir.as_path())
        .expect("reload canonical chain tick");
    assert_eq!(
        persisted_after_tick
            .cognition_economy()
            .expect("read economy after chain tick"),
        economy_after_publication,
        "a later chain tick must not refill or rewrite the one-time allowance"
    );

    drop(viewer);
    let mut restarted_viewer = ViewerRuntimeLiveServer::new(config).expect("restart chain viewer");
    let mut restart_session = RuntimeLiveSession::new();
    restart_session.subscribed.insert(ViewerStream::Events);
    let (mut writer, peer) = test_writer_pair();
    restarted_viewer
        .sync_chain_linked_runtime(&mut restart_session, &mut writer)
        .expect("replay chain-published authority after viewer restart");
    let _ = read_response_line(&peer, Duration::from_millis(200));
    let persisted_after_restart =
        crate::runtime::World::load_from_dir(execution_world_dir.as_path())
            .expect("reload canonical world after viewer restart");
    assert_eq!(
        persisted_after_restart
            .cognition_economy()
            .expect("read economy after viewer restart"),
        economy_after_publication,
        "viewer restart must not refill or rewrite canonical provisioning"
    );
}

#[test]
fn chain_linked_runtime_primes_initial_snapshot() {
    let execution_world_dir = runtime_live_temp_dir("chain_sync_initial_snapshot");
    let mut execution_world = crate::runtime::World::new_production_hardened();
    execution_world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "chain-agent".to_string(),
        pos: crate::geometry::GeoPos::new(1, 2, 0),
    });
    execution_world.step().expect("advance execution world");
    execution_world
        .save_to_dir_with_chain_resource_context(
            execution_world_dir.as_path(),
            crate::runtime::ChainResourceDerivationContext {
                world_id: "testnet-world",
                chain_id: "testnet-chain",
                genesis_ref: Some("testnet-genesis"),
                created_at_height: 1,
                manifest_height: 1,
                commit_block_hash: Some("testnet-block-1"),
                tick: execution_world.state().time,
            },
            "testnet-world-config",
            "testnet-generation-algorithm",
        )
        .expect("persist execution world");

    let chain_status = TestChainStatusServer::start(execution_world_dir.clone());
    chain_status.committed_height.store(1, Ordering::SeqCst);

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone())
            .with_chain_poll_interval(Duration::from_millis(50)),
    )
    .expect("runtime server");
    let mut session = RuntimeLiveSession::new();
    let (mut writer, peer) = test_writer_pair();

    server
        .handle_request(ViewerRequest::RequestSnapshot, &mut session, &mut writer)
        .expect("request snapshot");
    writer.flush().expect("flush snapshot");

    assert_eq!(server.world.state().time, execution_world.state().time);
    let responses = read_available_runtime_live_responses(&peer, Duration::from_millis(200));
    let snapshot = responses
        .iter()
        .find_map(|response| match response {
            ViewerResponse::Snapshot { snapshot } => Some(snapshot),
            _ => None,
        })
        .expect("initial snapshot response");
    assert_eq!(snapshot.time, execution_world.state().time);
    assert_eq!(snapshot.chain_resource_manifest.world_id, "testnet-world");
    assert_eq!(snapshot.chain_resource_manifest.chain_id, "testnet-chain");
    assert_eq!(
        snapshot.latest_chain_resource_delta.world_id,
        "testnet-world"
    );
    assert_eq!(
        snapshot.latest_chain_resource_delta.chain_id,
        "testnet-chain"
    );
    let runtime_snapshot = snapshot
        .runtime_snapshot
        .as_ref()
        .expect("runtime snapshot should be embedded");
    assert_eq!(
        runtime_snapshot.chain_resource_manifest.world_id,
        snapshot.chain_resource_manifest.world_id
    );
    assert_eq!(
        runtime_snapshot.chain_resource_manifest.chain_id,
        snapshot.chain_resource_manifest.chain_id
    );
    assert!(
        snapshot
            .model
            .agents
            .iter()
            .any(|(_, agent)| agent.id == "chain-agent")
    );
}

#[test]
fn chain_linked_runtime_enforcing_rejects_initial_snapshot_when_prime_fails() {
    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_chain_status_bind("127.0.0.1:1")
            .with_chain_link_policy(ChainLinkPolicy::Enforcing)
            .with_chain_poll_interval(Duration::from_millis(50)),
    )
    .expect("runtime server");
    let mut session = RuntimeLiveSession::new();
    let (mut writer, peer) = test_writer_pair();

    server
        .handle_request(ViewerRequest::RequestSnapshot, &mut session, &mut writer)
        .expect_err("enforcing initial snapshot should reject chain prime failure");
    writer.flush().expect("flush writer");

    let responses = read_available_runtime_live_responses(&peer, Duration::from_millis(50));
    assert!(
        responses.is_empty(),
        "enforcing chain prime failure must not emit a fallback snapshot"
    );
    assert!(
        !session.initial_snapshot_sent,
        "failed enforcing prime must not mark initial snapshot sent"
    );
}

#[test]
fn chain_linked_runtime_sync_rejects_same_watermark_snapshot_rebuild() {
    let execution_world_dir = runtime_live_temp_dir("chain_sync_same_watermark_rebuild");
    let mut first_world = crate::runtime::World::new_production_hardened();
    first_world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "first-agent".to_string(),
        pos: crate::geometry::GeoPos::new(1, 2, 0),
    });
    first_world.step().expect("advance first execution world");
    first_world
        .save_to_dir(execution_world_dir.as_path())
        .expect("persist first execution world");

    let chain_status = TestChainStatusServer::start(execution_world_dir.clone());
    chain_status.committed_height.store(1, Ordering::SeqCst);

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone())
            .with_chain_poll_interval(Duration::from_millis(50)),
    )
    .expect("runtime server");
    let mut session = RuntimeLiveSession::new();
    session.playing = false;
    session.subscribed.insert(ViewerStream::Events);
    session.subscribed.insert(ViewerStream::Snapshot);
    let (mut writer, peer) = test_writer_pair();

    let progressed = server
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect("first chain sync should succeed");
    assert!(progressed);
    assert!(server.world.state().agents.contains_key("first-agent"));
    assert!(read_response_line(&peer, Duration::from_millis(200)).is_some());

    let mut rebuilt_world = crate::runtime::World::new_production_hardened();
    rebuilt_world.submit_action(RuntimeAction::RegisterAgent {
        agent_id: "rebuilt-agent".to_string(),
        pos: crate::geometry::GeoPos::new(9, 2, 0),
    });
    rebuilt_world
        .step()
        .expect("advance rebuilt execution world");
    assert_eq!(rebuilt_world.state().time, first_world.state().time);
    assert_eq!(
        latest_runtime_event_seq(&rebuilt_world),
        latest_runtime_event_seq(&first_world)
    );
    rebuilt_world
        .save_to_dir(execution_world_dir.as_path())
        .expect("replace execution world with same-watermark rebuilt world");
    server
        .llm_sidecar
        .agent_player_bindings
        .insert("starter-agent-0".into(), "local-test-player-old".into());

    let (mut writer, peer) = test_writer_pair();
    let error = server
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect_err("different authority at the same watermark must fail closed");
    assert!(
        matches!(error, ViewerRuntimeLiveServerError::Init(message) if message.contains("different observer authority"))
    );
    assert!(server.world.state().agents.contains_key("first-agent"));
    assert!(!server.world.state().agents.contains_key("rebuilt-agent"));
    assert_eq!(
        server
            .llm_sidecar
            .agent_player_bindings
            .get("starter-agent-0"),
        Some(&"local-test-player-old".to_string()),
        "rejected checkpoint must not prune sidecar state"
    );
    assert!(read_response_line(&peer, Duration::from_millis(100)).is_none());
}

#[test]
fn chain_linked_runtime_sync_clears_stale_local_test_sidecar_binding() {
    let execution_world_dir = runtime_live_temp_dir("chain_sync_stale_local_test_binding");
    let execution_world = crate::runtime::World::new_production_hardened();
    execution_world
        .save_to_dir(execution_world_dir.as_path())
        .expect("persist empty execution world");

    let chain_status = TestChainStatusServer::start(execution_world_dir.clone());
    chain_status.committed_height.store(1, Ordering::SeqCst);

    let mut server = ViewerRuntimeLiveServer::new(
        ViewerRuntimeLiveServerConfig::new(WorldScenario::Minimal)
            .with_chain_status_bind(chain_status.addr.clone())
            .with_chain_execution_world_dir(execution_world_dir.clone())
            .with_chain_poll_interval(Duration::from_millis(50)),
    )
    .expect("runtime server");
    server.llm_sidecar.agent_player_bindings.insert(
        "starter-agent-0".to_string(),
        "local-test-player-old".to_string(),
    );
    server.llm_sidecar.player_agent_bindings.insert(
        "local-test-player-old".to_string(),
        "starter-agent-0".to_string(),
    );
    server
        .llm_sidecar
        .agent_public_key_bindings
        .insert("starter-agent-0".to_string(), "old-key".to_string());
    server
        .llm_sidecar
        .agent_player_bindings
        .insert("agent-real".to_string(), "player-real".to_string());
    server
        .llm_sidecar
        .player_agent_bindings
        .insert("player-real".to_string(), "agent-real".to_string());

    let published_time = execution_world.state().time;
    let published_events = execution_world.journal().events.clone();
    let mut session = RuntimeLiveSession::new();
    let (mut writer, _peer) = test_writer_pair();

    let progressed = server
        .sync_chain_linked_runtime(&mut session, &mut writer)
        .expect("chain sync should succeed");

    assert!(
        progressed,
        "new commit advances identity while pruning stale bindings"
    );
    assert_eq!(server.world.state().time, published_time);
    assert_eq!(server.world.journal().events, published_events);
    assert_eq!(server.last_chain_committed_height, 1);
    assert!(
        !server
            .sync_chain_linked_runtime(&mut session, &mut writer)
            .expect("same publication idle")
    );
    assert_eq!(server.world.state().time, published_time);
    assert_eq!(server.world.journal().events, published_events);
    assert_eq!(server.last_chain_committed_height, 1);
    assert_eq!(
        server
            .llm_sidecar
            .agent_player_bindings
            .get("starter-agent-0"),
        None
    );
    assert_eq!(
        server
            .llm_sidecar
            .player_agent_bindings
            .get("local-test-player-old"),
        None
    );
    assert_eq!(
        server
            .llm_sidecar
            .agent_public_key_bindings
            .get("starter-agent-0"),
        None
    );
    assert_eq!(
        server
            .llm_sidecar
            .agent_player_bindings
            .get("agent-real")
            .map(String::as_str),
        Some("player-real")
    );
}
