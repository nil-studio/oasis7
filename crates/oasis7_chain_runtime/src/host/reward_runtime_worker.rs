use oasis7::node_points_observation::measure_directory_storage_bytes;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use oasis7::observability::emit_stderr_or_event;
use oasis7_distfs::StorageChallengeProbeCursorState;
use oasis7_node::{NodeRole, NodeRuntime, PosConsensusStatus};
use oasis7_runtime::{
    Action as RuntimeAction, BlobStore, LocalCasStore, NodePointsConfig,
    NodePointsRuntimeCollector, NodePointsRuntimeCollectorSnapshot, NodePointsRuntimeHeuristics,
    ProtocolPowerReserve, RewardAssetConfig, RewardSignatureGovernancePolicy,
    World as RuntimeWorld,
};
use serde::Serialize;
use tracing::Level;

use super::distfs_probe_runtime::{
    DistfsProbeRuntimeConfig, collect_distfs_challenge_report_with_config,
    load_reward_runtime_distfs_probe_state, persist_reward_runtime_distfs_probe_state,
};
use super::reward_runtime_settlement::{
    auto_redeem_runtime_rewards, build_reward_settlement_mint_records,
};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RewardRuntimeMetricsSnapshot {
    pub enabled: bool,
    pub metrics_available: bool,
    pub report_dir: String,
    pub report_count: u64,
    pub latest_epoch_index: u64,
    pub latest_report_observed_at_unix_ms: i64,
    pub latest_total_distributed_points: u64,
    pub latest_minted_record_count: u64,
    pub cumulative_minted_record_count: u64,
    pub distfs_total_checks: u64,
    pub distfs_failed_checks: u64,
    pub distfs_failure_ratio: f64,
    pub settlement_apply_attempts_total: u64,
    pub settlement_apply_failures_total: u64,
    pub settlement_apply_failure_ratio: f64,
    pub invariant_ok: bool,
    pub last_error: Option<String>,
}

impl RewardRuntimeMetricsSnapshot {
    fn disabled(report_dir: &Path) -> Self {
        Self {
            enabled: false,
            metrics_available: false,
            report_dir: report_dir.display().to_string(),
            report_count: 0,
            latest_epoch_index: 0,
            latest_report_observed_at_unix_ms: 0,
            latest_total_distributed_points: 0,
            latest_minted_record_count: 0,
            cumulative_minted_record_count: 0,
            distfs_total_checks: 0,
            distfs_failed_checks: 0,
            distfs_failure_ratio: 0.0,
            settlement_apply_attempts_total: 0,
            settlement_apply_failures_total: 0,
            settlement_apply_failure_ratio: 0.0,
            invariant_ok: true,
            last_error: None,
        }
    }

    fn enabled(report_dir: &Path) -> Self {
        let mut snapshot = Self::disabled(report_dir);
        snapshot.enabled = true;
        snapshot
    }
}

pub(crate) type SharedRewardRuntimeMetrics = Arc<Mutex<RewardRuntimeMetricsSnapshot>>;

#[derive(Debug, Clone)]
pub(crate) struct RewardRuntimeWorkerConfig {
    pub enabled: bool,
    pub poll_interval: Duration,
    pub world_id: String,
    pub local_node_id: String,
    pub report_dir: PathBuf,
    pub state_path: PathBuf,
    pub distfs_probe_state_path: PathBuf,
    pub storage_root: PathBuf,
    pub signer_node_id: String,
    pub signer_private_key_hex: String,
    pub reward_runtime_epoch_duration_secs: Option<u64>,
    pub reward_runtime_auto_redeem: bool,
    pub reward_asset_config: RewardAssetConfig,
    pub reward_initial_reserve_power_units: i64,
    pub reward_runtime_node_identity_bindings: BTreeMap<String, String>,
    pub reward_distfs_probe_config: DistfsProbeRuntimeConfig,
}

#[derive(Debug)]
pub(crate) struct RewardRuntimeWorker {
    stop_tx: Sender<()>,
    error_rx: Receiver<String>,
    join_handle: Option<thread::JoinHandle<()>>,
}

pub(crate) fn init_shared_metrics(
    config: &RewardRuntimeWorkerConfig,
) -> SharedRewardRuntimeMetrics {
    let snapshot = if config.enabled {
        RewardRuntimeMetricsSnapshot::enabled(config.report_dir.as_path())
    } else {
        RewardRuntimeMetricsSnapshot::disabled(config.report_dir.as_path())
    };
    Arc::new(Mutex::new(snapshot))
}

