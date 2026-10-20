use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use oasis7_runtime::ModuleTickRoutingMetricsSnapshot;

use super::ExecutionBridgeRecord;

const EXECUTION_BRIDGE_STAGE_WARN_THRESHOLD: Duration = Duration::from_millis(500);
const EXECUTION_BRIDGE_TOTAL_WARN_THRESHOLD: Duration = Duration::from_millis(1_000);
const EXECUTION_BRIDGE_COMMIT_TIMING_WINDOW: usize = 128;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct SimulatorMirrorCommitObservation {
    pub(super) action_count: usize,
    pub(super) rejected_action_count: usize,
    pub(super) snapshot_bytes: usize,
    pub(super) journal_bytes: usize,
}

pub(super) struct CommitObservation<'a> {
    pub(super) world_id: &'a str,
    pub(super) height: u64,
    pub(super) action_count: usize,
    pub(super) runtime_action_count: usize,
    pub(super) simulator: SimulatorMirrorCommitObservation,
    pub(super) snapshot_bytes: usize,
    pub(super) journal_bytes: usize,
    pub(super) decode_ms: Duration,
    pub(super) runtime_step_ms: Duration,
    pub(super) simulator_step_ms: Duration,
    pub(super) serialize_ms: Duration,
    pub(super) cas_put_ms: Duration,
    pub(super) simulator_persist_ms: Duration,
    pub(super) world_head_proof_ms: Duration,
    pub(super) record_persist_ms: Duration,
    pub(super) persist_world_ms: Duration,
    pub(super) checkpoint_ms: Duration,
    pub(super) retention_ms: Duration,
    pub(super) total_ms: Duration,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub(crate) struct ExecutionBridgeStageTimingSnapshot {
    pub(crate) count: u64,
    pub(crate) cumulative_ms: u64,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub(crate) struct ExecutionBridgeCommitTimingSnapshot {
    pub(crate) window_capacity: usize,
    pub(crate) recent_commit_count: usize,
    pub(crate) recent_over_budget_count: u64,
    pub(crate) recent_over_budget_ratio_ppm: u64,
    pub(crate) p50_total_ms: Option<u64>,
    pub(crate) p95_total_ms: Option<u64>,
    /// The newest commit sample, retained independently from the sorted window.
    pub(crate) latest_total_ms: Option<u64>,
    pub(crate) max_total_ms: Option<u64>,
    pub(crate) slow_count: u64,
    pub(crate) last_slow_stage: Option<String>,
    pub(crate) stages: BTreeMap<String, ExecutionBridgeStageTimingSnapshot>,
}

#[derive(Debug, Default)]
struct ExecutionBridgeCommitTimingState {
    recent_total_ms: VecDeque<u64>,
    slow_count: u64,
    last_slow_stage: Option<String>,
    stages: BTreeMap<&'static str, ExecutionBridgeStageTimingCounter>,
    #[cfg(not(test))]
    module_tick_routing_metrics: Option<ModuleTickRoutingMetricsSnapshot>,
}

#[cfg(test)]
std::thread_local! {
    // Parallel tests represent independent drivers; do not share their live
    // routing samples. Production retains process-wide publication below.
    static MODULE_TICK_ROUTING_METRICS: std::cell::RefCell<Option<ModuleTickRoutingMetricsSnapshot>> =
        const { std::cell::RefCell::new(None) };
}

#[derive(Debug, Default)]
struct ExecutionBridgeStageTimingCounter {
    count: u64,
    cumulative_ms: u64,
}

pub(super) struct RestoreObservation<'a> {
    pub(super) world_id: &'a str,
    pub(super) height: u64,
    pub(super) pinned_ref_count: usize,
    pub(super) blob_count: usize,
    pub(super) bundle_bytes: usize,
    pub(super) snapshot_bytes: usize,
    pub(super) journal_bytes: usize,
    pub(super) simulator_mirror_present: bool,
    pub(super) simulator_snapshot_bytes: usize,
    pub(super) simulator_journal_bytes: usize,
    pub(super) blob_store_ms: Duration,
    pub(super) decode_ms: Duration,
    pub(super) rebuild_ms: Duration,
    pub(super) simulator_restore_ms: Duration,
    pub(super) persist_ms: Duration,
    pub(super) total_ms: Duration,
}

