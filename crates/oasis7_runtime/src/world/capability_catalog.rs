//! World-owned capability catalog and invocation-context projection.
//!
//! Provider turns may observe capabilities, but they must not construct a
//! catalog from request text or a process-local grant cache.  This module
//! derives both DTOs from the live World authorization registry, module
//! declarations, and the current authority/tick binding.

use super::World;
use crate::capability_authorization::CapabilityInvocationContext;
use crate::error::WorldError;
use oasis7_wasm_abi::{
    CapabilityCatalogEntry, CapabilityCatalogSnapshot, CapabilityGrantV2, CapabilityPresenter,
    CapabilitySubject,
};
use serde_json::Value;

#[path = "capability_catalog_rules.rs"]
pub(in crate::world) mod rules;

#[path="capability_catalog_audience.rs"]
pub(in crate::world) mod audience;
impl World {
    /// Derive the capability catalog and invocation context for one provider
    /// turn.  All subject, audience, grant, module, revocation, and lifetime
    /// fields are read from the live World; the presenter and response nonce
    /// are the only transport inputs.  The first matching command grant and
    /// command declaration are selected in canonical registry order for the
    /// invocation context, while the catalog includes every matching grant.
    pub fn capability_context_for_agent(
        &self,
        agent_id: &str,
        presenter: CapabilityPresenter,
        response_nonce: impl Into<String>,
    ) -> Result<(CapabilityCatalogSnapshot, CapabilityInvocationContext), WorldError> {
        rules::capability_context_from_sources_v1(
            &rules::LegacyCapabilityCatalogSourcesV1(self),
            &mut rules::LegacyCapabilityCatalogOpsV1,
            agent_id,
            presenter,
            response_nonce,
        )
    }

    /// Persist a provider invocation selected from the live authority view.
    /// This bootstrap/lease boundary never accepts a caller-supplied grant or
    /// subject and publishes no partial context when installation fails.
    pub fn install_capability_invocation_context_for_agent(
        &mut self,
        agent_id: &str,
        presenter: CapabilityPresenter,
        response_nonce: impl Into<String>,
    ) -> Result<CapabilityInvocationContext, WorldError> {
        let (_, invocation) =
            self.capability_context_for_agent(agent_id, presenter, response_nonce)?;
        let mut staged = self.clone();
        staged.install_capability_invocation_context(invocation.clone())?;
        *self = staged;
        Ok(invocation)
    }

    pub(super) fn verify_capability_catalog_audience(
        &self,
        grant: &CapabilityGrantV2,
    ) -> Result<(), WorldError> {
        audience::legacy(self,grant)
    }
}

fn deny(reason: impl Into<String>) -> WorldError {
    WorldError::CapabilityAuthorizationDenied {
        reason: reason.into(),
    }
}
