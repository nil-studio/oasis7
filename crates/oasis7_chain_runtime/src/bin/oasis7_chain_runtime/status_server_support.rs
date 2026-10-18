use super::*;
use oasis7::network_tier_manifest::LoadedNetworkTierManifest;
use tracing::Level;

const MAX_STATUS_HTTP_REQUEST_BYTES: usize = 65_536;

#[derive(Debug)]
pub(super) struct ChainStatusServer {
    stop_tx: Sender<()>,
    error_rx: Receiver<String>,
    join_handle: Option<thread::JoinHandle<()>>,
}

#[derive(Debug, Default, Serialize)]
pub(super) struct ChainReplicationDebugStatus {
    pub(super) local_peer_id: String,
    pub(super) connected_peers: Vec<String>,
    pub(super) peer_healths: Vec<ChainPeerHealthStatus>,
    pub(super) registered_protocols: Vec<String>,
    pub(super) protocol_retry_cooldown_peers: BTreeMap<String, Vec<String>>,
    pub(super) transport_retry_cooldown_peers: Vec<String>,
    pub(super) request_peer_scores: BTreeMap<String, u8>,
    pub(super) connection_events: Vec<ChainConnectionEventStatus>,
    pub(super) recent_errors: Vec<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct ChainConnectionEventStatus {
    pub(super) event: String,
    pub(super) peer_id: Option<String>,
    pub(super) direction: Option<String>,
    pub(super) reason: Option<String>,
    pub(super) protocol: Option<String>,
    pub(super) error: Option<String>,
    pub(super) num_established: Option<u32>,
    pub(super) active_path: Option<String>,
    pub(super) raw: String,
}

#[derive(Debug, Serialize)]
pub(super) struct ChainPeerHealthStatus {
    pub(super) peer_id: String,
    pub(super) status: String,
    pub(super) issues: Vec<String>,
    pub(super) discovery_sources: Vec<String>,
    pub(super) active_path_kind: Option<String>,
    pub(super) source_operator: Option<String>,
    pub(super) source_asn: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct ChainBalancesResponse {
    pub(super) ok: bool,
    pub(super) observed_at_unix_ms: i64,
    pub(super) node_id: String,
    pub(super) world_id: String,
    pub(super) execution_world_dir: String,
    pub(super) load_error: Option<String>,
    pub(super) node_asset_balance: Option<NodeAssetBalance>,
    pub(super) node_power_credit_balance: u64,
    pub(super) node_main_token_account: Option<String>,
    pub(super) node_main_token_liquid_balance: u64,
    pub(super) reward_mint_record_count: usize,
    pub(super) recent_reward_mint_records: Vec<NodeRewardMintRecord>,
}

#[expect(
    clippy::too_many_arguments,
    reason = "Stable protocol and runtime seam keeps independently validated inputs explicit."
)]
pub(super) fn start_chain_status_server(
    host: &str,
    port: u16,
    runtime: Arc<Mutex<NodeRuntime>>,
    replication_network: Arc<Libp2pReplicationNetwork>,
    options: CliOptions,
    node_id: String,
    world_id: String,
    execution_world_dir: PathBuf,
    execution_records_dir: PathBuf,
    execution_storage_root: PathBuf,
    loaded_network_tier_manifest: Option<LoadedNetworkTierManifest>,
    release_security_policy: ReleaseSecurityPolicy,
    effective_p2p_policy: NodeNetworkPolicy,
    reward_runtime_metrics: SharedRewardRuntimeMetrics,
    storage_metrics: storage_metrics::SharedStorageMetrics,
    feedback_submit_signer: FeedbackSubmitSigner,
    runtime_authority_binding: Option<super::runtime_authority::RuntimeAuthorityBinding>,
) -> Result<ChainStatusServer, String> {
    let listener = TcpListener::bind((host, port))
        .map_err(|err| format!("failed to bind status server at {host}:{port}: {err}"))?;
    let private_goal_store = super::world_service_api::private_goal::configured_store(
        listener
            .local_addr()
            .map_err(|_| "status listener address unavailable")?,
        &[
            &execution_world_dir,
            &execution_records_dir,
            &execution_storage_root,
        ],
    )?;
    listener
        .set_nonblocking(true)
        .map_err(|err| format!("failed to set status server listener nonblocking: {err}"))?;

    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (error_tx, error_rx) = mpsc::channel::<String>();

    let join_handle = thread::spawn(move || {
        if let Err(err) = run_chain_status_server_loop(
            listener,
            stop_rx,
            runtime,
            replication_network,
            options,
            node_id,
            world_id,
            execution_world_dir,
            execution_records_dir,
            execution_storage_root,
            loaded_network_tier_manifest,
            release_security_policy,
            effective_p2p_policy,
            reward_runtime_metrics,
            storage_metrics,
            feedback_submit_signer,
            runtime_authority_binding,
            private_goal_store,
        ) {
            let _ = error_tx.send(err);
        }
    });

    Ok(ChainStatusServer {
        stop_tx,
        error_rx,
        join_handle: Some(join_handle),
    })
}

