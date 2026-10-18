use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeMap;

const DEFAULT_WEIGHT_COMPUTE: f64 = 0.45;
const DEFAULT_WEIGHT_STORAGE: f64 = 0.35;
const DEFAULT_WEIGHT_UPTIME: f64 = 0.10;
const DEFAULT_WEIGHT_RELIABILITY: f64 = 0.10;
const DEFAULT_MIN_UPTIME_CHALLENGE_PASS_RATIO: f64 = 0.85;
const DEFAULT_MIN_STORAGE_CHALLENGE_PASS_RATIO: f64 = 0.85;
const DEFAULT_MIN_STORAGE_CHALLENGE_CHECKS: u64 = 1;
const BYTES_PER_GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Node points settlement configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodePointsConfig {
    pub epoch_duration_seconds: u64,
    pub epoch_pool_points: u64,
    pub storage_pool_points: u64,
    pub min_self_sim_compute_units: u64,
    pub min_uptime_challenge_pass_ratio: f64,
    pub min_storage_challenge_pass_ratio: f64,
    pub min_storage_challenge_checks: u64,
    pub max_rewardable_storage_to_staked_ratio: f64,
    pub delegated_compute_multiplier: f64,
    pub maintenance_compute_multiplier: f64,
    pub weight_compute: f64,
    pub weight_storage: f64,
    pub weight_uptime: f64,
    pub weight_reliability: f64,
    pub obligation_penalty_points: f64,
}

impl Default for NodePointsConfig {
    fn default() -> Self {
        Self {
            epoch_duration_seconds: 3600,
            epoch_pool_points: 1000,
            storage_pool_points: 0,
            min_self_sim_compute_units: 1,
            min_uptime_challenge_pass_ratio: DEFAULT_MIN_UPTIME_CHALLENGE_PASS_RATIO,
            min_storage_challenge_pass_ratio: DEFAULT_MIN_STORAGE_CHALLENGE_PASS_RATIO,
            min_storage_challenge_checks: DEFAULT_MIN_STORAGE_CHALLENGE_CHECKS,
            max_rewardable_storage_to_staked_ratio: 0.0,
            delegated_compute_multiplier: 1.0,
            maintenance_compute_multiplier: 1.2,
            weight_compute: DEFAULT_WEIGHT_COMPUTE,
            weight_storage: DEFAULT_WEIGHT_STORAGE,
            weight_uptime: DEFAULT_WEIGHT_UPTIME,
            weight_reliability: DEFAULT_WEIGHT_RELIABILITY,
            obligation_penalty_points: 5.0,
        }
    }
}

impl NodePointsConfig {
    fn normalized_weights(&self) -> (f64, f64, f64, f64) {
        let wc = self.weight_compute.max(0.0);
        let ws = if self.storage_pool_points > 0 {
            0.0
        } else {
            self.weight_storage.max(0.0)
        };
        let wu = self.weight_uptime.max(0.0);
        let wr = self.weight_reliability.max(0.0);
        let sum = wc + ws + wu + wr;
        if sum <= f64::EPSILON {
            return (
                DEFAULT_WEIGHT_COMPUTE,
                DEFAULT_WEIGHT_STORAGE,
                DEFAULT_WEIGHT_UPTIME,
                DEFAULT_WEIGHT_RELIABILITY,
            );
        }
        (wc / sum, ws / sum, wu / sum, wr / sum)
    }
}

/// A node contribution sample collected within one epoch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeContributionSample {
    pub node_id: String,
    pub self_sim_compute_units: u64,
    pub delegated_sim_compute_units: u64,
    pub world_maintenance_compute_units: u64,
    pub effective_storage_bytes: u64,
    pub uptime_seconds: u64,
    pub uptime_valid_checks: u64,
    pub uptime_total_checks: u64,
    pub storage_valid_checks: u64,
    pub storage_total_checks: u64,
    pub staked_storage_bytes: u64,
    pub verify_pass_ratio: f64,
    pub availability_ratio: f64,
    pub explicit_penalty_points: f64,
}

pub use oasis7_agent_api::world_action::NodeSettlement;

pub use oasis7_agent_api::world_action::EpochSettlementReport;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodePointsError {
    pub reason: String,
}

impl std::fmt::Display for NodePointsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.reason.as_str())
    }
}

impl std::error::Error for NodePointsError {}

/// Serializable snapshot for restoring a node points ledger.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodePointsLedgerSnapshot {
    pub config: NodePointsConfig,
    pub epoch_index: u64,
    pub cumulative_points: BTreeMap<String, u64>,
}

#[derive(Debug, Clone)]
struct RemainderEntry {
    settlement_index: usize,
    node_id: String,
    fractional: f64,
}

/// In-memory node points ledger.
#[derive(Debug, Clone)]
pub struct NodePointsLedger {
    config: NodePointsConfig,
    epoch_index: u64,
    cumulative_points: BTreeMap<String, u64>,
}

