mod governance;
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
    world_rule_overlay: Option<&'a crate::world_rules::PreparedWorldRule>,
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
    pub industry_overlay: Option<&'a super::industry_transition::PreparedIndustryEvent>,
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
    pub fn with_world_rule_overlay(
        mut self,
        overlay: &'a crate::world_rules::PreparedWorldRule,
    ) -> Self {
        self.world_rule_overlay = Some(overlay);
        self.agent_claim_economic_overlay = overlay.claim.as_ref();
        self.industry_overlay = overlay.industry.as_ref();
        self
    }
    pub fn borrowed(state: &'a WorldState) -> Self {
        Self {
            state,
            world_rule_overlay: None,
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

    pub fn with_industry_history_overlay(
        mut self,
        overlay: &'a super::industry_history_transition::PreparedIndustryHistoryEvent,
    ) -> Self {
        self.industry_history_overlay = Some(overlay);
        self
    }

    pub fn with_governance_registry_overlay(
        mut self,
        overlay: &'a crate::world::governance_registry_publication::PreparedGovernanceRegistryEvent,
    ) -> Self {
        self.governance_registry_overlay = Some(overlay);
        self
    }

    pub fn with_agent_intent_overlay(
        mut self,
        overlay: &'a crate::world::agent_intent_publication::PreparedAgentIntent,
    ) -> Self {
        self.agent_intent_overlay = Some(overlay);
        self
    }

    pub fn with_economy_data_overlay(
        mut self,
        overlay: &'a crate::world::economy_data_publication::PreparedEconomyDataEvent,
    ) -> Self {
        self.economy_data_overlay = Some(overlay);
        self
    }

    pub fn with_economic_contract_overlay(
        mut self,
        overlay: &'a crate::world::economic_contract_publication::PreparedEconomicContractEvent,
    ) -> Self {
        self.economic_contract_overlay = Some(overlay);
        self
    }

    pub fn with_alliance_war_overlay(
        mut self,
        overlay: &'a crate::world::alliance_war_publication::PreparedAllianceWarEvent,
    ) -> Self {
        self.alliance_war_overlay = Some(overlay);
        self
    }
    pub fn with_governance_meta_overlay(
        mut self,
        overlay: &'a crate::world::governance_meta_publication::PreparedGovernanceMetaEvent,
    ) -> Self {
        self.governance_meta_overlay = Some(overlay);
        self
    }
    pub fn with_core_policy_overlay(
        mut self,
        overlay: &'a super::core_policy_transition::PreparedCorePolicyEvent,
    ) -> Self {
        self.core_policy_overlay = Some(overlay);
        self
    }
    pub fn with_power_redemption_overlay(
        mut self,
        overlay: &'a crate::world::power_redemption_publication::PreparedPowerRedemptionEvent,
    ) -> Self {
        self.power_redemption_overlay = Some(overlay);
        self
    }

    pub fn with_node_points_settlement_overlay(
        mut self,
        overlay: &'a crate::world::node_points_settlement_publication::PreparedNodePointsSettlement,
    ) -> Self {
        self.node_points_settlement_overlay = Some(overlay);
        self
    }

    pub fn with_main_token_monetary_overlay(
        mut self,
        overlay: &'a crate::world::main_token_monetary_publication::PreparedMainTokenMonetaryEvent,
    ) -> Self {
        self.main_token_monetary_overlay = Some(overlay);
        self
    }

    pub fn with_main_token_governance_monetary_overlay(
        mut self,
        overlay: &'a crate::world::main_token_governance_monetary_publication::PreparedMainTokenGovernanceMonetaryEvent,
    ) -> Self {
        self.main_token_governance_monetary_overlay = Some(overlay);
        self
    }

    pub fn with_main_token_restricted_claim_overlay(
        mut self,
        overlay: &'a crate::world::main_token_restricted_claim_publication::PreparedMainTokenRestrictedClaimEvent,
    ) -> Self {
        self.main_token_restricted_claim_overlay = Some(overlay);
        self
    }

    pub fn with_body_overlay(mut self, body_overlay: BodyOverlay) -> Self {
        self.body_overlay = Some(body_overlay);
        self
    }

    pub fn with_product_validation_delivery_cursor(
        mut self,
        cursor: &'a ProductValidationDeliveryCursor,
    ) -> Self {
        self.product_validation_delivery_cursor = Some(cursor);
        self
    }

    pub fn with_module_instance_overlay(
        mut self,
        overlay: &'a module_instance_transition::PreparedModuleInstance,
    ) -> Self {
        self.module_instance_overlay = Some(overlay);
        self
    }

    pub fn with_module_release_overlay(
        mut self,
        overlay: &'a module_release_transition::PreparedModuleRelease,
    ) -> Self {
        self.module_release_overlay = Some(overlay);
        self
    }

    pub fn with_module_marketplace_overlay(
        mut self,
        overlay: &'a module_marketplace_transition::PreparedModuleMarketplace,
    ) -> Self {
        self.module_marketplace_overlay = Some(overlay);
        self
    }

    pub fn with_command_overlay(mut self, command_overlay: CommandStateOverlay<'a>) -> Self {
        self.command_overlay = Some(command_overlay);
        self
    }

    pub fn with_module_visual_entities_overlay(
        mut self,
        module_visual_entities: &'a BTreeMap<String, ModuleVisualEntity>,
    ) -> Self {
        self.module_visual_entities_overlay = Some(module_visual_entities);
        self
    }

    pub fn with_governance_identity_profile_overlay(
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

    pub fn with_governance_identity_profile_insert_overlay(
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

#[path = "projection_claims.rs"]
mod claims;
#[path = "projection_serialization.rs"]
mod serialization;

#[path = "projection/emitter.rs"]
mod emitter;
pub(crate) use emitter::SlotOutput as OriginalStateSlotOutputV1;
pub use emitter::{
    OriginalStateFieldEmissionV1, OriginalStateFieldSlotEventV1, OriginalStateFieldSlotV1,
};

#[expect(clippy::too_many_arguments)]
fn serialize_world_state<S>(
    state: &WorldState,
    world_rule_overlay: Option<&crate::world_rules::PreparedWorldRule>,
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
    agent_intent_overlay: Option<&crate::world::agent_intent_publication::PreparedAgentIntent>,
    economy_data_overlay: Option<&crate::world::economy_data_publication::PreparedEconomyDataEvent>,
    economic_contract_overlay: Option<
        &crate::world::economic_contract_publication::PreparedEconomicContractEvent,
    >,
    alliance_war_overlay: Option<&crate::world::alliance_war_publication::PreparedAllianceWarEvent>,
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
    emitter::emit_world_state_fields_v1(
        state,
        world_rule_overlay,
        body_overlay,
        command_overlay,
        module_visual_entities_overlay,
        module_instance_overlay,
        module_release_overlay,
        module_marketplace_overlay,
        governance_identity_profile_overlay,
        governance_registry_overlay,
        agent_intent_overlay,
        economy_data_overlay,
        economic_contract_overlay,
        alliance_war_overlay,
        governance_meta_overlay,
        core_policy_overlay,
        industry_overlay,
        power_redemption_overlay,
        node_points_settlement_overlay,
        main_token_monetary_overlay,
        main_token_governance_monetary_overlay,
        main_token_restricted_claim_overlay,
        starter_oc_claim_overlay,
        agent_claim_light_lifecycle_overlay,
        agent_claim_economic_overlay,
        agent_claim_terminal_overlay,
        product_validation_delivery_cursor,
        industry_history_overlay,
        |field_count| serializer.serialize_struct("WorldState", field_count),
    )
}
