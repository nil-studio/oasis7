/// Resource estimates only; this value grants no action admission or execution authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecipeResourceCost {
    pub electricity_cost: i64,
    pub data_cost: i64,
}
/// The ledger whose affordability this read-only query checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleResourceScope {
    SimulatorStock,
    /// The host checks its own ledger separately, including again during execution.
    HostLedgers,
}
impl WorldKernel {
    pub fn recipe_resource_cost(
        &self,
        recipe_id: &str,
        batches: i64,
    ) -> Option<RecipeResourceCost> {
        if batches <= 0 {
            return None;
        }
        let plan = self.recipe_plan(recipe_id)?;
        Some(RecipeResourceCost {
            electricity_cost: plan.electricity_per_batch.saturating_mul(batches),
            data_cost: plan.hardware_per_batch.saturating_mul(batches),
        })
    }

    pub fn quote_schedule_recipe(
        &self,
        owner: &ResourceOwner,
        factory_id: &str,
        recipe_id: &str,
        batches: i64,
    ) -> Result<ScheduleQuote, RejectReason> {
        self.query_schedule_recipe(
            owner,
            factory_id,
            recipe_id,
            batches,
            ScheduleResourceScope::SimulatorStock,
        )
    }

    /// Query recipe constraints and costs using the explicitly selected ledger authority.
    /// Host-ledger results are advisory until the host performs its own affordability checks.
    pub fn query_schedule_recipe(
        &self,
        owner: &ResourceOwner,
        factory_id: &str,
        recipe_id: &str,
        batches: i64,
        resource_scope: ScheduleResourceScope,
    ) -> Result<ScheduleQuote, RejectReason> {
        if recipe_id.trim().is_empty() {
            return Err(RejectReason::RuleDenied {
                notes: vec!["recipe_id cannot be empty".to_string()],
            });
        }
        if batches <= 0 {
            return Err(RejectReason::InvalidAmount { amount: batches });
        }
        self.ensure_owner_exists(owner)?;

        let Some(factory) = self.model.factories.get(factory_id) else {
            return Err(RejectReason::FacilityNotFound {
                facility_id: factory_id.to_string(),
            });
        };
        if factory.owner != *owner {
            return Err(RejectReason::RuleDenied {
                notes: vec!["factory owner mismatch".to_string()],
            });
        }
        let site_owner = ResourceOwner::Location {
            location_id: factory.location_id.clone(),
        };
        self.ensure_colocated(owner, &site_owner)?;

        let Some(plan) = self.recipe_plan(recipe_id) else {
            return Err(RejectReason::RuleDenied {
                notes: vec![format!("unsupported recipe_id: {recipe_id}")],
            });
        };
        if !factory
            .kind
            .eq_ignore_ascii_case(plan.required_factory_kind)
        {
            return Err(RejectReason::RuleDenied {
                notes: vec![format!(
                    "recipe {recipe_id} requires factory kind {}, got {}",
                    plan.required_factory_kind, factory.kind
                )],
            });
        }

        let electricity_cost = plan.electricity_per_batch.saturating_mul(batches);
        let hardware_cost = plan.hardware_per_batch.saturating_mul(batches);
        let data_output = plan.data_output_per_batch.saturating_mul(batches);
        let finished_product_units = plan
            .finished_product_units_per_batch
            .saturating_mul(batches);

        let stock = self
            .owner_stock(owner)
            .expect("owner exists after ensure_owner_exists");
        let available_electricity = stock.get(ResourceKind::Electricity);
        if matches!(resource_scope, ScheduleResourceScope::SimulatorStock) {
            if available_electricity < electricity_cost {
                return Err(RejectReason::InsufficientResource {
                    owner: owner.clone(),
                    kind: ResourceKind::Electricity,
                    requested: electricity_cost,
                    available: available_electricity,
                });
            }
            let available_hardware = stock.get(ResourceKind::Data);
            if available_hardware < hardware_cost {
                return Err(RejectReason::InsufficientResource {
                    owner: owner.clone(),
                    kind: ResourceKind::Data,
                    requested: hardware_cost,
                    available: available_hardware,
                });
            }
            if data_output > 0 {
                let data_after_cost = available_hardware.checked_sub(hardware_cost).ok_or(
                    RejectReason::InsufficientResource {
                        owner: owner.clone(),
                        kind: ResourceKind::Data,
                        requested: hardware_cost,
                        available: available_hardware,
                    },
                )?;
                if data_after_cost.checked_add(data_output).is_none() {
                    return Err(RejectReason::InvalidAmount {
                        amount: data_output,
                    });
                }
            }
        }

        let electricity_after = available_electricity.saturating_sub(electricity_cost);
        let (runway_before_ticks, runway_after_ticks, downtime_threshold_ppm, risk_is_elevated) =
            match owner {
                ResourceOwner::Agent { agent_id } => {
                    let agent = self
                        .model
                        .agents
                        .get(agent_id)
                        .expect("agent exists after ensure_owner_exists");
                    let power_state = self
                        .config
                        .power
                        .compute_state(agent.power.level, agent.power.capacity);
                    let idle_cost_per_tick = self.config.power.idle_cost_per_tick;
                    let runway_ticks = if matches!(power_state, AgentPowerState::Shutdown) {
                        0
                    } else if idle_cost_per_tick <= 0 {
                        i64::MAX
                    } else {
                        agent.power.level.max(0) / idle_cost_per_tick
                    };
                    let critical_threshold_pct =
                        self.config.power.critical_threshold_pct.clamp(0, 100);
                    (
                        runway_ticks,
                        runway_ticks,
                        critical_threshold_pct.saturating_mul(10_000),
                        matches!(
                            power_state,
                            AgentPowerState::Critical | AgentPowerState::Shutdown
                        ),
                    )
                }
                ResourceOwner::Location { .. } => (i64::MAX, i64::MAX, 0, false),
            };
        let continue_production_risk = if risk_is_elevated { "elevated" } else { "low" };
        let maintenance_pressure_delta = "unchanged";
        let recommended_pre_step = if risk_is_elevated {
            "restore_power_before_scheduling"
        } else {
            "schedule_now"
        };
        let recommended_maintenance_action = if risk_is_elevated {
            "restore_power"
        } else {
            "continue_production"
        };

        let Some(base_duration_ticks) = crate::canonical_recipe_base_duration_ticks(recipe_id)
        else {
            return Err(RejectReason::RuleDenied {
                notes: vec![format!(
                    "recipe has no canonical base duration: {recipe_id}"
                )],
            });
        };

        Ok(ScheduleQuote {
            owner: owner.clone(),
            factory_id: factory_id.to_string(),
            recipe_id: recipe_id.to_string(),
            batches,
            base_duration_ticks: i64::from(base_duration_ticks),
            electricity_cost,
            electricity_after,
            hardware_cost,
            data_output,
            finished_product_id: plan.finished_product_id.to_string(),
            finished_product_units,
            local_shortage_delay_ticks: 0,
            shortage_reason: "none".to_string(),
            recommended_pre_step: recommended_pre_step.to_string(),
            runway_before_ticks,
            runway_after_ticks,
            downtime_threshold_ppm,
            continue_production_risk: continue_production_risk.to_string(),
            maintenance_pressure_delta: maintenance_pressure_delta.to_string(),
            recommended_maintenance_action: recommended_maintenance_action.to_string(),
        })
    }
}
