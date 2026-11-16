//! Actual cold bootstrap uses the same Root/physical context and original
//! terminal metadata capture as ordinary execution. The complete opaque seed
//! is bound to the accepted parent before any private cache can be minted.
use super::*;
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
    pub(in crate::world) fn prepare_accepted_cold_projection_cache_v1<
        P: MerkleNodeReader,
        PE: FnMut() -> Option<ValueTreeError>,
        PC: FnMut() -> bool,
    >(
        &mut self,
        projection_work: &mut crate::world::incremental_normal_projection_commitment::NormalProjectionCurrentWorkV1<'_>,
        side: PaidVerifiedProjectionSidecarV1,
        parent: &BoundNormalTickParentV1<'_>,
        business_parent: &ReservedValue<NormalBusinessMetadataParentSourceV1>,
        revocation: &RuntimeRevocationCurrentV1,
        accepted_ledger: Option<&ReservedValue<AcceptedCognitionProofLedgerV1>>,
        accepted_retention: &ReservedValue<oasis7_world_state::incremental_cognition_retention_ledger::AcceptedCognitionRetentionLedgerV1>,
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
    ) -> Result<ReservedValue<AcceptedNormalProjectionCacheV1>, RootModuleExecutionErrorV1<O::Error>>
    {
        let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            parent
                .validate_context(common)
                .map_err(|e| BodyMutationError::Original(Box::new(e)))?;
            self.ctx
                .transaction
                .validate_bootstrap_read_only_v1(&live())?;
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
            let columns = self.capture_qualified_cold_projection_seed_v1(
                parent,
                &side,
                revocation,
                accepted_ledger,
                accepted_retention,
                policy,
                &frozen_authorization,
                &governance,
                read,
                projection_work,
                &mut metadata_work.originals,
                common,
                live,
            )?;
            let source=crate::world::incremental_normal_projection_bootstrap::bind_qualified_cold_projection_source_v1(
                columns,parent,self.ctx.transaction,side,common,&live(),
            )?;
            let finality =
                self.prepare_current_bootstrap_finality_v1(parent, finality_work, common, live)?;
            let selected = OriginalProjectionReadMetadataV1::Current(&governance);
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
            Ok(cache)
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
