use super::*;

impl World {
    pub(in crate::world) fn try_apply_runtime_module_action(
        &mut self,
        envelope: &ActionEnvelope,
    ) -> Result<bool, WorldError> {
        let action_id = envelope.id;
        match &envelope.action {
            Action::CompileModuleArtifactFromSource {
                publisher_agent_id,
                module_id,
                source_package,
            } => {
                if !self.state.agents.contains_key(publisher_agent_id) {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::AgentNotFound {
                                agent_id: publisher_agent_id.clone(),
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
                if module_id.trim().is_empty() {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![
                                    "compile module source rejected: module_id is empty"
                                        .to_string(),
                                ],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
                if !self.release_security_policy.allow_runtime_source_compile {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![
                                    "compile module source rejected: runtime source compile is disabled by production release policy; use external Docker builder and deploy binary + receipt"
                                        .to_string(),
                                ],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }

                let source_bytes_len = source_package.files.values().map(Vec::len).sum::<usize>();
                let compiled_bytes = match self
                    .source_compiler
                    .compile(module_id.as_str(), source_package)
                {
                    Ok(bytes) => bytes,
                    Err(err) => {
                        self.append_event(
                            WorldEventBody::Domain(DomainEvent::ActionRejected {
                                action_id,
                                reason: RejectReason::RuleDenied {
                                    notes: vec![format!("compile module source rejected: {err:?}")],
                                },
                            }),
                            Some(CausedBy::Action(action_id)),
                        )?;
                        return Ok(true);
                    }
                };

                let wasm_hash = super::super::super::util::sha256_hex(&compiled_bytes);
                let fee_kind = ResourceKind::Electricity;
                let fee_amount =
                    Self::module_compile_fee_amount(source_bytes_len, compiled_bytes.len());
                if !self.ensure_module_fee_affordable(
                    action_id,
                    publisher_agent_id.as_str(),
                    fee_kind,
                    fee_amount,
                )? {
                    return Ok(true);
                }

                match self.prepare_module_artifact_registration(
                    wasm_hash.clone(),
                    compiled_bytes.as_slice(),
                ) {
                    Ok(registration) => {
                        self.append_module_artifact_deployment(
                            DomainEvent::ModuleArtifactDeployed {
                                publisher_agent_id: publisher_agent_id.clone(),
                                wasm_hash,
                                bytes_len: compiled_bytes.len() as u64,
                                fee_kind,
                                fee_amount,
                            },
                            Some(CausedBy::Action(action_id)),
                            registration,
                        )?;
                    }
                    Err(err) => {
                        self.append_event(
                            WorldEventBody::Domain(DomainEvent::ActionRejected {
                                action_id,
                                reason: RejectReason::RuleDenied {
                                    notes: vec![format!(
                                        "compile module source rejected: register artifact failed: {err:?}"
                                    )],
                                },
                            }),
                            Some(CausedBy::Action(action_id)),
                        )?;
                    }
                }
                Ok(true)
            }
            Action::DeployModuleArtifact {
                publisher_agent_id,
                wasm_hash,
                wasm_bytes,
            } => {
                if !self.state.agents.contains_key(publisher_agent_id) {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::AgentNotFound {
                                agent_id: publisher_agent_id.clone(),
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }

                let computed_hash = super::super::super::util::sha256_hex(wasm_bytes);
                if computed_hash != *wasm_hash {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "deploy module artifact rejected: artifact hash mismatch expected {} found {}",
                                    wasm_hash, computed_hash
                                )],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }

                let fee_kind = ResourceKind::Electricity;
                let fee_amount = Self::module_deploy_fee_amount(wasm_bytes.len());
                if !self.ensure_module_fee_affordable(
                    action_id,
                    publisher_agent_id.as_str(),
                    fee_kind,
                    fee_amount,
                )? {
                    return Ok(true);
                }

                match self
                    .prepare_module_artifact_registration(wasm_hash.clone(), wasm_bytes.as_slice())
                {
                    Ok(registration) => {
                        self.append_module_artifact_deployment(
                            DomainEvent::ModuleArtifactDeployed {
                                publisher_agent_id: publisher_agent_id.clone(),
                                wasm_hash: wasm_hash.clone(),
                                bytes_len: wasm_bytes.len() as u64,
                                fee_kind,
                                fee_amount,
                            },
                            Some(CausedBy::Action(action_id)),
                            registration,
                        )?;
                    }
                    Err(err) => {
                        self.append_event(
                            WorldEventBody::Domain(DomainEvent::ActionRejected {
                                action_id,
                                reason: RejectReason::RuleDenied {
                                    notes: vec![format!(
                                        "deploy module artifact rejected: {err:?}"
                                    )],
                                },
                            }),
                            Some(CausedBy::Action(action_id)),
                        )?;
                    }
                }

                Ok(true)
            }
            Action::InstallModuleFromArtifact {
                installer_agent_id,
                manifest,
                activate,
            } => self.apply_install_module_action(
                action_id,
                installer_agent_id.as_str(),
                manifest,
                *activate,
                ModuleInstallTarget::SelfAgent,
                None,
            ),
            Action::InstallModuleFromArtifactWithFinality {
                installer_agent_id,
                manifest,
                activate,
                finality_certificate,
            } => self.apply_install_module_action(
                action_id,
                installer_agent_id.as_str(),
                manifest,
                *activate,
                ModuleInstallTarget::SelfAgent,
                Some(finality_certificate),
            ),
            Action::InstallModuleToTargetFromArtifact {
                installer_agent_id,
                manifest,
                activate,
                install_target,
            } => self.apply_install_module_action(
                action_id,
                installer_agent_id.as_str(),
                manifest,
                *activate,
                install_target.clone(),
                None,
            ),
            Action::InstallModuleToTargetFromArtifactWithFinality {
                installer_agent_id,
                manifest,
                activate,
                install_target,
                finality_certificate,
            } => self.apply_install_module_action(
                action_id,
                installer_agent_id.as_str(),
                manifest,
                *activate,
                install_target.clone(),
                Some(finality_certificate),
            ),
            Action::UpgradeModuleFromArtifact {
                upgrader_agent_id,
                instance_id,
                from_module_version,
                manifest,
                activate,
            } => self.apply_upgrade_module_action(
                action_id,
                upgrader_agent_id.as_str(),
                instance_id.as_str(),
                from_module_version.as_str(),
                manifest,
                *activate,
                None,
            ),
            Action::UpgradeModuleFromArtifactWithFinality {
                upgrader_agent_id,
                instance_id,
                from_module_version,
                manifest,
                activate,
                finality_certificate,
            } => self.apply_upgrade_module_action(
                action_id,
                upgrader_agent_id.as_str(),
                instance_id.as_str(),
                from_module_version.as_str(),
                manifest,
                *activate,
                Some(finality_certificate),
            ),
            Action::RollbackModuleInstance {
                operator_agent_id,
                instance_id,
                target_module_version,
            } => self.apply_rollback_module_instance_action(
                action_id,
                operator_agent_id.as_str(),
                instance_id.as_str(),
                target_module_version.as_str(),
                None,
            ),
            Action::RollbackModuleInstanceWithFinality {
                operator_agent_id,
                instance_id,
                target_module_version,
                finality_certificate,
            } => self.apply_rollback_module_instance_action(
                action_id,
                operator_agent_id.as_str(),
                instance_id.as_str(),
                target_module_version.as_str(),
                Some(finality_certificate),
            ),
            action
                if matches!(
                    action,
                    Action::ModuleReleaseSubmit { .. }
                        | Action::ModuleReleaseShadow { .. }
                        | Action::ModuleReleaseSubmitAttestation { .. }
                        | Action::ModuleReleaseApproveRole { .. }
                        | Action::ModuleReleaseBindRoles { .. }
                        | Action::ModuleReleaseReject { .. }
                        | Action::ModuleReleaseApply { .. }
                        | Action::ModuleReleaseApplyWithFinality { .. }
                ) =>
            {
                Ok(self
                    .try_apply_module_release_action(action_id, action)?
                    .expect("release action must be handled"))
            }
            Action::ListModuleArtifactForSale {
                seller_agent_id,
                wasm_hash,
                price_kind,
                price_amount,
            } => {
                if !self.state.agents.contains_key(seller_agent_id) {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::AgentNotFound {
                                agent_id: seller_agent_id.clone(),
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
                if *price_amount <= 0 {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::InvalidAmount {
                                amount: *price_amount,
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
                if !self.module_artifacts.contains(wasm_hash) {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "list module artifact rejected: missing artifact {}",
                                    wasm_hash
                                )],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
                let Some(owner_agent_id) = self.state.module_artifact_owners.get(wasm_hash) else {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "list module artifact rejected: owner missing for {}",
                                    wasm_hash
                                )],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                };
                if owner_agent_id != seller_agent_id {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "list module artifact rejected: seller {} does not own {} (owner {})",
                                    seller_agent_id, wasm_hash, owner_agent_id
                                )],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }

                let fee_kind = ResourceKind::Data;
                let fee_amount = MODULE_LIST_FEE_AMOUNT;
                if !self.ensure_module_fee_affordable(
                    action_id,
                    seller_agent_id.as_str(),
                    fee_kind,
                    fee_amount,
                )? {
                    return Ok(true);
                }
                let order_id = self.peek_next_module_market_order_id();

                self.append_module_marketplace_order(
                    DomainEvent::ModuleArtifactListed {
                        seller_agent_id: seller_agent_id.clone(),
                        wasm_hash: wasm_hash.clone(),
                        price_kind: *price_kind,
                        price_amount: *price_amount,
                        order_id,
                        fee_kind,
                        fee_amount,
                    },
                    Some(CausedBy::Action(action_id)),
                )?;
                Ok(true)
            }
            Action::BuyModuleArtifact {
                buyer_agent_id,
                wasm_hash,
            } => {
                if !self.state.agents.contains_key(buyer_agent_id) {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::AgentNotFound {
                                agent_id: buyer_agent_id.clone(),
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }

                let Some(listing) = self.state.module_artifact_listings.get(wasm_hash).cloned()
                else {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "buy module artifact rejected: listing missing for {}",
                                    wasm_hash
                                )],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                };
                if listing.seller_agent_id == *buyer_agent_id {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "buy module artifact rejected: buyer {} already owns listing {}",
                                    buyer_agent_id, wasm_hash
                                )],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
                if !self.state.agents.contains_key(&listing.seller_agent_id) {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::AgentNotFound {
                                agent_id: listing.seller_agent_id.clone(),
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }

                let available = self
                    .state
                    .agents
                    .get(buyer_agent_id)
                    .map(|cell| cell.state.resources.get(listing.price_kind))
                    .unwrap_or(0);
                if available < listing.price_amount {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::InsufficientResource {
                                agent_id: buyer_agent_id.clone(),
                                kind: listing.price_kind,
                                requested: listing.price_amount,
                                available,
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
                let sale_id = self.peek_next_module_market_sale_id();

                self.append_event(
                    WorldEventBody::Domain(DomainEvent::ModuleArtifactSaleCompleted {
                        buyer_agent_id: buyer_agent_id.clone(),
                        seller_agent_id: listing.seller_agent_id,
                        wasm_hash: wasm_hash.clone(),
                        price_kind: listing.price_kind,
                        price_amount: listing.price_amount,
                        sale_id,
                        listing_order_id: if listing.order_id > 0 {
                            Some(listing.order_id)
                        } else {
                            None
                        },
                        bid_order_id: None,
                    }),
                    Some(CausedBy::Action(action_id)),
                )?;
                Ok(true)
            }
            Action::DelistModuleArtifact {
                seller_agent_id,
                wasm_hash,
            } => {
                if !self.state.agents.contains_key(seller_agent_id) {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::AgentNotFound {
                                agent_id: seller_agent_id.clone(),
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }

                let Some(listing) = self.state.module_artifact_listings.get(wasm_hash).cloned()
                else {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "delist module artifact rejected: listing missing for {}",
                                    wasm_hash
                                )],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                };
                if listing.seller_agent_id != *seller_agent_id {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "delist module artifact rejected: seller {} does not own listing {}",
                                    seller_agent_id, wasm_hash
                                )],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
                let Some(owner_agent_id) = self.state.module_artifact_owners.get(wasm_hash) else {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "delist module artifact rejected: owner missing for {}",
                                    wasm_hash
                                )],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                };
                if owner_agent_id != seller_agent_id {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "delist module artifact rejected: seller {} does not own {} (owner {})",
                                    seller_agent_id, wasm_hash, owner_agent_id
                                )],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }

                let fee_kind = ResourceKind::Data;
                let fee_amount = MODULE_DELIST_FEE_AMOUNT;
                if !self.ensure_module_fee_affordable(
                    action_id,
                    seller_agent_id.as_str(),
                    fee_kind,
                    fee_amount,
                )? {
                    return Ok(true);
                }

                self.append_event(
                    WorldEventBody::Domain(DomainEvent::ModuleArtifactDelisted {
                        seller_agent_id: seller_agent_id.clone(),
                        wasm_hash: wasm_hash.clone(),
                        order_id: if listing.order_id > 0 {
                            Some(listing.order_id)
                        } else {
                            None
                        },
                        fee_kind,
                        fee_amount,
                    }),
                    Some(CausedBy::Action(action_id)),
                )?;
                Ok(true)
            }
            Action::PlaceModuleArtifactBid {
                bidder_agent_id,
                wasm_hash,
                price_kind,
                price_amount,
            } => {
                if !self.state.agents.contains_key(bidder_agent_id) {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::AgentNotFound {
                                agent_id: bidder_agent_id.clone(),
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
                if *price_amount <= 0 {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::InvalidAmount {
                                amount: *price_amount,
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
                if !self.module_artifacts.contains(wasm_hash) {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "place module artifact bid rejected: missing artifact {}",
                                    wasm_hash
                                )],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
                let Some(owner) = self.state.module_artifact_owners.get(wasm_hash) else {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "place module artifact bid rejected: owner missing for {}",
                                    wasm_hash
                                )],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                };
                if owner == bidder_agent_id {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "place module artifact bid rejected: bidder {} already owns {}",
                                    bidder_agent_id, wasm_hash
                                )],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
                let available = self
                    .state
                    .agents
                    .get(bidder_agent_id)
                    .map(|cell| cell.state.resources.get(*price_kind))
                    .unwrap_or(0);
                if available < *price_amount {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::InsufficientResource {
                                agent_id: bidder_agent_id.clone(),
                                kind: *price_kind,
                                requested: *price_amount,
                                available,
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }

                let order_id = self.peek_next_module_market_order_id();
                self.append_module_marketplace_order(
                    DomainEvent::ModuleArtifactBidPlaced {
                        bidder_agent_id: bidder_agent_id.clone(),
                        wasm_hash: wasm_hash.clone(),
                        order_id,
                        price_kind: *price_kind,
                        price_amount: *price_amount,
                    },
                    Some(CausedBy::Action(action_id)),
                )?;
                Ok(true)
            }
            Action::CancelModuleArtifactBid {
                bidder_agent_id,
                wasm_hash,
                bid_order_id,
            } => {
                if !self.state.agents.contains_key(bidder_agent_id) {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::AgentNotFound {
                                agent_id: bidder_agent_id.clone(),
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
                let Some(bids) = self.state.module_artifact_bids.get(wasm_hash) else {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "cancel module artifact bid rejected: no bids for {}",
                                    wasm_hash
                                )],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                };
                if !bids.iter().any(|entry| {
                    entry.order_id == *bid_order_id && entry.bidder_agent_id == *bidder_agent_id
                }) {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "cancel module artifact bid rejected: bid {} not found for {}",
                                    bid_order_id, bidder_agent_id
                                )],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }

                self.append_event(
                    WorldEventBody::Domain(DomainEvent::ModuleArtifactBidCancelled {
                        bidder_agent_id: bidder_agent_id.clone(),
                        wasm_hash: wasm_hash.clone(),
                        order_id: *bid_order_id,
                        reason: "cancelled_by_bidder".to_string(),
                    }),
                    Some(CausedBy::Action(action_id)),
                )?;
                Ok(true)
            }
            Action::DestroyModuleArtifact {
                owner_agent_id,
                wasm_hash,
                reason,
            } => {
                if !self.state.agents.contains_key(owner_agent_id) {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::AgentNotFound {
                                agent_id: owner_agent_id.clone(),
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
                if reason.trim().is_empty() {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![
                                    "destroy module artifact rejected: reason is empty".to_string(),
                                ],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
                if !self.module_artifacts.contains(wasm_hash) {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "destroy module artifact rejected: missing artifact {}",
                                    wasm_hash
                                )],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }

                let Some(owner) = self.state.module_artifact_owners.get(wasm_hash) else {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "destroy module artifact rejected: owner missing for {}",
                                    wasm_hash
                                )],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                };
                if owner != owner_agent_id {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "destroy module artifact rejected: owner {} does not own {} (owner {})",
                                    owner_agent_id, wasm_hash, owner
                                )],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }
                if self.has_active_module_using_artifact(wasm_hash) {
                    self.append_event(
                        WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "destroy module artifact rejected: artifact {} is used by active module",
                                    wasm_hash
                                )],
                            },
                        }),
                        Some(CausedBy::Action(action_id)),
                    )?;
                    return Ok(true);
                }

                let fee_kind = ResourceKind::Electricity;
                let fee_amount = MODULE_DESTROY_FEE_AMOUNT;
                if !self.ensure_module_fee_affordable(
                    action_id,
                    owner_agent_id.as_str(),
                    fee_kind,
                    fee_amount,
                )? {
                    return Ok(true);
                }

                let retirement = self.prepare_module_artifact_retirement(wasm_hash.clone());
                self.append_module_artifact_retirement(
                    DomainEvent::ModuleArtifactDestroyed {
                        owner_agent_id: owner_agent_id.clone(),
                        wasm_hash: wasm_hash.clone(),
                        reason: reason.clone(),
                        fee_kind,
                        fee_amount,
                    },
                    Some(CausedBy::Action(action_id)),
                    retirement,
                )?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
