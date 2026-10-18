use super::{World, WorldError};
use crate::{RuntimeCommittedTickContext, TICK_BLOCK_HEADER_SCHEMA_V2, WorldState};

impl World {
    pub(crate) fn mutate_state_and_refresh_tick_consensus_for_test(
        &mut self,
        edit: impl FnOnce(&mut WorldState),
    ) -> Result<(), WorldError> {
        edit(&mut self.state);
        let Some(record) = self.tick_consensus_records.last().cloned() else {
            return Ok(());
        };
        let tick = record.block.header.tick;
        let tick_events: Vec<_> = self
            .journal
            .events
            .iter()
            .filter(|event| event.time == tick)
            .cloned()
            .collect();
        let state_root = self.current_state_root_hash()?;
        let committed_context = if record.block.header.schema_version >= TICK_BLOCK_HEADER_SCHEMA_V2
        {
            Some(RuntimeCommittedTickContext {
                height: record.block.header.chain_height.ok_or_else(|| {
                    WorldError::DistributedValidationFailed {
                        reason: "test fixture tick context missing chain_height".to_string(),
                    }
                })?,
                slot: record.block.header.chain_slot.ok_or_else(|| {
                    WorldError::DistributedValidationFailed {
                        reason: "test fixture tick context missing chain_slot".to_string(),
                    }
                })?,
                epoch: record.block.header.chain_epoch.ok_or_else(|| {
                    WorldError::DistributedValidationFailed {
                        reason: "test fixture tick context missing chain_epoch".to_string(),
                    }
                })?,
                node_block_hash: record.block.header.node_block_hash.clone().ok_or_else(|| {
                    WorldError::DistributedValidationFailed {
                        reason: "test fixture tick context missing node_block_hash".to_string(),
                    }
                })?,
                action_root: record.block.header.action_root.clone().ok_or_else(|| {
                    WorldError::DistributedValidationFailed {
                        reason: "test fixture tick context missing action_root".to_string(),
                    }
                })?,
                authority_node_id: record.certificate.authority_source.clone(),
                committed_at_unix_ms: record.block.header.committed_at_unix_ms.ok_or_else(
                    || WorldError::DistributedValidationFailed {
                        reason: "test fixture tick context missing committed_at_unix_ms"
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
            &tick_events,
            state_root,
        )?;
        self.install_prepared_tick_consensus_record(refreshed)?;
        Ok(())
    }
}
