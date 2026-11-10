//! Expiry is a typed reason to deny fresh work, never a new grant.
use super::{World, WorldError};
use oasis7_wasm_abi::{CapabilityGrantV2, CapabilitySubject};

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
        let module = super::local_test_provider_bootstrap::LOCAL_TEST_PROVIDER_MODULE_ID;
        let mut local = self.manifest.content.get("local_test_provider_authority")
            .is_some_and(|marker| marker.get("actor").and_then(serde_json::Value::as_str) == Some(agent))
            || self.capability_invocation_contexts.values().any(|context| {
                context.module_id == module && matches!(&context.subject, CapabilitySubject::Agent { agent_id, .. } if agent_id == agent)
            });
        for encoded in self.capability_grants_v2.values() {
            let grant: CapabilityGrantV2 = serde_json::from_value(encoded.clone())
                .map_err(|_| deny("durable provider grant malformed"))?;
            local |= grant.issuer.issuer_id
                == super::local_test_provider_bootstrap::LOCAL_TEST_PROVIDER_ISSUER_ID
                && grant.scope.module_id == module
                && matches!(&grant.subject, CapabilitySubject::Agent { agent_id, .. } if agent_id == agent);
        }
        Ok(local)
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
        if self.release_security_policy().is_production_hardened()
            || !self.release_security_policy().allow_local_finality_signing
            || !self.state.agents.contains_key(agent)
        {
            return Err(deny(
                "terminal recovery is unavailable outside live DevLocal authority",
            ));
        }
        self.verify_capability_authorization_root()?;
        let identity = self
            .current_agent_identity(agent)
            .ok_or_else(|| deny("terminal Agent identity missing"))?;
        let owner = self
            .current_agent_owner(agent)
            .ok_or_else(|| deny("terminal current owner unavailable"))?;
        if owner.player_id != identity.owner_binding || owner.public_key.is_none() {
            return Err(deny("terminal current canonical owner changed"));
        }
        if let Ok(context) = self.canonical_agent_service_context(agent) {
            let mut live = Vec::new();
            for encoded in self.capability_grants_v2.values() {
                let grant: CapabilityGrantV2 = serde_json::from_value(encoded.clone())
                    .map_err(|_| deny("live local grant malformed"))?;
                if grant.issuer.issuer_id
                    == super::local_test_provider_bootstrap::LOCAL_TEST_PROVIDER_ISSUER_ID
                    && matches!(&grant.subject, CapabilitySubject::Agent { agent_id, owner_binding, generation }
                        if agent_id == agent && owner_binding == &identity.owner_binding && *generation == identity.generation)
                    && grant.issued_at_tick <= self.state.time
                    && grant
                        .expires_at_tick
                        .is_some_and(|expiry| self.state.time <= expiry)
                {
                    self.validate_local_terminal_grant(&grant, self.state.time)?;
                    live.push(grant.grant_id);
                }
            }
            if live.len() != 1 || live[0] != context.capability_invocation_context.grant_id {
                return Err(deny("live local provider grant is missing or ambiguous"));
            }
            return Ok(ProviderCapabilityAvailability::Live);
        }
        let mut candidates = std::collections::BTreeMap::new();
        for (key, value) in &self.capability_grants_v2 {
            let grant: CapabilityGrantV2 = serde_json::from_value(value.clone())
                .map_err(|_| deny("terminal grant is malformed"))?;
            let CapabilitySubject::Agent {
                agent_id,
                owner_binding,
                generation,
            } = &grant.subject
            else {
                continue;
            };
            if agent_id != agent {
                continue;
            }
            if owner_binding != &identity.owner_binding || *generation != identity.generation {
                return Err(deny("terminal grant owner or generation changed"));
            }
            if grant.scope.object_kind != "command"
                || !matches!(grant.scope.operation.as_str(), "execute" | "invoke")
            {
                continue;
            }
            if key != &grant.grant_id {
                return Err(deny("terminal grant key mismatch"));
            }
            self.validate_local_terminal_grant(&grant, grant.issued_at_tick)?;
            candidates.insert(grant.grant_id.clone(), grant);
        }
        if candidates.is_empty() {
            return Err(deny("terminal expired grant set missing"));
        }
        if candidates.values().any(|grant| {
            grant.issued_at_tick > self.state.time
                || grant
                    .expires_at_tick
                    .is_none_or(|expiry| self.state.time <= expiry)
        }) {
            return Err(deny("terminal candidates are not exclusively expired"));
        }
        let mut contexts = std::collections::BTreeSet::new();
        for (key, context) in &self.capability_invocation_contexts {
            if matches!(&context.subject, CapabilitySubject::Agent { agent_id, .. } if agent_id == agent)
            {
                super::capability_authorization::validate_invocation_context(context)?;
                let grant = candidates
                    .get(&context.grant_id)
                    .ok_or_else(|| deny("terminal invocation has no matching historical grant"))?;
                if context.subject != grant.subject
                    || context.audience != grant.audience
                    || context.module_id != grant.scope.module_id
                    || context.module_version != grant.scope.module_version
                    || context.presenter.presenter_kind != "provider"
                    || super::capability_authorization_events::capability_invocation_context_key(
                        context,
                    )? != *key
                {
                    return Err(deny("terminal persisted invocation identity is invalid"));
                }
                contexts.insert(context.grant_id.clone());
            }
        }
        if contexts.len() != candidates.len() {
            return Err(deny("terminal historical provider invocation missing"));
        }
        Ok(ProviderCapabilityAvailability::ExpiredOnly {
            grant_ids: candidates.into_keys().collect(),
        })
    }
    pub fn validate_provider_grant_for_terminal(
        &self,
        agent: &str,
        grant_id: &str,
        admitted_at_tick: u64,
    ) -> Result<CapabilityGrantV2, WorldError> {
        match self.local_provider_capability_availability(agent)? {
            ProviderCapabilityAvailability::ExpiredOnly { grant_ids }
                if grant_ids.iter().any(|id| id == grant_id) => {}
            _ => {
                return Err(deny(
                    "terminal recovery requires the exact expired original grant",
                ));
            }
        }
        let grant: CapabilityGrantV2 =
            serde_json::from_value(self.capability_grants_v2[grant_id].clone())
                .map_err(|_| deny("terminal original grant malformed"))?;
        self.validate_local_terminal_grant(&grant, admitted_at_tick)?;
        Ok(grant)
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
        grant
            .validate()
            .map_err(|_| deny("terminal grant structure invalid"))?;
        if grant.issuer.issuer_id
            != super::local_test_provider_bootstrap::LOCAL_TEST_PROVIDER_ISSUER_ID
            || grant.scope.module_id
                != super::local_test_provider_bootstrap::LOCAL_TEST_PROVIDER_MODULE_ID
            || grant.scope.module_version
                != super::local_test_provider_bootstrap::LOCAL_TEST_PROVIDER_MODULE_VERSION
            || grant.scope.namespace
                != super::local_test_provider_bootstrap::LOCAL_TEST_PROVIDER_NAMESPACE
            || grant.scope.object_name
                != super::local_test_provider_bootstrap::LOCAL_TEST_PROVIDER_COMMAND
            || grant.status != "verified"
            || grant.issued_at_tick > tick
            || grant.expires_at_tick.is_none_or(|expiry| tick > expiry)
            || !grant
                .body_hash_matches()
                .map_err(|_| deny("terminal grant hash invalid"))?
            || grant
                .expected_grant_id()
                .map_err(|_| deny("terminal grant identity invalid"))?
                != grant.grant_id
        {
            return Err(deny("terminal original grant lifetime or identity invalid"));
        }
        self.verify_issuer(grant)?;
        self.verify_live_revocation(grant)?;
        self.verify_parent_chain_at_tick(grant, tick)?;
        self.verify_capability_catalog_audience(grant)?;
        if !self.module_command_catalog().iter().any(|command| {
            grant.scope.module_id == command.module_id
                && grant.scope.module_version == command.module_version
                && grant.scope.namespace == command.namespace
                && grant.scope.object_name == command.name
        }) {
            return Err(deny("terminal grant has no unchanged active declaration"));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "provider_terminal_authority_tests.rs"]
mod tests;
