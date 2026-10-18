//! Atomic World transaction for installing a host context and executing one
//! trusted provider command.

use super::super::capability_authorization::{
    CapabilityAuthorizationAuditReceipt, CapabilityInvocationContext,
};
use super::super::{CapabilityAuthorizationEvent, WorldError, WorldEventBody};
use super::World;
use super::capability_authorization::{deny, validate_invocation_context};
use super::capability_authorization_events::capability_invocation_context_key;
use oasis7_wasm_abi::{
    AgentCommandResponse, CapabilityCatalogSnapshot, CapabilityGrantV2, ModuleSandbox,
};

impl World {
    /// Execute a provider command with its context installed in the same
    /// World transaction. Staging the context map avoids advancing the World
    /// head between catalog projection and execution; the durable install
    /// event is published only after command authorization succeeds.
    pub fn execute_trusted_module_command_with_context<E>(
        &mut self,
        grant: CapabilityGrantV2,
        catalog: CapabilityCatalogSnapshot,
        response: AgentCommandResponse,
        context: CapabilityInvocationContext,
        executor: &mut E,
        sandbox: &mut dyn ModuleSandbox,
    ) -> Result<CapabilityAuthorizationAuditReceipt, WorldError> {
        self.verify_capability_authorization_root()?;
        validate_invocation_context(&context)?;
        let key = capability_invocation_context_key(&context)?;
        if let Some(existing) = self.capability_invocation_contexts.get(&key)
            && existing != &context
        {
            return Err(deny("invocation context is immutable"));
        }
        let mut staged = self.clone();
        let already_installed = staged.capability_invocation_contexts.contains_key(&key);
        staged
            .capability_invocation_contexts
            .entry(key.clone())
            .or_insert_with(|| context.clone());
        staged.refresh_capability_authorization_root()?;
        let receipt =
            staged.execute_trusted_module_command(grant, catalog, response, executor, sandbox)?;
        if !already_installed {
            staged.append_event(
                WorldEventBody::CapabilityAuthorization(
                    CapabilityAuthorizationEvent::InvocationContextInstalled { key, context },
                ),
                None,
            )?;
        }
        *self = staged;
        Ok(receipt)
    }
}
