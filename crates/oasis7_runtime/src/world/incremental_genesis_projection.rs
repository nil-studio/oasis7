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
use oasis7_world_state::incremental_runtime_artifact_content::RuntimeArtifactOriginalWorkDebitV1;

/// No public constructor, Clone or root-based setter. The columns cannot be
/// detached into an accepted source: full Current/readback/seal gates remain.
pub struct RuntimeProjectedGenesisV1 {
    outer: MemoryReservation,
    business_metadata:
        super::incremental_normal_business_commitment::PreparedNormalBusinessMetadataGenesisV1,
    genesis: RuntimeGenesisSummaryV1,
    columns: NormalProjectionColumnSeedDataV1,
    witness: super::incremental_normal_projection_bootstrap::ProjectionGenesisWitnessV1,
    event_root: [u8; 32],
    event_count: usize,
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

    /// Complete original GEN journal from this same immutable World capture.
    /// This DATA carries no accepted-parent or normal transaction authority.
    pub fn formal_event_commitment_bounded_v1(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<([u8; 32], usize), BodyMutationError> {
        self.validate_context_bounded_v1(common)?;
        common
            .charge(1, 40)
            .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
        Ok((self.event_root, self.event_count))
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
// Static selection preserves the old public two-argument callback without a
// live mode value, a borrowing shim created before admission, or another Core.
trait ArtifactGenesis<P> {
    fn prepare<
        W: StatePageWriter + ValueNodeReader + SchedulePageWriter,
        C: FnMut() -> bool,
        A: FnMut(
            u64,
            &oasis7_world_state::ActionEnvelope,
        ) -> Result<OriginalArtifactRefV1, ValueTreeError>,
        L: FnMut(&str, u64, &oasis7_world_state::DomainEvent) -> Result<[u8; 32], BodyMutationError>,
        S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
    >(
        world: &World,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut RuntimeGenesisWorkV1,
        persist_artifact: &mut P,
        callbacks: (&mut A, &mut L, &mut S),
        witness: &mut super::incremental_normal_projection_bootstrap::ProjectionGenesisWitnessV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<RuntimeGenesisSummaryV1, RuntimeProjectedGenesisErrorV1>;
}
struct BorrowedArtifacts;
struct CommonArtifacts;

impl<P: FnMut(OriginalArtifactRefV1, &[u8]) -> Result<(), ValueTreeError>> ArtifactGenesis<P>
    for BorrowedArtifacts
{
    fn prepare<
        W: StatePageWriter + ValueNodeReader + SchedulePageWriter,
        C: FnMut() -> bool,
        A: FnMut(
            u64,
            &oasis7_world_state::ActionEnvelope,
        ) -> Result<OriginalArtifactRefV1, ValueTreeError>,
        L: FnMut(&str, u64, &oasis7_world_state::DomainEvent) -> Result<[u8; 32], BodyMutationError>,
        S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
    >(
        world: &World,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut RuntimeGenesisWorkV1,
        persist_artifact: &mut P,
        callbacks: (&mut A, &mut L, &mut S),
        witness: &mut super::incremental_normal_projection_bootstrap::ProjectionGenesisWitnessV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<RuntimeGenesisSummaryV1, RuntimeProjectedGenesisErrorV1> {
        let (persist_action, mailbox_original, sink) = callbacks;
        let shared = std::cell::RefCell::new(&mut *common);
        let first = std::cell::RefCell::new(None);
        let result = world.prepare_iws_genesis_v1(
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
        result.map_err(RuntimeProjectedGenesisErrorV1::Genesis)
    }
}

impl<
    P: FnMut(
        OriginalArtifactRefV1,
        &[u8],
        &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), ValueTreeError>,
> ArtifactGenesis<P> for CommonArtifacts
{
    fn prepare<
        W: StatePageWriter + ValueNodeReader + SchedulePageWriter,
        C: FnMut() -> bool,
        A: FnMut(
            u64,
            &oasis7_world_state::ActionEnvelope,
        ) -> Result<OriginalArtifactRefV1, ValueTreeError>,
        L: FnMut(&str, u64, &oasis7_world_state::DomainEvent) -> Result<[u8; 32], BodyMutationError>,
        S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
    >(
        world: &World,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        work: &mut RuntimeGenesisWorkV1,
        persist_artifact: &mut P,
        callbacks: (&mut A, &mut L, &mut S),
        witness: &mut super::incremental_normal_projection_bootstrap::ProjectionGenesisWitnessV1,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<RuntimeGenesisSummaryV1, RuntimeProjectedGenesisErrorV1> {
        let (persist_action, mailbox_original, sink) = callbacks;
        // Two closures capture four thin references. Six synchronous producer
        // layers plus two temporary hook slots carry the optional work hook.
        // Common arguments, both potentially overlapping RefMut guards and the
        // reservation guard are separate from the retained projection owner.
        let bridge_bytes = 4 * std::mem::size_of::<&()>()
            + 8 * std::mem::size_of::<Option<&RuntimeArtifactOriginalWorkDebitV1<'_>>>()
            + 2 * std::mem::size_of::<&mut FormalBoundedContextV1<'_>>()
            + std::mem::size_of::<std::cell::RefMut<'_, &mut FormalBoundedContextV1<'_>>>()
            + std::mem::size_of::<std::cell::RefMut<'_, Option<BodyMutationError>>>()
            + std::mem::size_of::<MemoryReservation>();
        common.charge(10, bridge_bytes as u64).map_err(|error| {
            RuntimeProjectedGenesisErrorV1::Source(BodyMutationError::Original(Box::new(error)))
        })?;
        let _bridge_fee = common
            .memory()
            .reserve(bridge_bytes, MemoryClass::Ordinary)
            .map_err(|error| {
                RuntimeProjectedGenesisErrorV1::Source(BodyMutationError::Original(Box::new(error)))
            })?;
        let shared = std::cell::RefCell::new(&mut *common);
        let first = std::cell::RefCell::new(None);
        let debit = |steps, bytes| {
            if let Err(error) = shared.borrow_mut().charge(steps, bytes) {
                let mut first = first.borrow_mut();
                if first.is_none() {
                    *first = Some(BodyMutationError::Original(Box::new(error)));
                }
                return Err(ValueTreeError::Invalid("GEN artifact original admission"));
            }
            Ok(())
        };
        let mut persist =
            |reference, bytes: &[u8]| persist_artifact(reference, bytes, &mut shared.borrow_mut());
        let result = world.prepare_iws_genesis_with_artifact_work_v1(
            ctx,
            work,
            (&mut persist, Some(&debit)),
            persist_action,
            mailbox_original,
            &mut |row| {
                if let Err(error) = witness.observe(&row, &mut shared.borrow_mut()) {
                    let mut first = first.borrow_mut();
                    if first.is_none() {
                        *first = Some(error);
                    }
                    return Err(ValueTreeError::Invalid("GEN projection witness admission"));
                }
                sink(row)
            },
        );
        if let Some(error) = first.into_inner() {
            return Err(RuntimeProjectedGenesisErrorV1::Source(error));
        }
        result.map_err(RuntimeProjectedGenesisErrorV1::Genesis)
    }
}

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
        self.prepare_iws_genesis_projection_inner_v1::<BorrowedArtifacts, _, _, _, _, _, _>(
            ctx,
            work,
            (persist_artifact, persist_action, mailbox_original, sink),
            common,
        )
    }

    /// The same actual GEN borrow, with prehash work and the physical P callback
    /// sharing Common. The result remains provisional DATA.
    pub fn prepare_iws_genesis_with_projection_and_artifact_work_v1<
        W: StatePageWriter + ValueNodeReader + SchedulePageWriter,
        C: FnMut() -> bool,
        P: FnMut(
            OriginalArtifactRefV1,
            &[u8],
            &mut FormalBoundedContextV1<'_>,
        ) -> Result<(), ValueTreeError>,
        A: FnMut(
            u64,
            &oasis7_world_state::ActionEnvelope,
        ) -> Result<OriginalArtifactRefV1, ValueTreeError>,
        L: FnMut(&str, u64, &oasis7_world_state::DomainEvent) -> Result<[u8; 32], BodyMutationError>,
        S: FnMut(ReservedValue<ExecutionWriteV1>) -> Result<(), ValueTreeError>,
    >(
        &self,
        ctx: &mut MemberRecordWriteContext<'_, W, C>,
        artifact: (&mut RuntimeGenesisWorkV1, &mut P),
        persist_action: &mut A,
        mailbox_original: &mut L,
        sink: &mut S,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<RuntimeProjectedGenesisV1, RuntimeProjectedGenesisErrorV1> {
        let (work, persist_artifact) = artifact;
        self.prepare_iws_genesis_projection_inner_v1::<CommonArtifacts, _, _, _, _, _, _>(
            ctx,
            work,
            (persist_artifact, persist_action, mailbox_original, sink),
            common,
        )
    }

    fn prepare_iws_genesis_projection_inner_v1<
        M: ArtifactGenesis<P>,
        P,
        W: StatePageWriter + ValueNodeReader + SchedulePageWriter,
        C: FnMut() -> bool,
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
        callbacks: (&mut P, &mut A, &mut L, &mut S),
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<RuntimeProjectedGenesisV1, RuntimeProjectedGenesisErrorV1> {
        let (persist_artifact, persist_action, mailbox_original, sink) = callbacks;
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
        let event_count = self.journal.events.len();
        let event_steps = u64::try_from(event_count)
            .ok()
            .and_then(|count| count.checked_add(1))
            .ok_or(RuntimeProjectedGenesisErrorV1::Source(
                BodyMutationError::Invalid("GEN journal event count"),
            ))?;
        common.consume_steps(event_steps).map_err(|e| {
            RuntimeProjectedGenesisErrorV1::Source(BodyMutationError::Original(Box::new(e)))
        })?;
        let event_root =
            super::incremental_normal_event_commitment::hash_world_event_sequence_bounded_v1(
                self.journal.events.iter(),
                common,
            )
            .map_err(RuntimeProjectedGenesisErrorV1::Source)?;
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
        let genesis = M::prepare(
            self,
            ctx,
            work,
            persist_artifact,
            (persist_action, mailbox_original, sink),
            &mut witness,
            common,
        )?;
        columns
            .validate_context(common)
            .map_err(RuntimeProjectedGenesisErrorV1::Source)?;
        Ok(RuntimeProjectedGenesisV1 {
            outer,
            genesis,
            columns,
            witness,
            business_metadata,
            event_root,
            event_count,
        })
    }
}