/// Test-only handled-route evidence; never records body, keys or signatures.
#[cfg(test)]
fn record_test_service_request_witness(method: &str, path: &str) -> Result<(), String> {
    let Some(destination) = std::env::var_os("PRE2_SERVICE_REQUEST_WITNESS") else {
        return Ok(());
    };
    static COUNTS: std::sync::OnceLock<std::sync::Mutex<std::collections::BTreeMap<String, u64>>> =
        std::sync::OnceLock::new();
    let mut counts = COUNTS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "test service route witness mutex poisoned".to_string())?;
    let key = format!("{method} {path}");
    let count = counts.entry(key).or_default();
    *count = count
        .checked_add(1)
        .ok_or("test service route witness overflow")?;
    let destination = std::path::PathBuf::from(destination);
    let temporary = destination.with_extension("pending.json");
    let bytes = serde_json::to_vec(&*counts).map_err(|e| e.to_string())?;
    std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(temporary, destination).map_err(|e| e.to_string())?;
    Ok(())
}

fn build_chain_runtime_perf_snapshot(
    loaded_network_tier_manifest: Option<&LoadedNetworkTierManifest>,
    node_role: NodeRole,
) -> Option<oasis7::simulator::RuntimePerfSnapshot> {
    let gate_tier = super::status_payload::runtime_perf_gate_tier_from_manifest(
        loaded_network_tier_manifest,
        node_role,
    );
    super::status_payload::build_runtime_perf_snapshot_from_execution_bridge_timing(
        &super::execution_bridge::snapshot_execution_bridge_commit_timing(),
        gate_tier,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Stable protocol and runtime seam keeps independently validated inputs explicit."
)]
fn run_chain_status_server_loop(
    listener: TcpListener,
    stop_rx: Receiver<()>,
    runtime: Arc<Mutex<NodeRuntime>>,
    replication_network: Arc<Libp2pReplicationNetwork>,
    options: CliOptions,
    node_id: String,
    world_id: String,
    execution_world_dir: PathBuf,
    execution_records_dir: PathBuf,
    execution_storage_root: PathBuf,
    loaded_network_tier_manifest: Option<LoadedNetworkTierManifest>,
    release_security_policy: ReleaseSecurityPolicy,
    effective_p2p_policy: NodeNetworkPolicy,
    reward_runtime_metrics: SharedRewardRuntimeMetrics,
    storage_metrics: storage_metrics::SharedStorageMetrics,
    feedback_submit_signer: FeedbackSubmitSigner,
    runtime_authority_binding: Option<super::runtime_authority::RuntimeAuthorityBinding>,
    private_goal_store: Option<Arc<oasis7::world_service::private_goal_store::PrivateGoalStore>>,
) -> Result<(), String> {
    let admission = super::status_admission::Admission::default();
    loop {
        match stop_rx.try_recv() {
            Ok(_) | Err(TryRecvError::Disconnected) => return Ok(()),
            Err(TryRecvError::Empty) => {}
        }

        match listener.accept() {
            Ok((stream, addr)) => {
                let permit = match admission.acquire(addr.ip()) {
                    Ok(permit) => permit,
                    Err(status) => {
                        super::status_admission::overload(stream, status);
                        continue;
                    }
                };
                let runtime = Arc::clone(&runtime);
                let replication_network = Arc::clone(&replication_network);
                let options = options.clone();
                let node_id = node_id.clone();
                let world_id = world_id.clone();
                let execution_world_dir = execution_world_dir.clone();
                let execution_records_dir = execution_records_dir.clone();
                let execution_storage_root = execution_storage_root.clone();
                let loaded_network_tier_manifest = loaded_network_tier_manifest.clone();
                let release_security_policy = release_security_policy.clone();
                let effective_p2p_policy = effective_p2p_policy.clone();
                let reward_runtime_metrics = Arc::clone(&reward_runtime_metrics);
                let storage_metrics = Arc::clone(&storage_metrics);
                let feedback_submit_signer = feedback_submit_signer.clone();
                let runtime_authority_binding = runtime_authority_binding.clone();
                let private_goal_store = private_goal_store.clone();
                thread::spawn(move || {
                    let _permit = permit;
                    if let Err(err) = handle_chain_status_connection(
                        stream,
                        runtime,
                        replication_network,
                        &options,
                        node_id.as_str(),
                        world_id.as_str(),
                        execution_world_dir.as_path(),
                        execution_records_dir.as_path(),
                        execution_storage_root.as_path(),
                        loaded_network_tier_manifest.as_ref(),
                        &release_security_policy,
                        effective_p2p_policy,
                        reward_runtime_metrics,
                        storage_metrics,
                        &feedback_submit_signer,
                        runtime_authority_binding.as_ref(),
                        private_goal_store,
                    ) {
                        let stderr_message =
                            format!("warning: chain status connection failed: {err}");
                        oasis7::observability::emit_stderr_or_event(
                            Level::WARN,
                            stderr_message.as_str(),
                            "chain status connection failed",
                        );
                    }
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(20));
            }
            Err(err) => {
                return Err(format!("chain status server accept failed: {err}"));
            }
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Stable protocol and runtime seam keeps independently validated inputs explicit."
)]
fn handle_chain_status_connection(
    mut stream: TcpStream,
    runtime: Arc<Mutex<NodeRuntime>>,
    replication_network: Arc<Libp2pReplicationNetwork>,
    options: &CliOptions,
    node_id: &str,
    world_id: &str,
    execution_world_dir: &Path,
    execution_records_dir: &Path,
    execution_storage_root: &Path,
    loaded_network_tier_manifest: Option<&LoadedNetworkTierManifest>,
    release_security_policy: &ReleaseSecurityPolicy,
    effective_p2p_policy: NodeNetworkPolicy,
    reward_runtime_metrics: SharedRewardRuntimeMetrics,
    storage_metrics: storage_metrics::SharedStorageMetrics,
    feedback_submit_signer: &FeedbackSubmitSigner,
    runtime_authority_binding: Option<&super::runtime_authority::RuntimeAuthorityBinding>,
    private_goal_store: Option<Arc<oasis7::world_service::private_goal_store::PrivateGoalStore>>,
) -> Result<(), String> {
    stream
        .set_nonblocking(false)
        .map_err(|err| format!("failed to set status stream blocking: {err}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|err| format!("failed to set read timeout: {err}"))?;

    let request_bytes = match read_complete_status_http_request(&mut stream) {
        Ok(bytes) => bytes,
        Err(error) => {
            let status = if error.contains("byte limit") {
                413
            } else {
                408
            };
            let _ = super::status_admission::reject_ingress(&mut stream, status);
            return Err(error);
        }
    };
    if request_bytes.is_empty() {
        return Ok(());
    }

    let request = String::from_utf8_lossy(request_bytes.as_slice());
    let Some(line) = request.lines().next() else {
        write_json_response(&mut stream, 400, b"{\"error\":\"bad request\"}", false)
            .map_err(|err| format!("failed to write 400 response: {err}"))?;
        return Ok(());
    };

    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    let path = target.split('?').next().unwrap_or(target);
    let head_only = method.eq_ignore_ascii_case("HEAD");
    if super::world_service_api::private_goal::maybe_handle(
        &mut stream,
        &request_bytes,
        &runtime,
        method,
        target,
        world_id,
        execution_world_dir,
        execution_records_dir,
        execution_storage_root,
        feedback_submit_signer,
        private_goal_store.as_deref(),
    )? {
        return Ok(());
    }
    if super::world_service_api::maybe_handle(
        &mut stream,
        &request_bytes,
        &runtime,
        method,
        path,
        world_id,
        execution_world_dir,
        execution_records_dir,
        execution_storage_root,
        feedback_submit_signer,
    )? {
        #[cfg(test)]
        record_test_service_request_witness(method, path)?;
        return Ok(());
    }

    if transfer_submit_api::maybe_handle_transfer_submit_request(
        &mut stream,
        request_bytes.as_slice(),
        &runtime,
        method,
        path,
        node_id,
        world_id,
        execution_world_dir,
    )? {
        return Ok(());
    }

    if agent_claim_api::maybe_handle_agent_claim_request(
        &mut stream,
        request_bytes.as_slice(),
        &runtime,
        method,
        target,
        path,
        node_id,
        world_id,
        execution_world_dir,
    )? {
        return Ok(());
    }

    if gameplay_submit_api::maybe_handle_gameplay_submit_request(
        &mut stream,
        request_bytes.as_slice(),
        &runtime,
        method,
        path,
        execution_world_dir,
    )? {
        return Ok(());
    }

    if module_release_attestation_submit_api::maybe_handle_module_release_attestation_submit_request(
        &mut stream,
        request_bytes.as_slice(),
        &runtime,
        method,
        path,
    )? {
        return Ok(());
    }

    if main_token_submit_api::maybe_handle_main_token_submit_request(
        &mut stream,
        request_bytes.as_slice(),
        &runtime,
        method,
        path,
    )? {
        return Ok(());
    }

    if method.eq_ignore_ascii_case("POST") && path == "/v1/chain/feedback/submit" {
        let body = match extract_http_json_body(request_bytes.as_slice()) {
            Ok(body) => body,
            Err(err) => {
                write_feedback_submit_error(&mut stream, 400, err.as_str())?;
                return Ok(());
            }
        };
        let submit_request = match parse_feedback_submit_request(body) {
            Ok(request) => request,
            Err(err) => {
                write_feedback_submit_error(&mut stream, 400, err.as_str())?;
                return Ok(());
            }
        };
        let submit_ip = stream
            .peer_addr()
            .map(|addr| addr.ip().to_string())
            .unwrap_or_else(|_| "127.0.0.1".to_string());
        let create_request = match build_feedback_create_request(
            submit_request,
            feedback_submit_signer,
            node_id,
            submit_ip.as_str(),
            now_unix_ms(),
        ) {
            Ok(request) => request,
            Err(err) => {
                write_feedback_submit_error(&mut stream, 400, err.as_str())?;
                return Ok(());
            }
        };
        let receipt = match runtime
            .lock()
            .map_err(|_| "failed to lock node runtime for feedback submit".to_string())?
            .submit_feedback(create_request)
        {
            Ok(receipt) => receipt,
            Err(err) => {
                write_feedback_submit_error(
                    &mut stream,
                    502,
                    format!("feedback submit failed: {err}").as_str(),
                )?;
                return Ok(());
            }
        };
        let response = ChainFeedbackSubmitResponse::success(&receipt);
        let body = serde_json::to_vec_pretty(&response)
            .map_err(|err| format!("failed to encode feedback submit response: {err}"))?;
        write_json_response(&mut stream, 200, body.as_slice(), false)
            .map_err(|err| format!("failed to write /v1/chain/feedback/submit response: {err}"))?;
        return Ok(());
    }

    if !method.eq_ignore_ascii_case("GET") && !head_only {
        write_json_response(
            &mut stream,
            405,
            b"{\"error\":\"method not allowed\"}",
            head_only,
        )
        .map_err(|err| format!("failed to write 405 response: {err}"))?;
        return Ok(());
    }

    match path {
        "/healthz" => {
            write_json_response(&mut stream, 200, b"{\"ok\":true}", head_only)
                .map_err(|err| format!("failed to write /healthz response: {err}"))?;
        }
        "/v1/chain/rebuild-proof" | "/v1/chain/rebuild-status" => {
            let snapshot = runtime
                .lock()
                .map_err(|_| "failed to read node runtime snapshot: lock poisoned".to_string())?
                .snapshot();
            let network = replication_network.debug_snapshot();
            let checkpoint =
                super::execution_bridge::load_latest_execution_checkpoint_status_evidence(
                    execution_records_dir,
                )
                .ok()
                .flatten();
            let payload = super::rebuild_status::build_rebuild_status_with_signer(
                snapshot,
                network,
                checkpoint,
                now_unix_ms(),
                loaded_network_tier_manifest,
                feedback_submit_signer,
            )?;
            let body = serde_json::to_vec_pretty(&payload)
                .map_err(|err| format!("failed to encode rebuild proof payload: {err}"))?;
            write_json_response(&mut stream, 200, body.as_slice(), head_only)
                .map_err(|err| format!("failed to write rebuild proof response: {err}"))?;
        }
        "/v1/chain/status" => {
            let (mut snapshot, udp_gossip_traffic) = match runtime.try_lock() {
                Ok(locked) => (locked.snapshot(), locked.gossip_traffic_snapshot()),
                Err(std::sync::TryLockError::WouldBlock) => {
                    // A consensus execution commit may legitimately hold the
                    // runtime mutex while publishing its durable world. Do
                    // not wait past the viewer's bounded chain-link read and
                    // then write into a closed socket; fail closed so the
                    // viewer retries without accepting a stale projection.
                    write_json_response(
                        &mut stream,
                        503,
                        br#"{"error":"runtime_busy"}"#,
                        head_only,
                    )
                    .map_err(|err| {
                        format!("failed to write busy /v1/chain/status response: {err}")
                    })?;
                    return Ok(());
                }
                Err(std::sync::TryLockError::Poisoned(_)) => {
                    return Err("failed to read node runtime snapshot: lock poisoned".to_string());
                }
            };
            attach_governance_slashing_receipts(&mut snapshot, execution_world_dir);
            let live_snapshot = replication_network.reachability_snapshot();
            let (p2p_recommendation, p2p_detection) =
                build_live_node_network_policy_recommendation(options, Some(&live_snapshot))?;
            let applied_effective_user_mode =
                applied_runtime_user_mode_label(options).map(str::to_string);
            let replication_debug_status =
                build_chain_replication_debug_status(replication_network.as_ref());
            let transactions =
                match transfer_submit_api::try_build_chain_transfer_metrics_status(&runtime) {
                    Ok(Some(transactions)) => transactions,
                    Ok(None) => {
                        write_json_response(
                            &mut stream,
                            503,
                            br#"{"error":"runtime_busy"}"#,
                            head_only,
                        )
                        .map_err(|err| {
                            format!("failed to write busy /v1/chain/status response: {err}")
                        })?;
                        return Ok(());
                    }
                    Err(err) => return Err(err),
                };
            let payload =
                super::status_payload::build_chain_status_payload_with_storage_root_and_authority(
                    snapshot,
                    execution_world_dir,
                    Some(execution_records_dir),
                    Some(execution_storage_root),
                    loaded_network_tier_manifest,
                    &p2p_recommendation,
                    applied_effective_user_mode,
                    effective_p2p_policy,
                    &live_snapshot,
                    p2p_detection,
                    release_security_policy.clone(),
                    snapshot_metrics(&reward_runtime_metrics),
                    storage_metrics::snapshot_storage_metrics(&storage_metrics),
                    build_chain_wasm_status(),
                    build_chain_runtime_perf_snapshot(
                        loaded_network_tier_manifest,
                        options.node_role,
                    ),
                    build_chain_traffic_status(replication_network.as_ref(), udp_gossip_traffic),
                    transactions,
                    replication_debug_status,
                    runtime_authority_binding,
                );
            let body = serde_json::to_vec_pretty(&payload)
                .map_err(|err| format!("failed to encode status payload: {err}"))?;
            write_json_response(&mut stream, 200, body.as_slice(), head_only)
                .map_err(|err| format!("failed to write /v1/chain/status response: {err}"))?;
        }
        "/v1/chain/balances" => {
            let payload = build_chain_balances_payload(node_id, world_id, execution_world_dir);
            let body = serde_json::to_vec_pretty(&payload)
                .map_err(|err| format!("failed to encode balances payload: {err}"))?;
            write_json_response(&mut stream, 200, body.as_slice(), head_only)
                .map_err(|err| format!("failed to write /v1/chain/balances response: {err}"))?;
        }
        _ => {
            write_json_response(&mut stream, 404, b"{\"error\":\"not found\"}", head_only)
                .map_err(|err| format!("failed to write 404 response: {err}"))?;
        }
    }

    Ok(())
}

fn read_complete_status_http_request(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let deadline = std::time::Instant::now() + super::status_admission::IO_DEADLINE;
    let mut request = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 4096];

    loop {
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .filter(|duration| !duration.is_zero())
            .ok_or("HTTP total request deadline exceeded")?;
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|err| err.to_string())?;
        let bytes = stream
            .read(&mut chunk)
            .map_err(|err| format!("failed to read request: {err}"))?;
        if bytes == 0 {
            if request.is_empty() {
                return Ok(request);
            }
            return Err("incomplete HTTP request before connection close".to_string());
        }
        request.extend_from_slice(&chunk[..bytes]);
        if request.len() > MAX_STATUS_HTTP_REQUEST_BYTES {
            return Err(format!(
                "HTTP request exceeds {MAX_STATUS_HTTP_REQUEST_BYTES} byte limit"
            ));
        }
        if status_http_request_is_complete(request.as_slice())? {
            return Ok(request);
        }
    }
}

pub(super) fn status_http_request_is_complete(request: &[u8]) -> Result<bool, String> {
    let Some(header_end) = request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
    else {
        return Ok(false);
    };
    let header = std::str::from_utf8(&request[..header_end])
        .map_err(|err| format!("HTTP request header is not valid UTF-8: {err}"))?;
    let Some(content_length) = status_http_content_length(header)? else {
        return Ok(true);
    };
    let expected = header_end
        .checked_add(content_length)
        .ok_or_else(|| "HTTP Content-Length overflow".to_string())?;
    if expected > MAX_STATUS_HTTP_REQUEST_BYTES {
        return Err(format!(
            "HTTP request exceeds {MAX_STATUS_HTTP_REQUEST_BYTES} byte limit"
        ));
    }
    Ok(request.len() >= expected)
}

pub(super) fn status_http_content_length(header: &str) -> Result<Option<usize>, String> {
    for line in header.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("Content-Length") {
            let length = value
                .trim()
                .parse::<usize>()
                .map_err(|_| "Content-Length must be a non-negative integer".to_string())?;
            return Ok(Some(length));
        }
    }
    Ok(None)
}

pub(super) fn attach_governance_slashing_receipts(
    snapshot: &mut oasis7_node::NodeSnapshot,
    execution_world_dir: &Path,
) {
    if snapshot.consensus.slashing_intents.is_empty() {
        return;
    }
    let Ok(world) = super::execution_bridge::load_execution_world(execution_world_dir) else {
        return;
    };
    let intents_by_evidence = snapshot
        .consensus
        .slashing_intents
        .iter()
        .map(|intent| (intent.evidence_hash.clone(), intent.clone()))
        .collect::<BTreeMap<_, _>>();
    let receipts = world
        .governance_identity_penalties()
        .values()
        .filter_map(|record| {
            let intent = intents_by_evidence.get(record.evidence_hash.as_str())?;
            Some(NodeConsensusSlashingReceiptSnapshot {
                penalty_id: record.penalty_id,
                intent_id: intent.intent_id.clone(),
                evidence_hash: record.evidence_hash.clone(),
                validator_id: intent.validator_id.clone(),
                target_agent_id: record.target_agent_id.clone(),
                slash_stake: record.slash_stake,
                status: governance_identity_penalty_status_label(record.status).to_string(),
                evidence_chain_hash: record.evidence_chain_hash.clone(),
                appeal_deadline_tick: record.appeal_deadline_tick,
                applied: matches!(
                    record.status,
                    oasis7_runtime::GovernanceIdentityPenaltyStatus::Applied
                        | oasis7_runtime::GovernanceIdentityPenaltyStatus::Appealed
                        | oasis7_runtime::GovernanceIdentityPenaltyStatus::AppealAccepted
                        | oasis7_runtime::GovernanceIdentityPenaltyStatus::AppealRejected
                ),
            })
        })
        .collect::<Vec<_>>();
    if !receipts.is_empty() {
        snapshot.consensus.slashing_receipts = receipts;
    }
}

fn governance_identity_penalty_status_label(
    status: oasis7_runtime::GovernanceIdentityPenaltyStatus,
) -> &'static str {
    match status {
        oasis7_runtime::GovernanceIdentityPenaltyStatus::Applied => "applied",
        oasis7_runtime::GovernanceIdentityPenaltyStatus::Appealed => "appealed",
        oasis7_runtime::GovernanceIdentityPenaltyStatus::AppealAccepted => "appeal_accepted",
        oasis7_runtime::GovernanceIdentityPenaltyStatus::AppealRejected => "appeal_rejected",
    }
}

