//! Event-bound sparse marketplace transaction. Canonical state is borrowed until install.
use super::*;
use serde::ser::SerializeMap;

#[derive(Debug)]
pub(crate) struct PreparedModuleMarketplace {
    events: Vec<DomainEvent>,
    hash: String,
    pub(crate) agents: BTreeMap<String, AgentCell>,
    pub(crate) resources: BTreeMap<ResourceKind, i64>,
    module_artifact_owners: BTreeMap<String, String>,
    module_artifact_listings: BTreeMap<String, ModuleArtifactListingState>,
    module_artifact_bids: BTreeMap<String, Vec<ModuleArtifactBidState>>,
    next_module_market_order_id: u64,
    next_module_market_sale_id: u64,
    world_materials: BTreeMap<String, i64>,
}

impl PreparedModuleMarketplace {
    pub(crate) fn new(state: &WorldState, event: &DomainEvent) -> Self {
        let (hash, fee) = match event {
            DomainEvent::ModuleArtifactListed {
                wasm_hash,
                fee_kind,
                fee_amount,
                ..
            } => (wasm_hash, (*fee_amount > 0).then_some(*fee_kind)),
            DomainEvent::ModuleArtifactDeployed {
                wasm_hash,
                fee_kind,
                fee_amount,
                ..
            }
            | DomainEvent::ModuleArtifactDelisted {
                wasm_hash,
                fee_kind,
                fee_amount,
                ..
            }
            | DomainEvent::ModuleArtifactDestroyed {
                wasm_hash,
                fee_kind,
                fee_amount,
                ..
            } => (wasm_hash, (*fee_amount > 0).then_some(*fee_kind)),
            DomainEvent::ModuleArtifactBidPlaced { wasm_hash, .. }
            | DomainEvent::ModuleArtifactBidCancelled { wasm_hash, .. }
            | DomainEvent::ModuleArtifactSaleCompleted { wasm_hash, .. } => (wasm_hash, None),
            _ => unreachable!("marketplace preparation requires a marketplace event"),
        };
        Self {
            events: Vec::new(),
            hash: hash.clone(),
            agents: BTreeMap::new(),
            resources: fee
                .map(|kind| (kind, state.resources.get(&kind).copied().unwrap_or(0)))
                .into_iter()
                .collect(),
            module_artifact_owners: state
                .module_artifact_owners
                .get(hash)
                .map(|v| (hash.clone(), v.clone()))
                .into_iter()
                .collect(),
            module_artifact_listings: state
                .module_artifact_listings
                .get(hash)
                .map(|v| (hash.clone(), v.clone()))
                .into_iter()
                .collect(),
            module_artifact_bids: state
                .module_artifact_bids
                .get(hash)
                .map(|v| (hash.clone(), v.clone()))
                .into_iter()
                .collect(),
            next_module_market_order_id: state.next_module_market_order_id,
            next_module_market_sale_id: state.next_module_market_sale_id,
            world_materials: state
                .material_ledgers
                .get(&MaterialLedgerId::world())
                .filter(|v| !v.is_empty())
                .unwrap_or(&state.materials)
                .clone(),
        }
    }

    pub(crate) fn matches_event(&self, event: &DomainEvent) -> bool {
        self.events.as_slice() == std::slice::from_ref(event)
    }