pub(super) struct CheckpointInstallObservation<'a> {
    pub(super) world_id: &'a str,
    pub(super) height: u64,
    pub(super) checkpoint_id: &'a str,
    pub(super) blob_count: usize,
    pub(super) bundle_bytes: usize,
    pub(super) pinned_ref_count: usize,
    pub(super) snapshot_bytes: usize,
    pub(super) journal_bytes: usize,
    pub(super) blob_store_ms: Duration,
    pub(super) pin_check_ms: Duration,
    pub(super) decode_ms: Duration,
    pub(super) rebuild_ms: Duration,
    pub(super) persist_ms: Duration,
    pub(super) retention_ms: Duration,
    pub(super) total_ms: Duration,
}

pub(super) fn execution_record_recovery_ref_count(record: &ExecutionBridgeRecord) -> usize {
    let mut refs = BTreeSet::new();
    refs.extend(record.latest_state_ref.iter().cloned());
    refs.extend(record.snapshot_ref.iter().cloned());
    refs.extend(record.journal_ref.iter().cloned());
    if let Some(simulator_mirror) = record.simulator_mirror.as_ref() {
        refs.insert(simulator_mirror.snapshot_ref.clone());
        refs.insert(simulator_mirror.journal_ref.clone());
    }
    refs.len()
}

pub(super) fn emit_stale_height_restore_start(
    world_id: &str,
    height: u64,
    pinned_ref_count: usize,
    simulator_mirror_present: bool,
) {
    emit_execution_bridge_observation(
        tracing::Level::INFO,
        format!(
            "execution bridge stale-height restore start world_id={world_id} height={height} pinned_ref_count={pinned_ref_count} simulator_mirror_present={simulator_mirror_present}"
        ),
        "execution bridge stale-height restore start",
    );
}

pub(super) fn emit_stale_height_restore_complete(observation: RestoreObservation<'_>) {
    let level = level_for_execution_bridge_sample(
        observation.total_ms,
        &[
            observation.blob_store_ms,
            observation.decode_ms,
            observation.rebuild_ms,
            observation.simulator_restore_ms,
            observation.persist_ms,
        ],
    );
    emit_execution_bridge_observation(
        info_or_warn(level),
        format!(
            "execution bridge stale-height restore complete world_id={} height={} pinned_ref_count={} blob_count={} bundle_bytes={} snapshot_bytes={} journal_bytes={} simulator_mirror_present={} simulator_snapshot_bytes={} simulator_journal_bytes={} blob_store_ms={} decode_ms={} rebuild_ms={} simulator_restore_ms={} persist_ms={} total_ms={}",
            observation.world_id,
            observation.height,
            observation.pinned_ref_count,
            observation.blob_count,
            observation.bundle_bytes,
            observation.snapshot_bytes,
            observation.journal_bytes,
            observation.simulator_mirror_present,
            observation.simulator_snapshot_bytes,
            observation.simulator_journal_bytes,
            duration_ms(observation.blob_store_ms),
            duration_ms(observation.decode_ms),
            duration_ms(observation.rebuild_ms),
            duration_ms(observation.simulator_restore_ms),
            duration_ms(observation.persist_ms),
            duration_ms(observation.total_ms)
        ),
        "execution bridge stale-height restore complete",
    );
}