pub(super) fn build_chain_replication_debug_status(
    replication_network: &Libp2pReplicationNetwork,
) -> ChainReplicationDebugStatus {
    let debug_snapshot = replication_network.debug_snapshot();
    let peer_healths: Vec<ChainPeerHealthStatus> = debug_snapshot
        .peer_healths
        .into_iter()
        .map(|health| ChainPeerHealthStatus {
            peer_id: health.peer_id,
            status: health.status,
            issues: health.issues,
            discovery_sources: health.discovery_sources,
            active_path_kind: health.active_path_kind,
            source_operator: health.source_operator,
            source_asn: health.source_asn,
        })
        .collect();

    let mut protocol_retry_cooldown_peers: BTreeMap<String, Vec<String>> = debug_snapshot
        .protocol_retry_cooldown_peers
        .into_iter()
        .collect();
    for peer_ids in protocol_retry_cooldown_peers.values_mut() {
        peer_ids.sort();
        peer_ids.dedup();
    }
    let mut transport_retry_cooldown_peers = debug_snapshot.transport_retry_cooldown_peers;
    transport_retry_cooldown_peers.sort();
    transport_retry_cooldown_peers.dedup();

    let connection_events =
        build_connection_event_statuses(debug_snapshot.recent_errors.as_slice());

    ChainReplicationDebugStatus {
        local_peer_id: debug_snapshot.local_peer_id,
        connected_peers: debug_snapshot.connected_peers,
        peer_healths,
        registered_protocols: debug_snapshot.registered_protocols,
        protocol_retry_cooldown_peers,
        transport_retry_cooldown_peers,
        request_peer_scores: debug_snapshot.request_peer_scores.into_iter().collect(),
        connection_events,
        recent_errors: debug_snapshot.recent_errors,
    }
}

