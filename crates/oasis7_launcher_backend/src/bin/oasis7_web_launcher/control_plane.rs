use super::runtime_paths::viewer_dev_dist_candidates;
use super::*;
use oasis7_client_api::network_tier_manifest::LoadedNetworkTierManifest;
use oasis7_proto::storage_profile::StorageProfile;
use std::io::{BufRead, BufReader, Read};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, Instant};

const CHAIN_TRANSFER_SUBMIT_PATH: &str = "/v1/chain/transfer/submit";
const CHAIN_FEEDBACK_SUBMIT_PATH: &str = "/v1/chain/feedback/submit";
const CHAIN_TRANSFER_PROXY_TIMEOUT_MS: u64 = 1_500;
const CHAIN_RECOVERY_MODE_FRESH_NODE_ID: &str = "fresh_node_id";
const CHAIN_RECOVERY_PORT_SCAN_LIMIT: u16 = 32;
const DEFAULT_CHAIN_STATUS_BIND_PORT: u16 = 5121;
const GAME_STATIC_DIR_ENV: &str = "OASIS7_GAME_STATIC_DIR";

#[path = "control_plane_chain_requests.rs"]
mod chain_requests;
#[path = "control_plane/chain_status_probe.rs"]
mod chain_status_probe;
#[path = "control_plane/provider_config.rs"]
mod provider_config;
#[path = "control_plane/support.rs"]
mod support;
#[cfg(test)]
#[path = "control_plane/tests.rs"]
mod tests;
use self::chain_requests::{submit_chain_feedback_remote, submit_chain_transfer_remote};
pub(super) use self::chain_status_probe::{ChainIdentitySnapshot, query_chain_status_endpoint};
#[cfg(test)]
pub(super) use self::provider_config::build_launcher_args;
pub(super) use self::provider_config::build_launcher_args_with_launcher_bin;
use self::provider_config::*;
#[cfg(test)]
use self::support::resolve_viewer_static_env_override;
use self::support::{
    chain_execution_world_dir, encoded_query_pair, resolve_chain_runtime_bin_from_config,
    resolve_chain_world_id, resolve_launcher_bin_from_config, resolve_runtime_host,
    resolve_viewer_static_dir_for_launcher, spawn_child_process, stop_child_process,
};

pub(super) fn host_for_url(host: &str) -> String {
    support::host_for_url(host)
}

pub(super) fn parse_config_request(body: &[u8], action: &str) -> Result<LauncherConfig, String> {
    let mut config: LauncherConfig = serde_json::from_slice(body)
        .map_err(|err| format!("parse {action} request JSON failed: {err}"))?;
    normalize_chain_network_tier_config(&mut config)?;
    Ok(config)
}

pub(super) fn parse_chain_transfer_request(
    body: &[u8],
) -> Result<ChainTransferSubmitRequest, String> {
    serde_json::from_slice(body)
        .map_err(|err| format!("parse chain transfer request JSON failed: {err}"))
}

pub(super) fn parse_chain_feedback_request(
    body: &[u8],
) -> Result<ChainFeedbackSubmitRequest, String> {
    serde_json::from_slice(body)
        .map_err(|err| format!("parse chain feedback request JSON failed: {err}"))
}

fn local_chain_runtime_is_blocked(config: &LauncherConfig) -> bool {
    !deployment_mode_from_config(config).allows_local_chain_runtime()
}

fn local_chain_runtime_blocked_error(config: &LauncherConfig) -> String {
    match DeploymentMode::parse(config.deployment_mode.as_str(), "deployment_mode") {
        Ok(deployment_mode) => format!(
            "deployment_mode={} keeps the local chain runtime disabled; public join stays on the guest/player session lane and node onboarding remains operator-managed",
            deployment_mode.as_str()
        ),
        Err(_) => format!(
            "deployment_mode={} is invalid; local chain runtime remains disabled and public join stays on the guest/player session lane while node onboarding remains operator-managed",
            config.deployment_mode.trim()
        ),
    }
}

pub(super) fn submit_chain_transfer(
    state: &mut ServiceState,
    request: &ChainTransferSubmitRequest,
) -> ChainTransferSubmitResponse {
    if !state.config.chain_enabled {
        let response =
            ChainTransferSubmitResponse::error("chain_disabled", "chain runtime is disabled");
        state.append_log("chain transfer submit rejected: chain runtime is disabled");
        state.mark_updated();
        return response;
    }
    if matches!(
        deployment_mode_from_config(&state.config),
        DeploymentMode::HostedPublicJoin
    ) {
        let response = ChainTransferSubmitResponse::error(
            "strong_auth_required",
            "hosted public join blocks main token transfer until the dedicated strong-auth lane lands; legacy viewer signer bootstrap is preview-only",
        );
        state.append_log(
            "chain transfer submit rejected: hosted_public_join requires strong_auth/private plane",
        );
        state.mark_updated();
        return response;
    }

    let chain_status_bind = state.config.chain_status_bind.clone();
    match submit_chain_transfer_remote(chain_status_bind.as_str(), request) {
        Ok(response) => {
            if response.ok {
                let action_id = response
                    .action_id
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "n/a".to_string());
                state.append_log(format!(
                    "chain transfer submitted via control plane (action_id={action_id})"
                ));
            } else {
                let error_code = response
                    .error_code
                    .as_deref()
                    .map(|code| format!(" ({code})"))
                    .unwrap_or_default();
                let error_text = response.error.as_deref().unwrap_or("unknown error");
                state.append_log(format!(
                    "chain transfer rejected by runtime{error_code}: {error_text}"
                ));
            }
            state.mark_updated();
            response
        }
        Err(err) => {
            let response = ChainTransferSubmitResponse::error("proxy_error", err.clone());
            state.append_log(format!("chain transfer proxy failed: {err}"));
            state.mark_updated();
            response
        }
    }
}