impl NodePointsLedger {
    pub fn new(config: NodePointsConfig) -> Self {
        Self {
            config,
            epoch_index: 0,
            cumulative_points: BTreeMap::new(),
        }
    }

    pub fn from_snapshot(snapshot: NodePointsLedgerSnapshot) -> Self {
        Self {
            config: snapshot.config,
            epoch_index: snapshot.epoch_index,
            cumulative_points: snapshot.cumulative_points,
        }
    }

    pub fn config(&self) -> &NodePointsConfig {
        &self.config
    }

    pub fn epoch_index(&self) -> u64 {
        self.epoch_index
    }

    pub fn cumulative_points(&self, node_id: &str) -> u64 {
        *self.cumulative_points.get(node_id).unwrap_or(&0)
    }

    pub fn snapshot(&self) -> NodePointsLedgerSnapshot {
        NodePointsLedgerSnapshot {
            config: self.config.clone(),
            epoch_index: self.epoch_index,
            cumulative_points: self.cumulative_points.clone(),
        }
    }

    pub fn settle_epoch(
        &mut self,
        samples: &[NodeContributionSample],
    ) -> Result<EpochSettlementReport, NodePointsError> {
        let mut settlements = samples
            .iter()
            .map(|sample| self.build_settlement(sample))
            .collect::<Vec<_>>();
        let distributed_points = allocate_awards_for_score(
            self.config.epoch_pool_points,
            &mut settlements,
            settlement_total_score,
            settlement_main_award_mut,
        )?;
        let storage_distributed_points = allocate_awards_for_score(
            self.config.storage_pool_points,
            &mut settlements,
            settlement_storage_reward_score,
            settlement_storage_award_mut,
        )?;

        let mut cumulative_updates = Vec::with_capacity(settlements.len());

        for settlement in &mut settlements {
            settlement.awarded_points = settlement
                .main_awarded_points
                .checked_add(settlement.storage_awarded_points)
                .ok_or_else(|| NodePointsError {
                    reason: format!(
                        "awarded points overflow for {}: main={} storage={}",
                        settlement.node_id,
                        settlement.main_awarded_points,
                        settlement.storage_awarded_points
                    ),
                })?;
            let current = self.cumulative_points(settlement.node_id.as_str());
            let next = current
                .checked_add(settlement.awarded_points)
                .ok_or_else(|| NodePointsError {
                    reason: format!(
                        "cumulative points overflow for {}: current={} delta={}",
                        settlement.node_id, current, settlement.awarded_points
                    ),
                })?;
            settlement.cumulative_points = next;
            cumulative_updates.push((settlement.node_id.clone(), next));
        }

        let total_distributed_points = distributed_points
            .checked_add(storage_distributed_points)
            .ok_or_else(|| NodePointsError {
            reason: format!(
                "total distributed points overflow: main={} storage={}",
                distributed_points, storage_distributed_points
            ),
        })?;
        let next_epoch_index = self
            .epoch_index
            .checked_add(1)
            .ok_or_else(|| NodePointsError {
                reason: format!("epoch index overflow at {}", self.epoch_index),
            })?;

        let report = EpochSettlementReport {
            epoch_index: self.epoch_index,
            pool_points: self.config.epoch_pool_points,
            storage_pool_points: self.config.storage_pool_points,
            distributed_points,
            storage_distributed_points,
            total_distributed_points,
            settlements,
        };

        for (node_id, next) in cumulative_updates {
            self.cumulative_points.insert(node_id, next);
        }
        self.epoch_index = next_epoch_index;

        Ok(report)
    }

    fn build_settlement(&self, sample: &NodeContributionSample) -> NodeSettlement {
        let verify_pass_ratio = clamp_ratio(sample.verify_pass_ratio);
        let availability_ratio = clamp_ratio(sample.availability_ratio);
        let compute_units = sample.delegated_sim_compute_units as f64
            * self.config.delegated_compute_multiplier.max(0.0)
            + sample.world_maintenance_compute_units as f64
                * self.config.maintenance_compute_multiplier.max(0.0);
        let compute_score = compute_units.max(0.0) * verify_pass_ratio;

        let storage_gib = sample.effective_storage_bytes as f64 / BYTES_PER_GIB;
        let storage_score = storage_gib.max(0.0).sqrt() * availability_ratio;
        let (rewardable_storage_bytes, storage_reward_score) =
            self.storage_reward_score(sample, availability_ratio);

        let raw_uptime_ratio = self.raw_uptime_ratio(sample);
        let uptime_score = normalize_ratio_with_threshold(
            raw_uptime_ratio,
            clamp_ratio(self.config.min_uptime_challenge_pass_ratio),
        );

        let reliability_score = (verify_pass_ratio + availability_ratio) / 2.0;
        let obligation_met =
            sample.self_sim_compute_units >= self.config.min_self_sim_compute_units;
        let mut penalty_score = sample.explicit_penalty_points.max(0.0);
        if !obligation_met {
            penalty_score += self.config.obligation_penalty_points.max(0.0);
        }

        let (weight_compute, weight_storage, weight_uptime, weight_reliability) =
            self.config.normalized_weights();
        let total_score = (weight_compute * compute_score
            + weight_storage * storage_score
            + weight_uptime * uptime_score
            + weight_reliability * reliability_score
            - penalty_score)
            .max(0.0);

        NodeSettlement {
            node_id: sample.node_id.clone(),
            obligation_met,
            compute_score,
            storage_score,
            uptime_score,
            reliability_score,
            storage_reward_score,
            rewardable_storage_bytes,
            penalty_score,
            total_score,
            main_awarded_points: 0,
            storage_awarded_points: 0,
            awarded_points: 0,
            cumulative_points: 0,
        }
    }