fn build_connection_event_statuses(recent_errors: &[String]) -> Vec<ChainConnectionEventStatus> {
    recent_errors
        .iter()
        .filter_map(|raw| parse_connection_event_status(raw.as_str()))
        .collect()
}

fn parse_connection_event_status(raw: &str) -> Option<ChainConnectionEventStatus> {
    let event = if raw.contains("libp2p connection established") {
        "connection_established"
    } else if raw.contains("libp2p connection closed") {
        "connection_closed"
    } else if raw.contains("libp2p redundant connections pruned") {
        "redundant_connections_pruned"
    } else if raw.contains("libp2p inbound failure") {
        "inbound_failure"
    } else if raw.contains("libp2p dial failed") {
        "dial_failed"
    } else if raw.contains("request failed:") {
        "request_failed"
    } else if raw.contains("peer record request failed") {
        "peer_record_request_failed"
    } else {
        return None;
    };
    let direction = if let Some(direction) = extract_key_value(raw, "direction=") {
        Some(direction)
    } else if raw.contains(" inbound ") || raw.contains(" inbound failure") {
        Some("inbound".to_string())
    } else if raw.contains(" outbound ") || raw.contains("outbound failure") {
        Some("outbound".to_string())
    } else if raw.contains("request failed:") {
        Some("request".to_string())
    } else {
        None
    };
    Some(ChainConnectionEventStatus {
        event: event.to_string(),
        peer_id: extract_peer_id(raw),
        direction,
        reason: extract_key_value(raw, "reason="),
        protocol: extract_key_value(raw, "protocol="),
        error: extract_connection_error(raw),
        num_established: extract_key_value(raw, "num_established=")
            .and_then(|value| value.parse::<u32>().ok()),
        active_path: extract_key_value(raw, "active_path="),
        raw: raw.to_string(),
    })
}

