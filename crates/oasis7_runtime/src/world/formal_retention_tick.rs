//! Rebind only an unqualified execution Tick after its deterministic archive plan.
use super::{World, WorldError};
use crate::world_event::WorldEvent;
use crate::{
    RuntimeCommittedTickContext, TICK_BLOCK_HEADER_SCHEMA_V2, TickConsensusRecord,
};
impl World {
    pub(super) fn refresh_candidate_tick_after_retention(
        &mut self,
        record: &TickConsensusRecord,
        tick_events: &[WorldEvent],
    ) -> Result<(), String> {
        let parent = self
            .formal_history_reader
            .as_ref()
            .and_then(|attached| attached.parent.as_ref())
            .ok_or("candidate Tick refresh requires a qualified retention parent")?;
        if let Some(parent_tick) = parent.logical_tick()
            && record.block.header.tick <= parent_tick
        {
            return Err("retention cannot re-sign an already qualified Tick".into());
        }
        let tick = record.block.header.tick;
        self.refresh_candidate_tick_after_retention_inner(record, tick_events, tick)
            .map_err(|error| format!("retention candidate Tick: {error:?}"))
    }

    fn refresh_candidate_tick_after_retention_inner(
        &mut self,
        record: &TickConsensusRecord,
        tick_events: &[WorldEvent],
        tick: u64,
    ) -> Result<(), super::WorldError> {
        let state_root = self.current_state_root_hash()?;
        let committed_context = if record.block.header.schema_version >= TICK_BLOCK_HEADER_SCHEMA_V2
        {
            Some(RuntimeCommittedTickContext {
                height: record.block.header.chain_height.ok_or_else(|| {
                    WorldError::DistributedValidationFailed {
                        reason: "retention candidate tick context missing chain_height".to_string(),
                    }
                })?,
                slot: record.block.header.chain_slot.ok_or_else(|| {
                    WorldError::DistributedValidationFailed {
                        reason: "retention candidate tick context missing chain_slot".to_string(),
                    }
                })?,
                epoch: record.block.header.chain_epoch.ok_or_else(|| {
                    WorldError::DistributedValidationFailed {
                        reason: "retention candidate tick context missing chain_epoch".to_string(),
                    }
                })?,
                node_block_hash: record.block.header.node_block_hash.clone().ok_or_else(|| {
                    WorldError::DistributedValidationFailed {
                        reason: "retention candidate tick context missing node_block_hash"
                            .to_string(),
                    }
                })?,
                action_root: record.block.header.action_root.clone().ok_or_else(|| {
                    WorldError::DistributedValidationFailed {
                        reason: "retention candidate tick context missing action_root".to_string(),
                    }
                })?,
                authority_node_id: record.certificate.authority_source.clone(),
                committed_at_unix_ms: record.block.header.committed_at_unix_ms.ok_or_else(
                    || WorldError::DistributedValidationFailed {
                        reason: "retention candidate tick context missing committed_at_unix_ms"
                            .to_string(),
                    },
                )?,
            })
        } else {
            None
        };
        let refreshed = self.build_tick_consensus_record_from_events(
            tick,
            &record.certificate.authority_source,
            record.certificate.submission_role,
            committed_context.as_ref(),
            tick_events,
            state_root,
        )?;
        self.install_prepared_tick_consensus_record(refreshed)?;
        Ok(())
    }
}