pub(super) fn submit_chain_feedback(
    state: &mut ServiceState,
    request: &ChainFeedbackSubmitRequest,
) -> ChainFeedbackSubmitResponse {
    if !state.config.chain_enabled {
        let response = ChainFeedbackSubmitResponse::error("chain runtime is disabled");
        state.append_log("chain feedback submit rejected: chain runtime is disabled");
        state.mark_updated();
        return response;
    }

    let chain_status_bind = state.config.chain_status_bind.clone();
    match submit_chain_feedback_remote(chain_status_bind.as_str(), request) {
        Ok(response) => {
            if response.ok {
                let feedback_id = response.feedback_id.as_deref().unwrap_or("n/a");
                let event_id = response.event_id.as_deref().unwrap_or("n/a");
                state.append_log(format!(
                    "chain feedback submitted via control plane (feedback_id={feedback_id}, event_id={event_id})"
                ));
            } else {
                let error_text = response.error.as_deref().unwrap_or("unknown error");
                state.append_log(format!("chain feedback rejected by runtime: {error_text}"));
            }
            state.mark_updated();
            response
        }
        Err(err) => {
            let response = ChainFeedbackSubmitResponse::error(err.clone());
            state.append_log(format!("chain feedback proxy failed: {err}"));
            state.mark_updated();
            response
        }
    }
}

fn clear_chain_session_evidence(state: &mut ServiceState) {
    state.chain_identity = None;
    state.chain_p2p_status = None;
    state.chain_observability_status = None;
    state.chain_replication_status = None;
}

pub(super) fn poll_service_state(state: &mut ServiceState) {
    poll_process_state(state);
    poll_chain_process_state(state);
    update_chain_runtime_status(state);
}

pub(super) fn poll_process_state(state: &mut ServiceState) {
    let Some(mut running) = state.running.take() else {
        return;
    };

    while let Ok(line) = running.log_rx.try_recv() {
        state.append_log(line);
    }

    match running.child.try_wait() {
        Ok(Some(status)) => {
            state.process_state = ProcessState::Exited(status.to_string());
            state.append_log(format!("oasis7_game_launcher exited: {status}"));
            state.mark_updated();
        }
        Ok(None) => {
            state.running = Some(running);
        }
        Err(err) => {
            state.process_state =
                ProcessState::Exited(format!("query process status failed: {err}"));
            state.append_log(format!("query process status failed: {err}"));
            state.mark_updated();
        }
    }
}

pub(super) fn poll_chain_process_state(state: &mut ServiceState) {
    let Some(mut running) = state.chain_running.take() else {
        return;
    };

    let mut recent_chain_logs = Vec::new();
    while let Ok(line) = running.log_rx.try_recv() {
        recent_chain_logs.push(line.clone());
        state.append_log(line);
    }

    match running.child.try_wait() {
        Ok(Some(status)) => {
            clear_chain_session_evidence(state);
            state.chain_started_at = None;
            state.last_chain_probe_at = None;
            let exit_line = format!("oasis7_chain_runtime exited: {status}");
            state.append_log(exit_line.clone());
            if let Some(recovery) =
                classify_stale_execution_world(state, recent_chain_logs.as_slice())
            {
                let reason = recovery.reason.clone();
                let node_id = recovery.node_id.clone();
                let fresh_node_id = recovery.fresh_node_id.clone();
                state.chain_runtime_status = ChainRuntimeStatus::StaleExecutionWorld(reason);
                state.chain_recovery = Some(recovery);
                state.append_log(format!(
                    "oasis7_chain_runtime stale execution world detected for node `{node_id}`; suggested fresh node id `{fresh_node_id}`"
                ));
            } else {
                state.chain_runtime_status = ChainRuntimeStatus::Unreachable(exit_line);
                state.chain_recovery = None;
            }
            state.mark_updated();
        }
        Ok(None) => {
            state.chain_running = Some(running);
        }
        Err(err) => {
            clear_chain_session_evidence(state);
            state.chain_started_at = None;
            state.last_chain_probe_at = None;
            state.chain_runtime_status =
                ChainRuntimeStatus::Unreachable(format!("query chain runtime failed: {err}"));
            state.chain_recovery = None;
            state.append_log(format!("query chain runtime status failed: {err}"));
            state.mark_updated();
        }
    }
}

