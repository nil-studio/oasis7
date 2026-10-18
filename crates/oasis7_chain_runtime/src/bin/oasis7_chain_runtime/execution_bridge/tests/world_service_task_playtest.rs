//! Explicit prepared-world live service for independent formal-Skill gameplay.
use super::*;
use oasis7::world_service::{
    client::{RemoteWorldServiceClient, WorldServiceClientConfig, WorldServicePort},
    private_goal_prepare::sign_goal_prepare,
    private_goal_read::publication,
    private_goal_store::PrivateGoalStore,
};
use oasis7_agent_api::game::{
    DecimalU64, GAME_EXECUTOR_CONTROL_DOMAIN, GAME_GOAL_PUBLISH_DOMAIN, GameExecutorControlV1,
    GameExecutorOperationV1, GameGoalPrepareV1, GameTaskFenceV1, GoalContentV1,
    game_goal_version_digest,
};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
// These integration probes each run a real ticking node, HTTP listener and
// fsync/CAS retention workload. Keep their fixture hosts from competing with
// each other for the runner's CPU and disk inside a wall-clock gameplay budget.
// Acquire before creating the fixture/deadline; retain every scenario and bound.
fn prepared_world_probe_slot() -> std::sync::MutexGuard<'static, ()> {
    static SLOT: Mutex<()> = Mutex::new(());
    SLOT.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