pub(crate) fn snapshot_metrics(
    metrics: &SharedRewardRuntimeMetrics,
) -> RewardRuntimeMetricsSnapshot {
    match metrics.lock() {
        Ok(locked) => locked.clone(),
        Err(_) => RewardRuntimeMetricsSnapshot::disabled(Path::new("")),
    }
}

pub(crate) fn start_reward_runtime_worker(
    node_runtime: Arc<Mutex<NodeRuntime>>,
    config: RewardRuntimeWorkerConfig,
    metrics: SharedRewardRuntimeMetrics,
) -> Result<Option<RewardRuntimeWorker>, String> {
    if !config.enabled {
        return Ok(None);
    }

    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (error_tx, error_rx) = mpsc::channel::<String>();
    let join_handle = thread::Builder::new()
        .name("chain-reward-runtime".to_string())
        .spawn(move || {
            if let Err(err) = reward_runtime_loop(node_runtime, config, stop_rx, metrics) {
                let _ = error_tx.send(err);
            }
        })
        .map_err(|err| format!("failed to spawn reward runtime worker: {err}"))?;

    Ok(Some(RewardRuntimeWorker {
        stop_tx,
        error_rx,
        join_handle: Some(join_handle),
    }))
}

pub(crate) fn poll_worker_error(
    worker: &mut RewardRuntimeWorker,
) -> Result<Option<String>, String> {
    match worker.error_rx.try_recv() {
        Ok(err) => Ok(Some(format!("reward runtime worker failed: {err}"))),
        Err(TryRecvError::Disconnected) => {
            if let Some(handle) = worker.join_handle.as_ref()
                && handle.is_finished()
            {
                return Ok(Some(
                    "reward runtime worker exited unexpectedly".to_string(),
                ));
            }
            Ok(None)
        }
        Err(TryRecvError::Empty) => {
            if let Some(handle) = worker.join_handle.as_ref()
                && handle.is_finished()
            {
                return Ok(Some(
                    "reward runtime worker exited unexpectedly".to_string(),
                ));
            }
            Ok(None)
        }
    }
}

pub(crate) fn stop_reward_runtime_worker(worker: &mut Option<RewardRuntimeWorker>) {
    let Some(mut worker) = worker.take() else {
        return;
    };
    let _ = worker.stop_tx.send(());
    if let Some(handle) = worker.join_handle.take() {
        let _ = handle.join();
    }
}

