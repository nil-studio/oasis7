//! Canonical read-only provider context derived from one authoritative World.
use super::World;
use oasis7_agent_api::CapabilityInvocationContext;
use oasis7_agent_api::world_context::WorldServiceAgentContext;
use oasis7_agent_api::{RuntimeBindingV1, h_v1};
use oasis7_wasm_abi::{CapabilityCatalogSnapshot, CapabilityPresenter, CapabilitySubject};

#[path="service_context_sources.rs"]
pub(crate) mod sources;

#[derive(Clone, Debug)]
pub(crate) struct ProviderCapabilityContext {
    pub(crate) catalog: CapabilityCatalogSnapshot,
    pub(crate) invocation: CapabilityInvocationContext,
}

impl World {
    /// Ordinary observation may precede provider provisioning. Existing provider
    /// records retain the strict selection and validation failures below.
    pub fn optional_canonical_agent_service_context(
        &self,
        agent_id: &str,
    ) -> Result<Option<WorldServiceAgentContext>, String> {
        match sources::optional_from_sources_v1(&sources::Legacy(self),agent_id){Ok(result)=>result,Err(never)=>match never{}}
    }

    /// Read-only authorized projection from the exact service-pinned World.
    /// No sidecar cache, signer material, or grant registry is transported.
    pub fn canonical_agent_service_context(
        &self,
        agent_id: &str,
    ) -> Result<WorldServiceAgentContext, String> {
        match sources::canonical_from_sources_v1(&sources::Legacy(self),agent_id){Ok(result)=>result,Err(never)=>match never{}}
    }
}
pub(crate) fn provider_capability_context(
    world: &World,
    runtime_binding: &RuntimeBindingV1,
    agent_id: &str,
    sequence: u64,
) -> Result<ProviderCapabilityContext, String> {
    match sources::provider_from_sources_v1(&sources::Legacy(world),runtime_binding,agent_id,sequence){Ok(result)=>result,Err(never)=>match never{}}
    }

fn is_provider_context_for_agent(context: &CapabilityInvocationContext, agent_id: &str) -> bool {
    matches!(
        &context.subject,
        CapabilitySubject::Agent { agent_id: subject_id, .. } if subject_id == agent_id
    ) && context.presenter.presenter_kind == "provider"
}

#[cfg(test)]
#[path = "service_context_tests.rs"]
mod tests;
