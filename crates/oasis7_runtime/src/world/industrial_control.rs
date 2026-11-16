use super::*;
use crate::CapabilityAgentIdentity;
pub use oasis7_world_state::world::industrial_control::*;
impl World {
    pub(crate) fn world_rule_binding(&self) -> Option<String> {
        match readonly_runtime_owner::world_rule_binding_readonly_v1(self){Ok(value)=>value,Err(error)=>match error{}}
    }
    pub(crate) fn namespace_was_authorized_readonly_v1(&self,namespace:&oasis7_agent_api::game::GameIdentityNamespaceV1)->bool{
        match readonly_runtime_owner::namespace_was_authorized_readonly_v1(self,namespace){Ok(value)=>value,Err(error)=>match error{}}
    }
    pub(crate) fn owner_public_key_observation_data_v1(&self,agent:&str)->Result<String,String>{
        match readonly_runtime_owner::owner_public_key_readonly_v1(self,agent){Ok(value)=>value.map_err(str::to_owned),Err(error)=>match error{}}
    }
    /// Native grants bind the current credential generation as well as the
    /// immutable economic claim source. Reauthentication cannot revive a grant.
    pub fn current_agent_owner(&self, agent: &str) -> Option<ResolvedAgentOwnerV1> {
        match readonly_runtime_owner::current_agent_owner_readonly_v1(self,agent){Ok(value)=>value,Err(error)=>match error{}}
    }
    pub fn current_agent_identity(&self, agent: &str) -> Option<CapabilityAgentIdentity> {
        match readonly_runtime_owner::current_agent_identity_readonly_v1(self,agent){Ok(value)=>value,Err(error)=>match error{}}
    }
}

/// A checkpoint advances on root credential changes; acquisitions advance the
/// persistent per-target sequence without requiring another identity event.
pub(crate) fn manufactured_identity_generation(
    revocation: &crate::capability_authorization::CapabilityRevocationState,
    target: &str,
    acquisition_sequence: u64,
) -> Option<u64> {
    oasis7_world_state::incremental_runtime_revocation::resolve_manufactured_identity_generation_v1(
        revocation.agent_identities.get(target),
        revocation
            .manufactured_identity_acquisition_sequences
            .get(target)
            .copied(),
        acquisition_sequence,
    )
}

impl World {
    pub(super) fn validate_manufactured_identity_acquisition(
        &self,
        event: &crate::DomainEvent,
    ) -> Result<(), crate::WorldError> {
        let target = match event {
            crate::DomainEvent::AgentClaimed {
                acquisition_contract_version: 1,
                target_agent_id,
                ..
            } if self.state.is_manufactured_agent(target_agent_id) => {
                Some(target_agent_id.as_str())
            }
            crate::DomainEvent::WorldRuleCommitted {
                command:
                    oasis7_agent_api::world_action::WorldRuleCommandV1::DeliverAgentOrder { order_id },
                ..
            } => self
                .state
                .world_rules
                .as_ref()
                .and_then(|r| r.agent_orders.get(order_id))
                .filter(|o| o.delivered_at.is_none())
                .map(|o| o.target_agent_id.as_str()),
            _ => None,
        };
        if let Some(target) = target {
            let sequence = self
                .state
                .agent_claim_acquisition_sequences
                .get(target)
                .copied()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or_else(|| {
                    crate::world_rules::invalid("manufactured_identity_generation_overflow")
                })?;
            manufactured_identity_generation(&self.capability_revocation_state, target, sequence)
                .ok_or_else(|| {
                crate::world_rules::invalid("manufactured_identity_generation_overflow")
            })?;
        }
        Ok(())
    }
}

#[path="readonly_runtime_owner.rs"]
pub(in crate::world) mod readonly_runtime_owner;
