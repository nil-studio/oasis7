impl WorldKernel {
    fn fragment_error_to_reject_reason(
        &self,
        location_id: &str,
        err: FragmentResourceError,
    ) -> RejectReason {
        match err {
            FragmentResourceError::LocationNotFound { location_id } => {
                RejectReason::LocationNotFound { location_id }
            }
            FragmentResourceError::FragmentBudgetMissing { location_id } => {
                RejectReason::InsufficientResource {
                    owner: ResourceOwner::Location { location_id },
                    kind: ResourceKind::Data,
                    requested: 1,
                    available: 0,
                }
            }
            FragmentResourceError::ChunkCoordUnavailable { location_id } => {
                RejectReason::RuleDenied {
                    notes: vec![format!(
                        "chunk coord unavailable while mining at location {location_id}"
                    )],
                }
            }
            FragmentResourceError::ChunkBudgetMissing { coord } => {
                RejectReason::ChunkGenerationFailed {
                    x: coord.x,
                    y: coord.y,
                    z: coord.z,
                }
            }
            FragmentResourceError::Budget(ElementBudgetError::InvalidAmount { amount_g }) => {
                RejectReason::InvalidAmount { amount: amount_g }
            }
            FragmentResourceError::Budget(ElementBudgetError::Insufficient {
                requested_g,
                remaining_g,
                ..
            }) => RejectReason::InsufficientResource {
                owner: ResourceOwner::Location {
                    location_id: location_id.to_string(),
                },
                kind: ResourceKind::Data,
                requested: requested_g,
                available: remaining_g,
            },
        }
    }

    fn compute_refine_compound_outputs(&self, compound_mass_g: i64) -> (i64, i64) {
        let economy = &self.config.economy;
        let mass_kg = compound_mass_g.saturating_add(999).saturating_div(1000);
        let electricity_cost = mass_kg.saturating_mul(economy.refine_electricity_cost_per_kg);
        let hardware_output = compound_mass_g
            .saturating_mul(economy.refine_hardware_yield_ppm)
            .saturating_div(PPM_BASE);
        (electricity_cost, hardware_output)
    }

    fn recipe_plan(&self, recipe_id: &str) -> Option<RecipePlan> {
        let economy = &self.config.economy;
        let normalized = recipe_id.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "recipe.smelter.iron_ingot" | "recipe.iron_ingot" => Some(RecipePlan {
                required_factory_kind: FACTORY_KIND_SMELTER_MK1,
                electricity_per_batch: economy.recipe_electricity_cost_per_batch,
                hardware_per_batch: economy.recipe_hardware_cost_per_batch,
                data_output_per_batch: economy.recipe_data_output_per_batch,
                finished_product_id: "iron_ingot",
                finished_product_units_per_batch: 1,
            }),
            "recipe.smelter.copper_wire" | "recipe.copper_wire" => Some(RecipePlan {
                required_factory_kind: FACTORY_KIND_SMELTER_MK1,
                electricity_per_batch: economy.recipe_electricity_cost_per_batch,
                hardware_per_batch: economy.recipe_hardware_cost_per_batch,
                data_output_per_batch: economy.recipe_data_output_per_batch,
                finished_product_id: "copper_wire",
                finished_product_units_per_batch: 1,
            }),
            "recipe.smelter.polymer_resin" | "recipe.polymer_resin" => Some(RecipePlan {
                required_factory_kind: FACTORY_KIND_SMELTER_MK1,
                electricity_per_batch: economy.recipe_electricity_cost_per_batch,
                hardware_per_batch: economy.recipe_hardware_cost_per_batch,
                data_output_per_batch: economy.recipe_data_output_per_batch,
                finished_product_id: "polymer_resin",
                finished_product_units_per_batch: 1,
            }),
            "recipe.smelter.alloy_plate" | "recipe.alloy_plate" => Some(RecipePlan {
                required_factory_kind: FACTORY_KIND_SMELTER_MK1,
                electricity_per_batch: economy.recipe_electricity_cost_per_batch.saturating_add(3),
                hardware_per_batch: economy.recipe_hardware_cost_per_batch.saturating_mul(2),
                data_output_per_batch: economy.recipe_data_output_per_batch.saturating_mul(2),
                finished_product_id: "alloy_plate",
                finished_product_units_per_batch: 2,
            }),
            "recipe.assembler.gear" | "recipe.gear" => Some(RecipePlan {
                required_factory_kind: FACTORY_KIND_ASSEMBLER_MK1,
                electricity_per_batch: economy
                    .recipe_electricity_cost_per_batch
                    .saturating_sub(2)
                    .max(1),
                hardware_per_batch: economy.recipe_hardware_cost_per_batch,
                data_output_per_batch: economy.recipe_data_output_per_batch,
                finished_product_id: "gear",
                finished_product_units_per_batch: 1,
            }),
            "recipe.assembler.control_chip" | "recipe.control_chip" => Some(RecipePlan {
                required_factory_kind: FACTORY_KIND_ASSEMBLER_MK1,
                electricity_per_batch: economy.recipe_electricity_cost_per_batch,
                hardware_per_batch: economy.recipe_hardware_cost_per_batch,
                data_output_per_batch: economy.recipe_data_output_per_batch,
                finished_product_id: "control_chip",
                finished_product_units_per_batch: 1,
            }),
            "recipe.assembler.motor_mk1" | "recipe.motor_mk1" => Some(RecipePlan {
                required_factory_kind: FACTORY_KIND_ASSEMBLER_MK1,
                electricity_per_batch: economy.recipe_electricity_cost_per_batch.saturating_mul(2),
                hardware_per_batch: economy.recipe_hardware_cost_per_batch.saturating_mul(2),
                data_output_per_batch: economy.recipe_data_output_per_batch.saturating_mul(2),
                finished_product_id: "motor_mk1",
                finished_product_units_per_batch: 1,
            }),
            "recipe.assembler.logistics_drone" | "recipe.logistics_drone" => Some(RecipePlan {
                required_factory_kind: FACTORY_KIND_ASSEMBLER_MK1,
                electricity_per_batch: economy.recipe_electricity_cost_per_batch.saturating_mul(4),
                hardware_per_batch: economy.recipe_hardware_cost_per_batch.saturating_mul(4),
                data_output_per_batch: economy.recipe_data_output_per_batch.saturating_mul(4),
                finished_product_id: "logistics_drone",
                finished_product_units_per_batch: 1,
            }),
            "recipe.assembler.sensor_pack" | "recipe.sensor_pack" => Some(RecipePlan {
                required_factory_kind: FACTORY_KIND_ASSEMBLER_MK1,
                electricity_per_batch: economy.recipe_electricity_cost_per_batch.saturating_add(2),
                hardware_per_batch: economy.recipe_hardware_cost_per_batch.saturating_mul(2),
                data_output_per_batch: economy.recipe_data_output_per_batch.saturating_mul(2),
                finished_product_id: "sensor_pack",
                finished_product_units_per_batch: 1,
            }),
            "recipe.assembler.module_rack" | "recipe.module_rack" => Some(RecipePlan {
                required_factory_kind: FACTORY_KIND_ASSEMBLER_MK1,
                electricity_per_batch: economy.recipe_electricity_cost_per_batch.saturating_add(4),
                hardware_per_batch: economy.recipe_hardware_cost_per_batch.saturating_mul(3),
                data_output_per_batch: economy.recipe_data_output_per_batch.saturating_mul(3),
                finished_product_id: "module_rack",
                finished_product_units_per_batch: 1,
            }),
            "recipe.assembler.factory_core" | "recipe.factory_core" => Some(RecipePlan {
                required_factory_kind: FACTORY_KIND_ASSEMBLER_MK1,
                electricity_per_batch: economy.recipe_electricity_cost_per_batch.saturating_add(8),
                hardware_per_batch: economy.recipe_hardware_cost_per_batch.saturating_mul(4),
                data_output_per_batch: economy.recipe_data_output_per_batch.saturating_mul(4),
                finished_product_id: "factory_core",
                finished_product_units_per_batch: 1,
            }),
            _ => None,
        }
    }

    fn owner_pos(
        &self,
        owner: &ResourceOwner,
    ) -> Result<Option<crate::geometry::GeoPos>, RejectReason> {
        match owner {
            ResourceOwner::Agent { agent_id } => self
                .model
                .agents
                .get(agent_id)
                .map(|agent| Some(agent.pos))
                .ok_or_else(|| RejectReason::AgentNotFound {
                    agent_id: agent_id.clone(),
                }),
            ResourceOwner::Location { location_id } => self
                .model
                .locations
                .get(location_id)
                .map(|location| Some(location.pos))
                .ok_or_else(|| RejectReason::LocationNotFound {
                    location_id: location_id.clone(),
                }),
        }
    }

    pub(super) fn owner_stock(
        &self,
        owner: &ResourceOwner,
    ) -> Option<&super::super::types::ResourceStock> {
        match owner {
            ResourceOwner::Agent { agent_id } => {
                self.model.agents.get(agent_id).map(|a| &a.resources)
            }
            ResourceOwner::Location { location_id } => {
                self.model.locations.get(location_id).map(|l| &l.resources)
            }
        }
    }

    pub(super) fn remove_from_owner(
        &mut self,
        owner: &ResourceOwner,
        kind: ResourceKind,
        amount: i64,
    ) -> Result<(), RejectReason> {
        if matches!(owner, ResourceOwner::Location { .. })
            && matches!(kind, ResourceKind::Electricity)
        {
            return Err(RejectReason::RuleDenied {
                notes: vec![LOCATION_ELECTRICITY_POOL_REMOVED_NOTE.to_string()],
            });
        }
        let stock = match owner {
            ResourceOwner::Agent { agent_id } => self
                .model
                .agents
                .get_mut(agent_id)
                .map(|agent| &mut agent.resources)
                .ok_or_else(|| RejectReason::AgentNotFound {
                    agent_id: agent_id.clone(),
                })?,
            ResourceOwner::Location { location_id } => self
                .model
                .locations
                .get_mut(location_id)
                .map(|location| &mut location.resources)
                .ok_or_else(|| RejectReason::LocationNotFound {
                    location_id: location_id.clone(),
                })?,
        };

        stock.remove(kind, amount).map_err(|err| match err {
            StockError::NegativeAmount { amount } => RejectReason::InvalidAmount { amount },
            StockError::Insufficient {
                requested,
                available,
                ..
            } => RejectReason::InsufficientResource {
                owner: owner.clone(),
                kind,
                requested,
                available,
            },
            StockError::Overflow { delta, .. } => RejectReason::InvalidAmount { amount: delta },
        })
    }

    pub(super) fn add_to_owner(
        &mut self,
        owner: &ResourceOwner,
        kind: ResourceKind,
        amount: i64,
    ) -> Result<(), RejectReason> {
        if matches!(owner, ResourceOwner::Location { .. })
            && matches!(kind, ResourceKind::Electricity)
        {
            return Err(RejectReason::RuleDenied {
                notes: vec![LOCATION_ELECTRICITY_POOL_REMOVED_NOTE.to_string()],
            });
        }
        let stock = match owner {
            ResourceOwner::Agent { agent_id } => self
                .model
                .agents
                .get_mut(agent_id)
                .map(|agent| &mut agent.resources)
                .ok_or_else(|| RejectReason::AgentNotFound {
                    agent_id: agent_id.clone(),
                })?,
            ResourceOwner::Location { location_id } => self
                .model
                .locations
                .get_mut(location_id)
                .map(|location| &mut location.resources)
                .ok_or_else(|| RejectReason::LocationNotFound {
                    location_id: location_id.clone(),
                })?,
        };

        stock.add(kind, amount).map_err(|err| match err {
            StockError::NegativeAmount { amount } => RejectReason::InvalidAmount { amount },
            StockError::Insufficient { .. } => RejectReason::InvalidAmount { amount },
            StockError::Overflow { delta, .. } => RejectReason::InvalidAmount { amount: delta },
        })
    }
}
