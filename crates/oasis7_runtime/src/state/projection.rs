use super::body_projection::AgentMapProjection;
pub use super::body_projection::BodyOverlay;
use super::formal_retention_projection::{base_field_count, serialize_prefix};
use super::governance_identity_projection::{
    GovernanceIdentityProfileMapProjection, GovernanceIdentityProfileOverlay,
};
use super::module_admission_freeze::serialize_module_instance_projection;
use super::module_release_transition::ReleaseMapProjection;
use super::*;
use oasis7_agent_api::world_entity::ModuleVisualEntity;
use serde::Serialize;
use serde::ser::SerializeStruct;
/// Borrowed typed overlays preserving canonical serialization.
#[derive(Debug)]
pub struct WorldStateProjection<'a> {
    state: &'a WorldState,
    body_overlay: Option<BodyOverlay>,
    command_overlay: Option<CommandStateOverlay<'a>>,
    module_visual_entities_overlay:
        Option<&'a BTreeMap<String, ModuleVisualEntity>>,
    module_instance_overlay: Option<&'a module_instance_transition::PreparedModuleInstance>,
    module_release_overlay: Option<&'a module_release_transition::PreparedModuleRelease>,
    module_marketplace_overlay:
        Option<&'a module_marketplace_transition::PreparedModuleMarketplace>,
    governance_identity_profile_overlay: Option<GovernanceIdentityProfileOverlay>,
    governance_registry_overlay: Option<
        &'a crate::world::governance_registry_publication::PreparedGovernanceRegistryEvent,
    >,
    agent_intent_overlay:
        Option<&'a crate::world::agent_intent_publication::PreparedAgentIntent>,
    economy_data_overlay:
        Option<&'a crate::world::economy_data_publication::PreparedEconomyDataEvent>,
    economic_contract_overlay: Option<
        &'a crate::world::economic_contract_publication::PreparedEconomicContractEvent,
    >,
    alliance_war_overlay: Option<&'a crate::world::alliance_war_publication::PreparedAllianceWarEvent>,
    governance_meta_overlay: Option<&'a crate::world::governance_meta_publication::PreparedGovernanceMetaEvent>,
    core_policy_overlay: Option<&'a super::core_policy_transition::PreparedCorePolicyEvent>,
    pub(crate) industry_overlay: Option<&'a super::industry_transition::PreparedIndustryEvent>,
    power_redemption_overlay: Option<
        &'a crate::world::power_redemption_publication::PreparedPowerRedemptionEvent,
    >,
    node_points_settlement_overlay: Option<
        &'a crate::world::node_points_settlement_publication::PreparedNodePointsSettlement,
    >,
    main_token_monetary_overlay: Option<
        &'a crate::world::main_token_monetary_publication::PreparedMainTokenMonetaryEvent,
    >,
    main_token_governance_monetary_overlay: Option<
        &'a crate::world::main_token_governance_monetary_publication::PreparedMainTokenGovernanceMonetaryEvent,
    >,
    main_token_restricted_claim_overlay: Option<
        &'a crate::world::main_token_restricted_claim_publication::PreparedMainTokenRestrictedClaimEvent,
    >,
    starter_oc_claim_overlay: Option<
        &'a crate::world::starter_oc_claim_publication::PreparedStarterOcClaimed,
    >,
    agent_claim_light_lifecycle_overlay: Option<
        &'a crate::world::agent_claim_light_lifecycle_publication::PreparedAgentClaimLightLifecycle,
    >,
    agent_claim_economic_overlay: Option<&'a crate::world::agent_claim_economic_publication::PreparedAgentClaimEconomic>,
    agent_claim_terminal_overlay: Option<&'a crate::world::agent_claim_terminal_publication::PreparedAgentClaimTerminal>,
    product_validation_delivery_cursor: Option<&'a ProductValidationDeliveryCursor>,
    industry_history_overlay: Option<&'a super::industry_history_transition::PreparedIndustryHistoryEvent>,
}

impl<'a> WorldStateProjection<'a> {
    pub fn borrowed(state: &'a WorldState) -> Self {
        Self {
            state,
            body_overlay: None,
            command_overlay: None,
            module_visual_entities_overlay: None,
            module_instance_overlay: None,
            module_release_overlay: None,
            module_marketplace_overlay: None,
            governance_identity_profile_overlay: None,
            governance_registry_overlay: None,
            agent_intent_overlay: None,
            economy_data_overlay: None,
            economic_contract_overlay: None,
            alliance_war_overlay: None,
            governance_meta_overlay: None,
            core_policy_overlay: None,
            industry_overlay: None,
            power_redemption_overlay: None,
            node_points_settlement_overlay: None,
            main_token_monetary_overlay: None,
            main_token_governance_monetary_overlay: None,
            main_token_restricted_claim_overlay: None,
            starter_oc_claim_overlay: None,
            agent_claim_light_lifecycle_overlay: None,
            agent_claim_economic_overlay: None,
            agent_claim_terminal_overlay: None,
            product_validation_delivery_cursor: None,
            industry_history_overlay: None,
        }
    }

    pub(crate) fn with_industry_history_overlay(
        mut self,
        overlay: &'a super::industry_history_transition::PreparedIndustryHistoryEvent,
    ) -> Self {
        self.industry_history_overlay = Some(overlay);
        self
    }

    pub(crate) fn with_governance_registry_overlay(
        mut self,
        overlay: &'a crate::world::governance_registry_publication::PreparedGovernanceRegistryEvent,
    ) -> Self {
        self.governance_registry_overlay = Some(overlay);
        self
    }