    fn raw_uptime_ratio(&self, sample: &NodeContributionSample) -> f64 {
        if sample.uptime_total_checks > 0 {
            return (sample.uptime_valid_checks as f64 / sample.uptime_total_checks as f64)
                .clamp(0.0, 1.0);
        }
        if self.config.epoch_duration_seconds == 0 {
            return 0.0;
        }
        (sample.uptime_seconds as f64 / self.config.epoch_duration_seconds as f64).clamp(0.0, 1.0)
    }

    fn storage_reward_score(
        &self,
        sample: &NodeContributionSample,
        availability_ratio: f64,
    ) -> (u64, f64) {
        let rewardable_storage_bytes = self.rewardable_storage_bytes(sample);
        if rewardable_storage_bytes == 0 {
            return (0, 0.0);
        }
        if sample.storage_total_checks < self.config.min_storage_challenge_checks {
            return (rewardable_storage_bytes, 0.0);
        }

        let raw_storage_pass_ratio = self.raw_storage_challenge_pass_ratio(sample);
        let normalized_pass_ratio = normalize_ratio_with_threshold(
            raw_storage_pass_ratio,
            clamp_ratio(self.config.min_storage_challenge_pass_ratio),
        );
        if normalized_pass_ratio <= 0.0 {
            return (rewardable_storage_bytes, 0.0);
        }

        let rewardable_storage_gib = rewardable_storage_bytes as f64 / BYTES_PER_GIB;
        let storage_reward_score =
            rewardable_storage_gib.sqrt() * normalized_pass_ratio * availability_ratio;
        (rewardable_storage_bytes, storage_reward_score.max(0.0))
    }

    fn raw_storage_challenge_pass_ratio(&self, sample: &NodeContributionSample) -> f64 {
        if sample.storage_total_checks == 0 {
            return 0.0;
        }
        (sample.storage_valid_checks as f64 / sample.storage_total_checks as f64).clamp(0.0, 1.0)
    }

    fn rewardable_storage_bytes(&self, sample: &NodeContributionSample) -> u64 {
        let mut rewardable_storage_bytes = sample.effective_storage_bytes;
        let ratio = self.config.max_rewardable_storage_to_staked_ratio;
        if !ratio.is_finite() || ratio <= 0.0 {
            return rewardable_storage_bytes;
        }
        if sample.staked_storage_bytes == 0 {
            return 0;
        }

        let staked_cap = (sample.staked_storage_bytes as f64 * ratio).floor();
        if !staked_cap.is_finite() || staked_cap <= 0.0 {
            return 0;
        }
        let staked_cap = staked_cap.min(u64::MAX as f64) as u64;
        rewardable_storage_bytes = rewardable_storage_bytes.min(staked_cap);
        rewardable_storage_bytes
    }
}

impl Default for NodePointsLedger {
    fn default() -> Self {
        Self::new(NodePointsConfig::default())
    }
}

fn settlement_total_score(settlement: &NodeSettlement) -> f64 {
    settlement.total_score
}

fn settlement_storage_reward_score(settlement: &NodeSettlement) -> f64 {
    settlement.storage_reward_score
}

fn settlement_main_award_mut(settlement: &mut NodeSettlement) -> &mut u64 {
    &mut settlement.main_awarded_points
}

fn settlement_storage_award_mut(settlement: &mut NodeSettlement) -> &mut u64 {
    &mut settlement.storage_awarded_points
}