pub(super) fn emit_commit_observation(observation: CommitObservation<'_>) {
    let level = level_for_execution_bridge_sample(
        observation.total_ms,
        &[
            observation.decode_ms,
            observation.runtime_step_ms,
            observation.simulator_step_ms,
            observation.serialize_ms,
            observation.cas_put_ms,
            observation.simulator_persist_ms,
            observation.world_head_proof_ms,
            observation.record_persist_ms,
            observation.persist_world_ms,
            observation.checkpoint_ms,
            observation.retention_ms,
        ],
    );
    record_commit_timing_observation(&observation, level);
    let message = format!(
        "execution bridge commit observation world_id={} height={} action_count={} runtime_action_count={} simulator_action_count={} simulator_rejected_action_count={} snapshot_bytes={} journal_bytes={} simulator_snapshot_bytes={} simulator_journal_bytes={} decode_ms={} runtime_step_ms={} simulator_step_ms={} serialize_ms={} cas_put_ms={} simulator_persist_ms={} world_head_proof_ms={} record_persist_ms={} persist_world_ms={} checkpoint_ms={} retention_ms={} total_ms={}",
        observation.world_id,
        observation.height,
        observation.action_count,
        observation.runtime_action_count,
        observation.simulator.action_count,
        observation.simulator.rejected_action_count,
        observation.snapshot_bytes,
        observation.journal_bytes,
        observation.simulator.snapshot_bytes,
        observation.simulator.journal_bytes,
        duration_ms(observation.decode_ms),
        duration_ms(observation.runtime_step_ms),
        duration_ms(observation.simulator_step_ms),
        duration_ms(observation.serialize_ms),
        duration_ms(observation.cas_put_ms),
        duration_ms(observation.simulator_persist_ms),
        duration_ms(observation.world_head_proof_ms),
        duration_ms(observation.record_persist_ms),
        duration_ms(observation.persist_world_ms),
        duration_ms(observation.checkpoint_ms),
        duration_ms(observation.retention_ms),
        duration_ms(observation.total_ms)
    );
    if level == tracing::Level::WARN {
        emit_execution_bridge_observation(
            tracing::Level::WARN,
            message,
            "execution bridge commit observation",
        );
    } else {
        tracing::debug!(message = %message, "execution bridge commit observation");
    }
}

pub(crate) fn snapshot_execution_bridge_commit_timing() -> ExecutionBridgeCommitTimingSnapshot {
    commit_timing_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .snapshot()
}

/// Returns the process-local module tick routing measurements from the active
/// execution driver. These wall-clock measurements intentionally never enter
/// the deterministic world snapshot.
#[allow(dead_code)]
pub(crate) fn snapshot_execution_bridge_module_tick_routing_metrics()
-> Option<ModuleTickRoutingMetricsSnapshot> {
    #[cfg(test)]
    {
        MODULE_TICK_ROUTING_METRICS.with(|metrics| metrics.borrow().clone())
    }
    #[cfg(not(test))]
    {
        commit_timing_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .module_tick_routing_metrics
            .clone()
    }
}

pub(crate) fn record_execution_bridge_module_tick_routing_metrics(
    metrics: ModuleTickRoutingMetricsSnapshot,
) {
    #[cfg(test)]
    MODULE_TICK_ROUTING_METRICS.with(|state| *state.borrow_mut() = Some(metrics));
    #[cfg(not(test))]
    {
        commit_timing_state()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .module_tick_routing_metrics = Some(metrics);
    }
}

fn record_commit_timing_observation(observation: &CommitObservation<'_>, level: tracing::Level) {
    let mut state = commit_timing_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.record(observation, level);
}