    pub(crate) fn with_agent_intent_overlay(
        mut self,
        overlay: &'a crate::world::agent_intent_publication::PreparedAgentIntent,
    ) -> Self {
        self.agent_intent_overlay = Some(overlay);
        self
    }

    pub(crate) fn with_economy_data_overlay(
        mut self,
        overlay: &'a crate::world::economy_data_publication::PreparedEconomyDataEvent,
    ) -> Self {
        self.economy_data_overlay = Some(overlay);
        self
    }

    pub(crate) fn with_economic_contract_overlay(
        mut self,
        overlay: &'a crate::world::economic_contract_publication::PreparedEconomicContractEvent,
    ) -> Self {
        self.economic_contract_overlay = Some(overlay);
        self
    }

    pub(crate) fn with_alliance_war_overlay(
        mut self,
        overlay: &'a crate::world::alliance_war_publication::PreparedAllianceWarEvent,
    ) -> Self {
        self.alliance_war_overlay = Some(overlay);
        self
    }
    pub(crate) fn with_governance_meta_overlay(
        mut self,
        overlay: &'a crate::world::governance_meta_publication::PreparedGovernanceMetaEvent,
    ) -> Self {
        self.governance_meta_overlay = Some(overlay);
        self
    }
    pub(crate) fn with_core_policy_overlay(
        mut self,
        overlay: &'a super::core_policy_transition::PreparedCorePolicyEvent,
    ) -> Self {
        self.core_policy_overlay = Some(overlay);
        self
    }
    pub(crate) fn with_power_redemption_overlay(
        mut self,
        overlay: &'a crate::world::power_redemption_publication::PreparedPowerRedemptionEvent,
    ) -> Self {
        self.power_redemption_overlay = Some(overlay);
        self
    }

    pub(crate) fn with_node_points_settlement_overlay(
        mut self,
        overlay: &'a crate::world::node_points_settlement_publication::PreparedNodePointsSettlement,
    ) -> Self {
        self.node_points_settlement_overlay = Some(overlay);
        self
    }

    pub(crate) fn with_main_token_monetary_overlay(
        mut self,
        overlay: &'a crate::world::main_token_monetary_publication::PreparedMainTokenMonetaryEvent,
    ) -> Self {
        self.main_token_monetary_overlay = Some(overlay);
        self
    }

    pub(crate) fn with_main_token_governance_monetary_overlay(
        mut self,
        overlay: &'a crate::world::main_token_governance_monetary_publication::PreparedMainTokenGovernanceMonetaryEvent,
    ) -> Self {
        self.main_token_governance_monetary_overlay = Some(overlay);
        self
    }

    pub(crate) fn with_main_token_restricted_claim_overlay(
        mut self,
        overlay: &'a crate::world::main_token_restricted_claim_publication::PreparedMainTokenRestrictedClaimEvent,
    ) -> Self {
        self.main_token_restricted_claim_overlay = Some(overlay);
        self
    }

    pub(crate) fn with_starter_oc_claim_overlay(
        mut self,
        overlay: &'a crate::world::starter_oc_claim_publication::PreparedStarterOcClaimed,
    ) -> Self {
        self.starter_oc_claim_overlay = Some(overlay);
        self
    }

    pub(crate) fn with_agent_claim_light_lifecycle_overlay(
        mut self,
        overlay: &'a crate::world::agent_claim_light_lifecycle_publication::PreparedAgentClaimLightLifecycle,
    ) -> Self {
        self.agent_claim_light_lifecycle_overlay = Some(overlay);
        self
    }
    pub(crate) fn with_agent_claim_economic_overlay(
        mut self,
        overlay:&'a crate::world::agent_claim_economic_publication::PreparedAgentClaimEconomic,
    ) -> Self {
        self.agent_claim_economic_overlay = Some(overlay);
        self
    }
    pub(crate) fn with_agent_claim_terminal_overlay(
        mut self,
        overlay: &'a crate::world::agent_claim_terminal_publication::PreparedAgentClaimTerminal,
    ) -> Self {
        self.agent_claim_terminal_overlay = Some(overlay);
        self
    }

    pub fn with_body_overlay(mut self, body_overlay: BodyOverlay) -> Self {
        self.body_overlay = Some(body_overlay);
        self
    }

    pub(crate) fn with_product_validation_delivery_cursor(
        mut self,
        cursor: &'a ProductValidationDeliveryCursor,
    ) -> Self {
        self.product_validation_delivery_cursor = Some(cursor);
        self
    }

    pub(crate) fn with_module_instance_overlay(
        mut self,
        overlay: &'a module_instance_transition::PreparedModuleInstance,
    ) -> Self {
        self.module_instance_overlay = Some(overlay);
        self
    }

    pub(crate) fn with_module_release_overlay(
        mut self,
        overlay: &'a module_release_transition::PreparedModuleRelease,
    ) -> Self {
        self.module_release_overlay = Some(overlay);
        self
    }

    pub(crate) fn with_module_marketplace_overlay(
        mut self,
        overlay: &'a module_marketplace_transition::PreparedModuleMarketplace,
    ) -> Self {
        self.module_marketplace_overlay = Some(overlay);
        self
    }

    pub(crate) fn with_command_overlay(mut self, command_overlay: CommandStateOverlay<'a>) -> Self {
        self.command_overlay = Some(command_overlay);
        self
    }

    pub(crate) fn with_module_visual_entities_overlay(
        mut self,
        module_visual_entities: &'a BTreeMap<String, ModuleVisualEntity>,
    ) -> Self {
        self.module_visual_entities_overlay = Some(module_visual_entities);
        self
    }

