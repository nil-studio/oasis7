//! Local administrator inputs join the ordinary rollback/commit candidate.
use super::*;
impl NodeRuntimeExecutionDriver {
    pub(super) fn pin_devlocal_participant_resource_identity(&mut self) -> Result<(), String> {
        if self.devlocal_participant.authority.is_none() {
            return Ok(());
        }
        let manifest = self.execution_world.chain_resource_manifest();
        let genesis_ref = self.participant_genesis()?;
        self.resource_identity = Some(super::ExecutionResourceIdentity {
            chain_id: manifest.chain_id.clone(),
            genesis_ref,
        });
        Ok(())
    }

    fn participant_genesis(&self) -> Result<String, String> {
        let manifest = self.execution_world.chain_resource_manifest();
        if let Some(origin) = &self.devlocal_participant.verified_origin {
            if origin.identity().world_id != manifest.world_id {
                return Err("verified participant origin differs from existing world".into());
            }
            if let Some(genesis) = &manifest.genesis_ref {
                if genesis != &origin.identity().genesis_digest {
                    return Err(
                        "verified participant origin cannot replace an existing genesis binding"
                            .into(),
                    );
                }
            } else if self.devlocal_participant.startup.is_some() {
                return Ok(origin.identity().genesis_digest.clone());
            }
        }
        manifest.genesis_ref.as_ref().filter(|v| !v.trim().is_empty()).cloned()
            .ok_or("local participant administration requires an existing authenticated genesis binding; explicit signed binding migration required".into())
    }
    fn participant_snapshot(&self) -> Result<oasis7_runtime::Snapshot, String> {
        let world = &self.execution_world;
        let manifest = world.chain_resource_manifest();
        if manifest.genesis_ref.is_some() {
            return Ok(world.snapshot());
        }
        let genesis = self.participant_genesis()?;
        // The Host verified this original origin under the independently pinned
        // node key; the operator authorization separately signs the same genesis.
        // This detached candidate is published only by the ordinary chain commit.
        Ok(world.snapshot_with_chain_resource_context(
            ChainResourceDerivationContext {
                world_id: &manifest.world_id,
                chain_id: &manifest.chain_id,
                genesis_ref: Some(&genesis),
                created_at_height: manifest.created_at_height,
                manifest_height: manifest.manifest_height,
                commit_block_hash: manifest.created_at_block_hash.as_deref(),
                tick: world.state().time,
            },
            manifest.world_config_hash.clone(),
            manifest.generation_algorithm_hash.clone(),
        ))
    }

    fn participant_candidate(&self) -> Result<Option<RuntimeWorld>, String> {
        let Some(authorization) = self.devlocal_participant.startup.clone() else {
            return Ok(None);
        };
        if self.formal_retention_enabled {
            return Err(
                "local participant administrator input is forbidden in formal execution".into(),
            );
        }
        // Never let an attached Runtime persistence handle publish early.
        let mut candidate = crate::execution_config::restore_execution_world(
            self.participant_snapshot()?,
            self.execution_world.journal().clone(),
            self.execution_world.release_security_policy().clone(),
            self.devlocal_participant.authority.as_ref(),
        )
        .map_err(|e| format!("local participant detached candidate restore failed: {e:?}"))?;
        candidate
            .install_devlocal_participant_policy(
                authorization.request.policy.clone(),
                authorization,
            )
            .map_err(|e| format!("local participant signed startup policy rejected: {e:?}"))?;
        Ok(Some(candidate))
    }
    pub(super) fn validate_devlocal_participant_startup(&self) -> Result<(), String> {
        self.participant_candidate().map(|_| ())
    }
    pub(super) fn apply_devlocal_participant_startup(&mut self) -> Result<(), String> {
        self.pin_devlocal_participant_resource_identity()?;
        if let Some(candidate) = self.participant_candidate()? {
            self.execution_world = candidate;
        }
        Ok(())
    }
}
