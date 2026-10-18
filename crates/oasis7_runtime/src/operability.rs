use serde::{Deserialize, Serialize};

use super::main_token::{MainTokenEconomyAuditReport, MainTokenEconomyAuditThresholds};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LongRunReleaseStage {
    #[default]
    Canary,
    Full,
}

impl LongRunReleaseStage {
    pub fn rank(self) -> u8 {
        match self {
            LongRunReleaseStage::Canary => 1,
            LongRunReleaseStage::Full => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LongRunOperabilityReleaseGateThresholds {
    pub max_logistics_breach_bps: u16,
    pub max_pending_actions_evicted: u64,
    pub max_journal_events_evicted: u64,
    pub max_tick_consensus_rejections: usize,
    pub min_rollback_drills: usize,
    pub required_release_stage: LongRunReleaseStage,
    pub economy_thresholds: MainTokenEconomyAuditThresholds,
}

impl Default for LongRunOperabilityReleaseGateThresholds {
    fn default() -> Self {
        Self {
            max_logistics_breach_bps: 500,
            max_pending_actions_evicted: 0,
            max_journal_events_evicted: 0,
            max_tick_consensus_rejections: 0,
            min_rollback_drills: 1,
            required_release_stage: LongRunReleaseStage::Full,
            economy_thresholds: MainTokenEconomyAuditThresholds::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LongRunOperabilityGateViolation {
    pub gate: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LongRunOperabilityReleaseGateReport {
    pub release_stage: LongRunReleaseStage,
    pub required_release_stage: LongRunReleaseStage,
    pub logistics_breach_bps: u16,
    pub pending_actions_evicted: u64,
    pub journal_events_evicted: u64,
    pub tick_consensus_rejections: usize,
    pub rollback_drill_count: usize,
    pub emergency_brake_active: bool,
    pub economy_report: MainTokenEconomyAuditReport,
    pub violations: Vec<LongRunOperabilityGateViolation>,
}

impl LongRunOperabilityReleaseGateReport {
    pub fn gate_passed(&self) -> bool {
        self.violations.is_empty()
    }
}
