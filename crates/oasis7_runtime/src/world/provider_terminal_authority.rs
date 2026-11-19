//! Expiry is a typed reason to deny fresh work, never a new grant.
use super::{World, WorldError};
use oasis7_wasm_abi::{CapabilityGrantV2, CapabilitySubject};

#[path="provider_terminal_authority_origin.rs"]
pub(in crate::world) mod origin;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderCapabilityAvailability {
    Live,
    ExpiredOnly { grant_ids: Vec<String> },
}
fn deny(reason: &str) -> WorldError {
    WorldError::CapabilityAuthorizationDenied {
        reason: reason.into(),
    }
}
impl World {
    pub(crate) fn has_local_provider_origin(&self, agent: &str) -> Result<bool, WorldError> {
        origin::legacy(self,agent)
    }

    pub(super) fn require_live_local_provider_admission(
        &self,
        agent: &str,
        supplied: Option<&crate::CapabilityInvocationContext>,
        require_invocation: bool,
    ) -> Result<(), WorldError> {
        let local = self.has_local_provider_origin(agent)?;
        if !local {
            return Ok(());
        }
        if require_invocation && supplied.is_none() {
            return Err(deny("fresh local Prefix original invocation missing"));
        }
        if self.local_provider_capability_availability(agent)?
            != ProviderCapabilityAvailability::Live
        {
            return Err(deny(
                "expired local provider grant cannot admit fresh Reserve or Prefix",
            ));
        }
        let binding = self.current_cognition_runtime_binding()?;
        let identity = self
            .current_agent_identity(agent)
            .ok_or_else(|| deny("fresh local provider identity missing"))?;
        let economy = self.cognition_economy()?;
        if economy
            .provisions
            .values()
            .filter(|record| {
                let r = &record.request;
                r.owner_binding == identity.owner_binding
                    && r.owner_generation == identity.generation
                    && r.world_id == binding.world_id
                    && r.branch_id == binding.branch_id
                    && r.reorg_epoch == binding.reorg_epoch
            })
            .count()
            != 1
        {
            return Err(deny("fresh local provider provision missing or ambiguous"));
        }
        let context = self
            .canonical_agent_service_context(agent)
            .map_err(|_| deny("fresh local provider capability context unavailable"))?;
        if let Some(original) = supplied {
            let expected = &context.capability_invocation_context;
            if original.grant_id != expected.grant_id
                || original.subject != expected.subject
                || original.presenter != expected.presenter
                || original.audience != expected.audience
                || original.module_id != expected.module_id
                || original.module_version != expected.module_version
            {
                return Err(deny("fresh local provider request grant binding differs"));
            }
        }
        Ok(())
    }

    /// Strict local-provider fallback. No error text determines availability.
    /// All non-time checks, including current revocation, precede ExpiredOnly.
    pub fn local_provider_capability_availability(
        &self,
        agent: &str,
    ) -> Result<ProviderCapabilityAvailability, WorldError> {
        super::provider_terminal_authority_rules::local_provider_capability_availability_from_sources_v1(
            &super::provider_terminal_authority_rules::LegacyTerminalSourcesV1(self),
            &mut super::provider_terminal_authority_rules::LegacyTerminalOpsV1,
            agent,
        )
    }
    pub fn validate_provider_grant_for_terminal(
        &self,
        agent: &str,
        grant_id: &str,
        admitted_at_tick: u64,
    ) -> Result<CapabilityGrantV2, WorldError> {
        super::provider_terminal_authority_rules::validate_provider_grant_for_terminal_from_sources_v1(
            &super::provider_terminal_authority_rules::LegacyTerminalSourcesV1(self),
            &mut super::provider_terminal_authority_rules::LegacyTerminalOpsV1,
            agent, grant_id, admitted_at_tick,
        )
    }
    pub fn validate_provider_grant_at_admission(
        &self,
        agent: &str,
        grant_id: &str,
        tick: u64,
    ) -> Result<CapabilityGrantV2, WorldError> {
        self.local_provider_capability_availability(agent)?;
        let identity = self
            .current_agent_identity(agent)
            .ok_or_else(|| deny("historical Agent identity missing"))?;
        let grant: CapabilityGrantV2 = serde_json::from_value(
            self.capability_grants_v2
                .get(grant_id)
                .ok_or_else(|| deny("historical original grant missing"))?
                .clone(),
        )
        .map_err(|_| deny("historical original grant malformed"))?;
        if grant.subject
            != (CapabilitySubject::Agent {
                agent_id: agent.into(),
                owner_binding: identity.owner_binding,
                generation: identity.generation,
            })
        {
            return Err(deny("historical grant subject changed"));
        }
        self.validate_local_terminal_grant(&grant, tick)?;
        Ok(grant)
    }

    fn validate_local_terminal_grant(
        &self,
        grant: &CapabilityGrantV2,
        tick: u64,
    ) -> Result<(), WorldError> {
        super::provider_terminal_authority_rules::validate_local_terminal_grant_from_sources_v1(
            &super::provider_terminal_authority_rules::LegacyTerminalSourcesV1(self),
            &mut super::provider_terminal_authority_rules::LegacyTerminalOpsV1,
            grant,
            tick,
        )
    }
}

#[cfg(test)]
#[path = "provider_terminal_authority_tests.rs"]
mod tests;