#[cfg(test)]
pub(super) fn snapshot_commit_observations_for_tests(
    observations: &[CommitObservation<'_>],
) -> ExecutionBridgeCommitTimingSnapshot {
    let mut state = ExecutionBridgeCommitTimingState::default();
    for observation in observations {
        let level = level_for_execution_bridge_sample(
            observation.total_ms,
            &[
                observation.decode_ms,
                observation.runtime_step_ms,
                observation.simulator_step_ms,
                observation.serialize_ms,
                observation.cas_put_ms,
                observation.simulator_persist_ms,
                observation.world_head_proof_ms,
                observation.record_persist_ms,
                observation.persist_world_ms,
                observation.checkpoint_ms,
                observation.retention_ms,
            ],
        );
        state.record(observation, level);
    }
    state.snapshot()
}

fn commit_timing_state() -> &'static Mutex<ExecutionBridgeCommitTimingState> {
    static STATE: OnceLock<Mutex<ExecutionBridgeCommitTimingState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(ExecutionBridgeCommitTimingState::default()))
}

impl ExecutionBridgeCommitTimingState {
    fn record(&mut self, observation: &CommitObservation<'_>, level: tracing::Level) {
        if self.recent_total_ms.len() == EXECUTION_BRIDGE_COMMIT_TIMING_WINDOW {
            self.recent_total_ms.pop_front();
        }
        self.recent_total_ms
            .push_back(duration_ms_u64(observation.total_ms));
        if level == tracing::Level::WARN {
            self.slow_count = self.slow_count.saturating_add(1);
            self.last_slow_stage = slowest_stage_name(observation).map(str::to_string);
        }
        for (stage, duration) in commit_stage_durations(observation) {
            let counter = self.stages.entry(stage).or_default();
            counter.count = counter.count.saturating_add(1);
            counter.cumulative_ms = counter
                .cumulative_ms
                .saturating_add(duration_ms_u64(duration));
        }
    }

    fn snapshot(&self) -> ExecutionBridgeCommitTimingSnapshot {
        let mut samples: Vec<u64> = self.recent_total_ms.iter().copied().collect();
        let recent_over_budget_count = samples
            .iter()
            .filter(|&&total_ms| total_ms >= duration_ms_u64(EXECUTION_BRIDGE_TOTAL_WARN_THRESHOLD))
            .count() as u64;
        let recent_over_budget_ratio_ppm = if samples.is_empty() {
            0
        } else {
            recent_over_budget_count.saturating_mul(1_000_000) / samples.len() as u64
        };
        samples.sort_unstable();
        ExecutionBridgeCommitTimingSnapshot {
            window_capacity: EXECUTION_BRIDGE_COMMIT_TIMING_WINDOW,
            recent_commit_count: samples.len(),
            recent_over_budget_count,
            recent_over_budget_ratio_ppm,
            p50_total_ms: percentile_ms(samples.as_slice(), 50),
            p95_total_ms: percentile_ms(samples.as_slice(), 95),
            latest_total_ms: self.recent_total_ms.back().copied(),
            max_total_ms: samples.last().copied(),
            slow_count: self.slow_count,
            last_slow_stage: self.last_slow_stage.clone(),
            stages: self
                .stages
                .iter()
                .map(|(stage, counter)| {
                    (
                        (*stage).to_string(),
                        ExecutionBridgeStageTimingSnapshot {
                            count: counter.count,
                            cumulative_ms: counter.cumulative_ms,
                        },
                    )
                })
                .collect(),
        }
    }
}

fn percentile_ms(samples: &[u64], percentile: usize) -> Option<u64> {
    if samples.is_empty() {
        return None;
    }
    let rank = (samples.len() * percentile).div_ceil(100).saturating_sub(1);
    samples.get(rank).copied()
}

fn commit_stage_durations(observation: &CommitObservation<'_>) -> [(&'static str, Duration); 11] {
    [
        ("decode", observation.decode_ms),
        ("runtime_step", observation.runtime_step_ms),
        ("simulator_step", observation.simulator_step_ms),
        ("serialize", observation.serialize_ms),
        ("cas_put", observation.cas_put_ms),
        ("simulator_persist", observation.simulator_persist_ms),
        ("world_head_proof", observation.world_head_proof_ms),
        ("record_persist", observation.record_persist_ms),
        ("persist_world", observation.persist_world_ms),
        ("checkpoint", observation.checkpoint_ms),
        ("retention", observation.retention_ms),
    ]
}

