//! Atomic World transaction for installing a host context and executing one
//! trusted provider command.

use std::cell::RefCell;

use crate::capability_authorization::{CapabilityAuthorizationAuditReceipt, CapabilityInvocationContext};
use crate::world::capability_authorization_command_stage::TrustedCommandDelta;
use crate::world::transition::{BorrowedWorldView, PreparedStagedCommit, StagedTransition, TransitionContext, TransitionReject};
use crate::{CapabilityAuthorizationEvent, WorldError, WorldEventBody};
use super::World;
use super::capability_authorization::{deny, validate_invocation_context};
use super::super::capability_authorization_events::capability_invocation_context_key;
use oasis7_wasm_abi::{AgentCommandResponse, CapabilityCatalogSnapshot, CapabilityGrantV2, ModuleSandbox};

#[allow(dead_code)] // adapter: #6142
pub(crate) struct StagedTrustedModuleCommandWithContext<'a> {
    pub(crate) grant: CapabilityGrantV2,
    pub(crate) catalog: CapabilityCatalogSnapshot,
    pub(crate) response: AgentCommandResponse,
    pub(crate) context: CapabilityInvocationContext,
    pub(crate) sandbox: RefCell<&'a mut dyn ModuleSandbox>,
    pub(crate) receipt: RefCell<Option<CapabilityAuthorizationAuditReceipt>>,
}

#[allow(dead_code)] // adapter: #6142
impl<'a> StagedTrustedModuleCommandWithContext<'a> {
    pub(crate) fn new(grant: CapabilityGrantV2, catalog: CapabilityCatalogSnapshot, response: AgentCommandResponse, context: CapabilityInvocationContext, sandbox: &'a mut dyn ModuleSandbox) -> Self {
        Self { grant, catalog, response, context, sandbox: RefCell::new(sandbox), receipt: RefCell::new(None) }
    }
    pub(crate) fn take_receipt(&self) -> Option<CapabilityAuthorizationAuditReceipt> { self.receipt.borrow_mut().take() }
}

impl StagedTransition for StagedTrustedModuleCommandWithContext<'_> {
    type Delta = Option<TrustedCommandDelta>;
    fn prepare(&self, view: &BorrowedWorldView<'_>, _ctx: &TransitionContext) -> Result<PreparedStagedCommit<Self::Delta>, TransitionReject> {
        let world = view.world();
        world.verify_capability_authorization_root()?;
        validate_invocation_context(&self.context)?;
        let key = capability_invocation_context_key(&self.context)?;
        if world.capability_invocation_contexts.get(&key).is_some_and(|e| e != &self.context) {
            return Err(deny("invocation context is immutable").into());
        }
        let mut sandbox = self.sandbox.borrow_mut();
        let (receipt, prepared) = world.prepare_trusted_module_command_with_staged_context(
            self.grant.clone(), self.catalog.clone(), self.response.clone(), &mut **sandbox, Some((key, self.context.clone())),
        )?;
        *self.receipt.borrow_mut() = Some(receipt);
        Ok(match prepared {
            Some(mut p) => PreparedStagedCommit::new(view.base_head().clone(), p.consensus_record.take(), Some(TrustedCommandDelta(p))),
            None => PreparedStagedCommit::new(view.base_head().clone(), None, None),
        })
    }
}

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
