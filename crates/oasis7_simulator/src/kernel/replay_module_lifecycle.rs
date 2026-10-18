use super::super::types::{ModuleInstallTarget, ResourceKind, ResourceOwner};
use super::super::world_model::{
    InstalledModuleState, ModuleArtifactBidState, ModuleArtifactListingState, ModuleArtifactState,
};
use super::WorldKernel;
use super::types::WorldEventKind;
use sha2::{Digest, Sha256};

impl WorldKernel {
    pub(super) fn replay_module_lifecycle_event(
        &mut self,
        kind: &WorldEventKind,
    ) -> Option<Result<(), String>> {
        match kind {
            WorldEventKind::ModuleArtifactDeployed {
                publisher_agent_id,
                wasm_hash,
                wasm_bytes,
                bytes_len,
                module_id_hint,
            } => Some(self.replay_module_artifact_deployed(
                publisher_agent_id,
                wasm_hash,
                wasm_bytes,
                *bytes_len,
                module_id_hint.as_deref(),
            )),
            WorldEventKind::ModuleInstalled {
                installer_agent_id,
                module_id,
                module_version,
                wasm_hash,
                install_target,
                active,
            } => Some(self.replay_module_installed(
                installer_agent_id,
                module_id,
                module_version,
                wasm_hash,
                install_target,
                *active,
            )),
            WorldEventKind::ModuleArtifactListed {
                seller_agent_id,
                wasm_hash,
                price_kind,
                price_amount,
                order_id,
            } => Some(self.replay_module_artifact_listed(
                seller_agent_id,
                wasm_hash,
                *price_kind,
                *price_amount,
                *order_id,
            )),
            WorldEventKind::ModuleArtifactDelisted {
                seller_agent_id,
                wasm_hash,
                order_id,
            } => Some(self.replay_module_artifact_delisted(seller_agent_id, wasm_hash, *order_id)),
            WorldEventKind::ModuleArtifactBidPlaced {
                bidder_agent_id,
                wasm_hash,
                order_id,
                price_kind,
                price_amount,
            } => Some(self.replay_module_artifact_bid_placed(
                bidder_agent_id,
                wasm_hash,
                *order_id,
                *price_kind,
                *price_amount,
            )),
            WorldEventKind::ModuleArtifactBidCancelled {
                bidder_agent_id,
                wasm_hash,
                order_id,
                ..
            } => Some(self.replay_module_artifact_bid_cancelled(
                bidder_agent_id,
                wasm_hash,
                *order_id,
            )),
            WorldEventKind::ModuleArtifactSaleCompleted {
                buyer_agent_id,
                seller_agent_id,
                wasm_hash,
                price_kind,
                price_amount,
                sale_id,
                listing_order_id,
                bid_order_id,
            } => Some(self.replay_module_artifact_sale_completed(
                buyer_agent_id,
                seller_agent_id,
                wasm_hash,
                *price_kind,
                *price_amount,
                *sale_id,
                *listing_order_id,
                *bid_order_id,
            )),
            WorldEventKind::ModuleArtifactDestroyed {
                owner_agent_id,
                wasm_hash,
                reason,
            } => Some(self.replay_module_artifact_destroyed(owner_agent_id, wasm_hash, reason)),
            _ => None,
        }
    }

    pub(super) fn replay_module_artifact_deployed(
        &mut self,
        publisher_agent_id: &str,
        wasm_hash: &str,
        wasm_bytes: &[u8],
        bytes_len: u64,
        module_id_hint: Option<&str>,
    ) -> Result<(), String> {
        if !self.model.agents.contains_key(publisher_agent_id) {
            return Err(format!(
                "module artifact publisher not found: {}",
                publisher_agent_id
            ));
        }
        if wasm_hash.trim().is_empty() {
            return Err("module artifact hash cannot be empty".to_string());
        }
        if wasm_bytes.is_empty() {
            return Err("module artifact bytes cannot be empty".to_string());
        }
        if bytes_len != wasm_bytes.len() as u64 {
            return Err(format!(
                "module artifact bytes_len mismatch: expected {} got {}",
                bytes_len,
                wasm_bytes.len()
            ));
        }

        let computed_hash = sha256_hex(wasm_bytes);
        if computed_hash != wasm_hash {
            return Err(format!(
                "module artifact hash mismatch: expected {} got {}",
                wasm_hash, computed_hash
            ));
        }

        if let Some(existing) = self.model.module_artifacts.get(wasm_hash)
            && existing.wasm_bytes != wasm_bytes
        {
            return Err(format!(
                "module artifact hash {} already exists with different bytes",
                wasm_hash
            ));
        }

        self.model.module_artifacts.insert(
            wasm_hash.to_string(),
            ModuleArtifactState {
                wasm_hash: wasm_hash.to_string(),
                publisher_agent_id: publisher_agent_id.to_string(),
                module_id_hint: module_id_hint.map(|value| value.to_string()),
                wasm_bytes: wasm_bytes.to_vec(),
                deployed_at_tick: self.time,
            },
        );

        Ok(())
    }

