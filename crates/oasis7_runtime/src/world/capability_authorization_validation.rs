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
        self.verify_parent_chain_inner(grant, &mut visited, self.state.time)
    }

    pub(super) fn verify_parent_chain_at_tick(
        &self,
        grant: &CapabilityGrantV2,
        tick: u64,
    ) -> Result<(), WorldError> {
        if grant.delegation_depth == 1 && grant.parent_grant_id.is_none() {
            return Err(deny(
                "delegated grant must carry an explicit parent authorization",
            ));
        }
        self.verify_parent_chain_inner(grant, &mut BTreeSet::new(), tick)
    }

    fn verify_parent_chain_inner(
        &self,
        grant: &CapabilityGrantV2,
        visited: &mut BTreeSet<String>,
        tick: u64,
    ) -> Result<(), WorldError> {
        super::incremental_module_capability_admission_rules::parents(&mut super::incremental_module_capability_admission_rules::Legacy(self),grant,tick,visited)
    }
}
