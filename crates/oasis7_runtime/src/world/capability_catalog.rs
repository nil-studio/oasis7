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
    CapabilitySubject, canonical_hash,
};
use serde_json::Value;

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
        if agent_id.trim().is_empty() || !self.state.agents.contains_key(agent_id) {
            return Err(deny("capability subject agent is not live"));
        }
        presenter
            .validate()
            .map_err(|error| deny(format!("capability presenter: {error}")))?;
        let response_nonce = response_nonce.into();
        if response_nonce.trim().is_empty() {
            return Err(deny("capability response nonce is required"));
        }
        let identity = self
            .capability_revocation_state
            .agent_identities
            .get(agent_id)
            .ok_or_else(|| deny("capability subject identity is not installed"))?;

        let mut grants = Vec::new();
        for (grant_id, encoded) in &self.capability_grants_v2 {
            let grant: CapabilityGrantV2 = serde_json::from_value(encoded.clone())
                .map_err(|_| deny("durable capability grant is malformed"))?;
            if grant.grant_id != *grant_id {
                return Err(deny("durable capability grant key does not match grant id"));
            }
            let CapabilitySubject::Agent {
                agent_id: subject_id,
                owner_binding,
                generation,
            } = &grant.subject
            else {
                continue;
            };
            if subject_id != agent_id
                || owner_binding != &identity.owner_binding
                || *generation != identity.generation
            {
                continue;
            }
            grant
                .validate()
                .map_err(|error| deny(format!("capability grant: {error}")))?;
            if !grant
                .body_hash_matches()
                .map_err(|error| deny(format!("capability grant body hash: {error}")))?
                || grant
                    .expected_grant_id()
                    .map_err(|error| deny(format!("capability grant id hash: {error}")))?
                    != grant.grant_id
            {
                return Err(deny("capability grant canonical identity mismatch"));
            }
            if grant.status != "verified"
                || grant.issued_at_tick > self.state.time
                || grant
                    .expires_at_tick
                    .is_none_or(|expires_at| expires_at < self.state.time)
            {
                continue;
            }
            self.verify_issuer(&grant)?;
            self.verify_live_revocation(&grant)?;
            self.verify_parent_chain(&grant)?;
            self.verify_capability_catalog_audience(&grant)?;
            if grant.scope.object_kind != "command"
                || !matches!(grant.scope.operation.as_str(), "execute" | "invoke")
            {
                continue;
            }
            grants.push(grant);
        }
        if grants.is_empty() {
            return Err(deny("no live command capability grant for agent"));
        }

        let first_audience = grants[0].audience.clone();
        if grants.iter().any(|grant| grant.audience != first_audience) {
            return Err(deny("agent capability grants have conflicting audiences"));
        }
        let mut entries = Vec::new();
        for command in self.module_command_catalog() {
            let eligible_grant_ids: Vec<String> = grants
                .iter()
                .filter(|grant| {
                    grant.scope.module_id == command.module_id
                        && grant.scope.module_version == command.module_version
                        && grant.scope.namespace == command.namespace
                        && grant.scope.object_name == command.name
                })
                .map(|grant| grant.grant_id.clone())
                .collect();
            if eligible_grant_ids.is_empty() {
                continue;
            }
            let max_payload_bytes = grants
                .iter()
                .filter(|grant| eligible_grant_ids.contains(&grant.grant_id))
                .filter_map(|grant| grant.scope.max_payload_bytes)
                .min()
                .unwrap_or(command.max_payload_bytes)
                .min(command.max_payload_bytes);
            if max_payload_bytes == 0 {
                continue;
            }
            entries.push(CapabilityCatalogEntry {
                module_id: command.module_id,
                module_version: command.module_version,
                namespace: command.namespace,
                command: command.name,
                schema_version: command.schema_version,
                schema_hash: command.schema_hash,
                max_payload_bytes,
                eligible_grant_ids,
            });
        }
        let selected_entry = entries
            .first()
            .ok_or_else(|| deny("live command grants have no active declaration"))?;
        let selected_grant_id = selected_entry
            .eligible_grant_ids
            .first()
            .cloned()
            .ok_or_else(|| deny("live command declaration has no eligible grant"))?;
        let selected_module_id = selected_entry.module_id.clone();
        let selected_module_version = selected_entry.module_version.clone();
        let module_registry_hash = canonical_hash(&self.module_registry)
            .map_err(|error| deny(format!("module registry hash: {error}")))?;
        let policy_hash = canonical_hash(&self.policies)
            .map_err(|error| deny(format!("policy hash: {error}")))?;
        let valid_until_tick = grants
            .iter()
            .filter_map(|grant| grant.expires_at_tick)
            .min()
            .ok_or_else(|| deny("live command grant expiry is missing"))?;
        let catalog = CapabilityCatalogSnapshot {
            snapshot_id: String::new(),
            world_id: first_audience.world_id.clone(),
            world_head: self
                .journal
                .events
                .last()
                .map(|event| event.id)
                .unwrap_or(0),
            branch_id: first_audience.branch_id.clone(),
            finality_epoch: first_audience.finality_epoch,
            logical_tick: self.state.time,
            module_registry_hash,
            policy_hash,
            revocation_epoch: self.capability_revocation_state.epoch,
            subject: grants[0].subject.clone(),
            presenter: presenter.clone(),
            audience: first_audience.clone(),
            entries,
            valid_until_tick,
        };
        let snapshot_id = catalog
            .canonical_hash()
            .map_err(|error| deny(format!("capability catalog hash: {error}")))?;
        let mut catalog = catalog;
        catalog.snapshot_id = snapshot_id.clone();
        catalog
            .validate()
            .map_err(|error| deny(format!("capability catalog: {error}")))?;
        let invocation = CapabilityInvocationContext {
            grant_id: selected_grant_id,
            subject: catalog.subject.clone(),
            presenter,
            audience: catalog.audience.clone(),
            catalog_snapshot_id: snapshot_id,
            module_id: selected_module_id,
            module_version: selected_module_version,
            response_nonce,
        };
        Ok((catalog, invocation))
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

    fn verify_capability_catalog_audience(
        &self,
        grant: &CapabilityGrantV2,
    ) -> Result<(), WorldError> {
        let audience = &grant.audience;
        match audience.target_kind.as_str() {
            "world" if audience.target_id.is_none() => {}
            "module_instance" => {
                let target_id = audience
                    .target_id
                    .as_deref()
                    .ok_or_else(|| deny("capability module-instance target id is required"))?;
                let live = self
                    .state
                    .module_instances
                    .get(target_id)
                    .is_some_and(|instance| {
                        instance.active
                            && instance.instance_id == target_id
                            && instance.module_id == grant.scope.module_id
                            && instance.module_version == grant.scope.module_version
                    });
                if !live {
                    return Err(deny("capability module-instance target is not live"));
                }
            }
            _ => return Err(deny("capability audience target is not live")),
        }
        let authority = self
            .capability_revocation_state
            .authority_records
            .get(&grant.issuer.issuer_id)
            .ok_or_else(|| deny("capability audience authority is not installed"))?;
        if authority.finality_status != "finalized"
            || authority.branch_id != audience.branch_id
            || authority.finality_epoch != audience.finality_epoch
        {
            return Err(deny("capability audience is not bound to live finality"));
        }
        if self.chain_resource_manifest.world_id != "unbound"
            && self.chain_resource_manifest.world_id != audience.world_id
        {
            return Err(deny("capability audience world does not match live world"));
        }
        if let Some(binding) = self.cognition.get("runtime_binding")
            && (binding.get("world_id").and_then(Value::as_str) != Some(audience.world_id.as_str())
                || binding.get("branch_id").and_then(Value::as_str)
                    != Some(audience.branch_id.as_str())
                || binding.get("finality_epoch").and_then(Value::as_u64)
                    != Some(audience.finality_epoch))
        {
            return Err(deny("capability audience conflicts with Runtime binding"));
        }
        if let Some(record) = self.tick_consensus_records.last() {
            let block_hash = record.block.block_hash();
            if record.certificate.block_hash != block_hash || record.certificate.threshold == 0 {
                return Err(deny("capability live finality record is invalid"));
            }
            if let Some(chain_epoch) = record.block.header.chain_epoch
                && chain_epoch < audience.finality_epoch
            {
                return Err(deny("capability finality epoch is ahead of live chain"));
            }
        }
        Ok(())
    }
}

fn deny(reason: impl Into<String>) -> WorldError {
    WorldError::CapabilityAuthorizationDenied {
        reason: reason.into(),
    }
}