pub(super) fn update_chain_runtime_status(state: &mut ServiceState) {
    if !state.config.chain_enabled || local_chain_runtime_is_blocked(&state.config) {
        state.chain_runtime_status = ChainRuntimeStatus::Disabled;
        state.chain_identity = None;
        state.chain_p2p_status = None;
        state.chain_observability_status = None;
        state.chain_replication_status = None;
        state.chain_recovery = None;
        state.last_chain_probe_at = None;
        return;
    }

    if state.chain_running.is_none() {
        clear_chain_session_evidence(state);
        if !matches!(
            state.chain_runtime_status,
            ChainRuntimeStatus::ConfigError(_)
                | ChainRuntimeStatus::StaleExecutionWorld(_)
                | ChainRuntimeStatus::Unreachable(_)
        ) {
            state.chain_runtime_status = ChainRuntimeStatus::NotStarted;
            state.chain_identity = None;
            state.chain_p2p_status = None;
            state.chain_observability_status = None;
            state.chain_replication_status = None;
            state.chain_recovery = None;
        }
        state.last_chain_probe_at = None;
        return;
    }

    let now = Instant::now();
    let should_probe = state.last_chain_probe_at.is_none_or(|last| {
        now.duration_since(last) >= Duration::from_millis(CHAIN_STATUS_PROBE_INTERVAL_MS)
    });
    if !should_probe {
        return;
    }

    state.last_chain_probe_at = Some(now);
    match query_chain_status_endpoint(state.config.chain_status_bind.as_str()) {
        Ok(status_snapshot) => {
            state.chain_runtime_status = ChainRuntimeStatus::Ready;
            state.chain_identity = status_snapshot.identity;
            state.chain_p2p_status = Some(status_snapshot.p2p);
            state.chain_observability_status = Some(status_snapshot.observability);
            state.chain_replication_status = Some(status_snapshot.replication);
            state.chain_recovery = None;
        }
        Err(err) => {
            state.chain_identity = None;
            state.chain_p2p_status = None;
            state.chain_observability_status = None;
            state.chain_replication_status = None;
            let within_grace = state.chain_started_at.is_some_and(|started_at| {
                now.duration_since(started_at)
                    < Duration::from_secs(CHAIN_STATUS_STARTING_GRACE_SECS)
            });
            if within_grace {
                state.chain_runtime_status = ChainRuntimeStatus::Starting;
            } else if err.contains("chain status bind") {
                state.chain_runtime_status = ChainRuntimeStatus::ConfigError(err);
                state.chain_recovery = None;
            } else {
                state.chain_runtime_status = ChainRuntimeStatus::Unreachable(err);
                state.chain_recovery = None;
            }
        }
    }
}

fn classify_stale_execution_world(
    state: &ServiceState,
    recent_chain_logs: &[String],
) -> Option<ChainRecoverySnapshot> {
    let recent_log_text = state
        .logs
        .iter()
        .rev()
        .take(64)
        .rev()
        .cloned()
        .chain(recent_chain_logs.iter().cloned())
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();
    let has_distributed_validation_failed = recent_log_text.contains("distributedvalidationfailed");
    let has_state_root_mismatch = recent_log_text.contains("latest state root mismatch");
    if !has_distributed_validation_failed || !has_state_root_mismatch {
        return None;
    }

    let node_id = state.config.chain_node_id.trim();
    let fresh_node_id = suggest_fresh_chain_node_id(node_id);
    let fresh_chain_status_bind =
        suggest_fresh_chain_status_bind(state.config.chain_status_bind.as_str());
    let mut suggested_config = state.config.clone();
    suggested_config.chain_node_id = fresh_node_id.clone();
    suggested_config.chain_status_bind = fresh_chain_status_bind.clone();
    let reason = format!(
        "stale execution world detected for node `{}`: latest state root mismatch from prior persisted chain state",
        if node_id.is_empty() {
            DEFAULT_CHAIN_NODE_ID
        } else {
            node_id
        }
    );

    Some(ChainRecoverySnapshot {
        error_code: "stale_execution_world".to_string(),
        reason,
        node_id: if node_id.is_empty() {
            DEFAULT_CHAIN_NODE_ID.to_string()
        } else {
            node_id.to_string()
        },
        execution_world_dir: chain_execution_world_dir(if node_id.is_empty() {
            DEFAULT_CHAIN_NODE_ID
        } else {
            node_id
        }),
        recovery_mode: CHAIN_RECOVERY_MODE_FRESH_NODE_ID.to_string(),
        reset_required: false,
        fresh_node_id,
        fresh_chain_status_bind,
        suggested_config,
    })
}

fn suggest_fresh_chain_node_id(node_id: &str) -> String {
    let base = node_id.trim();
    let base = if base.is_empty() {
        DEFAULT_CHAIN_NODE_ID
    } else {
        base
    };
    format!("{base}-fresh-{}", runtime_paths::now_unix_ms())
}

fn suggest_fresh_chain_status_bind(bind: &str) -> String {
    let (host, port) = parse_host_port(bind, "chain status bind")
        .unwrap_or_else(|_| ("127.0.0.1".to_string(), DEFAULT_CHAIN_STATUS_BIND_PORT));
    let normalized_host = runtime_paths::normalize_bind_host_for_local_access(host.as_str());
    let mut candidate = if port == u16::MAX {
        port
    } else {
        port.saturating_add(1)
    };
    for _ in 0..CHAIN_RECOVERY_PORT_SCAN_LIMIT {
        if candidate != port && TcpListener::bind((normalized_host.as_str(), candidate)).is_ok() {
            return format_host_port(host.as_str(), candidate);
        }
        if candidate == u16::MAX {
            break;
        }
        candidate = candidate.saturating_add(1);
    }
    format_host_port(
        host.as_str(),
        if port == u16::MAX {
            port
        } else {
            port.saturating_add(1)
        },
    )
}

fn format_host_port(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') && !host.ends_with(']') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

