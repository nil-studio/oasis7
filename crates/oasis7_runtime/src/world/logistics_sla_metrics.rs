use super::{MaterialTransitPriority, WorldTime};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogisticsSlaMetrics {
    pub completed_transits: u64,
    pub fulfilled_transits: u64,
    pub breached_transits: u64,
    pub total_delay_ticks: u64,
    pub urgent_completed_transits: u64,
    pub urgent_fulfilled_transits: u64,
    pub urgent_breached_transits: u64,
    pub urgent_total_delay_ticks: u64,
}

impl LogisticsSlaMetrics {
    pub(super) fn record_completion(
        &mut self,
        expected_ready_at: WorldTime,
        completed_at: WorldTime,
        priority: MaterialTransitPriority,
    ) {
        self.completed_transits = self.completed_transits.saturating_add(1);
        if priority == MaterialTransitPriority::Urgent {
            self.urgent_completed_transits = self.urgent_completed_transits.saturating_add(1);
        }
        if completed_at > expected_ready_at {
            let delay = completed_at.saturating_sub(expected_ready_at);
            self.breached_transits = self.breached_transits.saturating_add(1);
            self.total_delay_ticks = self.total_delay_ticks.saturating_add(delay);
            if priority == MaterialTransitPriority::Urgent {
                self.urgent_breached_transits = self.urgent_breached_transits.saturating_add(1);
                self.urgent_total_delay_ticks = self.urgent_total_delay_ticks.saturating_add(delay);
            }
        } else {
            self.fulfilled_transits = self.fulfilled_transits.saturating_add(1);
            if priority == MaterialTransitPriority::Urgent {
                self.urgent_fulfilled_transits = self.urgent_fulfilled_transits.saturating_add(1);
            }
        }
    }

    pub fn breach_rate(&self) -> f64 {
        if self.completed_transits == 0 {
            return 0.0;
        }
        self.breached_transits as f64 / self.completed_transits as f64
    }

    pub fn fulfillment_rate(&self) -> f64 {
        if self.completed_transits == 0 {
            return 1.0;
        }
        self.fulfilled_transits as f64 / self.completed_transits as f64
    }

    pub fn average_delay_ticks(&self) -> f64 {
        if self.completed_transits == 0 {
            return 0.0;
        }
        self.total_delay_ticks as f64 / self.completed_transits as f64
    }
}