    pub(crate) fn with_governance_identity_profile_overlay(
        mut self,
        target_agent_id: impl Into<String>,
        next_profile: GovernanceIdentityProfileState,
    ) -> Self {
        self.governance_identity_profile_overlay = Some(GovernanceIdentityProfileOverlay {
            target_agent_id: target_agent_id.into(),
            next_profile,
            allow_insert: false,
        });
        self
    }

    pub(crate) fn with_governance_identity_profile_insert_overlay(
        mut self,
        target_agent_id: impl Into<String>,
        next_profile: GovernanceIdentityProfileState,
    ) -> Self {
        self.governance_identity_profile_overlay = Some(GovernanceIdentityProfileOverlay {
            target_agent_id: target_agent_id.into(),
            next_profile,
            allow_insert: true,
        });
        self
    }
}

impl Serialize for WorldState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_world_state(
            self, None, None, None, None, None, None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None, None, None, None, None, None,
            serializer,
        )
    }
}

impl Serialize for WorldStateProjection<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        if let Some(overlay) = self.body_overlay.as_ref()
            && overlay.requires_body_target()
            && !self.state.agents.contains_key(&overlay.agent_id)
        {
            return Err(serde::ser::Error::custom(format!(
                "body overlay target agent not found: {}",
                overlay.agent_id
            )));
        }
        serialize_world_state(
            self.state,
            self.body_overlay.as_ref(),
            self.command_overlay.as_ref(),
            self.module_visual_entities_overlay,
            self.module_instance_overlay,
            self.module_release_overlay,
            self.module_marketplace_overlay,
            self.governance_identity_profile_overlay.as_ref(),
            self.governance_registry_overlay,
            self.agent_intent_overlay,
            self.economy_data_overlay,
            self.economic_contract_overlay,
            self.alliance_war_overlay,
            self.governance_meta_overlay,
            self.core_policy_overlay,
            self.industry_overlay,
            self.power_redemption_overlay,
            self.node_points_settlement_overlay,
            self.main_token_monetary_overlay,
            self.main_token_governance_monetary_overlay,
            self.main_token_restricted_claim_overlay,
            self.starter_oc_claim_overlay,
            self.agent_claim_light_lifecycle_overlay,
            self.agent_claim_economic_overlay,
            self.agent_claim_terminal_overlay,
            self.product_validation_delivery_cursor,
            self.industry_history_overlay,
            serializer,
        )
    }
}

