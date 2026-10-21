//! Canonical serialization entry points for base and projected world state.
use super::*;

impl Serialize for WorldState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_world_state(
            self, None, None, None, None, None, None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None, None, None, None, None, None, None,
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
            self.world_rule_overlay,
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
