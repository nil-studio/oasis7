use oasis7_wasm_abi::{AgentCommandResponse, CapabilityCatalogSnapshot};

use crate::capability_invocation_context::CapabilityInvocationContext;

use super::DecisionProviderError;

/// The immutable transport context attached to one provider decision turn.
/// The host creates this value and the provider can only echo it in an
/// [`AgentCommandResponse`].  It carries no authority; the native runtime
/// remains responsible for grant, revocation, and budget validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCapabilityContext {
    pub catalog: CapabilityCatalogSnapshot,
    pub invocation: CapabilityInvocationContext,
}

impl ProviderCapabilityContext {
    /// Validate the pair as one host-bound context before it is handed to a
    /// provider or used to admit a typed response. The native runtime remains
    /// authoritative for signatures, grants, and revocation; this check only
    /// rejects an internally inconsistent or incomplete transport envelope.
    pub fn validate(&self) -> Result<(), DecisionProviderError> {
        self.catalog.validate().map_err(|error| {
            DecisionProviderError::new("invalid_capability_context", error.to_string(), false)
        })?;
        if self.catalog.snapshot_id != self.invocation.catalog_snapshot_id
            || self.catalog.subject != self.invocation.subject
            || self.catalog.presenter != self.invocation.presenter
        {
            return Err(DecisionProviderError::new(
                "capability_context_mismatch",
                "capability catalog and invocation context are not bound to the same snapshot",
                false,
            ));
        }
        if self.invocation.grant_id.trim().is_empty()
            || self.invocation.catalog_snapshot_id.trim().is_empty()
            || self.invocation.response_nonce.trim().is_empty()
        {
            return Err(DecisionProviderError::new(
                "invalid_capability_context",
                "capability invocation grant, snapshot, and response nonce are required",
                false,
            ));
        }
        Ok(())
    }

    pub fn validate_response(
        &self,
        response: &AgentCommandResponse,
    ) -> Result<(), DecisionProviderError> {
        self.validate()?;
        response.validate().map_err(|error| {
            DecisionProviderError::new("module_command_response_invalid", error.to_string(), false)
        })?;
        if !response.matches_catalog(&self.catalog) {
            return Err(DecisionProviderError::new(
                "module_command_catalog_mismatch",
                "provider response does not match the runtime-produced capability catalog",
                false,
            ));
        }
        let context = &self.invocation;
        if context.subject != response.subject
            || context.presenter != response.presenter
            || context.audience != response.audience
            || context.catalog_snapshot_id != response.catalog_snapshot_id
            || context.response_nonce != response.response_nonce
            || context.module_id != response.selected_entry.module_id
            || context.module_version != response.selected_entry.module_version
        {
            return Err(DecisionProviderError::new(
                "module_command_context_mismatch",
                "provider response does not match the host-bound invocation context",
                false,
            ));
        }
        Ok(())
    }
}

pub use oasis7_agent_api::DecisionRequestContractError;
