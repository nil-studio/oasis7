//! Fixed authenticated Cold parent -> pristine Root -> signed projection cache
//! -> full resident. No caller cache, columns, roots or success flag enters.
use super::{
    PaidProjectionRetainedOriginalReaderV1,
    incremental_authorization_root_current::ProvisionalAuthorizationProjectionV1,
    incremental_body::BodyMutationError,
    incremental_body_event::AgentEventOriginalV1,
    incremental_module_execution_worker::RootModuleWorkerAllocatorV1,
    incremental_module_invocation::RootModuleExecutionErrorV1,
    incremental_normal_host_cold_sources::{
        NormalHostColdRuntimeConfigurationV1, prepare_authenticated_cold_normal_host_base_v1,
    },
    incremental_normal_host_context::{NormalHostContextErrorV1, with_begun_normal_root_v1},
    incremental_normal_host_resident::{NormalHostResidentSourcesV1, NormalHostStartupWorkV1},
    incremental_normal_host_work::NormalHostWorkV1,
    incremental_normal_process_source::VerifiedNormalColdProcessConfigurationV1,
    incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::{ExecutionError, ExecutionFence, ExecutionTransaction},
    incremental_material::OriginalObjectStreamReader,
    incremental_projection_sidecar::PaidVerifiedProjectionSidecarV1,
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_wasm_executor::bounded_worker::BoundedWorkerSandboxV1;
use oasis7_world_state::incremental_cognition_proof_ledger::AcceptedCognitionProofLedgerV1;
#[derive(Debug)]
pub enum NormalHostColdBootstrapErrorV1<OE> {
    Source(BodyMutationError),
    Context(NormalHostContextErrorV1<OE, RootModuleExecutionErrorV1<OE>>),
}
impl<OE> From<BodyMutationError> for NormalHostColdBootstrapErrorV1<OE> {
    fn from(e: BodyMutationError) -> Self {
        Self::Source(e)
    }
}
#[derive(Debug)]
struct SourceCause<T: std::fmt::Debug>(T);
impl<T: std::fmt::Debug> std::fmt::Display for SourceCause<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl<T: std::fmt::Debug> std::error::Error for SourceCause<T> {}
fn body(e: impl std::fmt::Debug + Send + Sync + 'static) -> BodyMutationError {
    BodyMutationError::Original(Box::new(SourceCause(e)))
}
#[allow(clippy::too_many_arguments)]
pub fn bootstrap_authenticated_cold_normal_host_resident_v1<'s, 'p, M, R, W, O, E, C, F>(
    parent: &'p ReservedValue<BoundNormalTickParentV1<'p>>,
    transaction: &mut ExecutionTransaction<'s, M>,
    state: &M,
    values: &R,
    member_sink: &mut W,
    fee_sink: &mut W,
    event_sink: &mut W,
    originals: &mut O,
    persist_original: F,
    worker: &BoundedWorkerSandboxV1<'_, RootModuleWorkerAllocatorV1>,
    projection: ProvisionalAuthorizationProjectionV1,
    process: &ReservedValue<VerifiedNormalColdProcessConfigurationV1>,
    configuration: &ReservedValue<NormalHostColdRuntimeConfigurationV1>,
    side: PaidVerifiedProjectionSidecarV1,
    accepted_ledger: Option<&ReservedValue<AcceptedCognitionProofLedgerV1>>,
    accepted_retention: &ReservedValue<oasis7_world_state::incremental_cognition_retention_ledger::AcceptedCognitionRetentionLedgerV1>,
    compiler: &crate::module_source_compiler::RuntimeSourceCompiler,
    startup: &mut NormalHostStartupWorkV1<'_>,
    work: &mut NormalHostWorkV1,
    take_state_error: E,
    cancelled: C,
    common: &mut FormalBoundedContextV1<'_>,
    live: &mut dyn FnMut() -> ExecutionFence,
) -> Result<ReservedValue<NormalHostResidentSourcesV1>, NormalHostColdBootstrapErrorV1<O::Error>>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    O: OriginalObjectStreamReader
        + PaidProjectionRetainedOriginalReaderV1<RetainedError = BodyMutationError>,
    O::Error: std::error::Error + Send + Sync + 'static,
    E: FnMut() -> Option<ValueTreeError> + Copy,
    C: FnMut() -> bool + Copy,
    F: FnMut(&mut dyn AgentEventOriginalV1) -> Result<(), BodyMutationError>,
{
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        parent.validate_context(common).map_err(body)?;
        transaction
            .validate_bootstrap_read_only_v1(&live())
            .map_err(BodyMutationError::from)?;
        let (base, _now) = prepare_authenticated_cold_normal_host_base_v1(
            parent,
            transaction,
            state,
            values,
            member_sink,
            originals,
            projection,
            process,
            configuration,
            &mut work.io,
            startup,
            take_state_error,
            cancelled,
            common,
        )?;
        let cache = {
            let root_sources = base.root_read_sources_v1();
            with_begun_normal_root_v1(parent,transaction,state,values,member_sink,fee_sink,event_sink,originals,persist_original,worker,&root_sources,work,take_state_error,cancelled,common,live,
 |root,_pending,revocation,_delivery,_current,parent_read,_admission,_factory,_recipe,_industry,_materials,driver,common,live|->Result<_,RootModuleExecutionErrorV1<O::Error>>{
 root.ctx.transaction.validate_bootstrap_read_only_v1(&live())?;
 let authorization=base.begin_bootstrap_authorization_v1(parent,root.ctx.transaction,common,&live())?;
 if driver.actions.common_identity()!=common.work().identity(){return Err(BodyMutationError::Invalid("Cold policy actual continuing work Core").into())}
 let source=super::incremental_normal_source_action_views::bind_normal_action_source_parents_v1(parent,base.parent_caches_v1().borrow(),common)?;
 let policy=source.with_sources_v1(&authorization,compiler,common,|sources,common|super::incremental_module_capability_admission_rules::read_current_bootstrap_policy_v1(sources.module_lifecycle,root.ctx,parent_read,&mut driver.actions.factory_schedule,&mut driver.actions.module_lifecycle.view(),common,&live()))?.map_err(body)?;
 let cache=root.prepare_accepted_cold_projection_cache_v1(&mut driver.final_sources.projection.loan(),side,parent,base.business_v1(),revocation,accepted_ledger,accepted_retention,&policy,base.process_source_v1(),&authorization,parent_read,&mut driver.final_sources.business,&mut driver.final_sources.terminal_metadata,&mut driver.bootstrap.finality,&mut driver.tail.cognition.hashes,common,live)?;
 root.ctx.transaction.validate_bootstrap_read_only_v1(&live())?;Ok(cache)
 }).map_err(NormalHostColdBootstrapErrorV1::Context)?
        };
        transaction
            .validate_bootstrap_read_only_v1(&live())
            .map_err(BodyMutationError::from)?;
        let resident = base.finish_projection_cache_v1(parent, cache, common)?;
        transaction
            .validate_bootstrap_read_only_v1(&live())
            .map_err(BodyMutationError::from)?;
        Ok(resident)
    }));
    match attempt {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => {
            let _: Result<(), ExecutionError> =
                transaction.contribute(|_| Err(ExecutionError::Aborted));
            Err(error)
        }
        Err(panic) => {
            let _: Result<(), ExecutionError> =
                transaction.contribute(|_| Err(ExecutionError::Aborted));
            std::panic::resume_unwind(panic)
        }
    }
}
