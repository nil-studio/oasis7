use std::collections::BTreeSet;

use oasis7_wasm_abi::{AgentCommandResponse, CapabilityCatalogSnapshot, CapabilityGrantV2};

use super::super::WorldError;
use super::World;
use super::capability_authorization::deny;

impl World {
    /// Resolve the audience against runtime-owned chain and target state at
    /// the execution boundary. A provider/catalog may repeat audience data,
    /// but it cannot make an institution or module-instance target live by
    /// assertion; this runtime currently exposes the world target only.
    pub(super) fn verify_live_capability_audience(
        &self,
        grant: &CapabilityGrantV2,
        catalog: &CapabilityCatalogSnapshot,
        response: &AgentCommandResponse,
    ) -> Result<(), WorldError> {
        if grant.audience != catalog.audience || grant.audience != response.audience {
            return Err(deny("capability audience changed before execution"));
        }
        match grant.audience.target_kind.as_str() {
            "world" if grant.audience.target_id.is_none() => {}
            "module_instance" => {
                let Some(target_id) = grant.audience.target_id.as_deref() else {
                    return Err(deny("module-instance audience target id is required"));
                };
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
                    return Err(deny("module-instance audience target is not live"));
                }
            }
            _ => {
                return Err(deny(
                    "capability audience target is not resolved by the live world runtime",
                ));
            }
        }
        let authority = self
            .capability_revocation_state
            .authority_records
            .get(&grant.issuer.issuer_id)
            .ok_or_else(|| deny("capability audience issuer authority is missing"))?;
        if authority.finality_status != "finalized"
            || authority.world_id != grant.audience.world_id
            || authority.branch_id != grant.audience.branch_id
            || authority.finality_epoch != grant.audience.finality_epoch
            || authority.finality_block_hash.trim().is_empty()
        {
            return Err(deny("capability audience has no live finalized authority"));
        }
        if self.chain_resource_manifest.world_id != "unbound"
            && self.chain_resource_manifest.world_id != grant.audience.world_id
        {
            return Err(deny("capability audience world does not match live chain"));
        }
        if let Some(record) = self.tick_consensus_records.last() {
            let block_hash = record.block.block_hash();
            if record.certificate.block_hash != block_hash || record.certificate.threshold == 0 {
                return Err(deny("capability audience live finality record is invalid"));
            }
            if let Some(chain_epoch) = record.block.header.chain_epoch
                && chain_epoch < grant.audience.finality_epoch
            {
                return Err(deny(
                    "capability audience finality epoch is ahead of live chain",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn verify_parent_chain(&self, grant: &CapabilityGrantV2) -> Result<(), WorldError> {
        // Depth one is the first delegated-child level. A higher-depth
        // parent may be issued directly by governance as a delegation root;
        // a depth-one grant without a parent is otherwise an orphan child.
        if grant.delegation_depth == 1 && grant.parent_grant_id.is_none() {
            return Err(deny(
                "delegated grant must carry an explicit parent authorization",
            ));
        }
        let mut visited = BTreeSet::new();
        self.verify_parent_chain_inner(grant, &mut visited)
    }

    fn verify_parent_chain_inner(
        &self,
        grant: &CapabilityGrantV2,
        visited: &mut BTreeSet<String>,
    ) -> Result<(), WorldError> {
        let Some(parent_id) = &grant.parent_grant_id else {
            return Ok(());
        };
        if !visited.insert(parent_id.clone()) {
            return Err(deny("delegated grant parent chain contains a cycle"));
        }
        let parent = self
            .capability_grants_v2
            .get(parent_id)
            .ok_or_else(|| deny("parent grant is not in the durable registry"))?;
        let parent: CapabilityGrantV2 = serde_json::from_value(parent.clone())
            .map_err(|_| deny("parent grant is malformed"))?;
        parent
            .validate()
            .map_err(|error| deny(format!("parent grant validation: {error}")))?;
        if !parent
            .body_hash_matches()
            .map_err(|error| deny(format!("parent grant body hash: {error}")))?
            || parent
                .expected_grant_id()
                .map_err(|error| deny(format!("parent grant id hash: {error}")))?
                != parent.grant_id
        {
            return Err(deny("parent grant canonical body hash or id mismatch"));
        }
        if parent.expires_at_tick.is_none()
            || parent
                .expires_at_tick
                .is_some_and(|expiry| self.state.time > expiry)
            || parent.issued_at_tick > self.state.time
        {
            return Err(deny("parent grant lifetime is not currently valid"));
        }
        self.verify_issuer(&parent)?;
        self.verify_live_revocation(&parent)?;
        if parent.subject != grant.subject
            || parent.audience != grant.audience
            || parent.issuer.issuer_id != grant.issuer.issuer_id
            || parent.issuer.issuer_kind != grant.issuer.issuer_kind
            || parent.issuer.governance_epoch != grant.issuer.governance_epoch
            || parent.issuer.finalized_receipt_id != grant.issuer.finalized_receipt_id
            || parent.issuer.key_id != grant.issuer.key_id
            || parent.issuer.issuer_key_epoch != grant.issuer.issuer_key_epoch
            || parent.issuer.authority_rotation_receipt_id
                != grant.issuer.authority_rotation_receipt_id
        {
            return Err(deny(
                "delegated grant child issuer, subject, or audience is not parent-bound",
            ));
        }
        let expiry_attenuates = match (parent.expires_at_tick, grant.expires_at_tick) {
            (Some(parent_expiry), Some(child_expiry)) => child_expiry <= parent_expiry,
            _ => false,
        };
        if parent.status != "verified"
            || parent.delegation_depth <= grant.delegation_depth
            || !expiry_attenuates
            || !parent.scope.contains_subset(&grant.scope)
        {
            return Err(deny("delegated grant does not attenuate its parent"));
        }
        self.verify_parent_chain_inner(&parent, visited)
    }
}
