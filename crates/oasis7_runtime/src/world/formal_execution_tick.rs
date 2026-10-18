//! Private execution phase for a frozen formal successor. This phase never
//! enters a Snapshot or grants qualification; completion still needs P/R.
use super::{World, WorldError};
use crate::{RuntimeCommittedTickContext, TickConsensusRecord};
fn invalid(reason: impl Into<String>) -> WorldError {
    WorldError::DistributedValidationFailed {
        reason: reason.into(),
    }
}
impl World {
    pub fn builtin_release_manifest(&self) -> &crate::BuiltinReleaseManifestState {
        &self.builtin_release_manifest
    }

    pub(super) fn reject_ordinary_step_during_formal_execution(&self) -> Result<(), WorldError> {
        if self.formal_execution_tick.is_some() {
            return Err(invalid(
                "formal successor requires its exact committed context step",
            ));
        }
        Ok(())
    }
    pub fn begin_formal_execution_tick(
        &mut self,
        context: &RuntimeCommittedTickContext,
    ) -> Result<(), WorldError> {
        if context.node_block_hash.trim().is_empty() || context.action_root.trim().is_empty() {
            return Err(invalid(
                "formal successor requires original committed node/action hashes",
            ));
        }
        if self.formal_execution_tick.is_some() {
            return Err(invalid("formal successor Tick already begun"));
        }
        let parent = self
            .formal_history_reader
            .as_ref()
            .and_then(|history| history.parent.as_ref())
            .ok_or_else(|| invalid("formal successor requires qualified parent"))?;
        let tick = parent
            .logical_tick()
            .ok_or_else(|| invalid("formal successor requires hash-bound parent time"))?;
        let bytes = serde_cbor::to_vec(&self.snapshot()).map_err(|e| invalid(e.to_string()))?;
        if self.state.time != tick
            || blake3::hash(&bytes).to_hex().as_str() != parent.after_snapshot_root()
            || context.height <= tick
        {
            return Err(invalid(
                "formal successor must advance exact qualified parent Snapshot",
            ));
        }
        self.state.time = context.height;
        self.formal_execution_tick = Some((context.clone(), false));
        Ok(())
    }
    pub fn finish_formal_administrative_tick(
        &mut self,
        context: &RuntimeCommittedTickContext,
    ) -> Result<(), WorldError> {
        self.finish_formal_execution_tick_inner(context, false)
    }
    pub fn finish_formal_execution_tick(
        &mut self,
        context: &RuntimeCommittedTickContext,
    ) -> Result<(), WorldError> {
        self.finish_formal_execution_tick_inner(context, true)
    }
    fn finish_formal_execution_tick_inner(
        &mut self,
        context: &RuntimeCommittedTickContext,
        prologue_completed: bool,
    ) -> Result<(), WorldError> {
        if self.formal_execution_tick.as_ref() != Some(&(context.clone(), prologue_completed))
            || self.state.time != context.height
        {
            return Err(invalid(
                "formal Tick context/prologue differs from begun successor",
            ));
        }
        let mut staged = self.clone();
        staged.record_tick_consensus_for_committed_context(context)?;
        staged.formal_execution_tick = None;
        staged.verify_tick_consensus_chain()?;
        *self = staged;
        Ok(())
    }
    pub(super) fn validate_formal_tick_immutability(
        &self,
        candidate: &TickConsensusRecord,
    ) -> Result<(), WorldError> {
        let Some(parent) = self
            .formal_history_reader
            .as_ref()
            .and_then(|history| history.parent.as_ref())
        else {
            return Ok(());
        };
        let Some(time) = parent.logical_tick() else {
            return Ok(());
        };
        let tick = candidate.block.header.tick;
        if tick > time {
            return Ok(());
        }
        let original = if let Some(record) = parent
            .tick_consensus_records()
            .iter()
            .find(|record| record.block.header.tick == tick)
        {
            Some(record.clone())
        } else {
            let (_, reader) = self.attached_formal_history()?;
            parent
                .index_view()
                .read_tick_consensus_at_tick(tick, reader)?
        };
        if original.as_ref() != Some(candidate) {
            return Err(invalid(
                "cannot rewrite or create an already qualified Tick",
            ));
        }
        Ok(())
    }
}
