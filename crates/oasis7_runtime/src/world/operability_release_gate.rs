use super::super::{
    LongRunOperabilityGateViolation, LongRunOperabilityReleaseGateReport,
    LongRunOperabilityReleaseGateThresholds, LongRunReleaseStage, WorldError, WorldEventBody,
};
use super::World;

impl World {
    pub fn evaluate_longrun_operability_release_gate(
        &self,
        release_stage: LongRunReleaseStage,
        economy_epoch_index: u64,
        thresholds: LongRunOperabilityReleaseGateThresholds,
    ) -> LongRunOperabilityReleaseGateReport {
        let logistics = self.logistics_sla_metrics();
        let logistics_breach_bps = if logistics.completed_transits == 0 {
            0
        } else {
            ((u128::from(logistics.breached_transits).saturating_mul(10_000))
                / u128::from(logistics.completed_transits)) as u16
        };
        let backpressure = self.runtime_backpressure_stats();
        let tick_consensus_rejections = self.tick_consensus_rejection_audit_events().len();
        let rollback_drill_count = self
            .journal()
            .events
            .iter()
            .filter(|event| matches!(&event.body, WorldEventBody::RollbackApplied(_)))
            .count();
        let emergency_brake_active = self
            .governance_emergency_brake_until_tick()
            .is_some_and(|until| self.state().time < until);
        let economy_report = self
            .main_token_economy_audit_report(economy_epoch_index, thresholds.economy_thresholds);

        let mut violations = Vec::new();
        if release_stage.rank() < thresholds.required_release_stage.rank() {
            violations.push(LongRunOperabilityGateViolation {
                gate: "rollout.stage".to_string(),
                reason: format!(
                    "release stage {:?} is below required {:?}",
                    release_stage, thresholds.required_release_stage
                ),
            });
        }
        if logistics_breach_bps > thresholds.max_logistics_breach_bps {
            violations.push(LongRunOperabilityGateViolation {
                gate: "slo.logistics_breach_bps".to_string(),
                reason: format!(
                    "logistics breach bps={} exceeds threshold={}",
                    logistics_breach_bps, thresholds.max_logistics_breach_bps
                ),
            });
        }
        if backpressure.pending_actions_evicted > thresholds.max_pending_actions_evicted {
            violations.push(LongRunOperabilityGateViolation {
                gate: "alerts.pending_actions_evicted".to_string(),
                reason: format!(
                    "pending_actions_evicted={} exceeds threshold={}",
                    backpressure.pending_actions_evicted, thresholds.max_pending_actions_evicted
                ),
            });
        }
        if backpressure.journal_events_evicted > thresholds.max_journal_events_evicted {
            violations.push(LongRunOperabilityGateViolation {
                gate: "alerts.journal_events_evicted".to_string(),
                reason: format!(
                    "journal_events_evicted={} exceeds threshold={}",
                    backpressure.journal_events_evicted, thresholds.max_journal_events_evicted
                ),
            });
        }
        if tick_consensus_rejections > thresholds.max_tick_consensus_rejections {
            violations.push(LongRunOperabilityGateViolation {
                gate: "consensus.rejection_audits".to_string(),
                reason: format!(
                    "tick_consensus_rejections={} exceeds threshold={}",
                    tick_consensus_rejections, thresholds.max_tick_consensus_rejections
                ),
            });
        }
        if rollback_drill_count < thresholds.min_rollback_drills {
            violations.push(LongRunOperabilityGateViolation {
                gate: "drill.rollback".to_string(),
                reason: format!(
                    "rollback_drill_count={} below minimum={}",
                    rollback_drill_count, thresholds.min_rollback_drills
                ),
            });
        }
        if emergency_brake_active {
            violations.push(LongRunOperabilityGateViolation {
                gate: "governance.emergency_brake".to_string(),
                reason: "governance emergency brake is active".to_string(),
            });
        }
        for alert in &economy_report.alerts {
            violations.push(LongRunOperabilityGateViolation {
                gate: format!("economy.{}", alert.metric),
                reason: format!(
                    "economy alert {} ({}) observed={} threshold={}",
                    alert.alert_id,
                    alert.exploit_signature,
                    alert.observed_bps,
                    alert.threshold_bps
                ),
            });
        }

        LongRunOperabilityReleaseGateReport {
            release_stage,
            required_release_stage: thresholds.required_release_stage,
            logistics_breach_bps,
            pending_actions_evicted: backpressure.pending_actions_evicted,
            journal_events_evicted: backpressure.journal_events_evicted,
            tick_consensus_rejections,
            rollback_drill_count,
            emergency_brake_active,
            economy_report,
            violations,
        }
    }

    pub fn enforce_longrun_operability_release_gate(
        &self,
        release_stage: LongRunReleaseStage,
        economy_epoch_index: u64,
        thresholds: LongRunOperabilityReleaseGateThresholds,
    ) -> Result<LongRunOperabilityReleaseGateReport, WorldError> {
        let report = self.evaluate_longrun_operability_release_gate(
            release_stage,
            economy_epoch_index,
            thresholds,
        );
        if let Some(violation) = report.violations.first() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "longrun operability release gate blocked: gate={} reason={}",
                    violation.gate, violation.reason
                ),
            });
        }
        Ok(report)
    }
}
