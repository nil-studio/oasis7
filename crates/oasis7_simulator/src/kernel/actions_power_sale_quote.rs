impl WorldKernel {
    pub fn quote_harvest_radiation_survival(
        &self,
        buyer: &ResourceOwner,
        max_amount: i64,
    ) -> Result<PowerSurvivalQuote, RejectReason> {
        let ResourceOwner::Agent { agent_id } = buyer else {
            return Err(RejectReason::RuleDenied {
                notes: vec![LOCATION_ELECTRICITY_POOL_REMOVED_NOTE.to_string()],
            });
        };
        let agent = self
            .model
            .agents
            .get(agent_id)
            .ok_or_else(|| RejectReason::AgentNotFound {
                agent_id: agent_id.clone(),
            })?;
        let location_pos = self
            .model
            .locations
            .get(&agent.location_id)
            .ok_or_else(|| RejectReason::LocationNotFound {
                location_id: agent.location_id.clone(),
            })?
            .pos;
        let mut projection = self.clone();
        projection.ensure_chunk_generated_at(location_pos, ChunkGenerationCause::Action)?;
        projection.quote_harvest_radiation_survival_generated(buyer, max_amount)
    }

    fn quote_harvest_radiation_survival_generated(
        &self,
        buyer: &ResourceOwner,
        max_amount: i64,
    ) -> Result<PowerSurvivalQuote, RejectReason> {
        if max_amount <= 0 {
            return Err(RejectReason::InvalidAmount { amount: max_amount });
        }
        let ResourceOwner::Agent { agent_id } = buyer else {
            return Err(RejectReason::RuleDenied {
                notes: vec![LOCATION_ELECTRICITY_POOL_REMOVED_NOTE.to_string()],
            });
        };
        let agent = self
            .model
            .agents
            .get(agent_id)
            .ok_or_else(|| RejectReason::AgentNotFound {
                agent_id: agent_id.clone(),
            })?;
        let location = self
            .model
            .locations
            .get(&agent.location_id)
            .ok_or_else(|| RejectReason::LocationNotFound {
                location_id: agent.location_id.clone(),
            })?;
        let local_available = self.radiation_available_at(location.pos);
        if local_available <= 0 {
            return Err(RejectReason::RadiationUnavailable {
                location_id: agent.location_id.clone(),
            });
        }

        let physics = &self.config.physics;
        let capped_available = if physics.max_harvest_per_tick > 0 {
            local_available.min(physics.max_harvest_per_tick)
        } else {
            local_available
        };
        let mut power_gain_estimate = max_amount.min(capped_available);
        if physics.thermal_capacity > 0 && agent.thermal.heat > physics.thermal_capacity {
            let ratio =
                (physics.thermal_capacity as f64 / agent.thermal.heat as f64).clamp(0.1, 1.0);
            power_gain_estimate = (power_gain_estimate as f64 * ratio).floor() as i64;
            if power_gain_estimate <= 0 {
                return Err(RejectReason::ThermalOverload {
                    heat: agent.thermal.heat,
                    capacity: physics.thermal_capacity,
                });
            }
        }

        let current_power_level = agent.resources.get(ResourceKind::Electricity);
        let recovered_power = current_power_level.checked_add(power_gain_estimate).ok_or(
            RejectReason::InvalidAmount {
                amount: power_gain_estimate,
            },
        )?;
        let power_capacity = agent.power.capacity;
        let power_state_before_value = self
            .config
            .power
            .compute_state(current_power_level, power_capacity);
        let power_state_after_value = self
            .config
            .power
            .compute_state(recovered_power, power_capacity);
        let idle_cost = self.config.power.idle_cost_per_tick.max(1);
        let survival_runway_ticks = recovered_power / idle_cost;
        let next_action_affordability_after_recovery =
            self.power_next_action_affordability(recovered_power, power_capacity);
        let recommended_power_action = self.power_survival_recommended_action(
            power_state_after_value,
            next_action_affordability_after_recovery,
        );
        let shutdown_avoidance_reason = self.power_survival_recovery_summary(
            power_state_before_value,
            power_state_after_value,
            survival_runway_ticks,
            &recommended_power_action,
        );

        Ok(PowerSurvivalQuote {
            agent_id: agent_id.clone(),
            current_power_level,
            power_state_before: power_state_before_value.label().to_string(),
            recovery_action: "harvest_radiation".to_string(),
            recovery_amount: max_amount,
            power_gain_estimate,
            price_per_pu: 0,
            price_or_time_cost: 0,
            power_state_after_recovery: power_state_after_value.label().to_string(),
            survival_runway_ticks,
            next_action_affordability_after_recovery: next_action_affordability_after_recovery
                .to_string(),
            shutdown_avoidance_reason,
            recommended_power_action,
        })
    }

    pub fn quote_power_survival(
        &self,
        buyer: &ResourceOwner,
        seller: &ResourceOwner,
        amount: i64,
        requested_price_per_pu: i64,
    ) -> Result<PowerSurvivalQuote, RejectReason> {
        if requested_price_per_pu < 0 {
            return Err(RejectReason::InvalidAmount {
                amount: requested_price_per_pu,
            });
        }

        let ResourceOwner::Agent { agent_id } = buyer else {
            return Err(RejectReason::RuleDenied {
                notes: vec![LOCATION_ELECTRICITY_POOL_REMOVED_NOTE.to_string()],
            });
        };
        let buyer_agent =
            self.model
                .agents
                .get(agent_id)
                .ok_or_else(|| RejectReason::AgentNotFound {
                    agent_id: agent_id.clone(),
                })?;
        let current_power_level = buyer_agent.resources.get(ResourceKind::Electricity);
        let power_capacity = buyer_agent.power.capacity;
        let power_state_before_value = self
            .config
            .power
            .compute_state(current_power_level, power_capacity);
        let power_state_before = power_state_before_value.label().to_string();

        let prepared = self.prepare_power_transfer_read_only(seller, buyer, amount)?;
        let price_per_pu = self
            .resolve_power_transfer_price(requested_price_per_pu, prepared.quoted_price_per_pu)?;
        let power_gain_estimate = amount - prepared.loss;
        let price_or_time_cost = power_gain_estimate.saturating_mul(price_per_pu);
        let recovered_power = current_power_level.checked_add(power_gain_estimate).ok_or(
            RejectReason::InvalidAmount {
                amount: power_gain_estimate,
            },
        )?;
        let power_state_after_value = self
            .config
            .power
            .compute_state(recovered_power, power_capacity);
        let power_state_after_recovery = power_state_after_value.label().to_string();
        let idle_cost = self.config.power.idle_cost_per_tick.max(1);
        let survival_runway_ticks = recovered_power / idle_cost;
        let next_action_affordability_after_recovery =
            self.power_next_action_affordability(recovered_power, power_capacity);
        let recommended_power_action = self.power_survival_recommended_action(
            power_state_after_value,
            next_action_affordability_after_recovery,
        );
        let shutdown_avoidance_reason = self.power_survival_recovery_summary(
            power_state_before_value,
            power_state_after_value,
            survival_runway_ticks,
            &recommended_power_action,
        );

        Ok(PowerSurvivalQuote {
            agent_id: agent_id.clone(),
            current_power_level,
            power_state_before,
            recovery_action: "buy_power".to_string(),
            recovery_amount: amount,
            power_gain_estimate,
            price_per_pu,
            price_or_time_cost,
            power_state_after_recovery,
            survival_runway_ticks,
            next_action_affordability_after_recovery: next_action_affordability_after_recovery
                .to_string(),
            shutdown_avoidance_reason,
            recommended_power_action,
        })
    }

    pub fn quote_power_sale(
        &self,
        seller: &ResourceOwner,
        buyer: &ResourceOwner,
        amount: i64,
        requested_price_per_pu: i64,
    ) -> Result<PowerSaleQuote, RejectReason> {
        if requested_price_per_pu < 0 {
            return Err(RejectReason::InvalidAmount {
                amount: requested_price_per_pu,
            });
        }

        let ResourceOwner::Agent { agent_id } = seller else {
            return Err(RejectReason::RuleDenied {
                notes: vec![LOCATION_ELECTRICITY_POOL_REMOVED_NOTE.to_string()],
            });
        };
        let seller_agent =
            self.model
                .agents
                .get(agent_id)
                .ok_or_else(|| RejectReason::AgentNotFound {
                    agent_id: agent_id.clone(),
                })?;
        let current_power_level = seller_agent.resources.get(ResourceKind::Electricity);
        let power_capacity = seller_agent.power.capacity;
        let power_state_before = self
            .config
            .power
            .compute_state(current_power_level, power_capacity)
            .label()
            .to_string();

        let prepared = self.prepare_power_transfer_read_only(seller, buyer, amount)?;
        let price_per_pu = self
            .resolve_power_transfer_price(requested_price_per_pu, prepared.quoted_price_per_pu)?;
        let delivered = amount - prepared.loss;
        let expected_revenue = delivered.saturating_mul(price_per_pu);
        let remaining_power = current_power_level.saturating_sub(amount);
        let power_state_after_sale = self
            .config
            .power
            .compute_state(remaining_power, power_capacity)
            .label()
            .to_string();
        let idle_cost = self.config.power.idle_cost_per_tick.max(1);
        let remaining_runway_ticks = remaining_power / idle_cost;
        let next_action_affordability_after_sale =
            self.power_next_action_affordability(remaining_power, power_capacity);
        let production_interrupt_risk = matches!(
            self.config
                .power
                .compute_state(remaining_power, power_capacity),
            AgentPowerState::Critical | AgentPowerState::Shutdown
        );
        let recommended_sale_action = self.power_sale_recommended_action(
            amount,
            current_power_level,
            remaining_power,
            power_capacity,
            production_interrupt_risk,
        );
        let why_sale_is_safe_or_risky = self.power_sale_risk_summary(
            remaining_power,
            remaining_runway_ticks,
            production_interrupt_risk,
            recommended_sale_action,
        );

        Ok(PowerSaleQuote {
            agent_id: agent_id.clone(),
            current_power_level,
            power_state_before,
            sale_amount: amount,
            price_per_pu,
            expected_revenue,
            power_state_after_sale,
            remaining_runway_ticks,
            next_action_affordability_after_sale: next_action_affordability_after_sale.to_string(),
            production_interrupt_risk,
            recommended_sale_action: recommended_sale_action.to_string(),
            why_sale_is_safe_or_risky,
        })
    }

    fn prepare_power_transfer_read_only(
        &self,
        from: &ResourceOwner,
        to: &ResourceOwner,
        amount: i64,
    ) -> Result<PreparedPowerTransfer, RejectReason> {
        if amount <= 0 {
            return Err(RejectReason::InvalidAmount { amount });
        }
        self.ensure_owner_exists(from)?;
        self.ensure_owner_exists(to)?;
        if matches!(from, ResourceOwner::Location { .. })
            || matches!(to, ResourceOwner::Location { .. })
        {
            return Err(RejectReason::RuleDenied {
                notes: vec![LOCATION_ELECTRICITY_POOL_REMOVED_NOTE.to_string()],
            });
        }

        let from_location = self.owner_location_id(from)?;
        let to_location = self.owner_location_id(to)?;
        if matches!(from, ResourceOwner::Agent { .. }) || matches!(to, ResourceOwner::Agent { .. })
        {
            self.ensure_colocated(from, to)?;
        }

        let seller_available_before = self
            .owner_stock(from)
            .map(|stock| stock.get(ResourceKind::Electricity))
            .unwrap_or(0);
        if seller_available_before < amount {
            return Err(RejectReason::InsufficientResource {
                owner: from.clone(),
                kind: ResourceKind::Electricity,
                requested: amount,
                available: seller_available_before,
            });
        }

        let loss = if from_location != to_location {
            let distance_km = self.power_transfer_distance_km(&from_location, &to_location)?;
            let max_distance_km = self.config.power.transfer_max_distance_km;
            if distance_km > max_distance_km {
                return Err(RejectReason::PowerTransferDistanceExceeded {
                    distance_km,
                    max_distance_km,
                });
            }
            let loss = self.power_transfer_loss(amount, distance_km);
            if loss >= amount {
                return Err(RejectReason::PowerTransferLossExceedsAmount { amount, loss });
            }
            loss
        } else {
            0
        };

        Ok(PreparedPowerTransfer {
            loss,
            quoted_price_per_pu: self
                .quote_power_market_price_per_pu(amount, seller_available_before),
        })
    }

    fn resolve_power_transfer_price(
        &self,
        requested_price_per_pu: i64,
        quoted_price_per_pu: i64,
    ) -> Result<i64, RejectReason> {
        if self.config.power.dynamic_price_enabled {
            if requested_price_per_pu == 0 {
                return Ok(quoted_price_per_pu);
            }
            let price_band_bps = self.config.power.market_price_band_bps;
            let quote = quoted_price_per_pu.max(1) as i128;
            let deviation_bps = ((requested_price_per_pu as i128 - quoted_price_per_pu as i128)
                .abs()
                .saturating_mul(10_000))
            .saturating_div(quote);
            if deviation_bps > price_band_bps as i128 {
                return Err(RejectReason::RuleDenied {
                    notes: vec![format!(
                        "requested power price {} out of band (quote {}, band_bps {}, deviation_bps {})",
                        requested_price_per_pu, quoted_price_per_pu, price_band_bps, deviation_bps
                    )],
                });
            }
            Ok(requested_price_per_pu)
        } else {
            Ok(requested_price_per_pu)
        }
    }

    fn power_next_action_affordability(
        &self,
        remaining_power: i64,
        power_capacity: i64,
    ) -> &'static str {
        let decision_cost = self.config.power.decision_cost.max(1);
        let idle_cost = self.config.power.idle_cost_per_tick.max(1);
        let move_cost = self.config.movement_cost(1).max(1);
        if matches!(
            self.config
                .power
                .compute_state(remaining_power, power_capacity),
            AgentPowerState::Normal
        ) && remaining_power >= decision_cost + idle_cost + move_cost
        {
            "healthy"
        } else if remaining_power >= decision_cost.max(idle_cost).max(move_cost) {
            "limited"
        } else {
            "blocked"
        }
    }

    fn power_survival_recommended_action(
        &self,
        power_state_after_recovery: AgentPowerState,
        next_action_affordability_after_recovery: &str,
    ) -> String {
        if matches!(
            power_state_after_recovery,
            AgentPowerState::Critical | AgentPowerState::Shutdown
        ) {
            "buy_more_power".to_string()
        } else if matches!(next_action_affordability_after_recovery, "healthy") {
            "buy_power".to_string()
        } else {
            "buy_power_partial".to_string()
        }
    }

    fn power_survival_recovery_summary(
        &self,
        power_state_before: AgentPowerState,
        power_state_after_recovery: AgentPowerState,
        survival_runway_ticks: i64,
        recommended_power_action: &str,
    ) -> String {
        if matches!(
            power_state_before,
            AgentPowerState::Critical | AgentPowerState::Shutdown
        ) && power_state_after_recovery.can_act()
        {
            format!(
                "recovery restores {} runway ticks and lifts agent from {} to {}; recommended action: {}",
                survival_runway_ticks,
                power_state_before.label(),
                power_state_after_recovery.label(),
                recommended_power_action
            )
        } else {
            format!(
                "recovery leaves agent in {} with {} runway ticks; recommended action: {}",
                power_state_after_recovery.label(),
                survival_runway_ticks,
                recommended_power_action
            )
        }
    }

    fn power_sale_recommended_action(
        &self,
        amount: i64,
        current_power_level: i64,
        remaining_power: i64,
        power_capacity: i64,
        production_interrupt_risk: bool,
    ) -> &'static str {
        if production_interrupt_risk {
            "defer_sale"
        } else if amount.saturating_mul(2) >= current_power_level
            || matches!(
                self.power_next_action_affordability(remaining_power, power_capacity),
                "limited"
            )
        {
            "sell_partial"
        } else {
            "sell_full"
        }
    }

    fn power_sale_risk_summary(
        &self,
        remaining_power: i64,
        remaining_runway_ticks: i64,
        production_interrupt_risk: bool,
        recommended_sale_action: &str,
    ) -> String {
        if production_interrupt_risk {
            format!(
                "sale would leave {} power and {} critical power runway ticks; recommended action: {}",
                remaining_power, remaining_runway_ticks, recommended_sale_action
            )
        } else {
            format!(
                "sale leaves {} power and {} runway ticks; recommended action: {}",
                remaining_power, remaining_runway_ticks, recommended_sale_action
            )
        }
    }
}