fn reward_runtime_loop(
    node_runtime: Arc<Mutex<NodeRuntime>>,
    config: RewardRuntimeWorkerConfig,
    stop_rx: Receiver<()>,
    metrics: SharedRewardRuntimeMetrics,
) -> Result<(), String> {
    fs::create_dir_all(config.report_dir.as_path()).map_err(|err| {
        format!(
            "reward runtime create report dir {} failed: {}",
            config.report_dir.display(),
            err
        )
    })?;
    if let Err(err) = ensure_distfs_probe_seed_blob(
        config.storage_root.as_path(),
        config.world_id.as_str(),
        config.local_node_id.as_str(),
    ) {
        update_metrics_error(metrics.as_ref(), err.as_str());
    }

    let points_config = reward_runtime_points_config(config.reward_runtime_epoch_duration_secs);
    let mut collector = match load_reward_runtime_collector_snapshot(config.state_path.as_path()) {
        Ok(Some(mut restored)) => {
            if let Some(epoch_duration_secs) = config.reward_runtime_epoch_duration_secs {
                restored.ledger.config.epoch_duration_seconds = epoch_duration_secs;
            }
            NodePointsRuntimeCollector::from_snapshot(restored)
        }
        Ok(None) => NodePointsRuntimeCollector::new(
            points_config.clone(),
            NodePointsRuntimeHeuristics::default(),
        ),
        Err(err) => {
            emit_stderr_or_event(
                Level::WARN,
                format!("reward runtime load collector state failed: {err}").as_str(),
                "reward runtime collector state load failed",
            );
            NodePointsRuntimeCollector::new(points_config, NodePointsRuntimeHeuristics::default())
        }
    };
    let mut distfs_probe_state =
        match load_reward_runtime_distfs_probe_state(config.distfs_probe_state_path.as_path()) {
            Ok(state) => state,
            Err(err) => {
                emit_stderr_or_event(
                    Level::WARN,
                    format!("reward runtime load distfs probe state failed: {err}").as_str(),
                    "reward runtime distfs probe state load failed",
                );
                StorageChallengeProbeCursorState::default()
            }
        };

    let mut reward_world = oasis7::runtime_host::world::new_production_hardened();
    reward_world.set_reward_asset_config(config.reward_asset_config.clone());
    reward_world.set_reward_signature_governance_policy(RewardSignatureGovernancePolicy {
        require_mintsig_v2: true,
        allow_mintsig_v1_fallback: false,
        require_redeem_signature: true,
        require_redeem_signer_match_node_id: true,
    });
    reward_world.set_protocol_power_reserve(ProtocolPowerReserve {
        epoch_index: 0,
        available_power_units: config.reward_initial_reserve_power_units.max(0),
        redeemed_power_units: 0,
    });
    for (node_id, public_key_hex) in &config.reward_runtime_node_identity_bindings {
        if let Err(err) = reward_world.bind_node_identity(node_id.as_str(), public_key_hex.as_str())
        {
            emit_stderr_or_event(
                Level::WARN,
                format!(
                    "reward runtime bind node identity failed node={} err={:?}",
                    node_id, err
                )
                .as_str(),
                "reward runtime node identity binding failed",
            );
        }
    }

    let mut cumulative_distfs_total_checks: u64 = 0;
    let mut cumulative_distfs_failed_checks: u64 = 0;
    let mut settlement_apply_attempts_total: u64 = 0;
    let mut settlement_apply_failures_total: u64 = 0;
    let mut report_count: u64 = 0;
    let mut cumulative_minted_record_count: u64 = 0;
    let mut latest_epoch_index: u64 = 0;
    let mut latest_total_distributed_points: u64 = 0;
    let mut latest_minted_record_count: u64 = 0;
    let mut latest_report_observed_at_unix_ms: i64 = 0;

    loop {
        match stop_rx.recv_timeout(config.poll_interval) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }

        let snapshot = match node_runtime.lock() {
            Ok(locked) => locked.snapshot(),
            Err(_) => {
                return Err("reward runtime failed to read node snapshot: lock poisoned".into());
            }
        };
        let observed_at_unix_ms = super::now_unix_ms();
        let effective_storage_bytes =
            measure_directory_storage_bytes(config.storage_root.as_path());
        let mut observation =
            oasis7::node_points_observation::node_points_observation_from_snapshot(
                &snapshot,
                effective_storage_bytes,
                observed_at_unix_ms,
            );
        if snapshot.role == NodeRole::Storage {
            match collect_distfs_challenge_report_with_config(
                config.storage_root.as_path(),
                snapshot.world_id.as_str(),
                snapshot.node_id.as_str(),
                observed_at_unix_ms,
                &mut distfs_probe_state,
                &config.reward_distfs_probe_config,
            ) {
                Ok(report) => {
                    observation.storage_checks_passed = report.passed_checks;
                    observation.storage_checks_total = report.total_checks;
                    if report.failed_checks > 0 {
                        observation.has_error = true;
                    }
                    cumulative_distfs_total_checks =
                        cumulative_distfs_total_checks.saturating_add(report.total_checks);
                    cumulative_distfs_failed_checks =
                        cumulative_distfs_failed_checks.saturating_add(report.failed_checks);
                }
                Err(err) => {
                    update_metrics_error(metrics.as_ref(), err.as_str());
                }
            }
        }

        let maybe_report = match collector.observe(observation) {
            Ok(report) => report,
            Err(err) => {
                update_metrics_error(metrics.as_ref(), err.to_string().as_str());
                continue;
            }
        };

        if let Err(err) =
            persist_reward_runtime_collector_state(config.state_path.as_path(), &collector)
        {
            update_metrics_error(metrics.as_ref(), err.as_str());
        }
        if let Err(err) = persist_reward_runtime_distfs_probe_state(
            config.distfs_probe_state_path.as_path(),
            &distfs_probe_state,
        ) {
            update_metrics_error(metrics.as_ref(), err.as_str());
        }

        if let Some(report) = maybe_report {
            latest_epoch_index = report.epoch_index;
            latest_total_distributed_points = report.total_distributed_points;
            latest_report_observed_at_unix_ms = observed_at_unix_ms;
            rollover_reward_reserve_epoch(&mut reward_world, report.epoch_index);

            let minted_records = match build_reward_settlement_mint_records(
                &reward_world,
                &report,
                config.signer_node_id.as_str(),
                config.signer_private_key_hex.as_str(),
            ) {
                Ok(records) => records,
                Err(err) => {
                    update_metrics_error(
                        metrics.as_ref(),
                        format!("reward runtime settlement mint failed: {err:?}").as_str(),
                    );
                    continue;
                }
            };
            latest_minted_record_count = minted_records.len() as u64;
            cumulative_minted_record_count =
                cumulative_minted_record_count.saturating_add(latest_minted_record_count);

            if reward_runtime_consensus_ready_for_settlement(&snapshot) {
                settlement_apply_attempts_total = settlement_apply_attempts_total.saturating_add(1);
                reward_world.submit_action(RuntimeAction::ApplyNodePointsSettlementSigned {
                    report: report.clone(),
                    signer_node_id: config.signer_node_id.clone(),
                    mint_records: minted_records.clone(),
                });
                if let Err(err) = reward_world.step() {
                    settlement_apply_failures_total =
                        settlement_apply_failures_total.saturating_add(1);
                    update_metrics_error(
                        metrics.as_ref(),
                        format!("reward runtime apply settlement failed: {err:?}").as_str(),
                    );
                } else if config.reward_runtime_auto_redeem {
                    auto_redeem_runtime_rewards(
                        &mut reward_world,
                        minted_records.as_slice(),
                        config.signer_node_id.as_str(),
                        config.signer_private_key_hex.as_str(),
                    );
                }
            }

            report_count = report_count.saturating_add(1);
            let payload = serde_json::json!({
                "observed_at_unix_ms": observed_at_unix_ms,
                "world_id": config.world_id,
                "node_id": config.local_node_id,
                "node_snapshot": {
                    "node_id": snapshot.node_id,
                    "role": snapshot.role.as_str(),
                    "consensus": {
                        "epoch": snapshot.consensus.epoch,
                        "committed_height": snapshot.consensus.committed_height,
                        "network_committed_height": snapshot.consensus.network_committed_height,
                        "last_status": snapshot.consensus.last_status.map(|status| format!("{status:?}")),
                    }
                },
                "settlement_report": report,
                "minted_records": minted_records,
                "reward_settlement_transport": {
                    "settlement_apply_attempts_total": settlement_apply_attempts_total,
                    "settlement_apply_failures_total": settlement_apply_failures_total,
                    "settlement_apply_failure_ratio": if settlement_apply_attempts_total > 0 {
                        settlement_apply_failures_total as f64 / settlement_apply_attempts_total as f64
                    } else {
                        0.0
                    }
                },
                "distfs": {
                    "total_checks": cumulative_distfs_total_checks,
                    "failed_checks": cumulative_distfs_failed_checks,
                    "failure_ratio": if cumulative_distfs_total_checks > 0 {
                        cumulative_distfs_failed_checks as f64 / cumulative_distfs_total_checks as f64
                    } else {
                        0.0
                    }
                }
            });
            let report_path = config
                .report_dir
                .join(format!("epoch-{}.json", latest_epoch_index));
            if let Ok(bytes) = serde_json::to_vec_pretty(&payload) {
                let _ = fs::write(report_path, bytes);
            }
        }

        let invariant_ok = reward_world.reward_asset_invariant_report().is_ok();
        update_metrics_snapshot(
            metrics.as_ref(),
            report_count,
            latest_epoch_index,
            latest_report_observed_at_unix_ms,
            latest_total_distributed_points,
            latest_minted_record_count,
            cumulative_minted_record_count,
            cumulative_distfs_total_checks,
            cumulative_distfs_failed_checks,
            settlement_apply_attempts_total,
            settlement_apply_failures_total,
            invariant_ok,
        );
    }

    Ok(())
}

