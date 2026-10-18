use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{
    EpochSettlementReport, NodeContributionSample, NodePointsConfig, NodePointsError,
    NodePointsLedger, NodePointsLedgerSnapshot,
};

pub use oasis7_agent_api::node_contribution::{DistFsChallengeProofHint, NodePointsRole};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodePointsRuntimeHeuristics {
    pub error_penalty_points: f64,
    pub degraded_verify_ratio: f64,
    pub degraded_availability_ratio: f64,
    pub storage_role_delegated_tick_ratio: f64,
}

impl Default for NodePointsRuntimeHeuristics {
    fn default() -> Self {
        Self {
            error_penalty_points: 2.0,
            degraded_verify_ratio: 0.7,
            degraded_availability_ratio: 0.8,
            storage_role_delegated_tick_ratio: 0.5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodePointsRuntimeCursorSnapshot {
    pub tick_count: u64,
    pub observed_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NodePointsRuntimeAccumulatorSnapshot {
    pub role: Option<String>,
    pub self_sim_compute_units: u64,
    pub delegated_sim_compute_units: u64,
    pub world_maintenance_compute_units: u64,
    pub uptime_ms: u64,
    pub uptime_checks_passed: u64,
    pub uptime_checks_total: u64,
    pub storage_checks_passed: u64,
    pub storage_checks_total: u64,
    pub staked_storage_bytes: u64,
    pub max_storage_bytes: u64,
    pub error_samples: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodePointsRuntimeCollectorSnapshot {
    pub ledger: NodePointsLedgerSnapshot,
    pub heuristics: NodePointsRuntimeHeuristics,
    pub epoch_started_at_unix_ms: Option<i64>,
    pub cursors: BTreeMap<String, NodePointsRuntimeCursorSnapshot>,
    pub current_epoch: BTreeMap<String, NodePointsRuntimeAccumulatorSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodePointsRuntimeObservation {
    pub node_id: String,
    pub role: NodePointsRole,
    pub tick_count: u64,
    pub running: bool,
    pub uptime_checks_passed: u64,
    pub uptime_checks_total: u64,
    pub storage_checks_passed: u64,
    pub storage_checks_total: u64,
    pub staked_storage_bytes: u64,
    pub observed_at_unix_ms: i64,
    pub has_error: bool,
    pub effective_storage_bytes: u64,
    pub storage_challenge_proof_hint: Option<DistFsChallengeProofHint>,
}

#[derive(Debug, Clone)]
struct NodeCursor {
    tick_count: u64,
    observed_at_unix_ms: i64,
}

impl NodeCursor {
    fn to_snapshot(&self) -> NodePointsRuntimeCursorSnapshot {
        NodePointsRuntimeCursorSnapshot {
            tick_count: self.tick_count,
            observed_at_unix_ms: self.observed_at_unix_ms,
        }
    }

    fn from_snapshot(snapshot: NodePointsRuntimeCursorSnapshot) -> Self {
        Self {
            tick_count: snapshot.tick_count,
            observed_at_unix_ms: snapshot.observed_at_unix_ms,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct NodeEpochAccumulator {
    role: Option<NodePointsRole>,
    self_sim_compute_units: u64,
    delegated_sim_compute_units: u64,
    world_maintenance_compute_units: u64,
    uptime_ms: u64,
    uptime_checks_passed: u64,
    uptime_checks_total: u64,
    storage_checks_passed: u64,
    storage_checks_total: u64,
    staked_storage_bytes: u64,
    max_storage_bytes: u64,
    error_samples: u64,
}

impl NodeEpochAccumulator {
    fn to_snapshot(&self) -> NodePointsRuntimeAccumulatorSnapshot {
        NodePointsRuntimeAccumulatorSnapshot {
            role: self.role.map(|role| role.as_str().to_string()),
            self_sim_compute_units: self.self_sim_compute_units,
            delegated_sim_compute_units: self.delegated_sim_compute_units,
            world_maintenance_compute_units: self.world_maintenance_compute_units,
            uptime_ms: self.uptime_ms,
            uptime_checks_passed: self.uptime_checks_passed,
            uptime_checks_total: self.uptime_checks_total,
            storage_checks_passed: self.storage_checks_passed,
            storage_checks_total: self.storage_checks_total,
            staked_storage_bytes: self.staked_storage_bytes,
            max_storage_bytes: self.max_storage_bytes,
            error_samples: self.error_samples,
        }
    }

    fn from_snapshot(snapshot: NodePointsRuntimeAccumulatorSnapshot) -> Self {
        Self {
            role: snapshot
                .role
                .as_deref()
                .and_then(|role| role.parse::<NodePointsRole>().ok()),
            self_sim_compute_units: snapshot.self_sim_compute_units,
            delegated_sim_compute_units: snapshot.delegated_sim_compute_units,
            world_maintenance_compute_units: snapshot.world_maintenance_compute_units,
            uptime_ms: snapshot.uptime_ms,
            uptime_checks_passed: snapshot.uptime_checks_passed,
            uptime_checks_total: snapshot.uptime_checks_total,
            storage_checks_passed: snapshot.storage_checks_passed,
            storage_checks_total: snapshot.storage_checks_total,
            staked_storage_bytes: snapshot.staked_storage_bytes,
            max_storage_bytes: snapshot.max_storage_bytes,
            error_samples: snapshot.error_samples,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NodePointsRuntimeCollector {
    ledger: NodePointsLedger,
    heuristics: NodePointsRuntimeHeuristics,
    epoch_started_at_unix_ms: Option<i64>,
    cursors: BTreeMap<String, NodeCursor>,
    latest_observed_at_unix_ms: i64,
    current_epoch: BTreeMap<String, NodeEpochAccumulator>,
}

impl NodePointsRuntimeCollector {
    pub fn new(config: NodePointsConfig, heuristics: NodePointsRuntimeHeuristics) -> Self {
        Self {
            ledger: NodePointsLedger::new(config),
            heuristics,
            epoch_started_at_unix_ms: None,
            cursors: BTreeMap::new(),
            latest_observed_at_unix_ms: 0,
            current_epoch: BTreeMap::new(),
        }
    }

    pub fn from_snapshot(snapshot: NodePointsRuntimeCollectorSnapshot) -> Self {
        let cursors: BTreeMap<_, _> = snapshot
            .cursors
            .into_iter()
            .map(|(node_id, cursor)| (node_id, NodeCursor::from_snapshot(cursor)))
            .collect();
        let latest_observed_at_unix_ms = cursors
            .values()
            .map(|cursor| cursor.observed_at_unix_ms)
            .max()
            .unwrap_or(0);

        Self {
            ledger: NodePointsLedger::from_snapshot(snapshot.ledger),
            heuristics: snapshot.heuristics,
            epoch_started_at_unix_ms: snapshot.epoch_started_at_unix_ms,
            cursors,
            latest_observed_at_unix_ms,
            current_epoch: snapshot
                .current_epoch
                .into_iter()
                .map(|(node_id, accumulator)| {
                    (node_id, NodeEpochAccumulator::from_snapshot(accumulator))
                })
                .collect(),
        }
    }

    pub fn ledger(&self) -> &NodePointsLedger {
        &self.ledger
    }

    pub fn snapshot(&self) -> NodePointsRuntimeCollectorSnapshot {
        NodePointsRuntimeCollectorSnapshot {
            ledger: self.ledger.snapshot(),
            heuristics: self.heuristics.clone(),
            epoch_started_at_unix_ms: self.epoch_started_at_unix_ms,
            cursors: self
                .cursors
                .iter()
                .map(|(node_id, cursor)| (node_id.clone(), cursor.to_snapshot()))
                .collect(),
            current_epoch: self
                .current_epoch
                .iter()
                .map(|(node_id, accumulator)| (node_id.clone(), accumulator.to_snapshot()))
                .collect(),
        }
    }

    pub fn observe(
        &mut self,
        observation: NodePointsRuntimeObservation,
    ) -> Result<Option<EpochSettlementReport>, NodePointsError> {
        if self.epoch_started_at_unix_ms.is_none() {
            self.epoch_started_at_unix_ms = Some(observation.observed_at_unix_ms);
        }

        self.apply_observation(observation);

        let epoch_ms = self.epoch_duration_ms();
        if let Some(start_ms) = self.epoch_started_at_unix_ms
            && epoch_ms > 0
        {
            let elapsed_ms = observation_time_delta(start_ms, self.latest_observed_at_unix_ms());
            if elapsed_ms >= epoch_ms {
                let report = self.settle_epoch_internal()?;
                self.epoch_started_at_unix_ms = Some(self.latest_observed_at_unix_ms());
                return Ok(report);
            }
        }
        Ok(None)
    }

    pub fn force_settle(&mut self) -> Result<Option<EpochSettlementReport>, NodePointsError> {
        if self.current_epoch.is_empty() {
            return Ok(None);
        }
        let report = self.settle_epoch_internal()?;
        self.epoch_started_at_unix_ms = Some(self.latest_observed_at_unix_ms());
        Ok(report)
    }

    fn settle_epoch_internal(&mut self) -> Result<Option<EpochSettlementReport>, NodePointsError> {
        let samples = self.build_epoch_samples();
        if samples.is_empty() {
            self.current_epoch.clear();
            return Ok(None);
        }
        let report = self.ledger.settle_epoch(samples.as_slice())?;
        self.current_epoch.clear();
        Ok(Some(report))
    }

    fn build_epoch_samples(&self) -> Vec<NodeContributionSample> {
        self.current_epoch
            .iter()
            .map(|(node_id, accumulator)| {
                let epoch_seconds = self.ledger.config().epoch_duration_seconds.max(1);
                let mut availability_ratio = (accumulator.uptime_ms as f64
                    / (epoch_seconds as f64 * 1000.0))
                    .clamp(0.0, 1.0);
                let mut verify_pass_ratio = 1.0;
                if accumulator.error_samples > 0 {
                    verify_pass_ratio = self.heuristics.degraded_verify_ratio.clamp(0.0, 1.0);
                    availability_ratio = availability_ratio
                        .min(self.heuristics.degraded_availability_ratio.clamp(0.0, 1.0));
                }
                NodeContributionSample {
                    node_id: node_id.clone(),
                    self_sim_compute_units: accumulator.self_sim_compute_units,
                    delegated_sim_compute_units: accumulator.delegated_sim_compute_units,
                    world_maintenance_compute_units: accumulator.world_maintenance_compute_units,
                    effective_storage_bytes: accumulator.max_storage_bytes,
                    uptime_seconds: accumulator.uptime_ms / 1000,
                    uptime_valid_checks: accumulator.uptime_checks_passed,
                    uptime_total_checks: accumulator.uptime_checks_total,
                    storage_valid_checks: accumulator.storage_checks_passed,
                    storage_total_checks: accumulator.storage_checks_total,
                    staked_storage_bytes: accumulator.staked_storage_bytes,
                    verify_pass_ratio,
                    availability_ratio,
                    explicit_penalty_points: self.heuristics.error_penalty_points.max(0.0)
                        * accumulator.error_samples as f64,
                }
            })
            .collect()
    }

    fn apply_observation(&mut self, observation: NodePointsRuntimeObservation) {
        self.latest_observed_at_unix_ms = self
            .latest_observed_at_unix_ms
            .max(observation.observed_at_unix_ms);

        let accumulator = self
            .current_epoch
            .entry(observation.node_id.clone())
            .or_default();
        accumulator.role = Some(observation.role);
        accumulator.max_storage_bytes = accumulator
            .max_storage_bytes
            .max(observation.effective_storage_bytes);
        if observation.has_error {
            accumulator.error_samples = accumulator.error_samples.saturating_add(1);
        }
        accumulator.uptime_checks_passed = accumulator
            .uptime_checks_passed
            .saturating_add(observation.uptime_checks_passed);
        accumulator.uptime_checks_total = accumulator
            .uptime_checks_total
            .saturating_add(observation.uptime_checks_total);
        accumulator.storage_checks_passed = accumulator
            .storage_checks_passed
            .saturating_add(observation.storage_checks_passed);
        accumulator.storage_checks_total = accumulator
            .storage_checks_total
            .saturating_add(observation.storage_checks_total);
        accumulator.staked_storage_bytes = accumulator
            .staked_storage_bytes
            .max(observation.staked_storage_bytes);

        if let Some(cursor) = self.cursors.get(observation.node_id.as_str()) {
            let delta_ticks = observation.tick_count.saturating_sub(cursor.tick_count);
            let delta_ms =
                observation_time_delta(cursor.observed_at_unix_ms, observation.observed_at_unix_ms);
            if observation.running {
                accumulator.uptime_ms = accumulator.uptime_ms.saturating_add(delta_ms);
            }
            accumulator.self_sim_compute_units = accumulator
                .self_sim_compute_units
                .saturating_add(delta_ticks);

            match observation.role {
                NodePointsRole::Sequencer => {
                    accumulator.world_maintenance_compute_units = accumulator
                        .world_maintenance_compute_units
                        .saturating_add(delta_ticks);
                }
                NodePointsRole::Observer => {
                    accumulator.delegated_sim_compute_units = accumulator
                        .delegated_sim_compute_units
                        .saturating_add(delta_ticks);
                }
                NodePointsRole::Storage => {
                    let delegated = scale_ticks(
                        delta_ticks,
                        self.heuristics.storage_role_delegated_tick_ratio,
                    );
                    accumulator.delegated_sim_compute_units = accumulator
                        .delegated_sim_compute_units
                        .saturating_add(delegated);
                }
            }
        }

        self.cursors.insert(
            observation.node_id,
            NodeCursor {
                tick_count: observation.tick_count,
                observed_at_unix_ms: observation.observed_at_unix_ms,
            },
        );
    }

    fn epoch_duration_ms(&self) -> u64 {
        self.ledger
            .config()
            .epoch_duration_seconds
            .saturating_mul(1000)
    }

    fn latest_observed_at_unix_ms(&self) -> i64 {
        self.latest_observed_at_unix_ms
    }
}

fn scale_ticks(ticks: u64, ratio: f64) -> u64 {
    if !ratio.is_finite() || ratio <= 0.0 {
        return 0;
    }
    (ticks as f64 * ratio).floor() as u64
}

fn observation_time_delta(start_ms: i64, end_ms: i64) -> u64 {
    if end_ms <= start_ms {
        return 0;
    }
    (end_ms - start_ms) as u64
}

#[cfg(test)]
mod tests {
    use super::NodePointsRole;
    use super::{
        NodePointsRuntimeCollector, NodePointsRuntimeCollectorSnapshot,
        NodePointsRuntimeHeuristics, NodePointsRuntimeObservation,
    };
    use crate::NodePointsConfig;

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn collector_observes_ticks_and_force_settles() {
        let mut config = NodePointsConfig::default();
        config.epoch_pool_points = 100;
        config.epoch_duration_seconds = 60;
        let mut collector =
            NodePointsRuntimeCollector::new(config, NodePointsRuntimeHeuristics::default());

        let first = NodePointsRuntimeObservation {
            node_id: "node-a".to_string(),
            role: NodePointsRole::Sequencer,
            tick_count: 10,
            running: true,
            uptime_checks_passed: 1,
            uptime_checks_total: 1,
            storage_checks_passed: 0,
            storage_checks_total: 0,
            staked_storage_bytes: 0,
            observed_at_unix_ms: 1_000,
            has_error: false,
            effective_storage_bytes: 1024,
            storage_challenge_proof_hint: None,
        };
        let second = NodePointsRuntimeObservation {
            tick_count: 30,
            observed_at_unix_ms: 11_000,
            effective_storage_bytes: 2048,
            ..first.clone()
        };

        assert!(collector.observe(first).expect("observe").is_none());
        assert!(collector.observe(second).expect("observe").is_none());

        let report = collector
            .force_settle()
            .expect("force settle")
            .expect("report");
        assert_eq!(report.settlements.len(), 1);
        let settlement = &report.settlements[0];
        assert!(settlement.compute_score > 0.0 || settlement.storage_score > 0.0);
        assert_eq!(report.distributed_points, 100);
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn collector_snapshot_roundtrip_restores_state() {
        let mut config = NodePointsConfig::default();
        config.epoch_pool_points = 100;
        config.epoch_duration_seconds = 60;
        let mut collector =
            NodePointsRuntimeCollector::new(config, NodePointsRuntimeHeuristics::default());

        let first = NodePointsRuntimeObservation {
            node_id: "node-a".to_string(),
            role: NodePointsRole::Observer,
            tick_count: 1,
            running: true,
            uptime_checks_passed: 1,
            uptime_checks_total: 1,
            storage_checks_passed: 0,
            storage_checks_total: 0,
            staked_storage_bytes: 0,
            observed_at_unix_ms: 100,
            has_error: false,
            effective_storage_bytes: 128,
            storage_challenge_proof_hint: None,
        };
        let second = NodePointsRuntimeObservation {
            tick_count: 9,
            observed_at_unix_ms: 8_100,
            ..first.clone()
        };
        assert!(collector.observe(first).expect("observe").is_none());
        assert!(collector.observe(second).expect("observe").is_none());

        let snapshot = collector.snapshot();
        let mut restored = NodePointsRuntimeCollector::from_snapshot(snapshot.clone());
        let restored_snapshot: NodePointsRuntimeCollectorSnapshot = restored.snapshot();
        assert_eq!(restored_snapshot, snapshot);

        let report = restored
            .force_settle()
            .expect("force settle")
            .expect("settle from restored");
        assert_eq!(report.pool_points, 100);
        assert_eq!(report.distributed_points, 100);
        assert_eq!(report.settlements.len(), 1);
        assert_eq!(report.settlements[0].node_id, "node-a");
    }

    #[test]
    fn collector_caches_historical_latest_timestamp_across_out_of_order_observations() {
        let mut collector = NodePointsRuntimeCollector::new(
            NodePointsConfig::default(),
            NodePointsRuntimeHeuristics::default(),
        );
        let first = NodePointsRuntimeObservation {
            node_id: "node-a".to_string(),
            role: NodePointsRole::Observer,
            tick_count: 1,
            running: true,
            uptime_checks_passed: 1,
            uptime_checks_total: 1,
            storage_checks_passed: 0,
            storage_checks_total: 0,
            staked_storage_bytes: 0,
            observed_at_unix_ms: 8_100,
            has_error: false,
            effective_storage_bytes: 128,
            storage_challenge_proof_hint: None,
        };

        collector.observe(first.clone()).expect("first observe");
        collector
            .observe(NodePointsRuntimeObservation {
                node_id: "node-b".to_string(),
                observed_at_unix_ms: 2_000,
                ..first.clone()
            })
            .expect("older node observe");
        collector
            .observe(NodePointsRuntimeObservation {
                observed_at_unix_ms: 4_000,
                ..first
            })
            .expect("out-of-order node observe");

        assert_eq!(collector.latest_observed_at_unix_ms, 8_100);
    }

    #[test]
    fn collector_restores_cached_latest_timestamp_from_snapshot_cursors() {
        let mut collector = NodePointsRuntimeCollector::new(
            NodePointsConfig::default(),
            NodePointsRuntimeHeuristics::default(),
        );
        let first = NodePointsRuntimeObservation {
            node_id: "node-a".to_string(),
            role: NodePointsRole::Observer,
            tick_count: 1,
            running: true,
            uptime_checks_passed: 1,
            uptime_checks_total: 1,
            storage_checks_passed: 0,
            storage_checks_total: 0,
            staked_storage_bytes: 0,
            observed_at_unix_ms: 1_200,
            has_error: false,
            effective_storage_bytes: 128,
            storage_challenge_proof_hint: None,
        };
        collector.observe(first.clone()).expect("first observe");
        collector
            .observe(NodePointsRuntimeObservation {
                node_id: "node-b".to_string(),
                observed_at_unix_ms: 9_500,
                ..first
            })
            .expect("latest observe");

        let restored = NodePointsRuntimeCollector::from_snapshot(collector.snapshot());

        assert_eq!(restored.latest_observed_at_unix_ms, 9_500);
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn collector_auto_settles_when_epoch_elapsed() {
        let mut config = NodePointsConfig::default();
        config.epoch_pool_points = 50;
        config.epoch_duration_seconds = 1;
        let mut collector =
            NodePointsRuntimeCollector::new(config, NodePointsRuntimeHeuristics::default());

        let first = NodePointsRuntimeObservation {
            node_id: "node-a".to_string(),
            role: NodePointsRole::Observer,
            tick_count: 5,
            running: true,
            uptime_checks_passed: 1,
            uptime_checks_total: 1,
            storage_checks_passed: 0,
            storage_checks_total: 0,
            staked_storage_bytes: 0,
            observed_at_unix_ms: 0,
            has_error: false,
            effective_storage_bytes: 100,
            storage_challenge_proof_hint: None,
        };
        let second = NodePointsRuntimeObservation {
            tick_count: 20,
            observed_at_unix_ms: 1_200,
            ..first.clone()
        };

        assert!(collector.observe(first).expect("observe").is_none());
        let report = collector
            .observe(second)
            .expect("observe")
            .expect("epoch report");
        assert_eq!(report.pool_points, 50);
        assert_eq!(report.distributed_points, 50);
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn collector_propagates_uptime_challenge_counts_into_rewards() {
        let mut config = NodePointsConfig::default();
        config.epoch_pool_points = 100;
        config.epoch_duration_seconds = 10;
        config.min_uptime_challenge_pass_ratio = 0.5;
        config.weight_compute = 0.0;
        config.weight_storage = 0.0;
        config.weight_uptime = 1.0;
        config.weight_reliability = 0.0;
        let mut collector =
            NodePointsRuntimeCollector::new(config, NodePointsRuntimeHeuristics::default());

        let node_a_first = NodePointsRuntimeObservation {
            node_id: "node-a".to_string(),
            role: NodePointsRole::Observer,
            tick_count: 10,
            running: true,
            uptime_checks_passed: 1,
            uptime_checks_total: 1,
            storage_checks_passed: 0,
            storage_checks_total: 0,
            staked_storage_bytes: 0,
            observed_at_unix_ms: 0,
            has_error: false,
            effective_storage_bytes: 100,
            storage_challenge_proof_hint: None,
        };
        let node_a_second = NodePointsRuntimeObservation {
            tick_count: 11,
            observed_at_unix_ms: 500,
            ..node_a_first.clone()
        };
        let node_b_first = NodePointsRuntimeObservation {
            node_id: "node-b".to_string(),
            uptime_checks_passed: 0,
            uptime_checks_total: 1,
            ..node_a_first.clone()
        };
        let node_b_second = NodePointsRuntimeObservation {
            tick_count: 11,
            observed_at_unix_ms: 500,
            uptime_checks_passed: 1,
            uptime_checks_total: 1,
            ..node_b_first.clone()
        };

        assert!(collector.observe(node_a_first).expect("observe").is_none());
        assert!(collector.observe(node_a_second).expect("observe").is_none());
        assert!(collector.observe(node_b_first).expect("observe").is_none());
        assert!(collector.observe(node_b_second).expect("observe").is_none());

        let report = collector
            .force_settle()
            .expect("force settle")
            .expect("settlement");
        let settlement_a = report
            .settlements
            .iter()
            .find(|entry| entry.node_id == "node-a")
            .expect("node-a settlement");
        let settlement_b = report
            .settlements
            .iter()
            .find(|entry| entry.node_id == "node-b")
            .expect("node-b settlement");

        assert_eq!(settlement_a.uptime_score, 1.0);
        assert_eq!(settlement_b.uptime_score, 0.0);
        assert_eq!(settlement_a.awarded_points, 100);
        assert_eq!(settlement_b.awarded_points, 0);
    }

    #[test]
    #[expect(
        clippy::field_reassign_with_default,
        reason = "Test fixture construction intentionally starts from canonical defaults before overriding scenario-specific fields."
    )]
    fn collector_propagates_storage_challenge_counts_into_storage_rewards() {
        let mut config = NodePointsConfig::default();
        config.epoch_pool_points = 0;
        config.storage_pool_points = 100;
        config.epoch_duration_seconds = 1;
        config.min_storage_challenge_pass_ratio = 0.75;
        config.min_storage_challenge_checks = 2;
        config.weight_compute = 0.0;
        config.weight_storage = 0.0;
        config.weight_uptime = 0.0;
        config.weight_reliability = 0.0;
        let mut collector =
            NodePointsRuntimeCollector::new(config, NodePointsRuntimeHeuristics::default());

        let gib16 = 16_u64 * 1024 * 1024 * 1024;
        let node_a_first = NodePointsRuntimeObservation {
            node_id: "node-a".to_string(),
            role: NodePointsRole::Storage,
            tick_count: 10,
            running: true,
            uptime_checks_passed: 1,
            uptime_checks_total: 1,
            storage_checks_passed: 1,
            storage_checks_total: 1,
            staked_storage_bytes: gib16,
            observed_at_unix_ms: 0,
            has_error: false,
            effective_storage_bytes: gib16,
            storage_challenge_proof_hint: None,
        };
        let node_a_second = NodePointsRuntimeObservation {
            tick_count: 11,
            observed_at_unix_ms: 500,
            ..node_a_first.clone()
        };

        let node_b_first = NodePointsRuntimeObservation {
            node_id: "node-b".to_string(),
            storage_checks_passed: 0,
            storage_checks_total: 1,
            ..node_a_first.clone()
        };
        let node_b_second = NodePointsRuntimeObservation {
            tick_count: 11,
            observed_at_unix_ms: 500,
            storage_checks_passed: 1,
            storage_checks_total: 1,
            ..node_b_first.clone()
        };

        assert!(collector.observe(node_a_first).expect("observe").is_none());
        assert!(collector.observe(node_a_second).expect("observe").is_none());
        assert!(collector.observe(node_b_first).expect("observe").is_none());
        assert!(collector.observe(node_b_second).expect("observe").is_none());

        let report = collector
            .force_settle()
            .expect("force settle")
            .expect("settlement");
        let settlement_a = report
            .settlements
            .iter()
            .find(|entry| entry.node_id == "node-a")
            .expect("node-a settlement");
        let settlement_b = report
            .settlements
            .iter()
            .find(|entry| entry.node_id == "node-b")
            .expect("node-b settlement");

        assert_eq!(report.pool_points, 0);
        assert_eq!(report.storage_pool_points, 100);
        assert_eq!(report.distributed_points, 0);
        assert_eq!(report.storage_distributed_points, 100);
        assert_eq!(settlement_a.storage_awarded_points, 100);
        assert_eq!(settlement_b.storage_awarded_points, 0);
        assert!(settlement_a.storage_reward_score > 0.0);
        assert_eq!(settlement_b.storage_reward_score, 0.0);
        assert_eq!(settlement_a.awarded_points, 100);
        assert_eq!(settlement_b.awarded_points, 0);
    }
}
