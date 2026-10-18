use super::super::{
    Action, DomainEvent, ModuleKind, ModuleRole, ModuleSubscriptionStage, WorldEventBody,
};

pub(super) fn event_kind_label(body: &WorldEventBody) -> &'static str {
    match body {
        WorldEventBody::Domain(event) => match event {
            DomainEvent::AgentRegistered { .. } => "domain.agent_registered",
            DomainEvent::AgentMoved { .. } => "domain.agent_moved",
            DomainEvent::AgentLocationAuthorityUpdated { .. } => {
                "domain.agent_location_authority_updated"
            }
            DomainEvent::LocationAnchorUpdated { .. } => "domain.location_anchor_updated",
            DomainEvent::FactorySiteAuthorityUpdated { .. } => {
                "domain.economy.factory_site_authority_updated"
            }
            DomainEvent::FactoryConstructionPowerProfileUpdated { .. } => {
                "domain.economy.factory_construction_power_profile_updated"
            }
            DomainEvent::ActionAccepted { .. } => "domain.action_accepted",
            DomainEvent::ActionRejected { .. } => "domain.action_rejected",
            DomainEvent::AgentIntentProposed { .. } => "domain.agent_intent_proposed",
            DomainEvent::AgentIntentSubmitted { .. } => "domain.agent_intent_submitted",
            DomainEvent::AgentIntentAccepted { .. } => "domain.agent_intent_accepted",
            DomainEvent::AgentIntentReplaced { .. } => "domain.agent_intent_replaced",
            DomainEvent::AgentIntentTransitioned { .. } => "domain.agent_intent_transitioned",
            DomainEvent::Observation { .. } => "domain.observation",
            DomainEvent::BodyAttributesUpdated { .. } => "domain.body_attributes_updated",
            DomainEvent::BodyAttributesRejected { .. } => "domain.body_attributes_rejected",
            DomainEvent::BodyInterfaceExpanded { .. } => "domain.body_interface_expanded",
            DomainEvent::BodyInterfaceExpandRejected { .. } => {
                "domain.body_interface_expand_rejected"
            }
            DomainEvent::ModuleArtifactDeployed { .. } => "domain.module_artifact_deployed",
            DomainEvent::ModuleInstalled { .. } => "domain.module_installed",
            DomainEvent::ModuleUpgraded { .. } => "domain.module_upgraded",
            DomainEvent::ModuleReleaseRequested { .. } => "domain.module_release.requested",
            DomainEvent::ModuleReleaseShadowed { .. } => "domain.module_release.shadowed",
            DomainEvent::ModuleReleaseRoleApproved { .. } => "domain.module_release.role_approved",
            DomainEvent::ModuleReleaseRolesBound { .. } => "domain.module_release.roles_bound",
            DomainEvent::ModuleReleaseAttested { .. } => "domain.module_release.attested",
            DomainEvent::ModuleReleaseRejected { .. } => "domain.module_release.rejected",
            DomainEvent::ModuleReleaseApplied { .. } => "domain.module_release.applied",
            DomainEvent::ModuleRollbackApplied { .. } => "domain.module.rollback_applied",
            DomainEvent::ModuleArtifactListed { .. } => "domain.module_artifact_listed",
            DomainEvent::ModuleArtifactDelisted { .. } => "domain.module_artifact_delisted",
            DomainEvent::ModuleArtifactDestroyed { .. } => "domain.module_artifact_destroyed",
            DomainEvent::ModuleArtifactBidPlaced { .. } => "domain.module_artifact_bid_placed",
            DomainEvent::ModuleArtifactBidCancelled { .. } => {
                "domain.module_artifact_bid_cancelled"
            }
            DomainEvent::ModuleArtifactSaleCompleted { .. } => "domain.module_artifact_sold",
            DomainEvent::ResourceTransferred { .. } => "domain.resource_transferred",
            DomainEvent::DataCollected { .. } => "domain.data_collected",
            DomainEvent::DataCollectedAuthenticated { .. } => "domain.data_collected_authenticated",
            DomainEvent::DataAccessGranted { .. } => "domain.data_access_granted",
            DomainEvent::DataAccessRevoked { .. } => "domain.data_access_revoked",
            DomainEvent::PowerRedeemed { .. } => "domain.power_redeemed",
            DomainEvent::PowerRedeemRejected { .. } => "domain.power_redeem_rejected",
            DomainEvent::NodePointsSettlementApplied { .. } => {
                "domain.reward.node_points_settlement_applied"
            }
            DomainEvent::MainTokenGenesisInitialized { .. } => {
                "domain.main_token.genesis_initialized"
            }
            DomainEvent::MainTokenVestingClaimed { .. } => "domain.main_token.vesting_claimed",
            DomainEvent::MainTokenTransferred { .. } => "domain.main_token.transferred",
            DomainEvent::MainTokenEpochIssued { .. } => "domain.main_token.epoch_issued",
            DomainEvent::MainTokenFeeSettled { .. } => "domain.main_token.fee_settled",
            DomainEvent::MainTokenPolicyUpdateScheduled { .. } => {
                "domain.main_token.policy_update_scheduled"
            }
            DomainEvent::MainTokenTreasuryDistributed { .. } => {
                "domain.main_token.treasury_distributed"
            }
            DomainEvent::RestrictedStarterClaimLiveopsPoolToppedUp { .. } => {
                "domain.main_token.restricted_claim_liveops_pool_topped_up"
            }
            DomainEvent::RestrictedStarterClaimGrantIssued { .. } => {
                "domain.main_token.restricted_claim_grant_issued"
            }
            DomainEvent::RestrictedStarterClaimGrantExpired { .. } => {
                "domain.main_token.restricted_claim_grant_expired"
            }
            DomainEvent::RestrictedStarterClaimGrantRevoked { .. } => {
                "domain.main_token.restricted_claim_grant_revoked"
            }
            DomainEvent::StarterOcClaimed { .. } => "domain.gameplay.starter_oc_claimed",
            DomainEvent::AgentClaimed { .. } => "domain.gameplay.agent_claimed",
            DomainEvent::AgentClaimReleaseRequested { .. } => {
                "domain.gameplay.agent_claim_release_requested"
            }
            DomainEvent::AgentClaimUpkeepSettled { .. } => {
                "domain.gameplay.agent_claim_upkeep_settled"
            }
            DomainEvent::AgentClaimEnteredGrace { .. } => {
                "domain.gameplay.agent_claim_entered_grace"
            }
            DomainEvent::AgentClaimIdleWarning { .. } => "domain.gameplay.agent_claim_idle_warning",
            DomainEvent::AgentClaimReleased { .. } => "domain.gameplay.agent_claim_released",
            DomainEvent::AgentClaimReclaimed { .. } => "domain.gameplay.agent_claim_reclaimed",
            DomainEvent::MaterialTransferred { .. } => "domain.material_transferred",
            DomainEvent::LogisticsRouteRegistered { .. } => "domain.logistics_route_registered",
            DomainEvent::LogisticsRouteAvailabilityChanged { .. } => {
                "domain.logistics_route_availability_changed"
            }
            DomainEvent::LogisticsPathRerouted { .. } => "domain.logistics_path_rerouted",
            DomainEvent::MaterialTransitStarted { .. } => "domain.material_transit_started",
            DomainEvent::MaterialTransitCompleted { .. } => "domain.material_transit_completed",
            DomainEvent::FactoryBuildStarted { .. } => "domain.economy.factory_build_started",
            DomainEvent::FactoryBuilt { .. } => "domain.economy.factory_built",
            DomainEvent::FactoryDurabilityChanged { .. } => {
                "domain.economy.factory_durability_changed"
            }
            DomainEvent::FactoryMaintained { .. } => "domain.economy.factory_maintained",
            DomainEvent::FactoryRecycled { .. } => "domain.economy.factory_recycled",
            DomainEvent::RecipeStarted { .. } => "domain.economy.recipe_started",
            DomainEvent::RecipeCompleted { .. } => "domain.economy.recipe_completed",
            DomainEvent::FactoryProductionBlocked { .. } => {
                "domain.economy.factory_production_blocked"
            }
            DomainEvent::FactoryProductionResumed { .. } => {
                "domain.economy.factory_production_resumed"
            }
            DomainEvent::FactoryProductionPaused { .. } => {
                "domain.economy.factory_production_paused"
            }
            DomainEvent::GameplayPolicyUpdated { .. } => "domain.gameplay.policy_updated",
            DomainEvent::EconomicContractOpened { .. } => {
                "domain.gameplay.economic_contract_opened"
            }
            DomainEvent::EconomicContractAccepted { .. } => {
                "domain.gameplay.economic_contract_accepted"
            }
            DomainEvent::EconomicContractSettled { .. } => {
                "domain.gameplay.economic_contract_settled"
            }
            DomainEvent::EconomicContractExpired { .. } => {
                "domain.gameplay.economic_contract_expired"
            }
            DomainEvent::AllianceFormed { .. } => "domain.gameplay.alliance_formed",
            DomainEvent::AllianceJoined { .. } => "domain.gameplay.alliance_joined",
            DomainEvent::AllianceLeft { .. } => "domain.gameplay.alliance_left",
            DomainEvent::AllianceDissolved { .. } => "domain.gameplay.alliance_dissolved",
            DomainEvent::WarDeclared { .. } => "domain.gameplay.war_declared",
            DomainEvent::WarConcluded { .. } => "domain.gameplay.war_concluded",
            DomainEvent::GovernanceProposalOpened { .. } => {
                "domain.gameplay.governance_proposal_opened"
            }
            DomainEvent::GovernanceVoteCast { .. } => "domain.gameplay.governance_vote_cast",
            DomainEvent::GovernanceProposalFinalized { .. } => {
                "domain.gameplay.governance_proposal_finalized"
            }
            DomainEvent::CrisisSpawned { .. } => "domain.gameplay.crisis_spawned",
            DomainEvent::CrisisResolved { .. } => "domain.gameplay.crisis_resolved",
            DomainEvent::CrisisTimedOut { .. } => "domain.gameplay.crisis_timed_out",
            DomainEvent::MetaProgressGranted { .. } => "domain.gameplay.meta_progress_granted",
            DomainEvent::ProductValidated { .. } => "domain.economy.product_validated",
            DomainEvent::ProductValidationRecorded { .. } => {
                "domain.economy.product_validation_recorded"
            }
            DomainEvent::ProductValidationAttemptStarted { .. } => {
                "domain.economy.product_validation_attempt_started"
            }
            DomainEvent::MaterialProfileGoverned { .. } => {
                "domain.economy.material_profile_governed"
            }
            DomainEvent::ProductProfileGoverned { .. } => "domain.economy.product_profile_governed",
            DomainEvent::RecipeProfileGoverned { .. } => "domain.economy.recipe_profile_governed",
            DomainEvent::FactoryProfileGoverned { .. } => "domain.economy.factory_profile_governed",
        },
        WorldEventBody::EffectQueued(_) => "effect.queued",
        WorldEventBody::ReceiptAppended(_) => "effect.receipt_appended",
        WorldEventBody::CapabilityAuthorization(_) => "capability.authorization",
        WorldEventBody::PolicyDecisionRecorded(_) => "policy.decision_recorded",
        WorldEventBody::RuleDecisionRecorded(_) => "rule.decision_recorded",
        WorldEventBody::ActionOverridden(_) => "rule.action_overridden",
        WorldEventBody::Governance(_) => "governance",
        WorldEventBody::ModuleEvent(_) => "module.event",
        WorldEventBody::ModuleCallFailed(_) => "module.call_failed",
        WorldEventBody::ModuleEmitted(_) => "module.emitted",
        WorldEventBody::ModuleStateUpdated(_) => "module.state_updated",
        WorldEventBody::ModuleRuntimeCharged(_) => "module.runtime_charged",
        WorldEventBody::ProductValidationDeliveryCursorUpdated(_) => {
            "runtime.product_validation_delivery_cursor_updated"
        }
        WorldEventBody::SnapshotCreated(_) => "snapshot.created",
        WorldEventBody::ManifestUpdated(_) => "manifest.updated",
        WorldEventBody::RollbackApplied(_) => "rollback.applied",
    }
}

