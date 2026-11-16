//! Fixed cold resident production from an actual authenticated accepted parent.
//! The configuration below is process resource DATA, never acceptance authority.
use super::{
    incremental_authorization_root_current::{
        ProvisionalAuthorizationProjectionV1, with_authorization_source_read_work_v1,
    },
    incremental_body::BodyMutationError,
    incremental_normal_host_resident::{
        NormalHostResidentSourcesV1, NormalHostStartupWorkV1,
        bootstrap_normal_host_resident_sources_v1,
    },
    incremental_normal_host_work::NormalHostIoWorkV1,
    incremental_normal_process_source::{
        NormalProcessStartupSourceV1, VerifiedNormalColdProcessConfigurationV1,
    },
    incremental_normal_sources::{NormalTickParentRefsV1, bind_normal_tick_parent_v1},
};
use oasis7_formal_protocol::{
    controlled_authority::formal::{
        FormalError,
        bounded_context::{FormalBoundedContextV1, WorkIdentityV1},
    },
    incremental_execution::{ExecutionParent, ExecutionTransaction, TransactionLimits},
    incremental_material::OriginalObjectStreamReader,
    incremental_state::{MerkleNodeReader, StateNamespaceV1},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueReadContext, ValueTreeError},
};
use oasis7_proto::memory_budget::{MemoryClass, ReservedValue};
use oasis7_world_state::{
    incremental_field_overlay::WorldFieldOverlayContextV1,
    incremental_runtime_revocation::begin_runtime_revocation_current_v1,
};

