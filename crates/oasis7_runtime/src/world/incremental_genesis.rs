//! Sole streaming composition of current Runtime and WorldState GEN producers.
//! The caller owns original history, complete qualification and publication.
use super::{
    World,
    incremental_body::{BodyMutationContext, BodyMutationError},
    incremental_genesis_raw::RuntimeRawGenesisWorkV1,
    incremental_genesis_typed::{RuntimeTypedGenesisSummaryV1, RuntimeTypedGenesisWorkV1},
};
use oasis7_formal_protocol::{
    incremental_artifacts::OriginalArtifactRefV1,
    incremental_execution::ExecutionWriteV1,
    incremental_schedule::SchedulePageWriter,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_world_state::{
    ActionEnvelope, DomainEvent,
    incremental_field_member::{MemberRecordError, MemberRecordWriteContext},
};

pub struct RuntimeGenesisWorkV1 {
    pub typed: RuntimeTypedGenesisWorkV1,
    pub raw: RuntimeRawGenesisWorkV1,
}

#[cfg(test)]
#[path = "incremental_genesis_tests.rs"]
mod tests;
pub struct RuntimeGenesisSummaryV1 {
    pub typed: RuntimeTypedGenesisSummaryV1,
    pub raw_rows: u64,
}
#[derive(Debug)]
pub enum RuntimeGenesisErrorV1 {
    Typed(MemberRecordError),
    Raw(BodyMutationError),
}
impl std::fmt::Display for RuntimeGenesisErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Typed(e) => e.fmt(f),
            Self::Raw(e) => e.fmt(f),
        }
    }
}
impl std::error::Error for RuntimeGenesisErrorV1 {}
impl From<ValueTreeError> for RuntimeGenesisErrorV1 {
    fn from(e: ValueTreeError) -> Self {
        Self::Typed(MemberRecordError::Value(e))
    }
}
impl World {
    /// Nothing is installed or accepted here. Every output is a provisional
    /// creation write; any error requires the caller to discard the prefix.
    /// All original callbacks must persist actual original bytes/digests.
    pub fn prepare_iws_genesis_v1<
        W: StatePageWriter + ValueNodeReader + SchedulePageWriter,
        C: FnMut() -> bool,
        P: FnMut(OriginalArtifactRefV1, &[u8]) -> Result<(), ValueTreeError>,
        A: FnMut(u64, &ActionEnvelope) -> Result<OriginalArtifactRefV1, ValueTreeError>,
        L: FnMut(&str, u64, &DomainEvent) -> Result<[u8; 32], BodyMutationError>,
        S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
    >(
        &self,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut RuntimeGenesisWorkV1,
        persist_artifact: &mut P,
        persist_action: &mut A,
        mailbox_original: &mut L,
        sink: &mut S,
    ) -> Result<RuntimeGenesisSummaryV1, RuntimeGenesisErrorV1> {
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        let mut output = |row: ReservedValue<ExecutionWriteV1>| {
            oasis7_world_state::incremental_world_key_router::route_world_key_v1(
                row.namespace,
                &row.mutation.key,
            )?;
            sink(row)
        };
        let typed = self
            .prepare_iws_runtime_typed_genesis_v1(
                ctx,
                &mut work.typed,
                persist_artifact,
                persist_action,
                &mut output,
            )
            .map_err(RuntimeGenesisErrorV1::Typed)?;
        let before = work.raw.writes_remaining;
        self.prepare_iws_runtime_raw_genesis_v1(
            &mut BodyMutationContext {
                sink: &mut *ctx.sink,
                memory: ctx.memory,
                io: &mut *ctx.io,
                cancelled: &mut ctx.cancelled,
            },
            &mut work.raw,
            mailbox_original,
            &mut output,
        )
        .map_err(RuntimeGenesisErrorV1::Raw)?;
        if (ctx.cancelled)() {
            return Err(ValueTreeError::Cancelled.into());
        }
        Ok(RuntimeGenesisSummaryV1 {
            typed,
            raw_rows: before
                .checked_sub(work.raw.writes_remaining)
                .ok_or(ValueTreeError::Invalid("raw GEN counter increased"))?,
        })
    }
}
