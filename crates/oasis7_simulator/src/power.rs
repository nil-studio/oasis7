//! Power system types and logic for the M4 social system.

use serde::{Deserialize, Serialize};

use super::types::{AgentId, FacilityId, LocationId, ResourceOwner};

// ============================================================================
// Power State
// ============================================================================

/// The power state of an Agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentPowerState {
    /// Normal operation - power is sufficient.
    #[default]
    Normal,
    /// Low power mode - power is below 20% of capacity.
    /// Agent should conserve energy (reduced movement, slower decisions).
    LowPower,
    /// Critical power - power is below 5% of capacity.
    /// Agent can only accept charging or send distress signals.
    Critical,
    /// Shutdown - power is depleted.
    /// Agent is removed from scheduling until externally recharged.
    Shutdown,
}

impl AgentPowerState {
    /// Returns true if the agent can perform normal actions.
    pub fn can_act(&self) -> bool {
        matches!(self, AgentPowerState::Normal | AgentPowerState::LowPower)
    }

    /// Returns true if the agent can move.
    pub fn can_move(&self) -> bool {
        matches!(self, AgentPowerState::Normal | AgentPowerState::LowPower)
    }

    /// Returns true if the agent is shut down.
    pub fn is_shutdown(&self) -> bool {
        matches!(self, AgentPowerState::Shutdown)
    }

    /// Returns the state label for display.
    pub fn label(&self) -> &'static str {
        match self {
            AgentPowerState::Normal => "normal",
            AgentPowerState::LowPower => "low_power",
            AgentPowerState::Critical => "critical",
            AgentPowerState::Shutdown => "shutdown",
        }
    }
}

// ============================================================================
// Power Configuration
// ============================================================================

/// Configuration for the power system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PowerConfig {
    /// Power consumed per tick when idle (default: 1).
    pub idle_cost_per_tick: i64,
    /// Power consumed per decision (default: 1).
    pub decision_cost: i64,
    /// Base power capacity for new agents (default: 100).
    pub default_power_capacity: i64,
    /// Initial power level for new agents (default: 100).
    pub default_power_level: i64,
    /// Low power threshold as percentage (default: 20).
    pub low_power_threshold_pct: i64,
    /// Critical power threshold as percentage (default: 5).
    pub critical_threshold_pct: i64,
    /// Power transfer loss per km, in basis points (default: 10 = 0.1%).
    pub transfer_loss_per_km_bps: i64,
    /// Maximum cross-location transfer distance in km (default: 10_000).
    pub transfer_max_distance_km: i64,
    /// Enable dynamic market pricing for `BuyPower` / `SellPower`.
    pub dynamic_price_enabled: bool,
    /// Base market price (price unit per power unit).
    pub market_base_price_per_pu: i64,
    /// Minimum allowed market price.
    pub market_price_min_per_pu: i64,
    /// Maximum allowed market price.
    pub market_price_max_per_pu: i64,
    /// Max demand-pressure premium in basis points:
    /// `clamp((requested / available - 1) * 10_000, 0, cap)`.
    pub market_scarcity_price_max_bps: i64,
    /// Deprecated compatibility field (distance no longer affects market quote).
    pub market_distance_price_per_km_bps: i64,
    /// Allowed explicit price deviation from quote, in bps.
    pub market_price_band_bps: i64,
}

impl Default for PowerConfig {
    fn default() -> Self {
        Self {
            idle_cost_per_tick: 1,
            decision_cost: 1,
            default_power_capacity: 100,
            default_power_level: 100,
            low_power_threshold_pct: 20,
            critical_threshold_pct: 5,
            transfer_loss_per_km_bps: 10,
            transfer_max_distance_km: 10_000,
            dynamic_price_enabled: true,
            market_base_price_per_pu: 1,
            market_price_min_per_pu: 1,
            market_price_max_per_pu: 200,
            market_scarcity_price_max_bps: 30_000,
            market_distance_price_per_km_bps: 10,
            market_price_band_bps: 20_000,
        }
    }
}

impl PowerConfig {
    /// Calculate the power state based on current level and capacity.
    pub fn compute_state(&self, current: i64, capacity: i64) -> AgentPowerState {
        if current <= 0 {
            return AgentPowerState::Shutdown;
        }
        if capacity <= 0 {
            return AgentPowerState::Normal;
        }
        let pct = (current as i128 * 100) / capacity as i128;
        if pct <= self.critical_threshold_pct as i128 {
            AgentPowerState::Critical
        } else if pct <= self.low_power_threshold_pct as i128 {
            AgentPowerState::LowPower
        } else {
            AgentPowerState::Normal
        }
    }
}

// ============================================================================
// Power Status (per-Agent)
// ============================================================================

/// Power status tracking for an Agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentPowerStatus {
    /// Maximum power capacity.
    pub capacity: i64,
    /// Current power level.
    pub level: i64,
    /// Current power state (derived from level/capacity).
    pub state: AgentPowerState,
}

