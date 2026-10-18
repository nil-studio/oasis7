impl WorldKernel {
    pub(super) fn apply_action(
        &mut self,
        action_id: super::super::types::ActionId,
        action: Action,
    ) -> WorldEventKind {
        match action {
            Action::RegisterLocation {
                location_id,
                name,
                pos,
                profile,
            } => {
                if self.model.locations.contains_key(&location_id) {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::LocationAlreadyExists { location_id },
                    };
                }
                if !self.config.space.contains(pos) {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::PositionOutOfBounds { pos },
                    };
                }
                if profile.radius_cm < 0 {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::InvalidAmount {
                            amount: profile.radius_cm,
                        },
                    };
                }
                if profile.radiation_emission_per_tick < 0 {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::InvalidAmount {
                            amount: profile.radiation_emission_per_tick,
                        },
                    };
                }
                let location = Location::new_with_profile(
                    location_id.clone(),
                    name.clone(),
                    pos,
                    profile.clone(),
                );
                self.model.locations.insert(location_id.clone(), location);
                WorldEventKind::LocationRegistered {
                    location_id,
                    name,
                    pos,
                    profile,
                }
            }
            Action::RegisterAgent {
                agent_id,
                location_id,
            } => {
                if self.model.agents.contains_key(&agent_id) {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::AgentAlreadyExists { agent_id },
                    };
                }
                let Some(location) = self.model.locations.get(&location_id) else {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::LocationNotFound { location_id },
                    };
                };
                let agent = Agent::new_with_power(
                    agent_id.clone(),
                    location_id.clone(),
                    location.pos,
                    &self.config.power,
                );
                self.model.agents.insert(agent_id.clone(), agent);
                WorldEventKind::AgentRegistered {
                    agent_id,
                    location_id,
                    pos: location.pos,
                }
            }
            Action::RegisterPowerPlant {
                facility_id,
                location_id,
                owner,
                capacity_per_tick,
                fuel_cost_per_pu,
                maintenance_cost,
                efficiency,
                degradation,
            } => {
                if self.model.power_plants.contains_key(&facility_id) {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::FacilityAlreadyExists { facility_id },
                    };
                }
                if !self.model.locations.contains_key(&location_id) {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::LocationNotFound { location_id },
                    };
                }
                if let Err(reason) = self.ensure_owner_exists(&owner) {
                    return WorldEventKind::ActionRejected { reason };
                }
                if capacity_per_tick < 0 {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::InvalidAmount {
                            amount: capacity_per_tick,
                        },
                    };
                }
                if fuel_cost_per_pu < 0 {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::InvalidAmount {
                            amount: fuel_cost_per_pu,
                        },
                    };
                }
                if maintenance_cost < 0 {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::InvalidAmount {
                            amount: maintenance_cost,
                        },
                    };
                }
                let plant = PowerPlant {
                    id: facility_id.clone(),
                    location_id,
                    owner,
                    capacity_per_tick,
                    current_output: 0,
                    fuel_cost_per_pu,
                    maintenance_cost,
                    status: PlantStatus::Running,
                    efficiency,
                    degradation,
                };
                self.model.power_plants.insert(facility_id, plant.clone());
                WorldEventKind::Power(PowerEvent::PowerPlantRegistered { plant })
            }
            Action::UpsertModuleVisualEntity { entity } => {
                let entity = entity.sanitized();
                if entity.entity_id.is_empty() || entity.module_id.is_empty() {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::InvalidAmount { amount: 0 },
                    };
                }
                if let Err(reason) = self.ensure_module_visual_anchor_exists(&entity.anchor) {
                    return WorldEventKind::ActionRejected { reason };
                }
                self.model
                    .module_visual_entities
                    .insert(entity.entity_id.clone(), entity.clone());
                WorldEventKind::ModuleVisualEntityUpserted { entity }
            }
            Action::RemoveModuleVisualEntity { entity_id } => {
                let entity_id = entity_id.trim().to_string();
                if entity_id.is_empty() {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::InvalidAmount { amount: 0 },
                    };
                }
                if self
                    .model
                    .module_visual_entities
                    .remove(&entity_id)
                    .is_none()
                {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::FacilityNotFound {
                            facility_id: entity_id,
                        },
                    };
                }
                WorldEventKind::ModuleVisualEntityRemoved { entity_id }
            }
            Action::MoveAgent { agent_id, to } => {
                let to_pos = match self.model.locations.get(&to) {
                    Some(location) => location.pos,
                    None => {
                        return WorldEventKind::ActionRejected {
                            reason: RejectReason::LocationNotFound { location_id: to },
                        };
                    }
                };
                if let Err(reason) =
                    self.ensure_chunk_generated_at(to_pos, ChunkGenerationCause::Action)
                {
                    return WorldEventKind::ActionRejected { reason };
                }
                let (from, distance_cm, electricity_cost, should_continue) = {
                    let Some(agent) = self.model.agents.get_mut(&agent_id) else {
                        return WorldEventKind::ActionRejected {
                            reason: RejectReason::AgentNotFound { agent_id },
                        };
                    };
                    if agent.power.is_shutdown() {
                        return WorldEventKind::ActionRejected {
                            reason: RejectReason::AgentShutdown { agent_id },
                        };
                    }
                    if agent.kinematics.speed_cm_per_tick <= 0 {
                        return WorldEventKind::ActionRejected {
                            reason: RejectReason::InvalidAmount {
                                amount: agent.kinematics.speed_cm_per_tick,
                            },
                        };
                    }

                    let continuing_move = agent.kinematics.move_target_location_id.is_some();
                    if let Some(active_target) = agent.kinematics.move_target_location_id.as_ref() {
                        if active_target != &to {
                            return WorldEventKind::ActionRejected {
                                reason: RejectReason::RuleDenied {
                                    notes: vec![format!(
                                        "agent {agent_id} is already moving to {active_target}"
                                    )],
                                },
                            };
                        }
                    } else if agent.location_id == to {
                        return WorldEventKind::ActionRejected {
                            reason: RejectReason::AgentAlreadyAtLocation {
                                agent_id,
                                location_id: to,
                            },
                        };
                    }

                    let from = agent.location_id.clone();
                    let mut electricity_cost = 0_i64;
                    if !continuing_move {
                        let distance_cm = space_distance_cm(agent.pos, to_pos);
                        let physics = &self.config.physics;
                        let max_move_distance_cm = physics.max_move_distance_cm_per_tick;
                        if max_move_distance_cm > 0 && distance_cm > max_move_distance_cm {
                            return WorldEventKind::ActionRejected {
                                reason: RejectReason::MoveDistanceExceeded {
                                    distance_cm,
                                    max_distance_cm: max_move_distance_cm,
                                },
                            };
                        }
                        let max_move_speed_cm_per_s = physics.max_move_speed_cm_per_s;
                        if max_move_speed_cm_per_s > 0 {
                            let time_step_s = physics.time_step_s.max(1);
                            let required_speed_cm_per_s =
                                (distance_cm + time_step_s - 1).saturating_div(time_step_s);
                            if required_speed_cm_per_s > max_move_speed_cm_per_s {
                                return WorldEventKind::ActionRejected {
                                    reason: RejectReason::MoveSpeedExceeded {
                                        required_speed_cm_per_s,
                                        max_speed_cm_per_s: max_move_speed_cm_per_s,
                                        time_step_s,
                                    },
                                };
                            }
                        }
                        electricity_cost = self.config.movement_cost(distance_cm);
                        if electricity_cost > 0 {
                            let available = agent.resources.get(ResourceKind::Electricity);
                            if available < electricity_cost {
                                return WorldEventKind::ActionRejected {
                                    reason: RejectReason::InsufficientResource {
                                        owner: ResourceOwner::Agent {
                                            agent_id: agent.id.clone(),
                                        },
                                        kind: ResourceKind::Electricity,
                                        requested: electricity_cost,
                                        available,
                                    },
                                };
                            }
                            if let Err(err) = agent
                                .resources
                                .remove(ResourceKind::Electricity, electricity_cost)
                            {
                                return WorldEventKind::ActionRejected {
                                    reason: match err {
                                        StockError::NegativeAmount { amount } => {
                                            RejectReason::InvalidAmount { amount }
                                        }
                                        StockError::Insufficient {
                                            requested,
                                            available,
                                            ..
                                        } => RejectReason::InsufficientResource {
                                            owner: ResourceOwner::Agent {
                                                agent_id: agent.id.clone(),
                                            },
                                            kind: ResourceKind::Electricity,
                                            requested,
                                            available,
                                        },
                                        StockError::Overflow { delta, .. } => {
                                            RejectReason::InvalidAmount { amount: delta }
                                        }
                                    },
                                };
                            }
                        }

                        let speed_per_tick = agent.kinematics.speed_cm_per_tick.max(1);
                        let eta_ticks = distance_cm
                            .saturating_add(speed_per_tick.saturating_sub(1))
                            .saturating_div(speed_per_tick)
                            .max(1) as u64;
                        agent.kinematics.move_target_location_id = Some(to.clone());
                        agent.kinematics.move_target = Some(to_pos);
                        agent.kinematics.move_started_at_tick = Some(self.time);
                        agent.kinematics.move_eta_tick = Some(self.time.saturating_add(eta_ticks));
                        agent.kinematics.move_remaining_cm = distance_cm;
                    }

                    let target_pos = agent.kinematics.move_target.unwrap_or(to_pos);
                    let remaining_cm = space_distance_cm(agent.pos, target_pos).max(0);
                    let step_distance_cm =
                        remaining_cm.min(agent.kinematics.speed_cm_per_tick.max(1));
                    if step_distance_cm <= 0 {
                        agent.pos = target_pos;
                        agent.location_id = to.clone();
                        agent.kinematics.clear_motion_state();
                        (from, 0, electricity_cost, false)
                    } else {
                        let arrived = step_distance_cm >= remaining_cm;
                        agent.pos = if arrived {
                            target_pos
                        } else {
                            move_towards_geo_pos(agent.pos, target_pos, step_distance_cm)
                        };
                        if arrived {
                            agent.location_id = to.clone();
                            agent.kinematics.clear_motion_state();
                        } else {
                            agent.kinematics.move_remaining_cm = remaining_cm - step_distance_cm;
                        }
                        (from, step_distance_cm, electricity_cost, !arrived)
                    }
                };
                if should_continue {
                    self.submit_action_from_system(Action::MoveAgent {
                        agent_id: agent_id.clone(),
                        to: to.clone(),
                    });
                }
                WorldEventKind::AgentMoved {
                    agent_id,
                    from,
                    to,
                    distance_cm,
                    electricity_cost,
                }
            }
            Action::SpeakToNearby {
                agent_id,
                message,
                target_agent_id,
            } => {
                let message = message.trim().to_string();
                if message.is_empty() {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::RuleDenied {
                            notes: vec!["speak_to_nearby requires non-empty message".to_string()],
                        },
                    };
                }
                let location_id = {
                    let Some(agent) = self.model.agents.get(&agent_id) else {
                        return WorldEventKind::ActionRejected {
                            reason: RejectReason::AgentNotFound { agent_id },
                        };
                    };
                    if agent.power.is_shutdown() {
                        return WorldEventKind::ActionRejected {
                            reason: RejectReason::AgentShutdown {
                                agent_id: agent.id.clone(),
                            },
                        };
                    }
                    agent.location_id.clone()
                };
                if let Some(target_agent_id) = target_agent_id.as_ref() {
                    let Some(target_agent) = self.model.agents.get(target_agent_id) else {
                        return WorldEventKind::ActionRejected {
                            reason: RejectReason::AgentNotFound {
                                agent_id: target_agent_id.clone(),
                            },
                        };
                    };
                    if target_agent.location_id != location_id {
                        return WorldEventKind::ActionRejected {
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "target agent {} is not co-located with speaker {}",
                                    target_agent_id, agent_id
                                )],
                            },
                        };
                    }
                }
                WorldEventKind::AgentSpoke {
                    agent_id,
                    location_id,
                    message,
                    target_agent_id,
                }
            }
            Action::InspectTarget {
                agent_id,
                target_kind,
                target_id,
            } => {
                let target_kind = target_kind.trim().to_string();
                let target_id = target_id.trim().to_string();
                if target_kind.is_empty() || target_id.is_empty() {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "inspect_target requires non-empty target_kind and target_id"
                                    .to_string(),
                            ],
                        },
                    };
                }
                let Some(agent) = self.model.agents.get(&agent_id) else {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::AgentNotFound { agent_id },
                    };
                };
                if agent.power.is_shutdown() {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::AgentShutdown {
                            agent_id: agent.id.clone(),
                        },
                    };
                }
                let target_exists = match target_kind.as_str() {
                    "agent" => self.model.agents.contains_key(&target_id),
                    "location" => self.model.locations.contains_key(&target_id),
                    "factory" => self.model.factories.contains_key(&target_id),
                    "power_plant" => self.model.power_plants.contains_key(&target_id),
                    "module_visual_entity" => {
                        self.model.module_visual_entities.contains_key(&target_id)
                    }
                    _ => {
                        return WorldEventKind::ActionRejected {
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "inspect_target does not support target_kind={target_kind}"
                                )],
                            },
                        };
                    }
                };
                if !target_exists {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "inspect_target target not found: kind={target_kind} id={target_id}"
                            )],
                        },
                    };
                }
                WorldEventKind::TargetInspected {
                    agent_id,
                    target_kind,
                    target_id,
                }
            }
            Action::SimpleInteract {
                agent_id,
                target_kind,
                target_id,
                interaction,
            } => {
                let target_kind = target_kind.trim().to_string();
                let target_id = target_id.trim().to_string();
                let interaction = interaction.trim().to_string();
                if target_kind.is_empty() || target_id.is_empty() || interaction.is_empty() {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::RuleDenied {
                            notes: vec![
                                "simple_interact requires non-empty target_kind, target_id, and interaction"
                                    .to_string(),
                            ],
                        },
                    };
                }
                let Some(agent) = self.model.agents.get(&agent_id) else {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::AgentNotFound { agent_id },
                    };
                };
                if agent.power.is_shutdown() {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::AgentShutdown {
                            agent_id: agent.id.clone(),
                        },
                    };
                }
                let target_exists = match target_kind.as_str() {
                    "agent" => self.model.agents.contains_key(&target_id),
                    "location" => self.model.locations.contains_key(&target_id),
                    "factory" => self.model.factories.contains_key(&target_id),
                    "power_plant" => self.model.power_plants.contains_key(&target_id),
                    "module_visual_entity" => {
                        self.model.module_visual_entities.contains_key(&target_id)
                    }
                    _ => {
                        return WorldEventKind::ActionRejected {
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "simple_interact does not support target_kind={target_kind}"
                                )],
                            },
                        };
                    }
                };
                if !target_exists {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "simple_interact target not found: kind={target_kind} id={target_id}"
                            )],
                        },
                    };
                }
                WorldEventKind::SimpleInteractionPerformed {
                    agent_id,
                    target_kind,
                    target_id,
                    interaction,
                }
            }
            Action::HarvestRadiation {
                agent_id,
                max_amount,
            } => {
                if max_amount <= 0 {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::InvalidAmount { amount: max_amount },
                    };
                }
                let location_id = match self.model.agents.get(&agent_id) {
                    Some(agent) => agent.location_id.clone(),
                    None => {
                        return WorldEventKind::ActionRejected {
                            reason: RejectReason::AgentNotFound { agent_id },
                        };
                    }
                };
                let location_pos = match self.model.locations.get(&location_id) {
                    Some(location) => location.pos,
                    None => {
                        return WorldEventKind::ActionRejected {
                            reason: RejectReason::LocationNotFound { location_id },
                        };
                    }
                };
                if let Err(reason) =
                    self.ensure_chunk_generated_at(location_pos, ChunkGenerationCause::Action)
                {
                    return WorldEventKind::ActionRejected { reason };
                }
                let harvest_pos = match self.model.locations.get(&location_id) {
                    Some(location) => location.pos,
                    None => {
                        return WorldEventKind::ActionRejected {
                            reason: RejectReason::LocationNotFound { location_id },
                        };
                    }
                };
                let local_available = self.radiation_available_at(harvest_pos);
                if local_available <= 0 {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::RadiationUnavailable { location_id },
                    };
                }
                let physics = &self.config.physics;
                let mut available_for_harvest = local_available;
                if physics.max_harvest_per_tick > 0 {
                    available_for_harvest = available_for_harvest.min(physics.max_harvest_per_tick);
                }
                let mut harvested = max_amount.min(available_for_harvest);
                let Some(agent) = self.model.agents.get_mut(&agent_id) else {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::AgentNotFound { agent_id },
                    };
                };
                if physics.thermal_capacity > 0 && agent.thermal.heat > physics.thermal_capacity {
                    let heat = agent.thermal.heat;
                    let capacity = physics.thermal_capacity;
                    let ratio = (capacity as f64 / heat as f64).clamp(0.1, 1.0);
                    harvested = (harvested as f64 * ratio).floor() as i64;
                    if harvested <= 0 {
                        return WorldEventKind::ActionRejected {
                            reason: RejectReason::ThermalOverload { heat, capacity },
                        };
                    }
                }
                if harvested > 0 {
                    if let Err(reason) = agent.resources.add(ResourceKind::Electricity, harvested) {
                        return WorldEventKind::ActionRejected {
                            reason: match reason {
                                StockError::NegativeAmount { amount } => {
                                    RejectReason::InvalidAmount { amount }
                                }
                                StockError::Insufficient { .. } => {
                                    RejectReason::InvalidAmount { amount: harvested }
                                }
                                StockError::Overflow { delta, .. } => {
                                    RejectReason::InvalidAmount { amount: delta }
                                }
                            },
                        };
                    }
                    if physics.heat_factor > 0 {
                        agent.thermal.heat = agent
                            .thermal
                            .heat
                            .saturating_add(harvested * physics.heat_factor);
                    }
                }
                WorldEventKind::RadiationHarvested {
                    agent_id,
                    location_id,
                    amount: harvested,
                    available: local_available,
                }
            }
            Action::BuyPower {
                buyer,
                seller,
                amount,
                price_per_pu,
            } => match self.transfer_power(&seller, &buyer, amount, price_per_pu) {
                Ok(power_event) => WorldEventKind::Power(power_event),
                Err(reason) => WorldEventKind::ActionRejected { reason },
            },
            Action::SellPower {
                seller,
                buyer,
                amount,
                price_per_pu,
            } => match self.transfer_power(&seller, &buyer, amount, price_per_pu) {
                Ok(power_event) => WorldEventKind::Power(power_event),
                Err(reason) => WorldEventKind::ActionRejected { reason },
            },
            Action::PlacePowerOrder {
                owner,
                side,
                amount,
                limit_price_per_pu,
            } => self.place_power_order(owner, side, amount, limit_price_per_pu),
            Action::CancelPowerOrder { owner, order_id } => {
                self.cancel_power_order(owner, order_id)
            }
            Action::TransferResource {
                from,
                to,
                kind,
                amount,
            } => {
                if let Err(reason) = self.ensure_owner_chunks_generated(&from, &to) {
                    return WorldEventKind::ActionRejected { reason };
                }
                match self.validate_transfer(&from, &to, kind, amount) {
                    Ok(()) => {
                        if let Err(reason) = self.apply_transfer(&from, &to, kind, amount) {
                            WorldEventKind::ActionRejected { reason }
                        } else {
                            WorldEventKind::ResourceTransferred {
                                from,
                                to,
                                kind,
                                amount,
                            }
                        }
                    }
                    Err(reason) => WorldEventKind::ActionRejected { reason },
                }
            }
            Action::DebugGrantResource {
                owner,
                kind,
                amount,
            } => {
                if amount <= 0 {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::InvalidAmount { amount },
                    };
                }
                if let Err(reason) = self.ensure_owner_exists(&owner) {
                    return WorldEventKind::ActionRejected { reason };
                }
                if let Err(reason) = self.add_to_owner(&owner, kind, amount) {
                    return WorldEventKind::ActionRejected { reason };
                }
                WorldEventKind::DebugResourceGranted {
                    owner,
                    kind,
                    amount,
                }
            }
            Action::MineCompound {
                owner,
                location_id,
                compound_mass_g,
            } => {
                if compound_mass_g <= 0 {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::InvalidAmount {
                            amount: compound_mass_g,
                        },
                    };
                }
                if let Err(reason) = self.ensure_owner_exists(&owner) {
                    return WorldEventKind::ActionRejected { reason };
                }
                if !self.model.locations.contains_key(&location_id) {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::LocationNotFound { location_id },
                    };
                }
                let site_owner = ResourceOwner::Location {
                    location_id: location_id.clone(),
                };
                if let Err(reason) = self.ensure_colocated(&owner, &site_owner) {
                    return WorldEventKind::ActionRejected { reason };
                }
                if let Err(reason) = self.ensure_owner_chunks_generated(&owner, &site_owner) {
                    return WorldEventKind::ActionRejected { reason };
                }

                let max_per_action = self.config.economy.mine_compound_max_per_action_g;
                if max_per_action > 0 && compound_mass_g > max_per_action {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::InvalidAmount {
                            amount: compound_mass_g,
                        },
                    };
                }

                let mined_so_far = self
                    .model
                    .locations
                    .get(&location_id)
                    .map(|location| location.mined_compound_g.max(0))
                    .unwrap_or(0);
                let max_per_location = self.config.economy.mine_compound_max_per_location_g;
                if max_per_location > 0 {
                    let available = max_per_location.saturating_sub(mined_so_far).max(0);
                    if compound_mass_g > available {
                        return WorldEventKind::ActionRejected {
                            reason: RejectReason::InsufficientResource {
                                owner: site_owner.clone(),
                                kind: ResourceKind::Data,
                                requested: compound_mass_g,
                                available,
                            },
                        };
                    }
                }

                let extraction_plan =
                    match self.plan_compound_extraction(&location_id, compound_mass_g) {
                        Ok(plan) => plan,
                        Err(reason) => return WorldEventKind::ActionRejected { reason },
                    };
                let electricity_cost = self.compute_mine_compound_electricity_cost(compound_mass_g);
                let available_electricity = self
                    .owner_stock(&owner)
                    .map(|stock| stock.get(ResourceKind::Electricity))
                    .unwrap_or(0);
                if available_electricity < electricity_cost {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::InsufficientResource {
                            owner: owner.clone(),
                            kind: ResourceKind::Electricity,
                            requested: electricity_cost,
                            available: available_electricity,
                        },
                    };
                }

                for (element, amount_g) in &extraction_plan {
                    if let Err(reason) =
                        self.consume_fragment_resource_for_action(&location_id, *element, *amount_g)
                    {
                        return WorldEventKind::ActionRejected { reason };
                    }
                }
                if let Some(location) = self.model.locations.get_mut(&location_id) {
                    location.mined_compound_g = mined_so_far.saturating_add(compound_mass_g);
                }

                if let Err(reason) =
                    self.remove_from_owner(&owner, ResourceKind::Electricity, electricity_cost)
                {
                    return WorldEventKind::ActionRejected { reason };
                }
                if let Err(reason) = self.add_to_owner(&owner, ResourceKind::Data, compound_mass_g)
                {
                    return WorldEventKind::ActionRejected { reason };
                }

                WorldEventKind::CompoundMined {
                    owner,
                    location_id,
                    compound_mass_g,
                    electricity_cost,
                    extracted_elements: extraction_plan.into_iter().collect::<BTreeMap<_, _>>(),
                }
            }
            Action::RefineCompound {
                owner,
                compound_mass_g,
            } => {
                if compound_mass_g <= 0 {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::InvalidAmount {
                            amount: compound_mass_g,
                        },
                    };
                }
                if let Err(reason) = self.ensure_owner_chunk_generated(&owner) {
                    return WorldEventKind::ActionRejected { reason };
                }

                let (electricity_cost, hardware_output) =
                    self.compute_refine_compound_outputs(compound_mass_g);
                if hardware_output <= 0 {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::InvalidAmount {
                            amount: compound_mass_g,
                        },
                    };
                }

                let available_compound = self
                    .owner_stock(&owner)
                    .map(|stock| stock.get(ResourceKind::Data))
                    .unwrap_or(0);
                if available_compound < compound_mass_g {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::InsufficientResource {
                            owner: owner.clone(),
                            kind: ResourceKind::Data,
                            requested: compound_mass_g,
                            available: available_compound,
                        },
                    };
                }

                let available = self
                    .owner_stock(&owner)
                    .map(|stock| stock.get(ResourceKind::Electricity))
                    .unwrap_or(0);
                if available < electricity_cost {
                    return WorldEventKind::ActionRejected {
                        reason: RejectReason::InsufficientResource {
                            owner: owner.clone(),
                            kind: ResourceKind::Electricity,
                            requested: electricity_cost,
                            available,
                        },
                    };
                }

                if let Err(reason) =
                    self.remove_from_owner(&owner, ResourceKind::Data, compound_mass_g)
                {
                    return WorldEventKind::ActionRejected { reason };
                }
                if let Err(reason) =
                    self.remove_from_owner(&owner, ResourceKind::Electricity, electricity_cost)
                {
                    return WorldEventKind::ActionRejected { reason };
                }
                if let Err(reason) = self.add_to_owner(&owner, ResourceKind::Data, hardware_output)
                {
                    return WorldEventKind::ActionRejected { reason };
                }

                WorldEventKind::CompoundRefined {
                    owner,
                    compound_mass_g,
                    electricity_cost,
                    hardware_output,
                }
            }
            Action::BuildFactory {
                owner,
                location_id,
                factory_id,
                factory_kind,
            } => self.apply_build_factory(owner, location_id, factory_id, factory_kind),
            Action::ScheduleRecipe {
                owner,
                factory_id,
                recipe_id,
                batches,
            } => self.apply_schedule_recipe(owner, factory_id, recipe_id, batches),
            Action::CompileModuleArtifactFromSource {
                publisher_agent_id,
                module_id,
                manifest_path,
                source_files,
            } => self.apply_compile_module_artifact_from_source(
                publisher_agent_id,
                module_id,
                manifest_path,
                source_files,
            ),
            Action::DeployModuleArtifact {
                publisher_agent_id,
                wasm_hash,
                wasm_bytes,
                module_id_hint,
            } => self.apply_deploy_module_artifact(
                publisher_agent_id,
                wasm_hash,
                wasm_bytes,
                module_id_hint,
            ),
            Action::InstallModuleFromArtifact {
                installer_agent_id,
                module_id,
                module_version,
                wasm_hash,
                activate,
            } => self.apply_install_module_from_artifact(
                installer_agent_id,
                module_id,
                module_version,
                wasm_hash,
                activate,
            ),
            Action::InstallModuleToTargetFromArtifact {
                installer_agent_id,
                module_id,
                module_version,
                wasm_hash,
                activate,
                install_target,
            } => self.apply_install_module_to_target_from_artifact(
                installer_agent_id,
                module_id,
                module_version,
                wasm_hash,
                activate,
                install_target,
            ),
            action @ (Action::InstallMicroDepot { .. }
            | Action::ServiceMicroDepotRepair { .. }
            | Action::ServiceMicroDepotLogistics { .. }
            | Action::PayMicroDepotUpkeep { .. }
            | Action::SuspendMicroDepot { .. }
            | Action::ReclaimMicroDepot { .. }) => self.apply_micro_depot_action(action_id, action),
            Action::ListModuleArtifactForSale {
                seller_agent_id,
                wasm_hash,
                price_kind,
                price_amount,
            } => self.apply_list_module_artifact_for_sale(
                seller_agent_id,
                wasm_hash,
                price_kind,
                price_amount,
            ),
            Action::BuyModuleArtifact {
                buyer_agent_id,
                wasm_hash,
            } => self.apply_buy_module_artifact(buyer_agent_id, wasm_hash),
            Action::DelistModuleArtifact {
                seller_agent_id,
                wasm_hash,
            } => self.apply_delist_module_artifact(seller_agent_id, wasm_hash),
            Action::DestroyModuleArtifact {
                owner_agent_id,
                wasm_hash,
                reason,
            } => self.apply_destroy_module_artifact(owner_agent_id, wasm_hash, reason),
            Action::PlaceModuleArtifactBid {
                bidder_agent_id,
                wasm_hash,
                price_kind,
                price_amount,
            } => self.apply_place_module_artifact_bid(
                bidder_agent_id,
                wasm_hash,
                price_kind,
                price_amount,
            ),
            Action::CancelModuleArtifactBid {
                bidder_agent_id,
                wasm_hash,
                bid_order_id,
            } => self.apply_cancel_module_artifact_bid(bidder_agent_id, wasm_hash, bid_order_id),
            Action::PublishSocialFact {
                actor,
                schema_id,
                subject,
                object,
                claim,
                confidence_ppm,
                evidence_event_ids,
                ttl_ticks,
                stake,
            } => self.apply_publish_social_fact(
                actor,
                schema_id,
                subject,
                object,
                claim,
                confidence_ppm,
                evidence_event_ids,
                ttl_ticks,
                stake,
            ),
            Action::ChallengeSocialFact {
                challenger,
                fact_id,
                reason,
                stake,
            } => self.apply_challenge_social_fact(challenger, fact_id, reason, stake),
            Action::AdjudicateSocialFact {
                adjudicator,
                fact_id,
                decision,
                notes,
            } => self.apply_adjudicate_social_fact(adjudicator, fact_id, decision, notes),
            Action::RevokeSocialFact {
                actor,
                fact_id,
                reason,
            } => self.apply_revoke_social_fact(actor, fact_id, reason),
            Action::DeclareSocialEdge {
                declarer,
                schema_id,
                relation_kind,
                from,
                to,
                weight_bps,
                backing_fact_ids,
                ttl_ticks,
            } => self.apply_declare_social_edge(
                declarer,
                schema_id,
                relation_kind,
                from,
                to,
                weight_bps,
                backing_fact_ids,
                ttl_ticks,
            ),
            Action::FormAlliance { .. }
            | Action::JoinAlliance { .. }
            | Action::LeaveAlliance { .. }
            | Action::DissolveAlliance { .. }
            | Action::DeclareWar { .. }
            | Action::OpenGovernanceProposal { .. }
            | Action::CastGovernanceVote { .. }
            | Action::ResolveCrisis { .. }
            | Action::GrantMetaProgress { .. }
            | Action::OpenEconomicContract { .. }
            | Action::AcceptEconomicContract { .. }
            | Action::SettleEconomicContract { .. } => WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec!["gameplay action is runtime-only in simulator kernel".to_string()],
                },
            },
        }
    }
}