    pub(super) fn replay_module_installed(
        &mut self,
        installer_agent_id: &str,
        module_id: &str,
        module_version: &str,
        wasm_hash: &str,
        install_target: &ModuleInstallTarget,
        active: bool,
    ) -> Result<(), String> {
        if !self.model.agents.contains_key(installer_agent_id) {
            return Err(format!(
                "module installer not found: {}",
                installer_agent_id
            ));
        }
        let installer_location_id = self
            .model
            .agents
            .get(installer_agent_id)
            .map(|agent| agent.location_id.clone())
            .unwrap_or_default();
        if module_id.trim().is_empty() {
            return Err("installed module_id cannot be empty".to_string());
        }
        if module_version.trim().is_empty() {
            return Err("installed module_version cannot be empty".to_string());
        }
        if wasm_hash.trim().is_empty() {
            return Err("installed module wasm_hash cannot be empty".to_string());
        }

        let Some(artifact) = self.model.module_artifacts.get(wasm_hash) else {
            return Err(format!(
                "installed module references missing artifact: {}",
                wasm_hash
            ));
        };
        if artifact.publisher_agent_id != installer_agent_id {
            return Err(format!(
                "installed module owner mismatch: installer {} artifact owner {}",
                installer_agent_id, artifact.publisher_agent_id
            ));
        }

        let install_target = match install_target {
            ModuleInstallTarget::SelfAgent => ModuleInstallTarget::SelfAgent,
            ModuleInstallTarget::LocationInfrastructure { location_id } => {
                let location_id = location_id.trim().to_string();
                if location_id.is_empty() {
                    return Err(
                        "installed module infrastructure target requires non-empty location_id"
                            .to_string(),
                    );
                }
                if !self.model.locations.contains_key(&location_id) {
                    return Err(format!(
                        "installed module target location not found: {}",
                        location_id
                    ));
                }
                if installer_location_id != location_id {
                    return Err(format!(
                        "installed module infrastructure location mismatch: installer={} at={} target={}",
                        installer_agent_id, installer_location_id, location_id
                    ));
                }
                ModuleInstallTarget::LocationInfrastructure { location_id }
            }
        };

        self.model.installed_modules.insert(
            module_id.to_string(),
            InstalledModuleState {
                module_id: module_id.to_string(),
                module_version: module_version.to_string(),
                wasm_hash: wasm_hash.to_string(),
                installer_agent_id: installer_agent_id.to_string(),
                install_target,
                active,
                installed_at_tick: self.time,
            },
        );

        Ok(())
    }

    pub(super) fn replay_module_artifact_listed(
        &mut self,
        seller_agent_id: &str,
        wasm_hash: &str,
        price_kind: ResourceKind,
        price_amount: i64,
        order_id: u64,
    ) -> Result<(), String> {
        if order_id == 0 {
            return Err("module artifact listing order_id must be > 0".to_string());
        }
        if price_amount <= 0 {
            return Err(format!(
                "module artifact listing price must be > 0, got {}",
                price_amount
            ));
        }
        if !self.model.agents.contains_key(seller_agent_id) {
            return Err(format!(
                "module artifact seller not found: {}",
                seller_agent_id
            ));
        }
        let Some(artifact) = self.model.module_artifacts.get(wasm_hash) else {
            return Err(format!(
                "module artifact listing missing artifact: {}",
                wasm_hash
            ));
        };
        if artifact.publisher_agent_id != seller_agent_id {
            return Err(format!(
                "module artifact listing owner mismatch: seller {} owner {}",
                seller_agent_id, artifact.publisher_agent_id
            ));
        }

        self.model.next_module_market_order_id = self
            .model
            .next_module_market_order_id
            .max(order_id.saturating_add(1));
        self.model.module_artifact_listings.insert(
            wasm_hash.to_string(),
            ModuleArtifactListingState {
                order_id,
                wasm_hash: wasm_hash.to_string(),
                seller_agent_id: seller_agent_id.to_string(),
                price_kind,
                price_amount,
                listed_at_tick: self.time,
            },
        );
        Ok(())
    }