pub struct NormalHostColdRuntimeConfigurationV1 {
    configuration: super::WorldRuntimeMemoryLimits,
    core: WorkIdentityV1,
    fee: ReservedValue<()>,
}
/// The exact typed configuration used by World initialization/override. A Node
/// with no override passes WorldRuntimeMemoryLimits::default(), just as the
/// original World initializer does. No max-events permission setter exists.
pub fn prepare_normal_host_cold_runtime_configuration_v1(
    configuration: super::WorldRuntimeMemoryLimits,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<NormalHostColdRuntimeConfigurationV1>, FormalError> {
    common.charge(
        1,
        std::mem::size_of::<super::WorldRuntimeMemoryLimits>() as u64,
    )?;
    let fee = common
        .memory()
        .reserve(
            std::mem::size_of::<NormalHostColdRuntimeConfigurationV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(|_| FormalError::Invalid("cold runtime configuration admission".into()))?;
    let outer = common
        .memory()
        .reserve(
            std::mem::size_of::<NormalHostColdRuntimeConfigurationV1>(),
            MemoryClass::Ordinary,
        )
        .map_err(|_| FormalError::Invalid("cold runtime configuration admission".into()))?;
    Ok(outer.retain(NormalHostColdRuntimeConfigurationV1 {
        configuration,
        core: common.work().identity(),
        fee: fee.retain(()),
    }))
}
impl NormalHostColdRuntimeConfigurationV1 {
    pub(in crate::world) fn validate_context(
        &self,
        common: &mut FormalBoundedContextV1<'_>,
    ) -> Result<(), FormalError> {
        common.consume_steps(1)?;
        if self.core != common.work().identity() || !self.fee.uses_budget(common.memory()) {
            return Err(FormalError::Invalid(
                "cold runtime configuration foreign context".into(),
            ));
        }
        Ok(())
    }
    pub(in crate::world) fn limits(&self) -> &super::WorldRuntimeMemoryLimits {
        &self.configuration
    }
}
#[derive(Debug)]
struct Cause<T: std::fmt::Debug>(T);
impl<T: std::fmt::Debug> std::fmt::Display for Cause<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl<T: std::fmt::Debug> std::error::Error for Cause<T> {}
fn original<T: std::fmt::Debug + Send + Sync + 'static>(e: T) -> BodyMutationError {
    BodyMutationError::Original(Box::new(Cause(e)))
}
fn formal(e: FormalError) -> BodyMutationError {
    BodyMutationError::Original(Box::new(e))
}
/// Fixed producer only: same parent Q, same IO/native lifetime, original seven
/// authorization joins, full five-cache GEN/cold bootstrap, no execution hook.
#[allow(clippy::too_many_arguments)]
pub fn bootstrap_authenticated_cold_normal_host_sources_v1<M, R, W, O, E, C>(
    refs: NormalTickParentRefsV1<'_>,
    state: &M,
    values: &R,
    member_sink: &mut W,
    originals: &mut O,
    projection: ProvisionalAuthorizationProjectionV1,
    projection_cache: ReservedValue<
        super::incremental_normal_projection_commitment::AcceptedNormalProjectionCacheV1,
    >,
    process: &ReservedValue<VerifiedNormalColdProcessConfigurationV1>,
    configuration: &ReservedValue<NormalHostColdRuntimeConfigurationV1>,
    io: &mut NormalHostIoWorkV1,
    startup: &mut NormalHostStartupWorkV1<'_>,
    transaction_limits: TransactionLimits,
    take_state_error: E,
    cancelled: C,
    common: &mut FormalBoundedContextV1<'_>,
) -> Result<ReservedValue<NormalHostResidentSourcesV1>, BodyMutationError>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: ValueNodeWriter,
    O: OriginalObjectStreamReader,
    O::Error: std::error::Error + Send + Sync + 'static,
    E: FnMut() -> Option<ValueTreeError> + Copy,
    C: FnMut() -> bool + Copy,
{
    configuration.validate_context(common).map_err(formal)?;
    let parent = bind_normal_tick_parent_v1(refs, common).map_err(formal)?;
    let memory = common.memory().clone();
    // Only a bounded RootSet clone for a pristine read-only bootstrap TX. Its
    // operation is the actual accepted head identity, not an input permission.
    common.charge(1, 8192).map_err(formal)?;
    let _phase = memory
        .reserve(8192, MemoryClass::Ordinary)
        .map_err(ValueTreeError::Memory)?;
    let fence = parent.fence();
    let mut tx = ExecutionTransaction::begin(
        ExecutionParent {
            commit: fence.parent_commit,
            roots: (**parent.refs().roots).clone(),
            authority: fence.authority,
        },
        parent.refs().head.verified_record_id(),
        state,
        &memory,
        transaction_limits,
    )?;
    let mut overlay = WorldFieldOverlayContextV1 {
        transaction: &mut tx,
        values: ValueReadContext {
            source: values,
            memory: &memory,
            class: MemoryClass::Ordinary,
            io: &mut io.overlay.io,
        },
        key_work: &mut io.overlay.keys,
        semantic_bytes: io.overlay.semantic_bytes,
        record_bytes: io.overlay.record_bytes,
        take_state_error,
        cancelled,
    };
    let mut read = io.current.field(
        parent
            .refs()
            .roots
            .state_root(StateNamespaceV1::Authorization),
        state,
        values,
        &memory,
        take_state_error,
        cancelled,
    );
    let mut members = io.members.members(member_sink, &memory, cancelled);
    // Fixed before==after begin performs its actual Current checks and creates
    // no writes. The exact original authority reader child meters both read
    // contexts and all typed/scan lanes; no nested Common child is active.
    let revocation = with_authorization_source_read_work_v1(
        parent.refs().roots,
        &mut overlay,
        &mut read,
        startup.authorization,
        common,
        |overlay, read, work| {
            let guard = overlay.transaction.guard();
            begin_runtime_revocation_current_v1(
                parent.refs().storage.current().runtime().revocation(),
                overlay,
                read,
                &mut work.scan,
                &mut work.typed,
                &mut members,
                &guard,
                &fence,
            )
            .map_err(Into::into)
        },
    )
    .map_err(original)?;
    let retention = super::incremental_world_event_counter::cold_configuration_retention_v1(
        configuration,
        common,
    )
    .map_err(formal)?;
    bootstrap_normal_host_resident_sources_v1(
        &parent,
        projection,
        projection_cache,
        NormalProcessStartupSourceV1::Cold(process),
        retention,
        &revocation,
        &mut overlay,
        &mut read,
        originals,
        startup,
        common,
        &fence,
    )
}

#[path="incremental_normal_host_cold_base.rs"]mod base;
pub(in crate::world) use base::prepare_authenticated_cold_normal_host_base_v1;
