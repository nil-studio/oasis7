//! Material adoption from an actual executed World, never qualification from a
//! proposed Snapshot. Only reproducibly derived chain-resource metadata changes.
use super::{World, WorldError};
use crate::{ChainResourceDerivationContext, Journal, Snapshot};
impl World {
    pub fn restore_candidate_snapshot_with_attached_formal_history(
        &self,
        snapshot: Snapshot,
        journal: Journal,
    ) -> Result<Self, WorldError> {
        let deny = |reason: &str| WorldError::DistributedValidationFailed {
            reason: reason.into(),
        };
        if journal != self.journal {
            return Err(deny("candidate journal differs from actual executed World"));
        }
        if self.state.retained_history_root.is_some() {
            if self
                .formal_history_reader
                .as_ref()
                .and_then(|attached| attached.parent.as_ref())
                .is_none()
            {
                return Err(deny(
                    "candidate restore requires existing qualified history evidence",
                ));
            }
            self.attached_formal_history()?;
        }
        let manifest = &snapshot.chain_resource_manifest;
        let expected = self.snapshot_with_chain_resource_context(
            ChainResourceDerivationContext {
                world_id: &manifest.world_id,
                chain_id: &manifest.chain_id,
                genesis_ref: manifest.genesis_ref.as_deref(),
                created_at_height: manifest.created_at_height,
                manifest_height: manifest.manifest_height,
                commit_block_hash: manifest.created_at_block_hash.as_deref(),
                tick: self.state.time,
            },
            manifest.world_config_hash.clone(),
            manifest.generation_algorithm_hash.clone(),
        );
        let encoded = |value: &Snapshot| {
            serde_cbor::to_vec(value).map_err(|error| WorldError::DistributedValidationFailed {
                reason: error.to_string(),
            })
        };
        if encoded(&snapshot)? != encoded(&expected)? {
            return Err(deny(
                "candidate snapshot differs from actual World/derived chain-resource metadata",
            ));
        }
        let mut candidate = self.clone();
        candidate.chain_resource_manifest = snapshot.chain_resource_manifest;
        candidate.latest_chain_resource_delta = snapshot.latest_chain_resource_delta;
        if encoded(&candidate.snapshot())? != encoded(&expected)? {
            return Err(deny(
                "candidate metadata installation changed executed Snapshot",
            ));
        }
        candidate.verify_tick_consensus_chain()?;
        Ok(candidate)
    }
}