    pub(crate) fn apply_event(
        &mut self,
        state: &WorldState,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<(), WorldError> {
        let (hash, ids): (&str, Vec<&String>) = match event {
            DomainEvent::ModuleArtifactListed {
                wasm_hash,
                seller_agent_id,
                ..
            } => (wasm_hash, vec![seller_agent_id]),
            DomainEvent::ModuleArtifactBidPlaced {
                wasm_hash,
                bidder_agent_id,
                ..
            } => (wasm_hash, vec![bidder_agent_id]),
            DomainEvent::ModuleArtifactSaleCompleted {
                wasm_hash,
                seller_agent_id,
                buyer_agent_id,
                ..
            } => (wasm_hash, vec![seller_agent_id, buyer_agent_id]),
            DomainEvent::ModuleArtifactDeployed {
                wasm_hash,
                publisher_agent_id,
                ..
            } => (wasm_hash, vec![publisher_agent_id]),
            DomainEvent::ModuleArtifactDelisted {
                wasm_hash,
                seller_agent_id,
                ..
            } => (wasm_hash, vec![seller_agent_id]),
            DomainEvent::ModuleArtifactDestroyed {
                wasm_hash,
                owner_agent_id,
                ..
            } => (wasm_hash, vec![owner_agent_id]),
            DomainEvent::ModuleArtifactBidCancelled {
                wasm_hash,
                bidder_agent_id,
                ..
            } => (wasm_hash, vec![bidder_agent_id]),
            _ => unreachable!(),
        };
        assert_eq!(
            hash, self.hash,
            "marketplace batch must retain its artifact identity"
        );
        for id in ids {
            if !self.agents.contains_key(id)
                && let Some(cell) = state.agents.get(id)
            {
                self.agents.insert(id.clone(), cell.clone());
            }
        }
        self.apply_inner(event, now)?;
        self.events.push(event.clone());
        Ok(())
    }

    fn settle_module_action_fee(
        &mut self,
        agent_id: &str,
        fee_kind: ResourceKind,
        fee_amount: i64,
        now: WorldTime,
    ) -> Result<(), WorldError> {
        if fee_amount < 0 {
            return Err(WorldError::ResourceBalanceInvalid {
                reason: format!("module action fee must be >= 0, got {}", fee_amount),
            });
        }
        let cell = self
            .agents
            .get_mut(agent_id)
            .ok_or_else(|| WorldError::AgentNotFound {
                agent_id: agent_id.to_string(),
            })?;
        if fee_amount > 0 {
            cell.state
                .resources
                .remove(fee_kind, fee_amount)
                .map_err(|err| WorldError::ResourceBalanceInvalid {
                    reason: format!(
                        "module action fee debit failed: agent={} kind={:?} amount={} err={:?}",
                        agent_id, fee_kind, fee_amount, err
                    ),
                })?;
            let balance = self
                .resources
                .get(&fee_kind)
                .copied()
                .unwrap_or(0)
                .saturating_add(fee_amount);
            self.resources.insert(fee_kind, balance);
        }
        cell.last_active = now;
        Ok(())
    }

    fn apply_inner(&mut self, event: &DomainEvent, now: WorldTime) -> Result<(), WorldError> {
        match event {
            DomainEvent::ModuleArtifactDeployed {
                publisher_agent_id,
                wasm_hash,
                fee_kind,
                fee_amount,
                ..
            } => {
                self.settle_module_action_fee(
                    publisher_agent_id.as_str(),
                    *fee_kind,
                    *fee_amount,
                    now,
                )?;
                self.module_artifact_owners
                    .insert(wasm_hash.clone(), publisher_agent_id.clone());
                self.module_artifact_listings.remove(wasm_hash);
                self.module_artifact_bids.remove(wasm_hash);
            }
            DomainEvent::ModuleArtifactListed {
                seller_agent_id,
                wasm_hash,
                price_kind,
                price_amount,
                order_id,
                fee_kind,
                fee_amount,
            } => {
                if *price_amount <= 0 {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "module artifact listing price must be > 0, got {}",
                            price_amount
                        ),
                    });
                }
                let owner = self.module_artifact_owners.get(wasm_hash).ok_or_else(|| {
                    WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "module artifact owner missing for listing hash {}",
                            wasm_hash
                        ),
                    }
                })?;
                if owner != seller_agent_id {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "module artifact listing seller mismatch: hash={} owner={} seller={}",
                            wasm_hash, owner, seller_agent_id
                        ),
                    });
                }
                self.settle_module_action_fee(
                    seller_agent_id.as_str(),
                    *fee_kind,
                    *fee_amount,
                    now,
                )?;
                self.module_artifact_listings.insert(
                    wasm_hash.clone(),
                    ModuleArtifactListingState {
                        order_id: *order_id,
                        seller_agent_id: seller_agent_id.clone(),
                        price_kind: *price_kind,
                        price_amount: *price_amount,
                        listed_at: now,
                    },
                );
                if *order_id > 0 {
                    self.next_module_market_order_id = self
                        .next_module_market_order_id
                        .max(order_id.saturating_add(1));
                }
            }
            DomainEvent::ModuleArtifactDelisted {
                seller_agent_id,
                wasm_hash,
                order_id,
                fee_kind,
                fee_amount,
            } => {
                let listing = self
                    .module_artifact_listings
                    .get(wasm_hash)
                    .ok_or_else(|| WorldError::ResourceBalanceInvalid {
                        reason: format!("module artifact listing missing for hash {}", wasm_hash),
                    })?;
                if listing.seller_agent_id != *seller_agent_id {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "module artifact delist seller mismatch: hash={} listing_seller={} event_seller={}",
                            wasm_hash, listing.seller_agent_id, seller_agent_id
                        ),
                    });
                }
                if let Some(expected_order_id) = order_id
                    && listing.order_id != *expected_order_id
                {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "module artifact delist order mismatch: hash={} listing_order_id={} event_order_id={}",
                            wasm_hash, listing.order_id, expected_order_id
                        ),
                    });
                }
                let owner = self.module_artifact_owners.get(wasm_hash).ok_or_else(|| {
                    WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "module artifact owner missing for delist hash {}",
                            wasm_hash
                        ),
                    }
                })?;
                if owner != seller_agent_id {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "module artifact delist seller is not owner: hash={} owner={} seller={}",
                            wasm_hash, owner, seller_agent_id
                        ),
                    });
                }
                self.settle_module_action_fee(
                    seller_agent_id.as_str(),
                    *fee_kind,
                    *fee_amount,
                    now,
                )?;
                self.module_artifact_listings.remove(wasm_hash);
            }
            DomainEvent::ModuleArtifactDestroyed {
                owner_agent_id,
                wasm_hash,
                reason,
                fee_kind,
                fee_amount,
            } => {
                if reason.trim().is_empty() {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "module artifact destroy reason cannot be empty for hash {}",
                            wasm_hash
                        ),
                    });
                }
                let owner = self.module_artifact_owners.get(wasm_hash).ok_or_else(|| {
                    WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "module artifact owner missing for destroy hash {}",
                            wasm_hash
                        ),
                    }
                })?;
                if owner != owner_agent_id {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "module artifact destroy owner mismatch: hash={} owner={} event_owner={}",
                            wasm_hash, owner, owner_agent_id
                        ),
                    });
                }
                self.settle_module_action_fee(
                    owner_agent_id.as_str(),
                    *fee_kind,
                    *fee_amount,
                    now,
                )?;
                self.module_artifact_owners.remove(wasm_hash);
                self.module_artifact_listings.remove(wasm_hash);
                self.module_artifact_bids.remove(wasm_hash);
            }
            DomainEvent::ModuleArtifactBidPlaced {
                bidder_agent_id,
                wasm_hash,
                order_id,
                price_kind,
                price_amount,
            } => {
                if *order_id == 0 {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "module artifact bid order_id must be > 0 for hash {}",
                            wasm_hash
                        ),
                    });
                }
                if *price_amount <= 0 {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "module artifact bid price must be > 0, got {}",
                            price_amount
                        ),
                    });
                }
                if !self.agents.contains_key(bidder_agent_id) {
                    return Err(WorldError::AgentNotFound {
                        agent_id: bidder_agent_id.clone(),
                    });
                }
                self.next_module_market_order_id = self
                    .next_module_market_order_id
                    .max(order_id.saturating_add(1));
                self.module_artifact_bids
                    .entry(wasm_hash.clone())
                    .or_default()
                    .push(ModuleArtifactBidState {
                        order_id: *order_id,
                        bidder_agent_id: bidder_agent_id.clone(),
                        price_kind: *price_kind,
                        price_amount: *price_amount,
                        bid_at: now,
                    });
                if let Some(cell) = self.agents.get_mut(bidder_agent_id) {
                    cell.last_active = now;
                }
            }
            DomainEvent::ModuleArtifactBidCancelled {
                bidder_agent_id,
                wasm_hash,
                order_id,
                ..
            } => {
                let remove_empty_entry = {
                    let bids = self
                        .module_artifact_bids
                        .get_mut(wasm_hash)
                        .ok_or_else(|| WorldError::ResourceBalanceInvalid {
                            reason: format!("module artifact bids missing for hash {}", wasm_hash),
                        })?;
                    let before = bids.len();
                    bids.retain(|entry| {
                        !(entry.order_id == *order_id && entry.bidder_agent_id == *bidder_agent_id)
                    });
                    if before == bids.len() {
                        return Err(WorldError::ResourceBalanceInvalid {
                            reason: format!(
                                "module artifact bid cancel target not found: hash={} order_id={} bidder={}",
                                wasm_hash, order_id, bidder_agent_id
                            ),
                        });
                    }
                    bids.is_empty()
                };
                if remove_empty_entry {
                    self.module_artifact_bids.remove(wasm_hash);
                }
                if let Some(cell) = self.agents.get_mut(bidder_agent_id) {
                    cell.last_active = now;
                }
            }
            DomainEvent::ModuleArtifactSaleCompleted {
                buyer_agent_id,
                seller_agent_id,
                wasm_hash,
                price_kind,
                price_amount,
                sale_id,
                listing_order_id,
                bid_order_id,
            } => {
                if buyer_agent_id == seller_agent_id {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "module artifact buyer and seller cannot be the same: {}",
                            buyer_agent_id
                        ),
                    });
                }
                if *price_amount <= 0 {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "module artifact sale price must be > 0, got {}",
                            price_amount
                        ),
                    });
                }

                let listing = self
                    .module_artifact_listings
                    .get(wasm_hash)
                    .ok_or_else(|| WorldError::ResourceBalanceInvalid {
                        reason: format!("module artifact listing missing for hash {}", wasm_hash),
                    })?;
                if listing.seller_agent_id != *seller_agent_id
                    || listing.price_kind != *price_kind
                    || listing.price_amount != *price_amount
                {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!("module artifact listing mismatch for hash {}", wasm_hash),
                    });
                }
                if let Some(expected_listing_order_id) = listing_order_id
                    && listing.order_id != *expected_listing_order_id
                {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "module artifact sale listing order mismatch: hash={} listing_order_id={} event_order_id={}",
                            wasm_hash, listing.order_id, expected_listing_order_id
                        ),
                    });
                }
                let owner = self.module_artifact_owners.get(wasm_hash).ok_or_else(|| {
                    WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "module artifact owner missing for sale hash {}",
                            wasm_hash
                        ),
                    }
                })?;
                if owner != seller_agent_id {
                    return Err(WorldError::ResourceBalanceInvalid {
                        reason: format!(
                            "module artifact sale seller is not owner: hash={} owner={} seller={}",
                            wasm_hash, owner, seller_agent_id
                        ),
                    });
                }

                let mut seller = self.agents.remove(seller_agent_id).ok_or_else(|| {
                    WorldError::AgentNotFound {
                        agent_id: seller_agent_id.clone(),
                    }
                })?;
                let mut buyer = self.agents.remove(buyer_agent_id).ok_or_else(|| {
                    WorldError::AgentNotFound {
                        agent_id: buyer_agent_id.clone(),
                    }
                })?;

                buyer
                    .state
                    .resources
                    .remove(*price_kind, *price_amount)
                    .map_err(|err| WorldError::ResourceBalanceInvalid {
                        reason: format!("module artifact sale buyer debit failed: {err:?}"),
                    })?;
                seller
                    .state
                    .resources
                    .add(*price_kind, *price_amount)
                    .map_err(|err| WorldError::ResourceBalanceInvalid {
                        reason: format!("module artifact sale seller credit failed: {err:?}"),
                    })?;
                seller.last_active = now;
                buyer.last_active = now;

                self.agents.insert(seller_agent_id.clone(), seller);
                self.agents.insert(buyer_agent_id.clone(), buyer);
                self.module_artifact_owners
                    .insert(wasm_hash.clone(), buyer_agent_id.clone());
                self.module_artifact_listings.remove(wasm_hash);
                if let Some(expected_bid_order_id) = bid_order_id {
                    let remove_empty_entry = {
                        let bids =
                            self.module_artifact_bids
                                .get_mut(wasm_hash)
                                .ok_or_else(|| WorldError::ResourceBalanceInvalid {
                                    reason: format!(
                                        "module artifact sale bid missing for hash {} order_id {}",
                                        wasm_hash, expected_bid_order_id
                                    ),
                                })?;
                        let before = bids.len();
                        bids.retain(|entry| {
                            !(entry.order_id == *expected_bid_order_id
                                && entry.bidder_agent_id == *buyer_agent_id)
                        });
                        if before == bids.len() {
                            return Err(WorldError::ResourceBalanceInvalid {
                                reason: format!(
                                    "module artifact sale bid not found: hash={} order_id={} buyer={}",
                                    wasm_hash, expected_bid_order_id, buyer_agent_id
                                ),
                            });
                        }
                        bids.is_empty()
                    };
                    if remove_empty_entry {
                        self.module_artifact_bids.remove(wasm_hash);
                    }
                }
                if *sale_id > 0 {
                    self.next_module_market_sale_id = self
                        .next_module_market_sale_id
                        .max(sale_id.saturating_add(1));
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    pub(crate) fn matching_sale(&self, state: &WorldState) -> Option<DomainEvent> {
        let listing = self.module_artifact_listings.get(&self.hash)?;
        let mut best: Option<&ModuleArtifactBidState> = None;
        for bid in self.module_artifact_bids.get(&self.hash)? {
            if bid.bidder_agent_id == listing.seller_agent_id
                || bid.price_kind != listing.price_kind
                || bid.price_amount < listing.price_amount
            {
                continue;
            }
            let available = self
                .agents
                .get(&bid.bidder_agent_id)
                .or_else(|| state.agents.get(&bid.bidder_agent_id))
                .map(|cell| cell.state.resources.get(listing.price_kind))
                .unwrap_or(0);
            if available < listing.price_amount {
                continue;
            }
            if best.is_none_or(|current| {
                bid.price_amount > current.price_amount
                    || (bid.price_amount == current.price_amount && bid.order_id < current.order_id)
            }) {
                best = Some(bid);
            }
        }
        let bid = best?;
        Some(DomainEvent::ModuleArtifactSaleCompleted {
            buyer_agent_id: bid.bidder_agent_id.clone(),
            seller_agent_id: listing.seller_agent_id.clone(),
            wasm_hash: self.hash.clone(),
            price_kind: listing.price_kind,
            price_amount: listing.price_amount,
            sale_id: self.next_module_market_sale_id.max(1),
            listing_order_id: (listing.order_id > 0).then_some(listing.order_id),
            bid_order_id: Some(bid.order_id),
        })
    }

    pub(crate) fn routed_agents(&self) -> BTreeMap<String, AgentCell> {
        let mut agents = self.agents.clone();
        for event in &self.events {
            if let Some(id) = event.agent_id()
                && let Some(cell) = agents.get_mut(id)
            {
                cell.mailbox.push_back(event.clone());
            }
        }
        agents
    }

    pub(crate) fn install_routed(mut self, state: &mut WorldState) {
        self.agents = self.routed_agents();
        self.install_infallible(state);
    }

    pub(crate) fn install_infallible(self, state: &mut WorldState) {
        state.agents.extend(self.agents);
        state.resources.extend(self.resources);
        replace(
            &mut state.module_artifact_owners,
            &self.hash,
            self.module_artifact_owners,
        );
        replace(
            &mut state.module_artifact_listings,
            &self.hash,
            self.module_artifact_listings,
        );
        replace(
            &mut state.module_artifact_bids,
            &self.hash,
            self.module_artifact_bids,
        );
        state.next_module_market_order_id = self.next_module_market_order_id;
        state.next_module_market_sale_id = self.next_module_market_sale_id;
        state.materials = self.world_materials.clone();
        state
            .material_ledgers
            .insert(MaterialLedgerId::world(), self.world_materials);
    }

    pub(crate) fn serialize_market_fields<S: serde::ser::SerializeStruct>(
        &self,
        state: &WorldState,
        output: &mut S,
    ) -> Result<(), S::Error> {
        output.serialize_field(
            "module_artifact_owners",
            &MarketEntryProjection {
                base: &state.module_artifact_owners,
                key: &self.hash,
                value: self.module_artifact_owners.get(&self.hash),
            },
        )?;
        output.serialize_field(
            "module_artifact_listings",
            &MarketEntryProjection {
                base: &state.module_artifact_listings,
                key: &self.hash,
                value: self.module_artifact_listings.get(&self.hash),
            },
        )?;
        output.serialize_field(
            "module_artifact_bids",
            &MarketEntryProjection {
                base: &state.module_artifact_bids,
                key: &self.hash,
                value: self.module_artifact_bids.get(&self.hash),
            },
        )
    }

    pub(crate) fn serialize_counter_fields<S: serde::ser::SerializeStruct>(
        &self,
        output: &mut S,
    ) -> Result<(), S::Error> {
        output.serialize_field(
            "next_module_market_order_id",
            &self.next_module_market_order_id,
        )?;
        output.serialize_field(
            "next_module_market_sale_id",
            &self.next_module_market_sale_id,
        )
    }

    pub(crate) fn serialize_material_fields<S: serde::ser::SerializeStruct>(
        &self,
        state: &WorldState,
        output: &mut S,
    ) -> Result<(), S::Error> {
        output.serialize_field("materials", &self.world_materials)?;
        output.serialize_field(
            "material_ledgers",
            &MarketEntryProjection {
                base: &state.material_ledgers,
                key: &MaterialLedgerId::world(),
                value: Some(&self.world_materials),
            },
        )
    }
}