fn extract_key_value(raw: &str, key: &str) -> Option<String> {
    let start = raw.find(key)? + key.len();
    let value = raw[start..].split_whitespace().next()?;
    if value.is_empty() {
        None
    } else {
        Some(value.trim_matches('"').to_string())
    }
}

fn extract_peer_id(raw: &str) -> Option<String> {
    extract_key_value(raw, "peer=").or_else(|| {
        let start = raw.find("PeerId(\"")? + "PeerId(\"".len();
        let rest = &raw[start..];
        let end = rest.find("\")")?;
        Some(rest[..end].to_string())
    })
}

fn extract_connection_error(raw: &str) -> Option<String> {
    if let Some(value) = raw.strip_prefix("request failed: ") {
        return value
            .split(" protocol=")
            .next()
            .map(|part| part.trim().to_string())
            .filter(|part| !part.is_empty());
    }
    if let Some(start) = raw.find("): ") {
        return Some(raw[start + 3..].trim().to_string());
    }
    if let Some(value) = raw.strip_prefix("libp2p peer record request failed: ") {
        return Some(value.trim().to_string());
    }
    if let Some(value) = raw.strip_prefix("libp2p dial failed: ") {
        return Some(value.trim().to_string());
    }
    None
}

pub(super) fn poll_chain_status_server_error(
    server: &mut ChainStatusServer,
) -> Result<Option<String>, String> {
    match server.error_rx.try_recv() {
        Ok(err) => Ok(Some(format!("status server failed: {err}"))),
        Err(TryRecvError::Disconnected) => Ok(Some(
            "status server channel disconnected unexpectedly".to_string(),
        )),
        Err(TryRecvError::Empty) => {
            if let Some(handle) = server.join_handle.as_ref()
                && handle.is_finished()
            {
                return Ok(Some("status server exited unexpectedly".to_string()));
            }
            Ok(None)
        }
    }
}

