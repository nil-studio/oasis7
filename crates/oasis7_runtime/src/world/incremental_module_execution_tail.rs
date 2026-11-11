//! Route the real last event after intervening module output contributions.
use super::*;
use crate::world::incremental_world_event_tail::{
    CurrentWorldEventTailWorkV1, recover_latest_counted_world_event_v1,
};
impl<'a, 't, 's, 'v, M, R, E, C, W, F, O>
    RootModuleExecutionContextV1<'a, 't, 's, 'v, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader
        + ValueNodeWriter
        + oasis7_formal_protocol::incremental_state::StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader,
{
    /// World::step routes journal.last() after PostAction, which may differ
    /// from the original business result. Recover only the counter's actual
    /// typed producer tail; no committed History or World scan is involved.
    pub(in crate::world) fn route_latest_world_event_v1(
        &mut self,
        directory: &ReservedValue<AcceptedInvocationDirectoryV1>,
        enumeration: &mut CurrentInvocationEnumerationWorkV1,
        work: &mut CurrentWorldEventTailWorkV1,
        live_owner: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<usize, RootModuleExecutionErrorV1<O::Error>> {
        let result = (|| {
            let owner = recover_latest_counted_world_event_v1(
                self.events.world_events,
                self.ctx.transaction,
                self.originals,
                self.ctx.values.memory,
                self.now,
                work,
                &mut self.ctx.cancelled,
                live_owner,
            )
            .map_err(RootModuleExecutionErrorV1::EventTail)?;
            let Some(owner) = owner else {
                return Ok(0);
            };
            let event = crate::world::incremental_module_execution_event::PersistedWorldEventV1::from_counted_tail(&owner);
            self.route_persisted_event_v1(&event, None, directory, enumeration, live_owner)
        })();
        if result.is_err() {
            let _: Result<(), oasis7_formal_protocol::incremental_execution::ExecutionError> =
                self.ctx.transaction.contribute(|_| {
                    Err(oasis7_formal_protocol::incremental_execution::ExecutionError::Aborted)
                });
        }
        result
    }
}