fn update_metrics_error(metrics: &Mutex<RewardRuntimeMetricsSnapshot>, err: &str) {
    if let Ok(mut locked) = metrics.lock() {
        locked.last_error = Some(err.to_string());
    }
}

#[allow(clippy::too_many_arguments)]
fn update_metrics_snapshot(
    metrics: &Mutex<RewardRuntimeMetricsSnapshot>,
    report_count: u64,
    latest_epoch_index: u64,
    latest_report_observed_at_unix_ms: i64,
    latest_total_distributed_points: u64,
    latest_minted_record_count: u64,
    cumulative_minted_record_count: u64,
    distfs_total_checks: u64,
    distfs_failed_checks: u64,
    settlement_apply_attempts_total: u64,
    settlement_apply_failures_total: u64,
    invariant_ok: bool,
) {
    if let Ok(mut locked) = metrics.lock() {
        locked.metrics_available = report_count > 0;
        locked.report_count = report_count;
        locked.latest_epoch_index = latest_epoch_index;
        locked.latest_report_observed_at_unix_ms = latest_report_observed_at_unix_ms;
        locked.latest_total_distributed_points = latest_total_distributed_points;
        locked.latest_minted_record_count = latest_minted_record_count;
        locked.cumulative_minted_record_count = cumulative_minted_record_count;
        locked.distfs_total_checks = distfs_total_checks;
        locked.distfs_failed_checks = distfs_failed_checks;
        locked.distfs_failure_ratio = if distfs_total_checks > 0 {
            distfs_failed_checks as f64 / distfs_total_checks as f64
        } else {
            0.0
        };
        locked.settlement_apply_attempts_total = settlement_apply_attempts_total;
        locked.settlement_apply_failures_total = settlement_apply_failures_total;
        locked.settlement_apply_failure_ratio = if settlement_apply_attempts_total > 0 {
            settlement_apply_failures_total as f64 / settlement_apply_attempts_total as f64
        } else {
            0.0
        };
        locked.invariant_ok = invariant_ok;
        locked.last_error = None;
    }
}