pub(super) fn stop_chain_status_server(server: &mut ChainStatusServer) {
    let _ = server.stop_tx.send(());
    if let Some(handle) = server.join_handle.take() {
        let _ = handle.join();
    }
}

pub(super) fn write_json_response(
    stream: &mut TcpStream,
    status_code: u16,
    body: &[u8],
    head_only: bool,
) -> std::io::Result<()> {
    let status_text = match status_code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        413 => "Payload Too Large",
        502 => "Bad Gateway",
        _ => "Internal Server Error",
    };
    let headers = format!(
        "HTTP/1.1 {status_code} {status_text}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let deadline = std::time::Instant::now() + super::status_admission::IO_DEADLINE;
    super::status_admission::write_bounded(stream, headers.as_bytes(), deadline)?;
    if !head_only {
        super::status_admission::write_bounded(stream, body, deadline)?;
    }
    stream.flush()?;
    Ok(())
}

/// The explicit engineering branch admits only existing signed WorldService
/// routes on loopback; checkpoint, CAS, observer and legacy submit routes have
/// no reachable handler here.
pub(super) fn handle_guarded_connection(
    mut stream: TcpStream,
    runtime: &Arc<Mutex<NodeRuntime>>,
    context: &super::controlled_live_runtime::GuardedConnectionContext,
    records: &Path,
    signer: &FeedbackSubmitSigner,
) -> Result<(), String> {
    if !stream
        .peer_addr()
        .map_err(|e| e.to_string())?
        .ip()
        .is_loopback()
    {
        return Err("guarded ingress requires loopback".into());
    }
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    let bytes = read_complete_status_http_request(&mut stream)?;
    let request = String::from_utf8_lossy(&bytes);
    let mut line = request
        .lines()
        .next()
        .ok_or("guarded request line missing")?
        .split_whitespace();
    let method = line.next().ok_or("guarded method missing")?;
    let target = line.next().ok_or("guarded path missing")?;
    let path = target.split('?').next().unwrap_or(target);
    let config = context.config_path.as_path();
    let fence = &context.read_authority;
    if !super::world_service_api::maybe_handle_with_guarded(
        &mut stream,
        &bytes,
        runtime,
        method,
        path,
        &context.world_id,
        Path::new(""),
        records,
        Path::new(""),
        signer,
        Some((config, fence)),
    )? {
        write_json_response(
            &mut stream,
            404,
            b"{\"error\":\"guarded_route_unavailable\"}",
            false,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}