#[allow(dead_code)]
fn probe_chain_status_endpoint(bind: &str) -> Result<(), String> {
    query_chain_status_endpoint(bind).map(|_| ())
}

pub(super) fn start_process(
    state: &mut ServiceState,
    config: LauncherConfig,
) -> Result<(), String> {
    if state.running.is_some() {
        return Err("oasis7_game_launcher is already running".to_string());
    }

    let launcher_bin = resolve_launcher_bin_from_config(&config, state.launcher_bin.as_str());
    let issues = validate_game_config_with_launcher_bin(&config, launcher_bin.as_str());
    if !issues.is_empty() {
        let detail = issues.join("; ");
        state.process_state = ProcessState::InvalidConfig(detail.clone());
        state.append_log(format!("config validation failed: {detail}"));
        state.mark_updated();
        return Err(detail);
    }

    let args = match build_launcher_args_with_launcher_bin(&config, launcher_bin.as_str()) {
        Ok(args) => args,
        Err(err) => {
            state.process_state = ProcessState::InvalidConfig(err.clone());
            state.append_log(format!("invalid launch args: {err}"));
            state.mark_updated();
            return Err(err);
        }
    };

    if !Path::new(launcher_bin.as_str()).is_file() {
        let err = format!("launcher binary does not exist: {launcher_bin}");
        state.process_state = ProcessState::StartFailed(err.clone());
        state.append_log(format!("start failed: {err}"));
        state.mark_updated();
        return Err(err);
    }

    let mut config = config;
    config.launcher_bin = launcher_bin.clone();
    if let Some(viewer_static_dir) = resolve_viewer_static_dir_for_launcher(
        config.viewer_static_dir.trim(),
        launcher_bin.as_str(),
    ) {
        config.viewer_static_dir = viewer_static_dir.to_string_lossy().to_string();
    }

    match spawn_child_process(launcher_bin.as_str(), args.as_slice(), "game") {
        Ok(process) => {
            let pid = process.child.id();
            state.running = Some(process);
            state.config = config;
            state.process_state = ProcessState::Running { pid };
            state.append_log(format!(
                "oasis7_game_launcher started (pid={pid}, bin={launcher_bin})"
            ));
            state.mark_updated();
            Ok(())
        }
        Err(err) => {
            state.process_state = ProcessState::StartFailed(err.clone());
            state.append_log(format!("start failed: {err}"));
            state.mark_updated();
            Err(err)
        }
    }
}

pub(super) fn start_chain_process(
    state: &mut ServiceState,
    config: LauncherConfig,
) -> Result<(), String> {
    if state.chain_running.is_some() {
        return Err("oasis7_chain_runtime is already running".to_string());
    }

    clear_chain_session_evidence(state);
    state.last_chain_probe_at = None;
    if local_chain_runtime_is_blocked(&config) {
        let err = local_chain_runtime_blocked_error(&config);
        state.config = config;
        state.chain_runtime_status = ChainRuntimeStatus::Disabled;
        state.chain_p2p_status = None;
        state.chain_observability_status = None;
        state.chain_recovery = None;
        state.chain_running = None;
        state.chain_started_at = None;
        state.last_chain_probe_at = None;
        state.append_log(format!("chain runtime start rejected: {err}"));
        state.mark_updated();
        return Err(err);
    }
    if !config.chain_enabled {
        state.config = config;
        state.chain_runtime_status = ChainRuntimeStatus::Disabled;
        state.chain_p2p_status = None;
        state.chain_observability_status = None;
        state.chain_recovery = None;
        state.chain_running = None;
        state.chain_started_at = None;
        state.last_chain_probe_at = None;
        state.mark_updated();
        return Err("chain runtime is disabled".to_string());
    }

    let issues = validate_chain_config(&config);
    if !issues.is_empty() {
        let detail = issues.join("; ");
        state.chain_runtime_status = ChainRuntimeStatus::ConfigError(detail.clone());
        state.chain_p2p_status = None;
        state.chain_observability_status = None;
        state.chain_recovery = None;
        state.append_log(format!("chain config validation failed: {detail}"));
        state.mark_updated();
        return Err(detail);
    }

    let args = match build_chain_runtime_args(&config) {
        Ok(args) => args,
        Err(err) => {
            state.chain_runtime_status = ChainRuntimeStatus::ConfigError(err.clone());
            state.chain_p2p_status = None;
            state.chain_observability_status = None;
            state.chain_recovery = None;
            state.append_log(format!("invalid chain runtime args: {err}"));
            state.mark_updated();
            return Err(err);
        }
    };

    let chain_runtime_bin =
        resolve_chain_runtime_bin_from_config(&config, state.chain_runtime_bin.as_str());
    if !Path::new(chain_runtime_bin.as_str()).is_file() {
        let err = format!("chain runtime binary does not exist: {chain_runtime_bin}");
        state.chain_runtime_status = ChainRuntimeStatus::Unreachable(err.clone());
        state.chain_p2p_status = None;
        state.chain_observability_status = None;
        state.chain_recovery = None;
        state.append_log(format!("chain runtime start failed: {err}"));
        state.mark_updated();
        return Err(err);
    }

    match spawn_child_process(chain_runtime_bin.as_str(), args.as_slice(), "chain") {
        Ok(process) => {
            let pid = process.child.id();
            state.chain_running = Some(process);
            state.config = config;
            state.chain_started_at = Some(Instant::now());
            state.last_chain_probe_at = None;
            state.chain_runtime_status = ChainRuntimeStatus::Starting;
            state.chain_p2p_status = None;
            state.chain_observability_status = None;
            state.chain_recovery = None;
            state.append_log(format!(
                "oasis7_chain_runtime started (pid={pid}, bin={chain_runtime_bin})"
            ));
            state.mark_updated();
            Ok(())
        }
        Err(err) => {
            state.chain_started_at = None;
            state.chain_runtime_status = ChainRuntimeStatus::Unreachable(err.clone());
            state.chain_p2p_status = None;
            state.chain_observability_status = None;
            state.chain_recovery = None;
            state.append_log(format!("chain runtime start failed: {err}"));
            state.mark_updated();
            Err(err)
        }
    }
}