fn reward_runtime_consensus_ready_for_settlement(snapshot: &oasis7_node::NodeSnapshot) -> bool {
    if matches!(
        snapshot.consensus.last_status,
        Some(PosConsensusStatus::Committed)
    ) {
        return true;
    }
    snapshot.consensus.committed_height > 0
        && snapshot.consensus.network_committed_height >= snapshot.consensus.committed_height
}

fn reward_runtime_points_config(epoch_duration_secs_override: Option<u64>) -> NodePointsConfig {
    let mut config = NodePointsConfig::default();
    if let Some(epoch_duration_secs) = epoch_duration_secs_override {
        config.epoch_duration_seconds = epoch_duration_secs;
    }
    config
}

fn rollover_reward_reserve_epoch(reward_world: &mut RuntimeWorld, epoch_index: u64) {
    let current = reward_world.protocol_power_reserve().clone();
    if current.epoch_index == epoch_index {
        return;
    }
    reward_world.set_protocol_power_reserve(ProtocolPowerReserve {
        epoch_index,
        available_power_units: current.available_power_units,
        redeemed_power_units: 0,
    });
}

fn load_reward_runtime_collector_snapshot(
    path: &Path,
) -> Result<Option<NodePointsRuntimeCollectorSnapshot>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(path)
        .map_err(|err| format!("read collector state {} failed: {}", path.display(), err))?;
    let snapshot: NodePointsRuntimeCollectorSnapshot = serde_json::from_slice(bytes.as_slice())
        .map_err(|err| format!("parse collector state {} failed: {}", path.display(), err))?;
    Ok(Some(snapshot))
}

fn persist_reward_runtime_collector_state(
    path: &Path,
    collector: &NodePointsRuntimeCollector,
) -> Result<(), String> {
    let snapshot = collector.snapshot();
    let bytes = serde_json::to_vec_pretty(&snapshot)
        .map_err(|err| format!("serialize collector state failed: {}", err))?;
    super::write_bytes_atomic(path, bytes.as_slice())
}

fn ensure_distfs_probe_seed_blob(
    storage_root: &Path,
    world_id: &str,
    node_id: &str,
) -> Result<(), String> {
    let store = LocalCasStore::new(storage_root);
    let existing_hashes = store
        .list_blob_hashes()
        .map_err(|err| format!("list distfs blobs failed: {err:?}"))?;
    if !existing_hashes.is_empty() {
        return Ok(());
    }
    let seed = format!(
        "reward-runtime-distfs-seed:v1 world_id={} node_id={}",
        world_id, node_id
    );
    store
        .put_bytes(seed.as_bytes())
        .map(|_| ())
        .map_err(|err| format!("write distfs seed blob failed: {err:?}"))
}

#[cfg(test)]
mod tests {
    use super::ensure_distfs_probe_seed_blob;
    use oasis7_runtime::LocalCasStore;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("duration")
            .as_nanos();
        std::env::temp_dir().join(format!("oasis7-reward-runtime-worker-{prefix}-{unique}"))
    }

    #[test]
    fn ensure_distfs_probe_seed_blob_populates_empty_store_once() {
        let dir = temp_dir("seed");
        ensure_distfs_probe_seed_blob(dir.as_path(), "world-a", "node-a")
            .expect("seed empty store");
        let store = LocalCasStore::new(dir.as_path());
        let hashes_after_first = store.list_blob_hashes().expect("list after first");
        assert_eq!(hashes_after_first.len(), 1);

        ensure_distfs_probe_seed_blob(dir.as_path(), "world-a", "node-a")
            .expect("seed non-empty store");
        let hashes_after_second = store.list_blob_hashes().expect("list after second");
        assert_eq!(hashes_after_second.len(), 1);

        let _ = std::fs::remove_dir_all(dir);
    }
}
