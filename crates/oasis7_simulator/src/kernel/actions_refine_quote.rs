impl WorldKernel {
    pub fn quote_refine_compound(
        &self,
        owner: &ResourceOwner,
        compound_mass_g: i64,
    ) -> Result<RefineQuote, RejectReason> {
        if compound_mass_g <= 0 {
            return Err(RejectReason::InvalidAmount {
                amount: compound_mass_g,
            });
        }
        self.ensure_owner_exists(owner)?;

        let (electricity_cost, hardware_output) =
            self.compute_refine_compound_outputs(compound_mass_g);
        if hardware_output <= 0 {
            return Err(RejectReason::InvalidAmount {
                amount: compound_mass_g,
            });
        }

        let stock = self
            .owner_stock(owner)
            .expect("owner exists after ensure_owner_exists");
        let available_compound = stock.get(ResourceKind::Data);
        if available_compound < compound_mass_g {
            return Err(RejectReason::InsufficientResource {
                owner: owner.clone(),
                kind: ResourceKind::Data,
                requested: compound_mass_g,
                available: available_compound,
            });
        }

        let available_electricity = stock.get(ResourceKind::Electricity);
        if available_electricity < electricity_cost {
            return Err(RejectReason::InsufficientResource {
                owner: owner.clone(),
                kind: ResourceKind::Electricity,
                requested: electricity_cost,
                available: available_electricity,
            });
        }

        available_compound
            .checked_sub(compound_mass_g)
            .and_then(|remaining| remaining.checked_add(hardware_output))
            .ok_or(RejectReason::InvalidAmount {
                amount: hardware_output,
            })?;
        let first_goal_hardware = self.config.economy.factory_build_hardware_cost.max(0);
        let current_goal_progress = available_compound.saturating_sub(compound_mass_g);
        let hardware_shortfall_before = first_goal_hardware
            .saturating_sub(current_goal_progress.max(0))
            .max(0);
        let hardware_shortfall_after = hardware_shortfall_before
            .saturating_sub(hardware_output.max(0))
            .max(0);
        let electricity_after = available_electricity.saturating_sub(electricity_cost);
        let recommended_refine_amount =
            self.recommended_refine_amount(compound_mass_g, hardware_shortfall_before);
        let refine_value_class = if hardware_shortfall_before == 0 {
            "poor_power_tradeoff"
        } else if hardware_shortfall_after == 0 {
            "enough_to_advance"
        } else if hardware_output > 0 && electricity_after >= 0 {
            "partial_progress"
        } else {
            "poor_power_tradeoff"
        };
        let first_goal_relevance = if hardware_shortfall_before == 0 {
            "does_not_reduce_factory_build_hardware_shortfall"
        } else if hardware_shortfall_after == 0 {
            "enables_factory_build_hardware_goal"
        } else if hardware_shortfall_after < hardware_shortfall_before {
            "reduces_factory_build_hardware_shortfall"
        } else {
            "does_not_reduce_factory_build_hardware_shortfall"
        };

        Ok(RefineQuote {
            owner: owner.clone(),
            compound_mass_g,
            electricity_cost,
            hardware_output,
            electricity_after,
            hardware_shortfall_before,
            hardware_shortfall_after,
            first_goal_relevance: first_goal_relevance.to_string(),
            recommended_refine_amount,
            refine_value_class: refine_value_class.to_string(),
        })
    }

    fn recommended_refine_amount(&self, requested_mass_g: i64, target_hardware: i64) -> i64 {
        if requested_mass_g <= 0 {
            return 0;
        }
        if target_hardware <= 0 {
            return 0;
        }
        let yield_ppm = self.config.economy.refine_hardware_yield_ppm;
        if yield_ppm <= 0 {
            return requested_mass_g;
        }
        let recommended = target_hardware
            .saturating_mul(PPM_BASE)
            .saturating_add(yield_ppm - 1)
            .saturating_div(yield_ppm);
        recommended.clamp(1, requested_mass_g)
    }
}
