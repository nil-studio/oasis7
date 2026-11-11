//! Borrow the actual persistent Runtime controls, excluding process observations.
use super::World;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_value::{ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::incremental_field_member::{MemberRecordError, MemberRecordWriteContext};
use oasis7_world_state::incremental_runtime_controls::*;
impl World {
    pub fn prepare_incremental_runtime_controls_genesis_v1<
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
    >(
        &self,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        remaining: &mut u64,
        sink: &mut S,
    ) -> Result<(), MemberRecordError> {
        prepare_runtime_controls_genesis_v1(
            &RuntimeControlsRefV1 {
                tick_consensus_authority_source: &self.tick_consensus_authority_source,
                next_governance_identity_penalty_id: self.next_governance_identity_penalty_id,
                governance_emergency_brake_until_tick: self.governance_emergency_brake_until_tick,
            },
            ctx,
            remaining,
            sink,
        )
    }
}