#[expect(clippy::too_many_arguments)]
fn serialize_world_state<S>(
    state: &WorldState,
    body_overlay: Option<&BodyOverlay>,
    command_overlay: Option<&CommandStateOverlay<'_>>,
    module_visual_entities_overlay: Option<&BTreeMap<String, ModuleVisualEntity>>,
    module_instance_overlay: Option<&module_instance_transition::PreparedModuleInstance>,
    module_release_overlay: Option<&module_release_transition::PreparedModuleRelease>,
    module_marketplace_overlay: Option<&module_marketplace_transition::PreparedModuleMarketplace>,
    governance_identity_profile_overlay: Option<&GovernanceIdentityProfileOverlay>,
    governance_registry_overlay: Option<
        &crate::world::governance_registry_publication::PreparedGovernanceRegistryEvent,
    >,
    agent_intent_overlay: Option<
        &crate::world::agent_intent_publication::PreparedAgentIntent,
    >,
    economy_data_overlay: Option<
        &crate::world::economy_data_publication::PreparedEconomyDataEvent,
    >,
    economic_contract_overlay: Option<
        &crate::world::economic_contract_publication::PreparedEconomicContractEvent,
    >,
    alliance_war_overlay: Option<
        &crate::world::alliance_war_publication::PreparedAllianceWarEvent,
    >,
    governance_meta_overlay: Option<
        &crate::world::governance_meta_publication::PreparedGovernanceMetaEvent,
    >,
    core_policy_overlay: Option<&super::core_policy_transition::PreparedCorePolicyEvent>,
    industry_overlay: Option<&super::industry_transition::PreparedIndustryEvent>,
    power_redemption_overlay: Option<
        &crate::world::power_redemption_publication::PreparedPowerRedemptionEvent,
    >,
    node_points_settlement_overlay: Option<
        &crate::world::node_points_settlement_publication::PreparedNodePointsSettlement,
    >,
    main_token_monetary_overlay: Option<
        &crate::world::main_token_monetary_publication::PreparedMainTokenMonetaryEvent,
    >,
    main_token_governance_monetary_overlay: Option<
        &crate::world::main_token_governance_monetary_publication::PreparedMainTokenGovernanceMonetaryEvent,
    >,
    main_token_restricted_claim_overlay: Option<
        &crate::world::main_token_restricted_claim_publication::PreparedMainTokenRestrictedClaimEvent,
    >,
    starter_oc_claim_overlay: Option<
        &crate::world::starter_oc_claim_publication::PreparedStarterOcClaimed,
    >,
    agent_claim_light_lifecycle_overlay: Option<
        &crate::world::agent_claim_light_lifecycle_publication::PreparedAgentClaimLightLifecycle,
    >,
    agent_claim_economic_overlay: Option<
        &crate::world::agent_claim_economic_publication::PreparedAgentClaimEconomic,
    >,
    agent_claim_terminal_overlay: Option<
        &crate::world::agent_claim_terminal_publication::PreparedAgentClaimTerminal,
    >,
    product_validation_delivery_cursor: Option<&ProductValidationDeliveryCursor>,
    industry_history_overlay: Option<
        &super::industry_history_transition::PreparedIndustryHistoryEvent,
    >,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let WorldState {
        time: _,
        agents: _,
        agent_intent_ledger: _,
        resources: _,
        materials: _,
        material_ledgers: _,
        material_profiles: _,
        logistics_routes: _,
        completed_logistics_route_ids: _,
        completed_logistics_paths: _,
        settled_logistics_transit_ids: _,
        logistics_settlement_receipts: _,
        factory_production_failure_dispositions: _,
        direct_material_transfer_receipts: _,
        product_validation_receipts: _,
        product_profiles: _,
        latest_product_validation: _,
        recipe_profiles: _,
        factory_profiles: _,
        agent_location_authorities: _,
        location_anchors: _,
        factory_site_authorities: _,
        factory_construction_power_profiles: _,
        factories: _,
        retired_factory_ids: _,
        settled_factory_build_ids: _,
        factory_construction_receipts: _,
        product_validation_attempts: _,
        product_validation_delivery_cursor: _,
        pending_factory_builds: _,
        pending_recipe_jobs: _,
        settled_recipe_job_ids: _,
        next_industry_settlement_order: _,
        industry_settlement_orders: _,
        recipe_completion_receipts: _,
        factory_recycle_receipts: _,
        pending_material_transits: _,
        industry_progress: _,
        alliances: _,
        gameplay_policy: _,
        data_access_permissions: _,
        economic_contracts: _,
        agent_claims: _,
        starter_oc_claims: _,
        authenticated_collect_data_last_nonces: _,
        agent_claim_last_processed_epoch: _,
        contract_pair_last_success_settled_at: _,
        reputation_reward_window_started_at: _,
        reputation_reward_window_accumulated: _,
        reputation_scores: _,
        wars: _,
        governance_votes: _,
        governance_proposals: _,
        governance_identity_profiles: _,
        crises: _,
        meta_progress: _,
        module_states: _,
        module_artifact_owners: _,
        module_artifact_listings: _,
        module_artifact_bids: _,
        module_instances: _,
        module_release_requests: _,
        module_release_manifest_mappings: _,
        next_module_release_request_id: _,
        module_release_role_bindings: _,
        installed_module_targets: _,
        next_module_instance_id: _,
        next_module_market_order_id: _,
        next_module_market_sale_id: _,
        main_token_config: _,
        main_token_supply: _,
        main_token_balances: _,
        restricted_starter_claim_grants: _,
        main_token_genesis_buckets: _,
        main_token_epoch_issuance_records: _,
        main_token_treasury_balances: _,
        main_token_claim_nonces: _,
        main_token_transfer_nonces: _,
        main_token_scheduled_policy_updates: _,
        main_token_node_points_bridge_records: _,
        main_token_treasury_distribution_records: _,
        restricted_starter_claim_liveops_pool_top_up_records: _,
        reward_asset_config: _,
        node_asset_balances: _,
        protocol_power_reserve: _,
        reward_mint_records: _,
        node_redeem_nonces: _,
        system_order_pool_budgets: _,
        node_identity_bindings: _,
        node_main_token_account_bindings: _,
        governance_finality_signer_registry: _,
        governance_validator_admissions: _,
        governance_main_token_controller_registry: _,
        reward_signature_governance_policy: _,
        ..
    } = state;

    let field_count = base_field_count(95, state, module_instance_overlay)
        - usize::from(
            state.agent_intent_ledger.is_empty()
                && agent_intent_overlay.is_none_or(|overlay| overlay.ledger_updates.is_empty()),
        )
        - usize::from(state.latest_product_validation.is_none() && governance_meta_overlay.is_none_or(|v| !matches!(v, crate::world::governance_meta_publication::PreparedGovernanceMetaEvent::Product(_))))
        - usize::from(state.starter_oc_claims.is_empty() && starter_oc_claim_overlay.is_none())
        - usize::from(
            state.authenticated_collect_data_last_nonces.is_empty()
                && economy_data_overlay.is_none_or(|overlay| !overlay.has_projected_nonces(state)),
        )
        - usize::from(state.product_validation_receipts.is_empty() && industry_history_overlay.is_none())
        - usize::from(state.factory_construction_receipts.is_empty() && industry_overlay.is_none_or(|v| !v.has_construction_receipt()))
        - usize::from(state.product_validation_attempts.is_empty() && industry_history_overlay.is_none())
        - usize::from(state.recipe_completion_receipts.is_empty() && industry_overlay.is_none_or(|v| !v.has_completion_receipt()))
        - usize::from(state.factory_recycle_receipts.is_empty() && industry_overlay.is_none_or(|v| !v.has_recycle_receipt()))
        - usize::from(
            module_visual_entities_overlay
                .unwrap_or(&state.module_visual_entities)
                .is_empty(),
        );
    let mut output = serializer.serialize_struct("WorldState", field_count)?;
    serialize_prefix(&mut output, state)?;
    if let Some(overlay) = agent_claim_terminal_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = agent_claim_economic_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = agent_claim_light_lifecycle_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = starter_oc_claim_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = main_token_restricted_claim_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = main_token_monetary_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = power_redemption_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = economy_data_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = core_policy_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = industry_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = industry_history_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = governance_meta_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = alliance_war_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(overlay) = economic_contract_overlay {
        overlay.serialize_agents(state, &mut output)?;
    } else if let Some(command_overlay) = command_overlay {
        output.serialize_field(
            "agents",
            &CommandAgentMapProjection {
                agents: &state.agents,
                updates: command_overlay.agents,
            },
        )?;
    } else {
        output.serialize_field(
            "agents",
            &AgentMapProjection {
                agents: &state.agents,
                body_overlay,
            },
        )?;
    }
    if let Some(overlay) = agent_intent_overlay
        && (!state.agent_intent_ledger.is_empty() || !overlay.ledger_updates.is_empty())
    {
        output.serialize_field(
            "agent_intent_ledger",
            &ReleaseMapProjection {
                base: &state.agent_intent_ledger,
                updates: &overlay.ledger_updates,
            },
        )?;
    } else if !state.agent_intent_ledger.is_empty() {
        output.serialize_field("agent_intent_ledger", &state.agent_intent_ledger)?;
    }
    if let Some(command_overlay) = command_overlay {
        output.serialize_field(
            "resources",
            &CommandResourceMapProjection {
                resources: &state.resources,
                updates: command_overlay.resources,
            },
        )?;
    } else if let Some(overlay) = economic_contract_overlay {
        overlay.serialize_resources(state, &mut output)?;
    } else {
        output.serialize_field("resources", &state.resources)?;
    }
    if let Some(overlay) = core_policy_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = industry_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = agent_claim_terminal_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = agent_claim_economic_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = agent_claim_light_lifecycle_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = starter_oc_claim_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = main_token_restricted_claim_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = main_token_governance_monetary_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = main_token_monetary_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = node_points_settlement_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = power_redemption_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = economy_data_overlay {
        overlay.serialize_material_fields(state, &mut output)?;
    } else if let Some(overlay) = module_instance_overlay {
        overlay.serialize_material_fields(state, &mut output)?;
    } else if let Some(overlay) = module_release_overlay {
        overlay.serialize_material_fields(state, &mut output)?;
    } else if let Some(overlay) = module_marketplace_overlay {
        overlay.serialize_material_fields(state, &mut output)?;
    } else if let Some(overlay) = alliance_war_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else if let Some(overlay) = economic_contract_overlay {
        overlay.serialize_materials(state, &mut output)?;
    } else {
        output.serialize_field("materials", &state.materials)?;
        output.serialize_field("material_ledgers", &state.material_ledgers)?;
    }
    if let Some(overlay) = core_policy_overlay {
        overlay.serialize_profile(state, &mut output)?;
    } else {
        output.serialize_field("material_profiles", &state.material_profiles)?;
    }
    if let Some(overlay) = industry_overlay {
        overlay.serialize_logistics(state, &mut output)?;
    } else {
        output.serialize_field("logistics_routes", &state.logistics_routes)?;
        output.serialize_field(
            "completed_logistics_route_ids",
            &state.completed_logistics_route_ids,
        )?;
        output.serialize_field(
            "completed_logistics_paths",
            &state.completed_logistics_paths,
        )?;
        output.serialize_field(
            "settled_logistics_transit_ids",
            &state.settled_logistics_transit_ids,
        )?;
        output.serialize_field(
            "logistics_settlement_receipts",
            &state.logistics_settlement_receipts,
        )?;
        output.serialize_field(
            "direct_material_transfer_receipts",
            &state.direct_material_transfer_receipts,
        )?;
    }
    if let Some(overlay) = industry_overlay {
        overlay.serialize_failure_dispositions(state, &mut output)?;
    } else {
        output.serialize_field(
            "factory_production_failure_dispositions",
            &state.factory_production_failure_dispositions,
        )?;
    }
    if let Some(overlay) = industry_history_overlay {
        overlay.serialize_receipts(state, &mut output)?;
    } else if !state.product_validation_receipts.is_empty() {
        output.serialize_field(
            "product_validation_receipts",
            &state.product_validation_receipts,
        )?;
    }
    if let Some(overlay) = module_release_overlay {
        output.serialize_field(
            "product_profiles",
            &ReleaseMapProjection {
                base: &state.product_profiles,
                updates: &overlay.products,
            },
        )?;
    } else {
        output.serialize_field("product_profiles", &state.product_profiles)?;
    }
    if let Some(overlay) = governance_meta_overlay {
        overlay.serialize_product(state, &mut output)?;
    } else if let Some(overlay) = industry_history_overlay {
        overlay.serialize_latest(state, &mut output)?;
    } else if state.latest_product_validation.is_some() {
        output.serialize_field(
            "latest_product_validation",
            &state.latest_product_validation,
        )?;
    }
    if let Some(overlay) = module_release_overlay {
        output.serialize_field(
            "recipe_profiles",
            &ReleaseMapProjection {
                base: &state.recipe_profiles,
                updates: &overlay.recipes,
            },
        )?;
        output.serialize_field(
            "factory_profiles",
            &ReleaseMapProjection {
                base: &state.factory_profiles,
                updates: &overlay.factories,
            },
        )?;
    } else {
        output.serialize_field("recipe_profiles", &state.recipe_profiles)?;
        output.serialize_field("factory_profiles", &state.factory_profiles)?;
    }
    if let Some(overlay) = industry_history_overlay {
        overlay.serialize_authorities(state, &mut output)?;
    } else {
        output.serialize_field(
            "agent_location_authorities",
            &state.agent_location_authorities,
        )?;
        output.serialize_field("location_anchors", &state.location_anchors)?;
        output.serialize_field("factory_site_authorities", &state.factory_site_authorities)?;
        output.serialize_field(
            "factory_construction_power_profiles",
            &state.factory_construction_power_profiles,
        )?;
    }
    if let Some(overlay) = industry_overlay {
        overlay.serialize_factory_fields(state, &mut output)?;
    } else {
        output.serialize_field("factories", &state.factories)?;
        output.serialize_field("retired_factory_ids", &state.retired_factory_ids)?;
        output.serialize_field(
            "settled_factory_build_ids",
            &state.settled_factory_build_ids,
        )?;
        output.serialize_field("pending_factory_builds", &state.pending_factory_builds)?;
        output.serialize_field("pending_recipe_jobs", &state.pending_recipe_jobs)?;
        output.serialize_field("settled_recipe_job_ids", &state.settled_recipe_job_ids)?;
    }
    if let Some(overlay) = industry_overlay {
        overlay.serialize_construction_receipts(state, &mut output)?;
    } else if !state.factory_construction_receipts.is_empty() {
        output.serialize_field(
            "factory_construction_receipts",
            &state.factory_construction_receipts,
        )?;
    }
    if let Some(overlay) = industry_history_overlay {
        overlay.serialize_attempts(state, &mut output)?;
    } else if !state.product_validation_attempts.is_empty() {
        output.serialize_field(
            "product_validation_attempts",
            &state.product_validation_attempts,
        )?;
    }
    output.serialize_field(
        "product_validation_delivery_cursor",
        &product_validation_delivery_cursor.unwrap_or(&state.product_validation_delivery_cursor),
    )?;
    if let Some(overlay) = industry_overlay {
        overlay.serialize_settlement_history(state, &mut output)?;
    } else {
        output.serialize_field(
            "next_industry_settlement_order",
            &state.next_industry_settlement_order,
        )?;
        output.serialize_field(
            "industry_settlement_orders",
            &state.industry_settlement_orders,
        )?;
    }
    if let Some(overlay) = industry_overlay {
        overlay.serialize_terminal_receipts(state, &mut output)?;
    } else {
        if !state.recipe_completion_receipts.is_empty() {
            output.serialize_field(
                "recipe_completion_receipts",
                &state.recipe_completion_receipts,
            )?;
        }
        if !state.factory_recycle_receipts.is_empty() {
            output.serialize_field("factory_recycle_receipts", &state.factory_recycle_receipts)?;
        }
    }
    if let Some(overlay) = industry_overlay {
        overlay.serialize_pending_and_progress(state, &mut output)?;
    } else {
        output.serialize_field(
            "pending_material_transits",
            &state.pending_material_transits,
        )?;
        output.serialize_field(
            "industry_progress",
            core_policy_overlay.map_or(&state.industry_progress, |overlay| {
                overlay.projected_progress(state)
            }),
        )?;
    }
    if let Some(overlay) = alliance_war_overlay {
        overlay.serialize_alliances(state, &mut output)?;
    } else {
        output.serialize_field("alliances", &state.alliances)?;
    }
    output.serialize_field(
        "gameplay_policy",
        core_policy_overlay.map_or(&state.gameplay_policy, |overlay| {
            overlay.projected_policy(state)
        }),
    )?;
    if let Some(overlay) = economy_data_overlay {
        overlay.serialize_permissions(state, &mut output)?;
    } else {
        output.serialize_field("data_access_permissions", &state.data_access_permissions)?;
    }
    if let Some(overlay) = economic_contract_overlay {
        overlay.serialize_contracts(state, &mut output)?;
    } else {
        output.serialize_field("economic_contracts", &state.economic_contracts)?;
    }
    if let Some(overlay) = agent_claim_terminal_overlay {
        overlay.serialize_claims(state, &mut output)?;
    } else if let Some(overlay) = agent_claim_economic_overlay {
        overlay.serialize_claims(state, &mut output)?;
    } else if let Some(overlay) = agent_claim_light_lifecycle_overlay {
        overlay.serialize_claims(state, &mut output)?;
    } else {
        output.serialize_field("agent_claims", &state.agent_claims)?;
    }
    if let Some(overlay) = starter_oc_claim_overlay {
        overlay.serialize_claims(state, &mut output)?;
    } else if !state.starter_oc_claims.is_empty() {
        output.serialize_field("starter_oc_claims", &state.starter_oc_claims)?;
    }
    if let Some(overlay) = economy_data_overlay
        && overlay.has_projected_nonces(state)
    {
        overlay.serialize_nonces(state, &mut output)?;
    } else if !state.authenticated_collect_data_last_nonces.is_empty() {
        output.serialize_field(
            "authenticated_collect_data_last_nonces",
            &state.authenticated_collect_data_last_nonces,
        )?;
    }
    if let Some(overlay) = agent_claim_terminal_overlay {
        overlay.serialize_last_epoch(&mut output)?;
    } else if let Some(overlay) = agent_claim_economic_overlay {
        overlay.serialize_last_epoch(&mut output)?;
    } else {
        output.serialize_field(
            "agent_claim_last_processed_epoch",
            &state.agent_claim_last_processed_epoch,
        )?;
    }
    if let Some(overlay) = economic_contract_overlay {
        overlay.serialize_reputation(state, &mut output)?;
    } else {
        output.serialize_field(
            "contract_pair_last_success_settled_at",
            &state.contract_pair_last_success_settled_at,
        )?;
        output.serialize_field(
            "reputation_reward_window_started_at",
            &state.reputation_reward_window_started_at,
        )?;
        output.serialize_field(
            "reputation_reward_window_accumulated",
            &state.reputation_reward_window_accumulated,
        )?;
        if let Some(overlay) = alliance_war_overlay {
            overlay.serialize_reputation(state, &mut output)?;
        } else {
            output.serialize_field("reputation_scores", &state.reputation_scores)?;
        }
    }
    if let Some(overlay) = alliance_war_overlay {
        overlay.serialize_wars(state, &mut output)?;
    } else {
        output.serialize_field("wars", &state.wars)?;
    }
    if let Some(overlay) = governance_meta_overlay {
        overlay.serialize_governance(state, &mut output)?;
    } else {
        output.serialize_field("governance_votes", &state.governance_votes)?;
        output.serialize_field("governance_proposals", &state.governance_proposals)?;
    }
    if let Some(overlay) = governance_identity_profile_overlay {
        if !overlay.allow_insert
            && !state
                .governance_identity_profiles
                .contains_key(overlay.target_agent_id.as_str())
        {
            return Err(serde::ser::Error::custom(format!(
                "governance identity profile overlay target not found: {}",
                overlay.target_agent_id
            )));
        }
        output.serialize_field(
            "governance_identity_profiles",
            &GovernanceIdentityProfileMapProjection {
                profiles: &state.governance_identity_profiles,
                overlay,
            },
        )?;
    } else {
        output.serialize_field(
            "governance_identity_profiles",
            &state.governance_identity_profiles,
        )?;
    }
    if let Some(overlay) = governance_meta_overlay {
        overlay.serialize_crises(state, &mut output)?;
        overlay.serialize_meta(state, &mut output)?;
    } else {
        output.serialize_field("crises", &state.crises)?;
        output.serialize_field("meta_progress", &state.meta_progress)?;
    }
    if let Some(command_overlay) = command_overlay {
        output.serialize_field(
            "module_states",
            &CommandModuleStateMapProjection {
                module_states: &state.module_states,
                updates: command_overlay.module_states,
            },
        )?;
    } else {
        output.serialize_field("module_states", &state.module_states)?;
    }
    let module_visual_entities =
        module_visual_entities_overlay.unwrap_or(&state.module_visual_entities);
    if !module_visual_entities.is_empty() {
        output.serialize_field("module_visual_entities", module_visual_entities)?;
    }
    if let Some(overlay) = module_marketplace_overlay {
        overlay.serialize_market_fields(state, &mut output)?;
    } else {
        output.serialize_field("module_artifact_owners", &state.module_artifact_owners)?;
        output.serialize_field("module_artifact_listings", &state.module_artifact_listings)?;
        output.serialize_field("module_artifact_bids", &state.module_artifact_bids)?;
    }
    serialize_module_instance_projection(state, module_instance_overlay, &mut output)?;
    if let Some(overlay) = module_release_overlay {
        output.serialize_field(
            "module_release_requests",
            &ReleaseMapProjection {
                base: &state.module_release_requests,
                updates: &overlay.requests,
            },
        )?;
        output.serialize_field(
            "module_release_manifest_mappings",
            &ReleaseMapProjection {
                base: &state.module_release_manifest_mappings,
                updates: &overlay.mappings,
            },
        )?;
    } else {
        output.serialize_field("module_release_requests", &state.module_release_requests)?;
        output.serialize_field(
            "module_release_manifest_mappings",
            &state.module_release_manifest_mappings,
        )?;
    }
    output.serialize_field(
        "next_module_release_request_id",
        &module_release_overlay
            .and_then(|overlay| overlay.next_request_id)
            .unwrap_or(state.next_module_release_request_id),
    )?;
    if let Some(overlay) = module_release_overlay {
        output.serialize_field(
            "module_release_role_bindings",
            &module_release_transition::ReleaseOptionalMapProjection {
                base: &state.module_release_role_bindings,
                updates: &overlay.role_bindings,
            },
        )?;
    } else {
        output.serialize_field(
            "module_release_role_bindings",
            &state.module_release_role_bindings,
        )?;
    }
    if let Some(overlay) = module_instance_overlay {
        overlay.serialize_target_fields(state, &mut output)?;
    } else {
        output.serialize_field("installed_module_targets", &state.installed_module_targets)?;
        output.serialize_field("next_module_instance_id", &state.next_module_instance_id)?;
    }
    if let Some(overlay) = module_marketplace_overlay {
        overlay.serialize_counter_fields(&mut output)?;
    } else {
        output.serialize_field(
            "next_module_market_order_id",
            &state.next_module_market_order_id,
        )?;
        output.serialize_field(
            "next_module_market_sale_id",
            &state.next_module_market_sale_id,
        )?;
    }
    output.serialize_field("main_token_config", &state.main_token_config)?;
    if let Some(overlay) = agent_claim_terminal_overlay {
        overlay.serialize_token(state, &mut output)?;
    } else if let Some(overlay) = agent_claim_economic_overlay {
        overlay.serialize_token(state, &mut output)?;
    } else if let Some(overlay) = starter_oc_claim_overlay {
        overlay.serialize_supply_balances(state, &mut output)?;
    } else if let Some(overlay) = main_token_restricted_claim_overlay {
        overlay.serialize_supply_balances(state, &mut output)?;
    } else if let Some(overlay) = main_token_governance_monetary_overlay {
        overlay.serialize_supply_balances(state, &mut output)?;
    } else if let Some(overlay) = main_token_monetary_overlay {
        overlay.serialize_supply_balances(state, &mut output)?;
    } else if let Some(overlay) = node_points_settlement_overlay {
        overlay.serialize_main_token_accounts(state, &mut output)?;
    } else {
        output.serialize_field("main_token_supply", &state.main_token_supply)?;
        output.serialize_field("main_token_balances", &state.main_token_balances)?;
    }
    if let Some(overlay) = main_token_restricted_claim_overlay {
        overlay.serialize_grants(state, &mut output)?;
    } else {
        output.serialize_field(
            "restricted_starter_claim_grants",
            &state.restricted_starter_claim_grants,
        )?;
    }
    if let Some(overlay) = main_token_monetary_overlay {
        overlay.serialize_buckets(state, &mut output)?;
        overlay.serialize_issuance(state, &mut output)?;
    } else {
        output.serialize_field(
            "main_token_genesis_buckets",
            &state.main_token_genesis_buckets,
        )?;
        output.serialize_field(
            "main_token_epoch_issuance_records",
            &state.main_token_epoch_issuance_records,
        )?;
    }
    if let Some(overlay) = agent_claim_terminal_overlay {
        overlay.serialize_treasury(state, &mut output)?;
    } else if let Some(overlay) = agent_claim_economic_overlay {
        overlay.serialize_treasury(state, &mut output)?;
    } else if let Some(overlay) = starter_oc_claim_overlay {
        overlay.serialize_treasury(state, &mut output)?;
    } else if let Some(overlay) = main_token_restricted_claim_overlay {
        overlay.serialize_treasury(state, &mut output)?;
    } else if let Some(overlay) = main_token_governance_monetary_overlay {
        overlay.serialize_treasury(state, &mut output)?;
    } else if let Some(overlay) = main_token_monetary_overlay {
        overlay.serialize_treasury(state, &mut output)?;
    } else if let Some(overlay) = node_points_settlement_overlay {
        overlay.serialize_treasury(state, &mut output)?;
    } else {
        output.serialize_field(
            "main_token_treasury_balances",
            &state.main_token_treasury_balances,
        )?;
    }
    if let Some(overlay) = main_token_monetary_overlay {
        overlay.serialize_claim_nonces(state, &mut output)?;
        overlay.serialize_transfer_nonces(state, &mut output)?;
    } else {
        output.serialize_field("main_token_claim_nonces", &state.main_token_claim_nonces)?;
        output.serialize_field(
            "main_token_transfer_nonces",
            &state.main_token_transfer_nonces,
        )?;
    }
    if let Some(overlay) = main_token_governance_monetary_overlay {
        overlay.serialize_scheduled(state, &mut output)?;
    } else {
        output.serialize_field(
            "main_token_scheduled_policy_updates",
            &state.main_token_scheduled_policy_updates,
        )?;
    }
    if let Some(overlay) = node_points_settlement_overlay {
        overlay.serialize_bridge(state, &mut output)?;
    } else {
        output.serialize_field(
            "main_token_node_points_bridge_records",
            &state.main_token_node_points_bridge_records,
        )?;
    }
    if let Some(overlay) = main_token_governance_monetary_overlay {
        overlay.serialize_distributions(state, &mut output)?;
    } else {
        output.serialize_field(
            "main_token_treasury_distribution_records",
            &state.main_token_treasury_distribution_records,
        )?;
    }
    if let Some(overlay) = main_token_restricted_claim_overlay {
        overlay.serialize_topups(state, &mut output)?;
    } else {
        output.serialize_field(
            "restricted_starter_claim_liveops_pool_top_up_records",
            &state.restricted_starter_claim_liveops_pool_top_up_records,
        )?;
    }
    output.serialize_field("reward_asset_config", &state.reward_asset_config)?;
    if let Some(overlay) = node_points_settlement_overlay {
        overlay.serialize_node_balances(state, &mut output)?;
        output.serialize_field("protocol_power_reserve", &state.protocol_power_reserve)?;
    } else if let Some(overlay) = power_redemption_overlay {
        overlay.serialize_node_balances(state, &mut output)?;
        overlay.serialize_reserve(state, &mut output)?;
    } else {
        output.serialize_field("node_asset_balances", &state.node_asset_balances)?;
        output.serialize_field("protocol_power_reserve", &state.protocol_power_reserve)?;
    }
    if let Some(overlay) = node_points_settlement_overlay {
        overlay.serialize_reward_mints(&mut output)?;
    } else {
        output.serialize_field("reward_mint_records", &state.reward_mint_records)?;
    }
    if let Some(overlay) = power_redemption_overlay {
        overlay.serialize_nonces(state, &mut output)?;
    } else {
        output.serialize_field("node_redeem_nonces", &state.node_redeem_nonces)?;
    }
    if let Some(overlay) = node_points_settlement_overlay {
        overlay.serialize_budgets(state, &mut output)?;
    } else {
        output.serialize_field(
            "system_order_pool_budgets",
            &state.system_order_pool_budgets,
        )?;
    }
    output.serialize_field(
        "node_identity_bindings",
        governance_registry_overlay.map_or(&state.node_identity_bindings, |o| &o.identity_bindings),
    )?;
    output.serialize_field(
        "node_main_token_account_bindings",
        governance_registry_overlay.map_or(&state.node_main_token_account_bindings, |o| {
            &o.account_bindings
        }),
    )?;
    output.serialize_field(
        "governance_finality_signer_registry",
        &state.governance_finality_signer_registry,
    )?;
    output.serialize_field(
        "governance_validator_admissions",
        governance_registry_overlay
            .map_or(&state.governance_validator_admissions, |o| &o.admissions),
    )?;
    output.serialize_field(
        "governance_main_token_controller_registry",
        governance_registry_overlay.map_or(&state.governance_main_token_controller_registry, |o| {
            &o.controller_registry
        }),
    )?;
    output.serialize_field(
        "reward_signature_governance_policy",
        &state.reward_signature_governance_policy,
    )?;
    output.end()
}