fn replace<V>(base: &mut BTreeMap<String, V>, key: &str, updates: BTreeMap<String, V>) {
    base.remove(key);
    base.extend(updates);
}

struct MarketEntryProjection<'a, K, V> {
    base: &'a BTreeMap<K, V>,
    key: &'a K,
    value: Option<&'a V>,
}
impl<K: Ord + Serialize, V: Serialize> Serialize for MarketEntryProjection<'_, K, V> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let len = self.base.len() - usize::from(self.base.contains_key(self.key))
            + usize::from(self.value.is_some());
        let mut map = serializer.serialize_map(Some(len))?;
        let mut inserted = false;
        for (key, value) in self.base {
            if !inserted && key >= self.key {
                if let Some(replacement) = self.value {
                    map.serialize_entry(self.key, replacement)?;
                }
                inserted = true;
            }
            if key != self.key {
                map.serialize_entry(key, value)?;
            }
        }
        if !inserted && let Some(value) = self.value {
            map.serialize_entry(self.key, value)?;
        }
        map.end()
    }
}
impl WorldState {
    pub(crate) fn prepare_module_marketplace_event(
        &self,
        event: &DomainEvent,
        now: WorldTime,
    ) -> Result<PreparedModuleMarketplace, WorldError> {
        let mut prepared = PreparedModuleMarketplace::new(self, event);
        prepared.apply_event(self, event, now)?;
        Ok(prepared)
    }
}