fn move_towards_geo_pos(from: GeoPos, to: GeoPos, step_cm: i64) -> GeoPos {
    if step_cm <= 0 {
        return from;
    }
    let dx = to.x_cm - from.x_cm;
    let dy = to.y_cm - from.y_cm;
    let dz = to.z_cm - from.z_cm;
    let distance_sq = squared_distance_cm(dx, dy, dz);
    if distance_sq == 0 {
        return to;
    }
    let step_sq = squared_distance_cm(step_cm, 0, 0);
    if distance_sq <= step_sq {
        return to;
    }
    let distance = (distance_sq as f64).sqrt();
    let ratio = (step_cm as f64 / distance).clamp(0.0, 1.0);
    let scaled = [
        (dx as f64) * ratio,
        (dy as f64) * ratio,
        (dz as f64) * ratio,
    ];
    let original = [dx, dy, dz];
    let mut delta = [
        scaled[0].trunc() as i64,
        scaled[1].trunc() as i64,
        scaled[2].trunc() as i64,
    ];

    let mut order = [0usize, 1, 2];
    order.sort_by(|left, right| {
        let left_residual = scaled[*left].abs() - delta[*left].abs() as f64;
        let right_residual = scaled[*right].abs() - delta[*right].abs() as f64;
        right_residual
            .total_cmp(&left_residual)
            .then_with(|| left.cmp(right))
    });

    for axis in order {
        let direction = original[axis].signum();
        if direction == 0 {
            continue;
        }
        let mut candidate = delta;
        candidate[axis] += direction;
        if candidate[axis].abs() > original[axis].abs() {
            continue;
        }
        if squared_distance_cm(candidate[0], candidate[1], candidate[2]) <= step_sq {
            delta = candidate;
        }
    }

    if delta == [0, 0, 0] {
        let mut dominant_axis = 0usize;
        for axis in 1..3 {
            if original[axis].abs() > original[dominant_axis].abs() {
                dominant_axis = axis;
            }
        }
        delta[dominant_axis] = original[dominant_axis].signum();
    }

    GeoPos::new(
        from.x_cm + delta[0],
        from.y_cm + delta[1],
        from.z_cm + delta[2],
    )
}

fn squared_distance_cm(x: i64, y: i64, z: i64) -> i128 {
    let x = x as i128;
    let y = y as i128;
    let z = z as i128;
    (x * x) + (y * y) + (z * z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::space_distance_cm;

    #[test]
    fn move_towards_geo_pos_keeps_progress_within_step_budget() {
        let from = GeoPos::new(0, 0, 0);
        let to = GeoPos::new(100, 100, 100);

        let moved = move_towards_geo_pos(from, to, 1);

        assert_ne!(moved, from);
        assert!(space_distance_cm(from, moved) <= 1);
    }

    #[test]
    fn move_towards_geo_pos_returns_target_when_step_reaches_destination() {
        let from = GeoPos::new(0, 0, 0);
        let to = GeoPos::new(3, 4, 0);

        assert_eq!(move_towards_geo_pos(from, to, 5), to);
    }
}