fn allocate_awards_for_score(
    pool_points: u64,
    settlements: &mut [NodeSettlement],
    score_of: fn(&NodeSettlement) -> f64,
    award_mut: for<'a> fn(&'a mut NodeSettlement) -> &'a mut u64,
) -> Result<u64, NodePointsError> {
    for settlement in settlements.iter_mut() {
        *award_mut(settlement) = 0;
    }
    if pool_points == 0 || settlements.is_empty() {
        return Ok(0);
    }

    let total_score = settlements
        .iter()
        .map(|settlement| score_of(settlement).max(0.0))
        .sum::<f64>();
    if total_score <= f64::EPSILON {
        return Ok(0);
    }

    let mut distributed = 0u64;
    let mut remainders = Vec::with_capacity(settlements.len());

    for (index, settlement) in settlements.iter_mut().enumerate() {
        let score = score_of(settlement).max(0.0);
        if score <= 0.0 {
            remainders.push(RemainderEntry {
                settlement_index: index,
                node_id: settlement.node_id.clone(),
                fractional: 0.0,
            });
            continue;
        }

        let exact_points = (pool_points as f64) * score / total_score;
        let floor_points = exact_points.floor() as u64;
        *award_mut(settlement) = floor_points;
        distributed = distributed
            .checked_add(floor_points)
            .ok_or_else(|| NodePointsError {
                reason: format!(
                    "award distribution overflow for {}: distributed={} floor_points={}",
                    settlement.node_id, distributed, floor_points
                ),
            })?;
        remainders.push(RemainderEntry {
            settlement_index: index,
            node_id: settlement.node_id.clone(),
            fractional: exact_points - floor_points as f64,
        });
    }

    let mut remaining = pool_points
        .checked_sub(distributed)
        .ok_or_else(|| NodePointsError {
            reason: format!(
                "distributed points exceed pool: pool={} distributed={}",
                pool_points, distributed
            ),
        })?;
    remainders.sort_by(|left, right| {
        right
            .fractional
            .partial_cmp(&left.fractional)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.node_id.cmp(&right.node_id))
    });

    for entry in remainders {
        if remaining == 0 {
            break;
        }
        if score_of(&settlements[entry.settlement_index]) <= 0.0 {
            continue;
        }
        let node_id = entry.node_id;
        let award = award_mut(&mut settlements[entry.settlement_index]);
        *award = award.checked_add(1).ok_or_else(|| NodePointsError {
            reason: format!("award overflow for {}", node_id),
        })?;
        distributed = distributed.checked_add(1).ok_or_else(|| NodePointsError {
            reason: format!(
                "distributed points overflow while assigning remainder to {}",
                node_id
            ),
        })?;
        remaining -= 1;
    }

    Ok(distributed)
}

fn clamp_ratio(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    value.clamp(0.0, 1.0)
}

