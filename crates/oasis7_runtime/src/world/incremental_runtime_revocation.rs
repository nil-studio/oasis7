//! Borrow the original Runtime revocation owner without DTO conversion.
use super::World;
use oasis7_formal_protocol::{
    incremental_execution::ExecutionWriteV1,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_field_member::{MemberRecordError, MemberRecordWriteContext},
    incremental_runtime_revocation::{
        RevocationWriteWorkV1, prepare_runtime_revocation_genesis_v1,
    },
};
impl World {
    pub fn prepare_iws_runtime_revocation_genesis_v1<
        W: ValueNodeWriter + ValueNodeReader + StatePageWriter,
        C: FnMut() -> bool,
        S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
    >(
        &self,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut RevocationWriteWorkV1,
        sink: &mut S,
    ) -> Result<(), MemberRecordError> {
        prepare_runtime_revocation_genesis_v1(&self.capability_revocation_state, ctx, work, sink)
    }
}