impl AgentPowerStatus {
    /// Create a new power status with default values.
    pub fn new(capacity: i64, level: i64) -> Self {
        let state = if level <= 0 {
            AgentPowerState::Shutdown
        } else {
            AgentPowerState::Normal
        };
        Self {
            capacity,
            level,
            state,
        }
    }

    /// Create from PowerConfig defaults.
    pub fn from_config(config: &PowerConfig) -> Self {
        Self::new(config.default_power_capacity, config.default_power_level)
    }

    /// Update state based on current level and config.
    pub fn update_state(&mut self, config: &PowerConfig) {
        self.state = config.compute_state(self.level, self.capacity);
    }

    /// Consume power, returning the amount actually consumed.
    /// Returns the actual amount consumed (may be less if insufficient).
    pub fn consume(&mut self, amount: i64, config: &PowerConfig) -> i64 {
        if amount <= 0 {
            return 0;
        }
        let consumed = amount.min(self.level);
        self.level -= consumed;
        self.update_state(config);
        consumed
    }

    /// Add power (e.g., charging).
    /// Returns the amount actually added (capped at capacity).
    pub fn charge(&mut self, amount: i64, config: &PowerConfig) -> i64 {
        if amount <= 0 {
            return 0;
        }
        let space = self.capacity - self.level;
        let added = amount.min(space.max(0));
        self.level += added;
        self.update_state(config);
        added
    }

    /// Check if the agent is shut down.
    pub fn is_shutdown(&self) -> bool {
        self.state.is_shutdown()
    }

    /// Get the power level as a percentage of capacity.
    pub fn level_pct(&self) -> i64 {
        if self.capacity <= 0 {
            return 100;
        }
        (self.level * 100) / self.capacity
    }
}

impl Default for AgentPowerStatus {
    fn default() -> Self {
        Self::from_config(&PowerConfig::default())
    }
}

// ============================================================================
// Power Facilities
// ============================================================================

/// Status of a power plant facility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlantStatus {
    #[default]
    Running,
    Offline,
    Maintenance,
}

/// Power generation facility.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PowerPlant {
    pub id: FacilityId,
    pub location_id: LocationId,
    pub owner: ResourceOwner,
    pub capacity_per_tick: i64,
    pub current_output: i64,
    pub fuel_cost_per_pu: i64,
    pub maintenance_cost: i64,
    pub status: PlantStatus,
    pub efficiency: f64,
    pub degradation: f64,
}

impl PowerPlant {
    pub fn new(
        id: FacilityId,
        location_id: LocationId,
        owner: ResourceOwner,
        capacity_per_tick: i64,
    ) -> Self {
        Self {
            id,
            location_id,
            owner,
            capacity_per_tick,
            current_output: 0,
            fuel_cost_per_pu: 0,
            maintenance_cost: 0,
            status: PlantStatus::Running,
            efficiency: 1.0,
            degradation: 0.0,
        }
    }

    /// Effective output after efficiency and degradation.
    pub fn effective_output(&self) -> i64 {
        if self.capacity_per_tick <= 0 {
            return 0;
        }
        let output = (self.capacity_per_tick as f64) * self.efficiency * (1.0 - self.degradation);
        output.floor().max(0.0) as i64
    }
}

// ============================================================================
// Power Events
// ============================================================================

/// Reason for power consumption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", content = "data")]
pub enum ConsumeReason {
    /// Idle consumption (per-tick baseline).
    Idle,
    /// Movement consumption.
    Move { distance_cm: i64 },
    /// Decision/computation consumption.
    Decision,
    /// Maintenance consumption (hardware degradation).
    Maintenance,
    /// Custom consumption with a label.
    Custom { name: String },
}

/// Power-related events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum PowerEvent {
    /// Power plant registered.
    PowerPlantRegistered { plant: PowerPlant },
    /// Power generated by a plant.
    PowerGenerated {
        plant_id: FacilityId,
        location_id: LocationId,
        amount: i64,
    },
    /// Power was consumed by an agent.
    PowerConsumed {
        agent_id: AgentId,
        amount: i64,
        reason: ConsumeReason,
        remaining: i64,
    },
    /// Agent's power state changed.
    PowerStateChanged {
        agent_id: AgentId,
        from: AgentPowerState,
        to: AgentPowerState,
        trigger_level: i64,
    },
    /// Power was transferred between owners.
    PowerTransferred {
        from: ResourceOwner,
        to: ResourceOwner,
        amount: i64,
        loss: i64,
        quoted_price_per_pu: i64,
        price_per_pu: i64,
        settlement_amount: i64,
    },
    /// Agent was charged (received power).
    PowerCharged {
        agent_id: AgentId,
        amount: i64,
        new_level: i64,
    },
}