fn write_private(path: &Path, bytes: &[u8]) {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
}
fn write_public_json(path: &Path, value: &serde_json::Value) {
    let temporary = path.with_extension("pending.json");
    fs::write(&temporary, serde_json::to_vec(value).unwrap()).unwrap();
    fs::rename(temporary, path).unwrap();
}
// Scope waits for child threads while unwinding; the listener must see stop first.
struct ListenerStopOnDrop<'a>(&'a AtomicBool);
impl Drop for ListenerStopOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
struct NodeStopOnDrop(Arc<Mutex<oasis7_node::NodeRuntime>>);
impl Drop for NodeStopOnDrop {
    fn drop(&mut self) {
        let mut node = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = node.stop();
    }
}
#[derive(Clone, Copy, Default, serde::Serialize)]
struct CleanupCategories {
    submit_response: u64,
    submit_unknown: u64,
    submit_transport: u64,
    submit_http_503: u64,
    lookup_unknown: u64,
    lookup_pending: u64,
    lookup_received: u64,
    lookup_history_unavailable: u64,
    lookup_committed: u64,
    lookup_transport: u64,
    cooldown: u64,
}
fn report_cleanup_boundary(
    phase: &'static str,
    error: &oasis7::world_service::client::WorldServiceClientError,
    categories: &CleanupCategories,
) {
    use oasis7::world_service::client::WorldServiceClientError;
    let (class, status) = match error {
        WorldServiceClientError::Configuration(_) => ("configuration", None),
        WorldServiceClientError::Transport(_) => ("transport", None),
        WorldServiceClientError::Rejected { status, .. } => ("rejected", Some(*status)),
        WorldServiceClientError::Assurance(_) => ("assurance", None),
        WorldServiceClientError::Cooldown { status, .. } => ("cooldown", Some(*status)),
    };
    // Only fixed classes, HTTP status and counters: never private payload or raw errors.
    eprintln!(
        "TASK_CLEANUP_BOUNDARY {}",
        serde_json::json!({"phase":phase,"class":class,"status":status,"categories":categories})
    );
}
fn settle_cleanup_original(
    client: &RemoteWorldServiceClient,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
    until: Instant,
    categories: &mut CleanupCategories,
) -> Result<serde_json::Value, &'static str> {
    let mut submitted = false;
    let mut next_submit = Instant::now();
    while Instant::now() < until {
        if !submitted && Instant::now() >= next_submit {
            // Retry the retained signed original only; uncertain delivery is not admission.
            next_submit = Instant::now() + Duration::from_secs(1);
            match client.submit(original.clone()) {
                Ok(SubmitObservation::Response(_)) => {
                    categories.submit_response += 1;
                    submitted = true;
                }
                Ok(SubmitObservation::OutcomeUnknown(_)) => {
                    categories.submit_unknown += 1;
                }
                Err(oasis7::world_service::client::WorldServiceClientError::Transport(_)) => {
                    categories.submit_transport += 1
                }
                Err(oasis7::world_service::client::WorldServiceClientError::Cooldown {
                    ..
                }) => categories.cooldown += 1,
                Err(oasis7::world_service::client::WorldServiceClientError::Rejected {
                    status: 503,
                    ..
                }) => {
                    categories.submit_http_503 += 1;
                    // The unsigned service boundary does not establish whether this exact
                    // original was admitted. Keep its identity and query its canonical result.
                }
                Err(error) => {
                    report_cleanup_boundary("submit", &error, categories);
                    return Err("cleanup submit boundary rejected");
                }
            }
        }
        match client.lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: original.correlation.key.clone(),
            },
            original.signed_payload.clone(),
        ) {
            Ok(result) => match result.outcome {
                IntentOutcome::Committed { .. } => {
                    categories.lookup_committed += 1;
                    return serde_json::to_value(result)
                        .map_err(|_| "cleanup receipt encoding failed");
                }
                IntentOutcome::Rejected { .. } | IntentOutcome::Expired => {
                    return Err("canonical cleanup rejected");
                }
                IntentOutcome::Unknown => {
                    categories.lookup_unknown += 1;
                    submitted = false;
                }
                IntentOutcome::Pending => categories.lookup_pending += 1,
                IntentOutcome::Received { .. } => categories.lookup_received += 1,
                IntentOutcome::HistoryUnavailable => categories.lookup_history_unavailable += 1,
            },
            Err(oasis7::world_service::client::WorldServiceClientError::Transport(_)) => {
                categories.lookup_transport += 1
            }
            Err(oasis7::world_service::client::WorldServiceClientError::Cooldown { .. }) => {
                categories.cooldown += 1
            }
            Err(error) => {
                report_cleanup_boundary("lookup", &error, categories);
                return Err("cleanup lookup boundary rejected");
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    eprintln!(
        "TASK_CLEANUP_DEADLINE {}",
        serde_json::json!({"categories":categories})
    );
    Err("cleanup original deadline exceeded")
}
// Only an exact, still-live terminal stale decision can omit the redundant
// public abort. Other rejected operations retain their existing cleanup lane.
fn verified_terminal_stale_action(
    client: &RemoteWorldServiceClient,
    world: &RuntimeWorld,
    identity: &oasis7_agent_api::game::TaskContextIdentityV1,
    original: &SubmitIntentRequest<WorldServicePayloadV1>,
) -> bool {
    let canonical = world
        .capability_revocation_state()
        .world_service_results
        .values()
        .filter_map(|value| serde_json::from_value::<CanonicalIntentResultV1>(value.clone()).ok())
        .filter(|result| result.request.correlation == original.correlation)
        .collect::<Vec<_>>();
    assert_eq!(
        canonical.len(),
        1,
        "original rejection must have one canonical result"
    );
    assert!(
        serde_json::to_value(&canonical[0].request).unwrap()
            == serde_json::to_value(original).unwrap(),
        "canonical rejection must retain the entire signed original"
    );
    let stale = format!(
        "{:?}",
        oasis7_runtime::WorldError::DistributedValidationFailed {
            reason: "stale_base".into()
        }
    );
    if canonical[0].rejected.as_deref() != Some(stale.as_str()) {
        return false;
    }
    let WorldServicePayloadV1::Cognition(signed) = &original.signed_payload else {
        panic!("terminal stale proof requires cognition");
    };
    assert!(
        original.correlation
            == correlation::derive_correlation(
                original.correlation.key.world.clone(),
                &original.signed_payload
            )
            .unwrap()
    );
    assert!(signed.request.task_fence.as_ref() == Some(&identity.fence));
    assert!(
        world.state().time <= identity.valid_until_tick.0,
        "terminal proof must not rely on context expiry"
    );
    let control =
        oasis7::world_service::game_task::current_control(world, &identity.fence.agent_id)
            .unwrap()
            .unwrap();
    assert!(
        GameTaskFenceV1::from_control(&control) == identity.fence,
        "terminal proof must not rely on a changed fence"
    );
    let replay = client
        .lookup(
            LookupIntentRequest {
                contract_version: 1,
                key: original.correlation.key.clone(),
            },
            original.signed_payload.clone(),
        )
        .unwrap_or_else(|_| panic!("exact rejected original lookup must verify"));
    assert!(
        matches!(
            replay.outcome,
            IntentOutcome::Rejected {
                reason: oasis7_client_api::world_service::WorldServiceErrorKind::Conflict
            }
        ),
        "exact original must retain its terminal conflict result"
    );
    let view = client
        .read_view(ReadWorldViewRequest {
            contract_version: 1,
            world: original.correlation.key.world.clone(),
            scope_id: format!("agent:{}", identity.fence.agent_id),
            observation_nonce: None,
            formal_anchor: None,
            min_commit: None,
            fixed_commit: None,
            deadline_unix_ms: None,
        })
        .unwrap_or_else(|_| panic!("terminal stale feedback requires a verified scoped read"));
    assert!(
        view.projection()
            .game_task_control
            .as_ref()
            .is_some_and(|control| GameTaskFenceV1::from_control(control) == identity.fence),
        "verified feedback must retain the current original fence"
    );
    assert!(
        view.projection().state.time <= identity.valid_until_tick.0,
        "verified feedback must arrive within the original context TTL"
    );
    let history = view
        .projection()
        .feedback_history
        .as_ref()
        .expect("scoped feedback history required");
    let r = &signed.request.request;
    let matching = history
        .records
        .iter()
        .filter(|record| {
            let f = &record.feedback;
            f.agent_subject == r.agent_id
                && f.agent_session_id == r.agent_session_id
                && f.agent_turn_id == r.agent_turn_id
                && f.decision_request_id == r.decision_request_id
                && f.request_digest.to_string() == r.request_digest
        })
        .collect::<Vec<_>>();
    assert_eq!(matching.len(), 1, "exact terminal stale feedback required");
    let f = &matching[0].feedback;
    assert!(
        f.status == "rejected"
            && f.reject_reason.as_deref() == Some("stale_base")
            && f.provenance == "runtime_authoritative"
            && f.candidate_action_id.is_none()
            && f.runtime_receipt_id.is_none()
            && matching[0].receipt_lineage.is_none(),
        "terminal stale feedback disposition must verify"
    );
    assert!(
        oasis7::world_service::game_task::validate_task_context_capture(
            world,
            identity,
            &signed.subject_public_key
        )
        .is_err(),
        "still-live old context must already be terminal before fresh preparation"
    );
    true
}

#[test]
#[ignore = "explicit 360-second prepared-world service; independent player uses shipped task Skill"]
fn private_task_prepared_world_live_service() {
    let ready = std::path::PathBuf::from(
        std::env::var_os("OASIS7_TASK_PLAYTEST_READY").expect("explicit ready path required"),
    );
    run_prepared_world_service(
        ready,
        Duration::from_secs(5),
        false,
        None,
        false,
        Duration::from_secs(30),
    );
}
fn run_prepared_world_service(
    ready: std::path::PathBuf,
    tick_interval: Duration,
    immediate_cleanup: bool,
    cleanup_after_height: Option<u64>,
    drop_first_cleanup_submit: bool,
    cleanup_budget: Duration,
) -> std::path::PathBuf {
    run_prepared_world_service_scenario(
        ready,
        tick_interval,
        immediate_cleanup,
        cleanup_after_height,
        drop_first_cleanup_submit,
        cleanup_budget,
        false,
        Duration::ZERO,
    )
}
#[expect(
    clippy::too_many_arguments,
    reason = "Explicit isolated scenario and ordinary fault timing controls"
)]
fn run_prepared_world_service_scenario(
    ready: std::path::PathBuf,
    tick_interval: Duration,
    immediate_cleanup: bool,
    cleanup_after_height: Option<u64>,
    drop_first_cleanup_submit: bool,
    cleanup_budget: Duration,
    logistics: bool,
    cleanup_admission_delay: Duration,
) -> std::path::PathBuf {
    run_prepared_world_service_kind(
        ready,
        tick_interval,
        immediate_cleanup,
        cleanup_after_height,
        drop_first_cleanup_submit,
        cleanup_budget,
        if logistics {
            PreparedWorldKind::Logistics
        } else {
            PreparedWorldKind::Movement
        },
        cleanup_admission_delay,
    )
}
#[expect(
    clippy::too_many_arguments,
    reason = "Explicit isolated prepared world and fault controls"
)]
fn run_prepared_world_service_kind(
    ready: std::path::PathBuf,
    tick_interval: Duration,
    immediate_cleanup: bool,
    cleanup_after_height: Option<u64>,
    drop_first_cleanup_submit: bool,
    cleanup_budget: Duration,
    kind: PreparedWorldKind,
    cleanup_admission_delay: Duration,
) -> std::path::PathBuf {
    run_prepared_world_service_kind_with_cognition_503(
        ready,
        tick_interval,
        immediate_cleanup,
        cleanup_after_height,
        drop_first_cleanup_submit,
        cleanup_budget,
        kind,
        cleanup_admission_delay,
        false,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Explicit isolated prepared world and fault controls"
)]
fn run_prepared_world_service_kind_with_cognition_503(
    ready: std::path::PathBuf,
    tick_interval: Duration,
    immediate_cleanup: bool,
    cleanup_after_height: Option<u64>,
    drop_first_cleanup_submit: bool,
    cleanup_budget: Duration,
    kind: PreparedWorldKind,
    cleanup_admission_delay: Duration,
    reject_first_cognition_submit_503: bool,
) -> std::path::PathBuf {
    assert!(ready.is_absolute());
    assert!(!ready.exists());
    let logistics = matches!(kind, PreparedWorldKind::Logistics);
    let production = matches!(kind, PreparedWorldKind::Production);
    let starter_chain = matches!(kind, PreparedWorldKind::StarterChain);
    let (dir, mut driver, world, owner, owner_public) = if starter_chain {
        production_playtest::fixture_kind(true)
    } else if production {
        production_playtest::fixture()
    } else if logistics {
        logistics_playtest::fixture()
    } else {
        canonical_agent_chat::driver_fixture_with_verified_authority()
    };
    let create = game_task_tests::create(&world, &owner, "context-task");
    commit(&mut driver, 3, create.clone());
    let c = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    let private = dir.canonicalize().unwrap().join("private-goals");
    let store = PrivateGoalStore::open(&private, [7; 32]).unwrap();
    let content = if starter_chain {
        starter_chain_playtest::goal()
    } else if production {
        production_playtest::goal()
    } else if logistics {
        logistics_playtest::goal()
    } else {
        GoalContentV1 {
            goal_text: format!(
                "Reach the landmark at coordinates (200, 0, 0) centimeters. Read the complete goal and choose a valid movement action yourself. {}",
                "The task goal is delivered privately without truncation.".repeat(24)
            ),
            soft_constraints: vec![
                "Use the currently bound executor and canonical signed task action lane.".into(),
            ],
        }
    };
    let goal = sign_goal_prepare(
        GameGoalPrepareV1 {
            api_rev: 1,
            world_id: world.world_id.clone(),
            genesis_digest: world.genesis_digest.clone(),
            task_id: c.task_id.clone(),
            agent_id: c.agent_id.clone(),
            owner_public_key: owner_public.clone(),
            owner_binding: c.owner_binding.clone(),
            owner_generation: c.owner_generation,
            branch_id: c.branch_id.clone(),
            reorg_epoch: c.reorg_epoch,
            expected_task_revision: c.task_revision,
            expected_latest_goal_revision: c.latest_goal_revision,
            goal_revision: DecimalU64(1),
            client_operation_id: "context-goal".into(),
            client_nonce: "ab".repeat(32),
            goal_digest: game_goal_version_digest(&c.task_id, DecimalU64(1), &content).unwrap(),
            content,
        },
        &owner,
    )
    .unwrap();
    let receipt = store.prepare(&goal).unwrap();
    let publish = publication(&goal, &receipt, "context-publish".into()).unwrap();
    commit(
        &mut driver,
        4,
        request(
            &world,
            WorldServicePayloadV1::GameGoalPublish(
                sign_read_request(GAME_GOAL_PUBLISH_DOMAIN, publish, &owner).unwrap(),
            ),
        ),
    );
    let delegate = hex::encode([73; 32]);
    let delegate_public = sign_read_request("key", (), &delegate)
        .unwrap()
        .subject_public_key;
    commit(
        &mut driver,
        5,
        request(
            &world,
            WorldServicePayloadV1::Delegation(
                sign_read_request(
                    "delegation",
                    AgentSignerDelegationChangeV1 {
                        world_id: world.world_id.clone(),
                        branch_id: "main".into(),
                        agent_id: "agent-a".into(),
                        owner_binding: "owner-a".into(),
                        agent_identity_generation: 1,
                        delegate_public_key: delegate_public.clone(),
                        generation: 1,
                        revoked: false,
                        nonce: 1,
                    },
                    &owner,
                )
                .unwrap(),
            ),
        ),
    );
    let c = game_task::current_control(&driver.execution_world, "agent-a")
        .unwrap()
        .unwrap();
    commit(
        &mut driver,
        6,
        request(
            &world,
            WorldServicePayloadV1::GameExecutorControl(
                sign_read_request(
                    GAME_EXECUTOR_CONTROL_DOMAIN,
                    GameExecutorControlV1 {
                        api_rev: 1,
                        client_operation_id: "bind-context".into(),
                        fence: GameTaskFenceV1::from_control(&c),
                        operation: GameExecutorOperationV1::Bind {
                            executor_id: "context-executor".into(),
                            executor_public_key: delegate_public,
                            delegation_generation: DecimalU64(1),
                        },
                    },
                    &owner,
                )
                .unwrap(),
            ),
        ),
    );

    let root = dir.canonicalize().unwrap();
    drop(driver);
    let _lock =
        crate::world_writer_lock::acquire_live_world_writer_lock(&root.join("world")).unwrap();
    let bootstrap = super::super::super::local_bootstrap::derive_service_execution_bootstrap(
        &root.join("world"),
        &root.join("records"),
        &root.join("store"),
        "w1",
    )
    .unwrap();
    let driver = NodeRuntimeExecutionDriver::new_with_local_bootstrap(
        root.join("state.json"),
        root.join("world"),
        root.join("records"),
        root.join("store"),
        &oasis7_proto::storage_profile::StorageProfileConfig::default(),
        bootstrap.clone(),
    )
    .unwrap();
    let config = oasis7_node::NodeConfig::new("node-a", "w1", NodeRole::Sequencer)
        .unwrap()
        .with_tick_interval(tick_interval)
        .unwrap();
    let mut node = oasis7_node::NodeRuntime::new(config)
        .with_execution_hook(driver)
        .with_local_execution_bootstrap(bootstrap);
    node.start().unwrap();
    let node = Arc::new(Mutex::new(node));
    let _node_stop = NodeStopOnDrop(node.clone());
    let service_key = hex::encode([9; 32]);
    let service_public = sign_read_request("service", (), &service_key)
        .unwrap()
        .subject_public_key;
    let signer = crate::feedback_submit_api::FeedbackSubmitSigner {
        private_key_hex: service_key,
        public_key_hex: service_public.clone(),
    };
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let device = root.join("client-device");
    fs::create_dir(&device).unwrap();
    fs::set_permissions(&device, fs::Permissions::from_mode(0o700)).unwrap();
    let state = device.join("state");
    fs::create_dir(&state).unwrap();
    fs::set_permissions(&state, fs::Permissions::from_mode(0o700)).unwrap();
    let signer_file = device.join("executor.key");
    write_private(&signer_file, &hex::decode(&delegate).unwrap());
    let config_path = device.join("client.json");
    write_private(&config_path,&serde_json::to_vec(&serde_json::json!({"endpoint":endpoint,"world_id":world.world_id,"genesis_digest":world.genesis_digest,"agent_id":"agent-a","service_public_key":service_public,"signer_file":signer_file,"state_directory":state})).unwrap());
    let owner_client = RemoteWorldServiceClient::new(WorldServiceClientConfig {
        endpoint: endpoint.clone(),
        trusted_service_public_key: signer.public_key_hex.clone(),
        expected_world: world.clone(),
        scope_id: "agent:agent-a".into(),
        read_private_key_hex: owner.clone(),
        timeout: Duration::from_secs(5),
        formal_read_trust: None,
        max_response_bytes: 2 * 1024 * 1024,
    })
    .unwrap();
    let stop = AtomicBool::new(false);
    let first_cleanup_drop = AtomicBool::new(drop_first_cleanup_submit);
    let first_cleanup_slow = AtomicBool::new(!cleanup_admission_delay.is_zero());
    let first_cognition_submit_503 = AtomicBool::new(reject_first_cognition_submit_503);
    let injected_cognition_submit_503 = AtomicBool::new(false);
    let exact_cognition_retry_seen = AtomicBool::new(false);
    let original_cognition_submit_body = Mutex::new(None::<Vec<u8>>);
    std::thread::scope(|scope| {
        let _listener_stop = ListenerStopOnDrop(&stop);
        let server = scope.spawn(|| {
            while !stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // Darwin inherits the listener's O_NONBLOCK on accept.
                        // HTTP reads below use blocking read_exact with the
                        // existing timeout; a fragmented header/body must not
                        // be mistaken for a disconnected client on WouldBlock.
                        // Match the production status server's stream setup.
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(5)))
                            .unwrap();
                        let mut bytes = Vec::new();
                        while !bytes.ends_with(b"\r\n\r\n") {
                            let mut byte = [0; 1];
                            if stream.read_exact(&mut byte).is_err() {
                                break;
                            }
                            bytes.push(byte[0]);
                            if bytes.len() > 65536 {
                                break;
                            }
                        }
                        let Ok(headers) = std::str::from_utf8(&bytes) else {
                            continue;
                        };
                        let mut first = headers.split_whitespace();
                        let Some(method) = first.next().map(str::to_owned) else {
                            continue;
                        };
                        let Some(path) = first.next().map(str::to_owned) else {
                            continue;
                        };
                        let route = task_private_route_tag(&path);
                        if route != "other" {
                            eprintln!(
                                "TASK_PRIVATE_FIXTURE fixture={} route={route} outcome=accepted",
                                kind.tag()
                            );
                        }
                        let length = headers
                            .split("\r\n")
                            .filter_map(|line| line.split_once(':'))
                            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                            .and_then(|(_, v)| v.trim().parse::<usize>().ok())
                            .unwrap_or(0);
                        if length > 256 * 1024 {
                            continue;
                        }
                        let mut body = vec![0; length];
                        if stream.read_exact(&mut body).is_err() {
                            continue;
                        }
                        let is_cognition_submit = method == "POST"
                            && path == SUBMIT_PATH
                            && serde_json::from_slice::<SubmitIntentRequest<WorldServicePayloadV1>>(
                                &body,
                            )
                            .is_ok_and(|request| {
                                matches!(request.signed_payload, WorldServicePayloadV1::Cognition(_))
                            });
                        if is_cognition_submit
                            && first_cognition_submit_503.swap(false, Ordering::SeqCst)
                        {
                            *original_cognition_submit_body
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                Some(body);
                            injected_cognition_submit_503.store(true, Ordering::SeqCst);
                            eprintln!("TASK_CLEANUP_FAULT injected_cognition_submit_503=1");
                            write!(
                                stream,
                                "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            )
                            .unwrap();
                            continue;
                        }
                        if is_cognition_submit
                            && reject_first_cognition_submit_503
                            && injected_cognition_submit_503.load(Ordering::SeqCst)
                            && !exact_cognition_retry_seen.load(Ordering::SeqCst)
                        {
                            let original = original_cognition_submit_body
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            if let Some(original) = original.as_ref() {
                                assert!(
                                    original == &body,
                                    "cleanup must retry the byte-identical signed cognition request"
                                );
                                exact_cognition_retry_seen.store(true, Ordering::SeqCst);
                            }
                        }
                        let drop_original=path==SUBMIT_PATH&&serde_json::from_slice::<SubmitIntentRequest<WorldServicePayloadV1>>(&body).is_ok_and(|request|matches!(request.signed_payload,WorldServicePayloadV1::GameExecutorControl(signed) if signed.request.client_operation_id=="playtest-cleanup-stop"));
                        if drop_original&&first_cleanup_drop.swap(false,Ordering::SeqCst){println!("TASK_CLEANUP_FAULT original Stop connection closed before admission");continue;}
                        if drop_original && first_cleanup_slow.swap(false,Ordering::SeqCst) {
                            println!("TASK_CLEANUP_SLOW_ADMISSION millis={}",cleanup_admission_delay.as_millis());
                            std::thread::sleep(cleanup_admission_delay);
                        }
                        bytes.extend(body);
                        let handler_started = Instant::now();
                        let response = crate::world_service_api::private_goal::maybe_handle(
                            &mut stream,
                            &bytes,
                            &node,
                            &method,
                            &path,
                            "w1",
                            &root.join("world"),
                            &root.join("records"),
                            &root.join("store"),
                            &signer,
                            Some(&store),
                        );
                        if route != "other" {
                            let outcome = match &response {
                                Ok(true) => "completed",
                                Ok(false) => "unhandled",
                                Err(error) if error == "private response write failed" => "write_failed",
                                Err(_) => "handler_failed",
                            };
                            eprintln!(
                                "TASK_PRIVATE_FIXTURE fixture={} route={route} outcome={outcome} elapsed_ms={}",
                                kind.tag(),
                                handler_started.elapsed().as_millis()
                            );
                        }
                        let handled = match response {
                            Ok(handled) => handled,
                            Err(error) if error == "private response write failed" => {
                                // A disconnected peer did not receive an outcome. Keep serving
                                // so the client can retry its original signed operation.
                                println!("TASK_PRIVATE_RESPONSE_PEER_DISCONNECTED");
                                continue;
                            }
                            Err(error) => panic!("private handler failed: {error}"),
                        };
                        if !handled {
                            assert!(
                                crate::world_service_api::maybe_handle(
                                    &mut stream,
                                    &bytes,
                                    &node,
                                    &method,
                                    &path,
                                    "w1",
                                    &root.join("world"),
                                    &root.join("records"),
                                    &root.join("store"),
                                    &signer
                                )
                                .unwrap()
                            );
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(20))
                    }
                    Err(error) => panic!("live service listener failed: {error}"),
                }
            }
        });
        write_public_json(
            &ready,
            &serde_json::json!({"schema":"oasis7/task-prepared-world-ready/v1","pid":std::process::id(),"endpoint":endpoint,"config":config_path,"root":root,"witness":root.join("task-playtest-witness.json"),"scope":"prepared world; real native API and NodeRuntime worker; consensus membership not evaluated"}),
        );
        let deadline = Instant::now() + Duration::from_secs(360);
        let mut goal2_submission = None;
        while !immediate_cleanup && Instant::now() < deadline && !root.join("service-stop").exists()
        {
            let pinned = super::super::super::world_service_read::pin(
                &root.join("records"),
                &root.join("store"),
                &world,
                None,
            )
            .unwrap();
            if cleanup_after_height.is_some_and(|height| pinned.commit.position >= height) {
                break;
            }
            let control = game_task::current_control(&pinned.world, "agent-a")
                .unwrap()
                .unwrap();
            let position = pinned.world.state().agents["agent-a"].state.pos;
            if !logistics
                && !production
                && !starter_chain
                && position == oasis7_agent_api::world_action::GeoPos::new(200, 0, 0)
                && control.active_goal_revision == DecimalU64(1)
                && goal2_submission.is_none()
            {
                let mut goal2 = goal.request.clone();
                goal2.goal_revision = DecimalU64(2);
                goal2.expected_task_revision = control.task_revision;
                goal2.expected_latest_goal_revision = control.latest_goal_revision;
                goal2.client_operation_id = "live-g2-prepare".into();
                goal2.client_nonce = "45".repeat(32);
                goal2.content=GoalContentV1{goal_text:"The first landmark is complete. Reach the second landmark at coordinates (-200, 0, 0) centimeters, using this new complete goal and a fresh decision context. Choose a valid movement action yourself.".into(),soft_constraints:vec!["Use the currently bound executor and canonical signed action lane.".into()]};
                goal2.goal_digest =
                    game_goal_version_digest(&goal2.task_id, goal2.goal_revision, &goal2.content)
                        .unwrap();
                let goal2 = sign_goal_prepare(goal2, &owner).unwrap();
                let prepared = owner_client.prepare_goal(&goal2).unwrap();
                let public = publication(&goal2, &prepared, "live-g2-publish".into()).unwrap();
                let submission = request(
                    &world,
                    WorldServicePayloadV1::GameGoalPublish(
                        sign_read_request(GAME_GOAL_PUBLISH_DOMAIN, public, &owner).unwrap(),
                    ),
                );
                let observed = owner_client.submit(submission.clone()).unwrap();
                assert!(!matches!(
                    observed,
                    oasis7::world_service::SubmitObservation::OutcomeUnknown(_)
                ));
                goal2_submission = Some(submission);
            }
            let effects = pinned
                .world
                .capability_revocation_state()
                .world_service_results
                .values()
                .filter_map(|value| {
                    serde_json::from_value::<CanonicalIntentResultV1>(value.clone()).ok()
                })
                .filter(|result| {
                    matches!(
                        &result.request.signed_payload,
                        WorldServicePayloadV1::Scheduler(_)
                            | WorldServicePayloadV1::Cognition(_)
                            | WorldServicePayloadV1::GameGoalPublish(_)
                            | WorldServicePayloadV1::GameExecutorControl(_)
                    )
                })
                .collect::<Vec<_>>();
            let node_snapshot = node.lock().unwrap().snapshot();
            write_public_json(
                &root.join("task-playtest-witness.json"),
                &serde_json::json!({"schema":"oasis7/task-prepared-world-witness/v1","commit":pinned.commit,"task_revision":control.task_revision,"goal_revision":control.active_goal_revision,"executor_epoch":control.executor_epoch,"executor_binding":control.executor_binding,"position":position,"logistics":logistics.then(||logistics_playtest::witness(&pinned.world)),"production":production.then(||production_playtest::witness(&pinned.world)),"starter_chain":starter_chain.then(||starter_chain_playtest::witness(&pinned.world)),"canonical_effect_receipts":effects,"goal2_public_submission":goal2_submission,"node":{"node_id":node_snapshot.node_id,"running":node_snapshot.running,"tick_count":node_snapshot.tick_count,"last_error":node_snapshot.last_error},"scope":"actual immutable canonical pin; setup owner only changes goals; player actions use native submitted intents"}),
            );
            std::thread::sleep(Duration::from_millis(250));
        }
        // Revoke this fixture's execution authority through canonical operations
        // while the real service is still running, then close only our worker.
        let pinned = super::super::super::world_service_read::pin(
            &root.join("records"),
            &root.join("store"),
            &world,
            None,
        )
        .unwrap();
        let control = game_task::current_control(&pinned.world, "agent-a")
            .unwrap()
            .unwrap();
        let stop_request = request(
            &world,
            WorldServicePayloadV1::GameExecutorControl(
                sign_read_request(
                    GAME_EXECUTOR_CONTROL_DOMAIN,
                    GameExecutorControlV1 {
                        api_rev: 1,
                        client_operation_id: "playtest-cleanup-stop".into(),
                        fence: GameTaskFenceV1::from_control(&control),
                        operation: GameExecutorOperationV1::Stop,
                    },
                    &owner,
                )
                .unwrap(),
            ),
        );
        let revoke_request = request(
            &world,
            WorldServicePayloadV1::Delegation(
                sign_read_request(
                    "delegation",
                    AgentSignerDelegationChangeV1 {
                        world_id: world.world_id.clone(),
                        branch_id: "main".into(),
                        agent_id: "agent-a".into(),
                        owner_binding: "owner-a".into(),
                        agent_identity_generation: 1,
                        delegate_public_key: sign_read_request("key", (), &delegate)
                            .unwrap()
                            .subject_public_key,
                        generation: 2,
                        revoked: true,
                        nonce: 2,
                    },
                    &owner,
                )
                .unwrap(),
            ),
        );
        let mut cleanup = Vec::new();
        let mut cleanup_categories = Vec::new();
        for original in [stop_request, revoke_request] {
            let until = Instant::now() + cleanup_budget;
            let mut categories = CleanupCategories::default();
            match settle_cleanup_original(&owner_client, &original, until, &mut categories) {
                Ok(receipt) => {
                    cleanup.push(receipt);
                    cleanup_categories.push(categories);
                }
                Err(category) => {
                    write_public_json(
                        &root.join("task-playtest-cleanup-failed.json"),
                        &serde_json::json!({"schema":"oasis7/task-prepared-world-cleanup-failed/v1","category":category,"categories":categories,"original_correlation":original.correlation,"settled_authority_operations":cleanup}),
                    );
                    panic!("{category}");
                }
            }
        }
        write_public_json(
            &root.join("task-playtest-cleanup.json"),
            &serde_json::json!({
                "schema":"oasis7/task-prepared-world-cleanup/v1", "authority_operations":cleanup,"categories":cleanup_categories,
            }),
        );
        stop.store(true, Ordering::SeqCst);
        server.join().unwrap();
    });
    node.lock().unwrap().stop().unwrap();
    println!("OASIS7_TASK_PREPARED_WORLD_SERVICE_STOPPED");
    root
}

