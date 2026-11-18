//! Accepted GEN -> the original readonly Root projection check -> resident.
//! The caller supplies real IO and the opaque output of the immutable GEN
//! producer. No caller-owned cache, raw root tuple or completion flag enters.
use super::*;
use crate::world::{
    DevLocalParticipantAuthority, PaidProjectionRetainedOriginalReaderV1,
    incremental_body_event::AgentEventOriginalV1,
    incremental_genesis::RuntimeGenesisSummaryV1,
    incremental_genesis_projection::RuntimeProjectedGenesisV1,
    incremental_module_execution_worker::RootModuleWorkerAllocatorV1,
    incremental_module_invocation::RootModuleExecutionErrorV1,
    incremental_normal_business_commitment::PreparedNormalBusinessMetadataGenesisV1,
    incremental_normal_host_cold_sources::prepare_authenticated_normal_host_base_v1,
    incremental_normal_host_context::{NormalHostContextErrorV1, with_begun_normal_root_v1},
    incremental_normal_host_work::NormalHostWorkV1,
    incremental_normal_process_source::NormalProcessStartupSourceV1,
};
use oasis7_formal_protocol::{
    incremental_execution::ExecutionTransaction,
    incremental_projection_sidecar::PaidVerifiedProjectionSidecarV1,
    incremental_state::StatePageWriter, incremental_value::ValueNodeWriter,
};
use oasis7_wasm_executor::bounded_worker::BoundedWorkerSandboxV1;
use oasis7_world_state::incremental_cognition_proof_ledger::AcceptedCognitionProofLedgerV1;

/// An explicit process-authority input, checked against the same immutable GEN
/// producer's captured local pin. Governed never falls back to DevLocal.
pub enum NormalHostGenesisAuthorityV1<'a> {
    Governed,
    DevLocal(&'a DevLocalParticipantAuthority),
}
impl NormalHostGenesisAuthorityV1<'_> {
    fn source<'a>(
        &'a self,
        genesis: &'a RuntimeGenesisSummaryV1,
    ) -> NormalProcessStartupSourceV1<'a> {
        match self {
            Self::Governed => {
                NormalProcessStartupSourceV1::GenesisGoverned(genesis.typed.process_source_v1())
            }
            Self::DevLocal(authority) => NormalProcessStartupSourceV1::GenesisDevLocal {
                genesis: genesis.typed.process_source_v1(),
                authority,
            },
        }
    }
}

#[derive(Debug)]
pub enum NormalHostGenesisBootstrapErrorV1<OE> {
    Source(BodyMutationError),
    Context(NormalHostContextErrorV1<OE, RootModuleExecutionErrorV1<OE>>),
}
impl<OE> From<BodyMutationError> for NormalHostGenesisBootstrapErrorV1<OE> {
    fn from(error: BodyMutationError) -> Self {
        Self::Source(error)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn bootstrap_authenticated_genesis_normal_host_resident_v1<'s, 'p, M, R, W, O, E, C, F>(
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
    projected: RuntimeProjectedGenesisV1,
    authority: NormalHostGenesisAuthorityV1<'_>,
    retention: ReservedValue<WorldJournalRetentionV1>,
    side: PaidVerifiedProjectionSidecarV1,
    accepted_ledger: Option<&ReservedValue<AcceptedCognitionProofLedgerV1>>,
    compiler: &crate::module_source_compiler::RuntimeSourceCompiler,
    work: &mut NormalHostWorkV1,
    take_state_error: E,
    cancelled: C,
    common: &mut FormalBoundedContextV1<'_>,
    live: &mut dyn FnMut() -> ExecutionFence,
) -> Result<
    (
        ReservedValue<NormalHostResidentSourcesV1>,
        RuntimeGenesisSummaryV1,
        PreparedNormalBusinessMetadataGenesisV1,
    ),
    NormalHostGenesisBootstrapErrorV1<O::Error>,
>
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
        parent.validate_context(common).map_err(formal)?;
        work.validate_context(common)?;
        projected.validate_context_bounded_v1(common)?;
        transaction
            .validate_bootstrap_read_only_v1(&live())
            .map_err(BodyMutationError::from)?;
        let (base, _now) = {
            let (io, mut startup) = work.startup_loans();
            prepare_authenticated_normal_host_base_v1(
                parent,
                transaction,
                state,
                values,
                member_sink,
                originals,
                projection,
                authority.source(projected.genesis()),
                retention,
                io,
                &mut startup,
                take_state_error,
                cancelled,
                common,
            )?
        };
        let (cache, summary, metadata) = {
            let root_sources = base.root_read_sources_v1();
            with_begun_normal_root_v1(
                parent, transaction, state, values, member_sink, fee_sink,
                event_sink, originals, persist_original, worker, &root_sources,
                work, take_state_error, cancelled, common, live,
                |root, _pending, revocation, _delivery, _current, parent_read,
                 _admission, _factory, _recipe, _industry, _materials, driver,
                 common, live| -> Result<_, RootModuleExecutionErrorV1<O::Error>> {
                    root.ctx.transaction.validate_bootstrap_read_only_v1(&live())?;
                    let authorization = base.begin_bootstrap_authorization_v1(
                        parent, root.ctx.transaction, common, &live(),
                    )?;
                    if driver.actions.common_identity() != common.work().identity() {
                        return Err(BodyMutationError::Invalid("GEN policy continuing work Core").into());
                    }
                    let source = crate::world::incremental_normal_source_action_views::bind_normal_action_source_parents_v1(
                        parent, base.parent_caches_v1().borrow(), common,
                    )?;
                    let policy = source.with_sources_v1(
                        &authorization, compiler, common, |sources, common| {
                            crate::world::incremental_module_capability_admission_rules::read_current_bootstrap_policy_v1(
                                sources.module_lifecycle, root.ctx, parent_read,
                                &mut driver.actions.factory_schedule,
                                &mut driver.actions.module_lifecycle.view(), common, &live(),
                            )
                        },
                    )?.map_err(original)?;
                    root.prepare_accepted_genesis_projection_cache_v1(
                        projected, side, parent, base.business_v1(), revocation,
                        accepted_ledger, &policy, base.process_source_v1(),
                        &authorization, parent_read, &mut driver.final_sources.business,
                        &mut driver.final_sources.terminal_metadata,
                        &mut driver.bootstrap.finality, &mut driver.tail.cognition.hashes,
                        common, live,
                    )
                },
            ).map_err(NormalHostGenesisBootstrapErrorV1::Context)?
        };
        transaction
            .validate_bootstrap_read_only_v1(&live())
            .map_err(BodyMutationError::from)?;
        let resident = base.finish_projection_cache_v1(parent, cache, common)?;
        transaction
            .validate_bootstrap_read_only_v1(&live())
            .map_err(BodyMutationError::from)?;
        Ok((resident, summary, metadata))
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

#[path = "incremental_normal_host_genesis_replica.rs"]
mod replica;
pub use replica::bootstrap_authenticated_genesis_replica_resident_v1;

#[cfg(any(test, feature = "test_tier_required"))]
#[path = "incremental_normal_host_genesis_hint_fixture.rs"]
mod hint_fixture;