pub(super) fn stop_process(state: &mut ServiceState) -> Result<(), String> {
    let Some(mut running) = state.running.take() else {
        if matches!(state.process_state, ProcessState::Running { .. }) {
            state.process_state = ProcessState::Stopped;
        }
        state.append_log("oasis7_game_launcher stop requested but process is not running");
        state.mark_updated();
        return Ok(());
    };

    match stop_child_process(&mut running.child) {
        Ok(()) => {
            state.process_state = ProcessState::Stopped;
            state.append_log("oasis7_game_launcher stopped");
            state.mark_updated();
            Ok(())
        }
        Err(err) => {
            state.process_state = ProcessState::StopFailed(err.clone());
            state.append_log(format!("stop failed: {err}"));
            state.mark_updated();
            Err(err)
        }
    }
}

pub(super) fn stop_chain_process(state: &mut ServiceState) -> Result<(), String> {
    clear_chain_session_evidence(state);
    let Some(mut running) = state.chain_running.take() else {
        if !matches!(
            state.chain_runtime_status,
            ChainRuntimeStatus::StaleExecutionWorld(_)
                | ChainRuntimeStatus::Unreachable(_)
                | ChainRuntimeStatus::ConfigError(_)
        ) {
            state.chain_runtime_status = if state.config.chain_enabled {
                ChainRuntimeStatus::NotStarted
            } else {
                ChainRuntimeStatus::Disabled
            };
        }
        state.chain_started_at = None;
        state.last_chain_probe_at = None;
        state.chain_p2p_status = None;
        state.chain_observability_status = None;
        state.chain_recovery = None;
        state.append_log("oasis7_chain_runtime stop requested but process is not running");
        state.mark_updated();
        return Ok(());
    };

    match stop_child_process(&mut running.child) {
        Ok(()) => {
            state.chain_started_at = None;
            state.last_chain_probe_at = None;
            state.chain_runtime_status = if state.config.chain_enabled {
                ChainRuntimeStatus::NotStarted
            } else {
                ChainRuntimeStatus::Disabled
            };
            state.chain_p2p_status = None;
            state.chain_observability_status = None;
            state.chain_recovery = None;
            state.append_log("oasis7_chain_runtime stopped");
            state.mark_updated();
            Ok(())
        }
        Err(err) => {
            state.chain_runtime_status = ChainRuntimeStatus::Unreachable(err.clone());
            state.chain_p2p_status = None;
            state.chain_observability_status = None;
            state.chain_recovery = None;
            state.append_log(format!("oasis7_chain_runtime stop failed: {err}"));
            state.mark_updated();
            Err(err)
        }
    }
}

pub(super) fn snapshot_from_state(
    state: &ServiceState,
    request_host: Option<&str>,
) -> StateSnapshot {
    let game_url = build_game_url(&state.config, request_host);
    StateSnapshot {
        status: state.process_state.code().to_string(),
        detail: state.process_state.detail(),
        pid: state.process_state.pid(),
        running: matches!(state.process_state, ProcessState::Running { .. }),
        launcher_bin: resolve_launcher_bin_from_config(&state.config, state.launcher_bin.as_str()),
        chain_status: state.chain_runtime_status.code().to_string(),
        chain_detail: state.chain_runtime_status.detail(),
        chain_pid: state
            .chain_running
            .as_ref()
            .map(|process| process.child.id()),
        chain_running: state.chain_running.is_some(),
        chain_runtime_bin: resolve_chain_runtime_bin_from_config(
            &state.config,
            state.chain_runtime_bin.as_str(),
        ),
        chain_identity: state.chain_identity.clone(),
        chain_p2p_status: state.chain_p2p_status.clone(),
        chain_observability_status: state.chain_observability_status.clone(),
        chain_replication_status: state.chain_replication_status.clone(),
        chain_recovery: state.chain_recovery.clone(),
        hosted_access: hosted_player_access_contract(deployment_mode_from_config(&state.config)),
        game_url,
        config: state.config.redacted_for_state_response(),
        logs: state.logs.iter().cloned().collect(),
        updated_at_unix_ms: state.updated_at_unix_ms,
    }
}

