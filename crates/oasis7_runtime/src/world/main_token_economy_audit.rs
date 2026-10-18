use super::super::{
    MainTokenEconomyAnomalyAlert, MainTokenEconomyAuditReport, MainTokenEconomyAuditThresholds,
    WorldError,
};
use super::World;

impl World {
    pub fn main_token_economy_audit_report(
        &self,
        epoch_index: u64,
        thresholds: MainTokenEconomyAuditThresholds,
    ) -> MainTokenEconomyAuditReport {
        let mint_total = self.state.main_token_supply.total_issued;
        let burn_total = self.state.main_token_supply.total_burned;
        let net_flow = i128::from(mint_total) - i128::from(burn_total);
        let issued_this_epoch = self
            .state
            .main_token_epoch_issuance_records
            .get(&epoch_index)
            .map(|record| record.issued_amount)
            .unwrap_or(0);
        let treasury_distributed_this_epoch = self
            .state
            .main_token_treasury_distribution_records
            .values()
            .filter(|record| record.distributed_epoch == epoch_index)
            .fold(0_u64, |acc, record| acc.saturating_add(record.total_amount));

        let total_supply = self.state.main_token_supply.total_supply.max(1);
        let positive_net_flow = mint_total.saturating_sub(burn_total);
        let net_flow_bps_of_total_supply =
            Self::main_token_audit_ratio_bps(positive_net_flow, total_supply);
        let epoch_issued_bps_of_total_supply =
            Self::main_token_audit_ratio_bps(issued_this_epoch, total_supply);
        let treasury_distribution_bps_of_total_supply =
            Self::main_token_audit_ratio_bps(treasury_distributed_this_epoch, total_supply);

        let mut alerts = Vec::new();
        if net_flow_bps_of_total_supply > thresholds.max_net_flow_bps_of_total_supply {
            alerts.push(MainTokenEconomyAnomalyAlert {
                alert_id: format!("econ.net_flow.{epoch_index}"),
                metric: "net_flow_bps_of_total_supply".to_string(),
                observed_bps: net_flow_bps_of_total_supply,
                threshold_bps: thresholds.max_net_flow_bps_of_total_supply,
                exploit_signature: "inflation:net_flow_pressure".to_string(),
            });
        }
        if epoch_issued_bps_of_total_supply > thresholds.max_epoch_issued_bps_of_total_supply {
            alerts.push(MainTokenEconomyAnomalyAlert {
                alert_id: format!("econ.issued.{epoch_index}"),
                metric: "epoch_issued_bps_of_total_supply".to_string(),
                observed_bps: epoch_issued_bps_of_total_supply,
                threshold_bps: thresholds.max_epoch_issued_bps_of_total_supply,
                exploit_signature: "inflation:epoch_issued_pressure".to_string(),
            });
        }
        if treasury_distribution_bps_of_total_supply
            > thresholds.max_treasury_distribution_bps_of_total_supply
        {
            alerts.push(MainTokenEconomyAnomalyAlert {
                alert_id: format!("econ.treasury_distribution.{epoch_index}"),
                metric: "treasury_distribution_bps_of_total_supply".to_string(),
                observed_bps: treasury_distribution_bps_of_total_supply,
                threshold_bps: thresholds.max_treasury_distribution_bps_of_total_supply,
                exploit_signature: "arbitrage:treasury_distribution_pressure".to_string(),
            });
        }

        MainTokenEconomyAuditReport {
            epoch_index,
            mint_total,
            burn_total,
            net_flow,
            issued_this_epoch,
            treasury_distributed_this_epoch,
            net_flow_bps_of_total_supply,
            epoch_issued_bps_of_total_supply,
            treasury_distribution_bps_of_total_supply,
            alerts,
        }
    }

    pub fn enforce_main_token_economy_gate(
        &self,
        epoch_index: u64,
        thresholds: MainTokenEconomyAuditThresholds,
    ) -> Result<MainTokenEconomyAuditReport, WorldError> {
        let report = self.main_token_economy_audit_report(epoch_index, thresholds);
        if let Some(alert) = report.alerts.first() {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!(
                    "main token economy gate failed: alert_id={} exploit_signature={} observed_bps={} threshold_bps={}",
                    alert.alert_id,
                    alert.exploit_signature,
                    alert.observed_bps,
                    alert.threshold_bps,
                ),
            });
        }
        Ok(report)
    }

    fn main_token_audit_ratio_bps(numerator: u64, denominator: u64) -> u32 {
        if denominator == 0 {
            return 0;
        }
        let ratio = u128::from(numerator)
            .saturating_mul(10_000)
            .saturating_div(u128::from(denominator));
        ratio.min(10_000) as u32
    }
}