fn normalize_ratio_with_threshold(raw_ratio: f64, min_ratio: f64) -> f64 {
    let raw = clamp_ratio(raw_ratio);
    if min_ratio >= 1.0 {
        if raw >= 1.0 {
            return 1.0;
        }
        return 0.0;
    }
    if raw <= min_ratio {
        return 0.0;
    }
    ((raw - min_ratio) / (1.0 - min_ratio)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_WEIGHT_COMPUTE, DEFAULT_WEIGHT_RELIABILITY, DEFAULT_WEIGHT_STORAGE,
        DEFAULT_WEIGHT_UPTIME, NodeContributionSample, NodePointsConfig, NodePointsLedger,
        NodePointsLedgerSnapshot,
    };
    use std::collections::BTreeMap;

    fn sample(node_id: &str) -> NodeContributionSample {
        NodeContributionSample {
            node_id: node_id.to_string(),
            self_sim_compute_units: 5,
            delegated_sim_compute_units: 0,
            world_maintenance_compute_units: 0,
            effective_storage_bytes: 0,
            uptime_seconds: 0,
            uptime_valid_checks: 0,
            uptime_total_checks: 0,
            storage_valid_checks: 0,
            storage_total_checks: 0,
            staked_storage_bytes: 0,
            verify_pass_ratio: 1.0,
            availability_ratio: 1.0,
            explicit_penalty_points: 0.0,
        }
    }

    fn gib(value: u64) -> u64 {
        value * 1024 * 1024 * 1024
    }

    fn compute_only_config(pool: u64) -> NodePointsConfig {
        NodePointsConfig {
            epoch_pool_points: pool,
            weight_compute: 1.0,
            weight_storage: 0.0,
            weight_uptime: 0.0,
            weight_reliability: 0.0,
            ..NodePointsConfig::default()
        }
    }

    #[test]
    fn rewards_extra_compute_not_self_obligation_compute() {
        let mut ledger = NodePointsLedger::new(compute_only_config(100));
        let mut high = sample("node-high");
        high.delegated_sim_compute_units = 10;
        high.self_sim_compute_units = 5;

        let mut baseline = sample("node-baseline");
        baseline.self_sim_compute_units = 100;

        let report = ledger.settle_epoch(&[high, baseline]).expect("settlement");
        assert_eq!(report.distributed_points, 100);
        assert_eq!(report.settlements[0].awarded_points, 100);
        assert_eq!(report.settlements[1].awarded_points, 0);
        assert_eq!(report.settlements[0].compute_score, 10.0);
        assert_eq!(report.settlements[1].compute_score, 0.0);
    }

    #[test]
    fn applies_obligation_penalty_when_self_compute_is_too_low() {
        let mut config = compute_only_config(100);
        config.min_self_sim_compute_units = 3;
        config.obligation_penalty_points = 4.0;
        let mut ledger = NodePointsLedger::new(config);

        let mut weak = sample("node-weak");
        weak.self_sim_compute_units = 2;
        weak.delegated_sim_compute_units = 10;

        let mut good = sample("node-good");
        good.self_sim_compute_units = 3;
        good.delegated_sim_compute_units = 6;

        let report = ledger.settle_epoch(&[weak, good]).expect("settlement");
        assert_eq!(report.distributed_points, 100);
        assert!(!report.settlements[0].obligation_met);
        assert!(report.settlements[1].obligation_met);
        assert_eq!(report.settlements[0].penalty_score, 4.0);
        assert_eq!(report.settlements[0].total_score, 6.0);
        assert_eq!(report.settlements[1].total_score, 6.0);
        assert_eq!(report.settlements[0].awarded_points, 50);
        assert_eq!(report.settlements[1].awarded_points, 50);
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn storage_score_uses_sqrt_curve_with_availability() {
        let mut config = NodePointsConfig::default();
        config.epoch_pool_points = 100;
        config.weight_compute = 0.0;
        config.weight_storage = 1.0;
        config.weight_uptime = 0.0;
        config.weight_reliability = 0.0;
        let mut ledger = NodePointsLedger::new(config);

        let mut one_gib = sample("node-a");
        one_gib.effective_storage_bytes = gib(1);

        let mut four_gib = sample("node-b");
        four_gib.effective_storage_bytes = gib(4);

        let mut nine_gib_half = sample("node-c");
        nine_gib_half.effective_storage_bytes = gib(9);
        nine_gib_half.availability_ratio = 0.5;

        let report = ledger
            .settle_epoch(&[one_gib, four_gib, nine_gib_half])
            .expect("settlement");
        assert_eq!(report.distributed_points, 100);
        assert_eq!(report.settlements[0].storage_score, 1.0);
        assert_eq!(report.settlements[1].storage_score, 2.0);
        assert_eq!(report.settlements[2].storage_score, 1.5);
        assert!(report.settlements[1].awarded_points > report.settlements[2].awarded_points);
        assert!(report.settlements[2].awarded_points > report.settlements[0].awarded_points);
    }

    #[test]
    fn remainder_distribution_is_stable_when_scores_tie() {
        let mut ledger = NodePointsLedger::new(compute_only_config(10));

        let mut a = sample("node-a");
        a.delegated_sim_compute_units = 1;
        let mut b = sample("node-b");
        b.delegated_sim_compute_units = 1;
        let mut c = sample("node-c");
        c.delegated_sim_compute_units = 1;

        let report = ledger.settle_epoch(&[a, b, c]).expect("settlement");
        assert_eq!(report.distributed_points, 10);
        assert_eq!(report.settlements[0].awarded_points, 4);
        assert_eq!(report.settlements[1].awarded_points, 3);
        assert_eq!(report.settlements[2].awarded_points, 3);
    }

    #[test]
    fn cumulative_points_accumulate_across_epochs() {
        let mut ledger = NodePointsLedger::new(compute_only_config(10));
        let mut a = sample("node-a");
        a.delegated_sim_compute_units = 1;

        let first = ledger.settle_epoch(&[a.clone()]).expect("settlement");
        assert_eq!(first.epoch_index, 0);
        assert_eq!(first.settlements[0].awarded_points, 10);
        assert_eq!(first.settlements[0].cumulative_points, 10);

        let second = ledger.settle_epoch(&[a]).expect("settlement");
        assert_eq!(second.epoch_index, 1);
        assert_eq!(second.settlements[0].awarded_points, 10);
        assert_eq!(second.settlements[0].cumulative_points, 20);
        assert_eq!(ledger.cumulative_points("node-a"), 20);
        assert_eq!(ledger.epoch_index(), 2);
    }

    #[test]
    fn ledger_snapshot_roundtrip_restores_epoch_and_cumulative_points() {
        let mut ledger = NodePointsLedger::new(compute_only_config(10));
        let mut a = sample("node-a");
        a.delegated_sim_compute_units = 1;
        let mut b = sample("node-b");
        b.delegated_sim_compute_units = 1;

        let _ = ledger
            .settle_epoch(&[a.clone(), b.clone()])
            .expect("settlement");
        let snapshot = ledger.snapshot();
        let restored = NodePointsLedger::from_snapshot(snapshot.clone());

        assert_eq!(restored.epoch_index(), snapshot.epoch_index);
        assert_eq!(restored.config(), &snapshot.config);
        assert_eq!(
            restored.cumulative_points("node-a"),
            *snapshot.cumulative_points.get("node-a").unwrap_or(&0)
        );
        assert_eq!(
            restored.cumulative_points("node-b"),
            *snapshot.cumulative_points.get("node-b").unwrap_or(&0)
        );

        let restored_snapshot: NodePointsLedgerSnapshot = restored.snapshot();
        assert_eq!(restored_snapshot, snapshot);
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn uses_default_weights_when_input_weights_are_all_zero() {
        let mut config = NodePointsConfig::default();
        config.epoch_pool_points = 100;
        config.weight_compute = 0.0;
        config.weight_storage = 0.0;
        config.weight_uptime = 0.0;
        config.weight_reliability = 0.0;
        let mut ledger = NodePointsLedger::new(config);

        let mut rich_compute = sample("node-compute");
        rich_compute.delegated_sim_compute_units = 10;

        let mut rich_storage = sample("node-storage");
        rich_storage.effective_storage_bytes = gib(16);

        let report = ledger
            .settle_epoch(&[rich_compute, rich_storage])
            .expect("settlement");
        assert_eq!(report.distributed_points, 100);
        let compute_settlement = &report.settlements[0];
        let storage_settlement = &report.settlements[1];
        assert!(compute_settlement.total_score > 0.0);
        assert!(storage_settlement.total_score > 0.0);
        assert_eq!(
            DEFAULT_WEIGHT_COMPUTE
                + DEFAULT_WEIGHT_STORAGE
                + DEFAULT_WEIGHT_UPTIME
                + DEFAULT_WEIGHT_RELIABILITY,
            1.0
        );
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn multi_node_closure_rewards_compute_and_storage_with_penalty() {
        let mut config = NodePointsConfig::default();
        config.epoch_pool_points = 1000;
        config.epoch_duration_seconds = 100;
        config.min_self_sim_compute_units = 5;
        config.obligation_penalty_points = 10.0;
        let mut ledger = NodePointsLedger::new(config);

        let mut node_a = sample("node-a-compute");
        node_a.self_sim_compute_units = 8;
        node_a.delegated_sim_compute_units = 120;
        node_a.world_maintenance_compute_units = 20;
        node_a.effective_storage_bytes = gib(20);
        node_a.uptime_seconds = 100;
        node_a.verify_pass_ratio = 1.0;
        node_a.availability_ratio = 0.95;

        let mut node_b = sample("node-b-storage");
        node_b.self_sim_compute_units = 7;
        node_b.delegated_sim_compute_units = 20;
        node_b.world_maintenance_compute_units = 10;
        node_b.effective_storage_bytes = gib(400);
        node_b.uptime_seconds = 98;
        node_b.verify_pass_ratio = 0.98;
        node_b.availability_ratio = 0.99;

        let mut node_c = sample("node-c-penalized");
        node_c.self_sim_compute_units = 1;
        node_c.delegated_sim_compute_units = 80;
        node_c.world_maintenance_compute_units = 40;
        node_c.effective_storage_bytes = gib(120);
        node_c.uptime_seconds = 40;
        node_c.verify_pass_ratio = 0.4;
        node_c.availability_ratio = 0.4;
        node_c.explicit_penalty_points = 20.0;

        let epoch0 = ledger
            .settle_epoch(&[node_a.clone(), node_b.clone(), node_c.clone()])
            .expect("settlement");
        assert_eq!(epoch0.epoch_index, 0);
        assert_eq!(epoch0.pool_points, 1000);
        assert_eq!(epoch0.distributed_points, 1000);

        let settlement_a0 = &epoch0.settlements[0];
        let settlement_b0 = &epoch0.settlements[1];
        let settlement_c0 = &epoch0.settlements[2];

        assert!(settlement_a0.obligation_met);
        assert!(settlement_b0.obligation_met);
        assert!(!settlement_c0.obligation_met);
        assert!(settlement_c0.penalty_score >= 30.0);
        assert_eq!(settlement_c0.total_score, 0.0);

        assert!(settlement_a0.awarded_points > settlement_b0.awarded_points);
        assert_eq!(settlement_c0.awarded_points, 0);
        assert_eq!(
            settlement_a0.awarded_points
                + settlement_b0.awarded_points
                + settlement_c0.awarded_points,
            1000
        );

        // Epoch 1: node-b improves compute output; node-c recovers but still trails.
        node_b.delegated_sim_compute_units = 140;
        node_b.world_maintenance_compute_units = 30;
        node_b.effective_storage_bytes = gib(420);
        node_b.uptime_seconds = 100;
        node_b.verify_pass_ratio = 1.0;
        node_b.availability_ratio = 1.0;

        node_c.self_sim_compute_units = 6;
        node_c.verify_pass_ratio = 0.7;
        node_c.availability_ratio = 0.8;
        node_c.uptime_seconds = 90;
        node_c.explicit_penalty_points = 5.0;

        let epoch1 = ledger
            .settle_epoch(&[node_a, node_b, node_c])
            .expect("settlement");
        assert_eq!(epoch1.epoch_index, 1);
        assert_eq!(epoch1.pool_points, 1000);
        assert_eq!(epoch1.distributed_points, 1000);

        let settlement_a1 = &epoch1.settlements[0];
        let settlement_b1 = &epoch1.settlements[1];
        let settlement_c1 = &epoch1.settlements[2];

        assert!(settlement_c1.obligation_met);
        assert!(settlement_c1.awarded_points > 0);
        assert!(settlement_b1.awarded_points > settlement_a1.awarded_points);
        assert!(settlement_a1.cumulative_points > settlement_a1.awarded_points);
        assert!(settlement_b1.cumulative_points > settlement_b1.awarded_points);
        assert!(settlement_c1.cumulative_points > settlement_c0.cumulative_points);

        let total_cumulative = settlement_a1.cumulative_points
            + settlement_b1.cumulative_points
            + settlement_c1.cumulative_points;
        assert_eq!(total_cumulative, 2000);
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn storage_system_pool_distributes_with_challenge_threshold() {
        let mut config = NodePointsConfig::default();
        config.epoch_pool_points = 0;
        config.storage_pool_points = 100;
        config.min_storage_challenge_pass_ratio = 0.8;
        config.min_storage_challenge_checks = 2;
        let mut ledger = NodePointsLedger::new(config);

        let mut good = sample("node-good");
        good.effective_storage_bytes = gib(16);
        good.storage_valid_checks = 10;
        good.storage_total_checks = 10;

        let mut weak = sample("node-weak");
        weak.effective_storage_bytes = gib(16);
        weak.storage_valid_checks = 8;
        weak.storage_total_checks = 10;

        let report = ledger.settle_epoch(&[good, weak]).expect("settlement");
        assert_eq!(report.pool_points, 0);
        assert_eq!(report.storage_pool_points, 100);
        assert_eq!(report.distributed_points, 0);
        assert_eq!(report.storage_distributed_points, 100);
        assert_eq!(report.total_distributed_points, 100);
        assert_eq!(report.settlements[0].main_awarded_points, 0);
        assert_eq!(report.settlements[1].main_awarded_points, 0);
        assert_eq!(report.settlements[0].storage_awarded_points, 100);
        assert_eq!(report.settlements[1].storage_awarded_points, 0);
        assert_eq!(report.settlements[0].awarded_points, 100);
        assert_eq!(report.settlements[1].awarded_points, 0);
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn storage_system_pool_requires_minimum_checks() {
        let mut config = NodePointsConfig::default();
        config.epoch_pool_points = 0;
        config.storage_pool_points = 50;
        config.min_storage_challenge_pass_ratio = 0.5;
        config.min_storage_challenge_checks = 3;
        let mut ledger = NodePointsLedger::new(config);

        let mut low_checks = sample("node-low-checks");
        low_checks.effective_storage_bytes = gib(20);
        low_checks.storage_valid_checks = 2;
        low_checks.storage_total_checks = 2;

        let mut pass = sample("node-pass");
        pass.effective_storage_bytes = gib(5);
        pass.storage_valid_checks = 3;
        pass.storage_total_checks = 3;

        let report = ledger
            .settle_epoch(&[low_checks, pass])
            .expect("settlement");
        assert_eq!(report.storage_distributed_points, 50);
        assert_eq!(report.settlements[0].storage_reward_score, 0.0);
        assert_eq!(report.settlements[0].storage_awarded_points, 0);
        assert_eq!(report.settlements[1].storage_awarded_points, 50);
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn storage_system_pool_caps_rewardable_storage_by_stake_ratio() {
        let mut config = NodePointsConfig::default();
        config.epoch_pool_points = 0;
        config.storage_pool_points = 100;
        config.min_storage_challenge_pass_ratio = 0.0;
        config.min_storage_challenge_checks = 1;
        config.max_rewardable_storage_to_staked_ratio = 1.0;
        let mut ledger = NodePointsLedger::new(config);

        let mut capped = sample("node-capped");
        capped.effective_storage_bytes = gib(100);
        capped.staked_storage_bytes = gib(10);
        capped.storage_valid_checks = 1;
        capped.storage_total_checks = 1;

        let mut uncapped = sample("node-uncapped");
        uncapped.effective_storage_bytes = gib(20);
        uncapped.staked_storage_bytes = gib(20);
        uncapped.storage_valid_checks = 1;
        uncapped.storage_total_checks = 1;

        let report = ledger
            .settle_epoch(&[capped, uncapped])
            .expect("settlement");
        assert_eq!(report.storage_distributed_points, 100);
        assert_eq!(report.settlements[0].rewardable_storage_bytes, gib(10));
        assert_eq!(report.settlements[1].rewardable_storage_bytes, gib(20));
        assert!(
            report.settlements[1].storage_awarded_points
                > report.settlements[0].storage_awarded_points
        );
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn uptime_score_uses_challenge_ratio_with_threshold() {
        let mut config = NodePointsConfig::default();
        config.epoch_pool_points = 100;
        config.weight_compute = 0.0;
        config.weight_storage = 0.0;
        config.weight_uptime = 1.0;
        config.weight_reliability = 0.0;
        config.min_uptime_challenge_pass_ratio = 0.85;
        let mut ledger = NodePointsLedger::new(config);

        let mut below = sample("node-below");
        below.uptime_seconds = 100;
        below.uptime_valid_checks = 8;
        below.uptime_total_checks = 10;

        let mut above = sample("node-above");
        above.uptime_seconds = 10;
        above.uptime_valid_checks = 9;
        above.uptime_total_checks = 10;

        let report = ledger.settle_epoch(&[below, above]).expect("settlement");
        assert_eq!(report.distributed_points, 100);
        assert_eq!(report.settlements[0].uptime_score, 0.0);
        assert!(
            (report.settlements[1].uptime_score - (1.0 / 3.0)).abs() <= 1e-6,
            "uptime score should be normalized by threshold"
        );
        assert_eq!(report.settlements[0].awarded_points, 0);
        assert_eq!(report.settlements[1].awarded_points, 100);
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn uptime_score_falls_back_to_uptime_seconds_when_no_checks() {
        let mut config = NodePointsConfig::default();
        config.epoch_pool_points = 10;
        config.epoch_duration_seconds = 100;
        config.weight_compute = 0.0;
        config.weight_storage = 0.0;
        config.weight_uptime = 1.0;
        config.weight_reliability = 0.0;
        config.min_uptime_challenge_pass_ratio = 0.5;
        let mut ledger = NodePointsLedger::new(config);

        let mut a = sample("node-a");
        a.uptime_seconds = 80;
        a.uptime_valid_checks = 0;
        a.uptime_total_checks = 0;

        let mut b = sample("node-b");
        b.uptime_seconds = 60;
        b.uptime_valid_checks = 0;
        b.uptime_total_checks = 0;

        let report = ledger.settle_epoch(&[a, b]).expect("settlement");
        assert!(
            (report.settlements[0].uptime_score - 0.6).abs() <= 1e-9,
            "fallback uptime score should use uptime seconds ratio"
        );
        assert!(report.settlements[0].awarded_points > report.settlements[1].awarded_points);
    }

    #[test]
    fn settle_epoch_rejects_cumulative_overflow_without_state_mutation() {
        let mut cumulative_points = BTreeMap::new();
        cumulative_points.insert("node-a".to_string(), u64::MAX);
        let snapshot = NodePointsLedgerSnapshot {
            config: compute_only_config(1),
            epoch_index: 7,
            cumulative_points,
        };
        let mut ledger = NodePointsLedger::from_snapshot(snapshot);

        let mut node_a = sample("node-a");
        node_a.delegated_sim_compute_units = 1;
        let err = ledger
            .settle_epoch(&[node_a])
            .expect_err("must reject cumulative overflow");
        assert!(err.reason.contains("cumulative points overflow"));
        assert_eq!(ledger.epoch_index(), 7);
        assert_eq!(ledger.cumulative_points("node-a"), u64::MAX);
    }

    #[test]
    fn settle_epoch_rejects_epoch_index_overflow_without_state_mutation() {
        let snapshot = NodePointsLedgerSnapshot {
            config: compute_only_config(1),
            epoch_index: u64::MAX,
            cumulative_points: BTreeMap::new(),
        };
        let mut ledger = NodePointsLedger::from_snapshot(snapshot);

        let mut node_a = sample("node-a");
        node_a.delegated_sim_compute_units = 1;
        let err = ledger
            .settle_epoch(&[node_a])
            .expect_err("must reject epoch overflow");
        assert!(err.reason.contains("epoch index overflow"));
        assert_eq!(ledger.epoch_index(), u64::MAX);
        assert_eq!(ledger.cumulative_points("node-a"), 0);
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn settle_epoch_rejects_total_distributed_overflow_without_state_mutation() {
        let mut config = NodePointsConfig::default();
        config.epoch_pool_points = u64::MAX;
        config.storage_pool_points = u64::MAX;
        config.weight_compute = 1.0;
        config.weight_storage = 0.0;
        config.weight_uptime = 0.0;
        config.weight_reliability = 0.0;

        let snapshot = NodePointsLedgerSnapshot {
            config,
            epoch_index: 3,
            cumulative_points: BTreeMap::new(),
        };
        let mut ledger = NodePointsLedger::from_snapshot(snapshot);

        let mut node_main = sample("node-main");
        node_main.delegated_sim_compute_units = 1;

        let mut node_storage = sample("node-storage");
        node_storage.effective_storage_bytes = gib(8);
        node_storage.staked_storage_bytes = gib(8);
        node_storage.storage_valid_checks = 1;
        node_storage.storage_total_checks = 1;

        let err = ledger
            .settle_epoch(&[node_main, node_storage])
            .expect_err("must reject total distributed overflow");
        assert!(err.reason.contains("total distributed points overflow"));
        assert_eq!(ledger.epoch_index(), 3);
        assert_eq!(ledger.cumulative_points("node-main"), 0);
        assert_eq!(ledger.cumulative_points("node-storage"), 0);
    }
}
