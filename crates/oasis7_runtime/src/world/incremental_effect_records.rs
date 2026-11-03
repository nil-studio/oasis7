//! Borrow original live effect collections without cloning a queue, map or params.
use super::World;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::incremental_field_member::{MemberRecordError, MemberRecordWriteContext};
use oasis7_world_state::incremental_runtime_effect_records::{
    RuntimeEffectRecordsRefV1, RuntimeEffectRecordsWorkV1,
    prepare_runtime_effect_records_genesis_v1,
};
impl World {
    /// Dedicated typed GEN over the original Runtime fields. Output remains
    /// provisional; ordinary publication, World activation and dispatch still
    /// require their existing independent authority and acceptance boundaries.
    pub fn prepare_incremental_effect_records_genesis_v1<W, C, S>(
        &self,
        context: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut RuntimeEffectRecordsWorkV1,
        sink: &mut S,
    ) -> Result<u64, MemberRecordError>
    where
        W: StatePageWriter + ValueNodeReader,
        C: FnMut() -> bool,
        S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
    {
        prepare_runtime_effect_records_genesis_v1(
            &RuntimeEffectRecordsRefV1 {
                pending: &self.pending_effects,
                inflight: &self.inflight_effects,
            },
            context,
            work,
            sink,
        )
    }
}
