//! Actual readonly GEN cache preparation inside the same Root/physical/source
//! scope. No accepted cache is required to read its own bootstrap metadata.
use crate::world::{
    PaidProjectionRetainedOriginalReaderV1,
    incremental_body::BodyMutationError,
    incremental_genesis::RuntimeGenesisSummaryV1,
    incremental_genesis_projection::RuntimeProjectedGenesisV1,
    incremental_module_capability_admission_rules::PaidCurrentBootstrapPolicyV1,
    incremental_module_invocation::*,
    incremental_normal_business_commitment::{
        NormalBusinessMetadataParentSourceV1, NormalBusinessReadWorkV1,
        NormalTerminalMetadataReadWorkV1, OriginalProjectionReadMetadataV1,
        PreparedNormalBusinessMetadataGenesisV1,
    },
    incremental_normal_event_commitment::NormalEventCommitmentBuilderV1,
    incremental_normal_process_source::NormalProcessSourceV1,
    incremental_normal_projection_commitment::AcceptedNormalProjectionCacheV1,
    incremental_normal_sources::BoundNormalTickParentV1,
};
use oasis7_formal_protocol::{
    controlled_authority::formal::bounded_context::FormalBoundedContextV1,
    incremental_execution::{ExecutionError, ExecutionFence},
    incremental_material::OriginalObjectStreamReader,
    incremental_projection_sidecar::PaidVerifiedProjectionSidecarV1,
    incremental_state::{MerkleNodeReader, StatePageWriter},
    incremental_value::{ValueNodeReader, ValueNodeWriter, ValueTreeError},
};
use oasis7_proto::memory_budget::ReservedValue;
use oasis7_wasm_abi::CanonicalCborWorkV1;
use oasis7_world_state::{
    incremental_cognition_proof_ledger::{
        AcceptedCognitionProofLedgerV1, begin_cognition_proof_ledger_current_v1,
    },
    incremental_field_read::WorldFieldReadContextV1,
    incremental_runtime_history_manifest::RuntimeHistoryLaneV1,
    incremental_runtime_revocation::RuntimeRevocationCurrentV1,
};
impl<M, R, E, C, W, F, O> RootModuleExecutionContextV1<'_, '_, '_, '_, M, R, E, C, W, F, O>
where
    M: MerkleNodeReader,
    R: ValueNodeReader,
    E: FnMut() -> Option<ValueTreeError>,
    C: FnMut() -> bool,
    W: ValueNodeReader + ValueNodeWriter + StatePageWriter,
    F: FnMut(
        &mut dyn crate::world::incremental_body_event::AgentEventOriginalV1,
    ) -> Result<(), BodyMutationError>,
    O: OriginalObjectStreamReader
        + PaidProjectionRetainedOriginalReaderV1<RetainedError = BodyMutationError>,
{
    #[allow(clippy::too_many_arguments)]
    pub(in crate::world) fn prepare_accepted_genesis_projection_cache_v1<
        P: MerkleNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    >(
        &mut self,
        projected: RuntimeProjectedGenesisV1,
        side: PaidVerifiedProjectionSidecarV1,
        parent: &BoundNormalTickParentV1<'_>,
        business_parent: &ReservedValue<NormalBusinessMetadataParentSourceV1>,
        revocation: &RuntimeRevocationCurrentV1,
        accepted_ledger: Option<&ReservedValue<AcceptedCognitionProofLedgerV1>>,
        policy: &PaidCurrentBootstrapPolicyV1,
        process: &NormalProcessSourceV1,
        authorization:&crate::world::incremental_authorization_root_current::CurrentAuthorizationProjectionV1<'_>,
        read: &mut WorldFieldReadContextV1<'_, P, R, PE, PC>,
        business_work: &mut NormalBusinessReadWorkV1,
        metadata_work: &mut NormalTerminalMetadataReadWorkV1,
        finality_work:&mut crate::world::incremental_normal_bootstrap_finality::CurrentBootstrapFinalityWorkV1,
        canonical: &mut CanonicalCborWorkV1,
        common: &mut FormalBoundedContextV1<'_>,
        live: &mut dyn FnMut() -> ExecutionFence,
    ) -> Result<
        (
            ReservedValue<AcceptedNormalProjectionCacheV1>,
            RuntimeGenesisSummaryV1,
            PreparedNormalBusinessMetadataGenesisV1,
        ),
        RootModuleExecutionErrorV1<O::Error>,
    > {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parent
                .validate_context(common)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            self.ctx
                .transaction
                .validate_bootstrap_read_only_v1(&live())?;
            let (source, summary, metadata) = projected
                .bind_accepted_genesis_projection_source_v1(
                    parent,
                    self.ctx.transaction,
                    side,
                    common,
                    &live(),
                )?;
            metadata.check_actual_parent(business_parent, common)?;
            let ledger = accepted_ledger
                .map(|ledger| {
                    begin_cognition_proof_ledger_current_v1(
                        ledger,
                        parent.refs().head,
                        self.ctx.transaction,
                        common,
                        &live(),
                    )
                })
                .transpose()
                .map_err(|error| BodyMutationError::from(error))?;
            // The pristine transaction proves that the appended suffix is
            // actually empty. Existing retained parent events remain in the
            // genuine parent history index and are read by metadata capture.
            let events =
                NormalEventCommitmentBuilderV1::new(0, self.ctx.transaction.guard(), common)?
                    .finish(common)?;
            let manifest = with_current_root_module_meter_v1(self, common, live, |root, _| {
                crate::world::incremental_module_execution_manifest_hash::prepare_current_world_manifest_hash_v1(
                    root.families.parent,root.families.head,root.families.roots,
                    root.manifest_cache,None,root.families.config,root.state,root.ctx,root.manifest_work,
                ).map_err(RootModuleExecutionErrorV1::Manifest)
            })?;
            let governance = self.capture_normal_terminal_metadata_v1(
                parent,
                business_parent,
                revocation,
                parent
                    .refs()
                    .runtime
                    .history_index(RuntimeHistoryLaneV1::Journal),
                &events,
                &manifest,
                ledger.as_ref(),
                read,
                business_work,
                metadata_work,
                common,
                live,
            )?;
            self.ctx
                .transaction
                .validate_bootstrap_read_only_v1(&live())?;
            let frozen_authorization = authorization
                .capture_normal_resident_v1(
                    parent.refs().storage.current().runtime(),
                    parent.refs().roots,
                    self.ctx,
                    read,
                    &mut business_work.authorization,
                    &mut metadata_work.pending_entries,
                    common,
                    &live(),
                )
                .map_err(|error| {
                    BodyMutationError::Original(Box::new(AuthorizationCaptureCause(error)))
                })?;
            let finality =
                self.prepare_current_bootstrap_finality_v1(parent, finality_work, common, live)?;
            let selected = OriginalProjectionReadMetadataV1::Genesis {
                source: &metadata,
                governance: &governance,
            };
            let cache = source.into_accepted_projection_cache_v1(
                parent,
                &selected,
                policy,
                process,
                &frozen_authorization,
                &finality,
                self.ctx.transaction,
                &mut *self.originals,
                common,
                canonical,
                live,
            )?;
            self.ctx
                .transaction
                .validate_bootstrap_read_only_v1(&live())?;
            Ok((cache, summary, metadata))
        }));
        match attempt {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(error)) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                Err(error)
            }
            Err(panic) => {
                let _: Result<(), ExecutionError> = self
                    .ctx
                    .transaction
                    .contribute(|_| Err(ExecutionError::Aborted));
                std::panic::resume_unwind(panic)
            }
        }
    }
}

#[derive(Debug)]
struct AuthorizationCaptureCause(crate::world::incremental_authorization_root_current::SourceError);
impl std::fmt::Display for AuthorizationCaptureCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for AuthorizationCaptureCause {}

#[path="incremental_normal_projection_bootstrap_capture_cold.rs"]mod cold_capture;