pub(super) fn action_kind_label(action: &Action) -> &'static str {
    match action {
        Action::WorldServiceIntent { .. } => "action.world_service.intent",
        Action::RegisterAgent { .. } => "action.register_agent",
        Action::MoveAgent { .. } => "action.move_agent",
        Action::QueryObservation { .. } => "action.query_observation",
        Action::EmitObservation { .. } => "action.emit_observation",
        Action::BodyAction { .. } => "action.body_action",
        Action::EmitBodyAttributes { .. } => "action.emit_body_attributes",
        Action::ExpandBodyInterface { .. } => "action.expand_body_interface",
        Action::DeployModuleArtifact { .. } => "action.module.deploy_artifact",
        Action::CompileModuleArtifactFromSource { .. } => {
            "action.module.compile_artifact_from_source"
        }
        Action::InstallModuleFromArtifact { .. } => "action.module.install_from_artifact",
        Action::InstallModuleFromArtifactWithFinality { .. } => {
            "action.module.install_from_artifact_with_finality"
        }
        Action::InstallModuleToTargetFromArtifact { .. } => {
            "action.module.install_to_target_from_artifact"
        }
        Action::InstallModuleToTargetFromArtifactWithFinality { .. } => {
            "action.module.install_to_target_from_artifact_with_finality"
        }
        Action::UpgradeModuleFromArtifact { .. } => "action.module.upgrade_from_artifact",
        Action::UpgradeModuleFromArtifactWithFinality { .. } => {
            "action.module.upgrade_from_artifact_with_finality"
        }
        Action::RollbackModuleInstance { .. } => "action.module.rollback_instance",
        Action::RollbackModuleInstanceWithFinality { .. } => {
            "action.module.rollback_instance_with_finality"
        }
        Action::ModuleReleaseSubmit { .. } => "action.module_release.submit",
        Action::ModuleReleaseShadow { .. } => "action.module_release.shadow",
        Action::ModuleReleaseApproveRole { .. } => "action.module_release.approve_role",
        Action::ModuleReleaseBindRoles { .. } => "action.module_release.bind_roles",
        Action::ModuleReleaseSubmitAttestation { .. } => "action.module_release.submit_attestation",
        Action::ModuleReleaseReject { .. } => "action.module_release.reject",
        Action::ModuleReleaseApply { .. } => "action.module_release.apply",
        Action::ModuleReleaseApplyWithFinality { .. } => {
            "action.module_release.apply_with_finality"
        }
        Action::ListModuleArtifactForSale { .. } => "action.module.list_artifact_for_sale",
        Action::BuyModuleArtifact { .. } => "action.module.buy_artifact",
        Action::DelistModuleArtifact { .. } => "action.module.delist_artifact",
        Action::DestroyModuleArtifact { .. } => "action.module.destroy_artifact",
        Action::PlaceModuleArtifactBid { .. } => "action.module.place_artifact_bid",
        Action::CancelModuleArtifactBid { .. } => "action.module.cancel_artifact_bid",
        Action::TransferResource { .. } => "action.transfer_resource",
        Action::CollectData { .. } => "action.economy.collect_data",
        Action::CollectDataAuthenticated { .. } => "action.economy.collect_data_authenticated",
        Action::GrantDataAccess { .. } => "action.economy.grant_data_access",
        Action::RevokeDataAccess { .. } => "action.economy.revoke_data_access",
        Action::RedeemPower { .. } => "action.redeem_power",
        Action::RedeemPowerSigned { .. } => "action.redeem_power_signed",
        Action::ApplyNodePointsSettlementSigned { .. } => {
            "action.reward.apply_node_points_settlement_signed"
        }
        Action::InitializeMainTokenGenesis { .. } => "action.main_token.initialize_genesis",
        Action::ClaimMainTokenVesting { .. } => "action.main_token.claim_vesting",
        Action::TransferMainToken { .. } => "action.main_token.transfer",
        Action::ApplyMainTokenEpochIssuance { .. } => "action.main_token.apply_epoch_issuance",
        Action::SettleMainTokenFee { .. } => "action.main_token.settle_fee",
        Action::UpdateMainTokenPolicy { .. } => "action.main_token.update_policy",
        Action::DistributeMainTokenTreasury { .. } => "action.main_token.distribute_treasury",
        Action::TopUpRestrictedStarterClaimLiveopsPool { .. } => {
            "action.main_token.top_up_restricted_claim_liveops_pool"
        }
        Action::IssueRestrictedStarterClaimGrant { .. } => {
            "action.main_token.issue_restricted_claim_grant"
        }
        Action::RevokeRestrictedStarterClaimGrant { .. } => {
            "action.main_token.revoke_restricted_claim_grant"
        }
        Action::ClaimAgent { .. } => "action.gameplay.claim_agent",
        Action::ClaimStarterOc { .. } => "action.gameplay.claim_starter_oc",
        Action::ReleaseAgentClaim { .. } => "action.gameplay.release_agent_claim",
        Action::TransferMaterial { .. } => "action.transfer_material",
        Action::RegisterLogisticsRoute { .. } => "action.register_logistics_route",
        Action::SetLogisticsRouteAvailability { .. } => "action.set_logistics_route_availability",
        Action::FormAlliance { .. } => "action.gameplay.form_alliance",
        Action::JoinAlliance { .. } => "action.gameplay.join_alliance",
        Action::LeaveAlliance { .. } => "action.gameplay.leave_alliance",
        Action::DissolveAlliance { .. } => "action.gameplay.dissolve_alliance",
        Action::DeclareWar { .. } => "action.gameplay.declare_war",
        Action::OpenGovernanceProposal { .. } => "action.gameplay.open_governance_proposal",
        Action::CastGovernanceVote { .. } => "action.gameplay.cast_governance_vote",
        Action::ResolveCrisis { .. } => "action.gameplay.resolve_crisis",
        Action::GrantMetaProgress { .. } => "action.gameplay.grant_meta_progress",
        Action::UpdateGameplayPolicy { .. } => "action.gameplay.update_policy",
        Action::UpdateRestrictedStarterClaimAdminRegistry { .. } => {
            "action.governance.update_restricted_claim_admin_registry"
        }
        Action::SubmitGovernanceValidatorAdmission { .. } => {
            "action.governance.submit_validator_admission"
        }
        Action::ApproveGovernanceValidatorAdmission { .. } => {
            "action.governance.approve_validator_admission"
        }
        Action::ActivateGovernanceValidatorAdmission { .. } => {
            "action.governance.activate_validator_admission"
        }
        Action::RevokeGovernanceValidatorAdmission { .. } => {
            "action.governance.revoke_validator_admission"
        }
        Action::OpenEconomicContract { .. } => "action.gameplay.open_economic_contract",
        Action::AcceptEconomicContract { .. } => "action.gameplay.accept_economic_contract",
        Action::SettleEconomicContract { .. } => "action.gameplay.settle_economic_contract",
        Action::EmitResourceTransfer { .. } => "action.emit_resource_transfer",
        Action::BuildFactory { .. } => "action.economy.build_factory",
        Action::BuildFactoryWithModule { .. } => "action.economy.build_factory_with_module",
        Action::MaintainFactory { .. } => "action.economy.maintain_factory",
        Action::RecycleFactory { .. } => "action.economy.recycle_factory",
        Action::ScheduleRecipe { .. } => "action.economy.schedule_recipe",
        Action::PauseFactoryProduction { .. } => "action.economy.pause_factory_production",
        Action::ScheduleRecipeWithModule { .. } => "action.economy.schedule_recipe_with_module",
        Action::ValidateProduct { .. } => "action.economy.validate_product",
        Action::ValidateProductWithModule { .. } => "action.economy.validate_product_with_module",
        Action::GovernMaterialProfile { .. } => "action.economy.govern_material_profile",
        Action::GovernProductProfile { .. } => "action.economy.govern_product_profile",
        Action::GovernRecipeProfile { .. } => "action.economy.govern_recipe_profile",
        Action::GovernFactoryProfile { .. } => "action.economy.govern_factory_profile",
    }
}

pub(super) fn subscription_stage_label(stage: ModuleSubscriptionStage) -> &'static str {
    match stage {
        ModuleSubscriptionStage::PreAction => "pre_action",
        ModuleSubscriptionStage::PostAction => "post_action",
        ModuleSubscriptionStage::PostEvent => "post_event",
        ModuleSubscriptionStage::Tick => "tick",
    }
}

pub(super) fn module_kind_label(kind: &ModuleKind) -> &'static str {
    match kind {
        ModuleKind::Reducer => "reducer",
        ModuleKind::Pure => "pure",
    }
}

pub(super) fn module_role_label(role: &ModuleRole) -> &'static str {
    match role {
        ModuleRole::Rule => "rule",
        ModuleRole::Domain => "domain",
        ModuleRole::Gameplay => "gameplay",
        ModuleRole::Body => "body",
        ModuleRole::AgentInternal => "agent_internal",
    }
}