pub(super) fn build_game_url(config: &LauncherConfig, request_host: Option<&str>) -> String {
    let viewer_host = resolve_runtime_host(config.viewer_host.as_str(), request_host);
    let viewer_port =
        parse_port(config.viewer_port.as_str(), "viewer port").unwrap_or(DEFAULT_VIEWER_PORT);
    let (web_host, web_port) = parse_host_port(config.web_bind.as_str(), "web bind")
        .unwrap_or((DEFAULT_VIEWER_HOST.to_string(), 5011));
    let web_host = resolve_runtime_host(web_host.as_str(), request_host);
    let viewer_host = host_for_url(viewer_host.as_str());
    let web_host = host_for_url(web_host.as_str());
    let ws_url = format!("ws://{web_host}:{web_port}");
    let hosted_access_hint = serde_json::to_string(&hosted_access::hosted_viewer_access_hint(
        deployment_mode_from_config(config),
    ))
    .unwrap_or_else(|_| "{}".to_string());
    format!(
        "http://{viewer_host}:{viewer_port}/?{}&{}&{}",
        encoded_query_pair("render_mode", "viewer"),
        encoded_query_pair("ws", ws_url.as_str()),
        encoded_query_pair("hosted_access", hosted_access_hint.as_str()),
    )
}

#[cfg(test)]
pub(super) fn validate_game_config(config: &LauncherConfig) -> Vec<String> {
    let launcher_bin = resolve_launcher_bin_from_config(config, config.launcher_bin.as_str());
    validate_game_config_with_launcher_bin(config, launcher_bin.as_str())
}

pub(super) fn validate_game_config_with_launcher_bin(
    config: &LauncherConfig,
    launcher_bin: &str,
) -> Vec<String> {
    let mut issues = Vec::new();
    if DeploymentMode::parse_user_facing(config.deployment_mode.as_str(), "deployment mode")
        .is_err()
    {
        issues.push(
            "deployment mode must be hosted_public_join; trusted_local_only has been removed from the user launcher flow".to_string(),
        );
    }
    if parse_host_port(config.live_bind.as_str(), "live bind").is_err() {
        issues.push("live bind must be in <host:port> format".to_string());
    }
    if parse_host_port(config.web_bind.as_str(), "web bind").is_err() {
        issues.push("web bind must be in <host:port> format".to_string());
    }
    if config.viewer_host.trim().is_empty() {
        issues.push("viewer host is required".to_string());
    }
    if parse_port(config.viewer_port.as_str(), "viewer port").is_err() {
        issues.push("viewer port must be integer in 1..=65535".to_string());
    }
    if !config.llm_enabled {
        issues.push(
            "llm must stay enabled because no-LLM is no longer a playable entry path".to_string(),
        );
    }
    collect_agent_provider_config_issues(config, &mut issues);

    let viewer_static_dir = config.viewer_static_dir.trim();
    if viewer_static_dir.is_empty() {
        issues.push("viewer static directory is required".to_string());
    } else if resolve_viewer_static_dir_for_launcher(viewer_static_dir, launcher_bin).is_none() {
        issues.push(format!(
            "viewer static directory does not exist or is not a directory: {viewer_static_dir}"
        ));
    }
    issues
}

pub(super) fn validate_chain_config(config: &LauncherConfig) -> Vec<String> {
    let mut issues = Vec::new();
    if !config.chain_enabled {
        return issues;
    }
    if parse_host_port(config.chain_status_bind.as_str(), "chain status bind").is_err() {
        issues.push("chain status bind must be in <host:port> format".to_string());
    }
    if config.chain_node_id.trim().is_empty() {
        issues.push("chain node id is required".to_string());
    }
    if canonical_chain_network_tier(config.chain_network_tier.as_str()).is_none() {
        issues.push(
            "chain runtime technical tier must be one of: local_devnet|public_testnet|mainnet"
                .to_string(),
        );
    }
    let network_tier_manifest = effective_chain_network_tier_manifest(config);
    let network_tier_manifest = network_tier_manifest.trim();
    if network_tier_manifest.is_empty() {
        if config
            .chain_storage_profile
            .parse::<StorageProfile>()
            .is_err()
        {
            issues.push(
                "chain storage profile must be one of: dev_local|release_default|soak_forensics"
                    .to_string(),
            );
        }
        if parse_chain_role(config.chain_node_role.as_str()).is_err() {
            issues.push("chain role must be one of: sequencer|storage|observer".to_string());
        }
    } else {
        if parse_chain_role(config.chain_node_role.as_str()).is_err() {
            issues.push("chain role must be one of: sequencer|storage|observer".to_string());
        }
        if let Err(err) =
            LoadedNetworkTierManifest::load(std::path::Path::new(network_tier_manifest))
                .and_then(|loaded| loaded.validate_runtime_support())
        {
            issues.push(format!("chain network tier manifest is invalid: {err}"));
        }
    }
    match parse_chain_p2p_user_mode(config.chain_p2p_user_mode.as_str()) {
        Ok(mode) => {
            if mode == "public_entry" && !config.chain_p2p_accept_public_entry {
                issues.push(
                    "public entry mode requires explicit confirmation via Accept Public Entry"
                        .to_string(),
                );
            }
        }
        Err(_) => {
            issues.push(
                "chain P2P user mode must be one of: auto_join|private_safe|public_entry"
                    .to_string(),
            );
        }
    }
    if parse_positive_u64(
        config.chain_node_tick_ms.as_str(),
        "chain node poll interval ms",
    )
    .is_err()
    {
        issues.push("chain node poll interval ms must be a positive integer".to_string());
    }
    if parse_positive_u64(
        config.chain_pos_slot_duration_ms.as_str(),
        "chain pos slot duration ms",
    )
    .is_err()
    {
        issues.push("chain pos slot duration ms must be a positive integer".to_string());
    }
    let ticks_per_slot = parse_positive_u64(
        config.chain_pos_ticks_per_slot.as_str(),
        "chain pos ticks per slot",
    );
    if ticks_per_slot.is_err() {
        issues.push("chain pos ticks per slot must be a positive integer".to_string());
    }
    let proposal_tick_phase = parse_non_negative_u64(
        config.chain_pos_proposal_tick_phase.as_str(),
        "chain pos proposal tick phase",
    );
    if proposal_tick_phase.is_err() {
        issues.push("chain pos proposal tick phase must be a non-negative integer".to_string());
    }
    if let (Ok(ticks_per_slot), Ok(proposal_tick_phase)) = (ticks_per_slot, proposal_tick_phase)
        && proposal_tick_phase >= ticks_per_slot
    {
        issues.push(
            "chain pos proposal tick phase must be less than chain pos ticks per slot".to_string(),
        );
    }
    if parse_optional_i64(
        config.chain_pos_slot_clock_genesis_unix_ms.as_str(),
        "chain pos slot clock genesis unix ms",
    )
    .is_err()
    {
        issues.push("chain pos slot clock genesis unix ms must be an integer or empty".to_string());
    }
    if parse_non_negative_u64(
        config.chain_pos_max_past_slot_lag.as_str(),
        "chain pos max past slot lag",
    )
    .is_err()
    {
        issues.push("chain pos max past slot lag must be a non-negative integer".to_string());
    }
    if parse_chain_validators(config.chain_node_validators.as_str()).is_err() {
        issues.push("chain validators must be in <validator_id:stake> format".to_string());
    }
    if parse_chain_replication_bootstrap_peers(config.chain_replication_bootstrap_peers.as_str())
        .is_err()
    {
        issues.push(
            "chain replication bootstrap peers must use multiaddr values like /dns4/bootstrap1.oasis7.tech/tcp/5612 or /ip4/203.0.113.10/tcp/4100/p2p/<peer-id>".to_string(),
        );
    }

    issues
}

