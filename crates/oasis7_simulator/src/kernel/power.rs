use super::super::power::{AgentPowerState, ConsumeReason, PlantStatus, PowerEvent};
use super::super::types::{AgentId, FacilityId, ResourceKind};
use super::WorldKernel;
use super::types::WorldEvent;

const FACTORY_KIND_RADIATION_POWER_MK1: &str = "factory.power.radiation.mk1";

impl WorldKernel {
    /// Process power consumption for all agents (idle consumption).
    /// Returns a list of power events generated.
    pub fn process_power_tick(&mut self) -> Vec<WorldEvent> {
        let mut events = Vec::new();
        let idle_cost = self.config.power.idle_cost_per_tick;
        let power_config = self.config.power.clone();
        let thermal_capacity = self.config.physics.thermal_capacity;
        let thermal_dissipation = self.config.physics.thermal_dissipation;
        let thermal_dissipation_gradient_bps = self.config.physics.thermal_dissipation_gradient_bps;

        let mut pending_power_events = Vec::new();

        for (agent_id, agent) in self.model.agents.iter_mut() {
            if agent.power.is_shutdown() {
                continue;
            }

            let old_state = agent.power.state;
            let consumed = agent.power.consume(idle_cost, &power_config);
            let new_state = agent.power.state;
            let remaining = agent.power.level;
            let thermal_drop = Self::scaled_thermal_dissipation(
                agent.thermal.heat,
                thermal_capacity,
                thermal_dissipation,
                thermal_dissipation_gradient_bps,
            );
            if thermal_drop > 0 {
                agent.thermal.heat = agent.thermal.heat.saturating_sub(thermal_drop);
            }
            if consumed > 0 {
                pending_power_events.push(PowerEvent::PowerConsumed {
                    agent_id: agent_id.clone(),
                    amount: consumed,
                    reason: ConsumeReason::Idle,
                    remaining,
                });
            }

            if old_state != new_state {
                pending_power_events.push(PowerEvent::PowerStateChanged {
                    agent_id: agent_id.clone(),
                    from: old_state,
                    to: new_state,
                    trigger_level: remaining,
                });
            }
        }

        for power_event in pending_power_events {
            let event = self.record_event(super::types::WorldEventKind::Power(power_event));
            events.push(event);
        }

        events
    }

    fn scaled_thermal_dissipation(
        heat: i64,
        thermal_capacity: i64,
        thermal_dissipation: i64,
        thermal_dissipation_gradient_bps: i64,
    ) -> i64 {
        if heat <= 0 || thermal_dissipation <= 0 || thermal_dissipation_gradient_bps <= 0 {
            return 0;
        }

        let reference_heat = thermal_capacity.max(1) as i128;
        let numerator = (thermal_dissipation as i128)
            .saturating_mul(heat as i128)
            .saturating_mul(thermal_dissipation_gradient_bps as i128);
        let denominator = reference_heat.saturating_mul(10_000);
        let scaled = numerator
            .saturating_add(denominator.saturating_sub(1))
            .saturating_div(denominator);
        scaled.clamp(1, i64::MAX as i128) as i64
    }