/// Pre-submit opportunity-cost preview for `SellPower`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PowerSaleQuote {
    pub agent_id: AgentId,
    pub current_power_level: i64,
    pub power_state_before: String,
    pub sale_amount: i64,
    pub price_per_pu: i64,
    pub expected_revenue: i64,
    pub power_state_after_sale: String,
    pub remaining_runway_ticks: i64,
    pub next_action_affordability_after_sale: String,
    pub production_interrupt_risk: bool,
    pub recommended_sale_action: String,
    pub why_sale_is_safe_or_risky: String,
}

/// Pre-submit survival preview for `BuyPower` recovery actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PowerSurvivalQuote {
    pub agent_id: AgentId,
    pub current_power_level: i64,
    pub power_state_before: String,
    pub recovery_action: String,
    pub recovery_amount: i64,
    pub power_gain_estimate: i64,
    pub price_per_pu: i64,
    pub price_or_time_cost: i64,
    pub power_state_after_recovery: String,
    pub survival_runway_ticks: i64,
    pub next_action_affordability_after_recovery: String,
    pub shutdown_avoidance_reason: String,
    pub recommended_power_action: String,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_state_methods() {
        assert!(AgentPowerState::Normal.can_act());
        assert!(AgentPowerState::LowPower.can_act());
        assert!(!AgentPowerState::Critical.can_act());
        assert!(!AgentPowerState::Shutdown.can_act());

        assert!(!AgentPowerState::Normal.is_shutdown());
        assert!(AgentPowerState::Shutdown.is_shutdown());
    }

    #[test]
    fn power_config_compute_state() {
        let config = PowerConfig::default();

        // 100/100 = 100% -> Normal
        assert_eq!(config.compute_state(100, 100), AgentPowerState::Normal);

        // 50/100 = 50% -> Normal
        assert_eq!(config.compute_state(50, 100), AgentPowerState::Normal);

        // 20/100 = 20% -> LowPower (at threshold)
        assert_eq!(config.compute_state(20, 100), AgentPowerState::LowPower);

        // 15/100 = 15% -> LowPower
        assert_eq!(config.compute_state(15, 100), AgentPowerState::LowPower);

        // 5/100 = 5% -> Critical (at threshold)
        assert_eq!(config.compute_state(5, 100), AgentPowerState::Critical);

        // 3/100 = 3% -> Critical
        assert_eq!(config.compute_state(3, 100), AgentPowerState::Critical);

        // 0/100 = 0% -> Shutdown
        assert_eq!(config.compute_state(0, 100), AgentPowerState::Shutdown);

        // Negative -> Shutdown
        assert_eq!(config.compute_state(-10, 100), AgentPowerState::Shutdown);
    }

    #[test]
    fn power_status_consume() {
        let config = PowerConfig::default();
        let mut status = AgentPowerStatus::new(100, 100);

        // Consume 30
        let consumed = status.consume(30, &config);
        assert_eq!(consumed, 30);
        assert_eq!(status.level, 70);
        assert_eq!(status.state, AgentPowerState::Normal);

        // Consume 55 -> 15 remaining -> LowPower
        let consumed = status.consume(55, &config);
        assert_eq!(consumed, 55);
        assert_eq!(status.level, 15);
        assert_eq!(status.state, AgentPowerState::LowPower);

        // Consume 12 -> 3 remaining -> Critical
        let consumed = status.consume(12, &config);
        assert_eq!(consumed, 12);
        assert_eq!(status.level, 3);
        assert_eq!(status.state, AgentPowerState::Critical);

        // Consume 10 but only 3 available -> Shutdown
        let consumed = status.consume(10, &config);
        assert_eq!(consumed, 3);
        assert_eq!(status.level, 0);
        assert_eq!(status.state, AgentPowerState::Shutdown);
    }

    #[test]
    fn power_status_charge() {
        let config = PowerConfig::default();
        let mut status = AgentPowerStatus::new(100, 0);
        assert_eq!(status.state, AgentPowerState::Shutdown);

        // Charge 10 -> Critical
        let added = status.charge(10, &config);
        assert_eq!(added, 10);
        assert_eq!(status.level, 10);
        // 10% is between 5% and 20%, so LowPower
        assert_eq!(status.state, AgentPowerState::LowPower);

        // Charge 30 -> Normal
        let added = status.charge(30, &config);
        assert_eq!(added, 30);
        assert_eq!(status.level, 40);
        assert_eq!(status.state, AgentPowerState::Normal);

        // Charge 100 but only 60 space -> capped
        let added = status.charge(100, &config);
        assert_eq!(added, 60);
        assert_eq!(status.level, 100);
    }

    #[test]
    fn power_status_level_pct() {
        let status = AgentPowerStatus::new(100, 50);
        assert_eq!(status.level_pct(), 50);

        let status = AgentPowerStatus::new(200, 50);
        assert_eq!(status.level_pct(), 25);

        let status = AgentPowerStatus::new(0, 0);
        assert_eq!(status.level_pct(), 100); // Avoid division by zero
    }
}
