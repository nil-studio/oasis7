//! One immutable GEN borrow feeds both the existing typed and raw physical producers
//! and its exact typed projection columns. The result remains provisional DATA.
use super::{
    World,
    incremental_body::BodyMutationError,
    incremental_genesis::{RuntimeGenesisErrorV1, RuntimeGenesisSummaryV1, RuntimeGenesisWorkV1},
    incremental_normal_projection_commitment::{
        NormalProjectionColumnInitializerV1, NormalProjectionColumnSeedDataV1,
    },
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_artifacts::OriginalArtifactRefV1,
    incremental_execution::ExecutionWriteV1,
    incremental_schedule::SchedulePageWriter,
    incremental_state::StatePageWriter,
    incremental_value::{ValueNodeReader, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, MemoryReservation, ReservedValue};
use oasis7_world_state::incremental_field_member::MemberRecordWriteContext;

/// No public constructor, Clone or root-based setter. The columns cannot be
/// detached into an accepted source: full Current/readback/seal gates remain.
pub struct RuntimeProjectedGenesisV1 {
    outer: MemoryReservation,
    business_metadata:
        super::incremental_normal_business_commitment::PreparedNormalBusinessMetadataGenesisV1,
    genesis: RuntimeGenesisSummaryV1,
    columns: NormalProjectionColumnSeedDataV1,
    witness: super::incremental_normal_projection_bootstrap::ProjectionGenesisWitnessV1,
}
impl RuntimeProjectedGenesisV1 {
    pub fn validate_context_bounded_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), BodyMutationError> {
        common
            .consume_steps(1)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        if !self.outer.uses_budget(common.memory()) {
            return Err(BodyMutationError::Invalid("GEN projected owner pool"));
        }
        self.columns.validate_context(common)?;
        self.witness.validate_context(common)?;
        self.business_metadata.validate_context(common)
    }

    pub(in crate::world) fn witness_for_bootstrap_v1(
        &self,
    ) -> &super::incremental_normal_projection_bootstrap::ProjectionGenesisWitnessV1 {
        &self.witness
    }
    pub fn genesis(&self) -> &RuntimeGenesisSummaryV1 {
        &self.genesis
    }
    pub(in crate::world) fn columns(&self) -> &NormalProjectionColumnSeedDataV1 {
        &self.columns
    }
    pub(in crate::world) fn business_metadata(
        &self,
    ) -> &super::incremental_normal_business_commitment::PreparedNormalBusinessMetadataGenesisV1
    {
        &self.business_metadata
    }
    pub(in crate::world) fn into_bootstrap_parts_v1(
        self,
    ) -> (
        RuntimeGenesisSummaryV1,
        NormalProjectionColumnSeedDataV1,
        super::incremental_normal_business_commitment::PreparedNormalBusinessMetadataGenesisV1,
        super::incremental_normal_projection_bootstrap::ProjectionGenesisWitnessV1,
        oasis7_proto::memory_budget::MemoryReservation,
    ) {
        (
            self.genesis,
            self.columns,
            self.business_metadata,
            self.witness,
            self.outer,
        )
    }
    pub(in crate::world) fn into_parts(
        self,
    ) -> (
        RuntimeGenesisSummaryV1,
        NormalProjectionColumnSeedDataV1,
        super::incremental_normal_business_commitment::PreparedNormalBusinessMetadataGenesisV1,
    ) {
        (self.genesis, self.columns, self.business_metadata)
    }
}
#[derive(Debug)]
pub enum RuntimeProjectedGenesisErrorV1 {
    Genesis(RuntimeGenesisErrorV1),
    Source(BodyMutationError),
    Projection(oasis7_world_state::incremental_projection::ProjectionError<BodyMutationError>),
}
impl std::fmt::Display for RuntimeProjectedGenesisErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "projected GEN: {self:?}")
    }
}
impl std::error::Error for RuntimeProjectedGenesisErrorV1 {}
impl World {
    /// Actual GEN only; never used to hydrate or inspect an ordinary World.
    /// Every borrowed value stays the same across both original typed visitors.
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_iws_genesis_with_projection_v1<
        W: StatePageWriter + ValueNodeReader + SchedulePageWriter,
        C: FnMut() -> bool,
        P: FnMut(OriginalArtifactRefV1, &[u8]) -> Result<(), ValueTreeError>,
        A: FnMut(
            u64,
            &oasis7_world_state::ActionEnvelope,
        ) -> Result<OriginalArtifactRefV1, ValueTreeError>,
        L: FnMut(&str, u64, &oasis7_world_state::DomainEvent) -> Result<[u8; 32], BodyMutationError>,
        S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
    >(
        &self,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut RuntimeGenesisWorkV1,
        persist_artifact: &mut P,
        persist_action: &mut A,
        mailbox_original: &mut L,
        sink: &mut S,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<RuntimeProjectedGenesisV1, RuntimeProjectedGenesisErrorV1> {
        common.consume_steps(1).map_err(|e| {
            RuntimeProjectedGenesisErrorV1::Source(BodyMutationError::Original(Box::new(e)))
        })?;
        if !ctx.memory.same_pool(common.memory()) {
            return Err(RuntimeProjectedGenesisErrorV1::Source(
                BodyMutationError::Invalid("GEN projection pool"),
            ));
        }
        let outer_bytes = std::mem::size_of::<RuntimeProjectedGenesisV1>();
        common.charge(1, outer_bytes as u64).map_err(|e| {
            RuntimeProjectedGenesisErrorV1::Source(BodyMutationError::Original(Box::new(e)))
        })?;
        let outer = common
            .memory()
            .reserve(outer_bytes, MemoryClass::Ordinary)
            .map_err(|e| {
                RuntimeProjectedGenesisErrorV1::Source(BodyMutationError::Original(Box::new(e)))
            })?;
        let mut columns = NormalProjectionColumnInitializerV1::new(common)
            .map_err(RuntimeProjectedGenesisErrorV1::Source)?;
        self.visit_iws_genesis_fields(&mut columns)
            .map_err(RuntimeProjectedGenesisErrorV1::Projection)?;
        columns
            .capture_original_state_fields_v1(&self.state)
            .map_err(RuntimeProjectedGenesisErrorV1::Source)?;
        columns
            .capture_original_auxiliary_fields_v1(self)
            .map_err(RuntimeProjectedGenesisErrorV1::Source)?;
        let columns = columns
            .finish()
            .map_err(RuntimeProjectedGenesisErrorV1::Source)?;
        let business_metadata = self
            .prepare_normal_business_metadata_parent_v1(common)
            .map_err(RuntimeProjectedGenesisErrorV1::Source)?;
        let mut witness =
            super::incremental_normal_projection_bootstrap::ProjectionGenesisWitnessV1::new(common)
                .map_err(RuntimeProjectedGenesisErrorV1::Source)?;
        let genesis = {
            let shared = std::cell::RefCell::new(&mut *common);
            let first = std::cell::RefCell::new(None);
            let result = self.prepare_iws_genesis_v1(
                ctx,
                work,
                persist_artifact,
                persist_action,
                mailbox_original,
                &mut |row| {
                    if let Err(error) = witness.observe(&row, &mut shared.borrow_mut()) {
                        *first.borrow_mut() = Some(error);
                        return Err(ValueTreeError::Invalid("GEN projection witness admission"));
                    }
                    sink(row)
                },
            );
            if let Some(error) = first.into_inner() {
                return Err(RuntimeProjectedGenesisErrorV1::Source(error));
            }
            result.map_err(RuntimeProjectedGenesisErrorV1::Genesis)?
        };
        columns
            .validate_context(common)
            .map_err(RuntimeProjectedGenesisErrorV1::Source)?;
        Ok(RuntimeProjectedGenesisV1 {
            outer,
            genesis,
            columns,
            witness,
            business_metadata,
        })
    }
}