#[path = "world_service_task_cleanup_tests.rs"]
mod cleanup_tests;

#[path = "world_service_task_logistics_playtest.rs"]
mod logistics_playtest;

#[path = "world_service_task_production_playtest.rs"]
mod production_playtest;

#[derive(Clone, Copy)]
enum PreparedWorldKind {
    Movement,
    Logistics,
    Production,
    StarterChain,
}
impl PreparedWorldKind {
    const fn tag(self) -> &'static str {
        match self {
            Self::Movement => "movement",
            Self::Logistics => "logistics",
            Self::Production => "production",
            Self::StarterChain => "starter_chain",
        }
    }
}
fn task_private_route_tag(path: &str) -> &'static str {
    match path.split('?').next().unwrap_or(path) {
        oasis7::world_service::private_task_context::TASK_CONTEXT_PREPARE_PATH => {
            "task_context_prepare"
        }
        oasis7::world_service::private_task_context::TASK_CONTEXT_READ_PATH => "task_context_read",
        oasis7::world_service::private_task_context::TASK_SNAPSHOT_PATH => "task_snapshot",
        oasis7::world_service::task_recipe_quote::TASK_RECIPE_QUOTE_PATH => "task_recipe_quote",
        oasis7::world_service::task_factory_quote::TASK_FACTORY_QUOTE_PATH => "task_factory_quote",
        oasis7::world_service::private_goal_read::GOAL_PREPARE_PATH => "goal_prepare",
        oasis7::world_service::private_goal_read::GOAL_CURRENT_PATH => "goal_current",
        _ => "other",
    }
}
#[path = "world_service_task_starter_chain_playtest.rs"]
mod starter_chain_playtest;

