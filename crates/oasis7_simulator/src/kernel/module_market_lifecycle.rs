use super::super::types::{ResourceKind, ResourceOwner};
use super::super::world_model::{ModuleArtifactBidState, ModuleArtifactListingState};
use super::WorldKernel;
use super::types::{RejectReason, WorldEventKind};

impl WorldKernel {
    pub(super) fn apply_list_module_artifact_for_sale(
        &mut self,
        seller_agent_id: String,
        wasm_hash: String,
        price_kind: ResourceKind,
        price_amount: i64,
    ) -> WorldEventKind {
        if !self.model.agents.contains_key(&seller_agent_id) {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::AgentNotFound {
                    agent_id: seller_agent_id,
                },
            };
        }
        if price_amount <= 0 {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::InvalidAmount {
                    amount: price_amount,
                },
            };
        }
        let Some(owner_agent_id) = self.module_artifact_owner_agent_id(wasm_hash.as_str()) else {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "list module artifact rejected: missing artifact {}",
                        wasm_hash
                    )],
                },
            };
        };
        if owner_agent_id != seller_agent_id {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "list module artifact rejected: seller {} does not own {} (owner {})",
                        seller_agent_id, wasm_hash, owner_agent_id
                    )],
                },
            };
        }

        let order_id = self.allocate_module_market_order_id();
        let listing = ModuleArtifactListingState {
            order_id,
            wasm_hash: wasm_hash.clone(),
            seller_agent_id: seller_agent_id.clone(),
            price_kind,
            price_amount,
            listed_at_tick: self.time,
        };

        if let Some(best_bid) = self.best_bid_for_listing(wasm_hash.as_str(), &listing) {
            let sale_id = self.next_module_market_sale_id();
            if let Err(reason) = self.settle_module_artifact_sale(
                best_bid.bidder_agent_id.as_str(),
                seller_agent_id.as_str(),
                wasm_hash.as_str(),
                price_kind,
                price_amount,
                sale_id,
                Some(order_id),
                Some(best_bid.order_id),
            ) {
                return WorldEventKind::ActionRejected { reason };
            }
            return WorldEventKind::ModuleArtifactSaleCompleted {
                buyer_agent_id: best_bid.bidder_agent_id,
                seller_agent_id,
                wasm_hash,
                price_kind,
                price_amount,
                sale_id,
                listing_order_id: Some(order_id),
                bid_order_id: Some(best_bid.order_id),
            };
        }

        self.model
            .module_artifact_listings
            .insert(wasm_hash.clone(), listing);
        WorldEventKind::ModuleArtifactListed {
            seller_agent_id,
            wasm_hash,
            price_kind,
            price_amount,
            order_id,
        }
    }

    pub(super) fn apply_buy_module_artifact(
        &mut self,
        buyer_agent_id: String,
        wasm_hash: String,
    ) -> WorldEventKind {
        if !self.model.agents.contains_key(&buyer_agent_id) {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::AgentNotFound {
                    agent_id: buyer_agent_id,
                },
            };
        }
        let Some(listing) = self.model.module_artifact_listings.get(&wasm_hash).cloned() else {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "buy module artifact rejected: listing missing for {}",
                        wasm_hash
                    )],
                },
            };
        };
        if listing.seller_agent_id == buyer_agent_id {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "buy module artifact rejected: buyer {} already owns listing {}",
                        buyer_agent_id, wasm_hash
                    )],
                },
            };
        }

        let sale_id = self.next_module_market_sale_id();
        if let Err(reason) = self.settle_module_artifact_sale(
            buyer_agent_id.as_str(),
            listing.seller_agent_id.as_str(),
            wasm_hash.as_str(),
            listing.price_kind,
            listing.price_amount,
            sale_id,
            Some(listing.order_id),
            None,
        ) {
            return WorldEventKind::ActionRejected { reason };
        }

        WorldEventKind::ModuleArtifactSaleCompleted {
            buyer_agent_id,
            seller_agent_id: listing.seller_agent_id,
            wasm_hash,
            price_kind: listing.price_kind,
            price_amount: listing.price_amount,
            sale_id,
            listing_order_id: Some(listing.order_id),
            bid_order_id: None,
        }
    }

    pub(super) fn apply_delist_module_artifact(
        &mut self,
        seller_agent_id: String,
        wasm_hash: String,
    ) -> WorldEventKind {
        if !self.model.agents.contains_key(&seller_agent_id) {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::AgentNotFound {
                    agent_id: seller_agent_id,
                },
            };
        }

        let Some(listing) = self.model.module_artifact_listings.get(&wasm_hash).cloned() else {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "delist module artifact rejected: listing missing for {}",
                        wasm_hash
                    )],
                },
            };
        };
        if listing.seller_agent_id != seller_agent_id {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "delist module artifact rejected: seller {} does not own listing {}",
                        seller_agent_id, wasm_hash
                    )],
                },
            };
        }

        let Some(owner_agent_id) = self.module_artifact_owner_agent_id(wasm_hash.as_str()) else {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "delist module artifact rejected: missing artifact {}",
                        wasm_hash
                    )],
                },
            };
        };
        if owner_agent_id != seller_agent_id {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "delist module artifact rejected: seller {} does not own {} (owner {})",
                        seller_agent_id, wasm_hash, owner_agent_id
                    )],
                },
            };
        }

        self.model.module_artifact_listings.remove(&wasm_hash);
        WorldEventKind::ModuleArtifactDelisted {
            seller_agent_id,
            wasm_hash,
            order_id: Some(listing.order_id),
        }
    }

    pub(super) fn apply_destroy_module_artifact(
        &mut self,
        owner_agent_id: String,
        wasm_hash: String,
        reason: String,
    ) -> WorldEventKind {
        if !self.model.agents.contains_key(&owner_agent_id) {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::AgentNotFound {
                    agent_id: owner_agent_id,
                },
            };
        }
        if reason.trim().is_empty() {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec!["destroy module artifact rejected: reason is empty".to_string()],
                },
            };
        }

        let Some(current_owner_agent_id) = self.module_artifact_owner_agent_id(wasm_hash.as_str())
        else {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "destroy module artifact rejected: missing artifact {}",
                        wasm_hash
                    )],
                },
            };
        };
        if current_owner_agent_id != owner_agent_id {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "destroy module artifact rejected: owner {} does not own {} (owner {})",
                        owner_agent_id, wasm_hash, current_owner_agent_id
                    )],
                },
            };
        }
        if self.has_active_installed_module_using_artifact(wasm_hash.as_str()) {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "destroy module artifact rejected: artifact {} is used by active module",
                        wasm_hash
                    )],
                },
            };
        }

        self.model.module_artifacts.remove(&wasm_hash);
        self.model.module_artifact_listings.remove(&wasm_hash);
        self.model.module_artifact_bids.remove(&wasm_hash);
        self.model
            .installed_modules
            .retain(|_, installed| installed.wasm_hash != wasm_hash);

        WorldEventKind::ModuleArtifactDestroyed {
            owner_agent_id,
            wasm_hash,
            reason,
        }
    }

    pub(super) fn apply_place_module_artifact_bid(
        &mut self,
        bidder_agent_id: String,
        wasm_hash: String,
        price_kind: ResourceKind,
        price_amount: i64,
    ) -> WorldEventKind {
        if !self.model.agents.contains_key(&bidder_agent_id) {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::AgentNotFound {
                    agent_id: bidder_agent_id,
                },
            };
        }
        if price_amount <= 0 {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::InvalidAmount {
                    amount: price_amount,
                },
            };
        }

        let Some(owner_agent_id) = self.module_artifact_owner_agent_id(wasm_hash.as_str()) else {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "place module artifact bid rejected: missing artifact {}",
                        wasm_hash
                    )],
                },
            };
        };
        if owner_agent_id == bidder_agent_id {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::RuleDenied {
                    notes: vec![format!(
                        "place module artifact bid rejected: bidder {} already owns {}",
                        bidder_agent_id, wasm_hash
                    )],
                },
            };
        }

        let bidder_available = self
            .owner_stock(&ResourceOwner::Agent {
                agent_id: bidder_agent_id.clone(),
            })
            .map(|stock| stock.get(price_kind))
            .unwrap_or(0);
        if bidder_available < price_amount {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::InsufficientResource {
                    owner: ResourceOwner::Agent {
                        agent_id: bidder_agent_id,
                    },
                    kind: price_kind,
                    requested: price_amount,
                    available: bidder_available,
                },
            };
        }

        let order_id = self.allocate_module_market_order_id();
        if let Some(listing) = self.model.module_artifact_listings.get(&wasm_hash).cloned() {
            let listing_is_matchable = listing.price_kind == price_kind
                && price_amount >= listing.price_amount
                && listing.seller_agent_id != bidder_agent_id
                && self.model.agents.contains_key(&listing.seller_agent_id)
                && self
                    .module_artifact_owner_agent_id(wasm_hash.as_str())
                    .is_some_and(|owner| owner == listing.seller_agent_id);

            if listing_is_matchable {
                let sale_id = self.next_module_market_sale_id();
                if let Err(reason) = self.settle_module_artifact_sale(
                    bidder_agent_id.as_str(),
                    listing.seller_agent_id.as_str(),
                    wasm_hash.as_str(),
                    listing.price_kind,
                    listing.price_amount,
                    sale_id,
                    Some(listing.order_id),
                    Some(order_id),
                ) {
                    return WorldEventKind::ActionRejected { reason };
                }
                return WorldEventKind::ModuleArtifactSaleCompleted {
                    buyer_agent_id: bidder_agent_id,
                    seller_agent_id: listing.seller_agent_id,
                    wasm_hash,
                    price_kind: listing.price_kind,
                    price_amount: listing.price_amount,
                    sale_id,
                    listing_order_id: Some(listing.order_id),
                    bid_order_id: Some(order_id),
                };
            }
        }

        self.model
            .module_artifact_bids
            .entry(wasm_hash.clone())
            .or_default()
            .push(ModuleArtifactBidState {
                order_id,
                wasm_hash: wasm_hash.clone(),
                bidder_agent_id: bidder_agent_id.clone(),
                price_kind,
                price_amount,
                placed_at_tick: self.time,
            });
        WorldEventKind::ModuleArtifactBidPlaced {
            bidder_agent_id,
            wasm_hash,
            order_id,
            price_kind,
            price_amount,
        }
    }

    pub(super) fn apply_cancel_module_artifact_bid(
        &mut self,
        bidder_agent_id: String,
        wasm_hash: String,
        bid_order_id: u64,
    ) -> WorldEventKind {
        if !self.model.agents.contains_key(&bidder_agent_id) {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::AgentNotFound {
                    agent_id: bidder_agent_id,
                },
            };
        }
        if bid_order_id == 0 {
            return WorldEventKind::ActionRejected {
                reason: RejectReason::InvalidAmount { amount: 0 },
            };
        }

        let remove_empty = {
            let Some(bids) = self.model.module_artifact_bids.get_mut(&wasm_hash) else {
                return WorldEventKind::ActionRejected {
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "cancel module artifact bid rejected: no bids for {}",
                            wasm_hash
                        )],
                    },
                };
            };
            let before = bids.len();
            bids.retain(|entry| {
                !(entry.order_id == bid_order_id && entry.bidder_agent_id == bidder_agent_id)
            });
            if before == bids.len() {
                return WorldEventKind::ActionRejected {
                    reason: RejectReason::RuleDenied {
                        notes: vec![format!(
                            "cancel module artifact bid rejected: bid {} not found for {}",
                            bid_order_id, bidder_agent_id
                        )],
                    },
                };
            }
            bids.is_empty()
        };
        if remove_empty {
            self.model.module_artifact_bids.remove(&wasm_hash);
        }

        WorldEventKind::ModuleArtifactBidCancelled {
            bidder_agent_id,
            wasm_hash,
            order_id: bid_order_id,
            reason: "cancelled_by_bidder".to_string(),
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Stable module-market settlement seam preserves deterministic sale inputs."
    )]
    fn settle_module_artifact_sale(
        &mut self,
        buyer_agent_id: &str,
        seller_agent_id: &str,
        wasm_hash: &str,
        price_kind: ResourceKind,
        price_amount: i64,
        sale_id: u64,
        listing_order_id: Option<u64>,
        bid_order_id: Option<u64>,
    ) -> Result<(), RejectReason> {
        if buyer_agent_id == seller_agent_id {
            return Err(RejectReason::RuleDenied {
                notes: vec![format!(
                    "module artifact sale rejected: buyer and seller are both {}",
                    buyer_agent_id
                )],
            });
        }
        if !self.model.agents.contains_key(buyer_agent_id) {
            return Err(RejectReason::AgentNotFound {
                agent_id: buyer_agent_id.to_string(),
            });
        }
        if !self.model.agents.contains_key(seller_agent_id) {
            return Err(RejectReason::AgentNotFound {
                agent_id: seller_agent_id.to_string(),
            });
        }
        let Some(owner_agent_id) = self.module_artifact_owner_agent_id(wasm_hash) else {
            return Err(RejectReason::RuleDenied {
                notes: vec![format!(
                    "module artifact sale rejected: missing artifact {}",
                    wasm_hash
                )],
            });
        };
        if owner_agent_id != seller_agent_id {
            return Err(RejectReason::RuleDenied {
                notes: vec![format!(
                    "module artifact sale rejected: seller {} does not own {} (owner {})",
                    seller_agent_id, wasm_hash, owner_agent_id
                )],
            });
        }

        if let Some(listing) = self.model.module_artifact_listings.get(wasm_hash) {
            if listing.seller_agent_id != seller_agent_id
                || listing.price_kind != price_kind
                || listing.price_amount != price_amount
            {
                return Err(RejectReason::RuleDenied {
                    notes: vec![format!(
                        "module artifact sale rejected: listing mismatch for {}",
                        wasm_hash
                    )],
                });
            }
            if let Some(expected_listing_order_id) = listing_order_id
                && listing.order_id != expected_listing_order_id
            {
                return Err(RejectReason::RuleDenied {
                    notes: vec![format!(
                        "module artifact sale rejected: listing order mismatch listing={} event={}",
                        listing.order_id, expected_listing_order_id
                    )],
                });
            }
        }

        let available = self
            .owner_stock(&ResourceOwner::Agent {
                agent_id: buyer_agent_id.to_string(),
            })
            .map(|stock| stock.get(price_kind))
            .unwrap_or(0);
        if available < price_amount {
            return Err(RejectReason::InsufficientResource {
                owner: ResourceOwner::Agent {
                    agent_id: buyer_agent_id.to_string(),
                },
                kind: price_kind,
                requested: price_amount,
                available,
            });
        }
        self.remove_from_owner(
            &ResourceOwner::Agent {
                agent_id: buyer_agent_id.to_string(),
            },
            price_kind,
            price_amount,
        )?;
        self.add_to_owner(
            &ResourceOwner::Agent {
                agent_id: seller_agent_id.to_string(),
            },
            price_kind,
            price_amount,
        )?;

        if let Some(order_id) = listing_order_id {
            self.model.next_module_market_order_id = self
                .model
                .next_module_market_order_id
                .max(order_id.saturating_add(1));
        }
        if let Some(order_id) = bid_order_id {
            self.model.next_module_market_order_id = self
                .model
                .next_module_market_order_id
                .max(order_id.saturating_add(1));
        }

        if let Some(order_id) = bid_order_id {
            let mut remove_empty = false;
            if let Some(bids) = self.model.module_artifact_bids.get_mut(wasm_hash) {
                let before = bids.len();
                bids.retain(|entry| {
                    !(entry.order_id == order_id && entry.bidder_agent_id == buyer_agent_id)
                });
                remove_empty = before != bids.len() && bids.is_empty();
            }
            if remove_empty {
                self.model.module_artifact_bids.remove(wasm_hash);
            }
        }

        self.model.module_artifact_listings.remove(wasm_hash);
        if let Some(artifact) = self.model.module_artifacts.get_mut(wasm_hash) {
            artifact.publisher_agent_id = buyer_agent_id.to_string();
        }

        if sale_id > 0 {
            self.model.next_module_market_sale_id = self
                .model
                .next_module_market_sale_id
                .max(sale_id.saturating_add(1));
        }
        Ok(())
    }

    fn module_artifact_owner_agent_id(&self, wasm_hash: &str) -> Option<&str> {
        self.model
            .module_artifacts
            .get(wasm_hash)
            .map(|artifact| artifact.publisher_agent_id.as_str())
    }

    pub(super) fn has_active_installed_module_using_artifact(&self, wasm_hash: &str) -> bool {
        self.model
            .installed_modules
            .values()
            .any(|installed| installed.active && installed.wasm_hash == wasm_hash)
    }

    fn allocate_module_market_order_id(&mut self) -> u64 {
        let order_id = self.next_module_market_order_id();
        self.model.next_module_market_order_id = order_id.saturating_add(1);
        order_id
    }

    fn next_module_market_order_id(&self) -> u64 {
        self.model.next_module_market_order_id.max(1)
    }

    fn next_module_market_sale_id(&self) -> u64 {
        self.model.next_module_market_sale_id.max(1)
    }

    fn best_bid_for_listing(
        &self,
        wasm_hash: &str,
        listing: &ModuleArtifactListingState,
    ) -> Option<ModuleArtifactBidState> {
        let bids = self.model.module_artifact_bids.get(wasm_hash)?;
        let mut best: Option<ModuleArtifactBidState> = None;
        for bid in bids {
            if bid.price_kind != listing.price_kind || bid.price_amount < listing.price_amount {
                continue;
            }
            if bid.bidder_agent_id == listing.seller_agent_id {
                continue;
            }
            if !self.model.agents.contains_key(&bid.bidder_agent_id) {
                continue;
            }
            let available = self
                .owner_stock(&ResourceOwner::Agent {
                    agent_id: bid.bidder_agent_id.clone(),
                })
                .map(|stock| stock.get(listing.price_kind))
                .unwrap_or(0);
            if available < listing.price_amount {
                continue;
            }

            let replace = match &best {
                Some(current) => {
                    bid.price_amount > current.price_amount
                        || (bid.price_amount == current.price_amount
                            && bid.order_id < current.order_id)
                }
                None => true,
            };
            if replace {
                best = Some(bid.clone());
            }
        }
        best
    }
}
