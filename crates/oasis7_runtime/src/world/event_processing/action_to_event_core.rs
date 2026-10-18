use super::super::logistics::{
    logistics_route_id, normalize_logistics_route_kind, resolve_logistics_transfer_path,
};
use super::*;

#[path = "action_to_event_core_main_token.rs"]
mod action_to_event_core_main_token;

impl World {
    fn ensure_restricted_starter_claim_admin(&self, issuer_account_id: &str) -> Result<(), String> {
        let registry = self
            .governance_main_token_controller_registry()
            .ok_or_else(|| "restricted grant admin registry is not configured".to_string())?;
        if registry
            .restricted_starter_claim_admin_account_ids
            .is_empty()
        {
            return Err("restricted grant admin registry is empty".to_string());
        }
        if !registry
            .restricted_starter_claim_admin_account_ids
            .contains(issuer_account_id)
        {
            return Err(format!(
                "restricted grant issuer is not allowlisted admin: issuer_account_id={issuer_account_id}"
            ));
        }
        if !registry
            .controller_signer_policies
            .contains_key(issuer_account_id)
        {
            return Err(format!(
                "restricted grant admin signer policy is not configured: issuer_account_id={issuer_account_id}"
            ));
        }
        Ok(())
    }

    pub(super) fn action_to_event_core(
        &self,
        action_id: ActionId,
        action: &Action,
    ) -> Result<WorldEventBody, WorldError> {
        match action {
            Action::RegisterAgent { agent_id, pos } => {
                if self.state.agents.contains_key(agent_id) {
                    Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentAlreadyExists {
                            agent_id: agent_id.clone(),
                        },
                    }))
                } else {
                    Ok(WorldEventBody::Domain(DomainEvent::AgentRegistered {
                        agent_id: agent_id.clone(),
                        pos: *pos,
                    }))
                }
            }
            Action::MoveAgent { agent_id, to } => match self.state.agents.get(agent_id) {
                Some(cell) => {
                    let target_location_id = format!("{}:{}:{}", to.x_cm, to.y_cm, to.z_cm);
                    if self
                        .state
                        .gameplay_policy
                        .forbidden_location_ids
                        .iter()
                        .any(|value| value == &target_location_id)
                    {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::RuleDenied {
                                notes: vec![format!(
                                    "move denied by gameplay forbidden_location_ids: {target_location_id}"
                                )],
                            },
                        }));
                    }
                    Ok(WorldEventBody::Domain(DomainEvent::AgentMoved {
                        agent_id: agent_id.clone(),
                        from: cell.state.pos,
                        to: *to,
                    }))
                }
                None => Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::AgentNotFound {
                        agent_id: agent_id.clone(),
                    },
                })),
            },
            Action::QueryObservation { agent_id } => {
                if self.state.agents.contains_key(agent_id) {
                    Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["observation requires rule module".to_string()],
                        },
                    }))
                } else {
                    Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: agent_id.clone(),
                        },
                    }))
                }
            }
            Action::EmitObservation { observation } => {
                Ok(WorldEventBody::Domain(DomainEvent::Observation {
                    observation: observation.clone(),
                }))
            }
            Action::BodyAction { agent_id, .. } => {
                if self.state.agents.contains_key(agent_id) {
                    Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["body action requires body module".to_string()],
                        },
                    }))
                } else {
                    Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: agent_id.clone(),
                        },
                    }))
                }
            }
            Action::EmitBodyAttributes {
                agent_id,
                view,
                reason,
            } => {
                let Some(cell) = self.state.agents.get(agent_id) else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: agent_id.clone(),
                        },
                    }));
                };
                if let Err(reason) = validate_body_kernel_view(&cell.state.body_view, view) {
                    return Ok(WorldEventBody::Domain(
                        DomainEvent::BodyAttributesRejected {
                            agent_id: agent_id.clone(),
                            reason,
                        },
                    ));
                }
                Ok(WorldEventBody::Domain(DomainEvent::BodyAttributesUpdated {
                    agent_id: agent_id.clone(),
                    view: view.clone(),
                    reason: reason.clone(),
                }))
            }
            Action::ExpandBodyInterface {
                agent_id,
                interface_module_item_id,
            } => Ok(evaluate_expand_body_interface(
                self,
                action_id,
                agent_id,
                interface_module_item_id,
            )),
            Action::DeployModuleArtifact { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "deploy_module_artifact requires runtime action loop".to_string(),
                        ],
                    },
                }))
            }
            Action::CompileModuleArtifactFromSource { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "compile_module_artifact_from_source requires runtime action loop"
                                .to_string(),
                        ],
                    },
                }))
            }
            Action::InstallModuleFromArtifact { .. }
            | Action::InstallModuleFromArtifactWithFinality { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "install_module_from_artifact requires runtime action loop".to_string(),
                        ],
                    },
                }))
            }
            Action::InstallModuleToTargetFromArtifact { .. }
            | Action::InstallModuleToTargetFromArtifactWithFinality { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "install_module_to_target_from_artifact requires runtime action loop"
                                .to_string(),
                        ],
                    },
                }))
            }
            Action::UpgradeModuleFromArtifact { .. }
            | Action::UpgradeModuleFromArtifactWithFinality { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "upgrade_module_from_artifact requires runtime action loop".to_string(),
                        ],
                    },
                }))
            }
            Action::RollbackModuleInstance { .. }
            | Action::RollbackModuleInstanceWithFinality { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "rollback_module_instance requires runtime action loop".to_string(),
                        ],
                    },
                }))
            }
            Action::ModuleReleaseSubmit { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "module_release_submit requires runtime action loop".to_string(),
                        ],
                    },
                }))
            }
            Action::ModuleReleaseShadow { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "module_release_shadow requires runtime action loop".to_string(),
                        ],
                    },
                }))
            }
            Action::ModuleReleaseApproveRole { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "module_release_approve_role requires runtime action loop".to_string(),
                        ],
                    },
                }))
            }
            Action::ModuleReleaseBindRoles { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "module_release_bind_roles requires runtime action loop".to_string(),
                        ],
                    },
                }))
            }
            Action::ModuleReleaseReject { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "module_release_reject requires runtime action loop".to_string(),
                        ],
                    },
                }))
            }
            Action::ModuleReleaseApply { .. } | Action::ModuleReleaseApplyWithFinality { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "module_release_apply requires runtime action loop".to_string(),
                        ],
                    },
                }))
            }
            Action::ListModuleArtifactForSale { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "list_module_artifact_for_sale requires runtime action loop"
                                .to_string(),
                        ],
                    },
                }))
            }
            Action::BuyModuleArtifact { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec!["buy_module_artifact requires runtime action loop".to_string()],
                    },
                }))
            }
            Action::DelistModuleArtifact { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "delist_module_artifact requires runtime action loop".to_string(),
                        ],
                    },
                }))
            }
            Action::DestroyModuleArtifact { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "destroy_module_artifact requires runtime action loop".to_string(),
                        ],
                    },
                }))
            }
            Action::PlaceModuleArtifactBid { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "place_module_artifact_bid requires runtime action loop".to_string(),
                        ],
                    },
                }))
            }
            Action::CancelModuleArtifactBid { .. } => {
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec![
                            "cancel_module_artifact_bid requires runtime action loop".to_string(),
                        ],
                    },
                }))
            }
            Action::TransferResource {
                from_agent_id,
                to_agent_id,
                ..
            } => {
                if !self.state.agents.contains_key(from_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: from_agent_id.clone(),
                        },
                    }));
                }
                if !self.state.agents.contains_key(to_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: to_agent_id.clone(),
                        },
                    }));
                }
                Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                    action_id,
                    reason: RejectReason::RuleDenied {
                        notes: vec!["transfer requires rule module".to_string()],
                    },
                }))
            }
            Action::RedeemPower {
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
            } => Ok(WorldEventBody::Domain(self.evaluate_redeem_power_action(
                node_id.as_str(),
                target_agent_id.as_str(),
                *redeem_credits,
                *nonce,
                None,
            ))),
            Action::RedeemPowerSigned {
                node_id,
                target_agent_id,
                redeem_credits,
                nonce,
                signer_node_id,
                signature,
            } => Ok(WorldEventBody::Domain(self.evaluate_redeem_power_action(
                node_id.as_str(),
                target_agent_id.as_str(),
                *redeem_credits,
                *nonce,
                Some((signer_node_id.as_str(), signature.as_str())),
            ))),
            Action::ApplyNodePointsSettlementSigned {
                report,
                signer_node_id,
                mint_records,
            } => Ok(WorldEventBody::Domain(
                self.evaluate_apply_node_points_settlement_action(
                    action_id,
                    report,
                    signer_node_id.as_str(),
                    mint_records.as_slice(),
                ),
            )),
            Action::InitializeMainTokenGenesis { allocations } => Ok(WorldEventBody::Domain(
                self.evaluate_initialize_main_token_genesis_action(
                    action_id,
                    allocations.as_slice(),
                ),
            )),
            Action::ClaimMainTokenVesting {
                bucket_id,
                beneficiary,
                nonce,
            } => Ok(WorldEventBody::Domain(
                self.evaluate_claim_main_token_vesting_action(
                    action_id,
                    bucket_id.as_str(),
                    beneficiary.as_str(),
                    *nonce,
                ),
            )),
            Action::TransferMainToken {
                from_account_id,
                to_account_id,
                amount,
                nonce,
                asset_id,
                memo,
                ..
            } => Ok(WorldEventBody::Domain(
                self.evaluate_transfer_main_token_action(
                    action_id,
                    from_account_id.as_str(),
                    to_account_id.as_str(),
                    *amount,
                    *nonce,
                    asset_id.as_deref(),
                    memo.as_deref(),
                ),
            )),
            Action::ApplyMainTokenEpochIssuance {
                epoch_index,
                actual_stake_ratio_bps,
            } => Ok(WorldEventBody::Domain(
                self.evaluate_apply_main_token_epoch_issuance_action(
                    action_id,
                    *epoch_index,
                    *actual_stake_ratio_bps,
                ),
            )),
            Action::SettleMainTokenFee { fee_kind, amount } => Ok(WorldEventBody::Domain(
                self.evaluate_settle_main_token_fee_action(action_id, *fee_kind, *amount),
            )),
            Action::UpdateMainTokenPolicy { proposal_id, next } => Ok(WorldEventBody::Domain(
                self.evaluate_update_main_token_policy_action(action_id, *proposal_id, next),
            )),
            Action::DistributeMainTokenTreasury {
                proposal_id,
                distribution_id,
                bucket_id,
                distributions,
            } => Ok(WorldEventBody::Domain(
                self.evaluate_distribute_main_token_treasury_action(
                    action_id,
                    *proposal_id,
                    distribution_id.as_str(),
                    bucket_id.as_str(),
                    distributions.as_slice(),
                ),
            )),
            Action::TopUpRestrictedStarterClaimLiveopsPool {
                controller_account_id,
                top_up_id,
                amount,
            } => Ok(WorldEventBody::Domain(
                self.evaluate_top_up_restricted_starter_claim_liveops_pool_action(
                    action_id,
                    controller_account_id.as_str(),
                    top_up_id.as_str(),
                    *amount,
                ),
            )),
            Action::IssueRestrictedStarterClaimGrant {
                issuer_account_id,
                beneficiary_account_id,
                amount,
                issuance_reason,
                expires_at_epoch,
            } => Ok(WorldEventBody::Domain(
                self.evaluate_issue_restricted_starter_claim_grant_action(
                    action_id,
                    issuer_account_id.as_str(),
                    beneficiary_account_id.as_str(),
                    *amount,
                    issuance_reason.as_str(),
                    *expires_at_epoch,
                ),
            )),
            Action::RevokeRestrictedStarterClaimGrant {
                issuer_account_id,
                beneficiary_account_id,
                revoke_reason,
            } => Ok(WorldEventBody::Domain(
                self.evaluate_revoke_restricted_starter_claim_grant_action(
                    action_id,
                    issuer_account_id.as_str(),
                    beneficiary_account_id.as_str(),
                    revoke_reason.as_str(),
                ),
            )),
            Action::TransferMaterial {
                requester_agent_id,
                from_ledger,
                to_ledger,
                kind,
                amount,
                distance_km,
                priority,
                route_id,
                route_ids,
                auto_reroute,
            } => {
                if !self.state.agents.contains_key(requester_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: requester_agent_id.clone(),
                        },
                    }));
                }
                if from_ledger == to_ledger {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["from_ledger and to_ledger cannot be the same".to_string()],
                        },
                    }));
                }
                if kind.trim().is_empty() {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["material kind cannot be empty".to_string()],
                        },
                    }));
                }
                if *amount <= 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::InvalidAmount { amount: *amount },
                    }));
                }
                if *distance_km < 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["distance_km must be >= 0".to_string()],
                        },
                    }));
                }
                if *distance_km > MATERIAL_TRANSFER_MAX_DISTANCE_KM {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::MaterialTransferDistanceExceeded {
                            distance_km: *distance_km,
                            max_distance_km: MATERIAL_TRANSFER_MAX_DISTANCE_KM,
                        },
                    }));
                }
                // Empty route bindings retain legacy direct semantics. The
                // graph planner is shared with transfer quotes and is opt-in
                // via an explicit route/path binding or auto_reroute.
                let path_plan = match resolve_logistics_transfer_path(
                    self,
                    from_ledger,
                    to_ledger,
                    kind.as_str(),
                    *amount,
                    *distance_km,
                    *priority,
                    route_id.as_deref(),
                    route_ids.as_slice(),
                    *auto_reroute,
                ) {
                    Ok(plan) => plan,
                    Err(reason) => {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason,
                        }));
                    }
                };
                let material_kind = path_plan.effective_kind.as_str();
                let available = self.ledger_material_balance(from_ledger, material_kind);
                if available < *amount {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::InsufficientMaterial {
                            material_kind: material_kind.to_string(),
                            requested: *amount,
                            available,
                        },
                    }));
                }
                let priority = priority
                    .as_ref()
                    .copied()
                    .or(path_plan.route_priority)
                    .unwrap_or_else(|| material_transit_priority_for_kind(self, material_kind));
                let event_kind = path_plan.effective_kind;
                let loss_bps = material_transit_loss_bps_for_kind(self, event_kind.as_str());
                let effective_distance_km = path_plan.effective_distance_km;
                let selected_route_ids = path_plan.route_ids;
                let path_id = path_plan.path_id;
                let tariff_electricity_total = path_plan.tariff_electricity_total;
                let reroute_count = path_plan.reroute_count;
                let event_route_id = if route_ids.is_empty() {
                    route_id.clone()
                } else {
                    None
                };
                if tariff_electricity_total > 0 {
                    let electricity = self
                        .state
                        .agents
                        .get(requester_agent_id)
                        .map(|cell| cell.state.resources.get(ResourceKind::Electricity))
                        .unwrap_or_default();
                    if electricity < tariff_electricity_total {
                        return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                            action_id,
                            reason: RejectReason::InsufficientResource {
                                agent_id: requester_agent_id.clone(),
                                kind: ResourceKind::Electricity,
                                requested: tariff_electricity_total,
                                available: electricity,
                            },
                        }));
                    }
                }

                if effective_distance_km == 0 && path_id.is_none() {
                    return Ok(WorldEventBody::Domain(DomainEvent::MaterialTransferred {
                        transfer_id: Some(action_id),
                        requester_agent_id: requester_agent_id.clone(),
                        from_ledger: from_ledger.clone(),
                        to_ledger: to_ledger.clone(),
                        kind: event_kind.clone(),
                        amount: *amount,
                        distance_km: effective_distance_km,
                        priority,
                        route_id: event_route_id,
                    }));
                }

                if self.state.pending_material_transits.len() >= MATERIAL_TRANSFER_MAX_INFLIGHT {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::MaterialTransitCapacityExceeded {
                            in_flight: self.state.pending_material_transits.len(),
                            max_in_flight: MATERIAL_TRANSFER_MAX_INFLIGHT,
                        },
                    }));
                }

                let transit_ticks =
                    material_transit_ticks(effective_distance_km).max(u64::from(path_id.is_some()));
                let ready_at = self.state.time.saturating_add(transit_ticks);
                Ok(WorldEventBody::Domain(
                    DomainEvent::MaterialTransitStarted {
                        job_id: action_id,
                        requester_agent_id: requester_agent_id.clone(),
                        from_ledger: from_ledger.clone(),
                        to_ledger: to_ledger.clone(),
                        kind: event_kind,
                        amount: *amount,
                        distance_km: effective_distance_km,
                        loss_bps,
                        ready_at,
                        priority,
                        route_id: event_route_id,
                        path_id,
                        route_ids: selected_route_ids,
                        tariff_electricity_total,
                        reroute_count,
                    },
                ))
            }
            Action::RegisterLogisticsRoute {
                requester_agent_id,
                from_ledger,
                to_ledger,
                kind,
                distance_km,
                priority,
                capacity_units,
                tariff_electricity_per_unit,
            } => {
                if !self.state.agents.contains_key(requester_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: requester_agent_id.clone(),
                        },
                    }));
                }
                if from_ledger == to_ledger {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["from_ledger and to_ledger cannot be the same".to_string()],
                        },
                    }));
                }
                let Some(normalized_kind) = normalize_logistics_route_kind(kind.as_str()) else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["material kind cannot be empty".to_string()],
                        },
                    }));
                };
                if *distance_km < 0 || *distance_km > MATERIAL_TRANSFER_MAX_DISTANCE_KM {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "distance_km must be within 0..={MATERIAL_TRANSFER_MAX_DISTANCE_KM}"
                            )],
                        },
                    }));
                }
                if *capacity_units <= 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["capacity_units must be > 0".to_string()],
                        },
                    }));
                }
                if *tariff_electricity_per_unit < 0 {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["tariff_electricity_per_unit must be >= 0".to_string()],
                        },
                    }));
                }
                let route_id = logistics_route_id(
                    from_ledger,
                    to_ledger,
                    normalized_kind.as_str(),
                    *distance_km,
                    *priority,
                );
                if self.state.logistics_routes.values().any(|route| {
                    route.from_ledger == *from_ledger
                        && route.to_ledger == *to_ledger
                        && route.kind == normalized_kind
                        && route.distance_km == *distance_km
                        && route.priority == *priority
                }) || self.state.logistics_routes.contains_key(&route_id)
                {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec!["logistics route tuple already registered".to_string()],
                        },
                    }));
                }
                Ok(WorldEventBody::Domain(
                    DomainEvent::LogisticsRouteRegistered {
                        requester_agent_id: requester_agent_id.clone(),
                        route_id: Some(route_id),
                        from_ledger: from_ledger.clone(),
                        to_ledger: to_ledger.clone(),
                        kind: normalized_kind,
                        distance_km: *distance_km,
                        priority: *priority,
                        owner_agent_id: requester_agent_id.clone(),
                        available: true,
                        capacity_units: *capacity_units,
                        tariff_electricity_per_unit: *tariff_electricity_per_unit,
                    },
                ))
            }
            Action::SetLogisticsRouteAvailability {
                requester_agent_id,
                route_id,
                available,
            } => {
                if !self.state.agents.contains_key(requester_agent_id) {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::AgentNotFound {
                            agent_id: requester_agent_id.clone(),
                        },
                    }));
                }
                let Some(route) = self.state.logistics_routes.get(route_id) else {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!("logistics route not found: route_id={route_id}")],
                        },
                    }));
                };
                if route.owner_agent_id != *requester_agent_id {
                    return Ok(WorldEventBody::Domain(DomainEvent::ActionRejected {
                        action_id,
                        reason: RejectReason::RuleDenied {
                            notes: vec![format!(
                                "logistics route availability requires owner: route_id={route_id}"
                            )],
                        },
                    }));
                }
                Ok(WorldEventBody::Domain(
                    DomainEvent::LogisticsRouteAvailabilityChanged {
                        requester_agent_id: requester_agent_id.clone(),
                        route_id: route_id.clone(),
                        available: *available,
                        owner_agent_id: route.owner_agent_id.clone(),
                    },
                ))
            }
            _ => unreachable!("action_to_event_core received unsupported action variant"),
        }
    }
}