pub(super) fn retry_private<T>(
    mut read: impl FnMut() -> Result<T, oasis7::world_service::client::WorldServiceClientError>,
    deadline: Instant,
) -> T {
    loop {
        assert!(
            Instant::now() < deadline,
            "private original read deadline exhausted"
        );
        match read() {
            Ok(value) => return value,
            Err(
                oasis7::world_service::client::WorldServiceClientError::Transport(_)
                | oasis7::world_service::client::WorldServiceClientError::Cooldown { .. },
            ) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => panic!("private original read failed: {e}"),
        }
    }
}

// Independent real-node scenarios share the QA host resource budget. Each scenario's
// own host workers, transport, pressure and fault paths remain concurrent.
fn private_task_resource_permit() -> Arc<super::super::scenario_resource::ScenarioResourcePermit> {
    super::super::scenario_resource::ScenarioResourcePermit::acquire()
}
#[path = "world_service_task_native_runner_conformance.rs"]
mod native_runner_conformance;

#[cfg(unix)]
#[path = "world_service_task_agc02_conformance.rs"]
mod agc02_conformance;

#[cfg(unix)]
#[path = "world_service_task_budget_conformance.rs"]
mod budget_conformance;

#[path = "world_service_task_quote_drift.rs"]
mod quote_drift;
