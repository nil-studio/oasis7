impl WorldKernel {
    fn next_power_order_id(&self) -> u64 {
        self.model.power_order_book.next_order_id.max(1)
    }

    fn find_power_order_index(&self, order_id: u64) -> Option<usize> {
        self.model
            .power_order_book
            .open_orders
            .iter()
            .position(|entry| entry.order_id == order_id)
    }

    fn power_order_limits_cross(
        incoming_side: PowerOrderSide,
        incoming_limit_price_per_pu: i64,
        opposite_limit_price_per_pu: i64,
    ) -> bool {
        match incoming_side {
            PowerOrderSide::Buy => incoming_limit_price_per_pu >= opposite_limit_price_per_pu,
            PowerOrderSide::Sell => opposite_limit_price_per_pu >= incoming_limit_price_per_pu,
        }
    }

    fn power_order_quote_within_limits(
        quoted_price_per_pu: i64,
        sell_limit_price_per_pu: i64,
        buy_limit_price_per_pu: i64,
    ) -> bool {
        quoted_price_per_pu >= sell_limit_price_per_pu
            && quoted_price_per_pu <= buy_limit_price_per_pu
    }

    fn append_auto_cancelled_order_id(auto_cancelled_order_ids: &mut Vec<u64>, order_id: u64) {
        if !auto_cancelled_order_ids.contains(&order_id) {
            auto_cancelled_order_ids.push(order_id);
        }
    }

    fn place_power_order(
        &mut self,
        owner: ResourceOwner,
        side: PowerOrderSide,
        amount: i64,
        limit_price_per_pu: i64,
    ) -> WorldEventKind {
        if amount <= 0 {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::InvalidAmount { amount },
            };
        }
        if limit_price_per_pu < 0 {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::InvalidAmount {
                    amount: limit_price_per_pu,
                },
            };
        }
        if let Err(reason) = self.ensure_owner_exists(&owner) {
            return WorldEventKind::ActionRejected { reason };
        }
        if matches!(owner, ResourceOwner::Location { .. }) {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec![LOCATION_ELECTRICITY_POOL_REMOVED_NOTE.to_string()],
                },
            };
        }
        if matches!(side, PowerOrderSide::Sell) {
            let available = self
                .owner_stock(&owner)
                .map(|stock| stock.get(ResourceKind::Electricity))
                .unwrap_or(0);
            if available < amount {
                return WorldEventKind::ActionRejected {
                    reason: RejectReason::InsufficientResource {
                        owner,
                        kind: ResourceKind::Electricity,
                        requested: amount,
                        available,
                    },
                };
            }
        }

        let order_id = self.next_power_order_id();
        self.model.power_order_book.next_order_id = order_id.saturating_add(1);
        let mut remaining_amount = amount;
        let mut fills = Vec::new();
        let mut auto_cancelled_order_ids = Vec::new();

        let candidates = self.sorted_opposite_power_order_candidates(side);
        let mut removed_original_indices = Vec::new();
        for candidate in candidates {
            if remaining_amount <= 0 {
                break;
            }

            let Some(candidate_index) =
                self.current_power_order_index(&candidate, &removed_original_indices)
            else {
                continue;
            };
            if !Self::power_order_limits_cross(
                side,
                limit_price_per_pu,
                candidate.limit_price_per_pu,
            ) {
                break;
            }

            let fill_amount = remaining_amount.min(candidate.remaining_amount);
            if fill_amount <= 0 {
                self.model
                    .power_order_book
                    .open_orders
                    .remove(candidate_index);
                Self::record_removed_power_order_original_index(
                    &mut removed_original_indices,
                    candidate.original_index,
                );
                Self::append_auto_cancelled_order_id(
                    &mut auto_cancelled_order_ids,
                    candidate.order_id,
                );
                continue;
            }

            let (seller, buyer, sell_limit_price_per_pu, buy_limit_price_per_pu) = match side {
                PowerOrderSide::Buy => (
                    candidate.owner.clone(),
                    owner.clone(),
                    candidate.limit_price_per_pu,
                    limit_price_per_pu,
                ),
                PowerOrderSide::Sell => (
                    owner.clone(),
                    candidate.owner.clone(),
                    limit_price_per_pu,
                    candidate.limit_price_per_pu,
                ),
            };
            let (buy_order_id, sell_order_id) = match side {
                PowerOrderSide::Buy => (order_id, candidate.order_id),
                PowerOrderSide::Sell => (candidate.order_id, order_id),
            };

            let prepared = match self.prepare_power_transfer(&seller, &buyer, fill_amount) {
                Ok(prepared) => prepared,
                Err(reason) => {
                    if matches!(side, PowerOrderSide::Buy)
                        && matches!(
                            reason,
                            RejectReason::InsufficientResource {
                                owner: ref rejected_owner,
                                kind: ResourceKind::Electricity,
                                ..
                            } if rejected_owner == &seller
                        )
                    {
                        self.model
                            .power_order_book
                            .open_orders
                            .remove(candidate_index);
                        Self::append_auto_cancelled_order_id(
                            &mut auto_cancelled_order_ids,
                            candidate.order_id,
                        );
                        Self::record_removed_power_order_original_index(
                            &mut removed_original_indices,
                            candidate.original_index,
                        );
                    }
                    continue;
                }
            };

            if !Self::power_order_quote_within_limits(
                prepared.quoted_price_per_pu,
                sell_limit_price_per_pu,
                buy_limit_price_per_pu,
            ) {
                if (matches!(side, PowerOrderSide::Buy)
                    && prepared.quoted_price_per_pu < sell_limit_price_per_pu)
                    || (matches!(side, PowerOrderSide::Sell)
                        && prepared.quoted_price_per_pu > buy_limit_price_per_pu)
                {
                    break;
                }
                continue;
            }

            let transfer = match self.transfer_power(
                &seller,
                &buyer,
                fill_amount,
                prepared.quoted_price_per_pu,
            ) {
                Ok(transfer) => transfer,
                Err(reason) => {
                    if matches!(side, PowerOrderSide::Buy)
                        && matches!(
                            reason,
                            RejectReason::InsufficientResource {
                                owner: ref rejected_owner,
                                kind: ResourceKind::Electricity,
                                ..
                            } if rejected_owner == &seller
                        )
                    {
                        self.model
                            .power_order_book
                            .open_orders
                            .remove(candidate_index);
                        Self::append_auto_cancelled_order_id(
                            &mut auto_cancelled_order_ids,
                            candidate.order_id,
                        );
                        Self::record_removed_power_order_original_index(
                            &mut removed_original_indices,
                            candidate.original_index,
                        );
                    }
                    continue;
                }
            };

            let PowerEvent::PowerTransferred {
                from,
                to,
                amount: transferred_amount,
                loss,
                quoted_price_per_pu,
                price_per_pu,
                settlement_amount,
            } = transfer
            else {
                continue;
            };

            let Some(candidate_state) = self
                .model
                .power_order_book
                .open_orders
                .get_mut(candidate_index)
            else {
                return Self::power_orderbook_inconsistent_missing(candidate.order_id);
            };
            if candidate_state.order_id != candidate.order_id {
                return Self::power_orderbook_inconsistent_missing(candidate.order_id);
            }
            if candidate_state.remaining_amount < transferred_amount {
                return WorldEventKind::ActionRejected {
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "power orderbook inconsistent: order {} remaining {} < fill {}",
                            candidate.order_id,
                            candidate_state.remaining_amount,
                            transferred_amount
                        )],
                    },
                };
            }
            candidate_state.remaining_amount = candidate_state
                .remaining_amount
                .saturating_sub(transferred_amount);
            if candidate_state.remaining_amount == 0 {
                self.model
                    .power_order_book
                    .open_orders
                    .remove(candidate_index);
                Self::record_removed_power_order_original_index(
                    &mut removed_original_indices,
                    candidate.original_index,
                );
            }

            remaining_amount = remaining_amount.saturating_sub(transferred_amount);
            fills.push(PowerOrderFill {
                buy_order_id,
                sell_order_id,
                buyer: to,
                seller: from,
                amount: transferred_amount,
                loss,
                quoted_price_per_pu,
                price_per_pu,
                settlement_amount,
            });
        }

        if remaining_amount > 0 {
            self.model
                .power_order_book
                .open_orders
                .push(PowerOrderState {
                    order_id,
                    owner: owner.clone(),
                    side,
                    remaining_amount,
                    limit_price_per_pu,
                    created_at: self.time,
                });
        }

        WorldEventKind::PowerOrderPlaced {
            order_id,
            owner,
            side,
            requested_amount: amount,
            remaining_amount,
            limit_price_per_pu,
            fills,
            auto_cancelled_order_ids,
        }
    }

    fn cancel_power_order(&mut self, owner: ResourceOwner, order_id: u64) -> WorldEventKind {
        if order_id == 0 {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::InvalidAmount { amount: 0 },
            };
        }
        if let Err(reason) = self.ensure_owner_exists(&owner) {
            return WorldEventKind::ActionRejected { reason };
        }
        if matches!(owner, ResourceOwner::Location { .. }) {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec![LOCATION_ELECTRICITY_POOL_REMOVED_NOTE.to_string()],
                },
            };
        }

        let Some(order_index) = self
            .model
            .power_order_book
            .open_orders
            .iter()
            .position(|entry| entry.order_id == order_id && entry.owner == owner)
        else {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "cancel power order rejected: order {} not found for owner {:?}",
                        order_id, owner
                    )],
                },
            };
        };

        let removed = self.model.power_order_book.open_orders.remove(order_index);
        WorldEventKind::PowerOrderCancelled {
            owner,
            order_id,
            side: removed.side,
            remaining_amount: removed.remaining_amount,
        }
    }

    fn apply_build_factory(
        &mut self,
        owner: ResourceOwner,
        location_id: String,
        factory_id: String,
        factory_kind: String,
    ) -> WorldEventKind {
        if factory_id.trim().is_empty() {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::InvalidAmount { amount: 0 },
            };
        }
        if factory_kind.trim().is_empty() {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec!["factory_kind cannot be empty".to_string()],
                },
            };
        }
        if !self.model.locations.contains_key(&location_id) {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::LocationNotFound { location_id },
            };
        }
        if self.model.factories.contains_key(&factory_id) {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::FacilityAlreadyExists {
                    facility_id: factory_id,
                },
            };
        }
        let is_radiation_power_factory =
            factory_kind.eq_ignore_ascii_case(FACTORY_KIND_RADIATION_POWER_MK1);
        if is_radiation_power_factory && self.model.power_plants.contains_key(&factory_id) {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::FacilityAlreadyExists {
                    facility_id: factory_id,
                },
            };
        }
        if let Err(reason) = self.ensure_owner_exists(&owner) {
            return WorldEventKind::ActionRejected { reason };
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

        let electricity_cost = self.config.economy.factory_build_electricity_cost;
        let hardware_cost = self.config.economy.factory_build_hardware_cost;

        let available_electricity = self
            .owner_stock(&owner)
            .map(|stock| stock.get(ResourceKind::Electricity))
            .unwrap_or(0);
        if available_electricity < electricity_cost {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::InsufficientResource {
                    owner,
                    kind: ResourceKind::Electricity,
                    requested: electricity_cost,
                    available: available_electricity,
                },
            };
        }
        let available_hardware = self
            .owner_stock(&owner)
            .map(|stock| stock.get(ResourceKind::Data))
            .unwrap_or(0);
        if available_hardware < hardware_cost {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::InsufficientResource {
                    owner,
                    kind: ResourceKind::Data,
                    requested: hardware_cost,
                    available: available_hardware,
                },
            };
        }

        if let Err(reason) =
            self.remove_from_owner(&owner, ResourceKind::Electricity, electricity_cost)
        {
            return WorldEventKind::ActionRejected { reason };
        }
        if let Err(reason) = self.remove_from_owner(&owner, ResourceKind::Data, hardware_cost) {
            return WorldEventKind::ActionRejected { reason };
        }

        self.model.factories.insert(
            factory_id.clone(),
            Factory {
                id: factory_id.clone(),
                owner: owner.clone(),
                location_id: location_id.clone(),
                kind: factory_kind.clone(),
            },
        );
        if is_radiation_power_factory {
            self.model.power_plants.insert(
                factory_id.clone(),
                PowerPlant {
                    id: factory_id.clone(),
                    location_id: location_id.clone(),
                    owner: owner.clone(),
                    capacity_per_tick: self.config.economy.radiation_power_plant_output_per_tick,
                    current_output: 0,
                    fuel_cost_per_pu: 0,
                    maintenance_cost: 0,
                    status: PlantStatus::Running,
                    efficiency: 1.0,
                    degradation: 0.0,
                },
            );
        }
        WorldEventKind::FactoryBuilt {
            owner,
            location_id,
            factory_id,
            factory_kind,
            electricity_cost,
            hardware_cost,
        }
    }

    fn apply_schedule_recipe(
        &mut self,
        owner: ResourceOwner,
        factory_id: String,
        recipe_id: String,
        batches: i64,
    ) -> WorldEventKind {
        if recipe_id.trim().is_empty() {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec!["recipe_id cannot be empty".to_string()],
                },
            };
        }
        if batches <= 0 {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::InvalidAmount { amount: batches },
            };
        }
        if let Err(reason) = self.ensure_owner_exists(&owner) {
            return WorldEventKind::ActionRejected { reason };
        }

        let Some(factory) = self.model.factories.get(&factory_id).cloned() else {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::FacilityNotFound {
                    facility_id: factory_id,
                },
            };
        };
        if factory.owner != owner {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec!["factory owner mismatch".to_string()],
                },
            };
        }
        let site_owner = ResourceOwner::Location {
            location_id: factory.location_id.clone(),
        };
        if let Err(reason) = self.ensure_colocated(&owner, &site_owner) {
            return WorldEventKind::ActionRejected { reason };
        }
        if let Err(reason) = self.ensure_owner_chunks_generated(&owner, &site_owner) {
            return WorldEventKind::ActionRejected { reason };
        }

        let Some(plan) = self.recipe_plan(recipe_id.as_str()) else {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec![format!("unsupported recipe_id: {recipe_id}")],
                },
            };
        };
        if !factory
            .kind
            .eq_ignore_ascii_case(plan.required_factory_kind)
        {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "recipe {recipe_id} requires factory kind {}, got {}",
                        plan.required_factory_kind, factory.kind
                    )],
                },
            };
        }
        let recipe_scale = batches;
        let electricity_cost = plan.electricity_per_batch.saturating_mul(recipe_scale);
        let hardware_cost = plan.hardware_per_batch.saturating_mul(recipe_scale);
        let data_output = plan.data_output_per_batch.saturating_mul(recipe_scale);
        let finished_product_units = plan
            .finished_product_units_per_batch
            .saturating_mul(recipe_scale);

        let available_electricity = self
            .owner_stock(&owner)
            .map(|stock| stock.get(ResourceKind::Electricity))
            .unwrap_or(0);
        if available_electricity < electricity_cost {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::InsufficientResource {
                    owner,
                    kind: ResourceKind::Electricity,
                    requested: electricity_cost,
                    available: available_electricity,
                },
            };
        }
        let available_hardware = self
            .owner_stock(&owner)
            .map(|stock| stock.get(ResourceKind::Data))
            .unwrap_or(0);
        if available_hardware < hardware_cost {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::InsufficientResource {
                    owner,
                    kind: ResourceKind::Data,
                    requested: hardware_cost,
                    available: available_hardware,
                },
            };
        }

        if data_output > 0
            && available_hardware
                .checked_sub(hardware_cost)
                .and_then(|data_after_cost| data_after_cost.checked_add(data_output))
                .is_none()
        {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::InvalidAmount {
                    amount: data_output,
                },
            };
        }

        if let Err(reason) =
            self.remove_from_owner(&owner, ResourceKind::Electricity, electricity_cost)
        {
            return WorldEventKind::ActionRejected { reason };
        }
        if let Err(reason) = self.remove_from_owner(&owner, ResourceKind::Data, hardware_cost) {
            return WorldEventKind::ActionRejected { reason };
        }
        if data_output > 0
            && let Err(reason) = self.add_to_owner(&owner, ResourceKind::Data, data_output)
        {
            return WorldEventKind::ActionRejected { reason };
        }

        WorldEventKind::RecipeScheduled {
            owner,
            factory_id,
            recipe_id,
            batches,
            electricity_cost,
            hardware_cost,
            data_output,
            finished_product_id: plan.finished_product_id.to_string(),
            finished_product_units,
        }
    }

    fn prepare_power_transfer(
        &mut self,
        from: &ResourceOwner,
        to: &ResourceOwner,
        amount: i64,
    ) -> Result<PreparedPowerTransfer, RejectReason> {
        if amount <= 0 {
            return Err(RejectReason::InvalidAmount { amount });
        }
        self.ensure_owner_exists(from)?;
        self.ensure_owner_exists(to)?;
        self.ensure_owner_chunks_generated(from, to)?;
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

    fn transfer_power(
        &mut self,
        from: &ResourceOwner,
        to: &ResourceOwner,
        amount: i64,
        requested_price_per_pu: i64,
    ) -> Result<PowerEvent, RejectReason> {
        if requested_price_per_pu < 0 {
            return Err(RejectReason::InvalidAmount {
                amount: requested_price_per_pu,
            });
        }

        let prepared = self.prepare_power_transfer(from, to, amount)?;
        let executed_price_per_pu = self
            .resolve_power_transfer_price(requested_price_per_pu, prepared.quoted_price_per_pu)?;

        let delivered = amount - prepared.loss;
        self.remove_from_owner(from, ResourceKind::Electricity, amount)?;
        if delivered > 0 {
            self.add_to_owner(to, ResourceKind::Electricity, delivered)?;
        } else {
            return Err(RejectReason::PowerTransferLossExceedsAmount {
                amount,
                loss: prepared.loss,
            });
        }
        let settlement_amount = delivered.saturating_mul(executed_price_per_pu);

        Ok(PowerEvent::PowerTransferred {
            from: from.clone(),
            to: to.clone(),
            amount,
            loss: prepared.loss,
            quoted_price_per_pu: prepared.quoted_price_per_pu,
            price_per_pu: executed_price_per_pu,
            settlement_amount,
        })
    }

    fn owner_location_id(&self, owner: &ResourceOwner) -> Result<String, RejectReason> {
        match owner {
            ResourceOwner::Agent { agent_id } => self
                .model
                .agents
                .get(agent_id)
                .map(|agent| agent.location_id.clone())
                .ok_or_else(|| RejectReason::AgentNotFound {
                    agent_id: agent_id.clone(),
                }),
            ResourceOwner::Location { location_id } => {
                if self.model.locations.contains_key(location_id) {
                    Ok(location_id.clone())
                } else {
                    Err(RejectReason::LocationNotFound {
                        location_id: location_id.clone(),
                    })
                }
            }
        }
    }

    fn power_transfer_distance_km(
        &self,
        from_location_id: &str,
        to_location_id: &str,
    ) -> Result<i64, RejectReason> {
        let from = self.model.locations.get(from_location_id).ok_or_else(|| {
            RejectReason::LocationNotFound {
                location_id: from_location_id.to_string(),
            }
        })?;
        let to = self.model.locations.get(to_location_id).ok_or_else(|| {
            RejectReason::LocationNotFound {
                location_id: to_location_id.to_string(),
            }
        })?;
        let distance_cm = space_distance_cm(from.pos, to.pos);
        let distance_km = (distance_cm + CM_PER_KM - 1) / CM_PER_KM;
        Ok(distance_km)
    }

    fn power_transfer_loss(&self, amount: i64, distance_km: i64) -> i64 {
        if amount <= 0 || distance_km <= 0 {
            return 0;
        }
        let bps = self.config.power.transfer_loss_per_km_bps;
        if bps <= 0 {
            return 0;
        }
        let loss = (amount as i128)
            .saturating_mul(distance_km as i128)
            .saturating_mul(bps as i128)
            / 10_000;
        loss.min(amount as i128) as i64
    }

    fn quote_power_market_price_per_pu(&self, amount: i64, seller_available_before: i64) -> i64 {
        let cfg = &self.config.power;
        let min_price = cfg.market_price_min_per_pu.max(0);
        let max_price = cfg.market_price_max_per_pu.max(min_price);
        let base_price = cfg.market_base_price_per_pu.clamp(min_price, max_price);

        // Pure supply-demand pricing:
        // demand_pressure_bps = clamp((requested / available - 1) * 10_000, 0, max_bps)
        let demand_pressure_bps = if seller_available_before <= 0 {
            cfg.market_scarcity_price_max_bps
        } else {
            ((amount as i128)
                .saturating_mul(10_000)
                .saturating_div(seller_available_before as i128))
            .saturating_sub(10_000)
            .clamp(0, cfg.market_scarcity_price_max_bps as i128) as i64
        };
        let demand_premium =
            ((base_price as i128).saturating_mul(demand_pressure_bps as i128) / 10_000) as i64;

        base_price
            .saturating_add(demand_premium)
            .clamp(min_price, max_price)
    }

    fn validate_transfer(
        &self,
        from: &ResourceOwner,
        to: &ResourceOwner,
        kind: ResourceKind,
        amount: i64,
    ) -> Result<(), RejectReason> {
        if amount <= 0 {
            return Err(RejectReason::InvalidAmount { amount });
        }

        self.ensure_owner_exists(from)?;
        self.ensure_owner_exists(to)?;
        self.ensure_colocated(from, to)?;

        let available = self
            .owner_stock(from)
            .map(|stock| stock.get(kind))
            .unwrap_or(0);
        if available < amount {
            return Err(RejectReason::InsufficientResource {
                owner: from.clone(),
                kind,
                requested: amount,
                available,
            });
        }

        Ok(())
    }

    fn apply_transfer(
        &mut self,
        from: &ResourceOwner,
        to: &ResourceOwner,
        kind: ResourceKind,
        amount: i64,
    ) -> Result<(), RejectReason> {
        self.remove_from_owner(from, kind, amount)?;
        self.add_to_owner(to, kind, amount)?;
        Ok(())
    }

    pub(super) fn ensure_module_visual_anchor_exists(
        &self,
        anchor: &ModuleVisualAnchor,
    ) -> Result<(), RejectReason> {
        match anchor {
            ModuleVisualAnchor::Agent { agent_id } => {
                if self.model.agents.contains_key(agent_id) {
                    Ok(())
                } else {
                    Err(RejectReason::AgentNotFound {
                        agent_id: agent_id.clone(),
                    })
                }
            }
            ModuleVisualAnchor::Location { location_id } => {
                if self.model.locations.contains_key(location_id) {
                    Ok(())
                } else {
                    Err(RejectReason::LocationNotFound {
                        location_id: location_id.clone(),
                    })
                }
            }
            ModuleVisualAnchor::Absolute { pos } => {
                if self.config.space.contains(*pos) {
                    Ok(())
                } else {
                    Err(RejectReason::PositionOutOfBounds { pos: *pos })
                }
            }
        }
    }

    pub(super) fn ensure_owner_exists(&self, owner: &ResourceOwner) -> Result<(), RejectReason> {
        match owner {
            ResourceOwner::Agent { agent_id } => {
                if self.model.agents.contains_key(agent_id) {
                    Ok(())
                } else {
                    Err(RejectReason::AgentNotFound {
                        agent_id: agent_id.clone(),
                    })
                }
            }
            ResourceOwner::Location { location_id } => {
                if self.model.locations.contains_key(location_id) {
                    Ok(())
                } else {
                    Err(RejectReason::LocationNotFound {
                        location_id: location_id.clone(),
                    })
                }
            }
        }
    }

    pub(super) fn ensure_colocated(
        &self,
        from: &ResourceOwner,
        to: &ResourceOwner,
    ) -> Result<(), RejectReason> {
        match (from, to) {
            (ResourceOwner::Agent { agent_id }, ResourceOwner::Location { location_id }) => {
                let agent =
                    self.model
                        .agents
                        .get(agent_id)
                        .ok_or_else(|| RejectReason::AgentNotFound {
                            agent_id: agent_id.clone(),
                        })?;
                if agent.location_id != *location_id {
                    return Err(RejectReason::AgentNotAtLocation {
                        agent_id: agent_id.clone(),
                        location_id: location_id.clone(),
                    });
                }
            }
            (ResourceOwner::Location { location_id }, ResourceOwner::Agent { agent_id }) => {
                let agent =
                    self.model
                        .agents
                        .get(agent_id)
                        .ok_or_else(|| RejectReason::AgentNotFound {
                            agent_id: agent_id.clone(),
                        })?;
                if agent.location_id != *location_id {
                    return Err(RejectReason::AgentNotAtLocation {
                        agent_id: agent_id.clone(),
                        location_id: location_id.clone(),
                    });
                }
            }
            (
                ResourceOwner::Agent { agent_id },
                ResourceOwner::Agent {
                    agent_id: other_agent_id,
                },
            ) => {
                let agent =
                    self.model
                        .agents
                        .get(agent_id)
                        .ok_or_else(|| RejectReason::AgentNotFound {
                            agent_id: agent_id.clone(),
                        })?;
                let other = self.model.agents.get(other_agent_id).ok_or_else(|| {
                    RejectReason::AgentNotFound {
                        agent_id: other_agent_id.clone(),
                    }
                })?;
                if agent.location_id != other.location_id {
                    return Err(RejectReason::AgentsNotCoLocated {
                        agent_id: agent_id.clone(),
                        other_agent_id: other_agent_id.clone(),
                    });
                }
            }
            (
                ResourceOwner::Location { location_id },
                ResourceOwner::Location {
                    location_id: other_location_id,
                },
            ) => {
                return Err(RejectReason::LocationTransferNotAllowed {
                    from: location_id.clone(),
                    to: other_location_id.clone(),
                });
            }
        }
        Ok(())
    }

    pub(super) fn ensure_owner_chunks_generated(
        &mut self,
        from: &ResourceOwner,
        to: &ResourceOwner,
    ) -> Result<(), RejectReason> {
        self.ensure_owner_chunk_generated(from)?;
        self.ensure_owner_chunk_generated(to)?;
        Ok(())
    }

    fn ensure_owner_chunk_generated(&mut self, owner: &ResourceOwner) -> Result<(), RejectReason> {
        if let Some(pos) = self.owner_pos(owner)? {
            self.ensure_chunk_generated_at(pos, ChunkGenerationCause::Action)?;
        }
        Ok(())
    }

    pub(super) fn radiation_available_at(&self, harvest_pos: GeoPos) -> i64 {
        let physics = &self.config.physics;
        let near_range_cm = CHUNK_SIZE_X_CM.max(1) as f64;
        let mut near_sources = 0.0;
        let mut background = 0.0;

        for source in self.model.locations.values() {
            let contribution = self.radiation_source_contribution(harvest_pos, source);
            if contribution <= 0.0 {
                continue;
            }

            let source_radius_cm = source.profile.radius_cm.max(1) as f64;
            let source_distance_cm = space_distance_cm(harvest_pos, source.pos).max(0) as f64;
            let surface_distance_cm = (source_distance_cm - source_radius_cm).max(0.0);

            if surface_distance_cm <= near_range_cm {
                near_sources += contribution;
            } else {
                background += contribution;
            }
        }

        let floor = physics.radiation_floor.max(0);
        let floor_cap = physics.radiation_floor_cap_per_tick.max(0);
        let floor_contribution = floor.min(floor_cap) as f64;
        (near_sources + background + floor_contribution).floor() as i64
    }

    fn radiation_source_contribution(&self, harvest_pos: GeoPos, source: &Location) -> f64 {
        let emission = source.profile.radiation_emission_per_tick.max(0) as f64;
        if emission <= 0.0 {
            return 0.0;
        }

        let source_radius_cm = source.profile.radius_cm.max(1) as f64;
        let source_distance_cm = space_distance_cm(harvest_pos, source.pos).max(0) as f64;
        let surface_distance_cm = (source_distance_cm - source_radius_cm).max(0.0);
        let normalized_distance = surface_distance_cm / source_radius_cm;

        let geometric_attenuation = 1.0 / (1.0 + normalized_distance * normalized_distance);
        let medium_decay = (-self.config.physics.radiation_decay_k * surface_distance_cm).exp();
        emission * geometric_attenuation * medium_decay
    }

    fn compute_mine_compound_electricity_cost(&self, compound_mass_g: i64) -> i64 {
        let mass_kg = compound_mass_g.saturating_add(999).saturating_div(1000);
        mass_kg.saturating_mul(self.config.economy.mine_electricity_cost_per_kg)
    }

    fn plan_compound_extraction(
        &self,
        location_id: &str,
        compound_mass_g: i64,
    ) -> Result<Vec<(FragmentElementKind, i64)>, RejectReason> {
        let location = self.model.locations.get(location_id).ok_or_else(|| {
            RejectReason::LocationNotFound {
                location_id: location_id.to_string(),
            }
        })?;
        let budget = location.fragment_budget.as_ref().ok_or_else(|| {
            RejectReason::InsufficientResource {
                owner: ResourceOwner::Location {
                    location_id: location_id.to_string(),
                },
                kind: ResourceKind::Data,
                requested: compound_mass_g,
                available: 0,
            }
        })?;

        let total_available = budget
            .remaining_by_element_g
            .values()
            .copied()
            .filter(|amount| *amount > 0)
            .sum::<i64>();
        if total_available < compound_mass_g {
            return Err(RejectReason::InsufficientResource {
                owner: ResourceOwner::Location {
                    location_id: location_id.to_string(),
                },
                kind: ResourceKind::Data,
                requested: compound_mass_g,
                available: total_available,
            });
        }

        let mut remaining = compound_mass_g;
        let mut plan = Vec::new();
        for (element, available) in &budget.remaining_by_element_g {
            if remaining <= 0 {
                break;
            }
            if *available <= 0 {
                continue;
            }
            let consume = (*available).min(remaining);
            if consume > 0 {
                plan.push((*element, consume));
                remaining = remaining.saturating_sub(consume);
            }
        }
        if remaining > 0 {
            return Err(RejectReason::InsufficientResource {
                owner: ResourceOwner::Location {
                    location_id: location_id.to_string(),
                },
                kind: ResourceKind::Data,
                requested: compound_mass_g,
                available: compound_mass_g.saturating_sub(remaining),
            });
        }
        Ok(plan)
    }

    fn consume_fragment_resource_for_action(
        &mut self,
        location_id: &str,
        kind: FragmentElementKind,
        amount_g: i64,
    ) -> Result<(), RejectReason> {
        self.consume_fragment_resource(location_id, kind, amount_g)
            .map(|_| ())
            .map_err(|err| self.fragment_error_to_reject_reason(location_id, err))
    }
}
