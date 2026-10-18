use super::super::persist_error::PersistError;
use super::super::power::PowerEvent;
use super::super::types::{PowerOrderSide, ResourceKind, ResourceOwner};
use super::super::world_model::PowerOrderState;
use super::WorldKernel;
use super::types::PowerOrderFill;
use crate::WorldTime;

const LOCATION_ELECTRICITY_POOL_REMOVED_NOTE: &str = "location electricity pool removed";

impl WorldKernel {
    #[expect(
        clippy::too_many_arguments,
        reason = "Stable replay seam preserves the serialized power-order event fields."
    )]
    pub(super) fn replay_power_order_placed(
        &mut self,
        event_time: WorldTime,
        order_id: u64,
        owner: &ResourceOwner,
        side: PowerOrderSide,
        requested_amount: i64,
        remaining_amount: i64,
        limit_price_per_pu: i64,
        fills: &[PowerOrderFill],
        auto_cancelled_order_ids: &[u64],
    ) -> Result<(), PersistError> {
        if order_id == 0 {
            return Err(PersistError::ReplayConflict {
                message: "power order id must be > 0".to_string(),
            });
        }
        if requested_amount <= 0 {
            return Err(PersistError::ReplayConflict {
                message: format!("power requested amount must be > 0, got {requested_amount}"),
            });
        }
        if remaining_amount < 0 || remaining_amount > requested_amount {
            return Err(PersistError::ReplayConflict {
                message: format!(
                    "invalid power remaining amount {remaining_amount} for requested {requested_amount}"
                ),
            });
        }
        if limit_price_per_pu < 0 {
            return Err(PersistError::ReplayConflict {
                message: format!("invalid power order limit price {limit_price_per_pu}"),
            });
        }
        self.ensure_owner_exists(owner)
            .map_err(|reason| PersistError::ReplayConflict {
                message: format!("invalid power order owner: {reason:?}"),
            })?;
        if matches!(owner, ResourceOwner::Location { .. }) {
            return Err(PersistError::ReplayConflict {
                message: LOCATION_ELECTRICITY_POOL_REMOVED_NOTE.to_string(),
            });
        }
        if self
            .model
            .power_order_book
            .open_orders
            .iter()
            .any(|entry| entry.order_id == order_id)
        {
            return Err(PersistError::ReplayConflict {
                message: format!("power order already exists: {order_id}"),
            });
        }
        self.model.power_order_book.next_order_id = self
            .model
            .power_order_book
            .next_order_id
            .max(order_id.saturating_add(1));

        for cancelled_order_id in auto_cancelled_order_ids {
            let Some(cancelled_index) = self
                .model
                .power_order_book
                .open_orders
                .iter()
                .position(|entry| entry.order_id == *cancelled_order_id)
            else {
                return Err(PersistError::ReplayConflict {
                    message: format!(
                        "power order auto-cancel target not found: {cancelled_order_id}"
                    ),
                });
            };
            self.model
                .power_order_book
                .open_orders
                .remove(cancelled_index);
        }

        let mut incoming_filled_amount = 0_i64;
        for fill in fills {
            if fill.amount <= 0 || fill.loss < 0 || fill.loss >= fill.amount {
                return Err(PersistError::ReplayConflict {
                    message: format!(
                        "invalid power order fill values: amount {} loss {}",
                        fill.amount, fill.loss
                    ),
                });
            }
            if fill.buy_order_id == fill.sell_order_id {
                return Err(PersistError::ReplayConflict {
                    message: format!(
                        "power order fill buy/sell id cannot be equal: {}",
                        fill.buy_order_id
                    ),
                });
            }
            if fill.buy_order_id == order_id || fill.sell_order_id == order_id {
                incoming_filled_amount = incoming_filled_amount.saturating_add(fill.amount);
            }

            self.ensure_owner_exists(&fill.seller).map_err(|reason| {
                PersistError::ReplayConflict {
                    message: format!("invalid power fill seller: {reason:?}"),
                }
            })?;
            self.ensure_owner_exists(&fill.buyer).map_err(|reason| {
                PersistError::ReplayConflict {
                    message: format!("invalid power fill buyer: {reason:?}"),
                }
            })?;
            self.ensure_owner_chunks_generated(&fill.seller, &fill.buyer)
                .map_err(|reason| PersistError::ReplayConflict {
                    message: format!("power order fill owner chunk generation failed: {reason:?}"),
                })?;
            if matches!(fill.seller, ResourceOwner::Location { .. })
                || matches!(fill.buyer, ResourceOwner::Location { .. })
            {
                return Err(PersistError::ReplayConflict {
                    message: LOCATION_ELECTRICITY_POOL_REMOVED_NOTE.to_string(),
                });
            }

            self.remove_from_owner_for_replay(
                &fill.seller,
                ResourceKind::Electricity,
                fill.amount,
            )?;
            let delivered = fill.amount.saturating_sub(fill.loss);
            if delivered <= 0 {
                return Err(PersistError::ReplayConflict {
                    message: format!(
                        "power order fill delivered amount must be positive: {delivered}"
                    ),
                });
            }
            self.add_to_owner_for_replay(&fill.buyer, ResourceKind::Electricity, delivered)?;

            for resting_order_id in [fill.buy_order_id, fill.sell_order_id] {
                if resting_order_id == order_id {
                    continue;
                }
                let Some(resting_index) = self
                    .model
                    .power_order_book
                    .open_orders
                    .iter()
                    .position(|entry| entry.order_id == resting_order_id)
                else {
                    return Err(PersistError::ReplayConflict {
                        message: format!(
                            "power order fill resting order not found: {resting_order_id}"
                        ),
                    });
                };
                let resting = &mut self.model.power_order_book.open_orders[resting_index];
                if resting.remaining_amount < fill.amount {
                    return Err(PersistError::ReplayConflict {
                        message: format!(
                            "power order fill exceeds resting order remaining: order {} remaining {} fill {}",
                            resting_order_id, resting.remaining_amount, fill.amount
                        ),
                    });
                }
                resting.remaining_amount = resting.remaining_amount.saturating_sub(fill.amount);
                if resting.remaining_amount == 0 {
                    self.model
                        .power_order_book
                        .open_orders
                        .remove(resting_index);
                }
            }
        }

        let expected_remaining = requested_amount.saturating_sub(incoming_filled_amount);
        if expected_remaining != remaining_amount {
            return Err(PersistError::ReplayConflict {
                message: format!(
                    "power order remaining mismatch: expected {expected_remaining}, got {remaining_amount}"
                ),
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
                    created_at: event_time,
                });
        }

        Ok(())
    }

    pub(super) fn replay_power_order_cancelled(
        &mut self,
        owner: &ResourceOwner,
        order_id: u64,
        side: PowerOrderSide,
        remaining_amount: i64,
    ) -> Result<(), PersistError> {
        if order_id == 0 {
            return Err(PersistError::ReplayConflict {
                message: "power order cancel id must be > 0".to_string(),
            });
        }
        if remaining_amount <= 0 {
            return Err(PersistError::ReplayConflict {
                message: format!(
                    "power order cancel remaining amount must be > 0, got {remaining_amount}"
                ),
            });
        }
        self.ensure_owner_exists(owner)
            .map_err(|reason| PersistError::ReplayConflict {
                message: format!("invalid power order cancel owner: {reason:?}"),
            })?;
        if matches!(owner, ResourceOwner::Location { .. }) {
            return Err(PersistError::ReplayConflict {
                message: LOCATION_ELECTRICITY_POOL_REMOVED_NOTE.to_string(),
            });
        }
        let Some(order_index) = self
            .model
            .power_order_book
            .open_orders
            .iter()
            .position(|entry| entry.order_id == order_id && entry.owner == *owner)
        else {
            return Err(PersistError::ReplayConflict {
                message: format!(
                    "power order cancel target not found: order_id={order_id} owner={owner:?}"
                ),
            });
        };
        let removed = self.model.power_order_book.open_orders.remove(order_index);
        if removed.side != side {
            return Err(PersistError::ReplayConflict {
                message: format!(
                    "power order cancel side mismatch: order {order_id} expected {:?} got {:?}",
                    removed.side, side
                ),
            });
        }
        if removed.remaining_amount != remaining_amount {
            return Err(PersistError::ReplayConflict {
                message: format!(
                    "power order cancel remaining mismatch: order {order_id} expected {} got {remaining_amount}",
                    removed.remaining_amount
                ),
            });
        }

        Ok(())
    }

    pub(super) fn replay_power_event(&mut self, event: &PowerEvent) -> Result<(), PersistError> {
        match event {
            PowerEvent::PowerPlantRegistered { plant } => {
                if self.model.power_plants.contains_key(&plant.id) {
                    return Err(PersistError::ReplayConflict {
                        message: format!("power plant already exists: {}", plant.id),
                    });
                }
                if !self.model.locations.contains_key(&plant.location_id) {
                    return Err(PersistError::ReplayConflict {
                        message: format!(
                            "location not found for power plant: {}",
                            plant.location_id
                        ),
                    });
                }
                self.ensure_owner_exists(&plant.owner).map_err(|reason| {
                    PersistError::ReplayConflict {
                        message: format!("invalid power plant owner: {reason:?}"),
                    }
                })?;
                self.model
                    .power_plants
                    .insert(plant.id.clone(), plant.clone());
            }
            PowerEvent::PowerGenerated {
                plant_id,
                location_id,
                amount,
            } => {
                if *amount < 0 {
                    return Err(PersistError::ReplayConflict {
                        message: format!("invalid power generated amount: {amount}"),
                    });
                }
                let owner = {
                    let plant = self.model.power_plants.get_mut(plant_id).ok_or_else(|| {
                        PersistError::ReplayConflict {
                            message: format!("power plant not found: {plant_id}"),
                        }
                    })?;
                    if &plant.location_id != location_id {
                        return Err(PersistError::ReplayConflict {
                            message: format!(
                                "power plant location mismatch: expected {}, got {}",
                                plant.location_id, location_id
                            ),
                        });
                    }
                    plant.current_output = *amount;
                    plant.owner.clone()
                };
                self.add_to_owner_for_replay(&owner, ResourceKind::Electricity, *amount)?;
            }
            PowerEvent::PowerConsumed {
                agent_id, amount, ..
            } => {
                if let Some(agent) = self.model.agents.get_mut(agent_id) {
                    let power_config = self.config.power.clone();
                    agent.power.consume(*amount, &power_config);
                }
            }
            PowerEvent::PowerCharged {
                agent_id, amount, ..
            } => {
                if let Some(agent) = self.model.agents.get_mut(agent_id) {
                    let power_config = self.config.power.clone();
                    agent.power.charge(*amount, &power_config);
                }
            }
            PowerEvent::PowerStateChanged { .. } => {}
            PowerEvent::PowerTransferred {
                from,
                to,
                amount,
                loss,
                ..
            } => {
                if *amount < 0 || *loss < 0 || *loss > *amount {
                    return Err(PersistError::ReplayConflict {
                        message: format!(
                            "invalid power transfer values: amount {amount}, loss {loss}"
                        ),
                    });
                }
                self.ensure_owner_exists(from)
                    .map_err(|reason| PersistError::ReplayConflict {
                        message: format!("invalid power transfer source: {reason:?}"),
                    })?;
                self.ensure_owner_exists(to)
                    .map_err(|reason| PersistError::ReplayConflict {
                        message: format!("invalid power transfer target: {reason:?}"),
                    })?;
                if matches!(from, ResourceOwner::Location { .. })
                    || matches!(to, ResourceOwner::Location { .. })
                {
                    return Err(PersistError::ReplayConflict {
                        message: format!(
                            "{LOCATION_ELECTRICITY_POOL_REMOVED_NOTE}: location power transfer unsupported"
                        ),
                    });
                }
                self.remove_from_owner_for_replay(from, ResourceKind::Electricity, *amount)?;
                let delivered = amount.saturating_sub(*loss);
                if delivered <= 0 {
                    return Err(PersistError::ReplayConflict {
                        message: format!(
                            "power transfer delivered amount must be positive: {delivered}"
                        ),
                    });
                }
                self.add_to_owner_for_replay(to, ResourceKind::Electricity, delivered)?;
            }
        }

        Ok(())
    }
}