    /// Process power generation for all power plants.
    /// Returns a list of power events generated.
    pub fn process_power_generation_tick(&mut self) -> Vec<WorldEvent> {
        let mut events = Vec::new();
        let plant_ids: Vec<FacilityId> = self.model.power_plants.keys().cloned().collect();

        for plant_id in plant_ids {
            let (base_output, location_id, owner) = {
                let plant = match self.model.power_plants.get_mut(&plant_id) {
                    Some(plant) => plant,
                    None => continue,
                };
                if plant.status != PlantStatus::Running {
                    plant.current_output = 0;
                    continue;
                }
                let output = plant.effective_output();
                (output, plant.location_id.clone(), plant.owner.clone())
            };

            if base_output <= 0 {
                continue;
            }
            let mut output = base_output;

            let is_radiation_power_factory =
                self.model.factories.get(&plant_id).is_some_and(|factory| {
                    factory
                        .kind
                        .eq_ignore_ascii_case(FACTORY_KIND_RADIATION_POWER_MK1)
                });
            if is_radiation_power_factory {
                let Some(location) = self.model.locations.get(&location_id) else {
                    continue;
                };
                let available_radiation = self.radiation_available_at(location.pos).max(0);
                output = output.min(available_radiation);
            }
            if let Some(plant) = self.model.power_plants.get_mut(&plant_id) {
                plant.current_output = output;
            }
            if output <= 0 {
                continue;
            }

            if self
                .add_to_owner(&owner, ResourceKind::Electricity, output)
                .is_err()
            {
                continue;
            }
            let power_event = PowerEvent::PowerGenerated {
                plant_id: plant_id.clone(),
                location_id: location_id.clone(),
                amount: output,
            };
            let event = self.record_event(super::types::WorldEventKind::Power(power_event));
            events.push(event);
        }

        events
    }

    /// Consume power from an agent for a specific reason.
    /// Returns the power event if power was consumed.
    pub fn consume_agent_power(
        &mut self,
        agent_id: &AgentId,
        amount: i64,
        reason: ConsumeReason,
    ) -> Option<WorldEvent> {
        let power_config = self.config.power.clone();

        let (consumed, remaining, old_state, new_state) = {
            let agent = self.model.agents.get_mut(agent_id)?;

            if agent.power.is_shutdown() {
                return None;
            }

            let old_state = agent.power.state;
            let consumed = agent.power.consume(amount, &power_config);
            let new_state = agent.power.state;

            if consumed == 0 {
                return None;
            }

            (consumed, agent.power.level, old_state, new_state)
        };

        let power_event = PowerEvent::PowerConsumed {
            agent_id: agent_id.clone(),
            amount: consumed,
            reason,
            remaining,
        };
        let event = self.record_event(super::types::WorldEventKind::Power(power_event));

        if old_state != new_state {
            let state_event = PowerEvent::PowerStateChanged {
                agent_id: agent_id.clone(),
                from: old_state,
                to: new_state,
                trigger_level: remaining,
            };
            self.record_event(super::types::WorldEventKind::Power(state_event));
        }

        Some(event)
    }

    /// Charge an agent's power.
    /// Returns the power event if power was added.
    pub fn charge_agent_power(&mut self, agent_id: &AgentId, amount: i64) -> Option<WorldEvent> {
        let power_config = self.config.power.clone();

        let (added, new_level, old_state, new_state) = {
            let agent = self.model.agents.get_mut(agent_id)?;

            let old_state = agent.power.state;
            let added = agent.power.charge(amount, &power_config);
            let new_state = agent.power.state;

            if added == 0 {
                return None;
            }

            (added, agent.power.level, old_state, new_state)
        };

        let power_event = PowerEvent::PowerCharged {
            agent_id: agent_id.clone(),
            amount: added,
            new_level,
        };
        let event = self.record_event(super::types::WorldEventKind::Power(power_event));

        if old_state != new_state {
            let state_event = PowerEvent::PowerStateChanged {
                agent_id: agent_id.clone(),
                from: old_state,
                to: new_state,
                trigger_level: new_level,
            };
            self.record_event(super::types::WorldEventKind::Power(state_event));
        }

        Some(event)
    }

    /// Get the power state of an agent.
    pub fn agent_power_state(&self, agent_id: &AgentId) -> Option<AgentPowerState> {
        self.model.agents.get(agent_id).map(|a| a.power.state)
    }

    /// Check if an agent is shut down.
    pub fn is_agent_shutdown(&self, agent_id: &AgentId) -> bool {
        self.model
            .agents
            .get(agent_id)
            .map(|a| a.power.is_shutdown())
            .unwrap_or(false)
    }

    /// Get all shutdown agents.
    pub fn shutdown_agents(&self) -> Vec<AgentId> {
        self.model
            .agents
            .iter()
            .filter(|(_, a)| a.power.is_shutdown())
            .map(|(id, _)| id.clone())
            .collect()
    }
}