    pub(super) fn replay_module_artifact_delisted(
        &mut self,
        seller_agent_id: &str,
        wasm_hash: &str,
        order_id: Option<u64>,
    ) -> Result<(), String> {
        let Some(listing) = self.model.module_artifact_listings.get(wasm_hash).cloned() else {
            return Err(format!(
                "module artifact delist missing listing for hash {}",
                wasm_hash
            ));
        };
        if listing.seller_agent_id != seller_agent_id {
            return Err(format!(
                "module artifact delist seller mismatch: listing seller {} event seller {}",
                listing.seller_agent_id, seller_agent_id
            ));
        }
        if let Some(expected_order_id) = order_id {
            if expected_order_id == 0 {
                return Err("module artifact delist order_id must be > 0".to_string());
            }
            if listing.order_id != expected_order_id {
                return Err(format!(
                    "module artifact delist order mismatch: listing={} event={}",
                    listing.order_id, expected_order_id
                ));
            }
        }

        self.model.module_artifact_listings.remove(wasm_hash);
        Ok(())
    }

    pub(super) fn replay_module_artifact_bid_placed(
        &mut self,
        bidder_agent_id: &str,
        wasm_hash: &str,
        order_id: u64,
        price_kind: ResourceKind,
        price_amount: i64,
    ) -> Result<(), String> {
        if order_id == 0 {
            return Err("module artifact bid order_id must be > 0".to_string());
        }
        if price_amount <= 0 {
            return Err(format!(
                "module artifact bid price must be > 0, got {}",
                price_amount
            ));
        }
        if !self.model.agents.contains_key(bidder_agent_id) {
            return Err(format!(
                "module artifact bidder not found: {}",
                bidder_agent_id
            ));
        }
        let Some(artifact) = self.model.module_artifacts.get(wasm_hash) else {
            return Err(format!(
                "module artifact bid missing artifact for hash {}",
                wasm_hash
            ));
        };
        if artifact.publisher_agent_id == bidder_agent_id {
            return Err(format!(
                "module artifact bid rejected: bidder {} already owns {}",
                bidder_agent_id, wasm_hash
            ));
        }
        let available = self
            .model
            .agents
            .get(bidder_agent_id)
            .map(|agent| agent.resources.get(price_kind))
            .unwrap_or(0);
        if available < price_amount {
            return Err(format!(
                "module artifact bid insufficient {:?}: requested {} available {}",
                price_kind, price_amount, available
            ));
        }

        self.model.next_module_market_order_id = self
            .model
            .next_module_market_order_id
            .max(order_id.saturating_add(1));
        self.model
            .module_artifact_bids
            .entry(wasm_hash.to_string())
            .or_default()
            .push(ModuleArtifactBidState {
                order_id,
                wasm_hash: wasm_hash.to_string(),
                bidder_agent_id: bidder_agent_id.to_string(),
                price_kind,
                price_amount,
                placed_at_tick: self.time,
            });
        Ok(())
    }