fn slowest_stage_name(observation: &CommitObservation<'_>) -> Option<&'static str> {
    commit_stage_durations(observation)
        .into_iter()
        .max_by_key(|(_, duration)| *duration)
        .map(|(stage, _)| stage)
}

pub(super) fn emit_checkpoint_bundle_install_start(
    world_id: &str,
    height: u64,
    checkpoint_id: &str,
    blob_count: usize,
    bundle_bytes: usize,
    pinned_ref_count: usize,
) {
    emit_execution_bridge_observation(
        tracing::Level::INFO,
        format!(
            "execution checkpoint bundle install start world_id={world_id} height={height} checkpoint_id={checkpoint_id} blob_count={blob_count} bundle_bytes={bundle_bytes} pinned_ref_count={pinned_ref_count} simulator_mirror_present=false"
        ),
        "execution checkpoint bundle install start",
    );
}

pub(super) fn emit_checkpoint_bundle_install_complete(
    observation: CheckpointInstallObservation<'_>,
) {
    let level = level_for_execution_bridge_sample(
        observation.total_ms,
        &[
            observation.blob_store_ms,
            observation.pin_check_ms,
            observation.decode_ms,
            observation.rebuild_ms,
            observation.persist_ms,
            observation.retention_ms,
        ],
    );
    emit_execution_bridge_observation(
        info_or_warn(level),
        format!(
            "execution checkpoint bundle install complete world_id={} height={} checkpoint_id={} blob_count={} bundle_bytes={} pinned_ref_count={} snapshot_bytes={} journal_bytes={} simulator_mirror_present=false blob_store_ms={} pin_check_ms={} decode_ms={} rebuild_ms={} persist_ms={} retention_ms={} total_ms={}",
            observation.world_id,
            observation.height,
            observation.checkpoint_id,
            observation.blob_count,
            observation.bundle_bytes,
            observation.pinned_ref_count,
            observation.snapshot_bytes,
            observation.journal_bytes,
            duration_ms(observation.blob_store_ms),
            duration_ms(observation.pin_check_ms),
            duration_ms(observation.decode_ms),
            duration_ms(observation.rebuild_ms),
            duration_ms(observation.persist_ms),
            duration_ms(observation.retention_ms),
            duration_ms(observation.total_ms)
        ),
        "execution checkpoint bundle install complete",
    );
}

fn duration_ms(duration: Duration) -> u128 {
    duration.as_millis()
}

fn duration_ms_u64(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn max_duration(durations: &[Duration]) -> Duration {
    durations.iter().copied().max().unwrap_or_default()
}

fn level_for_execution_bridge_sample(total: Duration, stages: &[Duration]) -> tracing::Level {
    if total >= EXECUTION_BRIDGE_TOTAL_WARN_THRESHOLD
        || max_duration(stages) >= EXECUTION_BRIDGE_STAGE_WARN_THRESHOLD
    {
        tracing::Level::WARN
    } else {
        tracing::Level::DEBUG
    }
}

fn info_or_warn(level: tracing::Level) -> tracing::Level {
    if level == tracing::Level::WARN {
        tracing::Level::WARN
    } else {
        tracing::Level::INFO
    }
}

fn emit_execution_bridge_observation(
    level: tracing::Level,
    stderr_message: String,
    event_message: &'static str,
) {
    oasis7::observability::emit_stderr_or_event(level, stderr_message.as_str(), event_message);
}

#[cfg(test)]
pub(crate) fn reset_execution_bridge_commit_timing_for_tests() {
    MODULE_TICK_ROUTING_METRICS.with(|metrics| *metrics.borrow_mut() = None);
    *commit_timing_state()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) =
        ExecutionBridgeCommitTimingState::default();
}