pub(super) fn build_chain_runtime_args(config: &LauncherConfig) -> Result<Vec<String>, String> {
    if local_chain_runtime_is_blocked(config) {
        return Err(local_chain_runtime_blocked_error(config));
    }
    parse_host_port(config.chain_status_bind.as_str(), "chain status bind")?;
    let chain_node_id = config.chain_node_id.trim();
    if chain_node_id.is_empty() {
        return Err("chain node id cannot be empty".to_string());
    }
    let chain_role = effective_chain_runtime_role(config)?;
    let chain_p2p_user_mode = parse_chain_p2p_user_mode(config.chain_p2p_user_mode.as_str())?;
    if chain_p2p_user_mode == "public_entry" && !config.chain_p2p_accept_public_entry {
        return Err(
            "public entry mode requires explicit confirmation via Accept Public Entry".to_string(),
        );
    }
    let chain_tick_ms = parse_positive_u64(
        config.chain_node_tick_ms.as_str(),
        "chain node poll interval ms",
    )?;
    let pos_slot_duration_ms = parse_positive_u64(
        config.chain_pos_slot_duration_ms.as_str(),
        "chain pos slot duration ms",
    )?;
    let pos_ticks_per_slot = parse_positive_u64(
        config.chain_pos_ticks_per_slot.as_str(),
        "chain pos ticks per slot",
    )?;
    let pos_proposal_tick_phase = parse_non_negative_u64(
        config.chain_pos_proposal_tick_phase.as_str(),
        "chain pos proposal tick phase",
    )?;
    if pos_proposal_tick_phase >= pos_ticks_per_slot {
        return Err(format!(
            "chain pos proposal tick phase={} must be less than chain pos ticks per slot={}",
            pos_proposal_tick_phase, pos_ticks_per_slot
        ));
    }
    let pos_slot_clock_genesis_unix_ms = parse_optional_i64(
        config.chain_pos_slot_clock_genesis_unix_ms.as_str(),
        "chain pos slot clock genesis unix ms",
    )?;
    let pos_max_past_slot_lag = parse_non_negative_u64(
        config.chain_pos_max_past_slot_lag.as_str(),
        "chain pos max past slot lag",
    )?;
    let validators = parse_chain_validators(config.chain_node_validators.as_str())?;
    let replication_bootstrap_peers =
        parse_chain_replication_bootstrap_peers(config.chain_replication_bootstrap_peers.as_str())?;
    let execution_world_dir = chain_execution_world_dir(chain_node_id);

    let mut args = vec![
        "--node-id".to_string(),
        chain_node_id.to_string(),
        "--world-id".to_string(),
        resolve_chain_world_id(config),
        "--status-bind".to_string(),
        config.chain_status_bind.trim().to_string(),
        "--execution-world-dir".to_string(),
        execution_world_dir,
        "--node-role".to_string(),
        chain_role.clone(),
        "--p2p-user-mode".to_string(),
        chain_p2p_user_mode,
        "--node-tick-ms".to_string(),
        chain_tick_ms.to_string(),
        "--pos-slot-duration-ms".to_string(),
        pos_slot_duration_ms.to_string(),
        "--pos-ticks-per-slot".to_string(),
        pos_ticks_per_slot.to_string(),
        "--pos-proposal-tick-phase".to_string(),
        pos_proposal_tick_phase.to_string(),
        if config.chain_pos_adaptive_tick_scheduler_enabled {
            "--pos-adaptive-tick-scheduler".to_string()
        } else {
            "--pos-no-adaptive-tick-scheduler".to_string()
        },
        "--pos-max-past-slot-lag".to_string(),
        pos_max_past_slot_lag.to_string(),
    ];
    let network_tier_manifest = effective_chain_network_tier_manifest(config);
    if network_tier_manifest.trim().is_empty() {
        let storage_profile = config.chain_storage_profile.parse::<StorageProfile>()?;
        args.push("--storage-profile".to_string());
        args.push(storage_profile.as_str().to_string());
    } else {
        args.push("--network-tier-manifest".to_string());
        args.push(network_tier_manifest.trim().to_string());
    }
    args.push(if config.chain_p2p_accept_public_entry {
        "--p2p-accept-public-entry".to_string()
    } else {
        "--p2p-reject-public-entry".to_string()
    });
    if let Some(genesis) = pos_slot_clock_genesis_unix_ms {
        args.push("--pos-slot-clock-genesis-unix-ms".to_string());
        args.push(genesis.to_string());
    }
    for validator in validators {
        args.push("--node-validator".to_string());
        args.push(validator);
    }
    for peer in replication_bootstrap_peers {
        args.push("--replication-network-peer".to_string());
        args.push(peer);
    }
    if let Some(registry_path) =
        observer_registry_path_from_manifest(network_tier_manifest.as_str(), chain_role.as_str())
    {
        args.push("--genesis-validator-registry".to_string());
        args.push(registry_path);
    }
    Ok(args)
}

