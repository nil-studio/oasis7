//! Canonical read-only provider context derived from one authoritative World.
use super::World;
use oasis7_agent_api::CapabilityInvocationContext;
use oasis7_agent_api::world_context::WorldServiceAgentContext;
use oasis7_agent_api::{RuntimeBindingV1, h_v1};
use oasis7_wasm_abi::{CapabilityCatalogSnapshot, CapabilityPresenter, CapabilitySubject};

#[derive(Clone, Debug)]
pub(crate) struct ProviderCapabilityContext {
    pub(crate) catalog: CapabilityCatalogSnapshot,
    pub(crate) invocation: CapabilityInvocationContext,
}

impl World {
    /// Read-only authorized projection from the exact service-pinned World.
    /// No sidecar cache, signer material, or grant registry is transported.
    pub fn canonical_agent_service_context(
        &self,
        agent_id: &str,
    ) -> Result<WorldServiceAgentContext, String> {
        let binding = self
            .current_cognition_runtime_binding()
            .map_err(|error| format!("{error:?}"))?;
        let context = provider_capability_context(self, &binding, agent_id, 0)?;
        let CapabilitySubject::Agent { owner_binding, .. } = &context.invocation.subject else {
            return Err("canonical provider subject is not an Agent".into());
        };
        let root = self.capability_authorization_root().to_string();
        Ok(WorldServiceAgentContext {
            agent_id: agent_id.into(),
            capability_authorization_root: root.clone(),
            capability_snapshot_hash: h_v1("oasis7.runtime.manifest.v1", &root).to_string(),
            authority_context_hash: h_v1("oasis7.runtime.authority-context.v1", &root).to_string(),
            payer_binding: serde_json::json!({"owner_binding":owner_binding}),
            capability_catalog: context.catalog,
            capability_invocation_context: context.invocation,
        })
    }
}
pub(crate) fn provider_capability_context(
    world: &World,
    runtime_binding: &RuntimeBindingV1,
    agent_id: &str,
    sequence: u64,
) -> Result<ProviderCapabilityContext, String> {
    // The persisted invocation identifies the governed presenter/session.  A
    // fresh catalog/context pair is then projected by Runtime for this turn;
    // Viewer never reconstructs grants, policy roots, or catalog entries.
    let invocation = world
        .capability_invocation_contexts()
        .values()
        .find(|context| {
            matches!(
                &context.subject,
                CapabilitySubject::Agent { agent_id: subject_id, .. } if subject_id == agent_id
            ) && context.presenter.presenter_kind == "provider"
                && context.audience.world_id == runtime_binding.world_id
                && context.audience.branch_id == runtime_binding.branch_id
                && context.audience.finality_epoch == runtime_binding.finality_epoch
        })
        .cloned()
        .ok_or_else(|| {
            format!(
                "Runtime capability invocation context is unavailable for provider agent {agent_id}"
            )
        })?;
    let presenter: CapabilityPresenter = invocation.presenter.clone();
    let response_nonce = format!(
        "runtime-live:{}:{}:{}",
        agent_id,
        world.state().time,
        sequence
    );
    let (catalog, invocation) = world
        .capability_context_for_agent(agent_id, presenter, response_nonce)
        .map_err(|error| format!("Runtime capability context unavailable: {error:?}"))?;
    if catalog.world_id != runtime_binding.world_id
        || catalog.branch_id != runtime_binding.branch_id
        || catalog.finality_epoch != runtime_binding.finality_epoch
        || catalog.logical_tick != runtime_binding.base_tick
    {
        return Err(
            "Runtime capability context is not bound to the current cognition binding".to_string(),
        );
    }
    let _session_id = invocation
        .presenter
        .session_id
        .as_deref()
        .filter(|session_id| !session_id.trim().is_empty())
        .ok_or_else(|| {
            format!(
                "Runtime provider presenter session identity is unavailable for agent {agent_id}"
            )
        })?
        .to_string();
    Ok(ProviderCapabilityContext {
        catalog,
        invocation,
    })
}
