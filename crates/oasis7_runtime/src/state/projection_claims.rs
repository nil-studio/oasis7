//! Atomic claim overlays and their canonical, order-preserving serialization.
use super::*;
impl<'a> WorldStateProjection<'a> {
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
        overlay: &'a crate::world::agent_claim_economic_publication::PreparedAgentClaimEconomic,
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
}
pub(super) fn serialize_claim_projection<S: SerializeStruct>(
    state: &WorldState,
    output: &mut S,
    agent_claim_terminal_overlay: Option<
        &crate::world::agent_claim_terminal_publication::PreparedAgentClaimTerminal,
    >,
    agent_claim_economic_overlay: Option<
        &crate::world::agent_claim_economic_publication::PreparedAgentClaimEconomic,
    >,
    agent_claim_light_lifecycle_overlay: Option<
        &crate::world::agent_claim_light_lifecycle_publication::PreparedAgentClaimLightLifecycle,
    >,
    starter_oc_claim_overlay: Option<
        &crate::world::starter_oc_claim_publication::PreparedStarterOcClaimed,
    >,
) -> Result<(), S::Error> {
    if let Some(overlay) = agent_claim_terminal_overlay {
        overlay.serialize_claims(state, output)?;
    } else if let Some(overlay) = agent_claim_economic_overlay {
        overlay.serialize_claims(state, output)?;
    } else if let Some(overlay) = agent_claim_light_lifecycle_overlay {
        overlay.serialize_claims(state, output)?;
    } else {
        output.serialize_field("agent_claims", &state.agent_claims)?;
    }
    if let Some(overlay) = agent_claim_economic_overlay {
        overlay.serialize_acquisition_sequences(state, output)?;
    } else if !state.agent_claim_acquisition_sequences.is_empty() {
        output.serialize_field(
            "agent_claim_acquisition_sequences",
            &state.agent_claim_acquisition_sequences,
        )?;
    }
    if let Some(overlay) = starter_oc_claim_overlay {
        overlay.serialize_claims(state, output)?;
    } else if !state.starter_oc_claims.is_empty() {
        output.serialize_field("starter_oc_claims", &state.starter_oc_claims)?;
    }
    Ok(())
}