fn observer_registry_path_from_manifest(manifest_path: &str, chain_role: &str) -> Option<String> {
    if chain_role != "observer" {
        return None;
    }
    let manifest_path = Path::new(manifest_path.trim());
    let manifest_bytes = std::fs::read(manifest_path).ok()?;
    let manifest: serde_json::Value = serde_json::from_slice(manifest_bytes.as_slice()).ok()?;
    if manifest.get("tier").and_then(serde_json::Value::as_str) != Some("public_testnet")
        || manifest
            .get("validator_policy")
            .and_then(serde_json::Value::as_object)
            .and_then(|policy| policy.get("allow_observer_nodes"))
            != Some(&serde_json::Value::Bool(true))
    {
        return None;
    }
    let registry_ref = manifest
        .get("deployment_validator_registry")
        .and_then(serde_json::Value::as_object)
        .and_then(|binding| binding.get("ref"))
        .and_then(serde_json::Value::as_str)?;
    let registry_ref_path = Path::new(registry_ref);
    if registry_ref_path.is_absolute()
        || registry_ref_path.components().any(|component| {
            matches!(
                component,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
    {
        return None;
    }
    let registry_name = registry_ref_path.file_name()?.to_str()?;
    if registry_name.is_empty() {
        return None;
    }
    let registry_path = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(registry_name);
    registry_path
        .is_file()
        .then(|| registry_path.to_string_lossy().into_owned())
}

fn effective_chain_runtime_role(config: &LauncherConfig) -> Result<String, String> {
    let role = parse_chain_role(config.chain_node_role.as_str())?;
    if canonical_chain_network_tier(config.chain_network_tier.as_str()) == Some("public_testnet")
        && role == "sequencer"
    {
        // The web launcher has no managed-triad inventory input.  Keep its
        // public-testnet lane on the non-managed observer contract; managed
        // identities still fail closed in runtime authority preflight.
        return Ok("observer".to_string());
    }
    Ok(role)
}

#[cfg(test)]
#[path = "control_plane/observer_registry_launcher_tests.rs"]
mod observer_registry_launcher_tests;

pub(super) fn finalize_chain_start_outcome(
    state: &ServiceState,
    outcome: Result<(), String>,
) -> Result<(), String> {
    match outcome {
        Err(err) => Err(err),
        Ok(()) => match &state.chain_runtime_status {
            ChainRuntimeStatus::Disabled => Err("chain runtime is disabled".to_string()),
            ChainRuntimeStatus::ConfigError(detail)
            | ChainRuntimeStatus::StaleExecutionWorld(detail)
            | ChainRuntimeStatus::Unreachable(detail) => Err(detail.clone()),
            ChainRuntimeStatus::NotStarted if state.chain_running.is_none() => {
                Err("oasis7_chain_runtime did not remain running".to_string())
            }
            ChainRuntimeStatus::NotStarted
            | ChainRuntimeStatus::Starting
            | ChainRuntimeStatus::Ready => Ok(()),
        },
    }
}

pub(super) fn chain_error_code_for_state(state: &ServiceState, error: &str) -> &'static str {
    if matches!(
        state.chain_runtime_status,
        ChainRuntimeStatus::StaleExecutionWorld(_)
    ) || error
        .to_ascii_lowercase()
        .contains("latest state root mismatch")
    {
        "stale_execution_world"
    } else if error.contains("chain runtime is disabled") {
        "chain_disabled"
    } else if error.contains("proxy") {
        "proxy_error"
    } else {
        "action_failed"
    }
}

pub(super) fn chain_error_data_for_state(state: &ServiceState) -> Option<serde_json::Value> {
    state
        .chain_recovery
        .as_ref()
        .and_then(|value| serde_json::to_value(value).ok())
}

fn parse_chain_p2p_user_mode(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "auto_join" | "private_safe" | "public_entry" => Ok(normalized),
        _ => Err(
            "chain P2P user mode must be one of: auto_join, private_safe, public_entry".to_string(),
        ),
    }
}