    pub(super) fn replay_module_artifact_bid_cancelled(
        &mut self,
        bidder_agent_id: &str,
        wasm_hash: &str,
        order_id: u64,
    ) -> Result<(), String> {
        if order_id == 0 {
            return Err("module artifact bid cancel order_id must be > 0".to_string());
        }
        let remove_empty = {
            let bids = self
                .model
                .module_artifact_bids
                .get_mut(wasm_hash)
                .ok_or_else(|| format!("module artifact bids missing for hash {}", wasm_hash))?;
            let before = bids.len();
            bids.retain(|entry| {
                !(entry.order_id == order_id && entry.bidder_agent_id == bidder_agent_id)
            });
            if before == bids.len() {
                return Err(format!(
                    "module artifact bid cancel target missing: hash={} order={} bidder={}",
                    wasm_hash, order_id, bidder_agent_id
                ));
            }
            bids.is_empty()
        };
        if remove_empty {
            self.model.module_artifact_bids.remove(wasm_hash);
        }
        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Stable replay seam preserves the serialized module-sale event fields."
    )]
    pub(super) fn replay_module_artifact_sale_completed(
        &mut self,
        buyer_agent_id: &str,
        seller_agent_id: &str,
        wasm_hash: &str,
        price_kind: ResourceKind,
        price_amount: i64,
        sale_id: u64,
        listing_order_id: Option<u64>,
        bid_order_id: Option<u64>,
    ) -> Result<(), String> {
        if buyer_agent_id == seller_agent_id {
            return Err(format!(
                "module artifact buyer and seller cannot match: {}",
                buyer_agent_id
            ));
        }
        if price_amount <= 0 {
            return Err(format!(
                "module artifact sale price must be > 0, got {}",
                price_amount
            ));
        }
        if !self.model.agents.contains_key(buyer_agent_id) {
            return Err(format!(
                "module artifact buyer not found: {}",
                buyer_agent_id
            ));
        }
        if !self.model.agents.contains_key(seller_agent_id) {
            return Err(format!(
                "module artifact seller not found: {}",
                seller_agent_id
            ));
        }

        let artifact = self.model.module_artifacts.get(wasm_hash).ok_or_else(|| {
            format!(
                "module artifact sale references missing artifact {}",
                wasm_hash
            )
        })?;
        if artifact.publisher_agent_id != seller_agent_id {
            return Err(format!(
                "module artifact sale seller mismatch: owner {} seller {}",
                artifact.publisher_agent_id, seller_agent_id
            ));
        }

        if let Some(listing) = self.model.module_artifact_listings.get(wasm_hash) {
            if listing.seller_agent_id != seller_agent_id
                || listing.price_kind != price_kind
                || listing.price_amount != price_amount
            {
                return Err(format!(
                    "module artifact sale listing mismatch for hash {}",
                    wasm_hash
                ));
            }
            if let Some(expected_listing_order_id) = listing_order_id {
                if expected_listing_order_id == 0 {
                    return Err("module artifact listing_order_id must be > 0".to_string());
                }
                if listing.order_id != expected_listing_order_id {
                    return Err(format!(
                        "module artifact sale listing order mismatch: listing={} event={}",
                        listing.order_id, expected_listing_order_id
                    ));
                }
            }
        }

        if let Some(order_id) = listing_order_id {
            if order_id == 0 {
                return Err("module artifact listing_order_id must be > 0".to_string());
            }
            self.model.next_module_market_order_id = self
                .model
                .next_module_market_order_id
                .max(order_id.saturating_add(1));
        }
        if let Some(order_id) = bid_order_id {
            if order_id == 0 {
                return Err("module artifact bid_order_id must be > 0".to_string());
            }
            self.model.next_module_market_order_id = self
                .model
                .next_module_market_order_id
                .max(order_id.saturating_add(1));
        }

        self.remove_from_owner_for_replay(
            &ResourceOwner::Agent {
                agent_id: buyer_agent_id.to_string(),
            },
            price_kind,
            price_amount,
        )
        .map_err(|err| format!("module artifact sale buyer debit failed: {err:?}"))?;
        self.add_to_owner_for_replay(
            &ResourceOwner::Agent {
                agent_id: seller_agent_id.to_string(),
            },
            price_kind,
            price_amount,
        )
        .map_err(|err| format!("module artifact sale seller credit failed: {err:?}"))?;

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

        let artifact = self
            .model
            .module_artifacts
            .get_mut(wasm_hash)
            .ok_or_else(|| {
                format!(
                    "module artifact sale missing mutable artifact after validation: {}",
                    wasm_hash
                )
            })?;
        artifact.publisher_agent_id = buyer_agent_id.to_string();

        if sale_id > 0 {
            self.model.next_module_market_sale_id = self
                .model
                .next_module_market_sale_id
                .max(sale_id.saturating_add(1));
        }

        Ok(())
    }

    pub(super) fn replay_module_artifact_destroyed(
        &mut self,
        owner_agent_id: &str,
        wasm_hash: &str,
        reason: &str,
    ) -> Result<(), String> {
        if reason.trim().is_empty() {
            return Err("module artifact destroy reason cannot be empty".to_string());
        }
        if !self.model.agents.contains_key(owner_agent_id) {
            return Err(format!(
                "module artifact owner not found: {}",
                owner_agent_id
            ));
        }
        let artifact = self.model.module_artifacts.get(wasm_hash).ok_or_else(|| {
            format!(
                "module artifact destroy missing artifact for hash {}",
                wasm_hash
            )
        })?;
        if artifact.publisher_agent_id != owner_agent_id {
            return Err(format!(
                "module artifact destroy owner mismatch: owner {} artifact owner {}",
                owner_agent_id, artifact.publisher_agent_id
            ));
        }
        if self.has_active_installed_module_using_artifact(wasm_hash) {
            return Err(format!(
                "module artifact destroy rejected: active module still uses {}",
                wasm_hash
            ));
        }

        self.model.module_artifacts.remove(wasm_hash);
        self.model.module_artifact_listings.remove(wasm_hash);
        self.model.module_artifact_bids.remove(wasm_hash);
        self.model
            .installed_modules
            .retain(|_, installed| installed.wasm_hash != wasm_hash);
        Ok(())
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}
