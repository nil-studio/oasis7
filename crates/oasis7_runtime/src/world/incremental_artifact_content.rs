//! Borrow original Runtime collections, without cloning module bytes.
use super::World;
use oasis7_formal_protocol::{
    incremental_artifacts::OriginalArtifactRefV1,
    incremental_execution::ExecutionWriteV1,
    incremental_value::{ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    incremental_field_member::{MemberRecordError, MemberRecordWriteContext},
    incremental_runtime_artifact_content::{
        RuntimeArtifactOriginalWorkV1, prepare_runtime_artifact_content_genesis_v1,
    },
};
impl World {
    pub fn prepare_iws_runtime_artifact_content_genesis_v1<W, C, P, S>(
        &self,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        records_remaining: &mut u64,
        work: &mut RuntimeArtifactOriginalWorkV1,
        persist: &mut P,
        sink: &mut S,
    ) -> Result<(), MemberRecordError>
    where
        W: ValueNodeWriter,
        C: FnMut() -> bool,
        P: FnMut(OriginalArtifactRefV1, &[u8]) -> Result<(), ValueTreeError>,
        S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
    {
        prepare_runtime_artifact_content_genesis_v1(
            &self.module_artifacts,
            &self.module_artifact_bytes,
            ctx,
            records_remaining,
            work,
            persist,
            sink,
        )
    }
}

#[cfg(all(test, unix))]
#[path = "incremental_artifact_content_physical_tests.rs"]
mod physical_tests;
